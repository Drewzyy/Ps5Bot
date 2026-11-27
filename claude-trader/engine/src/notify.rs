//! Connectors that send messages out: Discord webhook and a generic JSON webhook.
//! Outgoing only. Messages are built from real recorded events (a trade that closed, the day's real totals...), never invented.
//!
//! Safety rules:
//! - Secrets (webhook URLs, bot token, API keys) live in the OS secure storage (one entry per secret, encrypted file as
//!   fallback), exactly like the Helius key. The page only ever sees a masked hint. They never go to config.json,
//!   SQLite, logs or git, and every error text is scrubbed of them before it is stored or shown.
//! - Every connector has its own bounded queue and its own worker task. `emit` only pushes to the queue and returns at
//!   once; a full queue drops the message. A slow, failing or rate-limited service therefore never blocks trading.
//! - Sends are paced (minimum gap between messages) and a 429 answer waits for the service's `retry_after`.
//! - Messages carry a "Paper results" label while the app is paper only, so they never look like real profit.

use crate::db::Db;
use crate::net::mask;
use crate::secrets::KeyStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

const SETTINGS_KEY: &str = "notify.settings";
pub const WEBHOOK_SCHEMA: &str = "claude-trader/event/v1";

// ------------------------------------------------------------------ the secrets (one secure entry per secret)

/// Every secret field, by the name of its secure entry.
pub const FIELDS: [&str; 2] = ["discord-url", "webhook-url"];
/// Optional extra secret of the generic webhook (sent as the X-Claude-Trader-Secret header).
pub const WEBHOOK_SECRET_FIELD: &str = "webhook-secret";

pub struct ConnSecrets {
    make: Box<dyn Fn(&str) -> KeyStore + Send + Sync>,
}

impl ConnSecrets {
    pub fn system(dir: std::path::PathBuf) -> ConnSecrets {
        ConnSecrets {
            make: Box::new(move |name| {
                let acct: &'static str = match name {
                    "discord-url" => "connector-discord-url",
                    "webhook-url" => "connector-webhook-url",
                    _ => "connector-webhook-secret",
                };
                KeyStore::system_named(dir.clone(), acct, &format!("conn-{name}.enc"))
            }),
        }
    }
    /// Tests: every secret in memory.
    pub fn in_memory() -> ConnSecrets {
        let map: Arc<std::sync::Mutex<BTreeMap<String, Arc<crate::secrets::MemoryBackend>>>> = Default::default();
        ConnSecrets {
            make: Box::new(move |name| {
                let b = map.lock().unwrap().entry(name.to_string()).or_insert_with(|| Arc::new(crate::secrets::MemoryBackend::new())).clone();
                KeyStore::with_backends(Some(b), Arc::new(crate::secrets::MemoryBackend::new()))
            }),
        }
    }
    pub fn get(&self, name: &str) -> Option<String> {
        (self.make)(name).load().filter(|v| !v.is_empty())
    }
    pub fn set(&self, name: &str, value: &str) -> Result<(), String> {
        (self.make)(name).save_raw(value).map(|_| ()).map_err(|e| mask(&e))
    }
    pub fn remove(&self, name: &str) {
        let _ = (self.make)(name).remove();
    }
    fn all_values(&self) -> Vec<String> {
        FIELDS.iter().chain([&WEBHOOK_SECRET_FIELD]).filter_map(|f| self.get(f)).collect()
    }
}

/// Replace every secret (and any URL token or bot token) in `msg` by `***`.
pub fn redact(msg: &str, secrets: &[String]) -> String {
    let mut out = msg.to_string();
    for s in secrets {
        if s.len() >= 4 {
            out = out.replace(s.as_str(), "***");
            // the token part of a webhook URL or of a bot URL can appear on its own
            if let Some(tail) = s.rsplit('/').next().filter(|t| t.len() >= 8) {
                out = out.replace(tail, "***");
            }
        }
    }
    mask(&out)
}

/// "...abcd" style hint: the last four characters only.
pub fn tail_hint(secret: &str) -> String {
    let c: Vec<char> = secret.chars().collect();
    if c.len() < 12 { "saved".into() } else { format!("saved, ends in ...{}", c[c.len() - 4..].iter().collect::<String>()) }
}

/// "discord.com/…abcd": the host and the last four characters of a secret URL.
pub fn url_hint(url: &str) -> String {
    let host = url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or("");
    let c: Vec<char> = url.chars().collect();
    if c.len() < 12 { "saved".into() } else { format!("saved, {host}/...{}", c[c.len() - 4..].iter().collect::<String>()) }
}

// ------------------------------------------------------------------ validation

pub fn validate_discord_url(u: &str) -> Result<String, String> {
    let u = u.trim();
    let rest = u.strip_prefix("https://").ok_or("A Discord webhook link starts with https://discord.com/api/webhooks/")?;
    let (host, path) = rest.split_once('/').ok_or("That is not a Discord webhook link.")?;
    if !(host == "discord.com" || host == "discordapp.com" || host == "ptb.discord.com" || host == "canary.discord.com") {
        return Err("Only discord.com or discordapp.com webhook links are accepted.".into());
    }
    let parts: Vec<&str> = path.split('/').collect();
    let ok = parts.len() == 4
        && parts[0] == "api"
        && parts[1] == "webhooks"
        && !parts[2].is_empty()
        && parts[2].chars().all(|c| c.is_ascii_digit())
        && parts[3].len() >= 20
        && parts[3].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err("That does not look like a Discord webhook link. It ends with a number and a long code.".into());
    }
    Ok(u.to_string())
}

