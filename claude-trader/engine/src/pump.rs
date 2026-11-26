//! pump.fun / PumpPortal data shapes. Parsed leniently: unknown fields are ignored, missing ones default.
//!
//! Real new-token message (logged from the live feed 2026-11-26):
//! {"signature","mint","traderPublicKey","txType":"create","initialBuy","solAmount","bondingCurveKey",
//!  "vTokensInBondingCurve","vSolInBondingCurve","marketCapSol","name","symbol","uri","is_mayhem_mode","pool":"pump"}
//! Real migration message: {"signature","mint","txType":"migrate","pool":"pump-amm"}

use base64::Engine;
use serde::Deserialize;

pub const PUMP_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
/// Total supply of a pump.fun token (1 billion, 6 decimals).
pub const SUPPLY_TOKENS: f64 = 1_000_000_000.0;
/// Anchor event discriminator of pump.fun TradeEvent (verified against live logs).
const TRADE_DISC: [u8; 8] = [0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FeedMsg {
    pub signature: String,
    pub mint: String,
    pub trader_public_key: String,
    pub tx_type: String,
    pub initial_buy: f64,
    pub sol_amount: f64,
    pub bonding_curve_key: String,
    pub v_tokens_in_bonding_curve: f64,
    pub v_sol_in_bonding_curve: f64,
    pub market_cap_sol: f64,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub pool: String,
    pub is_mayhem_mode: bool,
    pub message: String,
}

pub enum Parsed {
    Create(FeedMsg),
    Migrate(FeedMsg),
    Info(String),
    Other,
}

pub fn parse_feed(text: &str) -> Parsed {
    let m: FeedMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(_) => return Parsed::Other,
    };
    match m.tx_type.as_str() {
        "create" if !m.mint.is_empty() => Parsed::Create(m),
        "migrate" if !m.mint.is_empty() => Parsed::Migrate(m),
        _ if !m.message.is_empty() => Parsed::Info(m.message),
        _ => Parsed::Other,
    }
}

/// One decoded pump.fun trade (from the Anchor TradeEvent in transaction logs).
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub mint: String,
    pub user: String,
    pub is_buy: bool,
    pub sol: f64,
    pub tokens: f64,
    pub slot: u64,
    pub signature: String,
    pub recv_ms: i64,
    pub v_sol: f64,
    pub v_tokens: f64,
    pub real_sol: f64,
}

impl Trade {
    /// Price in SOL per token.
    pub fn price(&self) -> f64 {
        if self.v_tokens > 0.0 { self.v_sol / self.v_tokens } else { 0.0 }
    }
    pub fn mcap_sol(&self) -> f64 {
        self.price() * SUPPLY_TOKENS
    }
}

/// Market cap in SOL from curve reserves (SOL and tokens already in whole units).
pub fn mcap_sol(v_sol: f64, v_tokens: f64) -> f64 {
    if v_tokens > 0.0 { v_sol / v_tokens * SUPPLY_TOKENS } else { 0.0 }
}

/// Decode a `Program data: <base64>` log payload into a Trade, if it is a pump.fun TradeEvent.
pub fn decode_trade_event(b64: &str, slot: u64, signature: &str, recv_ms: i64) -> Option<Trade> {
    let d = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    if d.len() < 129 || d[..8] != TRADE_DISC {
        return None;
    }
    let u64_at = |o: usize| u64::from_le_bytes(d[o..o + 8].try_into().unwrap());
    Some(Trade {
        mint: bs58::encode(&d[8..40]).into_string(),
        sol: u64_at(40) as f64 / 1e9,
        tokens: u64_at(48) as f64 / 1e6,
        is_buy: d[56] != 0,
        user: bs58::encode(&d[57..89]).into_string(),
        slot,
        signature: signature.to_string(),
        recv_ms,
        v_sol: u64_at(97) as f64 / 1e9,
        v_tokens: u64_at(105) as f64 / 1e6,
        real_sol: u64_at(113) as f64 / 1e9,
    })
}

/// Pull all trades out of one `logsNotification` JSON message. Failed transactions are ignored.
pub fn trades_from_logs_notification(v: &serde_json::Value, recv_ms: i64) -> Vec<Trade> {
    let res = &v["params"]["result"];
    let slot = res["context"]["slot"].as_u64().unwrap_or(0);
    let val = &res["value"];
    if !val["err"].is_null() {
        return vec![];
    }
    let sig = val["signature"].as_str().unwrap_or("");
    val["logs"]
        .as_array()
        .map(|logs| {
            logs.iter()
                .filter_map(|l| l.as_str()?.strip_prefix("Program data: "))
                .filter_map(|b| decode_trade_event(b, slot, sig, recv_ms))
                .collect()
        })
        .unwrap_or_default()
}

