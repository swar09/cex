use serde::{Deserialize, Serialize};

use crate::types::AssetId;

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Debug)]
#[repr(u8)]
pub enum Symbol {
    // INR Markets
    #[serde(rename = "USDT-INR")]
    UsdtInr = 0,
    #[serde(rename = "BTC-INR")]
    BtcInr = 1,
    #[serde(rename = "ETH-INR")]
    EthInr = 2,
    #[serde(rename = "SOL-INR")]
    SolInr = 3,

    // Tether (USDT) Global Markets
    #[serde(rename = "BTC-USDT")]
    BtcUsdt = 4,
    #[serde(rename = "ETH-USDT")]
    EthUsdt = 5,
    #[serde(rename = "SOL-USDT")]
    SolUsdt = 6,
    #[serde(rename = "BNB-USDT")]
    BnbUsdt = 7,
    #[serde(rename = "XRP-USDT")]
    XrpUsdt = 8,

    // USDC Stable Coin Markets
    #[serde(rename = "BTC-USDC")]
    BtcUsdc = 9,
    #[serde(rename = "ETH-USDC")]
    EthUsdc = 10,

    // Fiat Stable Coin Market
    #[serde(rename = "INR-USDT")]
    InrUsdt = 11,

    #[serde(rename = "UNKNOWN")]
    Unknown = 12,
}

impl Symbol {
    pub const COUNT: usize = 13;
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
    #[inline(always)]
    pub const fn index(&self) -> usize {
        *self as usize
    }
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
            Symbol::Unknown => "UNKNOWN",
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
            Symbol::Unknown => Currency::Unknown,
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
            Symbol::Unknown => Currency::Unknown,
        }
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Debug)]
#[repr(u64)]
pub enum Currency {
    // All are in there smallest units
    // eg 1 dollars = 100 cents , price will be in cents
    #[serde(rename = "USDT")]
    Usdt = 1,
    #[serde(rename = "BTC")]
    Btc = 2,
    #[serde(rename = "ETH")]
    Eth = 3,
    #[serde(rename = "SOL")]
    Sol = 4,
    #[serde(rename = "Inr")]
    Inr = 5,
    #[serde(rename = "BNB")]
    Bnb = 6,
    #[serde(rename = "XRP")]
    Xrp = 7,
    #[serde(rename = "USDC")]
    Usdc = 8,

    #[serde(rename = "UNKNOWN")]
    Unknown = 9,
}

impl Currency {
    #[inline]
    pub const fn asset_id(&self) -> AssetId {
        *self as AssetId
    }
}
