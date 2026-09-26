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
    pub fn can_reserve_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
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

    pub fn get_all_assets(&self) -> Option<Vec<AssetId>> {
        let mut assets = Vec::new();

        for asset in self.available.keys() {
            assets.push(*asset);
        }
        Some(assets)
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
    pub fn can_reserve(&mut self, amount: Balance) -> bool {
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
        if amount > self.available_balance {
            return false;
        }
        true
    }
    pub fn deposit(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Deposit) {
            return false;
        }
        self.available_balance += amount;
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
            let amount = price * quantity as u64;
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
            let amount = price * quantity as u64;
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
        let mut guard = self.accounts[user_id].lock();

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