pub fn solscan_tx(sig: &str) -> String {
    format!("https://solscan.io/tx/{sig}")
}
pub fn solscan_token(mint: &str) -> String {
    format!("https://solscan.io/token/{mint}")
}
pub fn solscan_account(a: &str) -> String {
    format!("https://solscan.io/account/{a}")
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Real create message captured from the live feed.
    pub const REAL_CREATE: &str = r#"{"signature":"2UPhdYgA9HGUwidf36hTa6sxJJRYebiFQwys5TDvT3huwwtJ71BhahFSELAQYhDjVi8dLZboSquMTSpKFcjCGnax","mint":"3n3sFqTnGPohox3dZ5Kx45Wew5ibXc4qhQktUxvWNcnb","traderPublicKey":"8wX221znA7rcwhGTDuQj4vp98zQCgVkotgwUZ6x2bPrb","txType":"create","initialBuy":0,"solAmount":0,"bondingCurveKey":"FMhKXAb81dftBvCqrSDX1jTruhBgwHC3ayaArDBwfkST","vTokensInBondingCurve":1073000000,"vSolInBondingCurve":30,"marketCapSol":27.958993476234856,"name":"Very Voluptuous vAGINA","symbol":"VVV","uri":"https://j7m.io/metadata/6aa433031fff426d.json","is_mayhem_mode":false,"pool":"pump"}"#;

    /// Real sell TradeEvent payload captured from live RPC logs.
    pub const REAL_SELL_B64: &str = "vdt/007mYe4tHQWmKzElq5LzwLTxeqyBic2eh3ZywxMxiH9Ugbpy3wyYNBMAAAAAj1mGKf0BAAAA4gICnZFk6aU/wi7Vc5lvMwvtSg45qeg4y3+171fcgHoXeAdrAAAAAKEnVP8PAAAAIgkr1xmqAQChezADCQAAACJxGIuIqwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABfAAAAAAAAACm1LgAAAAAAM2ars4PqjDiCMcAFt6uDb/odmwpyw3/KXhVWsGObP2keAAAAAAAAAPK/DgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAHNlbGwAAAAAAAAAAAAAAAAAAAAAAIgTAAAAAAAAlFoXAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMmDQTAAAAAKEnVP8PAAAAoXswAwkAAAAAAAAAAAAAAAAAAAAAAAAAbjtuLQAAAAAAAAAAAAAAAA==";

    #[test]
    fn parses_real_create_message() {
        match parse_feed(REAL_CREATE) {
            Parsed::Create(m) => {
                assert_eq!(m.symbol, "VVV");
                assert_eq!(m.bonding_curve_key, "FMhKXAb81dftBvCqrSDX1jTruhBgwHC3ayaArDBwfkST");
                assert!((m.market_cap_sol - 27.959).abs() < 0.01);
                assert_eq!(m.pool, "pump");
            }
            _ => panic!("not a create"),
        }
    }

    #[test]
    fn parses_migration_and_info() {
        let m = r#"{"signature":"s","mint":"5CPiu3nmhYWWnj6BfebvMzqqwPUBm8UzBtNGW4cEpump","txType":"migrate","pool":"pump-amm"}"#;
        assert!(matches!(parse_feed(m), Parsed::Migrate(_)));
        assert!(matches!(parse_feed(r#"{"message":"Successfully subscribed to token creation events."}"#), Parsed::Info(_)));
        assert!(matches!(parse_feed("not json"), Parsed::Other));
        // unknown extra fields must not break parsing
        assert!(matches!(parse_feed(r#"{"mint":"x","txType":"create","brandNewField":[1,2]}"#), Parsed::Create(_)));
    }

    #[test]
    fn decodes_real_trade_event() {
        let t = decode_trade_event(REAL_SELL_B64, 473941165, "sig", 1).unwrap();
        assert!(!t.is_buy);
        assert_eq!(t.mint.len() >= 32, true);
        assert!((t.sol - 0.32221390).abs() < 1e-6);
        assert!((t.v_sol - 68.708214689).abs() < 1e-6);
        assert!((t.real_sol - 38.708214689).abs() < 1e-6);
        assert!(t.mcap_sol() > 100.0);
        assert!(decode_trade_event("AAAA", 1, "s", 1).is_none());
    }
}
