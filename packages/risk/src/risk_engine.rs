use std::collections::HashMap;

use domain::{AssetId, Quantity};
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
    pub fn can_release_asset(&mut self, asset_id: AssetId, quantity: Quantity) -> bool {
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
    pub fn can_release_reserve(&mut self, amount: Balance) -> bool {
        if !self.status.allows(AccountOpp::Release) {
            return false;
        }
        if !(amount > 0) {
            return false;
        }
        self.reserved >= amount
    }
    pub fn withdraw() { // TODO
    }
    pub fn can_withdraw() { // TODO
    }
    pub fn deposit() { // TODO
    }
    pub fn can_deposit() { // TODO
    }

    pub fn settle() { // TODO
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

    pub fn check_and_pass() { // TODO
    }
    pub fn reserve() { // TODO 
    }
    pub fn check_reserve() { // TODO
    }

    // pub fn new_with_ids_balance(external_ids: Vec<ExternalUserId>, balance:
    // Vec<Balance>) -> Self {     let len = external_ids.len();
    //     let mut id_map: HashMap<ExternalUserId, InternalUserId> =
    // HashMap::with_capacity(len);     let mut accounts =
    // Vec::with_capacity(len);

    //     for (internal_id, &external_id) in external_ids.iter().enumerate() {
    //         id_map.insert(external_id, internal_id);
    //         let available_balance = balance.get(internal_id).unwrap_or(&0);
    //         let account = Account {
    //             user_internal_id: internal_id,
    //             available_balance: *available_balance,
    //             reserved: 0,
    //             status: AccountStatus::default(),
    //             holdings: Holdings::default(),
    //         };

    //         accounts.push(account);
    //     }

    //     Self { id_map, accounts }
    // }

    // pub fn get_internal_id(&self, external_id: ExternalUserId) ->
    // Option<InternalUserId> {     self.id_map.get(&external_id).copied()
    // }
}
