//! Engine orchestrator (M2): Memecoin Monitor -> Scam Shield -> Token Picker, with events for the UI.
//! Paper only: nothing here signs or sends a transaction.

use crate::config::Config;
use crate::db::{Db, DecisionRow};
use crate::events::*;
use crate::hub::Hub;
use crate::monitor::{self, Incoming, MonitorState};
use crate::picker;
use crate::pump::{solscan_token, FeedMsg, Trade};
use crate::rpc::{now_ms, Rpc, TradeFeed};
use crate::shield::rules::{ev, should_blacklist_creator, Outcome};
use crate::learning::{self, EntryFeatures, MISSED_MIN_SCORE, MISSED_WINDOW_MS};
use crate::trader::{Pick, Trader};
use crate::shield::{decide, ChainFacts, Decision, RugGate, RugOutcome, Shield, ShieldOpts, FALLBACK_SIZE_FACTOR, RUG_FALLBACK_NOTE};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Semaphore};

#[derive(Debug, Clone)]
pub struct EngineOpts {
    pub config: Config,
    pub db_path: String,
    pub rpc_url: String,
    pub feed_url: String,
    /// Sandbox only: metadata hosts blocked by the network policy count as a flagged pass instead of undecided.
    pub relax_egress: bool,
    pub sol_usd: Option<f64>,
    /// Where paper-trade JSONL and daily summaries go.
    pub logs_dir: String,
    /// Manual restart of a halted lane.
    pub clear_halt: bool,
    /// Run a scripted market instead of the live feeds (proves a full trade through the same engine).
    pub replay: Option<String>,
    /// Folder for the encrypted key file (only used if the OS vault fails). Default: next to the database.
    pub secrets_dir: Option<String>,
    /// True when CT_RPC_URL pinned the endpoint, so a saved key must not replace it.
    pub rpc_pinned: bool,
    /// Local address for the Chrome extension bridge, e.g. "127.0.0.1:8766". None = bridge off.
    pub bridge_addr: Option<String>,
}

impl EngineOpts {
    pub fn new(config: Config, db_path: &str) -> Self {
        // The Helius key (later) comes from env/secure storage, never from source. Public RPC is the default.
        let rpc_url = std::env::var("CT_RPC_URL").ok().filter(|s| !s.is_empty()).or_else(|| {
            std::env::var("HELIUS_API_KEY").ok().filter(|k| !k.is_empty()).map(|k| format!("https://mainnet.helius-rpc.com/?api-key={k}"))
        });
        EngineOpts {
            config,
            db_path: db_path.into(),
            rpc_url: rpc_url.unwrap_or_else(|| crate::rpc::PUBLIC_RPC.to_string()),
            feed_url: monitor::PUMPPORTAL_WS.into(),
            relax_egress: false,
            sol_usd: None,
            logs_dir: "paper-logs".into(),
            clear_halt: false,
            replay: None,
            secrets_dir: None,
            rpc_pinned: std::env::var("CT_RPC_URL").ok().filter(|s| !s.is_empty()).is_some(),
            bridge_addr: None,
        }
    }

    /// The folder for secrets that fall back to an encrypted file.
    pub fn secrets_path(&self) -> std::path::PathBuf {
        match &self.secrets_dir {
            Some(d) => std::path::PathBuf::from(d),
            None => std::path::Path::new(&self.db_path).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf()).unwrap_or_else(|| std::path::PathBuf::from(".")),
        }
    }

    /// Where the encrypted key file would live.
    pub fn key_store(&self) -> crate::secrets::KeyStore {
        crate::secrets::KeyStore::system(self.secrets_path())
    }

    /// Startup: if no environment override picked an endpoint, use the key saved in secure storage.
    /// The environment variable HELIUS_API_KEY and CT_RPC_URL still win (see `EngineOpts::new`).
    pub fn with_saved_key(mut self) -> Self {
        if !self.rpc_pinned && std::env::var("HELIUS_API_KEY").ok().filter(|k| !k.is_empty()).is_none() {
            if let Some(k) = self.key_store().load() {
                self.rpc_url = crate::secrets::helius_rpc_url(&k);
            }
        }
        self
    }
}

/// Paper fill realism presets (Settings). Harsher is safer for judging the strategy: it makes pretend trades
/// fail and slip more, like real ones often do. "Realistic" is the shipped default.
pub fn fill_preset(name: &str) -> Option<crate::paper::FillParams> {
    use crate::paper::FillParams;
    Some(match name {
        "optimistic" => FillParams { buy_fail_pct: 5.0, sell_fail_pct: 8.0, delay_ms_min: 200, delay_ms_max: 600, slippage_cap_pct: 25.0 },
        "realistic" => FillParams { buy_fail_pct: 10.0, sell_fail_pct: 15.0, delay_ms_min: 400, delay_ms_max: 1200, slippage_cap_pct: 15.0 },
        "harsh" => FillParams { buy_fail_pct: 20.0, sell_fail_pct: 25.0, delay_ms_min: 800, delay_ms_max: 2500, slippage_cap_pct: 10.0 },
        _ => return None,
    })
}

/// Settings the user changed in the app (stored in SQLite, never in config.json): today the paper fill realism.
pub fn apply_user_overrides(cfg: &mut Config, db: &Db) {
    if let Some(p) = db.kv_get("paperfill").and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok()) {
        let f = |k: &str, d: f64| p[k].as_f64().unwrap_or(d);
        let cand = crate::paper::FillParams { buy_fail_pct: f("buyFailPct", cfg.paper_fill.buy_fail_pct), sell_fail_pct: f("sellFailPct", cfg.paper_fill.sell_fail_pct), delay_ms_min: f("delayMsMin", cfg.paper_fill.delay_ms_min as f64) as u32, delay_ms_max: f("delayMsMax", cfg.paper_fill.delay_ms_max as f64) as u32, slippage_cap_pct: f("slippageCapPct", cfg.paper_fill.slippage_cap_pct) };
        if cand.check().is_ok() {
            cfg.paper_fill.buy_fail_pct = cand.buy_fail_pct;
            cfg.paper_fill.sell_fail_pct = cand.sell_fail_pct;
            cfg.paper_fill.delay_ms_min = cand.delay_ms_min;
            cfg.paper_fill.delay_ms_max = cand.delay_ms_max;
            cfg.paper_fill.slippage_cap_pct = cand.slippage_cap_pct;
        }
    }
}

/// The config the app really runs on: shipped values, the user's paper fill choice, and applied learning changes.
pub fn effective_config(base: &Config, db: &Arc<Db>) -> Config {
    let mut c = base.clone();
    apply_user_overrides(&mut c, db);
    learning::Learner::new(db.clone(), None, &c).current_main().cfg
}

