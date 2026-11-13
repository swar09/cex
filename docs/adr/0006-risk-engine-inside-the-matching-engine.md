# 0006: The risk engine lives inside the matching engine

- **Commits:** `a608ccc` → `c16b21d`, then `f982a0e`, `4fed9e0`, `d178171`
- **Status:** accepted

A note on naming: until now I called it "the order book plus its methods and the `Exchange` struct". From here on I call that whole thing the **matching engine**.

## The problem

Trades were happening, but there was one issue: there was no risk check. Before an order is added we have to verify that the user has the appropriate balance, or the appropriate assets.

## Where should risk checks live?

I researched this. The options were:

1. Outside the order book, on the gateway.
2. In a completely different service.
3. Inside the matching engine process itself.

Two things pull against each other here.

The matching engine is sensitive. If we add bloat to it, we lose throughput and we lose performance.

But risk checks are a very sensitive thing too. Wrong risk check logic is a financial bug, and we can lose money. If the risk checks become even slightly wrong, the whole purpose is defeated. What is the use of a fast exchange and a good system if the balances in it cannot be trusted?

## Why outside does not work

After my research, and after looking at other people's implementations, I found that the risk check must live within the matching engine process itself.

The risk check has to happen per order. Suppose it lives outside the matching engine:

- A user has some orders already resting in the book.
- He sends a new order. The outside check looks at his balance, sees enough, and lets it through.
- Meanwhile his resting orders get executed and his balance becomes zero.
- The new order is now illegal. But at the time we verified it, the balance was sufficient.

The check and the match are in two different places, so the check is always looking at old state. I knew that if I kept the risk engine outside, I would end up with exactly these bugs.

## What I did

I added a new layer, the risk engine, inside the matching engine. Now when `Exchange` handles an incoming command from the buffer, it first does the risk check, and only then does the order go into the order book.

The risk engine does not only check. It **reserves**.

- When an order comes in, the amount it needs is moved out of the user's available balance into a reserve. For a buy that is money. For a sell that is the asset.
- So if a user who holds 10 units tries to place a hundred sell orders of 10 units each, it is not possible. The first order reserves the 10 units, and every order after that fails the check.
- If the order does not match and gets cancelled, the reserve is released and the funds go back to the user's available balance.
- If the order matches, the trade event is emitted to the outgoing buffer, and at the same moment the risk engine is called to settle: the reserve is consumed on one side and the other side is credited.

So even when in-flight orders match and the account state changes underneath, a new order that has become illegal is not allowed in.

Every order becomes risk-safe. All orders are legal. We are not selling an asset from a user who does not have that asset. Before an order enters we do all the checks, and after it is done we release or consume the reserve.

## Low-level design

### Data

```rust
pub struct RiskEngine {
    id_map:   FxHashMap<ExternalUserId, InternalUserId>,
    accounts: Vec<Account>,          // indexed by internal user id
    exchange_account: Account,
}

pub struct Account {
    user_internal_id:  InternalUserId,
    available_balance: Balance,      // u64, money the user can still spend
    reserved:          Balance,      // money locked by open buy orders
    status:            AccountStatus,
    holdings:          Holdings,
    account_type:      AccountType,
}

pub struct Holdings {
    available: [Quantity; MAX_ASSETS],   // MAX_ASSETS = 32, indexed by asset
    reserved:  [Quantity; MAX_ASSETS],   // locked by open sell orders
}
```

- A user can hold money and can also hold various assets. Money is one `available`/`reserved` pair on the account. Assets are the same pair per asset, in fixed arrays.
- The outside world uses an external user id. The hash map turns it into an internal id once, and after that an account is a plain index into a `Vec`.
- All amounts are integers. `price * quantity` uses `checked_mul`, so an overflow rejects the order.

### Where it sits in the flow

```
command from buffer
      │
      ▼
Exchange::handle_cmd
      │
      ├─ risk.check_and_reserve(user, order) ── fails ──► OrderRejected(InsufficientBalance)
      │
      ▼
OrderBook::add_order
      │
      ├─ book refuses the order ──► risk.release(...)  ──► OrderRejected
      ├─ order rests            ──► reserve stays locked ► OrderPlaced
      └─ order matches          ──► risk.settle(buyer), risk.settle(seller) per trade
                                                         ► OrderPlaced, TradeExecuted
cancel command ──► OrderBook::cancel_order ──► risk.release(remaining) ──► OrderCancelled
```

### What each operation moves

| Operation | When | Buy side | Sell side |
|---|---|---|---|
| `reserve` | Before the order enters the book | `available_balance -= price × qty`, `reserved += price × qty` | `holdings.available[asset] -= qty`, `holdings.reserved[asset] += qty` |
| `release` | Order rejected by the book, or cancelled | `reserved → available_balance` | `holdings.reserved[asset] → holdings.available[asset]` |
| `settle` | Once per trade, for each side | `reserved -= price × qty`, `holdings[asset] += qty` | `holdings.reserved[asset] -= qty`, `available_balance += price × qty` |

### Account status

Every operation first asks the account status whether it is allowed:

| Status | Allowed |
|---|---|
| `Active` | Everything |
| `Frozen` | Release, settle, credit. Open orders can still be cancelled and matched trades still settle |
| `ReduceOnly` | Release, settle, credit. No withdrawals |
| `Closed` | Withdraw only |

## Keeping it cheap

The risk engine now runs on the hot path for every order, so I inlined the functions that I expected to be called again and again there.

## Commit map

| Commit | What |
|---|---|
| `a608ccc`, `4fc1051` | `risk` crate started: accounts and wallets, then renamed to `risk_engine` |
| `ed10fcd` → `71591cf` | Reserve, release, settle and withdraw logic, `Holdings`, unit tests |
| `534854b` | Risk engine wired into `Exchange::handle_cmd`: check and reserve before the book, release and settle after |
| `c16b21d` | User validation and data structure cleanup |
| `f982a0e` | Account operations inlined |
| `4fed9e0` | Reservation for market orders with a slippage cap |
| `d178171` | `#[inline(always)]` on the hot-path dispatch |
