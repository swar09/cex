use std::collections::HashMap;

use domain::{AssetId, Order, OrderId, OrderType, Price, Quantity, Side};

use crate::risk_engine::{
    AccountOpp::{Credit, Release, Settle, Withdraw},
    AccountStatus::{Active, Closed, Frozen, ReduceOnly},
};

pub type InternalUserId = usize;
pub type ExternalUserId = usize;
pub type Balance = u64;
pub enum AccountOpp {
    Release,
    Reserve,
    Settle,
    Withdraw,
    Deposit,
    Credit,
}
#[derive(Debug, Default, PartialEq)]
pub enum AccountStatus {
    #[default]
    Active,
    Frozen,
    Closed,
    ReduceOnly,
}
impl AccountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Frozen => "frozen",
            Self::Closed => "closed",
            Self::ReduceOnly => "reduce-only",
        }
    }
    pub fn allows(&self, op: AccountOpp) -> bool {
        matches!(
            (self, op),
            // active accounts can do every normal action like trading, depositing, and withdrawing
            (Active, _)
            // closed accounts are shut down so users can only take out any leftover money
            | (Closed, Withdraw)
            // frozen accounts are locked for safety, but they still need to cancel open orders or settle trades that already matched
            | (Frozen, Release | Settle | Credit)
            // reduce only accounts are in trouble with risk so they cannot take money out, but they can settle trades and cancel orders
            | (ReduceOnly, Settle | Release | Credit)
        )
    }
}

#[derive(Debug, Default)]
pub struct Holdings {
    pub reserved: HashMap<AssetId, Quantity>,
    pub available: HashMap<AssetId, Quantity>,
}
impl Holdings {
    pub fn new(asset_id: AssetId, quantity: Quantity) -> Self {
        let reserved = HashMap::new();
        let mut available = HashMap::new();
        available.insert(asset_id, quantity);
        Self { reserved, available }
    }
    pub fn get_available_quantity(&self, asset_id: AssetId) -> Option<Quantity> {
        self.available.get(&asset_id).copied()
    }
    pub fn get_reserved_quantity(&self, asset_id: AssetId) -> Option<Quantity> {
        self.reserved.get(&asset_id).copied()
    }
    pub fn reserve_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
        if quantity == 0 {
            return false;
        }
        match self.available.get_mut(&asset_id) {
            Some(available_quantity) => {
                if quantity > *available_quantity {
                    return false;
                }
                // we are reserving the asset first time it may or may not be avialabe in
                // reserve, so insert if missing
                let reserved_quantity = self.reserved.entry(asset_id).or_insert(0);
                *available_quantity -= quantity;
                *reserved_quantity += quantity;
                true
            },
            None => false,
        }
    }
    pub fn can_reserve_asset(&self, asset_id: AssetId, quantity: Quantity) -> bool {
        if quantity == 0 {
            return false;
        }
        match self.available.get(&asset_id) {
            Some(available_quantity) => {
                if quantity > *available_quantity {
                    return false;
                }
                true
            },
            None => false,
        }
    }

    pub fn release_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
        let Some(reserved_asset_quantity) = self.reserved.get_mut(&asset_id) else {
            return false;
        };

        if quantity > *reserved_asset_quantity {
            return false;
        }

        let available_asset_quantity = self.available.entry(asset_id).or_insert(0);

        *reserved_asset_quantity -= quantity;
        *available_asset_quantity += quantity;

        true
    }
    pub fn consume_reserved_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
        let Some(reserved_asset_quantity) = self.reserved.get_mut(&asset_id) else {
            return false;
        };

        if quantity > *reserved_asset_quantity {
            return false;
        }
        *reserved_asset_quantity -= quantity;

        true
    }
    pub fn can_release_asset(&self, asset_id: AssetId, quantity: Quantity) -> bool {
        let Some(reserved_asset_quantity) = self.reserved.get(&asset_id) else {
            return false;
        };

        if quantity > *reserved_asset_quantity {
            return false;
        }

        true
    }

    pub fn get_all_assets(&self) -> Vec<AssetId> {
        let mut assets: Vec<AssetId> = self.available.keys().chain(self.reserved.keys()).copied().collect();
        assets.sort_unstable();
        assets.dedup();
        assets
    }
    pub fn credit_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
        let current_asset_quantity = self.available.entry(asset_id).or_insert(0);
        *current_asset_quantity += quantity;
        true
    }
}

#[derive(Debug, Default)]
pub struct Account {
    pub user_internal_id: InternalUserId,
    pub available_balance: Balance,
    pub reserved: Balance,
    pub status: AccountStatus,
    pub holdings: Holdings,
}

impl Account {
    pub fn new_with_balance_and_holdings(int_id: InternalUserId, balance: Balance, holdings: Holdings) -> Self {
        Self {
            user_internal_id: int_id,
            available_balance: balance,
            reserved: 0,
            status: AccountStatus::Active,
            holdings,
        }
    }
    pub fn get_available_balance(&self) -> Option<Balance> {
        Some(self.available_balance)
    }
    pub fn get_reserved_balance(&self) -> Option<Balance> {
        Some(self.reserved)
    }

