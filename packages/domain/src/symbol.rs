use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Debug)]
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
    pub const ALL: [Symbol; 12] = [
        Symbol::BnbUsdt,
        Symbol::BtcInr,
        Symbol::BtcUsdc,
        Symbol::BtcUsdt,
        Symbol::EthInr,
        Symbol::EthUsdc,
        Symbol::EthUsdt,
        Symbol::InrUsdt,
        Symbol::SolInr,
        Symbol::SolUsdt,
        Symbol::UsdtInr,
        Symbol::XrpUsdt,
    ];
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
    pub fn get_quantity_unit(&self) -> Currency {
        match self {
            Symbol::BnbUsdt => Currency::Bnb,
            Symbol::BtcInr => Currency::Btc,
            Symbol::BtcUsdc => Currency::Btc,
            Symbol::BtcUsdt => Currency::Btc,
            Symbol::EthInr => Currency::Eth,
            Symbol::EthUsdc => Currency::Eth,
            Symbol::EthUsdt => Currency::Eth,
            Symbol::InrUsdt => Currency::Inr,
            Symbol::SolInr => Currency::Sol,
            Symbol::SolUsdt => Currency::Sol,
            Symbol::UsdtInr => Currency::Usdt,
            Symbol::XrpUsdt => Currency::Xrp,
        }
    }
    pub fn get_price_unit(&self) -> Currency {
        match self {
            Symbol::BnbUsdt => Currency::Usdt,
            Symbol::BtcInr => Currency::Inr,
            Symbol::BtcUsdc => Currency::Usdc,
            Symbol::BtcUsdt => Currency::Usdt,
            Symbol::EthInr => Currency::Inr,
            Symbol::EthUsdc => Currency::Usdc,
            Symbol::EthUsdt => Currency::Usdt,
            Symbol::InrUsdt => Currency::Usdt,
            Symbol::SolInr => Currency::Inr,
            Symbol::SolUsdt => Currency::Usdt,
            Symbol::UsdtInr => Currency::Inr,
            Symbol::XrpUsdt => Currency::Usdt,
        }
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Debug)]
pub enum Currency {
    // All are in there smallest units
    // eg 1 dollars = 100 cents , price will be in cents
    #[serde(rename = "USDT")]
    Usdt,
    #[serde(rename = "BTC")]
    Btc,
    #[serde(rename = "ETH")]
    Eth,
    #[serde(rename = "SOL")]
    Sol,
    #[serde(rename = "Inr")]
    Inr,
    #[serde(rename = "BNB")]
    Bnb,
    #[serde(rename = "XRP")]
    Xrp,
    #[serde(rename = "USDC")]
    Usdc,
}