pub fn validate_webhook_url(u: &str) -> Result<String, String> {
    let u = u.trim();
    if u.len() > 500 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("That is not a normal web link.".into());
    }
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"].iter().any(|p| u.starts_with(p));
    if !(u.starts_with("https://") || local) {
        return Err("The webhook link must start with https:// (http:// is only allowed for localhost).".into());
    }
    let host = u.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return Err("That is not a normal web link.".into());
    }
    Ok(u.to_string())
}

// ------------------------------------------------------------------ settings (no secrets in here)

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Events {
    pub buy: bool,
    pub sell: bool,
    pub daily_summary: bool,
    pub learning_change: bool,
    pub kill_switch: bool,
    pub loss_limit: bool,
    pub health: bool,
    pub update_installed: bool,
}

impl Default for Events {
    fn default() -> Self {
        Events { buy: true, sell: true, daily_summary: true, learning_change: true, kill_switch: true, loss_limit: true, health: true, update_installed: true }
    }
}

impl Events {
    pub fn allows(&self, kind: &str) -> bool {
        match kind {
            "buy" => self.buy,
            "sell" => self.sell,
            "dailySummary" => self.daily_summary,
            "learningChange" => self.learning_change,
            "killSwitch" => self.kill_switch,
            "lossLimit" => self.loss_limit,
            "health" => self.health,
            "updateInstalled" => self.update_installed,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnCfg {
    /// True once the needed secrets were saved (so `emit` does not have to open the secure storage).
    pub configured: bool,
    pub events: Events,
}

impl Default for ConnCfg {
    fn default() -> Self {
        ConnCfg { configured: false, events: Events::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifySettings {
    pub discord: ConnCfg,
    pub webhook: ConnCfg,
}

impl Default for NotifySettings {
    fn default() -> Self {
        NotifySettings { discord: ConnCfg::default(), webhook: ConnCfg::default() }
    }
}

impl NotifySettings {
    pub fn load(db: &Db) -> NotifySettings {
        db.kv_get(SETTINGS_KEY).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
    }
    pub fn save(&self, db: &Db) {
        db.kv_set(SETTINGS_KEY, &serde_json::to_string(self).unwrap_or_default());
    }
    pub fn cfg(&self, c: Conn) -> &ConnCfg {
        match c {
            Conn::Discord => &self.discord,
            Conn::Webhook => &self.webhook,
        }
    }
    pub fn cfg_mut(&mut self, c: Conn) -> &mut ConnCfg {
        match c {
            Conn::Discord => &mut self.discord,
            Conn::Webhook => &mut self.webhook,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Conn {
    Discord,
    Webhook,
}

impl Conn {
    pub const ALL: [Conn; 2] = [Conn::Discord, Conn::Webhook];
    pub fn name(&self) -> &'static str {
        match self {
            Conn::Discord => "discord",
            Conn::Webhook => "webhook",
        }
    }
    pub fn parse(s: &str) -> Option<Conn> {
        Conn::ALL.into_iter().find(|c| c.name() == s)
    }
    /// Seconds-level pacing: the least time between two sends.
    pub fn gap(&self) -> Duration {
        match self {
            Conn::Discord => Duration::from_millis(1200),
            Conn::Webhook => Duration::from_millis(500),
        }
    }
    fn secret_fields(&self) -> &'static [&'static str] {
        match self {
            Conn::Discord => &["discord-url"],
            Conn::Webhook => &["webhook-url"],
        }
    }
}

// ------------------------------------------------------------------ events (real data only)

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Buy { ts: i64, symbol: String, mint: String, lane: String, size_sol: f64, entry_price: f64, score: Option<f64> },
    Sell { ts: i64, symbol: String, mint: String, lane: String, pnl_sol: f64, pnl_pct: f64, exit_reason: String, hold_seconds: i64, fees_sol: f64 },
    DailySummary { date: String, trades: u32, wins: u32, losses: u32, pnl_sol: f64, fees_sol: f64, win_rate_pct: f64 },
    LearningApplied { ts: i64, key: String, label: String, before: f64, after: f64 },
    KillSwitch { ts: i64, on: bool, source: String },
    LossLimit { ts: i64, lane: String, text: String },
    Health { ts: i64, kind: String, text: String },
    UpdateInstalled { ts: i64, version: String },
    Test { ts: i64 },
}

fn sol(x: f64) -> String {
    format!("{x:+.4}")
}

fn dex(mint: &str) -> String {
    format!("https://dexscreener.com/solana/{mint}")
}

impl Event {
    /// The toggle that controls it. Test messages are always allowed.
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Buy { .. } => "buy",
            Event::Sell { .. } => "sell",
            Event::DailySummary { .. } => "dailySummary",
            Event::LearningApplied { .. } => "learningChange",
            Event::KillSwitch { .. } => "killSwitch",
            Event::LossLimit { .. } => "lossLimit",
            Event::Health { .. } => "health",
            Event::UpdateInstalled { .. } => "updateInstalled",
            Event::Test { .. } => "test",
        }
    }
    pub fn ts(&self) -> i64 {
        match self {
            Event::Buy { ts, .. } | Event::Sell { ts, .. } | Event::LearningApplied { ts, .. } | Event::KillSwitch { ts, .. } | Event::LossLimit { ts, .. } | Event::Health { ts, .. } | Event::UpdateInstalled { ts, .. } | Event::Test { ts } => *ts,
            Event::DailySummary { .. } => 0,
        }
    }
    pub fn mint(&self) -> Option<&str> {
        match self {
            Event::Buy { mint, .. } | Event::Sell { mint, .. } => Some(mint),
            _ => None,
        }
    }
    pub fn title(&self) -> String {
        match self {
            Event::Buy { symbol, .. } => format!("Bought {symbol}"),
            Event::Sell { symbol, pnl_sol, .. } => format!("{} {symbol}", if *pnl_sol > 0.0 { "Win" } else { "Loss" }),
            Event::DailySummary { date, .. } => format!("Daily summary {date}"),
            Event::LearningApplied { label, .. } => format!("Learning change applied: {label}"),
            Event::KillSwitch { on, .. } => if *on { "Kill switch ON".into() } else { "Trading resumed".into() },
            Event::LossLimit { lane, .. } => format!("Trading halted ({lane})"),
            Event::Health { .. } => "Health warning".into(),
            Event::UpdateInstalled { version, .. } => format!("Updated to v{version}"),
            Event::Test { .. } => "Test message".into(),
        }
    }
    /// (name, value) lines, all straight from the recorded data.
    pub fn fields(&self) -> Vec<(String, String)> {
        let f = |a: &str, b: String| (a.to_string(), b);
        match self {
            Event::Buy { size_sol, entry_price, lane, score, .. } => {
                let mut v = vec![f("Size", format!("{size_sol:.4} SOL")), f("Entry price", format!("{entry_price:.4e} SOL")), f("Lane", lane.clone())];
                if let Some(s) = score {
                    v.push(f("Score", format!("{s:.1}")));
                }
                v
            }
            Event::Sell { pnl_sol, pnl_pct, exit_reason, hold_seconds, fees_sol, lane, .. } => vec![f("P&L", format!("{} SOL ({:+.1}%)", sol(*pnl_sol), pnl_pct)), f("Exit", exit_reason.clone()), f("Held", format!("{hold_seconds} s")), f("Fees", format!("{fees_sol:.5} SOL")), f("Lane", lane.clone())],
            Event::DailySummary { trades, wins, losses, pnl_sol, fees_sol, win_rate_pct, .. } => vec![f("Trades", format!("{trades} ({wins} wins, {losses} losses)")), f("Win rate", format!("{win_rate_pct:.0}%")), f("Net P&L", format!("{} SOL", sol(*pnl_sol))), f("Fees", format!("{fees_sol:.5} SOL"))],
            Event::LearningApplied { key, before, after, .. } => vec![f("Setting", key.clone()), f("Change", format!("{} to {}", crate::learning::trim(*before), crate::learning::trim(*after)))],
            Event::KillSwitch { source, .. } => vec![f("From", source.clone())],
            Event::LossLimit { text, .. } => vec![f("Why", text.clone())],
            Event::Health { text, kind, .. } => vec![f("What", text.clone()), f("Kind", kind.clone())],
            Event::UpdateInstalled { version, .. } => vec![f("Version", version.clone())],
            Event::Test { .. } => vec![f("Status", "Connection works".into())],
        }
    }
}

// ------------------------------------------------------------------ message builders (pure)

pub fn paper_label(paper: bool) -> &'static str {
    if paper { "Paper results" } else { "" }
}

/// Discord embed: green for a win, red for a loss, grey-blue for everything else.
pub fn discord_payload(e: &Event, paper: bool) -> Value {
    let color = match e {
        Event::Sell { pnl_sol, .. } => if *pnl_sol > 0.0 { 0x2ecc71 } else { 0xe74c3c },
        Event::DailySummary { pnl_sol, .. } => if *pnl_sol > 0.0 { 0x2ecc71 } else if *pnl_sol < 0.0 { 0xe74c3c } else { 0x95a5a6 },
        Event::Buy { .. } => 0x3498db,
        Event::KillSwitch { .. } | Event::LossLimit { .. } | Event::Health { .. } => 0xf39c12,
        _ => 0x95a5a6,
    };
    let mut embed = json!({
        "title": e.title(),
        "color": color,
        "fields": e.fields().into_iter().map(|(n, v)| json!({"name": n, "value": v, "inline": true})).collect::<Vec<_>>(),
        "footer": {"text": if paper { "Paper results (pretend money)" } else { "Claude Trader" }},
    });
    if let Some(m) = e.mint() {
        embed["url"] = json!(dex(m));
        embed["description"] = json!(format!("[Dexscreener]({})", dex(m)));
    }
    if e.ts() > 0 {
        embed["timestamp"] = json!(crate::timefmt::iso_local(e.ts()));
    }
    json!({"username": "Claude Trader", "embeds": [embed]})
}

/// The generic webhook body. This is the documented schema (claude-trader/event/v1).
pub fn webhook_payload(e: &Event, paper: bool, sent_at_ms: i64) -> Value {
    json!({
        "schema": WEBHOOK_SCHEMA,
        "event": e.kind(),
        "mode": if paper { "paper" } else { "live" },
        "paper": paper,
        "sent_at": crate::timefmt::iso_local(sent_at_ms),
        "title": e.title(),
        "data": serde_json::to_value(e).unwrap_or(Value::Null),
        "links": e.mint().map(|m| json!({"dexscreener": dex(m), "solscan": crate::pump::solscan_token(m)})).unwrap_or(Value::Null),
    })
}

// ------------------------------------------------------------------ transport and delivery

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<f64>,
}

#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    async fn post(&self, url: &str, headers: Vec<(String, String)>, body: String) -> Result<Reply, String>;
}

pub struct HttpTransport;

#[async_trait::async_trait]
impl Transport for HttpTransport {
    async fn post(&self, url: &str, headers: Vec<(String, String)>, body: String) -> Result<Reply, String> {
        let mut rb = crate::net::http_client().post(url).timeout(Duration::from_secs(15)).header("Content-Type", "application/json").body(body);
        for (k, v) in headers {
            rb = rb.header(k, v);
        }
        let r = rb.send().await.map_err(|e| e.without_url().to_string())?;
        let status = r.status().as_u16();
        let hdr = r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<f64>().ok());
        let body = r.text().await.unwrap_or_default();
        Ok(Reply { status, body, retry_after: hdr })
    }
}

