//! Solana RPC access (HTTP JSON-RPC with a rate limiter) and the free trade feed.
//!
//! PumpPortal's `subscribeTokenTrade` needs an API key funded with 0.02 SOL (verified live 2026-11-26), so it is
//! NOT used. Instead one RPC WebSocket carries `logsSubscribe {mentions:[bondingCurve]}` per observed token and
//! the pump.fun TradeEvent in the logs is decoded (buy/sell, wallet, SOL, reserves, slot). Free, real time.
//! The endpoint is configurable (public mainnet-beta here, Helius later). Keys are masked in every log line.

use crate::net::{connect_wss, http_client, mask};
use crate::pump::{trades_from_logs_notification, Trade};
use crate::shield::rules::{parse_mint_account, Holder, MintInfo};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

pub const PUBLIC_RPC: &str = "https://api.mainnet-beta.solana.com";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Simple async rate limiter: at most `per_sec` acquisitions per second.
pub struct Limiter {
    interval: Duration,
    next: tokio::sync::Mutex<Instant>,
}

impl Limiter {
    pub fn new(per_sec: f64) -> Self {
        Limiter { interval: Duration::from_secs_f64(1.0 / per_sec), next: tokio::sync::Mutex::new(Instant::now()) }
    }
    pub async fn acquire(&self) {
        let wait = {
            let mut n = self.next.lock().await;
            let now = Instant::now();
            let at = (*n).max(now);
            *n = at + self.interval;
            at.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

pub struct Rpc {
    url: std::sync::RwLock<String>,
    http: reqwest::Client,
    limiter: Limiter,
}

impl Rpc {
    pub fn new(url: &str, per_sec: f64) -> Self {
        Rpc { url: std::sync::RwLock::new(url.to_string()), http: http_client(), limiter: Limiter::new(per_sec) }
    }
    fn url(&self) -> String {
        self.url.read().unwrap().clone()
    }
    /// Hot swap: later calls go to the new endpoint (a saved Helius key takes effect without a restart).
    pub fn set_url(&self, url: &str) {
        *self.url.write().unwrap() = url.to_string();
    }
    /// URL with keys hidden. Always use this in logs.
    pub fn masked_url(&self) -> String {
        mask(&self.url())
    }
    pub fn ws_url(&self) -> String {
        self.url().replacen("https://", "wss://", 1).replacen("http://", "ws://", 1)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.limiter.acquire().await;
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let resp = self.http.post(self.url()).json(&body).send().await.map_err(|e| mask(&format!("rpc send: {e}")))?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err("rpc rate limited (429)".into());
        }
        let v: Value = resp.json().await.map_err(|e| mask(&format!("rpc body: {e}")))?;
        if let Some(err) = v.get("error") {
            return Err(format!("rpc error: {}", err["message"].as_str().unwrap_or("unknown")));
        }
        Ok(v["result"].clone())
    }

    /// Mint account parsed on-chain. New mints are not visible at `confirmed` yet, so poll at `processed`
    /// (8 x 750 ms, SPEC 7). Each attempt is one RPC call.
    pub async fn get_mint(&self, mint: &str) -> Result<MintInfo, String> {
        for attempt in 0..8 {
            let r = self
                .call("getAccountInfo", json!([mint, {"encoding":"jsonParsed","commitment":"processed"}]))
                .await?;
            if !r["value"].is_null() {
                return parse_mint_account(&r["value"]);
            }
            if attempt < 7 {
                tokio::time::sleep(Duration::from_millis(750)).await;
            }
        }
        Err("mint account not visible after 8 polls".into())
    }

    /// Bonding-curve account state (RPC fallback for the exit loop). Layout: 8-byte discriminator, then
    /// virtualTokenReserves, virtualSolReserves, realTokenReserves, realSolReserves, tokenTotalSupply (u64 LE), complete (u8).
    pub async fn curve_state(&self, curve_addr: &str) -> Result<(crate::curve::Curve, bool), String> {
        let r = self.call("getAccountInfo", json!([curve_addr, {"encoding":"base64","commitment":"processed"}])).await?;
        let b64 = r["value"]["data"][0].as_str().ok_or("curve account not found")?;
        use base64::Engine;
        let d = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())?;
        decode_curve_account(&d)
    }

    /// The full bonding-curve account (mayhem flag, quote mint, holder rewards, creation slot).
    pub async fn curve_account(&self, curve_addr: &str) -> Result<CurveAccount, String> {
        let d = self.account_bytes(curve_addr).await?;
        decode_curve_account_full(&d)
    }

    /// The pump `Global` fee fields (read live: the block 0 tax rate can change).
    pub async fn global_fees(&self) -> Result<GlobalFees, String> {
        let d = self.account_bytes(GLOBAL_ACCOUNT).await?;
        decode_global_fees(&d)
    }

    async fn account_bytes(&self, addr: &str) -> Result<Vec<u8>, String> {
        let r = self.call("getAccountInfo", json!([addr, {"encoding":"base64","commitment":"processed"}])).await?;
        let b64 = r["value"]["data"][0].as_str().ok_or("account not found")?;
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| e.to_string())
    }

    /// Biggest holders from the chain itself (no RugCheck): `getTokenLargestAccounts` gives token accounts, then
    /// `getMultipleAccounts` gives each account's owner wallet. `supply_raw` is the mint's supply in base units.
    pub async fn top_holders(&self, mint: &str, supply_raw: f64) -> Result<Vec<Holder>, String> {
        let largest = self.call("getTokenLargestAccounts", json!([mint, {"commitment":"processed"}])).await?;
        let addrs: Vec<String> = largest["value"].as_array().map(|a| a.iter().filter_map(|e| e["address"].as_str().map(String::from)).collect()).unwrap_or_default();
        if addrs.is_empty() {
            return Err("no token accounts visible yet".into());
        }
        let multi = self.call("getMultipleAccounts", json!([addrs, {"encoding":"jsonParsed","commitment":"processed"}])).await?;
        parse_top_holders(&largest, &multi, supply_raw)
    }

    /// The pump.fun trades inside one confirmed transaction (top-of-stack rule applies, SPEC 5).
    pub async fn transaction_trades(&self, sig: &str) -> Result<Vec<Trade>, String> {
        let r = self.call("getTransaction", json!([sig, {"encoding":"json","commitment":"confirmed","maxSupportedTransactionVersion":0}])).await?;
        trades_from_transaction(&r, now_ms())
    }

