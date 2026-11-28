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
    /// Quote asset of the coin if the feed names one. Empty = SOL (every coin until non-SOL pairs launched).
    #[serde(alias = "quoteMint", alias = "quote_mint")]
    pub quote_mint: String,
    pub message: String,
}

/// The wrapped SOL mint. A curve whose `quote_mint` is this or all zeros is paired with SOL.
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// True when a quote mint string means something other than SOL (empty, all zeros and wrapped SOL are SOL).
pub fn quote_is_non_sol(q: &str) -> bool {
    let q = q.trim();
    !(q.is_empty() || q == WSOL_MINT || q == "11111111111111111111111111111111")
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
#[derive(Debug, Clone, PartialEq, Default)]
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
    /// Fields read from the end of the event when it is long enough (older events are shorter: false).
    pub mayhem: bool,
    pub non_sol_quote: bool,
    pub holder_reward: bool,
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
    let tail = tail_flags(&d);
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
        mayhem: tail.0,
        non_sol_quote: tail.1,
        holder_reward: tail.2,
    })
}

/// Fields that sit after the variable-length part of the TradeEvent (pump-public-docs IDL, 2026-11): `ix_name` (string),
/// `mayhem_mode`, four cashback/buyback u64, `shareholders` (vec of 34-byte entries), `quote_mint`, then three quote
/// amounts, `holder_rewards_bps`. Returns (mayhem, quote is not SOL, holder rewards). Missing bytes read as false.
fn tail_flags(d: &[u8]) -> (bool, bool, bool) {
    let rd32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let rd64 = |o: usize| d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let Some(len) = rd32(258) else { return (false, false, false) };
    if len > 64 {
        return (false, false, false);
    }
    let mut pos = 262 + len;
    let mayhem = d.get(pos).map(|b| *b != 0).unwrap_or(false);
    pos += 1 + 32;
    let Some(n) = rd32(pos) else { return (mayhem, false, false) };
    if n > 20 {
        return (mayhem, false, false);
    }
    pos += 4 + n * 34;
    let non_sol = d.get(pos..pos + 32).map(|q| q.iter().any(|b| *b != 0) && bs58::encode(q).into_string() != WSOL_MINT).unwrap_or(false);
    let holder = rd64(pos + 32 + 24).map(|b| b > 0).unwrap_or(false);
    (mayhem, non_sol, holder)
}