#[derive(Debug, Clone)]
pub struct Outgoing {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// How long the service asked us to wait (header or JSON body: Discord `retry_after` seconds).
pub fn retry_after_of(r: &Reply) -> f64 {
    let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
    r.retry_after.or_else(|| v["retry_after"].as_f64()).or_else(|| v["parameters"]["retry_after"].as_f64()).unwrap_or(5.0).clamp(0.0, 60.0)
}

fn explain(status: u16, body: &str) -> String {
    let hint = match status {
        400 => "the service did not accept the message",
        401 | 403 => "the service refused the key or permission",
        404 => "the link or bot was not found (deleted?)",
        _ => "unexpected answer",
    };
    let snippet: String = serde_json::from_str::<Value>(body).ok().and_then(|v| v["message"].as_str().or_else(|| v["description"].as_str()).or_else(|| v["detail"].as_str()).map(String::from)).unwrap_or_default().chars().take(120).collect();
    if snippet.is_empty() { format!("HTTP {status}: {hint}") } else { format!("HTTP {status}: {hint} ({snippet})") }
}

/// Send with up to three tries: waits for `retry_after` on 429, backs off a little on 5xx. Errors are scrubbed.
pub async fn deliver(t: &dyn Transport, out: &Outgoing, secrets: &[String], wait_scale: f64) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 1..=3u32 {
        match t.post(&out.url, out.headers.clone(), out.body.clone()).await {
            Ok(r) if (200..300).contains(&r.status) => return Ok(()),
            Ok(r) if r.status == 429 => {
                last = "rate limited (HTTP 429)".into();
                tokio::time::sleep(Duration::from_secs_f64(retry_after_of(&r) * wait_scale)).await;
            }
            Ok(r) if r.status >= 500 => {
                last = explain(r.status, &r.body);
                tokio::time::sleep(Duration::from_secs_f64(attempt as f64 * 2.0 * wait_scale)).await;
            }
            Ok(r) => return Err(redact(&explain(r.status, &r.body), secrets)),
            Err(e) => {
                last = format!("network: {e}");
                tokio::time::sleep(Duration::from_secs_f64(attempt as f64 * 2.0 * wait_scale)).await;
            }
        }
    }
    Err(redact(&format!("gave up after 3 tries: {last}"), secrets))
}

