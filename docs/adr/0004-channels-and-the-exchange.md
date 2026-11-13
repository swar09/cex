# 0004: Channels between I/O and the engine, and the `Exchange` struct

- **Commits:** `1a247a9` → `9b2d193`
- **Status:** the channel was replaced by a ring buffer later (`878c0b8`). The `Exchange` and command design is still in use

## The question

If the order book is single-threaded, how will I get the data? Orders come from the internet, so there is I/O, and I have to wait for the data. But the order book cannot wait.

## Going down the rabbit hole

I started reading about inter-process communication. My first idea: spawn a completely separate process on an async runtime, use axum to accept the orders, somehow share memory between that process and my single-threaded order book process, and let the order book read from that memory.

Then I got to know about the `crossbeam` crate and its channels. A channel is just memory on the heap. The async runtime writes the data into it, and the single-threaded engine reads from it and does all of its work: the order matching and whatever else the message instructs it to do. This works between threads inside one process, so I did not need a second process or shared memory at all.

So I/O is not a bottleneck for the engine any more.

## The `Exchange` struct

After a few refactors I implemented the `Exchange` struct.

I know this is not a good approach, but for learning purposes I kept multiple order books on a single thread, all owned by `Exchange`. I did this to keep things simple.

The structure became:

- `Exchange` owns the order books, one per symbol.
- The `Exchange` methods only read a message from the buffer, take action according to that message, and call the order book methods downstream.
- The `OrderBook` methods do the actual work of handling and matching the order.
- The results are emitted as events and written to another buffer, for the consumers on the other side.

## Messages as enums

The async runtime writes small messages into the buffer and `Exchange` parses them. I used enums that carry their data. If the variant is `AddNewOrder`, it carries the data of that particular order:

```rust
pub enum ExchangeCommand {
    AddNewOrder(Symbol, NewOrder),
    CancelOrder(Symbol, OrderId),
    ModifyOrder(Symbol, ModifyOrder),
    PruneExpiredOrders(Symbol, OrderType),
}
```

Once commands existed, the Good For Day pruner from [0003](0003-single-threaded-order-book.md) no longer needed a lock. It just sends a `PruneExpiredOrders` command like everyone else, and the `Arc<Mutex>` went away.

## Message size matters

I tried to keep the messages as small as possible. For experimenting, I also created some messages that were large in size.

The impact: if the message is large, say I add unnecessary metadata, it increases the latency and decreases the throughput drastically. And we do not need that much data in a single message anyway. Real matching engines work with small messages and acknowledgements. They never process large, complete messages. I only did it for simplicity and for learning.

## Commit map

| Commit | What |
|---|---|
| `1a247a9` | `crossbeam` added, `ExchangeCommand`, event enum, `Symbol`, `Exchange` struct |
| `a26e198` → `c736e8a` | `Exchange::handle_cmd` filled in, events carry order details and symbol |
| `9b2d193` | `PruneExpiredOrders` command, pruner sends a command and takes no lock |
