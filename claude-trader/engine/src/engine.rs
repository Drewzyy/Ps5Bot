//! Engine orchestrator (M2): Memecoin Monitor -> Scam Shield -> Token Picker, with events for the UI.
//! Paper only: nothing here signs or sends a transaction.

use crate::config::Config;
use crate::db::{Db, DecisionRow};
use crate::events::*;
use crate::hub::Hub;
use crate::monitor::{self, Incoming, MonitorState};
use crate::picker;
use crate::pump::{mcap_sol, solscan_token, FeedMsg};
use crate::rpc::{now_ms, Rpc, TradeFeed};
use crate::shield::rules::{ev, should_blacklist_creator, Outcome};
use crate::shield::{decide, ChainFacts, Decision, Shield, ShieldOpts};
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
        }
    }
}

struct Ctx {
    cfg: Config,
    shield: Shield,
    feed: TradeFeed,
    hub: Arc<Hub>,
    db: Arc<Db>,
    mon: Arc<MonitorState>,
    sol_usd: f64,
    chain_slots: Semaphore,
    checking: AtomicU32,
    observing: AtomicU32,
    picked: AtomicU32,
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
    let rpc = Arc::new(Rpc::new(&opts.rpc_url, 6.0));
    hub.log(format!("engine: rpc endpoint {}", rpc.masked_url()));
    let sol_usd = match opts.sol_usd {
        Some(p) => p,
        None => fetch_sol_usd().await.unwrap_or(150.0),
    };
    hub.log(format!("engine: SOL price used for USD conversion: ${sol_usd:.2}"));
    let hub_log = hub.clone();
    let feed = TradeFeed::spawn(rpc.ws_url(), Arc::new(move |s| hub_log.log(s)));
    let shield = Shield::new(opts.config.clone(), db.clone(), rpc, ShieldOpts { relax_egress: opts.relax_egress });
    let mon = Arc::new(MonitorState::default());
    let ctx = Arc::new(Ctx {
        cfg: opts.config.clone(),
        shield,
        feed,
        hub: hub.clone(),
        db: db.clone(),
        mon: mon.clone(),
        sol_usd,
        chain_slots: Semaphore::new(6),
        checking: AtomicU32::new(0),
        observing: AtomicU32::new(0),
        picked: AtomicU32::new(0),
    });