/// The request for one connector, or None when something it needs is missing.
pub fn build_request(c: Conn, e: &Event, sec: &ConnSecrets, _cfg: &NotifySettings, paper: bool, now_ms: i64) -> Option<Outgoing> {
    let json_h = || vec![("Content-Type".to_string(), "application/json".to_string())];
    match c {
        Conn::Discord => Some(Outgoing { url: sec.get("discord-url")?, headers: json_h(), body: discord_payload(e, paper).to_string() }),
        Conn::Webhook => {
            let mut h = json_h();
            if let Some(s) = sec.get(WEBHOOK_SECRET_FIELD) {
                h.push(("X-Claude-Trader-Secret".into(), s));
            }
            Some(Outgoing { url: sec.get("webhook-url")?, headers: h, body: webhook_payload(e, paper, now_ms).to_string() })
        }
    }
}

// ------------------------------------------------------------------ the router (queues and workers)

pub struct Router {
    db: Arc<Db>,
    senders: BTreeMap<&'static str, mpsc::Sender<Event>>,
}

const QUEUE: usize = 64;

fn err_key(c: Conn) -> String {
    format!("notify.err.{}", c.name())
}
fn ok_key(c: Conn) -> String {
    format!("notify.ok.{}", c.name())
}

impl Router {
    /// Spawns one worker per connector. `gap_scale` and `wait_scale` shrink the waits in tests (1.0 in the app).
    pub fn start(db: Arc<Db>, secrets: Arc<ConnSecrets>, paper: bool, transport: Arc<dyn Transport>, gap_scale: f64, wait_scale: f64) -> Router {
        let mut senders = BTreeMap::new();
        for c in Conn::ALL {
            let (tx, mut rx) = mpsc::channel::<Event>(QUEUE);
            senders.insert(c.name(), tx);
            let (db, secrets, transport) = (db.clone(), secrets.clone(), transport.clone());
            tokio::spawn(async move {
                let mut last = tokio::time::Instant::now() - Duration::from_secs(3600);
                while let Some(e) = rx.recv().await {
                    let gap = Duration::from_secs_f64(c.gap().as_secs_f64() * gap_scale);
                    let since = last.elapsed();
                    if since < gap {
                        tokio::time::sleep(gap - since).await;
                    }
                    let now = crate::rpc::now_ms();
                    let st = NotifySettings::load(&db);
                    let Some(req) = build_request(c, &e, &secrets, &st, paper, now) else { continue };
                    let all = secrets.all_values();
                    let res = deliver(transport.as_ref(), &req, &all, wait_scale).await;
                    last = tokio::time::Instant::now();
                    match res {
                        Ok(()) => {
                            db.kv_del(&err_key(c));
                            db.kv_set(&ok_key(c), &now.to_string());
                        }
                        Err(m) => db.kv_set(&err_key(c), &m),
                    }
                }
            });
        }
        Router { db, senders }
    }

