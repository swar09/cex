use std::collections::HashMap;

use domain::{AssetId, Price, Quantity, Side};

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
    pub fn get_available_quantity(&self, asset_id: AssetId) -> Option<Quantity> {
        self.available.get(&asset_id).copied()
    }
    pub fn get_reserved_quantity(&self, asset_id: AssetId) -> Option<Quantity> {
        self.reserved.get(&asset_id).copied()
    }
    pub fn reserve_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
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
    // pub fn new_empty() -> Self {
    //     Self {
    //         id_map: HashMap::new(),
    //         accounts: Vec::new(),
    //     }
    // }

    // pub fn check_and_pass(&mut self, user_id: ExternalUserId, asset_id: AssetId,
    // order: Order) -> bool {     // TODO
    //     let guard = self.accounts[user_id].lock();

    //     // cases by order types

    //     // order type == limit order

    //     // sell order case
    //     if order.side == Side::Sell {
    //         // TODO : allows to reserve asset check must be done here
    //         if !guard
    //             .holdings
    //             .can_reserve_asset(asset_id, order.get_remaining_quantity())
    //         {
    //             // cannot reserve asset
    //             return false;
    //         }
    //     } else {
    //         // buy order case
    //         let amount = order.get_price() * order.get_remaining_quantity() as
    // u64;         // checks are done in can reserve function
    //         if !guard.can_reserve(amount) {
    //             return false;
    //         }
    //     }

    //     true
    // }
    // pub fn reserve(&mut self, _user_id: ExternalUserId, _asset_id: AssetId,
    // _amount: Balance, _quantity: Quantity) { // todo }
    // pub fn check_reserve(&self, _user_id: ExternalUserId, _asset_id: AssetId,
    // _amount: Balance, _quantity: Quantity) { // todo }
    // pub fn get_internal_id(&self, external_id: ExternalUserId) ->
    // Option<InternalUserId> {     self.id_map.get(&external_id).copied()
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let account = Account {
            user_internal_id: 1,
            available_balance: 1000,
            reserved: 250,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

        assert_eq!(account.get_available_balance(), Some(1000));
        assert_eq!(account.get_reserved_balance(), Some(250));
    }

    #[test]
    fn test_account_can_reserve() {
        // users can only lock money if active and they have enough free cash
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 300,
            reserved: 200,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 300,
            reserved: 200,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 400,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 400,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 100,
            reserved: 0,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 100,
            reserved: 0,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 100,
            reserved: 0,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 200,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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

        // crediting maximum allowed quantity into an empty asset slot should work
        // without overflow
        let new_asset = 2;
        assert!(holdings.credit_asset(new_asset, u32::MAX));
        assert_eq!(holdings.get_available_quantity(new_asset), Some(u32::MAX));
    }

    #[test]
    fn test_overflow_account_balance_methods() {
        // checking that giving huge money amounts to cash methods does not panic
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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

        // depositing or crediting money that would push the total balance past the 64
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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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
}
