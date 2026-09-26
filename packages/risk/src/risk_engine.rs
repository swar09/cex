use std::collections::HashMap;

use domain::{AssetId, Order, Price, Quantity, Side};
use parking_lot::Mutex;

use crate::risk_engine::{
    AccountOpp::{Release, Settle, Withdraw},
    AccountStatus::{Active, Closed, Frozen, ReduceOnly},
};

pub type InternalUserId = usize;
pub type ExternalUserId = usize;
pub type Balance = u64;

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
                *available_quantity -= quantity;
                let Some(reserved_quantity) = self.reserved.get_mut(&asset_id) else {
                    return false;
                };
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

                let Some(_reserved_quantity) = self.reserved.get(&asset_id) else {
                    return false;
                };

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
        let mut assets: Vec<AssetId> = self
            .available
            .keys()
            .chain(self.reserved.keys())
            .copied()
            .collect();
        assets.sort_unstable();
        assets.dedup();
        assets
    }
}

pub enum AccountOpp {
    Release,
    Reserve,
    Settle,
    Withdraw,
    Deposit,
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
            (Active, _) | (Closed, Withdraw) | (Frozen, Release) | (ReduceOnly, Withdraw | Settle | Release)
        )
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
    pub fn withdraw_reserve(&mut self, amount: Balance) -> bool {
        if amount > self.reserved {
            return false;
        }
        self.reserved -= amount;
        true
    }
    pub fn withdraw_reserve_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
        if !self.holdings.can_release_asset(asset_id, quantity) {
            false
        } else {
            self.holdings.release_asset(asset_id, quantity)
        }
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

        if side == Side::Buy {
            let Some(amount) = price.checked_mul(quantity as u64) else {
                return false;
            };
            self.can_release_reserve(amount)
        } else {
            self.holdings.can_release_asset(asset_id, quantity)
        }
    }
    pub fn settle(&mut self, price: Price, asset_id: AssetId, quantity: Quantity, side: Side) -> bool {
        if !self.status.allows(AccountOpp::Settle) {
            return false;
        }

        if side == Side::Buy {
            let Some(amount) = price.checked_mul(quantity as u64) else {
                return false;
            };
            self.withdraw_reserve(amount)
        } else {
            self.withdraw_reserve_asset(asset_id, quantity)
        }
    }
}
#[derive(Debug, Default)]
pub struct RiskEngine {
    pub id_map: HashMap<ExternalUserId, InternalUserId>,
    // default Currency is dollars and stored as cents
    pub accounts: Vec<Mutex<Account>>, // index by internal user ids
}

impl RiskEngine {
    pub fn new_empty() -> Self {
        Self {
            id_map: HashMap::new(),
            accounts: Vec::new(),
        }
    }

    pub fn check_and_pass(&mut self, user_id: ExternalUserId, asset_id: AssetId, order: Order) -> bool {
        // TODO
        let guard = self.accounts[user_id].lock();

        // cases by order types

        // order type == limit order

        // sell order case
        if order.side == Side::Sell {
            // TODO : allows to reserve asset check must be done here
            if !guard
                .holdings
                .can_reserve_asset(asset_id, order.get_remaining_quantity())
            {
                // cannot reserve asset
                return false;
            }
        } else {
            // buy order case
            let amount = order.get_price() * order.get_remaining_quantity() as u64;
            // checks are done in can reserve function
            if !guard.can_reserve(amount) {
                return false;
            }
        }

        true
    }
    pub fn reserve(&mut self, _user_id: ExternalUserId, _asset_id: AssetId, _amount: Balance, _quantity: Quantity) { // TODO 
    }
    pub fn check_reserve(&self, _user_id: ExternalUserId, _asset_id: AssetId, _amount: Balance, _quantity: Quantity) { // TODO
    }
    pub fn get_internal_id(&self, external_id: ExternalUserId) -> Option<InternalUserId> {
        self.id_map.get(&external_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_holdings_get_available_and_reserved_quantity() {
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
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.can_reserve_asset(asset_id, 10));

        holdings.available.insert(asset_id, 50);
        assert!(!holdings.can_reserve_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 0);
        assert!(holdings.can_reserve_asset(asset_id, 10));
        assert!(holdings.can_reserve_asset(asset_id, 50));
        assert!(!holdings.can_reserve_asset(asset_id, 51));
    }

    #[test]
    fn test_holdings_reserve_asset() {
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.reserve_asset(asset_id, 10));

        holdings.available.insert(asset_id, 50);
        assert!(!holdings.reserve_asset(asset_id, 60));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(50));

        assert!(!holdings.reserve_asset(asset_id, 10));