    pub fn reserve(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Reserve) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        if self.available_balance < amount {
            false
        } else {
            self.available_balance -= amount;
            self.reserved += amount;
            true
        }
    }
    pub fn can_reserve(&self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Reserve) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        self.available_balance >= amount
    }
    pub fn release_reserve(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Release) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        if self.reserved < amount {
            false
        } else {
            self.reserved -= amount;
            self.available_balance += amount;
            true
        }
    }
    pub fn can_release_reserve(&self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Release) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        self.reserved >= amount
    }
    pub fn withdraw(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Withdraw) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        if amount > self.available_balance {
            return false;
        }
        self.available_balance -= amount;
        true
    }
    pub fn consume_reserve(&mut self, amount: Balance) -> bool {
        if amount > self.reserved {
            return false;
        }
        self.reserved -= amount;
        true
    }
    pub fn credit_amount(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Credit) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        let Some(new_balance) = self.available_balance.checked_add(amount) else {
            return false;
        };
        self.available_balance = new_balance;
        true
    }

    pub fn can_withdraw(&self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Withdraw) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        if amount > self.available_balance {
            return false;
        }
        true
    }
    pub fn deposit(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Deposit) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        let Some(new_balance) = self.available_balance.checked_add(amount) else {
            return false;
        };
        self.available_balance = new_balance;
        true
    }
    pub fn can_deposit(&self) -> bool {
        if !self.status.allows(AccountOpp::Deposit) {
            return false;
        }
        true
    }

    pub fn can_settle(&self, price: Price, asset_id: AssetId, quantity: Quantity, side: Side) -> bool {
        if !self.status.allows(AccountOpp::Settle) {
            return false;
        }
        if quantity == 0 || price == 0 {
            return false;
        }
        let Some(amount) = price.checked_mul(quantity as u64) else {
            return false;
        };

        match side {
            Side::Buy => self.reserved >= amount,
            Side::Sell => self.holdings.can_release_asset(asset_id, quantity),
        }
    }

    pub fn settle(&mut self, price: Price, asset_id: AssetId, quantity: Quantity, side: Side) -> bool {
        if !self.status.allows(AccountOpp::Settle) {
            return false;
        }
        if quantity == 0 || price == 0 {
            return false;
        }
        let Some(amount) = price.checked_mul(quantity as u64) else {
            return false;
        };

        match side {
            Side::Buy => {
                if !self.consume_reserve(amount) {
                    return false;
                }
                self.holdings.credit_asset(asset_id, quantity)
            },
            Side::Sell => {
                if !self.holdings.consume_reserved_asset(asset_id, quantity) {
                    return false;
                }
                self.credit_amount(amount)
            },
        }
    }
}
#[derive(Debug, Default)]
pub struct RiskEngine {
    pub id_map: HashMap<ExternalUserId, InternalUserId>,
    // default currency is dollars and stored as cents
    pub accounts: Vec<Account>, // index by internal user ids
}

impl RiskEngine {
    pub fn new_empty() -> Self {
        Self {
            id_map: HashMap::new(),
            accounts: Vec::new(),
        }
    }

    // this constructor will be used in production
    pub fn new_with_accounts(
        ext_user_ids: Vec<ExternalUserId>,
        init_balances: Vec<Balance>,
        init_holdings: Vec<(AssetId, Quantity)>,
    ) -> Self {
        let capacity = ext_user_ids.len();
        let mut id_map = HashMap::with_capacity(capacity);
        let mut accounts = Vec::with_capacity(capacity);
        for (ext_id, index) in ext_user_ids.iter().enumerate() {
            id_map.insert(ext_id, *index);
            let holdings = Holdings::new(init_holdings[*index].0, init_holdings[*index].1);
            let account = Account::new_with_balance_and_holdings(*index, init_balances[*index], holdings);
            accounts.push(account);
        }

        Self { id_map, accounts }
    }

