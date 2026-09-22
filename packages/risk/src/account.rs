use std::collections::HashMap;

use domain::Currency;

pub type InternalUserId = u32;
pub type ExternalUserId = u32;
pub type Balance = u64;
pub struct Wallets {
    // what a wallet can have ?
    // currency and balance in its smallest unit
    pub assets: HashMap<Currency, Balance>,
}

impl Wallets {
    pub fn new() -> Self {
        let assets = HashMap::new();
        Self { assets }
    }
    pub fn get_asset(&self, asset: Currency) -> Option<&u64> {
        self.assets.get(&asset)
    }
}

impl Default for Wallets {
    fn default() -> Self {
        Self::new()
    }
}
pub struct RiskEngine {
    pub id_map: HashMap<ExternalUserId, InternalUserId>,
    // default Currency is dollars and stored as cents
    pub accounts: Vec<Balance>, // index by internal user ids
    pub wallets: Vec<Wallets>,
    // pub
}

impl RiskEngine {
    pub fn new_empty() -> Self {
        Self {
            id_map: HashMap::new(),
            accounts: Vec::new(),
            wallets: Vec::new(),
        }
    }
    pub fn new_with_ids_balance(&mut self, external_ids: Vec<ExternalUserId>, balance: Vec<Balance>) -> Self {
        let len = external_ids.len();
        let mut id_map: HashMap<ExternalUserId, InternalUserId> = HashMap::new();
        let mut accounts = Vec::with_capacity(len);
        for (e, i) in external_ids.iter().enumerate() {
            id_map.insert(e as u32, *i);
            accounts.push(balance[*i as usize]);
        }
        let wallets: Vec<Wallets> = Vec::new();
        Self {
            id_map,
            accounts,
            wallets,
        }
    }
}

mod tests {}