        holdings.available.insert(asset_id, 50);
        holdings.reserved.insert(asset_id, 10);
        assert!(holdings.reserve_asset(asset_id, 20));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(30));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(30));

        assert!(holdings.reserve_asset(asset_id, 30));
        assert_eq!(holdings.get_available_quantity(asset_id), Some(0));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(60));
    }

    #[test]
    fn test_holdings_can_release_asset() {
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.can_release_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 25);
        assert!(!holdings.can_release_asset(asset_id, 26));
        assert!(holdings.can_release_asset(asset_id, 25));
        assert!(holdings.can_release_asset(asset_id, 10));
        assert!(holdings.can_release_asset(asset_id, 0));
    }

    #[test]
    fn test_holdings_release_asset() {
        let mut holdings = Holdings::default();
        let asset_id = 1;

        assert!(!holdings.release_asset(asset_id, 10));

        holdings.reserved.insert(asset_id, 25);
        assert!(!holdings.release_asset(asset_id, 30));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(25));

        assert!(holdings.release_asset(asset_id, 10));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(15));

        assert!(holdings.release_asset(asset_id, 15));
        assert_eq!(holdings.get_reserved_quantity(asset_id), Some(0));
    }

    #[test]
    fn test_holdings_get_all_assets() {
        let mut holdings = Holdings::default();
        let empty_assets = holdings.get_all_assets();
        assert!(empty_assets.is_empty());

        holdings.available.insert(1, 100);
        holdings.available.insert(2, 200);
        let mut assets = holdings.get_all_assets();
        assets.sort_unstable();
        assert_eq!(assets, vec![1, 2]);
    }


    #[test]
    fn test_account_get_balances() {
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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 400,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

        assert!(account.can_withdraw(400));
        assert!(account.can_withdraw(100));
        assert!(!account.can_withdraw(401));

        account.status = AccountStatus::Closed;
        assert!(account.can_withdraw(100));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_withdraw(100));

        account.status = AccountStatus::Frozen;
        assert!(!account.can_withdraw(100));
    }

    #[test]
    fn test_account_withdraw() {
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 400,
            reserved: 100,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

        assert!(!account.withdraw(500));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::Frozen;
        assert!(!account.withdraw(100));
        assert_eq!(account.available_balance, 400);

        account.status = AccountStatus::Closed;
        assert!(account.withdraw(100));
        assert_eq!(account.available_balance, 300);

        account.status = AccountStatus::ReduceOnly;
        assert!(account.withdraw(100));
        assert_eq!(account.available_balance, 200);

        account.status = AccountStatus::Active;
        assert!(account.withdraw(200));
        assert_eq!(account.available_balance, 0);
    }

    #[test]
    fn test_account_withdraw_reserve() {
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Frozen,
            holdings: Holdings::default(),
        };

        assert!(!account.withdraw_reserve(350));
        assert_eq!(account.reserved, 300);

        assert!(account.withdraw_reserve(100));
        assert_eq!(account.reserved, 200);

        assert!(account.withdraw_reserve(200));
        assert_eq!(account.reserved, 0);
    }

    #[test]
    fn test_account_withdraw_reserve_asset() {
        let mut account = Account::default();
        let asset_id = 1;

        assert!(!account.withdraw_reserve_asset(asset_id, 10));

        account.holdings.reserved.insert(asset_id, 50);

        assert!(!account.withdraw_reserve_asset(asset_id, 60));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(50));

        assert!(account.withdraw_reserve_asset(asset_id, 20));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(30));
    }

    #[test]
    fn test_account_can_deposit() {
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
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 100,
            reserved: 0,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

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

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_settle(price, 1, quantity, Side::Buy));

        account.status = AccountStatus::Frozen;
        assert!(!account.can_settle(price, 1, quantity, Side::Buy));

        account.status = AccountStatus::Closed;
        assert!(!account.can_settle(price, 1, quantity, Side::Buy));
    }

    #[test]
    fn test_account_settle_buy() {
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

        let price: Price = 10;
        let quantity: Quantity = 20;

        account.status = AccountStatus::Frozen;
        assert!(!account.settle(price, 1, quantity, Side::Buy));
        assert_eq!(account.reserved, 300);

        account.status = AccountStatus::Closed;
        assert!(!account.settle(price, 1, quantity, Side::Buy));
        assert_eq!(account.reserved, 300);

        account.status = AccountStatus::ReduceOnly;
        assert!(account.settle(price, 1, 10, Side::Buy));
        assert_eq!(account.reserved, 200);

        account.status = AccountStatus::Active;
        assert!(account.settle(price, 1, 20, Side::Buy));
        assert_eq!(account.reserved, 0);

        assert!(!account.settle(price, 1, 5, Side::Buy));
    }

    #[test]
    fn test_account_can_settle_sell() {
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

        account.status = AccountStatus::ReduceOnly;
        assert!(account.can_settle(100, asset_id, quantity, Side::Sell));

        account.status = AccountStatus::Frozen;
        assert!(!account.can_settle(100, asset_id, quantity, Side::Sell));

        account.status = AccountStatus::Closed;
        assert!(!account.can_settle(100, asset_id, quantity, Side::Sell));
    }

    #[test]
    fn test_account_settle_sell() {
        let mut account = Account {
            user_internal_id: 1,
            available_balance: 500,
            reserved: 300,
            status: AccountStatus::Active,
            holdings: Holdings::default(),
        };

        let asset_id: AssetId = 5;
        account.holdings.reserved.insert(asset_id, 25);

        account.status = AccountStatus::Frozen;
        assert!(!account.settle(100, asset_id, 5, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(25));

        account.status = AccountStatus::Closed;
        assert!(!account.settle(100, asset_id, 5, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(25));

        account.status = AccountStatus::ReduceOnly;
        assert!(account.settle(100, asset_id, 10, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(15));

        account.status = AccountStatus::Active;
        assert!(account.settle(100, asset_id, 15, Side::Sell));
        assert_eq!(account.holdings.get_reserved_quantity(asset_id), Some(0));

        assert!(!account.settle(100, asset_id, 1, Side::Sell));
    }
}

