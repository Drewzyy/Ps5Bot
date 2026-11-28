//! Local bridge to the Chrome extension (SPEC 4d). A WebSocket on 127.0.0.1 that only the pinned extension may use.
//!
//! Rules, all enforced here and tested below:
//! 1. The handshake is refused unless the `Origin` header is exactly `chrome-extension://<pinned id>`. Web pages cannot
//!    forge that header, so a website can never connect.
//! 2. Even the right origin must pair first: a one-time pairing code (shown in the app, valid 10 minutes, one use) is swapped
//!    for a random token. Only a hash of the token is stored. Without a valid code or token nothing is sent.
//! 3. The extension gets the same public events the dashboard shows (no Helius key, no secrets). It can ask for the verdict
//!    on a coin, ask for a scam check, and press the kill switch or resume. There is NO message that buys or sells.
//!    "Check this coin" runs the Scam Shield checks only: this module has no handle to the trader.
//! 4. X Monitor (SPEC 4e): the extension may ask for its next x.com page (`xnext`), report how it is doing (`xstatus`)
//!    and return what it read (`xresult`). The page address is built by the app from checked parts. These messages only
//!    carry reading jobs and reading results; none of them can trade, and none of them can make the extension write to X.

use crate::db::Db;
use crate::events::EngineEvent;
use crate::hub::Hub;
use crate::rpc::now_ms;
use crate::shield::{decide, Shield};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

/// Chrome extension ID that comes from the public key pinned in extension/manifest.json ("key").
pub const EXTENSION_ID: &str = "jihnmdnpkeglfhoombfjhdfbmiopnljj";
pub const DEFAULT_PORT: u16 = 8766;
const PAIR_TTL_MS: i64 = 10 * 60 * 1000;
const CHECK_COOLDOWN_MS: i64 = 30_000;
const BOOK_MAX: usize = 8000;

pub fn extension_origin() -> String {
    format!("chrome-extension://{EXTENSION_ID}")
}

/// The only control messages the extension may send on to the engine.
pub const ALLOWED_COMMANDS: [&str; 2] = ["kill", "resume"];

// ------------------------------------------------------------------ pairing (all state lives in SQLite kv, so the app and engine-cli share it)

