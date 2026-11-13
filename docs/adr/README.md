# Design journal

How this exchange was built, one decision at a time, in the order it happened. Each record says what I was trying to do, what I did, what was wrong with it, and which commits it covers. The mistakes are left in on purpose.

| # | Decision | Commits |
|---|---|---|
| [0001](0001-domain-types-and-first-order-book.md) | Domain types and the first order book (`BTreeMap` + linked list) | `8a522f7` → `09d9048` |
| [0002](0002-linked-list-to-slab.md) | From linked list to slab | `acdaa99` → `801a512` |
| [0003](0003-single-threaded-order-book.md) | Keep the order book single-threaded | `1497bb1` → `434343e` |
| [0004](0004-channels-and-the-exchange.md) | Channels between I/O and the engine, and the `Exchange` struct | `1a247a9` → `9b2d193` |
| 0005 | From channels to a ring buffer (not written yet) | `878c0b8` → `12651e9` |
| [0006](0006-risk-engine-inside-the-matching-engine.md) | The risk engine lives inside the matching engine | `a608ccc` → `c16b21d` |
| [0007](0007-workers-around-the-matching-engine.md) | Workers around the matching engine | `53050b8`, `98f6dd5`, `8b0d8e3` → `04b1d11` |