/// Payloads of the `Program data:` lines that the pump.fun program itself emitted. Every `Program X invoke [n]` pushes X,
/// `Program X success` / `failed` pops it, and a data line belongs to the program on top. A router or aggregator that
/// calls pump.fun can print look-alike events while IT is on top; those are ignored. After a `Log truncated` line the
/// owner of later lines is unknown, so nothing after it counts.
pub fn pump_data_lines<'a, S: AsRef<str> + 'a>(logs: impl IntoIterator<Item = &'a S>) -> Vec<&'a str> {
    let mut stack: Vec<&str> = vec![];
    let mut out = vec![];
    for l in logs {
        let l = l.as_ref();
        if l.starts_with("Log truncated") {
            break;
        }
        let Some(rest) = l.strip_prefix("Program ") else { continue };
        if let Some(b64) = rest.strip_prefix("data: ") {
            if stack.last() == Some(&PUMP_PROGRAM) {
                out.push(b64);
            }
            continue;
        }
        let mut it = rest.splitn(2, ' ');
        let (Some(id), Some(tail)) = (it.next(), it.next()) else { continue };
        if tail.starts_with("invoke [") {
            stack.push(id);
        } else if tail == "success" || tail.starts_with("failed") {
            if let Some(i) = stack.iter().rposition(|p| *p == id) {
                stack.truncate(i);
            }
        }
    }
    out
}

/// All pump.fun trades in one transaction's log lines (top of the invoke stack only).
pub fn trades_from_log_lines<'a, S: AsRef<str> + 'a>(logs: impl IntoIterator<Item = &'a S>, slot: u64, signature: &str, recv_ms: i64) -> Vec<Trade> {
    pump_data_lines(logs).into_iter().filter_map(|b| decode_trade_event(b, slot, signature, recv_ms)).collect()
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
            let lines: Vec<&str> = logs.iter().filter_map(|l| l.as_str()).collect();
            trades_from_log_lines(lines.iter(), slot, sig, recv_ms)
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

    /// Build a TradeEvent log payload with the current layout (disc, mint, sol, tokens, is_buy, user, ts, 4 reserves, then
    /// the fee fields as zeros), so tests can replay a whole market through the real decoder.
    pub fn encode_trade_event(mint: &str, user: &str, is_buy: bool, sol: f64, tokens: f64, v_sol: f64, v_tokens: f64, real_sol: f64) -> String {
        let mut d = Vec::with_capacity(260);
        d.extend_from_slice(&TRADE_DISC);
        d.extend_from_slice(&bs58::decode(mint).into_vec().unwrap());
        d.extend_from_slice(&((sol * 1e9) as u64).to_le_bytes());
        d.extend_from_slice(&((tokens * 1e6) as u64).to_le_bytes());
        d.push(is_buy as u8);
        d.extend_from_slice(&bs58::decode(user).into_vec().unwrap());
        d.extend_from_slice(&1_795_000_000i64.to_le_bytes());
        d.extend_from_slice(&((v_sol * 1e9) as u64).to_le_bytes());
        d.extend_from_slice(&((v_tokens * 1e6) as u64).to_le_bytes());
        d.extend_from_slice(&((real_sol * 1e9) as u64).to_le_bytes());
        d.extend_from_slice(&793_000_000_000_000u64.to_le_bytes()); // real token reserves
        d.resize(260, 0); // fee recipient, fee bps, fee, creator ... (not read)
        base64::engine::general_purpose::STANDARD.encode(d)
    }

    #[test]
    fn decoder_offsets_match_the_pump_idl_trade_event_layout() {
        // The first ten TradeEvent fields from the pump.fun IDL, saved in tests/fixtures. Field sizes give each offset.
        let v: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/pump_trade_event_layout.json")).unwrap();
        let size = |t: &str| match t { "pubkey" => 32, "u64" | "i64" => 8, "bool" => 1, o => panic!("unexpected type {o}") };
        let mut off = 8; // discriminator
        let mut at = std::collections::HashMap::new();
        for f in v["TradeEvent"].as_array().unwrap() {
            at.insert(f["name"].as_str().unwrap().to_string(), off);
            off += size(f["type"].as_str().unwrap());
        }
        assert_eq!(v["discriminator"], serde_json::json!(TRADE_DISC.to_vec()), "event discriminator changed");
        // what decode_trade_event reads: mint 8, sol 40, tokens 48, is_buy 56, user 57, vSol 97, vTokens 105, realSol 113
        for (name, want) in [("mint", 8), ("sol_amount", 40), ("token_amount", 48), ("is_buy", 56), ("user", 57), ("virtual_sol_reserves", 97), ("virtual_token_reserves", 105), ("real_sol_reserves", 113)] {
            assert_eq!(at[name], want, "{name} moved in the IDL: update decode_trade_event");
        }
        // and a synthetic event built from the IDL layout decodes back to the same numbers
        let mint = bs58::encode([7u8; 32]).into_string();
        let user = bs58::encode([9u8; 32]).into_string();
        let b64 = encode_trade_event(&mint, &user, true, 1.5, 1234.5, 31.5, 1_000_000_000.0, 1.5);
        let t = decode_trade_event(&b64, 5, "s", 1).unwrap();
        assert!(t.is_buy && t.mint == mint && t.user == user);
        assert!((t.sol - 1.5).abs() < 1e-9 && (t.tokens - 1234.5).abs() < 1e-6 && (t.v_sol - 31.5).abs() < 1e-9 && (t.real_sol - 1.5).abs() < 1e-9);
    }


    /// A TradeEvent with the full current tail (IDL order): `ix_name`, mayhem flag, 4 u64, empty shareholders, quote mint, quote amounts.
    pub fn encode_trade_event_tail(mint: &str, user: &str, mayhem: bool, quote: [u8; 32], holder_bps: u64) -> String {
        let b = encode_trade_event(mint, user, true, 1.0, 1000.0, 31.0, 1.0e9, 1.0);
        let mut d = base64::engine::general_purpose::STANDARD.decode(b).unwrap();
        d.truncate(258);
        d.extend_from_slice(&3u32.to_le_bytes());
        d.extend_from_slice(b"buy");
        d.push(mayhem as u8);
        d.extend_from_slice(&[0u8; 32]); // cashback bps, cashback, buyback bps, buyback fee
        d.extend_from_slice(&0u32.to_le_bytes()); // shareholders
        d.extend_from_slice(&quote);
        d.extend_from_slice(&[0u8; 24]); // quote amount, virtual and real quote reserves
        d.extend_from_slice(&holder_bps.to_le_bytes());
        d.extend_from_slice(&[0u8; 24]); // unclaimed, transfer fee, block0 fee
        base64::engine::general_purpose::STANDARD.encode(d)
    }

    const MINT_K: [u8; 32] = [7u8; 32];

    fn logs_with(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_data_lines_printed_while_pump_is_on_top_of_the_stack_count() {
        let mint = bs58::encode(MINT_K).into_string();
        let user = bs58::encode([9u8; 32]).into_string();
        let real = encode_trade_event(&mint, &user, true, 1.0, 10.0, 31.0, 1.0e9, 1.0);
        let fake = encode_trade_event(&mint, &user, true, 500.0, 10.0, 31.0, 1.0e9, 500.0);
        let router = "RoUTErxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let logs = logs_with(&[
            &format!("Program {router} invoke [1]"),
            &format!("Program data: {fake}"), // the router prints a look-alike event itself
            &format!("Program {PUMP_PROGRAM} invoke [2]"),
            "Program log: Instruction: Buy",
            &format!("Program data: {real}"), // the real one, from the pump program
            &format!("Program {PUMP_PROGRAM} success"),
            &format!("Program data: {fake}"), // back in the router: ignored again
            &format!("Program {router} success"),
        ]);
        let t = trades_from_log_lines(logs.iter(), 7, "sig", 1);
        assert_eq!(t.len(), 1, "{t:?}");
        assert!((t[0].sol - 1.0).abs() < 1e-9, "the router's fake 500 SOL event must not count");
    }

    #[test]
    fn an_event_with_no_invoke_lines_or_after_a_truncation_is_not_attributed() {
        let mint = bs58::encode(MINT_K).into_string();
        let b = encode_trade_event(&mint, &bs58::encode([9u8; 32]).into_string(), true, 1.0, 10.0, 31.0, 1.0e9, 1.0);
        assert!(trades_from_log_lines(logs_with(&[&format!("Program data: {b}")]).iter(), 1, "s", 1).is_empty());
        let logs = logs_with(&[&format!("Program {PUMP_PROGRAM} invoke [1]"), "Log truncated", &format!("Program data: {b}")]);
        assert!(trades_from_log_lines(logs.iter(), 1, "s", 1).is_empty());
        // a failed inner call pops its frame: the data line after it is the outer program's
        let logs = logs_with(&["Program Other111 invoke [1]", &format!("Program {PUMP_PROGRAM} invoke [2]"), &format!("Program {PUMP_PROGRAM} failed: custom program error: 0x1772"), &format!("Program data: {b}"), "Program Other111 success"]);
        assert!(trades_from_log_lines(logs.iter(), 1, "s", 1).is_empty());
    }

    #[test]
    fn two_pump_calls_in_one_transaction_both_count() {
        let user = bs58::encode([9u8; 32]).into_string();
        let a = encode_trade_event(&bs58::encode([1u8; 32]).into_string(), &user, true, 1.0, 10.0, 31.0, 1.0e9, 1.0);
        let b = encode_trade_event(&bs58::encode([2u8; 32]).into_string(), &user, false, 2.0, 10.0, 31.0, 1.0e9, 1.0);
        let logs = logs_with(&[&format!("Program {PUMP_PROGRAM} invoke [1]"), &format!("Program data: {a}"), &format!("Program {PUMP_PROGRAM} success"), &format!("Program {PUMP_PROGRAM} invoke [1]"), &format!("Program data: {b}"), &format!("Program {PUMP_PROGRAM} success")]);
        assert_eq!(trades_from_log_lines(logs.iter(), 1, "s", 1).len(), 2);
    }

    #[test]
    fn mayhem_and_non_sol_quote_flags_are_read_from_the_event_tail() {
        let mint = bs58::encode(MINT_K).into_string();
        let user = bs58::encode([9u8; 32]).into_string();
        let plain = decode_trade_event(&encode_trade_event_tail(&mint, &user, false, [0u8; 32], 0), 1, "s", 1).unwrap();
        assert!(!plain.mayhem && !plain.non_sol_quote && !plain.holder_reward);
        let mayhem = decode_trade_event(&encode_trade_event_tail(&mint, &user, true, [0u8; 32], 0), 1, "s", 1).unwrap();
        assert!(mayhem.mayhem && !mayhem.non_sol_quote);
        let usdc = decode_trade_event(&encode_trade_event_tail(&mint, &user, false, [5u8; 32], 0), 1, "s", 1).unwrap();
        assert!(usdc.non_sol_quote && !usdc.mayhem);
        let wsol = decode_trade_event(&encode_trade_event_tail(&mint, &user, false, bs58::decode(WSOL_MINT).into_vec().unwrap().try_into().unwrap(), 0), 1, "s", 1).unwrap();
        assert!(!wsol.non_sol_quote, "wrapped SOL is SOL");
        let holders = decode_trade_event(&encode_trade_event_tail(&mint, &user, false, [0u8; 32], 30), 1, "s", 1).unwrap();
        assert!(holders.holder_reward);
        // the old short layout (real 2026-11-26 event, no tail we can read as a flag) stays false, and a short event never panics
        let old = decode_trade_event(REAL_SELL_B64, 1, "s", 1).unwrap();
        assert!(!old.mayhem && !old.non_sol_quote);
        assert!(decode_trade_event(&encode_trade_event(&mint, &user, true, 1.0, 1.0, 1.0, 1.0, 1.0), 1, "s", 1).is_some());
        assert!(quote_is_non_sol("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v") && !quote_is_non_sol("") && !quote_is_non_sol(WSOL_MINT));
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