/// Validate and store a paper fill choice (a preset name or custom numbers), then return the values.
pub fn save_paper_fill(db: &Db, json: &str) -> Result<crate::paper::FillParams, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| "Could not read those values.".to_string())?;
    let p = if let Some(n) = v["preset"].as_str() {
        fill_preset(n).ok_or("Unknown preset.")?
    } else {
        let g = |k: &str| v[k].as_f64().ok_or_else(|| "Every number is needed.".to_string());
        crate::paper::FillParams { buy_fail_pct: g("buyFailPct")?, sell_fail_pct: g("sellFailPct")?, delay_ms_min: g("delayMsMin")? as u32, delay_ms_max: g("delayMsMax")? as u32, slippage_cap_pct: g("slippageCapPct")? }
    };
    p.check()?;
    let preset = v["preset"].as_str().unwrap_or("custom");
    db.kv_set("paperfill", &serde_json::json!({"preset": preset, "buyFailPct": p.buy_fail_pct, "sellFailPct": p.sell_fail_pct, "delayMsMin": p.delay_ms_min, "delayMsMax": p.delay_ms_max, "slippageCapPct": p.slippage_cap_pct}).to_string());
    Ok(p)
}

pub struct Ctx {
    cfg: Config,
    links: std::sync::Mutex<std::collections::HashMap<String, crate::shield::rules::SocialLinks>>,
    shield: Shield,
    xmon: Arc<crate::xmon::XMon>,
    trader: Arc<Trader>,
    hub: Arc<Hub>,
    db: Arc<Db>,
    mon: Arc<MonitorState>,
    chain_slots: Semaphore,
    checking: AtomicU32,
    observing: AtomicU32,
    picked: AtomicU32,
}

impl Ctx {
    /// One link to show next to a coin (its website, else X, else Telegram), if its metadata has one.
    fn source_for(&self, mint: &str) -> Option<String> {
        self.links.lock().unwrap().get(mint).and_then(|l| l.best().map(String::from))
    }
}

fn status(hub: &Hub, agent: AgentId, color: StatusColor, line: impl Into<String>) {
    hub.emit(EngineEvent::Status(AgentStatus { agent, color, line: line.into() }));
}

async fn fetch_sol_usd() -> Option<f64> {
    let c = crate::net::http_client();
    let v: serde_json::Value = c
        .get("https://api.dexscreener.com/tokens/v1/solana/So11111111111111111111111111111111111111112")
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    v.as_array()?.iter().filter_map(|p| p["priceUsd"].as_str()?.parse::<f64>().ok()).next()
}

