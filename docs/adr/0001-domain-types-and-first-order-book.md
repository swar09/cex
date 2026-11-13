# 0001: Domain types and the first order book

- **Commits:** `8a522f7` → `09d9048`
- **Status:** replaced by [0002](0002-linked-list-to-slab.md)

## Where it started

It all started with the core domain. I added the enums `OrderType` and `Side`, and type aliases over the primitive types: `Price`, `Quantity` and `OrderId`. Then I started on the order book itself, with the `OrderBook` struct, the other basic structs, and their methods.

## The first design

The order book is just asks and bids, and both are B-trees (`BTreeMap`). Each one holds key-value pairs of price → price level.

Within a price level I maintained a FIFO queue. The FIFO queue was a linked list, and the linked list held pointers to the actual order objects:

```rust
type OrderPointer  = Rc<RefCell<Order>>;
type OrderPointers = LinkedList<OrderPointer>;   // just an alias, nothing great

struct OrderBook {
    bids:   BTreeMap<Reverse<Price>, OrderPointers>, // highest price first
    asks:   BTreeMap<Price, OrderPointers>,          // lowest price first
    orders: HashMap<OrderId, OrderEntry>,            // O(1) access by order id
}
```

I also maintained a hash map for O(1) order access by order id. From the order id I get the order pointer, and from the order pointer I get the order object.

In the following commits I added the basic methods like `add_order` and `match_orders`, and the unit tests.

## What I knew was wrong with it

Soon I realized that this is multiple pointer hops: B-tree → linked list node → `Rc` → order. That is pointer chasing, and there are too many allocations, so we lose performance.

I realized this while writing it. But at this point my goal was not to chase performance. My goal was to just make it work. So I kept going and kept adding more order types, and the methods to support them, on top of this structure.

## Commit map

| Commit | What |
|---|---|
| `8a522f7` | Monorepo skeleton |
| `7c9ba10` | Enums, type aliases, `OrderBook` with `BTreeMap` + `LinkedList` + `HashMap` |
| `3a168b7`, `9459023` | Matching logic, fill checks, trade creation |
| `09d9048` | `Price` changed from `i32` to `i64` |
