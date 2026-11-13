# centralized exchange

A low-latency matching engine written in Rust. A single-threaded order book receives commands through a lock-free disruptor ring buffer, so the matching path takes no locks and performs no I/O. An in-process risk engine reserves balance or assets before an order enters the book. Every command is persisted to a write-ahead log before it is matched. Results are emitted as events on a second ring buffer, where dedicated workers deliver them to WebSocket clients, Kafka and a Redis read replica.

Benchmarks: up to 3.9M ops with p50 @ 190 nanoseconds on the matching core. See the [benchmark](benchmarks/bench.md).

The matching engine is part of a larger centralized exchange that is still in progress. The low-latency parts are in Rust and the other services will be in TypeScript.

## What is implemented

### Matching engine (`rust/matching-engine`)

| Part | Description | Status |
|---|---|---|
| Order book and matching | Matches buy and sell orders by price and time | Done |
| Command and event pipeline | Ring buffers in and out of the single-threaded engine | Done |
| Market data over WebSocket | Sends every order book event to connected clients | Done |
| Event stream to Kafka | Publishes events for other services | Done |
| Good For Day expiry | Cancels day orders at the end of the day | Done |
| Risk engine | Checks and reserves balance before an order enters the book | Partial |
| Write-ahead log | Writes every command to disk before it is matched | Partial |
| Read replica in Redis | Copy of orders and balances for read queries | Partial |
| HTTP API | Create, get, cancel and modify orders | Partial |
| CPU core pinning | Pins each worker thread to its own core | Partial |
| Snapshots | Saves the order book state to speed up recovery | Not started |
| Crash recovery | Rebuilds the order book after a restart | Not started |

### API gateway (`rust/api-gateway`)

| Part | Description | Status |
|---|---|---|
| JWT verification | Verifies tokens and blocks revoked ones | Done |
| Identity headers | Passes user id and role to the services behind it | Done |
| Request hygiene | Header cleanup, size limit, timeout, request id, CORS | Done |
| Rate limiting | Limits requests per user and role | Done |
| Load balancing | Spreads requests across service instances | Done |
| Reverse proxy and routing | Forwards requests to the right service | Done |
| Tests | Unit, contract, integration and end-to-end | Done |

### Services and workers

| Service | Description | Status |
|---|---|---|
| API contracts | OpenAPI specs and generated types | Done |
| Auth service | Sign up, login and token issuing | Not started |
| User service | User profiles | Not started |
| Order service | Order entry and history | Not started |
| Ledger service | Balances and transaction records | Not started |
| Mock KYC service | Fake identity verification | Not started |
| Settlement worker | Moves funds after a trade | Not started |
| Notification worker | Tells users about their orders and trades | Not started |
| Ledger reconciliation worker | Checks the ledger against the engine | Not started |
| Shared packages | Common auth, error and type code | Not started |
| Web client | Trading UI | Not started |

### Infrastructure

| Part | Description | Status |
|---|---|---|
| Local Redis and Kafka | Started with `docker compose up -d` | Done |
| Postgres | Database for the services | Not started |
| Deployment | Running outside a local machine | Not started |

## How I got here

I am doing this out of pure curiosity. 

The [design journal](docs/adr/README.md) walks through the decisions in the order I made them, with the commits for each one and the mistakes left in.

The [workers README](rust/matching-engine/workers/README.md) explains the write-ahead log, the Kafka and Redis consumers, and why each one exists.

## Running it

```bash
docker compose up -d            # Redis, Kafka, rate-limiter Redis
cargo run -p server --release   # matching engine
./scripts/test.sh               # all test suites
./scripts/check.sh              # typos, formatting, clippy
```