    /// Queue an event for every connector that is set up and has this event switched on. Never blocks, never fails.
    pub fn emit(&self, e: Event) {
        let st = NotifySettings::load(&self.db);
        for c in Conn::ALL {
            let cfg = st.cfg(c);
            if cfg.configured && cfg.events.allows(e.kind()) {
                if let Some(tx) = self.senders.get(c.name()) {
                    let _ = tx.try_send(e.clone()); // a full queue drops the message instead of waiting
                }
            }
        }
    }
}

static ROUTER: OnceLock<Router> = OnceLock::new();

/// Start the workers (once). Called by the engine at startup.
pub fn init(db: Arc<Db>, secrets: Arc<ConnSecrets>, paper: bool) {
    let _ = ROUTER.set(Router::start(db, secrets, paper, Arc::new(HttpTransport), 1.0, 1.0));
}

/// The one call the rest of the engine makes. Does nothing before `init` or when no connector wants the event.
pub fn emit(e: Event) {
    if let Some(r) = ROUTER.get() {
        r.emit(e);
    }
}

// ------------------------------------------------------------------ status and actions for the Settings cards

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnStatus {
    pub conn: Conn,
    pub configured: bool,
    /// Masked hints only, one per saved secret field: "saved, ends in ...abcd".
    pub hints: BTreeMap<String, String>,
    pub events: Events,
    pub last_error: String,
    pub last_ok_ms: i64,
}

pub fn status(db: &Db, sec: &ConnSecrets, c: Conn, _now_ms: i64) -> ConnStatus {
    let st = NotifySettings::load(db);
    let cfg = st.cfg(c);
    let mut hints = BTreeMap::new();
    for f in c.secret_fields().iter().chain(if c == Conn::Webhook { &[WEBHOOK_SECRET_FIELD][..] } else { &[][..] }) {
        if let Some(v) = sec.get(f) {
            hints.insert(f.to_string(), if f.ends_with("url") { url_hint(&v) } else { tail_hint(&v) });
        }
    }
    ConnStatus {
        conn: c,
        configured: cfg.configured,
        hints,
        events: cfg.events,
        last_error: db.kv_get(&err_key(c)).unwrap_or_default(),
        last_ok_ms: db.kv_get(&ok_key(c)).and_then(|v| v.parse().ok()).unwrap_or(0),
    }
}

/// Save the fields typed in a card (name -> value; empty values are ignored so a saved secret is kept). Validates first.
pub fn save(db: &Db, sec: &ConnSecrets, c: Conn, fields: &BTreeMap<String, String>) -> Result<(), String> {
    let get = |k: &str| fields.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let mut put: Vec<(&str, String)> = vec![];
    match c {
        Conn::Discord => {
            if let Some(u) = get("discord-url") {
                put.push(("discord-url", validate_discord_url(&u)?));
            }
        }
        Conn::Webhook => {
            if let Some(u) = get("webhook-url") {
                put.push(("webhook-url", validate_webhook_url(&u)?));
            }
            if let Some(s) = get(WEBHOOK_SECRET_FIELD) {
                if s.len() > 200 || s.chars().any(|c| c.is_control()) {
                    return Err("The secret header value is too long or has odd characters.".into());
                }
                put.push((WEBHOOK_SECRET_FIELD, s));
            }
        }
    }
    for (f, v) in &put {
        sec.set(f, v)?;
    }
    let mut st = NotifySettings::load(db);
    let have_all = c.secret_fields().iter().all(|f| sec.get(f).is_some());
    st.cfg_mut(c).configured = have_all;
    st.save(db);
    Ok(())
}

pub fn remove(db: &Db, sec: &ConnSecrets, c: Conn) {
    for f in c.secret_fields().iter().chain(if c == Conn::Webhook { &[WEBHOOK_SECRET_FIELD][..] } else { &[][..] }) {
        sec.remove(f);
    }
    let mut st = NotifySettings::load(db);
    let keep = st.cfg(c).events;
    *st.cfg_mut(c) = ConnCfg { configured: false, events: keep };
    st.save(db);
    db.kv_del(&err_key(c));
}