    // if order failed / rejected by orderbook release assets/amount for next orders
    pub fn release(
        &mut self,
        user_id: InternalUserId,
        _order_id: OrderId,
        asset_id: AssetId,
        side: Side,
        qutoe_price: Price,
        quote_quantity: Quantity,
    ) -> bool {
        match side {
            Side::Buy => {
                let account = &mut self.accounts[user_id];

                let Some(release_req_amount) = qutoe_price.checked_mul(quote_quantity as u64) else {
                    return false;
                };
                if release_req_amount > account.reserved {
                    return false;
                }
                account.reserved -= release_req_amount;
                account.available_balance += release_req_amount;
                true
            },
            Side::Sell => {
                let account = &mut self.accounts[user_id];
                let Some(current_reserved_asset_quantity) = account.holdings.reserved.get_mut(&asset_id) else {
                    return false;
                };

                if quote_quantity > *current_reserved_asset_quantity {
                    return false;
                }
                let Some(current_available_quantity) = account.holdings.available.get_mut(&asset_id) else {
                    return false;
                };

                *current_reserved_asset_quantity -= quote_quantity;
                *current_available_quantity += quote_quantity;
                true
            },
        }
    }
    // if order matched by orderbook settle reserved assets/amount for next orders
    pub fn settle(
        &mut self,
        user_id: InternalUserId,
        _order_id: OrderId,
        asset_id: AssetId,
        side: Side,
        quote_price: Price,
        quote_quantity: Quantity,
    ) -> bool {
        match side {
            Side::Buy => {
                let account = &mut self.accounts[user_id];

                let current_available_quantity = account.holdings.available.entry(asset_id).or_insert(0);
                let Some(debit_req_amount) = quote_price.checked_mul(quote_quantity as u64) else {
                    return false;
                };

                let reserved_amount = account.reserved;
                if !(reserved_amount >= debit_req_amount) {
                    // not possible i guess
                    return false;
                }

                *current_available_quantity += quote_quantity;
                account.reserved -= debit_req_amount;
                true
            },
            Side::Sell => {
                let account = &mut self.accounts[user_id];

                let Some(credit_req_amount) = quote_price.checked_mul(quote_quantity as u64) else {
                    return false;
                };
                let Some(account_reserved_assets) = account.holdings.reserved.get_mut(&asset_id) else {
                    return false;
                };
                if quote_quantity > *account_reserved_assets {
                    // not possible i guess
                    return false;
                }
                *account_reserved_assets -= quote_quantity;
                account.available_balance += credit_req_amount;
                true
            },
        }
    }
    // check order if valid , reserve the funds/asset and return.
    pub fn check_and_reserve(&mut self, user_id: ExternalUserId, order: Order) -> bool {
        self.check(user_id, order) && self.reserve(user_id, order)
    }
    // only check if order is valid or not
    pub fn check(&mut self, user_id: ExternalUserId, order: Order) -> bool {
        // temp solution to avoid errors ;
        let asset_id = 1;

        let order_type = order.order_type;
        if order_type == OrderType::Market {
            todo!();
            // handle market order here cause this order has no price
            // price = none
            // let price = orderbook.get_last_match(); // write some methods later while
            // working on ordebrook
            return true;
        }

        // order_type != Market
        // means normal limit orders where price != None

        let id = self.get_internal_id(user_id);
        let account = &self.accounts[id];
        match order.side {
            Side::Buy => {
                let Some(amount) = order.price.unwrap().checked_mul(order.initial_quantity as u64) else {
                    // amount is invalid
                    return false;
                };
                // return can reserve
                account.can_reserve(amount)
            },
            Side::Sell => account.holdings.can_reserve_asset(asset_id, order.initial_quantity),
        }
    }
    // only reserve, assumes that provided order is valid
    pub fn reserve(&mut self, user_id: ExternalUserId, order: Order) -> bool {
        let asset_id = 1;

        let order_type = order.order_type;
        if order_type == OrderType::Market {
            todo!();
            // handle market order here cause this order has no price
            // price = none
            // let price = orderbook.get_last_match(); // write some methods later while
            // working on ordebrook
            return true;
        }

        let id = self.get_internal_id(user_id);
        let account = &mut self.accounts[id];

        // order_type != Market
        // means normal limit orders where price != None

        match order.side {
            Side::Buy => {
                let Some(amount) = order.price.unwrap().checked_mul(order.initial_quantity as u64) else {
                    // amount is invalid
                    return false;
                };
                // return can reserve
                account.reserve(amount)
            },
            Side::Sell => account.holdings.reserve_asset(asset_id, order.initial_quantity),
        }
    }

    pub fn get_internal_id(&mut self, external_id: ExternalUserId) -> InternalUserId {
        // if internal_id not found then insert id which will be self.accounts.len()
        *self.id_map.entry(external_id).or_insert(self.accounts.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_account(available_balance: Balance, reserved: Balance) -> Account {
        let mut account = Account::new_with_balance_and_holdings(1, available_balance, Holdings::default());
        account.reserved = reserved;
        account
    }

    fn limit_order(order_id: OrderId, side: Side, price: Price, quantity: Quantity) -> Order {
        Order::new(order_id, side, price, quantity, OrderType::GoodTillCancel)
    }

    fn holdings_with_reserved(asset_id: AssetId, available: Quantity, reserved: Quantity) -> Holdings {
        let mut holdings = Holdings::new(asset_id, available);
        holdings.reserved.insert(asset_id, reserved);
        holdings
    }

    fn holdings_reserved(asset_id: AssetId, reserved: Quantity) -> Holdings {
        let mut holdings = Holdings::default();
        holdings.reserved.insert(asset_id, reserved);
        holdings
    }

    fn setup_engine(
        ext_user: ExternalUserId,
        available_balance: Balance,
        reserved: Balance,
        holdings: Holdings,
    ) -> (RiskEngine, InternalUserId) {
        let mut engine = RiskEngine::new_empty();
        let internal_id = engine.get_internal_id(ext_user);
        let mut account = Account::new_with_balance_and_holdings(internal_id, available_balance, holdings);
        account.reserved = reserved;
        engine.accounts.push(account);
        (engine, internal_id)
    }

    #[test]
    fn test_holdings_get_available_and_reserved_quantity() {
        // checking that we can read available and reserved asset amounts correctly
        let mut holdings = Holdings::default();
        let asset_btc = 1;
        let asset_eth = 2;

        assert_eq!(holdings.get_available_quantity(asset_btc), None);
        assert_eq!(holdings.get_reserved_quantity(asset_btc), None);

        holdings.available.insert(asset_btc, 50);
        holdings.reserved.insert(asset_btc, 20);

        assert_eq!(holdings.get_available_quantity(asset_btc), Some(50));
        assert_eq!(holdings.get_reserved_quantity(asset_btc), Some(20));
        assert_eq!(holdings.get_available_quantity(asset_eth), None);
        assert_eq!(holdings.get_reserved_quantity(asset_eth), None);
    }

    #[test]
    fn test_holdings_can_reserve_asset() {
        // we can reserve only if the user actually has enough available coins
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.can_reserve_asset(asset_id, 10));

        holdings.available.insert(asset_id, 50);
        assert!(!holdings.can_reserve_asset(asset_id, 0));
        assert!(holdings.can_reserve_asset(asset_id, 10));
        assert!(holdings.can_reserve_asset(asset_id, 50));
        assert!(!holdings.can_reserve_asset(asset_id, 51));
    }

    #[test]
    fn test_holdings_reserve_asset() {
        // reserving locks coins for an order and moves them from available to reserved
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.reserve_asset(asset_id, 10));

        holdings.available.insert(asset_id, 50);
        assert!(!holdings.reserve_asset(asset_id, 60));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(50));