pub async fn run(opts: EngineOpts, hub: Arc<Hub>, stop: watch::Receiver<bool>) -> Result<(), String> {
    let db = Arc::new(Db::open(&opts.db_path)?);
    // outgoing connectors (Discord, generic webhook): queues and workers, nothing is sent until set up in Settings
    crate::notify::init(db.clone(), Arc::new(crate::notify::ConnSecrets::system(opts.secrets_path())), opts.config.mode == crate::config::Mode::Paper);
    let mut opts = opts;
    apply_user_overrides(&mut opts.config, &db);
    let rpc = Arc::new(Rpc::new(&opts.rpc_url, 6.0));
    hub.log(format!("engine: rpc endpoint {}", rpc.masked_url()));
    let sol_usd = match opts.sol_usd {
        Some(p) => p,
        None => fetch_sol_usd().await.unwrap_or(150.0),
    };
    hub.log(format!("engine: SOL price used for USD conversion: ${sol_usd:.2}"));
    let hub_log = hub.clone();
    let feed = if opts.replay.is_some() { TradeFeed::offline() } else { TradeFeed::spawn(rpc.ws_url(), Arc::new(move |s| hub_log.log(s))) };
    let trader = Trader::new(opts.config.clone(), db.clone(), hub.clone(), feed, if opts.replay.is_some() { None } else { Some(rpc.clone()) }, sol_usd, std::path::PathBuf::from(&opts.logs_dir), opts.replay.is_some());
    if opts.clear_halt {
        trader.clear_halts();
    }
    {
        let tr = trader.clone();
        let rpc2 = rpc.clone();
        let hub2 = hub.clone();
        let store = opts.key_store();
        let db2 = db.clone();
        let pinned = opts.rpc_pinned;
        let replay = opts.replay.is_some();
        hub.set_command_handler(Arc::new(move |cmd| match cmd {
            "kill" => tr.kill("dashboard"),
            "resume" => tr.resume_trading("dashboard"),
            "kill-state" => tr.kill_state().text,
            // before an update is installed: sell open positions the normal way (not the persistent kill switch)
            "update-close" => tr.close_for_update(),
            "update-close-cancel" => tr.cancel_update_close(),
            "open-count" => tr.open_count().to_string(),
            // the Helius key was saved or removed: switch the RPC and the trade feed to the new endpoint, no restart
            "reload-rpc" => {
                if pinned || replay {
                    return "endpoint is pinned by CT_RPC_URL (or a replay), not changed".into();
                }
                let url = crate::secrets::rpc_url_for(store.effective_key().as_deref());
                rpc2.set_url(&url);
                tr.feed.set_ws_url(&rpc2.ws_url());
                hub2.log(format!("engine: rpc endpoint switched to {}", rpc2.masked_url()));
                format!("now using {}", rpc2.masked_url())
            }
            c if c.starts_with("set-paper-fill:") => match save_paper_fill(&db2, &c["set-paper-fill:".len()..]) {
                Ok(p) => {
                    tr.exec.set_fill_params(p);
                    hub2.log(format!("engine: paper fill realism changed: buy fail {}%, sell fail {}%, delay {}-{} ms, sell slippage cap {}%", p.buy_fail_pct, p.sell_fail_pct, p.delay_ms_min, p.delay_ms_max, p.slippage_cap_pct));
                    "saved".into()
                }
                Err(e) => e,
            },
            c if c.starts_with("undo-change:") || c.starts_with("approve-change:") => tr.learning_command(c),
            _ => format!("unknown command {cmd}"),
        }));
    }
    trader.restore_learning();
    {
        let mut checks = trader.exec.skipped_checks();
        if opts.relax_egress {
            checks.push(crate::exec::SkippedCheck { name: "Coin links and image check".into(), why: "This computer's network blocks the sites that hold coin links, so those checks are flagged instead of blocking. This is a test setting and is off in the real app.".into() });
        }
        hub.emit(EngineEvent::Skipped { executor: trader.exec.name().into(), checks });
    }
    let resumed = trader.resume();
    if trader.is_killed() {
        hub.log("engine: KILL SWITCH is on (saved in SQLite): no new buys. Open positions are being sold. Resume trading needs an explicit resume.");
    }
    if resumed > 0 {
        hub.log(format!("engine: resumed {resumed} open paper position(s) from SQLite"));
    }
    trader.spawn_ticker(stop.clone());
    // Chrome extension bridge: it gets its own Shield for "Check this coin" and no handle to the trader at all.
    // ONE RugCheck client for the whole app (2 in flight, rate limited, cached per mint).
    let rug_client = Arc::new(crate::shield::net::RugCheck::new());
    // X Monitor + Thesis Finder: the extension reads x.com and sends results over the bridge (SPEC 4e). No handle to the trader.
    let xmon = crate::xmon::XMon::new(db.clone(), hub.clone(), opts.config.x_monitor.clone());
    if let Some(addr) = &opts.bridge_addr {
        let checker = Arc::new(crate::bridge::ShieldChecker { shield: Arc::new(Shield::with_rugcheck(opts.config.clone(), db.clone(), rpc.clone(), ShieldOpts { relax_egress: opts.relax_egress }, rug_client.clone())), hub: hub.clone(), db: db.clone() });
        let bridge = crate::bridge::Bridge::new(hub.clone(), db.clone(), checker);
        bridge.set_xmon(xmon.clone());
        match bridge.serve(addr).await {
            Ok(a) => hub.log(format!("engine: Chrome extension bridge on ws://{a} (pinned extension only, pairing required)")),
            Err(e) => hub.log(format!("engine: Chrome extension bridge NOT started: {e}")),
        }
    }
    let shield = Shield::with_rugcheck(opts.config.clone(), db.clone(), rpc, ShieldOpts { relax_egress: opts.relax_egress }, rug_client.clone());
    let mon = Arc::new(MonitorState::default());
    let ctx = Arc::new(Ctx {
        links: Default::default(),
        cfg: opts.config.clone(),
        shield,
        xmon: xmon.clone(),
        trader: trader.clone(),
        hub: hub.clone(),
        db: db.clone(),
        mon: mon.clone(),
        chain_slots: Semaphore::new(6),
        checking: AtomicU32::new(0),
        observing: AtomicU32::new(0),
        picked: AtomicU32::new(0),
    });

    status(&hub, AgentId::TopGainers, StatusColor::Grey, "Not active yet (later version)");
    xmon.tick(now_ms());
    status(&hub, AgentId::TopTraderMonitor, StatusColor::Grey, "Off in version 1");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let _keep_tx;
    if let Some(name) = &opts.replay {
        let sc = crate::replay::scenario(name).ok_or_else(|| format!("unknown replay scenario {name:?} (use hot or dump)"))?;
        mon.connected.store(true, Ordering::Relaxed);
        mon.last_msg_ms.store(now_ms(), Ordering::Relaxed);
        hub.log(format!("REPLAY: scripted market {:?}. Every number below is replayed, not live.", sc.name));
        tokio::spawn(replay_drive(ctx.clone(), sc));
        _keep_tx = Some(tx);
    } else {
        _keep_tx = None;
        tokio::spawn(monitor::run(opts.feed_url.clone(), tx, mon.clone(), hub.clone(), stop.clone()));
    }

    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut health = crate::health::HealthTracker::default();
    let mut last_warn = String::new();
    let mut stop = stop;
    let day_start = |n: i64| n - n.rem_euclid(86_400_000);
    loop {
        tokio::select! {
            _ = stop.changed() => { if *stop.borrow() { break; } }
            m = rx.recv() => match m {
                Some(Incoming::Create(t)) => { tokio::spawn(process_token(ctx.clone(), t)); }
                Some(Incoming::Migrate(t)) => {
                    trader.on_migration(&t.mint);
                    hub.emit(EngineEvent::Activity { ts: now_ms(), agent: AgentId::MemecoinMonitor, text: format!("Migration to the DEX seen: {}", short_mint(&t.mint)), mint: Some(t.mint.clone()), source_url: None });
                }
                None => break,
            },
            _ = tick.tick() => {
                let now = now_ms();
                crate::notify::maybe_daily_summary(&ctx.db, now);
                // Memecoin Monitor: the new-coin feed AND the buy/sell data (a coin scores 0 without it)
                let created = mon.created.load(Ordering::Relaxed);
                let fh = feed_health(&ctx, &mut health, &rug_client, now);
                hub.emit(EngineEvent::FeedHealth(fh.clone()));
                if mon.connected.load(Ordering::Relaxed) {
                    let ago = (now - mon.last_msg_ms.load(Ordering::Relaxed)) / 1000;
                    if fh.warn != last_warn {
                        if !fh.warn.is_empty() {
                            crate::vault::situation(&ctx.db, now, "feed-warning", "Feed health warning", &fh.warn, "");
                            crate::notify::emit(crate::notify::Event::Health { ts: now, kind: "feed".into(), text: fh.warn.clone() });
                        }
                        last_warn = fh.warn.clone();
                    }
                    if !fh.warn.is_empty() {
                        status(&hub, AgentId::MemecoinMonitor, StatusColor::Red, "No buy/sell data arriving: every coin scores 0");
                    } else if fh.watched == 0 {
                        status(&hub, AgentId::MemecoinMonitor, StatusColor::Green, format!("Live: {created} new coins seen, last message {ago} s ago"));
                    } else {
                        status(&hub, AgentId::MemecoinMonitor, StatusColor::Green, format!("Live. {} trades/min on {} coins", fh.trades_last_min, fh.watched));
                    }
                } else if mon.no_buys() {
                    status(&hub, AgentId::MemecoinMonitor, StatusColor::Red, "Feed down for over 5 s: NO BUYS until it reconnects");
                } else {
                    status(&hub, AgentId::MemecoinMonitor, StatusColor::Amber, "Connecting to the live feed");
                }
                let (checked, blocked, passed, by) = db.summary(day_start(now));
                hub.emit(EngineEvent::ShieldSummary { ts: now, checked, blocked, passed, by_reason: by });
                hub.emit(EngineEvent::Funnel(funnel_view(&ctx, day_start(now), now)));
                ctx.xmon.tick(now);
                let chk = ctx.checking.load(Ordering::Relaxed);
                status(&hub, AgentId::ScamShield, if chk > 0 { StatusColor::Green } else { StatusColor::Amber }, format!("Checking {chk} coins. Today: {blocked} blocked, {passed} passed"));
                let obs = ctx.observing.load(Ordering::Relaxed);
                status(&hub, AgentId::TokenPicker, if obs > 0 { StatusColor::Green } else { StatusColor::Amber }, format!("Watching {obs} coins for 60 s each. {} picked so far", ctx.picked.load(Ordering::Relaxed)));
            }
        }
    }
    Ok(())
}