    /// Trades of one coin that the live feed may have missed: recent signatures on the curve that are not stored yet, at most
    /// `cap` transactions read (each is one RPC call, so a reconnect can never burn the rate budget).
    pub async fn backfill_curve(&self, curve: &str, mint: &str, known: &HashSet<String>, cap: usize) -> Vec<Trade> {
        let Ok(sigs) = self.signatures(curve, 60).await else { return vec![] };
        let mut out = vec![];
        for sig in plan_backfill(&sigs, known, cap) {
            if let Ok(ts) = self.transaction_trades(&sig).await {
                out.extend(ts.into_iter().filter(|t| t.mint == mint));
            }
        }
        out
    }

    /// First funding of a wallet (SPEC 7c): the sender of its first incoming SOL transfer. Returns (status, funder, when ms,
    /// lamports) where status is `ok`, `none` (nothing found) or `old` (the wallet has 25 or more transactions or its first one
    /// is older than `fresh_hours`: not a new wallet, not judged). Two RPC calls at most.
    pub async fn first_funding(&self, wallet: &str, fresh_hours: u32) -> Result<(String, String, i64, u64), String> {
        let list = self.call("getSignaturesForAddress", json!([wallet, {"limit": crate::funder::FRESH_MAX_SIGNATURES, "commitment":"confirmed"}])).await?;
        let a = list.as_array().cloned().unwrap_or_default();
        if a.len() >= crate::funder::FRESH_MAX_SIGNATURES {
            return Ok(("old".into(), String::new(), 0, 0));
        }
        let Some(first) = a.last() else { return Ok(("none".into(), String::new(), 0, 0)) };
        let ms = first["blockTime"].as_i64().map(|t| t * 1000).unwrap_or(0);
        if ms > 0 && now_ms() - ms > fresh_hours as i64 * 3_600_000 {
            return Ok(("old".into(), String::new(), ms, 0));
        }
        let sig = first["signature"].as_str().ok_or("no signature")?;
        let tx = self.call("getTransaction", json!([sig, {"encoding":"jsonParsed","commitment":"confirmed","maxSupportedTransactionVersion":0}])).await?;
        Ok(match crate::funder::funding_source_from_tx(&tx, wallet) {
            Some((f, l)) => ("ok".into(), f, ms, l),
            None => ("none".into(), String::new(), ms, 0),
        })
    }