pub fn set_events(db: &Db, c: Conn, events: Events) {
    let mut st = NotifySettings::load(db);
    st.cfg_mut(c).events = events;
    st.save(db);
}

/// "Send test message": goes straight out (not through the queue) so the answer can be shown.
pub async fn send_test(db: &Db, sec: &ConnSecrets, c: Conn, paper: bool, t: &dyn Transport, wait_scale: f64) -> Result<String, String> {
    let now = crate::rpc::now_ms();
    let st = NotifySettings::load(db);
    let e = Event::Test { ts: now };
    let req = build_request(c, &e, sec, &st, paper, now).ok_or("Save the connection details first.")?;
    deliver(t, &req, &sec.all_values(), wait_scale).await?;
    db.kv_set(&ok_key(c), &now.to_string());
    db.kv_del(&err_key(c));
    Ok("Test message sent. Check the channel.".into())
}

// ------------------------------------------------------------------ scans that produce events

/// Newly applied learning changes (once each), from the changes table.
pub fn scan_learning(db: &Db) {
    let seen: i64 = db.kv_get("notify.applied_seen").and_then(|v| v.parse().ok()).unwrap_or(-1);
    let applied: Vec<_> = crate::learning::changes_all(db).into_iter().filter(|c| c.status == "applied").collect();
    if seen < 0 {
        // first look: whatever was applied before connectors existed is not announced
        db.kv_set("notify.applied_seen", &applied.iter().map(|c| c.id).max().unwrap_or(0).to_string());
        return;
    }
    let mut top = seen;
    for c in applied.iter().filter(|c| c.id > seen) {
        emit(Event::LearningApplied { ts: c.done_ms.max(c.ts), key: c.key.clone(), label: c.label.clone(), before: c.before, after: c.after });
        top = top.max(c.id);
    }
    if top != seen {
        db.kv_set("notify.applied_seen", &top.to_string());
    }
}

/// Real totals of one local day from closed trades.
pub fn day_summary(db: &Db, date: &str) -> Option<Event> {
    let lines: Vec<crate::report::TradeLine> = db.closed_trades_all().iter().filter(|p| crate::timefmt::date_local(p.exit_ms.unwrap_or(p.entry_ms)) == date).map(crate::report::TradeLine::from_position).collect();
    if lines.is_empty() {
        return None;
    }
    let s = crate::report::summarize(date, &lines, 1.0);
    Some(Event::DailySummary { date: date.into(), trades: s.trades, wins: s.wins, losses: s.losses, pnl_sol: s.total_pnl_sol, fees_sol: s.fees_sol, win_rate_pct: s.win_rate_pct })
}

/// Once per day, after midnight: send yesterday's summary (only if there were trades, only the latest missed day).
pub fn maybe_daily_summary(db: &Db, now_ms: i64) {
    let yesterday = crate::timefmt::date_local(now_ms - 86_400_000);
    let last = db.kv_get("notify.last_summary_day").unwrap_or_default();
    if last >= yesterday {
        return;
    }
    db.kv_set("notify.last_summary_day", &yesterday);
    if let Some(e) = day_summary(db, &yesterday) {
        emit(e);
    }
}