fn sha_hex(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn norm_code(c: &str) -> String {
    c.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

/// New one-time pairing code like "4821-9037". Replaces any earlier unused code. The code itself is never stored, only its hash.
pub fn new_pair_code(db: &Db) -> Result<String, String> {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).map_err(|e| format!("random: {e}"))?;
    let n = u64::from_le_bytes(b) % 100_000_000;
    let code = format!("{:04}-{:04}", n / 10_000, n % 10_000);
    db.kv_set("ext.pair", &format!("{}|{}", sha_hex(&norm_code(&code)), now_ms() + PAIR_TTL_MS));
    Ok(code)
}

pub fn is_paired(db: &Db) -> bool {
    db.kv_get("ext.token").is_some()
}

/// Forget the paired extension (its token stops working) and any unused code.
pub fn unpair(db: &Db) {
    db.kv_del("ext.token");
    db.kv_del("ext.pair");
}

/// Swap a valid, unused, unexpired code for a new token. The code is consumed even if it was right.
fn redeem_code(db: &Db, code: &str) -> Option<String> {
    let stored = db.kv_get("ext.pair")?;
    let (hash, exp) = stored.split_once('|')?;
    let exp: i64 = exp.parse().ok()?;
    let ok = now_ms() <= exp && ct_eq(hash, &sha_hex(&norm_code(code)));
    if !ok {
        return None;
    }
    db.kv_del("ext.pair");
    let mut raw = [0u8; 32];
    getrandom::getrandom(&mut raw).ok()?;
    let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    db.kv_set("ext.token", &sha_hex(&token));
    Some(token)
}

fn token_ok(db: &Db, token: &str) -> bool {
    match db.kv_get("ext.token") {
        Some(h) => !token.is_empty() && ct_eq(&h, &sha_hex(token)),
        None => false,
    }
}

/// 5 wrong attempts in a row lock sign-in for 60 s (a local program cannot brute-force the code).
#[derive(Default)]
struct Guard {
    state: Mutex<(u32, i64)>,
}

impl Guard {
    fn locked(&self) -> bool {
        self.state.lock().unwrap().1 > now_ms()
    }
    fn fail(&self) {
        let mut s = self.state.lock().unwrap();
        s.0 += 1;
        if s.0 >= 5 {
            *s = (0, now_ms() + 60_000);
        }
    }
    fn ok(&self) {
        *self.state.lock().unwrap() = (0, 0);
    }
}

// ------------------------------------------------------------------ what the bot thinks of a coin

#[derive(Debug, Clone, Default)]
struct Info {
    symbol: String,
    shield: String,
    stage: String,
    reason: String,
    detail: String,
    level: String,
    score: Option<f64>,
    min_score: f64,
    picked: bool,
    gates: Vec<String>,
}

#[derive(Default)]
pub struct MintBook {
    map: HashMap<String, Info>,
    held: HashSet<String>,
    closed: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoinVerdict {
    pub mint: String,
    pub symbol: String,
    /// held, watching, skipped, blocked, closed or unknown
    pub status: String,
    /// pass, block or none (Scam Shield)
    pub shield: String,
    /// LOW_SIGNAL, ELEVATED or HIGH, or empty
    pub shield_level: String,
    pub score: Option<f64>,
    pub min_score: Option<f64>,
    pub reason: String,
    /// X Monitor's view of the coin (score, flags, blocks), when the bot knows the coin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<crate::xmon::score::XEval>,
}

impl MintBook {
    pub fn update(&mut self, e: &EngineEvent) {
        if self.map.len() > BOOK_MAX {
            self.map.clear();
        }
        match e {
            EngineEvent::Token { mint, symbol, .. } => {
                self.map.entry(mint.clone()).or_default().symbol = symbol.clone();
            }
            EngineEvent::Verdict { mint, symbol, stage, outcome, reason, detail, verdict, .. } => {
                let i = self.map.entry(mint.clone()).or_default();
                if i.symbol.is_empty() {
                    i.symbol = symbol.clone();
                }
                // a block stays a block; a pass never overwrites it
                if i.shield == "block" && outcome == "pass" {
                    return;
                }
                i.shield = outcome.clone();
                i.stage = stage.clone();
                i.reason = reason.clone();
                i.detail = detail.clone();
                i.level = verdict.clone();
            }
            EngineEvent::Score { mint, symbol, score, min_score, picked, gates, .. } => {
                let i = self.map.entry(mint.clone()).or_default();
                if i.symbol.is_empty() {
                    i.symbol = symbol.clone();
                }
                i.score = Some(*score);
                i.min_score = *min_score;
                i.picked = *picked;
                i.gates = gates.clone();
            }
            EngineEvent::Positions { positions, .. } => {
                self.held = positions.iter().filter(|p| p.state != "CLOSED").map(|p| p.mint.clone()).collect();
                for p in positions.iter().filter(|p| p.state == "CLOSED") {
                    self.closed.insert(p.mint.clone());
                }
            }
            _ => {}
        }
    }

    pub fn verdict(&self, mint: &str, db: &Db) -> CoinVerdict {
        let mut info = self.map.get(mint).cloned();
        if info.as_ref().map(|i| i.shield.is_empty()).unwrap_or(true) {
            // after a restart the in-memory book is empty: the decisions table remembers
            if let Some((symbol, stage, outcome, reason, detail, level)) = db.latest_decision(mint) {
                let mut i = info.unwrap_or_default();
                i.symbol = if i.symbol.is_empty() { symbol } else { i.symbol };
                i.shield = outcome;
                i.stage = stage;
                i.reason = reason;
                i.detail = detail;
                i.level = level;
                info = Some(i);
            }
        }
        let held = self.held.contains(mint) || db.mint_is_held(mint);
        let traded = self.closed.contains(mint) || db.mint_has_position(mint);
        let i = info.unwrap_or_default();
        let symbol = if i.symbol.is_empty() { format!("{}...", mint.chars().take(4).collect::<String>()) } else { i.symbol.clone() };
        let (status, reason) = if held {
            ("held", "The bot holds this coin right now.".to_string())
        } else if i.shield == "block" {
            ("blocked", if i.detail.is_empty() { i.reason.clone() } else { i.detail.clone() })
        } else if traded {
            ("closed", "The bot traded this coin and the trade is closed.".to_string())
        } else if let Some(s) = i.score {
            if i.picked {
                ("watching", format!("Picked with score {s:.0}. The bot is about to buy by its own rules."))
            } else if let Some(g) = i.gates.first() {
                ("skipped", format!("Not bought. Score {s:.0} (needs {:.0}). {g}", i.min_score))
            } else {
                ("skipped", format!("Not bought. Score {s:.0} is below the {:.0} the bot needs.", i.min_score))
            }
        } else if i.stage == "manual-check" && i.shield == "pass" {
            ("skipped", "Passed the scam check. The bot did not pick it, and it only buys by its own rules.".to_string())
        } else if i.shield == "pass" {
            ("watching", "Passed Scam Shield. The bot is watching it.".to_string())
        } else {
            ("unknown", "The bot has not seen this coin. Press Check this coin to run a scam check.".to_string())
        };
        CoinVerdict {
            mint: mint.into(),
            symbol,
            status: status.into(),
            shield: if i.shield.is_empty() { "none".into() } else { i.shield.clone() },
            shield_level: i.level.clone(),
            score: i.score,
            min_score: i.score.map(|_| i.min_score),
            reason,
            x: None,
        }
    }

    /// Record the answer of a manual check so the next lookup shows it at once.
    pub fn apply_check(&mut self, mint: &str, a: &CheckAnswer) {
        let i = self.map.entry(mint.to_string()).or_default();
        if i.symbol.is_empty() {
            i.symbol = a.symbol.clone();
        }
        i.shield = a.outcome.clone();
        i.stage = "manual-check".into();
        i.reason = a.reason.clone();
        i.detail = a.detail.clone();
        i.level = a.level.clone();
    }
}

/// Solana addresses are 32 to 44 base58 characters. Anything else is refused before it reaches any check.
pub fn valid_mint(s: &str) -> bool {
    const ALPHA: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    (32..=44).contains(&s.len()) && s.chars().all(|c| ALPHA.contains(c))
}

// ------------------------------------------------------------------ the scam check (checks only, never a trade)

#[derive(Debug, Clone)]
pub struct CheckAnswer {
    pub symbol: String,
    pub outcome: String,
    pub reason: String,
    pub detail: String,
    pub level: String,
}

#[async_trait]
pub trait Checker: Send + Sync {
    async fn check(&self, mint: &str) -> CheckAnswer;
}

/// The real checker: Scam Shield checks for one coin, saved as a decision and shown in the activity feed.
/// It holds a Shield, the hub and the database. It has no trader, so it cannot open a position.
pub struct ShieldChecker {
    pub shield: Arc<Shield>,
    pub hub: Arc<Hub>,
    pub db: Arc<Db>,
}

#[async_trait]
impl Checker for ShieldChecker {
    async fn check(&self, mint: &str) -> CheckAnswer {
        let d = decide(self.shield.manual_check(mint).await);
        let (outcome, reason, detail) = match &d.block {
            Some(b) => ("block", b.code.clone(), b.detail.clone()),
            None => ("pass", String::new(), format!("{} checks done, none blocked.", d.results.len())),
        };
        let symbol = self.db.latest_decision(mint).map(|x| x.0).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{}...", mint.chars().take(4).collect::<String>()));
        let now = now_ms();
        self.db.insert_decision(&crate::db::DecisionRow { ts: now, mint: mint.into(), symbol: symbol.clone(), stage: "manual-check".into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into() });
        self.hub.emit(EngineEvent::Verdict { ts: now, mint: mint.into(), symbol: symbol.clone(), stage: "manual-check".into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into(), evidence: vec![] });
        self.hub.emit(EngineEvent::Activity {
            ts: now,
            agent: crate::events::AgentId::ScamShield,
            text: if outcome == "block" { format!("Scam check you asked for on {symbol}: blocked ({reason}). {detail}") } else { format!("Scam check you asked for on {symbol}: passed ({}). This does not buy anything.", d.verdict) },
            mint: Some(mint.into()),
            source_url: None,
        });
        CheckAnswer { symbol, outcome: outcome.into(), reason, detail, level: d.verdict.into() }
    }
}

// ------------------------------------------------------------------ the server

pub struct Bridge {
    hub: Arc<Hub>,
    db: Arc<Db>,
    book: Mutex<MintBook>,
    checker: Arc<dyn Checker>,
    /// Origins allowed to connect. Production: only the pinned extension. Tests may add one.
    origins: Vec<String>,
    guard: Guard,
    clients: AtomicU32,
    last_seen: AtomicI64,
    last_check: Mutex<HashMap<String, i64>>,
    xmon: std::sync::OnceLock<Arc<crate::xmon::XMon>>,
}

impl Bridge {
    pub fn new(hub: Arc<Hub>, db: Arc<Db>, checker: Arc<dyn Checker>) -> Arc<Bridge> {
        Arc::new(Bridge { hub, db, book: Mutex::new(MintBook::default()), checker, origins: vec![extension_origin()], guard: Guard::default(), clients: AtomicU32::new(0), last_seen: AtomicI64::new(0), last_check: Mutex::new(HashMap::new()), xmon: std::sync::OnceLock::new() })
    }

    #[cfg(test)]
    fn with_origins(mut self: Arc<Self>, o: Vec<String>) -> Arc<Self> {
        Arc::get_mut(&mut self).unwrap().origins = o;
        self
    }

    pub fn connected(&self) -> bool {
        self.clients.load(Ordering::Relaxed) > 0
    }

    /// Plug in the X Monitor (job queue and results). Without it the X messages are answered with "nothing to do".
    pub fn set_xmon(&self, x: Arc<crate::xmon::XMon>) {
        let _ = self.xmon.set(x);
    }

    pub fn verdict(&self, mint: &str) -> CoinVerdict {
        let mut v = self.book.lock().unwrap().verdict(mint, &self.db);
        if let Some(x) = self.xmon.get() {
            if v.status != "unknown" || x.knows(mint) {
                v.x = Some(x.evaluate(mint, now_ms()));
            }
        }
        v
    }

    /// Keep the coin book up to date from the hub and publish the link status once a second.
    fn spawn_background(self: &Arc<Self>) {
        let me = self.clone();
        let mut rx = self.hub.subscribe();
        for e in self.hub.snapshot() {
            self.book.lock().unwrap().update(&e);
        }
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(e) => me.book.lock().unwrap().update(&e),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                }
            }
        });
        let me = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(1));
            loop {
                t.tick().await;
                me.hub.emit(EngineEvent::Extension { ts: now_ms(), paired: is_paired(&me.db), connected: me.connected(), last_seen_ms: me.last_seen.load(Ordering::Relaxed) });
            }
        });
    }

    fn forward(e: &EngineEvent) -> Option<String> {
        let mut v = match e {
            EngineEvent::Activity { .. } | EngineEvent::Positions { .. } | EngineEvent::Stats { .. } | EngineEvent::Kill(_) | EngineEvent::Learning(_) => serde_json::to_value(e).ok()?,
            // the side panel only needs one line and a few counts
            EngineEvent::XMonitor(x) => json!({"type": "x-monitor", "state": x.state, "statusLine": x.status_line, "checkedHour": x.checked_hour, "risingCount": x.rising.len(), "loadsLastHour": x.loads_last_hour, "maxPerHour": x.max_per_hour, "loggedIn": x.logged_in, "pausedReason": x.paused_reason}),
            _ => return None,
        };
        if let Some(ps) = v.get_mut("positions").and_then(|p| p.as_array_mut()) {
            for p in ps {
                if let Some(o) = p.as_object_mut() {
                    o.remove("ticks"); // candle data: the side panel does not draw charts
                }
            }
        }
        Some(json!({"t": "event", "e": v}).to_string())
    }

    pub async fn serve(self: Arc<Self>, addr: &str) -> Result<std::net::SocketAddr, String> {
        let l = TcpListener::bind(addr).await.map_err(|e| format!("bridge bind {addr}: {e}"))?;
        let local = l.local_addr().map_err(|e| e.to_string())?;
        self.spawn_background();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = l.accept().await else { continue };
                let me = self.clone();
                tokio::spawn(async move { me.client(sock).await });
            }
        });
        Ok(local)
    }

    async fn client(self: Arc<Self>, sock: tokio::net::TcpStream) {
        let origins = self.origins.clone();
        let check = move |req: &tokio_tungstenite::tungstenite::handshake::server::Request, resp| {
            let ok = req.headers().get("origin").and_then(|v| v.to_str().ok()).map(|o| origins.iter().any(|a| a == o)).unwrap_or(false);
            if ok {
                Ok(resp)
            } else {
                Err(tokio_tungstenite::tungstenite::http::Response::builder().status(403).body(Some("origin not allowed".to_string())).unwrap())
            }
        };
        let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(sock, check).await else { return };

        // 1. sign in: a token, or a pairing code. Nothing else is answered before this.
        let first = match tokio::time::timeout(Duration::from_secs(8), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<Value>(&t).unwrap_or(Value::Null),
            _ => return,
        };
        let deny = |why: &str| json!({"t": "auth", "ok": false, "error": why}).to_string();
        if self.guard.locked() {
            let _ = ws.send(Message::Text(deny("Too many wrong tries. Wait a minute and try again."))).await;
            return;
        }
        match first["t"].as_str().unwrap_or("") {
            "auth" => {
                if token_ok(&self.db, first["token"].as_str().unwrap_or("")) {
                    self.guard.ok();
                    let _ = ws.send(Message::Text(json!({"t": "auth", "ok": true}).to_string())).await;
                } else {
                    self.guard.fail();
                    let _ = ws.send(Message::Text(deny("Not paired. Get a new code in the Claude Trader app."))).await;
                    return;
                }
            }
            "pair" => match redeem_code(&self.db, first["code"].as_str().unwrap_or("")) {
                Some(token) => {
                    self.guard.ok();
                    let _ = ws.send(Message::Text(json!({"t": "paired", "token": token}).to_string())).await;
                }
                None => {
                    self.guard.fail();
                    let _ = ws.send(Message::Text(deny("That code is wrong or has expired. Get a new one in the app."))).await;
                    return;
                }
            },
            _ => {
                self.guard.fail();
                let _ = ws.send(Message::Text(deny("Pair first."))).await;
                return;
            }
        }

        // 2. signed in
        self.clients.fetch_add(1, Ordering::Relaxed);
        self.last_seen.store(now_ms(), Ordering::Relaxed);
        self.run(ws).await;
        self.clients.fetch_sub(1, Ordering::Relaxed);
        self.last_seen.store(now_ms(), Ordering::Relaxed);
    }

    async fn run<S>(self: &Arc<Self>, mut ws: tokio_tungstenite::WebSocketStream<S>)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let mut rx = self.hub.subscribe();
        let settings_text = || {
            let mut v = json!({"t": "settings", "startup": crate::startup::StartupSettings::load(&self.db)});
            if let Some(x) = self.xmon.get() {
                v["xmonitor"] = x.settings_for_extension();
            }
            v.to_string()
        };
        let mut last_settings = settings_text();
        if ws.send(Message::Text(json!({"t": "hello", "v": 1}).to_string())).await.is_err() || ws.send(Message::Text(last_settings.clone())).await.is_err() {
            return;
        }
        for e in self.hub.snapshot() {
            if let Some(m) = Self::forward(&e) {
                if ws.send(Message::Text(m)).await.is_err() {
                    return;
                }
            }
        }
        let (otx, mut orx) = mpsc::unbounded_channel::<String>();
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        let mut beat = 0u32;
        loop {
            tokio::select! {
                e = rx.recv() => match e {
                    Ok(e) => if let Some(m) = Self::forward(&e) { if ws.send(Message::Text(m)).await.is_err() { return; } },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                },
                Some(m) = orx.recv() => { if ws.send(Message::Text(m)).await.is_err() { return; } }
                _ = tick.tick() => {
                    self.last_seen.store(now_ms(), Ordering::Relaxed);
                    let s = settings_text();
                    if s != last_settings {
                        last_settings = s.clone();
                        if ws.send(Message::Text(s)).await.is_err() { return; }
                    }
                    beat += 1;
                    if beat % 10 == 0 && ws.send(Message::Text(json!({"t": "ping"}).to_string())).await.is_err() { return; }
                    // a token that was revoked (Unpair in the app) ends the session
                    if !is_paired(&self.db) { return; }
                }
                m = ws.next() => match m {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                    Some(Ok(Message::Text(t))) => {
                        self.last_seen.store(now_ms(), Ordering::Relaxed);
                        let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                        self.request(v, otx.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    /// One request from the extension. Everything it can ask for is listed here.
    fn request(self: &Arc<Self>, v: Value, out: mpsc::UnboundedSender<String>) {
        let id = v["id"].clone();
        let reply = move |body: Value| {
            let mut b = body;
            b["id"] = id.clone();
            let _ = out.send(b.to_string());
        };
        match v["t"].as_str().unwrap_or("") {
            "pong" => {}
            "verdict" => {
                let mint = v["mint"].as_str().unwrap_or("");
                if !valid_mint(mint) {
                    return reply(json!({"t": "error", "error": "That is not a coin address."}));
                }
                reply(json!({"t": "verdict", "verdict": self.verdict(mint)}));
            }
            "check" => {
                let mint = v["mint"].as_str().unwrap_or("").to_string();
                if !valid_mint(&mint) {
                    return reply(json!({"t": "error", "error": "That is not a coin address."}));
                }
                let now = now_ms();
                {
                    let mut lc = self.last_check.lock().unwrap();
                    if lc.len() > 500 {
                        lc.clear();
                    }
                    if lc.get(&mint).map(|t| now - t < CHECK_COOLDOWN_MS).unwrap_or(false) {
                        return reply(json!({"t": "check", "verdict": self.verdict(&mint), "note": "Checked a moment ago."}));
                    }
                    lc.insert(mint.clone(), now);
                }
                let me = self.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(30), me.checker.check(&mint)).await {
                        Ok(a) => {
                            me.book.lock().unwrap().apply_check(&mint, &a);
                            reply(json!({"t": "check", "verdict": me.verdict(&mint)}));
                        }
                        Err(_) => reply(json!({"t": "error", "error": "The check took too long. Try again."})),
                    }
                });
            }
            // ---- X Monitor: reading jobs and reading results only
            "xnext" => {
                let Some(x) = self.xmon.get() else { return reply(json!({"t": "xjob", "job": null})) };
                let now = now_ms();
                x.set_ext_status(serde_json::from_value(v.clone()).unwrap_or_default(), now);
                reply(json!({"t": "xjob", "job": x.next_job(now)}));
            }
            "xstatus" => {
                let Some(x) = self.xmon.get() else { return reply(json!({"t": "ok"})) };
                x.set_ext_status(serde_json::from_value(v.clone()).unwrap_or_default(), now_ms());
                reply(json!({"t": "ok"}));
            }
            "xresult" => {
                let Some(x) = self.xmon.get() else { return reply(json!({"t": "ok"})) };
                match serde_json::from_value::<crate::xmon::model::XResult>(v.clone()) {
                    Ok(r) => match x.submit(r, now_ms()) {
                        Ok(()) => reply(json!({"t": "ok"})),
                        Err(e) => reply(json!({"t": "error", "error": e})),
                    },
                    Err(_) => reply(json!({"t": "error", "error": "That result could not be read."})),
                }
            }
            "cmd" => {
                let cmd = v["cmd"].as_str().unwrap_or("");
                if !ALLOWED_COMMANDS.contains(&cmd) {
                    return reply(json!({"t": "error", "error": "That command is not allowed."}));
                }
                let ans = self.hub.command(cmd);
                self.hub.log(format!("extension command {cmd}: {ans}"));
                reply(json!({"t": "cmd", "cmd": cmd, "answer": ans}));
            }
            _ => reply(json!({"t": "error", "error": "Unknown request."})),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    const MINT: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";

    struct Fake(Mutex<Vec<String>>);
    #[async_trait]
    impl Checker for Fake {
        async fn check(&self, mint: &str) -> CheckAnswer {
            self.0.lock().unwrap().push(mint.into());
            CheckAnswer { symbol: "FAKE".into(), outcome: "block".into(), reason: "CREATOR_HOLDING".into(), detail: "Creator holds 40%".into(), level: "HIGH".into() }
        }
    }

    async fn start() -> (Arc<Bridge>, Arc<Hub>, Arc<Db>, Arc<Fake>, std::net::SocketAddr, Arc<Mutex<Vec<String>>>) {
        let hub = Hub::new();
        let cmds: Arc<Mutex<Vec<String>>> = Default::default();
        let c2 = cmds.clone();
        hub.set_command_handler(Arc::new(move |c| {
            c2.lock().unwrap().push(c.to_string());
            "ok".into()
        }));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let fake = Arc::new(Fake(Default::default()));
        let b = Bridge::new(hub.clone(), db.clone(), fake.clone());
        let addr = b.clone().serve("127.0.0.1:0").await.unwrap();
        (b, hub, db, fake, addr, cmds)
    }

    type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn connect(addr: std::net::SocketAddr, origin: Option<&str>) -> Result<Ws, String> {
        let mut req = format!("ws://{addr}").into_client_request().unwrap();
        if let Some(o) = origin {
            req.headers_mut().insert("origin", o.parse().unwrap());
        }
        tokio_tungstenite::connect_async(req).await.map(|r| r.0).map_err(|e| e.to_string())
    }

    async fn say(ws: &mut Ws, v: Value) {
        ws.send(Message::Text(v.to_string())).await.unwrap();
    }

    /// Next frame of type `t`, skipping events.
    async fn next_t(ws: &mut Ws, t: &str) -> Value {
        loop {
            let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.expect("timeout").expect("closed").expect("err");
            if let Message::Text(s) = m {
                let v: Value = serde_json::from_str(&s).unwrap();
                if v["t"] == t {
                    return v;
                }
            }
        }
    }

    async fn paired_client(addr: std::net::SocketAddr, db: &Db) -> (Ws, String) {
        let code = new_pair_code(db).unwrap();
        let mut ws = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut ws, json!({"t": "pair", "code": code})).await;
        let p = next_t(&mut ws, "paired").await;
        (ws, p["token"].as_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn only_the_pinned_extension_origin_can_connect() {
        let (_b, _h, _d, _f, addr, _) = start().await;
        for bad in [Some("https://pump.fun"), Some("https://evil.example"), Some("http://127.0.0.1:1420"), Some("http://localhost"), Some("chrome-extension://abcdefghijklmnopabcdefghijklmnop"), Some("chrome-extension://jihnmdnpkeglfhoombfjhdfbmiopnljjx"), Some("null"), None] {
            let r = connect(addr, bad).await;
            assert!(r.is_err(), "origin {bad:?} must be refused");
        }
        assert!(connect(addr, Some(&extension_origin())).await.is_ok());
    }

    #[test]
    fn the_pinned_id_matches_the_manifest_key() {
        // extension/manifest.json holds the public key; its SHA-256 (first 16 bytes, hex mapped to a-p) is the ID
        let m: Value = serde_json::from_str(include_str!("../../extension/manifest.json")).unwrap();
        use base64::Engine;
        let der = base64::engine::general_purpose::STANDARD.decode(m["key"].as_str().unwrap()).unwrap();
        let id: String = Sha256::digest(&der).iter().take(16).flat_map(|b| [b >> 4, b & 15]).map(|n| (b'a' + n) as char).collect();
        assert_eq!(id, EXTENSION_ID);
    }

    #[tokio::test]
    async fn right_origin_without_pairing_gets_nothing() {
        let (_b, hub, db, _f, addr, _) = start().await;
        // a request before signing in, no token, a made-up token, a wrong code: all refused, no data sent
        for first in [json!({"t": "verdict", "mint": MINT, "id": 1}), json!({"t": "auth", "token": ""}), json!({"t": "auth", "token": "deadbeef"}), json!({"t": "pair", "code": "0000-0000"})] {
            let mut ws = connect(addr, Some(&extension_origin())).await.unwrap();
            say(&mut ws, first).await;
            let a = next_t(&mut ws, "auth").await;
            assert_eq!(a["ok"], false);
            hub.emit(EngineEvent::Activity { ts: 1, agent: crate::events::AgentId::ScamShield, text: "secret-ish".into(), mint: None, source_url: None });
            // nothing but a close may follow: no event text ever arrives on an unsigned connection
            loop {
                match tokio::time::timeout(Duration::from_millis(300), ws.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => panic!("unsigned connection received data: {t}"),
                    Ok(Some(Ok(_))) => continue,
                    _ => break,
                }
            }
        }
        assert!(!is_paired(&db));
    }

    #[tokio::test]
    async fn pairing_code_works_once_then_the_token_signs_in() {
        let (b, _h, db, _f, addr, _) = start().await;
        let code = new_pair_code(&db).unwrap();
        assert_eq!(code.len(), 9);
        // stored as a hash only
        assert!(!db.kv_get("ext.pair").unwrap().contains(&norm_code(&code)));
        let mut ws = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut ws, json!({"t": "pair", "code": code})).await;
        let token = next_t(&mut ws, "paired").await["token"].as_str().unwrap().to_string();
        assert_eq!(token.len(), 64);
        assert!(!db.kv_get("ext.token").unwrap().contains(&token), "only a hash of the token is stored");
        // the code cannot be used twice
        let mut again = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut again, json!({"t": "pair", "code": code})).await;
        assert_eq!(next_t(&mut again, "auth").await["ok"], false);
        // the token signs in later, with no code
        let mut ws2 = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut ws2, json!({"t": "auth", "token": token})).await;
        assert_eq!(next_t(&mut ws2, "auth").await["ok"], true);
        next_t(&mut ws2, "hello").await;
        assert!(b.connected());
        // unpair: the token stops working
        unpair(&db);
        let mut ws3 = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut ws3, json!({"t": "auth", "token": token})).await;
        assert_eq!(next_t(&mut ws3, "auth").await["ok"], false);
    }

    #[tokio::test]
    async fn expired_code_is_refused_and_wrong_tries_lock_sign_in() {
        let (_b, _h, db, _f, addr, _) = start().await;
        let code = new_pair_code(&db).unwrap();
        let h = sha_hex(&norm_code(&code));
        db.kv_set("ext.pair", &format!("{h}|{}", now_ms() - 1));
        let mut ws = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut ws, json!({"t": "pair", "code": code})).await;
        assert_eq!(next_t(&mut ws, "auth").await["ok"], false);
        // five wrong tries in a row, then even a right code waits a minute
        for _ in 0..5 {
            let mut w = connect(addr, Some(&extension_origin())).await.unwrap();
            say(&mut w, json!({"t": "auth", "token": "nope"})).await;
            next_t(&mut w, "auth").await;
        }
        let good = new_pair_code(&db).unwrap();
        let mut w = connect(addr, Some(&extension_origin())).await.unwrap();
        say(&mut w, json!({"t": "pair", "code": good})).await;
        let a = next_t(&mut w, "auth").await;
        assert_eq!(a["ok"], false);
        assert!(a["error"].as_str().unwrap().contains("Too many"));
    }

    #[tokio::test]
    async fn events_sent_to_the_extension_carry_no_secrets_and_no_candle_ticks() {
        let (_b, hub, db, _f, addr, _) = start().await;
        let (mut ws, _t) = paired_client(addr, &db).await;
        hub.emit(EngineEvent::Activity { ts: 5, agent: crate::events::AgentId::Executor, text: "Bought TEST".into(), mint: Some(MINT.into()), source_url: None });
        let e = next_t(&mut ws, "event").await;
        assert_eq!(e["e"]["type"], "activity");
        // verdict and token events (hundreds per hour) are not forwarded
        hub.emit(EngineEvent::Token { ts: 1, mint: MINT.into(), symbol: "X".into(), name: "x".into(), creator: "c".into(), mcap_sol: 1.0 });
        hub.emit(EngineEvent::Activity { ts: 6, agent: crate::events::AgentId::Executor, text: "second".into(), mint: None, source_url: None });
        let e2 = next_t(&mut ws, "event").await;
        assert_eq!(e2["e"]["text"], "second");
    }

    #[tokio::test]
    async fn verdict_lookup_covers_blocked_skipped_watching_held_and_unknown() {
        let (b, hub, db, _f, addr, _) = start().await;
        let (mut ws, _t) = paired_client(addr, &db).await;
        let other = |n: u8| bs58::encode([n; 32]).into_string();
        let (m_blocked, m_skipped, m_watch, m_held, m_unknown) = (other(1), other(2), other(3), other(4), other(5));
        let verdict = |mint: &str, sym: &str, out: &str, stage: &str, reason: &str, detail: &str| EngineEvent::Verdict { ts: 1, mint: mint.into(), symbol: sym.into(), stage: stage.into(), outcome: out.into(), reason: reason.into(), detail: detail.into(), verdict: if out == "block" { "HIGH".into() } else { "LOW_SIGNAL".into() }, evidence: vec![] };
        hub.emit(verdict(&m_blocked, "BAD", "block", "chain", "CREATOR_HOLDING", "Creator holds 40% of the supply"));
        hub.emit(verdict(&m_skipped, "MEH", "pass", "chain", "", "ok"));
        hub.emit(EngineEvent::Score { ts: 2, mint: m_skipped.clone(), symbol: "MEH".into(), score: 41.0, min_score: 80.0, picked: false, buyers: 3, buys: 4, sells: 2, ratio: 2.0, price_change_pct: 1.0, curve_sol: 3.0, mcap_sol: 30.0, gates: vec!["curve SOL 3.0 outside 12-40".into()] });
        hub.emit(verdict(&m_watch, "WAIT", "pass", "chain", "", "ok"));
        hub.emit(verdict(&m_held, "OWN", "pass", "chain", "", "ok"));
        let pos = crate::events::PositionView { id: 1, lane: "momentum".into(), mint: m_held.clone(), symbol: "OWN".into(), state: "OPEN".into(), entry_ts: 1, entry_price: 1.0, current_price: 1.0, size_sol: 0.1, tokens_remaining: 1.0, pnl_sol: 0.0, pnl_pct: 0.0, price_pct: 0.0, hold_seconds: 1.0, next_action: "".into(), soft_stop: 0.9, hard_stop: 0.85, take_profits: vec![], tranches_done: 0, exits: vec![], exit_reason: "".into(), ticks: vec![[1.0, 1.0]], replay: false };
        hub.emit(EngineEvent::Positions { ts: 3, positions: vec![pos] });
        tokio::time::sleep(Duration::from_millis(200)).await;
        let ask = |ws_mint: String, id: u32| json!({"t": "verdict", "mint": ws_mint, "id": id});
        let mut got = HashMap::new();
        for (i, m) in [&m_blocked, &m_skipped, &m_watch, &m_held, &m_unknown].iter().enumerate() {
            say(&mut ws, ask(m.to_string(), i as u32)).await;
            let r = next_t(&mut ws, "verdict").await;
            assert_eq!(r["id"], i as u32);
            got.insert(m.to_string(), r["verdict"].clone());
        }
        assert_eq!(got[&m_blocked]["status"], "blocked");
        assert_eq!(got[&m_blocked]["symbol"], "BAD");
        assert_eq!(got[&m_blocked]["shield"], "block");
        assert_eq!(got[&m_blocked]["shieldLevel"], "HIGH");
        assert!(got[&m_blocked]["reason"].as_str().unwrap().contains("40%"));
        assert_eq!(got[&m_skipped]["status"], "skipped");
        assert_eq!(got[&m_skipped]["score"], 41.0);
        assert!(got[&m_skipped]["reason"].as_str().unwrap().contains("curve SOL"));
        assert_eq!(got[&m_watch]["status"], "watching");
        assert_eq!(got[&m_held]["status"], "held");
        assert_eq!(got[&m_unknown]["status"], "unknown");
        assert_eq!(got[&m_unknown]["shield"], "none");
        // a bad address is an error, not a lookup
        say(&mut ws, json!({"t": "verdict", "mint": "not a mint", "id": 99})).await;
        assert_eq!(next_t(&mut ws, "error").await["id"], 99);
        // verdicts survive a restart: an empty book reads the decisions table
        db.insert_decision(&crate::db::DecisionRow { ts: 1, mint: m_unknown.clone(), symbol: "OLD".into(), stage: "chain".into(), outcome: "block".into(), reason: "MINT_AUTHORITY".into(), detail: "Mint authority is set".into(), verdict: "HIGH".into() });
        let fresh = MintBook::default().verdict(&m_unknown, &db);
        assert_eq!(fresh.status, "blocked");
        assert_eq!(fresh.symbol, "OLD");
        let _ = b;
    }

    #[tokio::test]
    async fn a_block_is_not_overwritten_by_a_later_pass() {
        let db = Db::open(":memory:").unwrap();
        let mut book = MintBook::default();
        let mk = |out: &str| EngineEvent::Verdict { ts: 1, mint: MINT.into(), symbol: "S".into(), stage: "chain".into(), outcome: out.into(), reason: "R".into(), detail: "d".into(), verdict: "HIGH".into(), evidence: vec![] };
        book.update(&mk("block"));
        book.update(&mk("pass"));
        assert_eq!(book.verdict(MINT, &db).status, "blocked");
    }

    #[tokio::test]
    async fn check_this_coin_runs_the_checker_and_can_never_trigger_a_buy() {
        let (b, hub, db, fake, addr, cmds) = start().await;
        let (mut ws, _t) = paired_client(addr, &db).await;
        say(&mut ws, json!({"t": "check", "mint": MINT, "id": 7})).await;
        let r = next_t(&mut ws, "check").await;
        assert_eq!(r["id"], 7);
        assert_eq!(r["verdict"]["status"], "blocked");
        assert_eq!(fake.0.lock().unwrap().as_slice(), [MINT.to_string()]);
        // asking again at once does not run it twice
        say(&mut ws, json!({"t": "check", "mint": MINT, "id": 8})).await;
        next_t(&mut ws, "check").await;
        assert_eq!(fake.0.lock().unwrap().len(), 1);
        // every way of asking for a trade is refused and nothing reaches the engine's command handler
        for msg in [
            json!({"t": "buy", "mint": MINT, "id": 1}),
            json!({"t": "sell", "mint": MINT, "id": 2}),
            json!({"t": "cmd", "cmd": "buy", "id": 3}),
            json!({"t": "cmd", "cmd": format!("buy:{MINT}"), "id": 4}),
            json!({"t": "cmd", "cmd": "set-paper-fill:{}", "id": 5}),
            json!({"t": "cmd", "cmd": "approve-change:1", "id": 6}),
            json!({"t": "cmd", "cmd": "reload-rpc", "id": 9}),
            json!({"t": "check", "mint": MINT, "id": 10, "buy": true, "force": true}),
        ] {
            say(&mut ws, msg.clone()).await;
            let r = tokio::time::timeout(Duration::from_secs(5), async { loop { let m = ws.next().await.unwrap().unwrap(); if let Message::Text(s) = m { let v: Value = serde_json::from_str(&s).unwrap(); if v["id"] == msg["id"] { return v; } } } }).await.unwrap();
            if msg["t"] != "check" {
                assert_eq!(r["t"], "error", "{msg} must be refused");
            }
        }
        assert!(cmds.lock().unwrap().is_empty(), "no command may reach the engine: {:?}", cmds.lock().unwrap());
        assert_eq!(db.position_count(), 0);
        // the two allowed controls do reach it
        say(&mut ws, json!({"t": "cmd", "cmd": "kill", "id": 11})).await;
        assert_eq!(next_t(&mut ws, "cmd").await["answer"], "ok");
        assert_eq!(cmds.lock().unwrap().as_slice(), ["kill".to_string()]);
        let _ = (b, hub);
    }

    #[test]
    fn the_command_whitelist_has_no_trade_commands() {
        assert_eq!(ALLOWED_COMMANDS, ["kill", "resume"]);
        // the source of this module never builds a buy: it must not mention the trader or entering a position
        let src = include_str!("bridge.rs");
        let code = src.split("#[cfg(test)]").next().unwrap();
        for bad in ["crate::trader", "Arc<Trader>", "try_enter", "crate::exec", "Executor", ".buy(", "paper::"] {
            assert!(!code.contains(bad), "bridge code must not reference {bad}");
        }
    }

    #[tokio::test]
    async fn the_real_checker_only_records_a_decision_and_opens_no_position() {
        use crate::rpc::Rpc;
        use crate::shield::ShieldOpts;
        let cfg = crate::Config::from_json(include_str!("../../config.json")).unwrap();
        let db = Arc::new(Db::open(":memory:").unwrap());
        let hub = Hub::new();
        // an endpoint that refuses connections: every chain check is "undecided", which blocks
        let rpc = Arc::new(Rpc::new("http://127.0.0.1:1", 50.0));
        let shield = Arc::new(Shield::new(cfg, db.clone(), rpc, ShieldOpts { relax_egress: false }));
        let c = ShieldChecker { shield, hub: hub.clone(), db: db.clone() };
        let a = tokio::time::timeout(Duration::from_secs(40), c.check(MINT)).await.expect("check finished");
        assert_eq!(a.outcome, "block");
        assert!(db.latest_decision(MINT).map(|d| d.1 == "manual-check").unwrap_or(false));
        assert_eq!(db.position_count(), 0);
        assert!(!db.mint_has_position(MINT));
    }
}