    for a in [AgentId::ThesisFinder, AgentId::XMonitor, AgentId::TopGainers] {
        status(&hub, a, StatusColor::Grey, "Not active yet (later version)");
    }
    status(&hub, AgentId::TopTraderMonitor, StatusColor::Grey, "Off in version 1");
    status(&hub, AgentId::HoldTimer, StatusColor::Grey, "No open positions");
    status(&hub, AgentId::Executor, StatusColor::Grey, "Idle: paper trades start in M3");

    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(monitor::run(opts.feed_url.clone(), tx, mon.clone(), hub.clone(), stop.clone()));

    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut stop = stop;
    let day_start = |n: i64| n - n.rem_euclid(86_400_000);
    loop {
        tokio::select! {
            _ = stop.changed() => { if *stop.borrow() { break; } }
            m = rx.recv() => match m {
                Some(Incoming::Create(t)) => { tokio::spawn(process_token(ctx.clone(), t)); }
                Some(Incoming::Migrate(t)) => {
                    hub.emit(EngineEvent::Activity { ts: now_ms(), agent: AgentId::MemecoinMonitor, text: format!("Migration to the DEX seen: {}", short_mint(&t.mint)), mint: Some(t.mint.clone()) });
                }
                None => break,
            },
            _ = tick.tick() => {
                let now = now_ms();
                // Memecoin Monitor
                let created = mon.created.load(Ordering::Relaxed);
                if mon.connected.load(Ordering::Relaxed) {
                    let ago = (now - mon.last_msg_ms.load(Ordering::Relaxed)) / 1000;
                    status(&hub, AgentId::MemecoinMonitor, StatusColor::Green, format!("Live: {created} new coins seen, last message {ago} s ago"));
                } else if mon.no_buys() {
                    status(&hub, AgentId::MemecoinMonitor, StatusColor::Red, "Feed down for over 5 s: NO BUYS until it reconnects");
                } else {
                    status(&hub, AgentId::MemecoinMonitor, StatusColor::Amber, "Connecting to the live feed");
                }
                let (checked, blocked, passed, by) = db.summary(day_start(now));
                hub.emit(EngineEvent::ShieldSummary { ts: now, checked, blocked, passed, by_reason: by });
                let chk = ctx.checking.load(Ordering::Relaxed);
                status(&hub, AgentId::ScamShield, if chk > 0 { StatusColor::Green } else { StatusColor::Amber }, format!("Checking {chk} coins. Today: {blocked} blocked, {passed} passed"));
                let obs = ctx.observing.load(Ordering::Relaxed);
                status(&hub, AgentId::TokenPicker, if obs > 0 { StatusColor::Green } else { StatusColor::Amber }, format!("Watching {obs} coins for 60 s each. {} picked so far", ctx.picked.load(Ordering::Relaxed)));
            }
        }
    }
    Ok(())
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
        None => ("pass", String::new(), format!("{} checks done, none blocked.{}", d.results.len(), flags(d)), vec![]),
    };
    if evidence.is_empty() {
        evidence = d.results.iter().flat_map(|r| r.evidence.clone()).take(3).collect();
    }
    if evidence.is_empty() {
        evidence.push(ev("Token on Solscan", solscan_token(&t.mint)));
    }
    ctx.db.insert_decision(&DecisionRow { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: stage.into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into() });
    ctx.hub.emit(EngineEvent::Verdict { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: stage.into(), outcome: outcome.into(), reason: reason.clone(), detail: detail.clone(), verdict: d.verdict.into(), evidence });
    let text = if outcome == "block" { format!("Blocked {} at {stage}: {reason}. {detail}", t.symbol) } else { format!("{} passed {stage} ({})", t.symbol, d.verdict) };
    ctx.hub.emit(EngineEvent::Decision { ts: now, agent: AgentId::ScamShield, text: text.clone() });
    if outcome == "block" {
        ctx.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::ScamShield, text, mint: Some(t.mint.clone()) });
    }
}

