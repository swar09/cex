# 0003: Keep the order book single-threaded

- **Commits:** `1497bb1` → `434343e`
- **Status:** accepted. The `Arc<Mutex>` part was replaced in [0004](0004-channels-and-the-exchange.md)

## What I wanted to do

I thought I would make the order book multi-threaded. The plan was the standard `Arc<Mutex<..>>` way: a reference count so I can share ownership across multiple threads, and a mutex inside it so only one thread can write at a time.

## Why I backed off

Before implementing it I understood the cost. If I add locks, I may increase my overall throughput, but the latency will be much higher. A mutex that is contended, where a thread has to be parked and woken up again, can cost thousands of nanoseconds. That is very expensive when the matching itself takes a few hundred.

So I immediately backed off from that approach and decided to keep the order book single-threaded.

There is a way to go multi-threaded, with concurrent data structures or lock-free programming. I explored that side too. But I decided to keep it simple and stay single-threaded.

## The one place I still used a lock

There is an order type called Good For Day. These orders must be removed at the end of the day, when the market closes. For that there is a background thread which, at a particular time, takes a lock on the order book and removes all the Good For Day orders.

Here I used the plain implementation and wrapped the order book in `Arc<Mutex>`. This was just an approach to make it work.

## What real exchanges do

At this point the order book worked with a few order types, but it was just a single-threaded program. I could only add orders by hard-coding them in `main.rs`. It was nothing more than that.

So I started researching how real exchanges work. I found that most of the standard exchanges use a single-threaded order book per symbol: one order book per thread, and that order book process is completely single-threaded. They avoid locks in the first place, because they cannot afford them at all.

That confirmed the decision. It also raised the next question: if the process is single-threaded, how does it get its data?

## Commit map

| Commit | What |
|---|---|
| `1497bb1` | `OrderBook` moved into its own `engine` crate |
| `e46a920` | Good For Day pruning with `Arc<Mutex<OrderBook>>` |
| `434343e` | Locking simplified, prune function cleaned up |
