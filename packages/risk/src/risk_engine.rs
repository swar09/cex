use std::collections::HashMap;

use domain::Currency;

pub type InternalUserId = u32;
pub type ExternalUserId = u32;
pub type Balance = u64;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wallets {
    // currency and balance in its smallest unit
    pub assets: HashMap<Currency, Balance>,
}

impl Wallets {
    pub fn new() -> Self {
        Self { assets: HashMap::new() }
    }

    pub fn get_asset(&self, asset: Currency) -> Option<&u64> {
        self.assets.get(&asset)
    }

    pub fn set_asset(&mut self, asset: Currency, balance: Balance) {
        self.assets.insert(asset, balance);
    }
}

#[derive(Debug, Default)]
pub struct RiskEngine {
    pub id_map: HashMap<ExternalUserId, InternalUserId>,
    // default Currency is dollars and stored as cents
    pub accounts: Vec<Balance>, // index by internal user ids
    pub wallets: Vec<Wallets>,
}

impl RiskEngine {
    pub fn new_empty() -> Self {
        Self {
            id_map: HashMap::new(),
            accounts: Vec::new(),
            wallets: Vec::new(),
        }
    }

    pub fn new_with_ids_balance(external_ids: Vec<ExternalUserId>, balance: Vec<Balance>) -> Self {
        let len = external_ids.len();
        let mut id_map: HashMap<ExternalUserId, InternalUserId> = HashMap::with_capacity(len);
        let mut accounts = Vec::with_capacity(len);
        let mut wallets = Vec::with_capacity(len);

        for (internal_id, &external_id) in external_ids.iter().enumerate() {
            id_map.insert(external_id, internal_id as u32);
            accounts.push(balance.get(internal_id).copied().unwrap_or(0));
            wallets.push(Wallets::new());
        }

        Self {
            id_map,
            accounts,
            wallets,
        }
    }

    pub fn get_internal_id(&self, external_id: ExternalUserId) -> Option<InternalUserId> {
        self.id_map.get(&external_id).copied()
    }

    pub fn get_balance(&self, internal_id: InternalUserId) -> Option<Balance> {
        self.accounts.get(internal_id as usize).copied()
    }

    pub fn get_balance_by_external_id(&self, external_id: ExternalUserId) -> Option<Balance> {
        let internal_id = self.get_internal_id(external_id)?;
        self.get_balance(internal_id)
    }

    pub fn get_wallet(&self, internal_id: InternalUserId) -> Option<&Wallets> {
        self.wallets.get(internal_id as usize)
    }

    pub fn get_wallet_mut(&mut self, internal_id: InternalUserId) -> Option<&mut Wallets> {
        self.wallets.get_mut(internal_id as usize)
    }

    pub fn check_and_pass() {}
    pub fn reserve() {}
    pub fn check_reserve() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallets_new_and_default() {
        let w1 = Wallets::new();
        assert!(w1.assets.is_empty());

        let w2 = Wallets::default();
        assert!(w2.assets.is_empty());
        assert_eq!(w1, w2);
    }

    #[test]
    fn test_wallets_get_and_set_asset() {
        let mut wallet = Wallets::new();
        assert_eq!(wallet.get_asset(Currency::Usdt), None);

        wallet.set_asset(Currency::Usdt, 100_000);
        assert_eq!(wallet.get_asset(Currency::Usdt), Some(&100_000));
        assert_eq!(wallet.get_asset(Currency::Btc), None);
    }

    #[test]
    fn test_risk_engine_new_empty() {
        let engine = RiskEngine::new_empty();
        assert!(engine.id_map.is_empty());
        assert!(engine.accounts.is_empty());
        assert!(engine.wallets.is_empty());
    }

    #[test]
    fn test_risk_engine_new_with_ids_balance() {
        let external_ids = vec![101, 202, 303];
        let balances = vec![50_000, 75_000, 100_000];

        let engine = RiskEngine::new_with_ids_balance(external_ids, balances);

        assert_eq!(engine.id_map.len(), 3);
        assert_eq!(engine.accounts.len(), 3);
        assert_eq!(engine.wallets.len(), 3);

        // Check internal ID mapping
        assert_eq!(engine.get_internal_id(101), Some(0));
        assert_eq!(engine.get_internal_id(202), Some(1));
        assert_eq!(engine.get_internal_id(303), Some(2));
        assert_eq!(engine.get_internal_id(999), None);

        // Check account balances
        assert_eq!(engine.get_balance(0), Some(50_000));
        assert_eq!(engine.get_balance(1), Some(75_000));
        assert_eq!(engine.get_balance(2), Some(100_000));
        assert_eq!(engine.get_balance(3), None);

        // Check balance by external ID
        assert_eq!(engine.get_balance_by_external_id(101), Some(50_000));
        assert_eq!(engine.get_balance_by_external_id(202), Some(75_000));
        assert_eq!(engine.get_balance_by_external_id(303), Some(100_000));
        assert_eq!(engine.get_balance_by_external_id(999), None);
    }

    #[test]
    fn test_risk_engine_wallet_access() {
        let mut engine = RiskEngine::new_with_ids_balance(vec![42], vec![1_000]);

        // Mutate wallet for internal user 0
        if let Some(wallet) = engine.get_wallet_mut(0) {
            wallet.set_asset(Currency::Btc, 5);
        }

        let wallet = engine.get_wallet(0).expect("wallet exists");
        assert_eq!(wallet.get_asset(Currency::Btc), Some(&5));
        assert_eq!(wallet.get_asset(Currency::Eth), None);

        assert!(engine.get_wallet(99).is_none());
    }

    #[test]
    fn test_risk_engine_stubs() {
        RiskEngine::check_and_pass();
        RiskEngine::reserve();
        RiskEngine::check_reserve();
    }
}
