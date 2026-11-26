//! Solana RPC access (HTTP JSON-RPC with a rate limiter) and the free trade feed.
//!
//! PumpPortal's `subscribeTokenTrade` needs an API key funded with 0.02 SOL (verified live 2026-11-26), so it is
//! NOT used. Instead one RPC WebSocket carries `logsSubscribe {mentions:[bondingCurve]}` per observed token and
//! the pump.fun TradeEvent in the logs is decoded (buy/sell, wallet, SOL, reserves, slot). Free, real time.
//! The endpoint is configurable (public mainnet-beta here, Helius later). Keys are masked in every log line.

use crate::net::{connect_wss, http_client, mask};
use crate::pump::{trades_from_logs_notification, Trade};
use crate::shield::rules::{parse_mint_account, MintInfo};
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
    url: String,
    http: reqwest::Client,
    limiter: Limiter,
}

impl Rpc {
    pub fn new(url: &str, per_sec: f64) -> Self {
        Rpc { url: url.to_string(), http: http_client(), limiter: Limiter::new(per_sec) }
    }
    /// URL with keys hidden. Always use this in logs.
    pub fn masked_url(&self) -> String {
        mask(&self.url)
    }
    pub fn ws_url(&self) -> String {
        self.url.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.limiter.acquire().await;
        let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let resp = self.http.post(&self.url).json(&body).send().await.map_err(|e| mask(&format!("rpc send: {e}")))?;
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

// ------------------------------------------------------------------ trade store + log feed

#[derive(Default)]
pub struct TradeStore {
    /// mint -> (trades, dedup keys). Only mints that were `start`ed are tracked; everything is freed on `stop`.
    inner: Mutex<HashMap<String, (Vec<Trade>, HashSet<String>)>>,
}

impl TradeStore {
    pub fn add(&self, t: Trade) {
        let key = format!("{}:{}:{}", t.signature, t.user, t.is_buy);
        let mut g = self.inner.lock().unwrap();
        if let Some((v, seen)) = g.get_mut(&t.mint) {
            if seen.insert(key) {
                v.push(t);
            }
        }
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

enum FeedCmd {
    Sub { mint: String, curve: String },
    Unsub { mint: String },
}

#[derive(Clone)]
pub struct TradeFeed {
    tx: mpsc::UnboundedSender<FeedCmd>,
    pub store: Arc<TradeStore>,
    pub connected: Arc<std::sync::atomic::AtomicBool>,
}

impl TradeFeed {
    /// Start the background task. `ws_url` is the RPC's WebSocket URL.
    pub fn spawn(ws_url: String, log: Arc<dyn Fn(String) + Send + Sync>) -> TradeFeed {
        let (tx, rx) = mpsc::unbounded_channel();
        let store = Arc::new(TradeStore::default());
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        tokio::spawn(feed_task(ws_url, rx, store.clone(), connected.clone(), log));
        TradeFeed { tx, store, connected }
    }
    pub fn subscribe(&self, mint: &str, curve: &str) {
        self.store.start(mint);
        let _ = self.tx.send(FeedCmd::Sub { mint: mint.into(), curve: curve.into() });
    }
    pub fn unsubscribe(&self, mint: &str) {
        self.store.stop(mint);
        let _ = self.tx.send(FeedCmd::Unsub { mint: mint.into() });
    }
}

async fn feed_task(
    ws_url: String,
    mut rx: mpsc::UnboundedReceiver<FeedCmd>,
    store: Arc<TradeStore>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    log: Arc<dyn Fn(String) + Send + Sync>,
) {
    use std::sync::atomic::Ordering;
    let mut want: HashMap<String, String> = HashMap::new(); // mint -> curve
    let mut backoff = 1u64;
    loop {
        let mut ws = match connect_wss(&ws_url).await {
            Ok(w) => w,
            Err(e) => {
                log(format!("trade feed: connect failed ({}), retry in {backoff}s", mask(&e)));
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
        log("trade feed: connected".into());
        let mut next_id: u64 = 100;
        let mut pending: HashMap<u64, String> = HashMap::new(); // request id -> mint
        let mut subs: HashMap<String, u64> = HashMap::new(); // mint -> subscription id
        for (mint, curve) in want.clone() {
            next_id += 1;
            pending.insert(next_id, mint);
            let _ = ws.send(Message::Text(sub_msg(next_id, &curve))).await;
        }
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        let (mut n_notif, mut n_trades, mut n_acks) = (0u64, 0u64, 0u64);
        let end: String = loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { return };
                    match cmd {
                        FeedCmd::Sub { mint, curve } => {
                            want.insert(mint.clone(), curve.clone());
                            next_id += 1;
                            pending.insert(next_id, mint);
                            if ws.send(Message::Text(sub_msg(next_id, &curve))).await.is_err() { break "send failed".into(); }
                        }
                        FeedCmd::Unsub { mint } => {
                            want.remove(&mint);
                            if let Some(id) = subs.remove(&mint) {
                                next_id += 1;
                                let m = json!({"jsonrpc":"2.0","id":next_id,"method":"logsUnsubscribe","params":[id]}).to_string();
                                let _ = ws.send(Message::Text(m)).await;
                            }
                        }
                    }
                }
                _ = ping.tick() => {
                    if ws.send(Message::Ping(vec![])).await.is_err() { break "ping failed".into(); }
                    log(format!("trade feed: {n_acks} subscriptions confirmed, {n_notif} log messages, {n_trades} trades decoded"));
                }
                m = ws.next() => {
                    match m {
                        Some(Ok(Message::Text(t))) => {
                            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                            if v["method"] == "logsNotification" {
                                n_notif += 1;
                                for tr in trades_from_logs_notification(&v, now_ms()) { n_trades += 1; store.add(tr); }
                            } else if let (Some(id), Some(sub)) = (v["id"].as_u64(), v["result"].as_u64()) {
                                n_acks += 1;
                                if let Some(mint) = pending.remove(&id) {
                                    if want.contains_key(&mint) { subs.insert(mint, sub); }
                                }
                            } else if v["error"].is_object() {
                                log(format!("trade feed: rpc said {}", mask(&v["error"].to_string())));
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break "closed by server".into(),
                        Some(Err(e)) => break mask(&e.to_string()),
                        _ => {}
                    }
                }
            }
        };
        connected.store(false, Ordering::Relaxed);
        log(format!("trade feed: disconnected ({end}), reconnecting in {backoff}s"));
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
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

    #[test]
    fn logs_notification_decoding_skips_failed() {
        let v = json!({"method":"logsNotification","params":{"result":{"context":{"slot":9},"value":{"signature":"sg","err":null,"logs":["Program log: x", format!("Program data: {REAL_SELL_B64}")]}}}});
        assert_eq!(trades_from_logs_notification(&v, 1).len(), 1);
        let failed = json!({"method":"logsNotification","params":{"result":{"context":{"slot":9},"value":{"signature":"sg","err":{"InstructionError":[0,"x"]},"logs":[format!("Program data: {REAL_SELL_B64}")]}}}});
        assert!(trades_from_logs_notification(&failed, 1).is_empty());
    }
}
