use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub enum Symbol {
    // INR Markets
    #[serde(rename = "USDT-INR")]
    UsdtInr,
    #[serde(rename = "BTC-INR")]
    BtcInr,
    #[serde(rename = "ETH-INR")]
    EthInr,
    #[serde(rename = "SOL-INR")]
    SolInr,

    // Tether (USDT) Global Markets
    #[serde(rename = "BTC-USDT")]
    BtcUsdt,
    #[serde(rename = "ETH-USDT")]
    EthUsdt,
    #[serde(rename = "SOL-USDT")]
    SolUsdt,
    #[serde(rename = "BNB-USDT")]
    BnbUsdt,
    #[serde(rename = "XRP-USDT")]
    XrpUsdt,

    // USDC Stable Coin Markets
    #[serde(rename = "BTC-USDC")]
    BtcUsdc,
    #[serde(rename = "ETH-USDC")]
    EthUsdc,

    // Fiat Stable Coin Market
    #[serde(rename = "INR-USDT")]
    InrUsdt,
}

impl Symbol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Symbol::BnbUsdt => "BNB-USDT",
            Symbol::BtcInr => "BTC-INR",
            Symbol::BtcUsdc => "BTC-USDC",
            Symbol::BtcUsdt => "BTC-USDT",
            Symbol::EthInr => "ETH-INR",
            Symbol::EthUsdc => "ETH-USDC",
            Symbol::EthUsdt => "ETH-USDT",
            Symbol::InrUsdt => "INR-USDT",
            Symbol::SolInr => "SOL-INR",
            Symbol::SolUsdt => "SOL-USDT",
            Symbol::UsdtInr => "USDT-INR",
            Symbol::XrpUsdt => "XRP-USDT",
        }
    }
}