/// Numbers for the health line. `watched` counts coins we are collecting trades for.
fn feed_health(ctx: &Ctx, tracker: &mut crate::health::HealthTracker, rug: &crate::shield::net::RugCheck, now: i64) -> FeedHealthView {
    use std::sync::atomic::Ordering::Relaxed;
    let feed = &ctx.trader.feed;
    let st = &feed.stats;
    let watched = feed.store.active() as u32;
    let trades_connected = feed.connected.load(Relaxed);
    let last = st.last_trade_ms.load(Relaxed);
    let warn = tracker.warn(now, watched, trades_connected, last);
    FeedHealthView {
        ts: now,
        coins_connected: ctx.mon.connected.load(Relaxed),
        trades_connected,
        watched,
        subs_active: st.subs_active.load(Relaxed) as u32,
        trades_last_min: st.trades_within(now, 60_000),
        trades_total: st.trades_total.load(Relaxed),
        last_trade_ms: last,
        last_error: st.last_error(),
        warn,
        rug_requests: rug.requests.load(Relaxed),
        rug_failures: rug.failures.load(Relaxed),
    }
}

fn funnel_view(ctx: &Ctx, since: i64, now: i64) -> FunnelView {
    let f = ctx.db.funnel(since, 10);
    FunnelView {
        ts: now,
        in_progress: f.seen.saturating_sub(f.blocked + f.scored),
        seen: f.seen,
        blocked: f.blocked,
        blocked_reasons: f.blocked_reasons,
        scored: f.scored,
        scored_no_trades: f.scored_no_trades,
        best_score: f.best_score,
        best_symbol: f.best_symbol,
        min_score: ctx.cfg.observation.min_score,
        best_blocked_by: f.best_blocked_by,
        picked: f.picked,
        bought: f.bought,
        rug_fallback_passed: f.rug_fallback_passed,
        rug_fallback_blocked: f.rug_fallback_blocked,
    }
}

fn short_mint(m: &str) -> String {
    crate::shield::rules::short(m)
}

fn flags(d: &Decision) -> String {
    let f: Vec<String> = d.results.iter().filter(|r| r.outcome == Outcome::NotAvailable || r.outcome == Outcome::Undecided || r.code == "RUGCHECK_WARN").map(|r| format!("{}: {}", r.name, r.detail)).collect();
    if f.is_empty() { String::new() } else { format!(" Flags: {}", f.join(" | ")) }
}

/// Persist the stage decision and tell the UI.
fn record(ctx: &Ctx, t: &FeedMsg, stage: &str, d: &Decision) {
    let now = now_ms();
    let (outcome, reason, detail, mut evidence) = match &d.block {
        Some(b) => ("block", b.code.clone(), b.detail.clone(), b.evidence.clone()),
        None if stage == "rugcheck-fallback" => ("pass", String::new(), format!("{RUG_FALLBACK_NOTE}. {} on-chain checks all passed; half trade size if bought.", d.results.len().saturating_sub(1)), vec![]),
        None => ("pass", String::new(), format!("{} checks done, none blocked.{}", d.results.len(), flags(d)), vec![]),
    };
    if evidence.is_empty() {
        evidence = d.results.iter().flat_map(|r| r.evidence.clone()).take(3).collect();
    }
    if evidence.is_empty() {
        evidence.push(ev("Token on Solscan", solscan_token(&t.mint)));
    }
    if stage == "rugcheck-fallback" {
        // RugCheck was down for this coin: one note per hour is enough
        let hour = (now / 3_600_000).to_string();
        if ctx.db.kv_get("vault.rug_hour").as_deref() != Some(hour.as_str()) {
            ctx.db.kv_set("vault.rug_hour", &hour);
            crate::notify::emit(crate::notify::Event::Health { ts: now, kind: "rugcheck".into(), text: format!("RugCheck could not rate {}; the bot used its own on-chain checks.", t.symbol) });
            crate::vault::situation(&ctx.db, now, "rugcheck-outage", "RugCheck outage", &format!("RugCheck could not rate {} (the on-chain fallback decided: {outcome}). {detail}", t.symbol), &t.mint);
        }
    }
    ctx.db.insert_decision(&DecisionRow { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: stage.into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into() });
    ctx.hub.emit(EngineEvent::Verdict { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: stage.into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into(), evidence });
    let text = if outcome == "block" { format!("Blocked {} at {stage}: {reason}. {detail}", t.symbol) } else { format!("{} passed {stage} ({})", t.symbol, d.verdict) };
    ctx.hub.emit(EngineEvent::Decision { ts: now, agent: AgentId::ScamShield, text: text.clone() });
    if outcome == "block" {
        ctx.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::ScamShield, text, mint: Some(t.mint.clone()), source_url: ctx.source_for(&t.mint) });
    }
}

fn pulse(ctx: &Ctx, from: AgentId, to: AgentId, text: String, mint: &str) {
    ctx.hub.emit(EngineEvent::Pulse { from, to, text: text.clone() });
    ctx.hub.emit(EngineEvent::Activity { ts: now_ms(), agent: from, text, mint: Some(mint.into()), source_url: ctx.source_for(mint) });
}

