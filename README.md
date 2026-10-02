[benchmark results](benchmarks/bench.md)

this is an implementation of system design concepts and an attempt to build a low-latency system. all performance-critical parts are written in rust, and the other services are in typescript and node.js.

the order book is single-threaded as of now. the exchange struct runs on a single thread with core affinity and has `orderbook: fxhashmap<symbol, orderbook>`, so symbols do not have separate threads and all order books run on this single thread. i plan to improve this.

i am using a circular ring buffer for exchange command ingestion, which ingests all commands from the gateway service and writes them to the circular ring buffer. the exchange implementation handles this ingestion, calls the order books to execute add and cancel order operations, and returns trades here. i have huge room for optimization and know the limitations, but the first goal is to make it right and finish the other services first.