/// After a restart: if the version changed since the last start, the update was installed.
pub fn on_app_start(db: &Db, version: &str, now_ms: i64) {
    let last = db.kv_get("app.last_version");
    db.kv_set("app.last_version", version);
    if let Some(l) = last.filter(|l| l != version) {
        emit(Event::UpdateInstalled { ts: now_ms, version: version.to_string() });
        db.kv_set("app.updated_from", &l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;

    const DISCORD: &str = "https://discord.com/api/webhooks/1234567890123/AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-abcd";

    fn sell(pnl: f64) -> Event {
        Event::Sell { ts: 1_795_000_000_000, symbol: "CAT".into(), mint: "MintAAA111".into(), lane: "momentum".into(), pnl_sol: pnl, pnl_pct: pnl / 0.1 * 100.0, exit_reason: "trailing-stop".into(), hold_seconds: 95, fees_sol: 0.0012 }
    }

    #[test]
    fn webhook_urls_are_validated() {
        assert!(validate_discord_url(DISCORD).is_ok());
        assert!(validate_discord_url(&DISCORD.replace("discord.com", "discordapp.com")).is_ok());
        for bad in ["http://discord.com/api/webhooks/1/abc", "https://evil.com/api/webhooks/1234567890123/AbCdEfGhIjKlMnOpQrStUvWx", "https://discord.com.evil.com/api/webhooks/1234567890123/AbCdEfGhIjKlMnOpQrStUvWx", "https://discord.com/api/webhooks/abc/AbCdEfGhIjKlMnOpQrStUvWx", "https://discord.com/api/webhooks/1234567890123/short", "https://discord.com/api/webhooks/1234567890123/AbCdEfGhIjKlMnOpQrStUvWx?x=1", "https://discord.com/api/webhooks/1234567890123/AbCdEfGhIjKlMnOpQrStUvWx/extra", ""] {
            assert!(validate_discord_url(bad).is_err(), "{bad}");
        }
        assert!(validate_webhook_url("https://hooks.zapier.com/hooks/catch/1/abc").is_ok());
        assert!(validate_webhook_url("http://localhost:5678/webhook/x").is_ok());
        for bad in ["http://example.com/x", "ftp://a.com", "https://user:pw@example.com/x", "https://a.com/ b", "javascript:alert(1)", "https://"] {
            assert!(validate_webhook_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn secrets_are_masked_in_hints_and_errors() {
        assert_eq!(tail_hint("abcdefghijklmnop1234"), "saved, ends in ...1234");
        assert_eq!(tail_hint("short"), "saved");
        let h = url_hint(DISCORD);
        assert!(h.contains("discord.com") && h.ends_with("abcd") && !h.contains("AbCdEf"), "{h}");
        let secrets = vec![DISCORD.to_string(), "hunter2-secret-header-value".to_string()];
        let msg = format!("error sending to {DISCORD} with header hunter2-secret-header-value failed");
        let r = redact(&msg, &secrets);
        assert!(!r.contains("AbCdEfGhIj") && !r.contains("hunter2"), "{r}");
        assert!(r.contains("***"));
    }

    #[test]
    fn payloads_come_from_the_event_data() {
        let win = discord_payload(&sell(0.018), true);
        let e = &win["embeds"][0];
        assert_eq!(e["color"], 0x2ecc71);
        assert_eq!(e["title"], "Win CAT");
        assert_eq!(e["url"], "https://dexscreener.com/solana/MintAAA111");
        let flat = e["fields"].to_string();
        assert!(flat.contains("+0.0180 SOL") && flat.contains("trailing-stop") && flat.contains("95 s"), "{flat}");
        assert_eq!(e["footer"]["text"], "Paper results (pretend money)");
        assert_eq!(discord_payload(&sell(-0.02), true)["embeds"][0]["color"], 0xe74c3c);
        let d = Event::DailySummary { date: "2026-11-27".into(), trades: 5, wins: 3, losses: 2, pnl_sol: 0.0412, fees_sol: 0.01, win_rate_pct: 60.0 };
        let day = discord_payload(&d, true);
        assert_eq!(day["embeds"][0]["title"], "Daily summary 2026-11-27");
        let flat = day["embeds"][0]["fields"].to_string();
        assert!(flat.contains("5 (3 wins, 2 losses)") && flat.contains("+0.0412 SOL") && flat.contains("60%"), "{flat}");
        assert!(day["embeds"][0]["footer"]["text"].as_str().unwrap().contains("Paper results"), "paper label on every message while paper");
        assert!(!discord_payload(&d, false)["embeds"][0]["footer"]["text"].as_str().unwrap().contains("Paper"), "no label once live exists");
        let w = webhook_payload(&sell(0.018), true, 1_795_000_000_000);
        assert_eq!(w["schema"], WEBHOOK_SCHEMA);
        assert_eq!((w["event"].as_str(), w["mode"].as_str(), w["paper"].as_bool()), (Some("sell"), Some("paper"), Some(true)));
        assert_eq!(w["data"]["symbol"], "CAT");
        assert_eq!(w["data"]["pnl_sol"], 0.018);
        assert_eq!(w["links"]["dexscreener"], "https://dexscreener.com/solana/MintAAA111");
    }

    // -------------------------------------------------------------- queue, pacing, failures

    struct Mock {
        calls: Mutex<Vec<(String, Instant, String)>>,
        script: Mutex<Vec<Result<Reply, String>>>,
    }
    impl Mock {
        fn new(script: Vec<Result<Reply, String>>) -> Arc<Mock> {
            Arc::new(Mock { calls: Mutex::new(vec![]), script: Mutex::new(script) })
        }
        fn ok() -> Reply {
            Reply { status: 204, body: String::new(), retry_after: None }
        }
    }
    #[async_trait::async_trait]
    impl Transport for Mock {
        async fn post(&self, url: &str, _h: Vec<(String, String)>, body: String) -> Result<Reply, String> {
            self.calls.lock().unwrap().push((url.to_string(), Instant::now(), body));
            let mut s = self.script.lock().unwrap();
            if s.is_empty() { Ok(Mock::ok()) } else { s.remove(0) }
        }
    }

    fn setup(c: Conn, script: Vec<Result<Reply, String>>, gap_scale: f64) -> (Arc<Db>, Arc<ConnSecrets>, Arc<Mock>, Router) {
        let db = Arc::new(Db::open(":memory:").unwrap());
        let sec = Arc::new(ConnSecrets::in_memory());
        let mut f = BTreeMap::new();
        match c {
            Conn::Discord => f.insert("discord-url".to_string(), DISCORD.to_string()),
            Conn::Webhook => f.insert("webhook-url".to_string(), "https://hooks.example.com/abc".to_string()),
        };
        save(&db, &sec, c, &f).unwrap();
        let m = Mock::new(script);
        let r = Router::start(db.clone(), sec.clone(), true, m.clone(), gap_scale, 0.01);
        (db, sec, m, r)
    }

    async fn settle(m: &Mock, n: usize) {
        for _ in 0..400 {
            if m.calls.lock().unwrap().len() >= n {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn sends_are_paced_and_a_429_waits_for_retry_after() {
        let (_db, _s, m, r) = setup(Conn::Discord, vec![Ok(Reply { status: 429, body: r#"{"retry_after": 2.0}"#.into(), retry_after: None })], 0.1);
        r.emit(sell(0.01));
        r.emit(sell(-0.01));
        settle(&m, 3).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let c = m.calls.lock().unwrap();
        assert_eq!(c.len(), 3, "first try, the retry after the 429, then the second message");
        assert!(c[1].1 - c[0].1 >= Duration::from_millis(18), "the retry waited for retry_after (scaled 0.01 x 2 s)");
        assert!(c[2].1 - c[1].1 >= Duration::from_millis(100), "the next message waited for the pacing gap (scaled 1.2 s x 0.1)");
        assert!(c[0].2.contains("Win CAT") && c[2].2.contains("Loss CAT"), "queue keeps the order");
    }

    #[tokio::test]
    async fn a_failing_service_never_blocks_the_caller_and_the_error_is_masked() {
        let fail = || Err(format!("error sending request for url ({DISCORD})"));
        let (db, _s, m, r) = setup(Conn::Discord, (0..400).map(|_| fail()).collect(), 0.01);
        let t0 = Instant::now();
        for _ in 0..200 {
            r.emit(sell(0.01));
        }
        assert!(t0.elapsed() < Duration::from_millis(250), "emit only queues: 200 emits took {:?}", t0.elapsed());
        settle(&m, 3).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let err = db.kv_get("notify.err.discord").unwrap_or_default();
        assert!(err.contains("gave up after 3 tries"), "{err}");
        assert!(!err.contains("AbCdEfGhIj") && !err.contains(DISCORD), "{err}");
        assert!(m.calls.lock().unwrap().len() < 200, "a full queue drops messages instead of waiting");
    }

    #[tokio::test]
    async fn only_configured_connectors_with_the_event_switched_on_get_it() {
        let (db, _s, m, r) = setup(Conn::Discord, vec![], 0.01);
        let mut ev = Events::default();
        ev.sell = false;
        set_events(&db, Conn::Discord, ev);
        r.emit(sell(0.01));
        r.emit(Event::KillSwitch { ts: 1, on: true, source: "dashboard".into() });
        settle(&m, 1).await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let c = m.calls.lock().unwrap();
        assert_eq!(c.len(), 1);
        assert!(c[0].2.contains("Kill switch ON"));
        drop(c);
        // the webhook was never set up: nothing goes there
        assert!(!NotifySettings::load(&db).webhook.configured);
    }

    #[tokio::test]
    async fn saving_keeps_secrets_out_of_settings_and_status_shows_only_hints() {
        let db = Arc::new(Db::open(":memory:").unwrap());
        let sec = ConnSecrets::in_memory();
        let f: BTreeMap<String, String> = [("discord-url".to_string(), DISCORD.to_string())].into();
        save(&db, &sec, Conn::Discord, &f).unwrap();
        let settings_json = db.kv_get(SETTINGS_KEY).unwrap();
        assert!(!settings_json.contains("AbCdEfGhIj") && !settings_json.contains("webhooks"), "no secret in SQLite");
        let s = status(&db, &sec, Conn::Discord, 0);
        assert!(s.configured);
        let shown = serde_json::to_string(&s).unwrap();
        assert!(!shown.contains("AbCdEfGhIj") && shown.contains("...abcd"), "{shown}");
        assert!(save(&db, &sec, Conn::Discord, &[("discord-url".to_string(), "https://evil.com/x".to_string())].into()).is_err());
        assert!(sec.get("discord-url").unwrap().contains("discord.com"), "a rejected save keeps the old secret");
        remove(&db, &sec, Conn::Discord);
        assert!(!status(&db, &sec, Conn::Discord, 0).configured && sec.get("discord-url").is_none());
    }

    #[tokio::test]
    async fn the_test_button_reports_a_failure_without_leaking_the_secret() {
        let db = Db::open(":memory:").unwrap();
        let sec = ConnSecrets::in_memory();
        save(&db, &sec, Conn::Discord, &[("discord-url".to_string(), DISCORD.to_string())].into()).unwrap();
        let bad = Mock::new(vec![Ok(Reply { status: 404, body: format!(r#"{{"message":"Unknown Webhook {DISCORD}"}}"#), retry_after: None })]);
        let e = send_test(&db, &sec, Conn::Discord, true, bad.as_ref(), 0.01).await.unwrap_err();
        assert!(e.contains("404") && !e.contains("AbCdEfGhIj"), "{e}");
        let good = Mock::new(vec![]);
        assert!(send_test(&db, &sec, Conn::Discord, true, good.as_ref(), 0.01).await.is_ok());
        assert!(good.calls.lock().unwrap()[0].2.contains("Test message"));
        assert!(send_test(&db, &sec, Conn::Webhook, true, good.as_ref(), 0.01).await.is_err(), "not saved yet");
    }

    #[test]
    fn day_summary_and_update_events_use_real_rows_only() {
        let db = Db::open(":memory:").unwrap();
        assert!(day_summary(&db, "2026-11-27").is_none(), "no trades, no summary");
        // app start: first run records the version, a changed version is an installed update
        on_app_start(&db, "0.1.5", 1);
        assert_eq!(db.kv_get("app.last_version").as_deref(), Some("0.1.5"));
        assert_eq!(db.kv_get("app.updated_from"), None);
        on_app_start(&db, "0.1.6", 2);
        assert_eq!(db.kv_get("app.updated_from").as_deref(), Some("0.1.5"));
    }
}