async fn process_token(ctx: Arc<Ctx>, t: FeedMsg) {
    let t0 = now_ms();
    ctx.db.upsert_token(&t.mint, t0, &t.name, &t.symbol, &t.trader_public_key, t.market_cap_sol);
    ctx.hub.emit(EngineEvent::Token { ts: t0, mint: t.mint.clone(), symbol: t.symbol.clone(), name: t.name.clone(), creator: t.trader_public_key.clone(), mcap_sol: t.market_cap_sol });
    pulse(&ctx, AgentId::MemecoinMonitor, AgentId::ScamShield, format!("New coin {} handed to Scam Shield", t.symbol), &t.mint);
    ctx.checking.fetch_add(1, Ordering::Relaxed);

    // Stage A: cheap, no network
    let a = decide(ctx.shield.stage_a(&t));
    record(&ctx, &t, "cheap-prefilter", &a);
    if a.block.is_some() {
        ctx.checking.fetch_sub(1, Ordering::Relaxed);
        return;
    }

    // Start collecting trades right away so the chain checks and the observation share the same data.
    ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
    // RugCheck runs in parallel from now on (retrying a busy or not-yet-indexed API); it is judged during the watch.
    let rug = ctx.shield.start_rugcheck(&t.mint);

    // Stage B: chain checks (bounded concurrency; waiting too long counts as an undecided required check)
    let (b, mut facts): (Decision, ChainFacts) = match tokio::time::timeout(Duration::from_secs(8), ctx.chain_slots.acquire()).await {
        Ok(Ok(_p)) => {
            let (res, facts) = ctx.shield.stage_b(&t).await;
            (decide(res), facts)
        }
        _ => {
            let r = crate::shield::rules::CheckResult::block("backlog", "BACKLOG", "Too many coins were being checked at once; this one was skipped.");
            (decide(vec![r]), ChainFacts::default())
        }
    };
    ctx.checking.fetch_sub(1, Ordering::Relaxed);
    facts.rug = Some(rug);
    if facts.links != crate::shield::rules::SocialLinks::default() {
        ctx.links.lock().unwrap().insert(t.mint.clone(), facts.links.clone());
        ctx.hub.emit(EngineEvent::Links { mint: t.mint.clone(), website: facts.links.website.clone(), twitter: facts.links.twitter.clone(), telegram: facts.links.telegram.clone() });
        // every coin with an X link is remembered (clone ranking, narratives); pages are only opened for coins that pass the checks below
        ctx.xmon.note_coin(&t.mint, &t.symbol, &t.name, &facts.links.twitter, now_ms());
    }
    record(&ctx, &t, "chain", &b);
    if b.block.is_some() {
        ctx.trader.release_feed(&t.mint);
        return;
    }
    // passed the cheap prefilter and the chain checks and has an X link: ask the extension to read it, while the 60 s watch runs
    if !facts.links.twitter.is_empty() {
        let n = ctx.xmon.queue_coin(&t.mint, now_ms());
        if n > 0 {
            pulse(&ctx, AgentId::TokenPicker, AgentId::XMonitor, format!("{}: {} X page(s) queued for the extension", t.symbol, n), &t.mint);
        }
    }
    pulse(&ctx, AgentId::ScamShield, AgentId::TokenPicker, format!("Clean coin {} handed to Token Picker", t.symbol), &t.mint);
    observe_and_trade(ctx, t, t0, facts).await;
}

/// Wait out the observation window while RugCheck's request runs in parallel. When its report arrives it is judged
/// at once: a block ends the watch early (returns the decision, already recorded).
async fn watch_window(ctx: &Ctx, t: &FeedMsg, facts: &mut ChainFacts, wait_ms: u64) -> Option<Decision> {
    let end = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
    let Some(mut rx) = facts.rug.clone() else {
        tokio::time::sleep_until(end).await;
        return None;
    };
    loop {
        if let Some(d) = ctx.shield.judge_report_if_ready(t, facts) {
            record(ctx, t, "rugcheck", &d);
            if d.block.is_some() {
                return Some(d);
            }
        }
        let settled = facts.rug_done.is_some() || matches!(rx.borrow().outcome, RugOutcome::Unavailable(_));
        if settled {
            tokio::time::sleep_until(end).await;
            return None;
        }
        tokio::select! {
            _ = tokio::time::sleep_until(end) => return None,
            r = rx.changed() => { if r.is_err() { tokio::time::sleep_until(end).await; return None; } }
        }
    }
}