        assert!(holdings.reserve_asset(asset_id, 20));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(30));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(20));

        assert!(holdings.reserve_asset(asset_id, 30));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(0));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(50));
    }

    #[test]
    fn test_holdings_can_release_asset() {
        // can release checks if the user has enough locked coins to unlock
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.can_release_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 25);
        assert!(!holdings.can_release_asset(asset_id, 26));
        assert!(holdings.can_release_asset(asset_id, 25));
        assert!(holdings.can_release_asset(asset_id, 10));
    }

    #[test]
    fn test_holdings_release_asset() {
        // releasing moves locked coins back to available when an order is cancelled
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.release_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 25);
        holdings.available.insert(asset_id, 10);
        assert!(!holdings.release_asset(asset_id, 30));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(25));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(10));

        assert!(holdings.release_asset(asset_id, 10));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(15));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(20));

        assert!(holdings.release_asset(asset_id, 15));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(0));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(35));
    }

    #[test]
    fn test_holdings_consume_reserved_asset() {
        // consuming removes locked coins completely when an order is filled and sold
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.consume_reserved_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 30);
        holdings.available.insert(asset_id, 10);

        assert!(!holdings.consume_reserved_asset(asset_id, 40));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(30));

        assert!(holdings.consume_reserved_asset(asset_id, 10));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(20));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(10));
    }

    #[test]
    fn test_holdings_credit_asset() {
        // crediting puts newly bought coins into available balance
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(holdings.credit_asset(asset_id, 25));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(25));

        assert!(holdings.credit_asset(asset_id, 15));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(40));
    }

    #[test]
    fn test_holdings_get_all_assets() {
        // returns a clean list of every unique coin the user holds
        let mut holdings = Holdings::default();
        let empty_assets = holdings.get_all_assets();
        assert!(empty_assets.is_empty());

        holdings.available.insert(1, 100);
        holdings.reserved.insert(2, 200);
        let assets = holdings.get_all_assets();
        assert_eq!(assets, vec![1, 2]);
    }

    #[test]
    fn test_account_get_balances() {
        // check reading cash balances
        let account = setup_account(1000, 250);

        assert_eq!(account.get_available_balance(), Some(1000));
        assert_eq!(account.get_reserved_balance(), Some(250));
    }

    #[test]
    fn test_account_can_reserve() {
        // users can only lock money if active and they have enough free cash
        let mut account = setup_account(500, 100);

        assert!(!account.can_reserve(0));
        assert!(account.can_reserve(500));
        assert!(account.can_reserve(200));
        assert!(!account.can_reserve(501));

        account.status = AccountStatus::Frozen;
        assert!(!account.can_reserve(100));

        account.status = AccountStatus::Closed;
        assert!(!account.can_reserve(100));

        account.status = AccountStatus::ReduceOnly;
        assert!(!account.can_reserve(100));
    }

    #[test]
    fn test_account_reserve() {
        // moves free cash into locked reserve for a buy order
        let mut account = setup_account(500, 100);

        assert!(!account.reserve(0));
        assert_eq!(account.available_balance, 500);
        assert_eq!(account.reserved, 100);

        assert!(!account.reserve(600));
        assert_eq!(account.available_balance, 500);
        assert_eq!(account.reserved, 100);

        account.status = AccountStatus::Frozen;
        assert!(!account.reserve(100));
        account.status = AccountStatus::Closed;
        assert!(!account.reserve(100));
        account.status = AccountStatus::ReduceOnly;
        assert!(!account.reserve(100));

        account.status = AccountStatus::Active;
        assert!(account.reserve(200));
        assert_eq!(account.available_balance, 300);
        assert_eq!(account.reserved, 300);
    }

    #[test]
    fn test_account_can_release_reserve() {
        // checks if locked cash can be unlocked back to free cash
        let mut account = setup_account(300, 200);

        assert!(!account.can_release_reserve(0));
        assert!(account.can_release_reserve(200));
        assert!(account.can_release_reserve(100));
        assert!(!account.can_release_reserve(201));

        account.status = AccountStatus::Frozen;
        assert!(account.can_release_reserve(100));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_release_reserve(100));

        account.status = AccountStatus::Closed;
        assert!(!account.can_release_reserve(100));
    }

    #[test]
    fn test_account_release_reserve() {
        // unlocks cash when a buy order is cancelled
        let mut account = setup_account(300, 200);

        assert!(!account.release_reserve(0));

        assert!(!account.release_reserve(250));

        account.status = AccountStatus::Closed;
        assert!(!account.release_reserve(50));

        account.status = AccountStatus::Frozen;
        assert!(account.release_reserve(50));
        assert_eq!(account.reserved, 150);
        assert_eq!(account.available_balance, 350);

        account.status = AccountStatus::ReduceOnly;
        assert!(account.release_reserve(50));
        assert_eq!(account.reserved, 100);
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::Active;
        assert!(account.release_reserve(100));
        assert_eq!(account.reserved, 0);
        assert_eq!(account.available_balance, 500);
    }

    #[test]
    fn test_account_can_withdraw() {
        // users can withdraw if active or closed, but not if frozen or in reduce only
        // risk state
        let mut account = setup_account(400, 100);

        assert!(!account.can_withdraw(0));
        assert!(account.can_withdraw(400));
        assert!(account.can_withdraw(100));
        assert!(!account.can_withdraw(401));

        account.status = AccountStatus::Closed;
        assert!(account.can_withdraw(100));

        account.status = AccountStatus::ReduceOnly;
        assert!(!account.can_withdraw(100));

        account.status = AccountStatus::Frozen;
        assert!(!account.can_withdraw(100));
    }

    #[test]
    fn test_account_withdraw() {
        // taking cash out of the exchange
        let mut account = setup_account(400, 100);

        assert!(!account.withdraw(0));
        assert!(!account.withdraw(500));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::Frozen;
        assert!(!account.withdraw(100));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::ReduceOnly;
        assert!(!account.withdraw(100));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::Closed;
        assert!(account.withdraw(100));
        assert_eq!(account.available_balance, 300);

        account.status = AccountStatus::Active;
        assert!(account.withdraw(300));
        assert_eq!(account.available_balance, 0);
    }

    #[test]
    fn test_account_consume_reserve() {
        // removes locked cash after a buy trade executes
        let mut account = setup_account(500, 300);

        assert!(!account.consume_reserve(350));
        assert_eq!(account.reserved, 300);

        assert!(account.consume_reserve(100));
        assert_eq!(account.reserved, 200);

        assert!(account.consume_reserve(200));
        assert_eq!(account.reserved, 0);
    }

    #[test]
    fn test_account_credit_amount() {
        // adding cash proceeds to the account after selling an asset
        let mut account = setup_account(100, 0);

        assert!(!account.credit_amount(0));
        assert!(!account.credit_amount(u64::MAX));

        assert!(account.credit_amount(250));
        assert_eq!(account.available_balance, 350);

        account.status = AccountStatus::Frozen;
        assert!(account.credit_amount(50));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::ReduceOnly;
        assert!(account.credit_amount(50));
        assert_eq!(account.available_balance, 450);

        account.status = AccountStatus::Closed;
        assert!(!account.credit_amount(50));
        assert_eq!(account.available_balance, 450);
    }

    #[test]
    fn test_account_can_deposit() {
        // only active accounts can deposit new money
        let mut account = setup_account(100, 0);

        assert!(account.can_deposit());

        account.status = AccountStatus::Frozen;
        assert!(!account.can_deposit());

        account.status = AccountStatus::Closed;
        assert!(!account.can_deposit());

        account.status = AccountStatus::ReduceOnly;
        assert!(!account.can_deposit());
    }

    #[test]
    fn test_account_deposit() {
        // adding new deposit money from a bank or crypto transfer
        let mut account = setup_account(100, 0);

        assert!(!account.deposit(0));
        assert!(!account.deposit(u64::MAX));

        assert!(account.deposit(200));
        assert_eq!(account.available_balance, 300);

        account.status = AccountStatus::Frozen;
        assert!(!account.deposit(50));
        assert_eq!(account.available_balance, 300);

        account.status = AccountStatus::Closed;
        assert!(!account.deposit(50));
        assert_eq!(account.available_balance, 300);

        account.status = AccountStatus::ReduceOnly;
        assert!(!account.deposit(50));
        assert_eq!(account.available_balance, 300);
    }

    #[test]
    fn test_account_can_settle_buy() {
        // checking if buyer has enough locked cash to pay for the fill
        let mut account = setup_account(500, 200);

        let price: Price = 10;
        let quantity: Quantity = 20;

        assert!(account.can_settle(price, 1, quantity, Side::Buy));
        assert!(!account.can_settle(price, 1, quantity + 1, Side::Buy));

        assert!(!account.can_settle(0, 1, quantity, Side::Buy));
        assert!(!account.can_settle(price, 1, 0, Side::Buy));
        assert!(!account.can_settle(u64::MAX, 1, 2, Side::Buy));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_settle(price, 1, quantity, Side::Buy));

        account.status = AccountStatus::Frozen;
        assert!(account.can_settle(price, 1, quantity, Side::Buy));

        account.status = AccountStatus::Closed;
        assert!(!account.can_settle(price, 1, quantity, Side::Buy));
    }

    #[test]
    fn test_account_can_settle_sell() {
        // checking if seller has enough locked coins to deliver for the fill
        let mut account = setup_account(500, 300);

        let asset_id: AssetId = 5;
        let quantity: Quantity = 10;

        assert!(!account.can_settle(100, asset_id, quantity, Side::Sell));

        account.holdings.reserved.insert(asset_id, 10);

        assert!(account.can_settle(100, asset_id, quantity, Side::Sell));
        assert!(!account.can_settle(100, asset_id, quantity + 1, Side::Sell));

        assert!(!account.can_settle(0, asset_id, quantity, Side::Sell));
        assert!(!account.can_settle(100, asset_id, 0, Side::Sell));
        assert!(!account.can_settle(u64::MAX, asset_id, 2, Side::Sell));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_settle(100, asset_id, quantity, Side::Sell));

        account.status = AccountStatus::Frozen;
        assert!(account.can_settle(100, asset_id, quantity, Side::Sell));

        account.status = AccountStatus::Closed;
        assert!(!account.can_settle(100, asset_id, quantity, Side::Sell));
    }

    #[test]
    fn test_account_settle_buy() {
        // buyer pays locked cash and receives the purchased coins into available
        // balance
        let mut account = setup_account(500, 300);

        let price: Price = 10;
        let quantity: Quantity = 20;
        let asset_id: AssetId = 1;

        assert!(!account.settle(0, asset_id, quantity, Side::Buy));
        assert!(!account.settle(price, asset_id, 0, Side::Buy));
        assert!(!account.settle(u64::MAX, asset_id, 2, Side::Buy));

        account.status = AccountStatus::Closed;
        assert!(!account.settle(price, asset_id, quantity, Side::Buy));
        assert_eq!(account.reserved, 300);
        assert_eq!(account.holdings.get_available_quantity(asset_id), None);

        account.status = AccountStatus::Frozen;
        assert!(account.settle(price, asset_id, 10, Side::Buy));
        assert_eq!(account.reserved, 200);
        assert_eq!(account.holdings.get_available_quantity(asset_id), Some(10));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.settle(price, asset_id, 10, Side::Buy));
        assert_eq!(account.reserved, 100);
        assert_eq!(account.holdings.get_available_quantity(asset_id), Some(20));

        account.status = AccountStatus::Active;
        assert!(account.settle(price, asset_id, 10, Side::Buy));
        assert_eq!(account.reserved, 0);
        assert_eq!(account.holdings.get_available_quantity(asset_id), Some(30));

        assert!(!account.settle(price, asset_id, 5, Side::Buy));
        assert_eq!(account.holdings.get_available_quantity(asset_id), Some(30));
    }

    #[test]
    fn test_account_settle_sell() {
        // seller delivers locked coins and receives cash proceeds into available
        // balance
        let mut account = setup_account(500, 300);

        let asset_id: AssetId = 5;
        let price: Price = 10;
        account.holdings.reserved.insert(asset_id, 25);

        assert!(!account.settle(0, asset_id, 5, Side::Sell));
        assert!(!account.settle(price, asset_id, 0, Side::Sell));
        assert!(!account.settle(u64::MAX, asset_id, 2, Side::Sell));

        account.status = AccountStatus::Closed;
        assert!(!account.settle(price, asset_id, 5, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(25));
        assert_eq!(account.available_balance, 500);

        account.status = AccountStatus::Frozen;
        assert!(account.settle(price, asset_id, 5, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(20));
        assert_eq!(account.available_balance, 550);

        account.status = AccountStatus::ReduceOnly;
        assert!(account.settle(price, asset_id, 10, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(10));
        assert_eq!(account.available_balance, 650);

        account.status = AccountStatus::Active;
        assert!(account.settle(price, asset_id, 10, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(0));
        assert_eq!(account.available_balance, 750);

        assert!(!account.settle(price, asset_id, 1, Side::Sell));
        assert_eq!(account.available_balance, 750);
    }
    #[test]
    fn test_overflow_holdings_methods() {
        // checking that giving huge quantity numbers to coin holding methods does not
        // panic
        let mut holdings = Holdings::default();
        let asset_id = 1;

        // trying to check or reserve the highest possible 32 bit number when we have
        // only 50 coins
        holdings.available.insert(asset_id, 50);
        assert!(!holdings.can_reserve_asset(asset_id, u32::MAX));
        assert!(!holdings.reserve_asset(asset_id, u32::MAX));

        // trying to unlock or consume the highest possible 32 bit number when we have
        // only 20 locked coins
        holdings.reserved.insert(asset_id, 20);
        assert!(!holdings.can_release_asset(asset_id, u32::MAX));
        assert!(!holdings.release_asset(asset_id, u32::MAX));
        assert!(!holdings.consume_reserved_asset(asset_id, u32::MAX));

        // crediting maximum allowed quantity into an empty asset slto should work
        // without overflow
        let new_asset = 2;
        assert!(holdings.credit_asset(new_asset, u32::MAX));
        assert_eq!(holdings.get_available_quantity(new_asset), Some(u32::MAX));
    }

    #[test]
    fn test_overflow_account_balance_methods() {
        // checking that giving huge money amounts to cash methods does not panic
        let mut account = setup_account(500, 300);

        // locking more money than exists in the universe should safely return false
        // without panicking
        assert!(!account.can_reserve(u64::MAX));
        assert!(!account.reserve(u64::MAX));

        // unlocking more money than exists in reserve should safely return false
        // without panicking
        assert!(!account.can_release_reserve(u64::MAX));
        assert!(!account.release_reserve(u64::MAX));

        // withdrawing more money than available should safely return false without
        // panicking
        assert!(!account.can_withdraw(u64::MAX));
        assert!(!account.withdraw(u64::MAX));
        assert!(!account.consume_reserve(u64::MAX));

        // depositing or crediting money that would push the ttoal balance past the 64
        // bit maximum safely returns false
        assert!(!account.deposit(u64::MAX));
        assert!(!account.credit_amount(u64::MAX));

        // balances should remain completely intact and unmodified after rejected
        // overflow attempts
        assert_eq!(account.available_balance, 500);
        assert_eq!(account.reserved, 300);
    }

    #[test]
    fn test_overflow_settle_and_can_settle_calculations() {
        // checking that extreme prices and quantities during trade settlement do not
        // crash the engine
        let mut account = setup_account(500, 300);

        let asset_id = 1;
        account.holdings.reserved.insert(asset_id, 25);

        // buy trade multiplication overflow where price times quantity exceeds 64 bit
        // maximum
        assert!(!account.can_settle(u64::MAX, asset_id, 2, Side::Buy));
        assert!(!account.can_settle(u64::MAX, asset_id, u32::MAX, Side::Buy));
        assert!(!account.settle(u64::MAX, asset_id, 2, Side::Buy));
        assert!(!account.settle(u64::MAX, asset_id, u32::MAX, Side::Buy));

        // sell trade multiplication overflow where price times quantity exceeds 64 bit
        // maximum
        assert!(!account.can_settle(u64::MAX, asset_id, 2, Side::Sell));
        assert!(!account.can_settle(u64::MAX, asset_id, u32::MAX, Side::Sell));
        assert!(!account.settle(u64::MAX, asset_id, 2, Side::Sell));
        assert!(!account.settle(u64::MAX, asset_id, u32::MAX, Side::Sell));

        // extreme quantity on sell trade that exceeds the user reserved coins
        assert!(!account.can_settle(10, asset_id, u32::MAX, Side::Sell));
        assert!(!account.settle(10, asset_id, u32::MAX, Side::Sell));

        // balances and reserved holdings must remain clean and unaffected
        assert_eq!(account.reserved, 300);
        assert_eq!(account.available_balance, 500);
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(25));
    }

    #[test]
    fn test_risk_engine_new_empty() {
        let engine = RiskEngine::new_empty();
        assert!(engine.id_map.is_empty());
        assert!(engine.accounts.is_empty());
    }

    #[test]
    fn test_risk_engine_get_internal_id() {
        let mut engine = RiskEngine::new_empty();
        let ext_user_1 = 1001;
        let ext_user_2 = 1002;

        let id1 = engine.get_internal_id(ext_user_1);
        assert_eq!(id1, 0);
        assert_eq!(engine.get_internal_id(ext_user_1), 0);

        engine
            .accounts
            .push(Account::new_with_balance_and_holdings(id1, 500, Holdings::default()));

        let id2 = engine.get_internal_id(ext_user_2);
        assert_eq!(id2, 1);
        assert_eq!(engine.get_internal_id(ext_user_2), 1);
        assert_eq!(engine.get_internal_id(ext_user_1), 0);
    }

    #[test]
    fn test_risk_engine_check_buy_order() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 100, Holdings::default());

        let valid_order = limit_order(1, Side::Buy, 100, 5);
        assert!(engine.check(ext_user, valid_order));
        assert_eq!(engine.accounts[internal_id].available_balance, 1000);
        assert_eq!(engine.accounts[internal_id].reserved, 100);

        let expensive_order = limit_order(2, Side::Buy, 200, 10);
        assert!(!engine.check(ext_user, expensive_order));
        assert_eq!(engine.accounts[internal_id].available_balance, 1000);
        assert_eq!(engine.accounts[internal_id].reserved, 100);

        let zero_qty_order = limit_order(3, Side::Buy, 100, 0);
        assert!(!engine.check(ext_user, zero_qty_order));

        let overflow_order = limit_order(4, Side::Buy, u64::MAX, 2);
        assert!(!engine.check(ext_user, overflow_order));

        engine.accounts[internal_id].status = AccountStatus::Frozen;
        assert!(!engine.check(ext_user, valid_order));

        engine.accounts[internal_id].status = AccountStatus::Closed;
        assert!(!engine.check(ext_user, valid_order));

        engine.accounts[internal_id].status = AccountStatus::ReduceOnly;
        assert!(!engine.check(ext_user, valid_order));
    }

    #[test]
    fn test_risk_engine_check_sell_order() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 0, Holdings::new(1, 50));

        let valid_order = limit_order(1, Side::Sell, 100, 30);
        assert!(engine.check(ext_user, valid_order));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(50)
        );
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), None);

        let too_much_order = limit_order(2, Side::Sell, 100, 51);
        assert!(!engine.check(ext_user, too_much_order));

        let zero_qty_order = limit_order(3, Side::Sell, 100, 0);
        assert!(!engine.check(ext_user, zero_qty_order));
    }

    #[test]
    fn test_risk_engine_reserve_buy_order() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 200, Holdings::default());

        let valid_order = limit_order(1, Side::Buy, 100, 4);
        assert!(engine.reserve(ext_user, valid_order));
        assert_eq!(engine.accounts[internal_id].available_balance, 600);
        assert_eq!(engine.accounts[internal_id].reserved, 600);

        let too_much_order = limit_order(2, Side::Buy, 100, 7);
        assert!(!engine.reserve(ext_user, too_much_order));
        assert_eq!(engine.accounts[internal_id].available_balance, 600);
        assert_eq!(engine.accounts[internal_id].reserved, 600);

        let overflow_order = limit_order(3, Side::Buy, u64::MAX, 2);
        assert!(!engine.reserve(ext_user, overflow_order));

        let zero_qty_order = limit_order(4, Side::Buy, 100, 0);
        assert!(!engine.reserve(ext_user, zero_qty_order));
    }

    #[test]
    fn test_risk_engine_reserve_sell_order() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 0, Holdings::new(1, 50));

        let valid_order = limit_order(1, Side::Sell, 100, 20);
        assert!(engine.reserve(ext_user, valid_order));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(20));

        let too_much_order = limit_order(2, Side::Sell, 100, 35);
        assert!(!engine.reserve(ext_user, too_much_order));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(20));

        let zero_qty_order = limit_order(3, Side::Sell, 100, 0);
        assert!(!engine.reserve(ext_user, zero_qty_order));
    }

    #[test]
    fn test_risk_engine_check_and_reserve_buy() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 0, Holdings::default());

        let order = limit_order(1, Side::Buy, 100, 4);
        assert!(engine.check_and_reserve(ext_user, order));
        assert_eq!(engine.accounts[internal_id].available_balance, 600);
        assert_eq!(engine.accounts[internal_id].reserved, 400);

        let fail_order = limit_order(2, Side::Buy, 100, 7);
        assert!(!engine.check_and_reserve(ext_user, fail_order));
        assert_eq!(engine.accounts[internal_id].available_balance, 600);
        assert_eq!(engine.accounts[internal_id].reserved, 400);
    }

    #[test]
    fn test_risk_engine_check_and_reserve_sell() {
        let ext_user = 10;
        let (mut engine, internal_id) = setup_engine(ext_user, 1000, 0, Holdings::new(1, 50));

        let order = limit_order(1, Side::Sell, 100, 20);
        assert!(engine.check_and_reserve(ext_user, order));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(20));

        let fail_order = limit_order(2, Side::Sell, 100, 35);
        assert!(!engine.check_and_reserve(ext_user, fail_order));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(20));
    }

    #[test]
    fn test_risk_engine_release_buy() {
        let (mut engine, internal_id) = setup_engine(1, 500, 300, Holdings::default());

        assert!(engine.release(internal_id, 1, 1, Side::Buy, 10, 20));
        assert_eq!(engine.accounts[internal_id].reserved, 100);
        assert_eq!(engine.accounts[internal_id].available_balance, 700);

        assert!(!engine.release(internal_id, 2, 1, Side::Buy, 10, 20));
        assert_eq!(engine.accounts[internal_id].reserved, 100);
        assert_eq!(engine.accounts[internal_id].available_balance, 700);

        assert!(!engine.release(internal_id, 3, 1, Side::Buy, u64::MAX, 2));
    }

    #[test]
    fn test_risk_engine_release_sell() {
        let (mut engine, internal_id) = setup_engine(1, 500, 0, holdings_with_reserved(1, 10, 30));

        assert!(engine.release(internal_id, 1, 1, Side::Sell, 10, 20));
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(10));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );

        assert!(!engine.release(internal_id, 2, 1, Side::Sell, 10, 15));
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(10));
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(30)
        );

        assert!(!engine.release(internal_id, 3, 999, Side::Sell, 10, 5));
    }

    #[test]
    fn test_risk_engine_release_sell_uninitialized_available() {
        let (mut engine, internal_id) = setup_engine(1, 500, 0, holdings_reserved(2, 30));

        assert!(!engine.release(internal_id, 1, 2, Side::Sell, 10, 20));
    }

    #[test]
    fn test_risk_engine_settle_buy() {
        let (mut engine, internal_id) = setup_engine(1, 500, 300, Holdings::new(1, 0));

        assert!(engine.settle(internal_id, 1, 1, Side::Buy, 10, 20));
        assert_eq!(engine.accounts[internal_id].reserved, 100);
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(1),
            Some(20)
        );

        assert!(!engine.settle(internal_id, 2, 1, Side::Buy, 10, 20));
        assert_eq!(engine.accounts[internal_id].reserved, 100);

        assert!(!engine.settle(internal_id, 3, 1, Side::Buy, u64::MAX, 2));
    }

    #[test]
    fn test_risk_engine_settle_buy_new_asset() {
        let (mut engine, internal_id) = setup_engine(1, 500, 300, Holdings::default());

        assert!(engine.settle(internal_id, 1, 2, Side::Buy, 10, 20));
        assert_eq!(engine.accounts[internal_id].reserved, 100);
        assert_eq!(
            engine.accounts[internal_id].holdings.get_available_quantity(2),
            Some(20)
        );
    }

    #[test]
    fn test_risk_engine_settle_sell() {
        let (mut engine, internal_id) = setup_engine(1, 500, 0, holdings_reserved(1, 30));

        assert!(engine.settle(internal_id, 1, 1, Side::Sell, 10, 20));
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(10));
        assert_eq!(engine.accounts[internal_id].available_balance, 700);

        assert!(!engine.settle(internal_id, 2, 1, Side::Sell, 10, 15));
        assert_eq!(engine.accounts[internal_id].holdings.get_reserved_quantity(1), Some(10));
        assert_eq!(engine.accounts[internal_id].available_balance, 700);

        assert!(!engine.settle(internal_id, 3, 1, Side::Sell, u64::MAX, 2));
    }
}
