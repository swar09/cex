# 0002: From linked list to slab

- **Commits:** `acdaa99` → `801a512`
- **Status:** replaced later (the `VecDeque` and the `Rc` pointers were removed in `f982a0e`)

## The problem

The linked list from [0001](0001-domain-types-and-first-order-book.md) was scattered across the heap. There were too many cache misses and too much pointer chasing, plus an allocation for every order.

## What I did

I removed the linked list and added a slab.

A slab is a contiguous data structure. Underneath it is like a vector, but a vector that reuses its empty slots. So the orders at one price level sit next to each other in memory and are not spread over the heap.

Next to the slab I maintained a `VecDeque`. This was the FIFO queue, and it stored the slab indices of the orders in FIFO order:

```rust
type OrderPointer = Rc<RefCell<Order>>;

struct PriceLevel {
    orders: Slab<OrderPointer>,
    queue:  VecDeque<usize>,
}
```

To reach an order I would read the index from the queue, access that index in the slab, and then follow the pointer to the order.

## What was still wrong

It is not the best. Inside the slab I was still storing a pointer to the order, not the order itself, so the pointer chasing was still there. That is still this many hops: queue → slab slot → `Rc` → order.

I was aware of these mistakes. But it was better than the linked list implementation, and there were fewer cache misses.

## What I learned

Again I was trying to optimize for cache locality, and it was a mistake. I should not optimize this early. The book was not even wired to anything yet, and I had no benchmark to tell me whether this mattered.

## Commit map

| Commit | What |
|---|---|
| `acdaa99` | `slab` added, `PriceLevel` with `Slab` + `VecDeque` |
| `c634c01` | Order modification with tests |
| `c4af288` | Market order support |
| `801a512` | Single `main.rs` split into `orderbook.rs`, `level.rs`, `orders.rs`, `types.rs` |