/// Observation window, score, pre-buy checks, then the entry rule of each lane. Shared by live and replay runs.
pub async fn observe_and_trade(ctx: Arc<Ctx>, t: FeedMsg, t0: i64, mut facts: ChainFacts) {
    // Observation window
    ctx.observing.fetch_add(1, Ordering::Relaxed);
    let min_age = ctx.cfg.entry.min_age_seconds as i64 * 1000;
    let wait = (t0 + min_age - now_ms()).max(0) as u64;
    // Watch for the window. RugCheck's answer is judged the moment it arrives: a danger flag ends the watch at once.
    if watch_window(&ctx, &t, &mut facts, wait).await.is_some() {
        ctx.observing.fetch_sub(1, Ordering::Relaxed);
        ctx.trader.release_feed(&t.mint);
        return;
    }
    let now = now_ms();
    let age = (now - t0) as f64 / 1000.0;
    let trades = ctx.trader.feed.store.snapshot(&t.mint);
    let launch_price = if t.v_tokens_in_bonding_curve > 0.0 { t.v_sol_in_bonding_curve / t.v_tokens_in_bonding_curve } else { 0.0 };
    let obs = picker::observe(&trades, &t.trader_public_key, t.initial_buy, launch_price, (t.v_sol_in_bonding_curve - 30.0).max(0.0), age, now);
    // The main lane may run on learned thresholds (applied changes). The shadow lane runs a candidate change, if a test is on.
    let main_t = ctx.trader.lane_tuned("momentum");
    let mcfg = &main_t.cfg;
    // X Monitor: whatever the extension has read by now. Not ready, failed or stale means "unknown": no points and no block.
    let xe = ctx.xmon.evaluate(&t.mint, now);
    let sc = picker::with_x(picker::score_with(&obs, mcfg, &main_t.weights), &xe, mcfg.observation.min_score);
    let shadow_sc = if ctx.trader.shadow_active() {
        let st = ctx.trader.lane_tuned(learning::SHADOW_LANE);
        Some(picker::with_x(picker::score_with(&obs, &st.cfg, &st.weights), &xe, st.cfg.observation.min_score))
    } else {
        None
    };
    if !matches!(xe.status, crate::xmon::score::XStatus::NoLink | crate::xmon::score::XStatus::Off) {
        let blocked = xe.is_blocked();
        ctx.db.insert_decision(&DecisionRow { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: "x-monitor".into(), outcome: if blocked { "block" } else { "pass" }.into(), reason: if blocked { "X_BLOCK".into() } else { String::new() }, detail: crate::xmon::XMon::log_for(&xe).summary(), verdict: String::new() });
        if xe.status == crate::xmon::score::XStatus::Ready || blocked {
            ctx.hub.emit(EngineEvent::Pulse { from: AgentId::XMonitor, to: AgentId::TokenPicker, text: format!("{}: X community {}", t.symbol, if blocked { "blocked".to_string() } else { format!("{:.0} (+{} points)", xe.score, xe.points) }) });
        }
    }

    // serial rugger bookkeeping and "seen trending"
    let peak = trades.iter().map(|x| x.mcap_sol()).fold(0.0, f64::max);
    if should_blacklist_creator(peak, obs.mcap_sol, age) && obs.mcap_sol > 0.0 {
        ctx.db.blacklist_creator(&t.trader_public_key, &format!("{} fell over 70% from its peak within {age:.0} s", t.symbol), now);
    }
    if obs.mcap_sol >= ctx.cfg.feed.min_market_cap_sol {
        ctx.db.mark_trending(&t.mint);
    }

    let mut gates = sc.gates.clone();
    let mut picked = sc.picked;
    // The paper variant (dip-40pct) shares the observation gate (score) but not the momentum entry gates.
    let mut dip_eligible = picker::dip_ok(&sc, ctx.cfg.observation.min_score) && obs.dev_sold_pct <= 0.0;
    let mut shadow_picked = shadow_sc.as_ref().map(|s| s.picked).unwrap_or(false);
    if (picked || dip_eligible || shadow_picked) && ctx.mon.no_buys() {
        gates.push("feed down: no buys".into());
        picked = false;
        dip_eligible = false;
        shadow_picked = false;
    }
    if picked || dip_eligible || shadow_picked {
        let d = decide(ctx.shield.pre_buy(&trades, &facts).into_iter().collect());
        record(&ctx, &t, "pre-buy", &d);
        if d.block.is_some() {
            picked = false;
            dip_eligible = false;
            shadow_picked = false;
            gates.push(format!("Scam Shield pre-buy: {}", d.block.as_ref().map(|b| b.code.clone()).unwrap_or_default()));
        }
    }

    if picked || shadow_picked {
        // momentum lanes only (main and shadow): chase risk and falling knife (the dip-40pct lane is not affected)
        let d = decide(ctx.shield.pre_buy_momentum(&t, obs.mcap_sol, peak).into_iter().collect());
        if picked {
            record(&ctx, &t, "pre-buy", &d);
        }
        if let Some(b) = &d.block {
            if picked {
                gates.push(format!("Scam Shield pre-buy: {}", b.code));
            }
            picked = false;
            shadow_picked = false;
        }
    }
    // RugCheck verdict for a coin that would otherwise be picked: its report, or (no answer) our own on-chain checks at half size.
    let mut size_factor = 1.0;
    if picked || dip_eligible || shadow_picked {
        let (gate, recs) = ctx.shield.rug_gate(&t, &mut facts).await;
        for (stage, d) in &recs {
            record(&ctx, &t, stage, d);
        }
        match gate {
            RugGate::Full => {}
            RugGate::Half => {
                size_factor = FALLBACK_SIZE_FACTOR;
                ctx.hub.emit(EngineEvent::Activity { ts: now_ms(), agent: AgentId::ScamShield, text: format!("{}: {RUG_FALLBACK_NOTE}. If bought, half size.", t.symbol), mint: Some(t.mint.clone()), source_url: ctx.source_for(&t.mint) });
            }
            RugGate::Block(code) => {
                picked = false;
                dip_eligible = false;
                shadow_picked = false;
                gates.push(format!("Scam Shield: RugCheck / on-chain check ({code})"));
            }
        }
    }
    let mut blocked_by = gates.first().cloned().unwrap_or_default();
    if blocked_by.is_empty() && sc.score < mcfg.observation.min_score {
        blocked_by = format!("score below {}", mcfg.observation.min_score);
    }

    let now = now_ms();
    ctx.hub.emit(EngineEvent::Score {
        ts: now,
        mint: t.mint.clone(),
        symbol: t.symbol.clone(),
        score: sc.score,
        min_score: mcfg.observation.min_score,
        picked,
        buyers: obs.unique_buyers,
        buys: obs.buys,
        sells: obs.sells,
        ratio: obs.ratio,
        price_change_pct: obs.price_change_pct,
        curve_sol: obs.curve_sol,
        mcap_sol: obs.mcap_sol,
        gates: gates.clone(),
    });
    let reason = if picked { String::new() } else if sc.score < mcfg.observation.min_score { "LOW_SCORE".to_string() } else { "GATE".to_string() };
    let detail = if picked {
        format!("Score {:.1} (needs {}). All entry gates hold.", sc.score, mcfg.observation.min_score)
    } else {
        format!("Score {:.1} (needs {}). {}", sc.score, mcfg.observation.min_score, gates.join("; "))
    };
    ctx.db.insert_decision(&DecisionRow { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: "observation".into(), outcome: if picked { "pass" } else { "block" }.into(), reason, detail: detail.clone(), verdict: String::new() });
    ctx.hub.emit(EngineEvent::Decision { ts: now, agent: AgentId::TokenPicker, text: format!("{}: {detail}", t.symbol) });
    ctx.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::TokenPicker, text: format!("Scored {} at {:.1}{}", t.symbol, sc.score, if picked { ": picked" } else { "" }), mint: Some(t.mint.clone()), source_url: ctx.source_for(&t.mint) });
    ctx.observing.fetch_sub(1, Ordering::Relaxed);
    let gate_labels: Vec<String> = {
        let mut v: Vec<String> = gates.iter().map(|g| picker::gate_label(g).to_string()).collect();
        if v.is_empty() && !picked && sc.score < mcfg.observation.min_score {
            v.push(picker::gate_label("score below").to_string());
        }
        v.dedup();
        v
    };
    ctx.db.insert_score(now, &t.mint, &t.symbol, sc.score, picked, &gate_labels, trades.len() as u32, obs.unique_buyers);
    let features = EntryFeatures { score: sc.score, buyers: obs.unique_buyers, buys: obs.buys, sells: obs.sells, ratio: obs.ratio, curve_sol: obs.curve_sol, age_seconds: age, mcap_sol: obs.mcap_sol, picked_ms: now, source_url: ctx.source_for(&t.mint).unwrap_or_default(), x_community: Some(crate::xmon::XLog::from(&xe)) };
    let pick = Pick { mint: t.mint.clone(), symbol: t.symbol.clone(), creator: t.trader_public_key.clone(), curve_key: t.bonding_curve_key.clone(), score: sc.score, features, size_factor };
    // A coin that scored high and was still not bought is a "missed coin": watch what it did for 5 minutes.
    if sc.score >= MISSED_MIN_SCORE && !picked {
        ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
        let base = trades.iter().max_by_key(|x| (x.slot, x.recv_ms)).map(|x| x.price()).unwrap_or(0.0);
        tokio::spawn(missed_watch(ctx.clone(), t.mint.clone(), t.symbol.clone(), sc.score, blocked_by.clone(), base, now));
    }
    if shadow_picked {
        let (tr, pk) = (ctx.trader.clone(), Pick { score: shadow_sc.as_ref().map(|s| s.score).unwrap_or(sc.score), ..pick_clone(&pick) });
        ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
        tokio::spawn(async move {
            let _ = tr.try_enter(learning::SHADOW_LANE, &pk).await;
            tr.release_feed(&pk.mint);
        });
    }
    if dip_eligible {
        ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
        tokio::spawn(dip_watch(ctx.clone(), pick_clone(&pick), t0));
    }
    if picked {
        ctx.picked.fetch_add(1, Ordering::Relaxed);
        pulse(&ctx, AgentId::TokenPicker, AgentId::Executor, format!("Pick {} (score {:.0}) handed to Executor", t.symbol, sc.score), &t.mint);
        status(&ctx.hub, AgentId::Executor, StatusColor::Amber, format!("Pick received: {}, buying on paper", t.symbol));
        let _ = ctx.trader.try_enter("momentum", &pick).await;
    }
    ctx.trader.release_feed(&t.mint);
}