    /// (signature, slot, failed) newest first.
    pub async fn signatures(&self, addr: &str, limit: u32) -> Result<Vec<(String, u64, bool)>, String> {
        let r = self.call("getSignaturesForAddress", json!([addr, {"limit": limit, "commitment":"confirmed"}])).await?;
        Ok(r.as_array()
            .map(|a| {
                a.iter()
                    .map(|e| (e["signature"].as_str().unwrap_or("").to_string(), e["slot"].as_u64().unwrap_or(0), !e["err"].is_null()))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// Pure part of `Rpc::top_holders`: owners and percentages from the two RPC answers.
pub fn parse_top_holders(largest: &Value, multi: &Value, supply_raw: f64) -> Result<Vec<Holder>, String> {
    if supply_raw <= 0.0 {
        return Err("mint supply unknown".into());
    }
    let list = largest["value"].as_array().ok_or("no holder list")?;
    let accts = multi["value"].as_array().ok_or("no account list")?;
    let mut out = vec![];
    for (i, e) in list.iter().enumerate() {
        let amount = e["amount"].as_str().and_then(|a| a.parse::<f64>().ok()).unwrap_or(0.0);
        let owner = accts.get(i).map(|a| &a["data"]["parsed"]["info"]["owner"]).and_then(|o| o.as_str()).unwrap_or("");
        if owner.is_empty() {
            // an account we cannot read must not be silently dropped: list it under its own address
            out.push(Holder { owner: e["address"].as_str().unwrap_or("").to_string(), pct: amount / supply_raw * 100.0, insider: false });
        } else {
            out.push(Holder { owner: owner.to_string(), pct: amount / supply_raw * 100.0, insider: false });
        }
    }
    Ok(out)
}

/// Smallest bonding-curve account we accept: everything up to and including `is_holder_reward` (byte 124). Accounts
/// written before a field existed are shorter and the missing trailing fields read as zero / false (pump-public-docs,
/// PUMP_PROGRAM_README "Fields appended to BondingCurve": 125, 151, 166 and 206 byte accounts exist; today's is 206).
pub const MIN_CURVE_ACCOUNT_LEN: usize = 125;

/// The bonding-curve account, decoded (pump IDL, BondingCurve): reserves at 8..48, `complete` at 48, `creator` 49..81,
/// `is_mayhem_mode` 81, `is_cashback_coin` 82, `quote_mint` 83..115, `is_holder_reward` 124, `created_at_slot` 166..174.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveAccount {
    pub curve: crate::curve::Curve,
    pub complete: bool,
    pub creator: String,
    pub is_mayhem: bool,
    pub non_sol_quote: bool,
    pub is_holder_reward: bool,
    /// 0 on curves created before the field existed.
    pub created_at_slot: u64,
    pub len: usize,
}

pub fn decode_curve_account_full(d: &[u8]) -> Result<CurveAccount, String> {
    if d.len() < MIN_CURVE_ACCOUNT_LEN {
        return Err(format!("curve account too short ({} bytes, need at least {MIN_CURVE_ACCOUNT_LEN})", d.len()));
    }
    let u = |o: usize| d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    let curve = crate::curve::Curve { v_tokens: u(8) as f64 / 1e6, v_sol: u(16) as f64 / 1e9, real_sol: u(32) as f64 / 1e9 };
    let quote = &d[83..115];
    let non_sol_quote = quote.iter().any(|b| *b != 0) && bs58::encode(quote).into_string() != crate::pump::WSOL_MINT;
    Ok(CurveAccount {
        curve,
        complete: d[48] != 0,
        creator: bs58::encode(&d[49..81]).into_string(),
        is_mayhem: d[81] != 0,
        non_sol_quote,
        is_holder_reward: d[124] != 0,
        created_at_slot: u(166),
        len: d.len(),
    })
}

/// Which signatures to fetch for a backfill: newest first, failed ones and ones we already hold skipped, at most `cap`.
pub fn plan_backfill(sigs: &[(String, u64, bool)], known: &HashSet<String>, cap: usize) -> Vec<String> {
    sigs.iter().filter(|(s, _, failed)| !*failed && !known.contains(s)).take(cap).map(|(s, _, _)| s.clone()).collect()
}

/// Trades from a `getTransaction` answer. A failed transaction has none. Time is the block time when the node gives one.
pub fn trades_from_transaction(r: &Value, fallback_ms: i64) -> Result<Vec<Trade>, String> {
    if r.is_null() {
        return Err("transaction not found".into());
    }
    if !r["meta"]["err"].is_null() {
        return Ok(vec![]);
    }
    let slot = r["slot"].as_u64().unwrap_or(0);
    let sig = r["transaction"]["signatures"][0].as_str().unwrap_or("");
    let ms = r["blockTime"].as_i64().map(|t| t * 1000).unwrap_or(fallback_ms);
    let logs: Vec<&str> = r["meta"]["logMessages"].as_array().map(|a| a.iter().filter_map(|l| l.as_str()).collect()).unwrap_or_default();
    Ok(crate::pump::trades_from_log_lines(logs.iter(), slot, sig, ms))
}

/// Liveness of the node connection from `slotSubscribe` (about 2.5 slot updates a second). A half-open socket never fires
/// `close`, and silence on a quiet coin looks the same as a dead socket, so trade silence is a slow signal. A stalled slot
/// clock is not: no slot update for `STALL_MS` means the socket is dead and is reopened.
#[derive(Debug, Clone, Default)]
pub struct SlotWatch {
    start_ms: i64,
    last_ms: Option<i64>,
    disabled: bool,
    pub slots_seen: u64,
}

pub const SLOT_SUB_ID: u64 = 1;
/// No slot update for this long on a connection that has been delivering them = stalled.
pub const SLOT_STALL_MS: i64 = 6_000;
/// A connection that never delivers a first slot update within this time is judged stalled too (unless the node refused the
/// subscription, which switches the watchdog off).
pub const SLOT_FIRST_WAIT_MS: i64 = 15_000;

impl SlotWatch {
    pub fn started(now_ms: i64) -> SlotWatch {
        SlotWatch { start_ms: now_ms, ..Default::default() }
    }
    pub fn on_slot(&mut self, now_ms: i64) {
        self.last_ms = Some(now_ms);
        self.slots_seen += 1;
    }
    /// The node refused `slotSubscribe`: no watchdog (the ping and the trade-silence health stay).
    pub fn disable(&mut self) {
        self.disabled = true;
    }
    /// Some(reason) when the connection should be dropped and reopened.
    pub fn check(&self, now_ms: i64) -> Option<String> {
        if self.disabled {
            return None;
        }
        match self.last_ms {
            Some(t) if now_ms - t > SLOT_STALL_MS => Some(format!("no slot update for {} s (socket stalled)", (now_ms - t) / 1000)),
            None if now_ms - self.start_ms > SLOT_FIRST_WAIT_MS => Some(format!("no slot update {} s after connecting (socket stalled)", (now_ms - self.start_ms) / 1000)),
            _ => None,
        }
    }
}

pub fn decode_curve_account(d: &[u8]) -> Result<(crate::curve::Curve, bool), String> {
    decode_curve_account_full(d).map(|a| (a.curve, a.complete))
}

/// The pump `Global` account (PDA of `["global"]`).
pub const GLOBAL_ACCOUNT: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";

/// Fee fields of `Global` (pump IDL): `fee_basis_points` 105, `creator_fee_basis_points` 154, `buyback_basis_points` 997,
/// `block0_tax_bps` 1096..1104 (docs/BLOCK0_TAX.md: live since 2026-11-27, 0 until further notice). A shorter account
/// (before a field existed) reads the missing ones as 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlobalFees {
    pub fee_bps: u64,
    pub creator_fee_bps: u64,
    pub buyback_bps: u64,
    pub block0_tax_bps: u64,
    pub len: usize,
}

pub fn decode_global_fees(d: &[u8]) -> Result<GlobalFees, String> {
    if d.len() < 113 {
        return Err(format!("global account too short ({} bytes)", d.len()));
    }
    let u = |o: usize| d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
    Ok(GlobalFees { fee_bps: u(105), creator_fee_bps: u(154), buyback_bps: u(997), block0_tax_bps: u(1096), len: d.len() })
}

impl GlobalFees {
    /// Per-side percent the curve charges: protocol plus creator fee.
    pub fn pump_pct_per_side(&self) -> f64 {
        (self.fee_bps + self.creator_fee_bps) as f64 / 100.0
    }
    pub fn block0_pct(&self) -> f64 {
        self.block0_tax_bps as f64 / 100.0
    }
}

// ------------------------------------------------------------------ trade store + log feed

#[derive(Default)]
pub struct TradeStore {
    /// mint -> (trades, dedup keys). Only mints that were `start`ed are tracked; everything is freed on `stop`.
    inner: Mutex<HashMap<String, (Vec<Trade>, HashSet<String>)>>,
    /// Every accepted trade is also appended to the raw feed recording (SPEC 4f) when one is set.
    recorder: Mutex<Option<Arc<crate::recorder::Recorder>>>,
}

impl TradeStore {
    pub fn set_recorder(&self, r: Arc<crate::recorder::Recorder>) {
        *self.recorder.lock().unwrap() = Some(r);
    }
    pub fn recorder(&self) -> Option<Arc<crate::recorder::Recorder>> {
        self.recorder.lock().unwrap().clone()
    }
    pub fn add(&self, t: Trade) {
        let key = format!("{}:{}:{}", t.signature, t.user, t.is_buy);
        let accepted = {
            let mut g = self.inner.lock().unwrap();
            match g.get_mut(&t.mint) {
                Some((v, seen)) => {
                    if seen.insert(key) {
                        v.push(t.clone());
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        };
        if accepted {
            if let Some(r) = self.recorder() {
                r.record_trade(&t);
            }
        }
    }
    /// Signatures already stored for a mint (backfill skips these).
    pub fn known_signatures(&self, mint: &str) -> HashSet<String> {
        self.inner.lock().unwrap().get(mint).map(|e| e.0.iter().map(|t| t.signature.clone()).collect()).unwrap_or_default()
    }
    /// Newest trade (by slot, then arrival) of a tracked mint.
    pub fn last(&self, mint: &str) -> Option<Trade> {
        self.inner.lock().unwrap().get(mint).and_then(|e| e.0.iter().max_by_key(|t| (t.slot, t.recv_ms)).cloned())
    }
    pub fn start(&self, mint: &str) {
        self.inner.lock().unwrap().entry(mint.to_string()).or_default();
    }
    pub fn snapshot(&self, mint: &str) -> Vec<Trade> {
        self.inner.lock().unwrap().get(mint).map(|e| e.0.clone()).unwrap_or_default()
    }
    pub fn stop(&self, mint: &str) {
        self.inner.lock().unwrap().remove(mint);
    }
    pub fn active(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

/// What the trade feed has been doing, for the health line (Memecoin Monitor node and Settings > Connections).
#[derive(Default)]
pub struct FeedStats {
    pub trades_total: std::sync::atomic::AtomicU64,
    pub notifications: std::sync::atomic::AtomicU64,
    pub sub_acks: std::sync::atomic::AtomicU64,
    pub sub_errors: std::sync::atomic::AtomicU64,
    /// Subscriptions the node has confirmed on the current connection.
    pub subs_active: std::sync::atomic::AtomicUsize,
    /// Coins we asked to watch.
    pub subs_wanted: std::sync::atomic::AtomicUsize,
    pub last_trade_ms: std::sync::atomic::AtomicI64,
    recent: Mutex<std::collections::VecDeque<i64>>,
    last_error: Mutex<String>,
}

impl FeedStats {
    pub fn record_trade(&self, now: i64) {
        use std::sync::atomic::Ordering::Relaxed;
        self.trades_total.fetch_add(1, Relaxed);
        self.last_trade_ms.store(now, Relaxed);
        let mut g = self.recent.lock().unwrap();
        g.push_back(now);
        while g.front().map_or(false, |t| now - *t > 300_000) {
            g.pop_front();
        }
    }
    /// Trades decoded in the last `window_ms`.
    pub fn trades_within(&self, now: i64, window_ms: i64) -> u32 {
        self.recent.lock().unwrap().iter().filter(|t| now - **t <= window_ms).count() as u32
    }
    pub fn set_error(&self, e: &str) {
        self.sub_errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self.last_error.lock().unwrap() = e.to_string();
    }
    pub fn last_error(&self) -> String {
        self.last_error.lock().unwrap().clone()
    }
}

enum FeedCmd {
    Sub { mint: String, curve: String },
    Unsub { mint: String },
}

#[derive(Clone)]
pub struct TradeFeed {
    tx: mpsc::UnboundedSender<FeedCmd>,
    /// New WebSocket endpoint (hot swap). The feed task reconnects at once when it changes.
    url_tx: tokio::sync::watch::Sender<String>,
    pub store: Arc<TradeStore>,
    pub connected: Arc<std::sync::atomic::AtomicBool>,
    /// When the current connection was established (ms), 0 while disconnected. Trades in a gap are lost.
    pub connected_since_ms: Arc<std::sync::atomic::AtomicI64>,
    pub stats: Arc<FeedStats>,
}

impl TradeFeed {
    /// Start the background task. `ws_url` is the RPC's WebSocket URL.
    pub fn spawn(ws_url: String, rpc: Option<Arc<Rpc>>, log: Arc<dyn Fn(String) + Send + Sync>) -> TradeFeed {
        let (tx, rx) = mpsc::unbounded_channel();
        let store = Arc::new(TradeStore::default());
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let since = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let (url_tx, url_rx) = tokio::sync::watch::channel(ws_url);
        let stats = Arc::new(FeedStats::default());
        tokio::spawn(feed_task(url_rx, rx, store.clone(), connected.clone(), since.clone(), stats.clone(), rpc, log));
        TradeFeed { tx, url_tx, store, connected, connected_since_ms: since, stats }
    }
    /// Point the live trade feed at a new endpoint. Existing subscriptions are re-sent after the reconnect.
    pub fn set_ws_url(&self, ws_url: &str) {
        self.url_tx.send_if_modified(|u| {
            if u != ws_url {
                *u = ws_url.to_string();
                true
            } else {
                false
            }
        });
    }
    /// A feed with no network task: trades are pushed into `store` by hand (scripted replay and tests).
    pub fn offline() -> TradeFeed {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (url_tx, _) = tokio::sync::watch::channel(String::new());
        TradeFeed { tx, url_tx, store: Arc::new(TradeStore::default()), connected: Arc::new(std::sync::atomic::AtomicBool::new(true)), connected_since_ms: Arc::new(std::sync::atomic::AtomicI64::new(0)), stats: Arc::new(FeedStats::default()) }
    }
    pub fn subscribe(&self, mint: &str, curve: &str) {
        self.store.start(mint);
        self.stats.subs_wanted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let _ = self.tx.send(FeedCmd::Sub { mint: mint.into(), curve: curve.into() });
    }
    pub fn unsubscribe(&self, mint: &str) {
        self.store.stop(mint);
        let _ = self.stats.subs_wanted.fetch_update(std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed, |n| Some(n.saturating_sub(1)));
        let _ = self.tx.send(FeedCmd::Unsub { mint: mint.into() });
    }
}

/// Per-connection bookkeeping: which subscribe request belongs to which coin, and which subscriptions the node confirmed.
#[derive(Default)]
pub struct FeedSession {
    /// request id -> (mint, when sent, resends so far)
    pub pending: HashMap<u64, (String, Instant, u8)>,
    /// mint -> subscription id
    pub subs: HashMap<String, u64>,
    pub next_id: u64,
    /// Slot clock of this connection (the liveness watchdog).
    pub slots: SlotWatch,
}

/// One text message from the RPC WebSocket: a `logsNotification` (decode trades into the store), a subscribe
/// confirmation, or an error. Everything the feed learns goes through here, so tests drive the real path.
pub fn on_ws_text(text: &str, now: i64, want: &HashMap<String, String>, store: &TradeStore, stats: &FeedStats, sess: &mut FeedSession, log: &dyn Fn(String)) {
    use std::sync::atomic::Ordering::Relaxed;
    let Ok(v) = serde_json::from_str::<Value>(text) else { return };
    if v["method"] == "slotNotification" {
        sess.slots.on_slot(now);
        return;
    }
    if v["id"].as_u64() == Some(SLOT_SUB_ID) {
        // the answer to slotSubscribe: a refusal only switches the watchdog off, it is not a trade-feed error
        if v["error"].is_object() {
            sess.slots.disable();
            log(format!("trade feed: the node refused slotSubscribe ({}), liveness watchdog off", mask(&v["error"]["message"].to_string())));
        }
        return;
    }
    if v["method"] == "logsNotification" {
        stats.notifications.fetch_add(1, Relaxed);
        for tr in trades_from_logs_notification(&v, now) {
            stats.record_trade(now);
            store.add(tr);
        }
    } else if let (Some(id), Some(sub)) = (v["id"].as_u64(), v["result"].as_u64()) {
        if let Some((mint, _, _)) = sess.pending.remove(&id) {
            stats.sub_acks.fetch_add(1, Relaxed);
            if want.contains_key(&mint) {
                sess.subs.insert(mint, sub);
            }
            stats.subs_active.store(sess.subs.len(), Relaxed);
        }
    } else if v["error"].is_object() {
        let msg = mask(&v["error"].to_string());
        // a failed subscribe must not stay "pending" forever
        if let Some(id) = v["id"].as_u64() {
            sess.pending.remove(&id);
        }
        if stats.sub_errors.load(Relaxed) < 5 {
            log(format!("trade feed: rpc said {msg}"));
        }
        stats.set_error(&msg);
    }
}

async fn feed_task(
    mut url_rx: tokio::sync::watch::Receiver<String>,
    mut rx: mpsc::UnboundedReceiver<FeedCmd>,
    store: Arc<TradeStore>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    since: Arc<std::sync::atomic::AtomicI64>,
    stats: Arc<FeedStats>,
    rpc: Option<Arc<Rpc>>,
    log: Arc<dyn Fn(String) + Send + Sync>,
) {
    use std::sync::atomic::Ordering;
    let mut want: HashMap<String, String> = HashMap::new(); // mint -> curve
    let mut backoff = 1u64;
    // when the last message arrived on the previous connection (0 = none yet): the start of a gap
    let mut last_msg_ms = 0i64;
    loop {
        let ws_url = url_rx.borrow_and_update().clone();
        let mut ws = match connect_wss(&ws_url).await {
            Ok(w) => w,
            Err(e) => {
                log(format!("trade feed: connect failed ({}), retry in {backoff}s", mask(&e)));
                stats.set_error(&format!("could not connect: {}", mask(&e)));
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(30);
                // keep draining commands so the queue does not grow
                while let Ok(c) = rx.try_recv() {
                    apply_cmd(&mut want, c);
                }
                continue;
            }
        };
        backoff = 1;
        connected.store(true, Ordering::Relaxed);
        since.store(now_ms(), Ordering::Relaxed);
        log("trade feed: connected".into());
        let mut sess = FeedSession { next_id: 100, slots: SlotWatch::started(now_ms()), ..Default::default() };
        stats.subs_active.store(0, Ordering::Relaxed);
        let _ = ws.send(Message::Text(json!({"jsonrpc":"2.0","id":SLOT_SUB_ID,"method":"slotSubscribe"}).to_string())).await;
        // what we missed while the socket was down: record the gap and read the missed trades of every watched coin
        if last_msg_ms > 0 && !want.is_empty() {
            let now = now_ms();
            if let Some(r) = store.recorder() {
                r.record_gap(last_msg_ms, now, "trade feed reconnected");
            }
            if let Some(rpc) = rpc.clone() {
                spawn_backfill(rpc, store.clone(), want.clone(), stats.clone(), log.clone());
            }
        }
        for (mint, curve) in want.clone() {
            sess.next_id += 1;
            sess.pending.insert(sess.next_id, (mint, Instant::now(), 0));
            let _ = ws.send(Message::Text(sub_msg(sess.next_id, &curve))).await;
        }
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        let mut resub = tokio::time::interval(Duration::from_secs(5));
        let mut dog = tokio::time::interval(Duration::from_secs(1));
        let mut swapped = false;
        let end: String = loop {
            tokio::select! {
                _ = url_rx.changed() => { swapped = true; break "endpoint changed".into(); }
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { return };
                    match cmd {
                        FeedCmd::Sub { mint, curve } => {
                            want.insert(mint.clone(), curve.clone());
                            sess.next_id += 1;
                            sess.pending.insert(sess.next_id, (mint, Instant::now(), 0));
                            if ws.send(Message::Text(sub_msg(sess.next_id, &curve))).await.is_err() { break "send failed".into(); }
                        }
                        FeedCmd::Unsub { mint } => {
                            want.remove(&mint);
                            if let Some(id) = sess.subs.remove(&mint) {
                                stats.subs_active.store(sess.subs.len(), Ordering::Relaxed);
                                sess.next_id += 1;
                                let m = json!({"jsonrpc":"2.0","id":sess.next_id,"method":"logsUnsubscribe","params":[id]}).to_string();
                                let _ = ws.send(Message::Text(m)).await;
                            }
                        }
                    }
                }
                _ = resub.tick() => {
                    // A subscribe the node never answered (dropped message, busy node): send it again, up to 3 times.
                    let stale: Vec<u64> = sess.pending.iter().filter(|(_, (_, at, n))| at.elapsed() > Duration::from_secs(8) && *n < 3).map(|(id, _)| *id).collect();
                    for old in stale {
                        if let Some((mint, _, n)) = sess.pending.remove(&old) {
                            if let Some(curve) = want.get(&mint).cloned() {
                                sess.next_id += 1;
                                sess.pending.insert(sess.next_id, (mint, Instant::now(), n + 1));
                                if ws.send(Message::Text(sub_msg(sess.next_id, &curve))).await.is_err() { break; }
                            }
                        }
                    }
                    // anything that never got an answer after 3 resends is dropped, so the health line shows the gap
                    sess.pending.retain(|_, (m, at, n)| want.contains_key(m) && !(*n >= 3 && at.elapsed() > Duration::from_secs(8)));
                }
                _ = dog.tick() => {
                    if let Some(why) = sess.slots.check(now_ms()) { break why; }
                }
                _ = ping.tick() => {
                    if ws.send(Message::Ping(vec![])).await.is_err() { break "ping failed".into(); }
                    log(format!("trade feed: {} subscriptions confirmed ({} wanted), {} log messages, {} trades decoded in total", sess.subs.len(), want.len(), stats.notifications.load(Ordering::Relaxed), stats.trades_total.load(Ordering::Relaxed)));
                }
                m = ws.next() => {
                    match m {
                        Some(Ok(Message::Text(t))) => { last_msg_ms = now_ms(); on_ws_text(&t, last_msg_ms, &want, &store, &stats, &mut sess, &*log) }
                        Some(Ok(Message::Close(_))) | None => break "closed by server".into(),
                        Some(Err(e)) => break mask(&e.to_string()),
                        _ => {}
                    }
                }
            }
        };
        connected.store(false, Ordering::Relaxed);
        since.store(0, Ordering::Relaxed);
        stats.subs_active.store(0, Ordering::Relaxed);
        if swapped {
            log("trade feed: endpoint changed, reconnecting now".into());
            continue;
        }
        stats.set_error(&format!("connection lost: {end}"));
        log(format!("trade feed: disconnected ({end}), reconnecting in {backoff}s"));
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

/// At most this many transactions are read per coin and per reconnect, and in total, so a reconnect cannot burn the RPC budget.
pub const BACKFILL_PER_COIN: usize = 12;
pub const BACKFILL_TOTAL: usize = 60;

/// Read the trades the live feed missed while it was down (`getSignaturesForAddress` on each watched curve, then
/// `getTransaction` for the ones we do not hold) and put them into the store, which drops repeats.
fn spawn_backfill(rpc: Arc<Rpc>, store: Arc<TradeStore>, want: HashMap<String, String>, stats: Arc<FeedStats>, log: Arc<dyn Fn(String) + Send + Sync>) {
    tokio::spawn(async move {
        let mut budget = BACKFILL_TOTAL;
        let mut added = 0usize;
        for (mint, curve) in want {
            if budget == 0 {
                break;
            }
            let cap = BACKFILL_PER_COIN.min(budget);
            let known = store.known_signatures(&mint);
            let found = rpc.backfill_curve(&curve, &mint, &known, cap).await;
            budget = budget.saturating_sub(cap);
            for t in found {
                if !known.contains(&t.signature) {
                    stats.record_trade(now_ms());
                    store.add(t);
                    added += 1;
                }
            }
        }
        log(format!("trade feed: reconnected, backfilled {added} missed trade(s)"));
    });
}

fn apply_cmd(want: &mut HashMap<String, String>, c: FeedCmd) {
    match c {
        FeedCmd::Sub { mint, curve } => {
            want.insert(mint, curve);
        }
        FeedCmd::Unsub { mint } => {
            want.remove(&mint);
        }
    }
}

fn sub_msg(id: u64, curve: &str) -> String {
    json!({"jsonrpc":"2.0","id":id,"method":"logsSubscribe","params":[{"mentions":[curve]},{"commitment":"processed"}]}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pump::tests::REAL_SELL_B64;

    #[test]
    fn curve_account_layout_decodes() {
        let mut d = vec![0u8; 125];
        d[8..16].copy_from_slice(&(7_000_000_000_000_000u64).to_le_bytes()); // 7e9 tokens? no: 7e15 / 1e6 = 7e9
        d[16..24].copy_from_slice(&(45_500_000_000u64).to_le_bytes());
        d[32..40].copy_from_slice(&(15_500_000_000u64).to_le_bytes());
        d[48] = 1;
        let (c, done) = decode_curve_account(&d).unwrap();
        assert!((c.v_sol - 45.5).abs() < 1e-9 && (c.real_sol - 15.5).abs() < 1e-9 && (c.v_tokens - 7.0e9).abs() < 1.0);
        assert!(done);
        assert!(decode_curve_account(&d[..124]).is_err(), "under 125 bytes is not a curve account we can read");
    }

    fn curve_bytes(len: usize) -> Vec<u8> {
        let mut d = vec![0u8; len];
        d[8..16].copy_from_slice(&1_000_000_000_000_000u64.to_le_bytes());
        d[16..24].copy_from_slice(&60_000_000_000u64.to_le_bytes());
        d[32..40].copy_from_slice(&30_000_000_000u64.to_le_bytes());
        d
    }

    #[test]
    fn any_curve_account_length_from_125_to_206_decodes_and_missing_fields_read_as_zero() {
        for len in [125usize, 151, 166, 206] {
            let a = decode_curve_account_full(&curve_bytes(len)).unwrap_or_else(|e| panic!("{len}: {e}"));
            assert!((a.curve.real_sol - 30.0).abs() < 1e-9 && (a.curve.v_sol - 60.0).abs() < 1e-9, "{len}");
            assert!(!a.is_mayhem && !a.non_sol_quote && !a.is_holder_reward && !a.complete);
            assert_eq!(a.created_at_slot, 0);
            assert_eq!(a.len, len);
        }
        // longer than today's 206: still readable (a future upgrade only appends)
        assert!(decode_curve_account_full(&curve_bytes(240)).is_ok());
        assert!(decode_curve_account_full(&curve_bytes(124)).is_err());
    }

    #[test]
    fn curve_flags_mayhem_quote_mint_holder_rewards_and_creation_slot_are_read_at_the_idl_offsets() {
        let mut d = curve_bytes(206);
        d[81] = 1; // is_mayhem_mode
        d[83..115].copy_from_slice(&[5u8; 32]); // quote_mint = something that is not SOL
        d[124] = 1; // is_holder_reward
        d[166..174].copy_from_slice(&123_456_789u64.to_le_bytes());
        d[49..81].copy_from_slice(&[9u8; 32]);
        let a = decode_curve_account_full(&d).unwrap();
        assert!(a.is_mayhem && a.non_sol_quote && a.is_holder_reward);
        assert_eq!(a.created_at_slot, 123_456_789);
        assert_eq!(a.creator, bs58::encode([9u8; 32]).into_string());
        // wrapped SOL as the quote is still SOL
        let mut w = curve_bytes(206);
        w[83..115].copy_from_slice(&bs58::decode(crate::pump::WSOL_MINT).into_vec().unwrap());
        assert!(!decode_curve_account_full(&w).unwrap().non_sol_quote);
        // a curve that is complete (the synthetic migration state: every trade fails until migrate_v2)
        let mut c = curve_bytes(206);
        c[48] = 1;
        assert!(decode_curve_account_full(&c).unwrap().complete);
    }

    #[test]
    fn global_fee_fields_are_read_including_the_block_0_tax_at_1096() {
        let mut g = vec![0u8; 1104];
        g[105..113].copy_from_slice(&95u64.to_le_bytes());
        g[154..162].copy_from_slice(&30u64.to_le_bytes());
        g[997..1005].copy_from_slice(&10u64.to_le_bytes());
        let f = decode_global_fees(&g).unwrap();
        assert_eq!((f.fee_bps, f.creator_fee_bps, f.buyback_bps, f.block0_tax_bps), (95, 30, 10, 0));
        assert!((f.pump_pct_per_side() - 1.25).abs() < 1e-12, "95 + 30 bps is the 1.25% per side the model uses");
        g[1096..1104].copy_from_slice(&500u64.to_le_bytes());
        let f = decode_global_fees(&g).unwrap();
        assert_eq!(f.block0_tax_bps, 500);
        assert!((f.block0_pct() - 5.0).abs() < 1e-12);
        // the 1096-byte account from before the upgrade: the new field reads as 0
        let old = decode_global_fees(&g[..1096]).unwrap();
        assert_eq!(old.block0_tax_bps, 0);
        assert!(decode_global_fees(&g[..100]).is_err());
    }

    #[test]
    fn ws_url_derivation_and_masking() {
        let r = Rpc::new("https://mainnet.helius-rpc.com/?api-key=SECRETKEY", 5.0);
        assert_eq!(r.ws_url(), "wss://mainnet.helius-rpc.com/?api-key=SECRETKEY");
        assert!(!r.masked_url().contains("SECRETKEY"));
    }

    #[tokio::test]
    async fn limiter_spaces_calls() {
        let l = Limiter::new(20.0); // 50 ms apart
        let t = Instant::now();
        for _ in 0..5 {
            l.acquire().await;
        }
        assert!(t.elapsed() >= Duration::from_millis(190), "{:?}", t.elapsed());
    }

    #[test]
    fn store_dedups_and_only_tracks_started_mints() {
        let s = TradeStore::default();
        let t = crate::pump::decode_trade_event(REAL_SELL_B64, 1, "sig1", 5).unwrap();
        s.add(t.clone()); // not started: dropped
        assert!(s.snapshot(&t.mint).is_empty());
        s.start(&t.mint);
        s.add(t.clone());
        s.add(t.clone()); // duplicate
        assert_eq!(s.snapshot(&t.mint).len(), 1);
        s.stop(&t.mint);
        assert_eq!(s.active(), 0);
    }

    fn notification(mint: &str, user: &str, is_buy: bool, sol: f64, v_sol: f64, v_tokens: f64, real_sol: f64, sig: &str) -> String {
        let b64 = crate::pump::tests::encode_trade_event(mint, user, is_buy, sol, sol * 1e5, v_sol, v_tokens, real_sol);
        json!({"jsonrpc":"2.0","method":"logsNotification","params":{"subscription":24040,"result":{"context":{"slot":300_000_001u64},"value":{"signature":sig,"err":null,"logs":[
            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]","Program log: Instruction: Buy", format!("Program data: {b64}"),"Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P success"]}}}}).to_string()
    }

    /// The Helius path end to end without a network: subscribe confirmation, then logsNotification messages shaped like the
    /// node sends them, through the SAME function the live feed task uses, into the trade store, then the picker.
    #[test]
    fn replayed_logs_notifications_reach_the_picker_and_a_busy_coin_scores_high() {
        use crate::picker;
        let cfg = crate::Config::from_json(include_str!("../../config.json")).unwrap();
        let mint = bs58::encode([3u8; 32]).into_string();
        let curve = bs58::encode([4u8; 32]).into_string();
        let creator = bs58::encode([5u8; 32]).into_string();
        let store = TradeStore::default();
        let stats = FeedStats::default();
        let mut sess = FeedSession { next_id: 100, ..Default::default() };
        let mut want = HashMap::new();
        // subscribe immediately at launch (what Trader::hold_feed does), before any Scam Shield result
        let feed = TradeFeed::offline();
        feed.subscribe(&mint, &curve);
        store.start(&mint);
        want.insert(mint.clone(), curve.clone());
        sess.pending.insert(101, (mint.clone(), Instant::now(), 0));
        let log = |_: String| {};
        on_ws_text(r#"{"jsonrpc":"2.0","result":24040,"id":101}"#, 1, &want, &store, &stats, &mut sess, &log);
        assert_eq!(stats.subs_active.load(std::sync::atomic::Ordering::Relaxed), 1, "the node's confirmation is counted");

        let t0 = 1_800_000_000_000i64;
        // 2 early buyers, 6 buyers in the last 30 s, price rising from the 30 SOL launch curve to 45.5 SOL
        let plan: [(i64, f64, f64, f64); 8] = [(10, 1.0, 31.0, 0.9), (20, 1.0, 32.0, 2.0), (36, 1.0, 36.0, 6.0), (40, 1.0, 38.0, 8.0), (44, 1.0, 40.0, 10.0), (48, 1.0, 42.0, 12.0), (52, 1.0, 44.0, 14.0), (56, 1.0, 45.5, 15.5)];
        for (i, (sec, sol, v_sol, real)) in plan.iter().enumerate() {
            let user = bs58::encode([10u8 + i as u8; 32]).into_string();
            let v_tokens = 1_073_000_000_000.0 / v_sol * 30.0 / 1000.0 * 0.65; // any positive curve; mcap checked below
            let msg = notification(&mint, &user, true, *sol, *v_sol, v_tokens, *real, &format!("sig{i}"));
            on_ws_text(&msg, t0 + sec * 1000, &want, &store, &stats, &mut sess, &log);
        }
        assert_eq!(stats.trades_total.load(std::sync::atomic::Ordering::Relaxed), 8);
        assert_eq!(stats.trades_within(t0 + 61_000, 60_000), 8);
        assert_eq!(stats.last_trade_ms.load(std::sync::atomic::Ordering::Relaxed), t0 + 56_000);
        let trades = store.snapshot(&mint);
        assert_eq!(trades.len(), 8);
        let launch_price = 30.0 / 1_073_000_000.0;
        let obs = picker::observe(&trades, &creator, 0.0, launch_price, 0.0, 61.0, t0 + 61_000);
        assert_eq!((obs.buys, obs.unique_buyers, obs.buyers_30s), (8, 8, 6), "{obs:?}");
        assert!((obs.curve_sol - 15.5).abs() < 1e-9);
        let sc = picker::score(&obs, &cfg);
        assert!(sc.picked, "score {} gates {:?}", sc.score, sc.gates);

        // the same market, but the coin was never subscribed: nothing is stored, the coin scores 0
        let other = bs58::encode([6u8; 32]).into_string();
        let msg = notification(&other, &bs58::encode([99u8; 32]).into_string(), true, 1.0, 40.0, 7.0e8, 10.0, "x");
        on_ws_text(&msg, t0, &want, &store, &stats, &mut sess, &log);
        assert!(store.snapshot(&other).is_empty());
        let empty = picker::observe(&[], &creator, 0.0, launch_price, 0.0, 61.0, t0 + 61_000);
        assert_eq!(picker::score(&empty, &cfg).score, 0.0);
    }

    #[test]
    fn a_failed_subscribe_is_recorded_not_silent() {
        let store = TradeStore::default();
        let stats = FeedStats::default();
        let mut sess = FeedSession { next_id: 100, ..Default::default() };
        sess.pending.insert(101, ("M".into(), Instant::now(), 0));
        let want = HashMap::new();
        on_ws_text(r#"{"jsonrpc":"2.0","error":{"code":-32602,"message":"too many subscriptions for api-key=SECRETKEY"},"id":101}"#, 1, &want, &store, &stats, &mut sess, &|_| {});
        assert!(sess.pending.is_empty());
        assert!(stats.last_error().contains("too many subscriptions") && !stats.last_error().contains("SECRETKEY"), "{}", stats.last_error());
        assert_eq!(stats.subs_active.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn top_holders_come_from_owners_not_token_accounts() {
        let largest = json!({"value":[{"address":"ATA1","amount":"60000000000000","decimals":6},{"address":"ATA2","amount":"20000000000000","decimals":6},{"address":"ATA3","amount":"5000000000000","decimals":6}]});
        let multi = json!({"value":[{"data":{"parsed":{"info":{"owner":"CURVE"}}}},{"data":{"parsed":{"info":{"owner":"WALLET2"}}}},null]});
        let h = parse_top_holders(&largest, &multi, 1.0e15).unwrap();
        assert_eq!(h[0], Holder { owner: "CURVE".into(), pct: 6.0, insider: false });
        assert_eq!(h[1].owner, "WALLET2");
        assert!((h[1].pct - 2.0).abs() < 1e-9);
        assert_eq!(h[2].owner, "ATA3", "an unreadable account is kept under its own address, not dropped");
        assert!(parse_top_holders(&largest, &multi, 0.0).is_err());
    }


    // ---- slot watchdog and backfill

    #[test]
    fn the_slot_watchdog_fires_when_slot_updates_stop_and_not_before() {
        let mut w = SlotWatch::started(0);
        assert_eq!(w.check(10_000), None, "a fresh connection gets time for its first slot update");
        assert!(w.check(SLOT_FIRST_WAIT_MS + 1).unwrap().contains("after connecting"), "never any slot update: stalled");
        w.on_slot(20_000);
        w.on_slot(20_400);
        assert_eq!(w.check(20_400 + SLOT_STALL_MS), None, "exactly at the limit is still fine");
        let why = w.check(20_400 + SLOT_STALL_MS + 1).unwrap();
        assert!(why.contains("no slot update for 6 s"), "{why}");
        w.on_slot(30_000);
        assert_eq!(w.check(30_100), None, "a new update clears it");
        assert_eq!(w.slots_seen, 3);
        // a node that refuses slotSubscribe turns the watchdog off instead of reconnecting forever
        let mut off = SlotWatch::started(0);
        off.disable();
        assert_eq!(off.check(1_000_000), None);
    }

    #[test]
    fn slot_messages_feed_the_watchdog_and_a_refused_slot_subscribe_is_not_a_trade_feed_error() {
        let store = TradeStore::default();
        let stats = FeedStats::default();
        let mut sess = FeedSession { next_id: 100, slots: SlotWatch::started(0), ..Default::default() };
        let want = HashMap::new();
        on_ws_text(r#"{"jsonrpc":"2.0","method":"slotNotification","params":{"result":{"slot":300000001,"parent":300000000,"root":299999969},"subscription":7}}"#, 5_000, &want, &store, &stats, &mut sess, &|_| {});
        assert_eq!(sess.slots.slots_seen, 1);
        assert_eq!(sess.slots.check(5_000 + SLOT_STALL_MS + 1).is_some(), true);
        on_ws_text(r#"{"jsonrpc":"2.0","result":7,"id":1}"#, 5_000, &want, &store, &stats, &mut sess, &|_| {});
        assert_eq!(stats.sub_acks.load(std::sync::atomic::Ordering::Relaxed), 0, "the slot subscription is not a coin subscription");
        let mut sess2 = FeedSession { next_id: 100, slots: SlotWatch::started(0), ..Default::default() };
        on_ws_text(r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#, 5_000, &want, &store, &stats, &mut sess2, &|_| {});
        assert_eq!(sess2.slots.check(1_000_000), None, "refused: watchdog off");
        assert_eq!(stats.sub_errors.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn backfill_plan_skips_failed_and_known_signatures_and_obeys_the_cap() {
        let sigs: Vec<(String, u64, bool)> = (0..20).map(|i| (format!("sig{i}"), 100 + i as u64, i == 2)).collect();
        let known: HashSet<String> = ["sig0".to_string(), "sig1".to_string()].into_iter().collect();
        let plan = plan_backfill(&sigs, &known, 5);
        assert_eq!(plan, vec!["sig3", "sig4", "sig5", "sig6", "sig7"], "newest first, sig2 failed, sig0 and sig1 already stored");
        assert!(plan_backfill(&sigs, &known, 0).is_empty());
        assert!(BACKFILL_TOTAL >= BACKFILL_PER_COIN && BACKFILL_TOTAL <= 100, "a reconnect can never run away with the rate budget");
    }

    fn tx_json(sig: &str, err: Value, logs: Vec<String>) -> Value {
        json!({"slot": 300_000_005u64, "blockTime": 1_795_000_000i64, "transaction": {"signatures": [sig]}, "meta": {"err": err, "logMessages": logs}})
    }

    #[test]
    fn missed_trades_are_read_from_a_transaction_with_the_same_top_of_stack_rule_and_land_once_in_the_store() {
        let mint = bs58::encode([3u8; 32]).into_string();
        let user = bs58::encode([8u8; 32]).into_string();
        let real = crate::pump::tests::encode_trade_event(&mint, &user, true, 1.0, 10.0, 31.0, 1.0e9, 1.0);
        let fake = crate::pump::tests::encode_trade_event(&mint, &user, true, 99.0, 10.0, 31.0, 1.0e9, 99.0);
        let pump = crate::pump::PUMP_PROGRAM;
        let tx = tx_json("sigX", Value::Null, vec![
            "Program Router1111 invoke [1]".into(), format!("Program data: {fake}"), format!("Program {pump} invoke [2]"), format!("Program data: {real}"), format!("Program {pump} success"), "Program Router1111 success".into(),
        ]);
        let t = trades_from_transaction(&tx, 5).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].slot, t[0].recv_ms, t[0].signature.as_str()), (300_000_005, 1_795_000_000_000, "sigX"));
        assert!((t[0].sol - 1.0).abs() < 1e-9);
        // a failed transaction and a missing one give nothing
        assert!(trades_from_transaction(&tx_json("f", json!({"InstructionError":[0,"x"]}), vec![format!("Program {pump} invoke [1]"), format!("Program data: {real}")]), 5).unwrap().is_empty());
        assert!(trades_from_transaction(&Value::Null, 5).is_err());
        // into the store: a live copy and the backfilled copy are one trade
        let store = TradeStore::default();
        store.start(&mint);
        store.add(t[0].clone());
        store.add(Trade { recv_ms: 9, ..t[0].clone() });
        assert_eq!(store.snapshot(&mint).len(), 1);
        assert!(store.known_signatures(&mint).contains("sigX"));
    }

    #[test]
    fn trades_accepted_by_the_store_are_recorded_once() {
        let dir = std::env::temp_dir().join(format!("ct-store-rec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rec = crate::recorder::Recorder::new(dir.clone(), crate::config::RecorderCfg::default());
        let store = TradeStore::default();
        store.set_recorder(rec.clone());
        let t = crate::pump::decode_trade_event(REAL_SELL_B64, 1, "sig1", 1_795_000_000_000).unwrap();
        store.add(t.clone()); // not watched: not recorded
        store.start(&t.mint);
        store.add(t.clone());
        store.add(t.clone()); // duplicate: not recorded twice
        assert_eq!(rec.flush(), 1);
        let back = crate::recorder::read_dir(&dir, "", "").unwrap();
        assert_eq!(back.trades[&t.mint], vec![t]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn logs_notification_decoding_skips_failed() {
        let v = json!({"method":"logsNotification","params":{"result":{"context":{"slot":9},"value":{"signature":"sg","err":null,"logs":["Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]","Program log: x", format!("Program data: {REAL_SELL_B64}"),"Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P success"]}}}});
        assert_eq!(trades_from_logs_notification(&v, 1).len(), 1);
        let failed = json!({"method":"logsNotification","params":{"result":{"context":{"slot":9},"value":{"signature":"sg","err":{"InstructionError":[0,"x"]},"logs":["Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]", format!("Program data: {REAL_SELL_B64}")]}}}});
        assert!(trades_from_logs_notification(&failed, 1).is_empty());
    }
}