fn pulse(ctx: &Ctx, from: AgentId, to: AgentId, text: String, mint: &str) {
    ctx.hub.emit(EngineEvent::Pulse { from, to, text: text.clone() });
    ctx.hub.emit(EngineEvent::Activity { ts: now_ms(), agent: from, text, mint: Some(mint.into()) });
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
    ctx.feed.subscribe(&t.mint, &t.bonding_curve_key);

    // Stage B: chain checks (bounded concurrency; waiting too long counts as an undecided required check)
    let permit = tokio::time::timeout(Duration::from_secs(8), ctx.chain_slots.acquire()).await;
    let (b, facts): (Decision, ChainFacts) = match permit {
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
    record(&ctx, &t, "chain", &b);
    if b.block.is_some() {
        ctx.feed.unsubscribe(&t.mint);
        return;
    }
    pulse(&ctx, AgentId::ScamShield, AgentId::TokenPicker, format!("Clean coin {} handed to Token Picker", t.symbol), &t.mint);

    // Observation window
    ctx.observing.fetch_add(1, Ordering::Relaxed);
    let min_age = ctx.cfg.entry.min_age_seconds as i64 * 1000;
    let wait = (t0 + min_age - now_ms()).max(0) as u64;
    tokio::time::sleep(Duration::from_millis(wait)).await;
    let now = now_ms();
    let age = (now - t0) as f64 / 1000.0;
    let trades = ctx.feed.store.snapshot(&t.mint);
    let launch_price = if t.v_tokens_in_bonding_curve > 0.0 { t.v_sol_in_bonding_curve / t.v_tokens_in_bonding_curve } else { 0.0 };
    let obs = picker::observe(&trades, &t.trader_public_key, launch_price, (t.v_sol_in_bonding_curve - 30.0).max(0.0), age, now);
    let sc = picker::score(&obs, &ctx.cfg, ctx.sol_usd);

    // serial rugger bookkeeping and "seen trending"
    let peak = trades.iter().map(|x| x.mcap_sol()).fold(0.0, f64::max);
    if should_blacklist_creator(peak, obs.mcap_sol, age) && obs.mcap_sol > 0.0 {
        ctx.db.blacklist_creator(&t.trader_public_key, &format!("{} fell over 70% from its peak within {age:.0} s", t.symbol), now);
    }
    if obs.mcap_sol * ctx.sol_usd >= ctx.cfg.feed.min_market_cap_usd {
        ctx.db.mark_trending(&t.mint);
    }

    let mut gates = sc.gates.clone();
    let mut picked = sc.picked;
    let mut pre: Option<Decision> = None;
    if picked {
        if ctx.mon.no_buys() {
            gates.push("feed down: no buys".into());
            picked = false;
        } else {
            let launch_mcap = mcap_sol(t.v_sol_in_bonding_curve, t.v_tokens_in_bonding_curve);
            let d = decide(ctx.shield.pre_buy(&t, &trades, &facts, obs.mcap_sol, age).into_iter().collect());
            record(&ctx, &t, "pre-buy", &d);
            let _ = launch_mcap;
            if d.block.is_some() {
                picked = false;
                gates.push(format!("Scam Shield pre-buy: {}", d.block.as_ref().map(|b| b.code.clone()).unwrap_or_default()));
            }
            pre = Some(d);
        }
    }

    let now = now_ms();
    ctx.hub.emit(EngineEvent::Score {
        ts: now,
        mint: t.mint.clone(),
        symbol: t.symbol.clone(),
        score: sc.score,
        min_score: ctx.cfg.observation.min_score,
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
    let reason = if picked { String::new() } else if sc.score < ctx.cfg.observation.min_score { "LOW_SCORE".to_string() } else { "GATE".to_string() };
    let detail = if picked {
        format!("Score {:.1} (needs {}). All entry gates hold.", sc.score, ctx.cfg.observation.min_score)
    } else {
        format!("Score {:.1} (needs {}). {}", sc.score, ctx.cfg.observation.min_score, gates.join("; "))
    };
    ctx.db.insert_decision(&DecisionRow { ts: now, mint: t.mint.clone(), symbol: t.symbol.clone(), stage: "observation".into(), outcome: if picked { "pass" } else { "block" }.into(), reason, detail: detail.clone(), verdict: String::new() });
    ctx.hub.emit(EngineEvent::Decision { ts: now, agent: AgentId::TokenPicker, text: format!("{}: {detail}", t.symbol) });
    ctx.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::TokenPicker, text: format!("Scored {} at {:.1}{}", t.symbol, sc.score, if picked { ": picked" } else { "" }), mint: Some(t.mint.clone()) });
    ctx.observing.fetch_sub(1, Ordering::Relaxed);
    ctx.feed.unsubscribe(&t.mint);
    let _ = pre;
    if picked {
        ctx.picked.fetch_add(1, Ordering::Relaxed);
        pulse(&ctx, AgentId::TokenPicker, AgentId::Executor, format!("Pick {} (score {:.0}) handed to Executor", t.symbol, sc.score), &t.mint);
        status(&ctx.hub, AgentId::Executor, StatusColor::Amber, format!("Pick received: {}. Paper fills arrive in M3", t.symbol));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opts_default_to_public_rpc_without_keys() {
        let cfg = Config::from_json(include_str!("../../config.json")).unwrap();
        std::env::remove_var("CT_RPC_URL");
        std::env::remove_var("HELIUS_API_KEY");
        let o = EngineOpts::new(cfg, ":memory:");
        assert_eq!(o.rpc_url, crate::rpc::PUBLIC_RPC);
        assert!(!o.relax_egress);
    }
}