/// Five minutes after a high-scoring coin was skipped: what did its price do? Recorded as an efficiency note.
async fn missed_watch(ctx: Arc<Ctx>, mint: String, symbol: String, score: f64, blocked_by: String, base_price: f64, from_ms: i64) {
    tokio::time::sleep(Duration::from_millis(MISSED_WINDOW_MS as u64 + 1500)).await;
    let trades = ctx.trader.feed.store.snapshot(&mint);
    let out = learning::missed_outcome(&trades, base_price, from_ms);
    let text = learning::missed_text(&symbol, score, &blocked_by, out);
    let src = ctx.source_for(&mint).unwrap_or_default();
    ctx.trader.learner.add_note(now_ms(), "missed", &text, &src, &mint);
    crate::vault::missed_coin(&ctx.db, now_ms(), score, &text, &mint);
    ctx.hub.log(format!("LEARNING {text}"));
    ctx.trader.learner.emit(now_ms());
    ctx.trader.release_feed(&mint);
}

/// Scripted market: injects the scenario's trades into the trade store at their offsets, then runs the normal
/// observation -> score -> entry path. After a restart (the coin already has a position) it only continues the
/// market, so the resumed position meets the same prices it would have met live.
async fn replay_drive(ctx: Arc<Ctx>, sc: crate::replay::Scenario) {
    let key = format!("replay_t0:{}", sc.name);
    let restart = ctx.db.mint_has_position(&sc.create.mint);
    let t0 = match (restart, ctx.db.kv_get(&key).and_then(|v| v.parse::<i64>().ok())) {
        (true, Some(t)) => t,
        _ => {
            let t = now_ms();
            ctx.db.kv_set(&key, &t.to_string());
            t
        }
    };
    let t = sc.create.clone();
    ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
    ctx.hub.emit(EngineEvent::Token { ts: t0, mint: t.mint.clone(), symbol: t.symbol.clone(), name: t.name.clone(), creator: t.trader_public_key.clone(), mcap_sol: t.market_cap_sol });
    let store = ctx.trader.feed.store.clone();
    let mint = t.mint.clone();
    let events = sc.events;
    tokio::spawn(async move {
        for (off, tr) in events {
            let due = t0 + off;
            let now = now_ms();
            if due > now {
                tokio::time::sleep(Duration::from_millis((due - now) as u64)).await;
            }
            store.add(Trade { recv_ms: due, mint: mint.clone(), ..tr });
        }
    });
    if restart {
        ctx.hub.log(format!("REPLAY: restart detected, the scripted market continues from its original clock (started {} s ago)", (now_ms() - t0) / 1000));
        return;
    }
    ctx.db.upsert_token(&t.mint, t0, &t.name, &t.symbol, &t.trader_public_key, t.market_cap_sol);
    observe_and_trade(ctx.clone(), t, t0, ChainFacts::default()).await;
}

fn pick_clone(p: &Pick) -> Pick {
    Pick { mint: p.mint.clone(), symbol: p.symbol.clone(), creator: p.creator.clone(), curve_key: p.curve_key.clone(), score: p.score, features: p.features.clone(), size_factor: p.size_factor }
}

/// Paper variant `dip-40pct`: after the observation gate, wait (until the coin is 4 minutes old) for a 40-50% dip
/// from the peak with buy volume expanding, then enter. Holds the trade feed for the coin while it watches.
async fn dip_watch(ctx: Arc<Ctx>, p: Pick, t0: i64) {
    let until = t0 + 240_000;
    let style = ctx.cfg.entry.paper_variant.style.clone();
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let now = now_ms();
        if now >= until || ctx.mon.no_buys() {
            break;
        }
        let trades = ctx.trader.feed.store.snapshot(&p.mint);
        if trades.iter().any(|x| !x.is_buy && x.user == p.creator) {
            break; // creator sold: not a dip we buy
        }
        if let Some(dip) = picker::dip_signal(&trades, now, &ctx.cfg) {
            ctx.hub.log(format!("TRADE[{style}] dip signal on {}: {dip:.1}% below the peak with buying expanding", p.symbol));
            pulse(&ctx, AgentId::TokenPicker, AgentId::Executor, format!("Dip pick {} ({dip:.0}% off the peak) handed to Executor", p.symbol), &p.mint);
            let _ = ctx.trader.try_enter(&style, &p).await;
            break;
        }
    }
    ctx.trader.release_feed(&p.mint);
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------- RugCheck outage, end to end through observe_and_trade

    use crate::shield::rules::Holder;

    fn test_ctx(answers: Vec<Result<Option<serde_json::Value>, String>>, require_rugcheck: bool) -> Arc<Ctx> {
        let mut cfg = Config::from_json(include_str!("../../config.json")).unwrap();
        cfg.filters.require_rugcheck = require_rugcheck;
        cfg.paper_fill.buy_fail_pct = 0.0;
        cfg.paper_fill.sell_fail_pct = 0.0;
        cfg.paper_fill.delay_ms_min = 0;
        cfg.paper_fill.delay_ms_max = 0;
        let db = Arc::new(Db::open(":memory:").unwrap());
        let hub = Hub::new();
        let rpc = Arc::new(Rpc::new("http://127.0.0.1:9", 50.0));
        let dir = std::env::temp_dir().join(format!("ct-eng-{}-{}", now_ms(), answers.len()));
        let trader = Trader::new(cfg.clone(), db.clone(), hub.clone(), TradeFeed::offline(), None, 150.0, dir, true);
        let rc = Arc::new(crate::shield::net::RugCheck::scripted(answers, vec![Duration::from_millis(5); 3]));
        let shield = Shield::with_rugcheck(cfg.clone(), db.clone(), rpc, ShieldOpts { relax_egress: false }, rc);
        Arc::new(Ctx { cfg: cfg.clone(), links: Default::default(), shield, xmon: crate::xmon::XMon::new(db.clone(), hub.clone(), cfg.x_monitor.clone()), trader, hub, db, mon: Arc::new(MonitorState::default()), chain_slots: Semaphore::new(6), checking: AtomicU32::new(0), observing: AtomicU32::new(0), picked: AtomicU32::new(0) })
    }

    fn create() -> FeedMsg {
        match crate::pump::parse_feed(crate::pump::tests::REAL_CREATE) {
            crate::pump::Parsed::Create(m) => m,
            _ => panic!(),
        }
    }

    /// A busy coin: 16 different buyers over the last 55 s, the curve filling up to 15.5 SOL. It scores far above 80
    /// and passes every entry and pre-buy rule, so only the RugCheck step decides whether it is bought and at what size.
    fn seed_busy_coin(ctx: &Ctx, t: &FeedMsg) {
        ctx.trader.hold_feed(&t.mint, &t.bonding_curve_key);
        let now = now_ms();
        for i in 0..16u64 {
            let age_s = if i < 3 { 50 - i as i64 * 8 } else { 27 - (i as i64 - 3) * 2 };
            ctx.trader.feed.store.add(Trade {
                mint: t.mint.clone(),
                user: format!("buyer{i:02}xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
                is_buy: true,
                sol: 1.0,
                tokens: 20_000_000.0,
                slot: 300_000_000 + i * 40,
                signature: format!("sig{i}"),
                recv_ms: now - age_s * 1000,
                v_sol: 30.0 + (i as f64 + 1.0) * 1.0,
                v_tokens: 7.0e8,
                real_sol: (i as f64 + 1.0) * 1.0,
            });
        }
    }

    fn clean_facts(ctx: &Ctx, t: &FeedMsg) -> ChainFacts {
        let mut f = ChainFacts::default();
        f.excluded.insert(t.bonding_curve_key.clone());
        f.mint_info = Some(crate::shield::rules::MintInfo { program: "spl-token-2022".into(), supply: 1.0e15, decimals: 6, ..Default::default() });
        f.core = vec![crate::shield::rules::CheckResult::pass("dev_snipe", "x"), crate::shield::rules::CheckResult::pass("bundle", "x")];
        f.rug = Some(ctx.shield.start_rugcheck(&t.mint));
        f
    }

    fn opened_size(ctx: &Ctx) -> Option<f64> {
        ctx.db.with_conn(|c| c.query_row("SELECT size_sol FROM positions ORDER BY id LIMIT 1", [], |r| r.get::<_, f64>(0)).ok())
    }

    fn five_502() -> Vec<Result<Option<serde_json::Value>, String>> {
        (0..4).map(|_| Err("rugcheck http 502".to_string())).collect()
    }

    #[tokio::test]
    async fn rugcheck_down_but_onchain_checks_pass_buys_at_half_size() {
        let ctx = test_ctx(five_502(), false);
        *ctx.shield.holders_stub.lock().unwrap() = Some(Ok(vec![Holder { owner: create().bonding_curve_key, pct: 60.0, insider: false }, Holder { owner: "w1".into(), pct: 3.0, insider: false }]));
        let t = create();
        seed_busy_coin(&ctx, &t);
        let normal = ctx.trader.lane("momentum").unwrap().risk.lock().unwrap().size();
        let facts = clean_facts(&ctx, &t);
        observe_and_trade(ctx.clone(), t.clone(), now_ms() - 61_000, facts).await;
        let size = opened_size(&ctx).expect("the coin was bought");
        assert!((size - normal / 2.0).abs() < 1e-9, "half of {normal}, got {size}");
        let rows: Vec<(String, String, String)> = ctx.db.with_conn(|c| c.prepare("SELECT stage, outcome, detail FROM decisions WHERE mint=?").unwrap().query_map([&t.mint], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().flatten().collect());
        let fb = rows.iter().find(|r| r.0 == "rugcheck-fallback").expect("fallback decision recorded");
        assert_eq!(fb.1, "pass");
        assert!(fb.2.contains("RugCheck unavailable, passed on-chain checks"), "{}", fb.2);
    }

    #[tokio::test]
    async fn rugcheck_report_ok_buys_at_full_size() {
        let report = serde_json::json!({"rugged": false, "creator": "c", "token": {"supply": 1.0e15}, "topHolders": [{"owner": "w1", "pct": 2.0}], "risks": [], "fileMeta": {"image": "i"}});
        let ctx = test_ctx(vec![Ok(Some(report))], false);
        let t = create();
        seed_busy_coin(&ctx, &t);
        let normal = ctx.trader.lane("momentum").unwrap().risk.lock().unwrap().size();
        let facts = clean_facts(&ctx, &t);
        observe_and_trade(ctx.clone(), t.clone(), now_ms() - 61_000, facts).await;
        assert!((opened_size(&ctx).expect("bought") - normal).abs() < 1e-9);
    }

    #[tokio::test]
    async fn rugcheck_danger_flag_stops_the_watch_early_and_nothing_is_bought() {
        let danger = serde_json::json!({"rugged": false, "creator": "c", "token": {"supply": 1.0e15}, "topHolders": [], "risks": [{"name": "Copycat token", "level": "danger", "description": "x"}]});
        let ctx = test_ctx(vec![Ok(Some(danger))], false);
        *ctx.shield.holders_stub.lock().unwrap() = Some(Ok(vec![]));
        let t = create();
        seed_busy_coin(&ctx, &t);
        let facts = clean_facts(&ctx, &t);
        // the coin is brand new (60 s of watching ahead): the danger report must end the watch within a moment
        let started = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(5), observe_and_trade(ctx.clone(), t.clone(), now_ms(), facts)).await.expect("ended early");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(opened_size(&ctx).is_none());
        let n: i64 = ctx.db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM decisions WHERE mint=? AND stage='rugcheck' AND outcome='block' AND reason='RUGCHECK_DANGER'", [&t.mint], |r| r.get(0)).unwrap());
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn require_rugcheck_on_buys_nothing_when_rugcheck_is_down() {
        let ctx = test_ctx(five_502(), true);
        *ctx.shield.holders_stub.lock().unwrap() = Some(Ok(vec![]));
        let t = create();
        seed_busy_coin(&ctx, &t);
        let facts = clean_facts(&ctx, &t);
        observe_and_trade(ctx.clone(), t.clone(), now_ms() - 61_000, facts).await;
        assert!(opened_size(&ctx).is_none());
        let score_rows: i64 = ctx.db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM scores WHERE picked=0 AND gates LIKE '%RugCheck or on-chain%'", [], |r| r.get(0)).unwrap());
        assert_eq!(score_rows, 1, "the funnel can see which gate stopped the coin");
    }


    #[test]
    fn opts_default_to_public_rpc_without_keys() {
        let cfg = Config::from_json(include_str!("../../config.json")).unwrap();
        let _g = crate::secrets::TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CT_RPC_URL");
        std::env::remove_var("HELIUS_API_KEY");
        let o = EngineOpts::new(cfg, ":memory:");
        assert_eq!(o.rpc_url, crate::rpc::PUBLIC_RPC);
        assert!(!o.relax_egress);
    }
}
