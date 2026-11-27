//! Trader: risk gates -> paper buy -> persisted position -> 1 s exit loop -> paper sells -> JSONL + stats.
//! Paper only. Two lanes run the same engine side by side: `momentum` (the configured entry style) and the
//! paper variant `dip-40pct`; every position, log line and statistic carries its lane so they can be compared.

use crate::config::Config;
use crate::curve::{Curve, FeeBreakdown, FeeModel, Rng};
use crate::db::{Db, DecisionRow};
use crate::events::*;
use crate::exits::{self, ExitAction, Market};
use crate::hub::Hub;
use crate::exec::Executor;
use crate::learning::{Effect, EntryFeatures, Learner, Tuned, SHADOW_LANE};
use crate::paper::PaperExecutor;
use crate::position::{PosState, Position};
use crate::pump::{Trade, SUPPLY_TOKENS};
use crate::report::{self, TradeLine};
use crate::risk::{day_of, RiskState};
use crate::rpc::{now_ms, Rpc, TradeFeed};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Pick {
    pub mint: String,
    pub symbol: String,
    pub creator: String,
    pub curve_key: String,
    pub score: f64,
    /// What the picker saw (kept with the position for the learning journal).
    pub features: EntryFeatures,
    /// 1.0 normally; 0.5 when RugCheck gave no answer and the coin was judged on our own on-chain checks (SPEC 11b #29).
    pub size_factor: f64,
}

/// Trade size after the pick's size factor (half for a coin that skipped RugCheck), rounded like `RiskState::size`.
pub fn scaled_size(size: f64, factor: f64) -> f64 {
    let f = if factor.is_finite() && factor > 0.0 { factor.min(1.0) } else { 1.0 };
    (size * f * 1e9).round() / 1e9
}

pub struct Lane {
    pub style: String,
    pub risk: Mutex<RiskState>,
    /// What this lane runs on: the shipped config plus any applied (or, for the shadow lane, tested) learning change.
    pub tuned: std::sync::RwLock<Tuned>,
    live: Mutex<HashMap<i64, Position>>,
    recent: Mutex<VecDeque<PositionView>>,
}

pub struct Trader {
    pub cfg: Config,
    pub db: Arc<Db>,
    pub hub: Arc<Hub>,
    pub feed: TradeFeed,
    rpc: Option<Arc<Rpc>>,
    pub exec: Arc<dyn Executor>,
    pub learner: Arc<Learner>,
    fees: FeeModel,
    pub lanes: Vec<Arc<Lane>>,
    sol_usd: f64,
    logs_dir: PathBuf,
    pub replay: bool,
    feed_refs: Mutex<HashMap<String, u32>>,
    polled: Mutex<HashMap<String, (Curve, i64, bool)>>,
    migrated: Mutex<HashSet<String>>,
    poll_checks: std::sync::atomic::AtomicU32,
    /// Kill switch (persisted in SQLite kv "killed"; this flag is refreshed from it every second).
    killed: std::sync::atomic::AtomicBool,
}

fn pf(x: f64) -> String {
    format!("{x:.9}")
}

/// Token prices are tiny (about 1e-7 SOL), so they print in scientific notation.
fn pp(x: f64) -> String {
    format!("{x:.6e}")
}

impl Trader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(cfg: Config, db: Arc<Db>, hub: Arc<Hub>, feed: TradeFeed, rpc: Option<Arc<Rpc>>, sol_usd: f64, logs_dir: PathBuf, replay: bool) -> Arc<Trader> {
        let mut exec = PaperExecutor::new(&cfg, Rng::from_time());
        if cfg!(test) {
            exec.time_scale = 0.0; // unit tests never wait out the 20 s "unconfirmed" check
        }
        Self::with_executor(cfg, db, hub, feed, rpc, sol_usd, logs_dir, replay, Arc::new(exec))
    }

    /// The same trader with any executor plugged in (paper today, the gated live executor later).
    #[allow(clippy::too_many_arguments)]
    pub fn with_executor(cfg: Config, db: Arc<Db>, hub: Arc<Hub>, feed: TradeFeed, rpc: Option<Arc<Rpc>>, sol_usd: f64, logs_dir: PathBuf, replay: bool, exec: Arc<dyn Executor>) -> Arc<Trader> {
        let now = now_ms();
        let mut styles = vec!["momentum".to_string()];
        styles.push(cfg.entry.paper_variant.style.clone());
        // the shadow lane only trades while a learning change is being tested (it is idle otherwise)
        styles.push(SHADOW_LANE.to_string());
        let tuned0 = Tuned::from_config(&cfg);
        let lanes = styles
            .iter()
            .map(|s| {
                let mut r = RiskState::new(&cfg.risk, cfg.paper.bankroll_sol, now);
                let reset = db.kv_get(&format!("reset:{s}")).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                let mut after_reset_pnl = 0.0;
                for p in db.load_closed(s, true) {
                    let exit = p.exit_ms.unwrap_or(p.entry_ms);
                    if p.kind == "trade" {
                        r.record_open(p.entry_ms, &p.mint, p.size_sol, p.slippage_pct);
                        r.record_close(exit, &p.mint, p.size_sol, p.pnl_sol, false);
                    } else {
                        r.add_cost(exit, p.failed_sol);
                    }
                    if exit > reset {
                        after_reset_pnl += p.pnl_sol;
                    }
                }
                if reset > 0 {
                    // a manual restart happened: the streak and the day's loss restart from there
                    r.consec_losses = 0;
                    if day_of(reset) == day_of(now) {
                        r.day_start_equity = r.equity() - after_reset_pnl;
                        r.day_pnl = after_reset_pnl;
                    }
                }
                r.halt = db.kv_get(&format!("halt:{s}")).filter(|h| !h.is_empty());
                Arc::new(Lane { style: s.clone(), risk: Mutex::new(r), tuned: std::sync::RwLock::new(tuned0.clone()), live: Mutex::new(HashMap::new()), recent: Mutex::new(VecDeque::new()) })
            })
            .collect();
        let killed = db.killed_at().is_some();
        let fees = FeeModel::from_config(&cfg);
        let learner = Arc::new(Learner::new(db.clone(), Some(hub.clone()), &cfg));
        Arc::new(Trader {
            learner,
            cfg,
            db,
            hub,
            feed,
            rpc,
            exec,
            fees,
            lanes,
            sol_usd,
            logs_dir,
            replay,
            feed_refs: Mutex::new(HashMap::new()),
            polled: Mutex::new(HashMap::new()),
            migrated: Mutex::new(HashSet::new()),
            poll_checks: std::sync::atomic::AtomicU32::new(0),
            killed: std::sync::atomic::AtomicBool::new(killed),
        })
    }

    /// The config a lane is running right now (shipped values plus learning changes).
    pub fn lane_cfg(&self, style: &str) -> Config {
        self.lane(style).map(|l| l.tuned.read().unwrap().cfg.clone()).unwrap_or_else(|| self.cfg.clone())
    }

    pub fn lane_tuned(&self, style: &str) -> Tuned {
        self.lane(style).map(|l| l.tuned.read().unwrap().clone()).unwrap_or_else(|| Tuned::from_config(&self.cfg))
    }

    /// Carry out what the learning loop decided.
    pub fn apply_effects(&self, fx: Vec<Effect>) {
        for e in fx {
            match e {
                Effect::SetMain(t) => {
                    if let Some(l) = self.lane("momentum") {
                        *l.tuned.write().unwrap() = *t;
                    }
                }
                Effect::SetShadow(t) => {
                    if let Some(l) = self.lane(SHADOW_LANE) {
                        // a stopped test keeps the lane's tuning at the main lane's so it stays comparable
                        *l.tuned.write().unwrap() = match t {
                            Some(t) => *t,
                            None => self.lane_tuned("momentum"),
                        };
                    }
                }
                Effect::Log(m) => self.hub.log(m),
            }
        }
    }

    /// After a restart: put applied changes and a running test back on their lanes.
    pub fn restore_learning(&self) {
        let fx = self.learner.restore();
        self.apply_effects(fx);
        self.learner.emit(now_ms());
    }

    pub fn learning_command(&self, cmd: &str) -> String {
        let (msg, fx) = self.learner.command(cmd);
        self.apply_effects(fx);
        msg
    }

    /// True while a change is being tested in the shadow lane.
    pub fn shadow_active(&self) -> bool {
        self.learner.shadow_tuned().is_some()
    }

    pub fn lane(&self, style: &str) -> Option<Arc<Lane>> {
        self.lanes.iter().find(|l| l.style == style).cloned()
    }

    /// Manual restart of a halted lane (`engine-cli feed --clear-halt`).
    pub fn clear_halts(&self) {
        for l in &self.lanes {
            let mut r = l.risk.lock().unwrap();
            if r.halt.is_some() {
                self.hub.log(format!("TRADE[{}] halt cleared by hand: {}", l.style, r.halt.clone().unwrap_or_default()));
            }
            r.clear_halt();
            self.db.kv_del(&format!("halt:{}", l.style));
            self.db.kv_set(&format!("reset:{}", l.style), &now_ms().to_string());
        }
    }

    // ------------------------------------------------------------ kill switch

    pub fn is_killed(&self) -> bool {
        self.killed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Stop everything: no new entries in any lane, pending buys cancelled, every open position sold through the
    /// normal sell path (fees, fail rates, escalating retries) until closed. Persisted, so a restart stays stopped.
    pub fn kill(&self, source: &str) -> String {
        if self.is_killed() {
            return "kill switch already on".into();
        }
        let now = now_ms();
        self.db.kv_set("killed", &now.to_string());
        self.killed.store(true, std::sync::atomic::Ordering::SeqCst);
        let n: usize = self.lanes.iter().map(|l| l.live.lock().unwrap().len()).sum();
        self.hub.log(format!("TRADE KILL SWITCH ON (from {source}): no new entries, selling {n} open position(s) now"));
        self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::Executor, text: format!("KILL SWITCH ON: all trading stopped, selling {n} open position(s)"), mint: None, source_url: None });
        self.hub.emit(EngineEvent::Kill(self.kill_state()));
        format!("kill switch on, {n} position(s) closing")
    }

    /// Separate explicit step: only this clears the persisted KILLED state.
    pub fn resume_trading(&self, source: &str) -> String {
        if !self.is_killed() {
            return "kill switch is not on".into();
        }
        self.db.kv_del("killed");
        self.killed.store(false, std::sync::atomic::Ordering::SeqCst);
        self.hub.log(format!("TRADE KILL SWITCH OFF (from {source}): trading may resume"));
        self.hub.emit(EngineEvent::Activity { ts: now_ms(), agent: AgentId::Executor, text: "Kill switch off: trading resumed".into(), mint: None, source_url: None });
        self.hub.emit(EngineEvent::Kill(self.kill_state()));
        "trading resumed".into()
    }

    /// Pick up a kill/resume written to SQLite by another process (`engine-cli kill`).
    fn sync_kill(&self) {
        let k = self.db.killed_at().is_some();
        let was = self.killed.swap(k, std::sync::atomic::Ordering::SeqCst);
        if k && !was {
            self.hub.log("TRADE KILL SWITCH ON (from another process): no new entries, selling every open position");
            self.hub.emit(EngineEvent::Activity { ts: now_ms(), agent: AgentId::Executor, text: "KILL SWITCH ON: all trading stopped, selling every open position".into(), mint: None, source_url: None });
        } else if !k && was {
            self.hub.log("TRADE KILL SWITCH OFF (from another process): trading may resume");
        }
    }

    pub fn kill_state(&self) -> KillState {
        let killed_at = self.db.killed_at().unwrap_or(0);
        let mut items = vec![];
        for l in &self.lanes {
            for p in l.live.lock().unwrap().values() {
                let status = match (p.state, p.sell_fails) {
                    (PosState::Selling, _) => "selling now".to_string(),
                    (_, 0) => "closing".to_string(),
                    (_, n) => format!("retrying: {n} sell attempt(s) failed, fee escalated"),
                };
                items.push(KillItem { symbol: p.symbol.clone(), lane: l.style.clone(), status });
            }
        }
        let closing = items.len() as u32;
        let closed = if killed_at > 0 { self.db.scalar(&format!("SELECT COUNT(*) FROM positions WHERE exit_reason='kill-switch' AND state='CLOSED' AND exit_ms>={killed_at}")) as u32 } else { 0 };
        let killed = self.is_killed();
        let text = if !killed {
            String::new()
        } else if closing > 0 {
            format!("Kill switch on: {closing} position(s) closing")
        } else {
            format!("Kill switch on: all closed ({closed} sold). No new buys until you press Resume trading.")
        };
        KillState { ts: now_ms(), killed, killed_at_ms: killed_at, closing, closed, items, text }
    }

    // ------------------------------------------------------------ market data

    pub fn hold_feed(&self, mint: &str, curve: &str) {
        let mut g = self.feed_refs.lock().unwrap();
        let n = g.entry(mint.to_string()).or_insert(0);
        if *n == 0 {
            self.feed.subscribe(mint, curve);
        }
        *n += 1;
    }

    pub fn release_feed(&self, mint: &str) {
        let mut g = self.feed_refs.lock().unwrap();
        if let Some(n) = g.get_mut(mint) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                g.remove(mint);
                self.feed.unsubscribe(mint);
            }
        }
    }

    pub fn on_migration(&self, mint: &str) {
        self.migrated.lock().unwrap().insert(mint.to_string());
    }

    /// Latest curve state: newest trade event, or the RPC poll when that is newer. None until something is known.
    pub fn current_curve(&self, mint: &str) -> Option<Curve> {
        let t = self.feed.store.last(mint);
        let p = self.polled.lock().unwrap().get(mint).cloned();
        let from_trade = t.as_ref().map(|t| (Curve { v_sol: t.v_sol, v_tokens: t.v_tokens, real_sol: t.real_sol }, t.recv_ms));
        let best = match (from_trade, p) {
            (Some(a), Some((c, ts, _))) => if ts > a.1 + 1500 { Some(c) } else { Some(a.0) },
            (Some(a), None) => Some(a.0),
            (None, Some((c, _, _))) => Some(c),
            (None, None) => None,
        };
        best.filter(|c| c.valid())
    }

    async fn poll_curve(&self, mint: &str, curve_addr: &str) {
        let Some(rpc) = &self.rpc else { return };
        match rpc.curve_state(curve_addr).await {
            Ok((c, complete)) if c.valid() => {
                // self-check against the newest trade event: a layout surprise must never feed bad prices to the exits
                if let Some(t) = self.feed.store.last(mint) {
                    let drift = (c.price() / t.price() - 1.0).abs();
                    let n = self.poll_checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n < 6 || drift > 0.25 {
                        self.hub.log(format!("curve poll check {mint}: polled price {:.4e} vs last trade {:.4e} (drift {:.1}%, trade {} ms old)", c.price(), t.price(), drift * 100.0, now_ms() - t.recv_ms));
                    }
                    if drift > 0.25 && now_ms() - t.recv_ms < 5_000 {
                        return;
                    }
                }
                self.polled.lock().unwrap().insert(mint.to_string(), (c, now_ms(), complete));
                if complete {
                    self.on_migration(mint);
                }
            }
            Ok(_) => {}
            Err(e) => {
                let n = self.poll_checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 6 {
                    self.hub.log(format!("curve poll failed for {mint}: {e}"));
                }
            }
        }
    }

    // ------------------------------------------------------------ entry

    fn block(&self, lane: &str, p: &Pick, reason: &str, detail: &str) {
        let now = now_ms();
        self.db.insert_decision(&DecisionRow { ts: now, mint: p.mint.clone(), symbol: p.symbol.clone(), stage: "risk-gate".into(), outcome: "block".into(), reason: reason.into(), detail: format!("[{lane}] {detail}"), verdict: String::new() });
        self.hub.log(format!("TRADE[{lane}] SKIP {} ({}) {reason}: {detail}", p.symbol, p.mint));
        self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::Executor, text: format!("[{lane}] Skipped {}: {detail}", p.symbol), mint: Some(p.mint.clone()), source_url: None });
    }

    /// Run the risk gates, write the position to SQLite, do the paper buy and start managing it.
    pub async fn try_enter(self: &Arc<Self>, style: &str, p: &Pick) -> Result<i64, String> {
        let Some(lane) = self.lane(style) else { return Err("unknown lane".into()) };
        let now = now_ms();
        if self.is_killed() {
            self.block(style, p, "KILLED", "Kill switch is on. No new entries.");
            return Err("KILLED".into());
        }
        let Some(curve) = self.current_curve(&p.mint) else {
            self.block(style, p, "NO_CURVE_DATA", "No curve data for this coin yet.");
            return Err("no curve data".into());
        };
        let size = scaled_size(lane.risk.lock().unwrap().size(), p.size_factor);
        let (tokens, quote_price, after) = self.exec.quote_buy(&curve, size);
        let sell_impact = if tokens > 0.0 { (1.0 - after.sell(tokens).0 / (after.price() * tokens)) * 100.0 } else { 0.0 };
        let impact = self.exec.buy_impact_pct(&curve, size) + sell_impact;
        let lane_cfg = self.lane_cfg(style);
        let tp1 = lane_cfg.exits.tranches.first().map(|t| t.at_pct).unwrap_or(20.0);
        {
            let mut r = lane.risk.lock().unwrap();
            if let Err(g) = r.check_entry(now, &p.mint, size, &self.fees, tp1, impact) {
                drop(r);
                self.block(style, p, g.code(), &g.detail());
                return Err(g.code().into());
            }
            r.record_open(now, &p.mint, size, 0.0);
        }
        // SPEC 11b #24: the position is in SQLite BEFORE the buy goes out.
        let mut pos = Position::new_buying(style, &p.mint, &p.symbol, &p.creator, &p.curve_key, size, now, self.sol_usd);
        pos.id = self.db.insert_position(&pos);
        lane.live.lock().unwrap().insert(pos.id, pos.clone());
        self.hold_feed(&p.mint, &p.curve_key);
        self.hub.log(format!("TRADE[{style}] SIGNAL {} mint={} score={:.1} size={size} SOL quote={} impactRoundTrip={impact:.2}%", p.symbol, p.mint, p.score, pp(quote_price)));

        if self.is_killed() {
            // switched on between the gates and the buy: cancel the pending buy
            pos.fail_buy(&FeeBreakdown::default(), now_ms(), "buy-cancelled: kill switch");
            self.db.save_position(&pos);
            lane.risk.lock().unwrap().cancel_open(&p.mint);
            lane.live.lock().unwrap().remove(&pos.id);
            self.release_feed(&p.mint);
            self.hub.log(format!("TRADE[{style}] BUY CANCELLED {} by the kill switch", p.symbol));
            return Err("KILLED".into());
        }
        let mint = p.mint.clone();
        let res = self.exec.buy(size, quote_price, &|| self.current_curve(&mint)).await;
        let now = now_ms();
        match res {
            Ok(f) => {
                let mcap_usd = f.curve_after.price() * SUPPLY_TOKENS * self.sol_usd;
                pos.entry_ms = now;
                pos.fill_buy(f.entry_price, f.tokens, mcap_usd, &f.fees, f.slippage_pct)?;
                self.db.save_position(&pos);
                self.learner.record_features(pos.id, style, &p.features);
                lane.live.lock().unwrap().insert(pos.id, pos.clone());
                lane.risk.lock().unwrap().set_last_slippage(f.slippage_pct);
                self.hub.log(format!(
                    "TRADE[{style}] FILL BUY {} mint={} spent={} SOL tokens={:.2} avgPrice={} slippage={:.2}% delay={}ms fees={} SOL (pump.fun {} + PumpPortal {} + tx {} + priority {})",
                    p.symbol, p.mint, pf(size), f.tokens, pp(f.entry_price), f.slippage_pct, f.delay_ms, pf(f.fees.total()), pf(f.fees.pump_fun), pf(f.fees.pump_portal), pf(f.fees.tx), pf(f.fees.priority)
                ));
                self.hub.emit(EngineEvent::Pulse { from: AgentId::Executor, to: AgentId::HoldTimer, text: format!("{} bought, Hold Timer takes over", p.symbol) });
                self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::Executor, text: format!("[{style}] Paper bought {} for {size} SOL at {}", p.symbol, pf(f.entry_price)), mint: Some(p.mint.clone()), source_url: None });
                let me = self.clone();
                let id = pos.id;
                tokio::spawn(async move { me.manage(lane, id).await });
                Ok(pos.id)
            }
            Err(e) => {
                pos.fail_buy(&e.fees, now, &format!("buy-failed: {}", e.detail));
                self.db.save_position(&pos);
                {
                    let mut r = lane.risk.lock().unwrap();
                    r.cancel_open(&p.mint);
                    r.add_cost(now, pos.failed_sol);
                }
                lane.live.lock().unwrap().remove(&pos.id);
                self.release_feed(&p.mint);
                self.hub.log(format!("TRADE[{style}] BUY FAILED {} {:?}: {} (fees paid {} SOL)", p.symbol, e.kind, e.detail, pf(e.fees.total())));
                self.db.insert_decision(&DecisionRow { ts: now, mint: p.mint.clone(), symbol: p.symbol.clone(), stage: "execution".into(), outcome: "block".into(), reason: format!("BUY_{:?}", e.kind).to_uppercase(), detail: e.detail.clone(), verdict: String::new() });
                Err(e.detail)
            }
        }
    }

    // ------------------------------------------------------------ restart

    /// After a restart: close rows that never filled, put OPEN/SELLING positions back under management.
    pub fn resume(self: &Arc<Self>) -> usize {
        let now = now_ms();
        for mut p in self.db.load_buying_positions() {
            p.fail_buy(&FeeBreakdown::default(), now, "crash-before-fill");
            self.db.save_position(&p);
            self.hub.log(format!("TRADE[{}] restart: buy of {} never filled, row closed", p.lane, p.symbol));
        }
        let mut n = 0;
        for mut p in self.db.load_live_positions() {
            let Some(lane) = self.lane(&p.lane) else { continue };
            p.state = PosState::Open; // a sell that was in flight left no fill in the DB: it did not happen
            self.db.save_position(&p);
            {
                let mut r = lane.risk.lock().unwrap();
                r.record_open(p.entry_ms, &p.mint, p.size_sol, p.slippage_pct);
            }
            lane.live.lock().unwrap().insert(p.id, p.clone());
            self.hold_feed(&p.mint, &p.curve);
            self.hub.log(format!(
                "TRADE[{}] RESUMED {} mint={} entryPrice={} tokensLeft={:.2} tranchesDone={} heldSoFar={:.0}s",
                p.lane, p.symbol, p.mint, pp(p.entry_price), p.tokens_remaining, p.tranches_done, p.hold_seconds(now)
            ));
            let me = self.clone();
            let id = p.id;
            tokio::spawn(async move { me.manage(lane, id).await });
            n += 1;
        }
        n
    }

    // ------------------------------------------------------------ exit loop

    async fn manage(self: Arc<Self>, lane: Arc<Lane>, id: i64) {
        let every = Duration::from_millis(self.cfg.exits.exit_loop_ms as u64);
        let style = lane.style.clone();
        let mut tick = 0u64;
        loop {
            tokio::time::sleep(every).await;
            let Some(mut pos) = lane.live.lock().unwrap().get(&id).cloned() else { return };
            if pos.state == PosState::Closed {
                return;
            }
            if !self.replay && tick % 2 == 0 {
                self.poll_curve(&pos.mint, &pos.curve).await;
            }
            tick += 1;
            let now = now_ms();
            let curve = self.current_curve(&pos.mint);
            let price = match &curve {
                Some(c) if pos.tokens_remaining > 0.0 => {
                    let g = c.sell(pos.tokens_remaining).0 / pos.tokens_remaining;
                    if g > 0.0 { g } else { pos.last_price }
                }
                _ => pos.last_price,
            };
            let trades: Vec<Trade> = self.feed.store.snapshot(&pos.mint);
            let since = self.feed.connected_since_ms.load(std::sync::atomic::Ordering::Relaxed);
            let ok_since = if self.replay { 0 } else if since == 0 { now } else { since };
            let migrated = self.migrated.lock().unwrap().contains(&pos.mint);
            let real_sol = curve.map(|c| c.real_sol).unwrap_or(0.0);
            let market = Market { now_ms: now, price, real_sol, trades: &trades, feed_ok_since_ms: ok_since, migrated };
            let action = exits::evaluate(&mut pos, &market, &self.lane_cfg(&style));
            // kill switch: sell everything that is left, whatever the exit rules say. A failed sell leaves the
            // position OPEN and this loop tries again every tick with the escalated fee (SPEC 11b #17).
            let action = if self.is_killed() && pos.tokens_remaining > 0.0 { Some(ExitAction::all("kill-switch")) } else { action };
            lane.live.lock().unwrap().insert(id, pos.clone());
            self.db.save_position(&pos);
            if let Some(a) = action {
                self.do_sell(&lane, id, a).await;
                let closed = lane.live.lock().unwrap().get(&id).map(|p| p.state == PosState::Closed).unwrap_or(true);
                if closed {
                    return;
                }
            }
        }
    }

    async fn do_sell(self: &Arc<Self>, lane: &Arc<Lane>, id: i64, a: ExitAction) {
        let Some(mut pos) = lane.live.lock().unwrap().get(&id).cloned() else { return };
        if let Err(e) = pos.begin_sell() {
            self.hub.log(format!("TRADE[{}] {} {e}", lane.style, pos.symbol));
            return;
        }
        lane.live.lock().unwrap().insert(id, pos.clone());
        self.db.save_position(&pos);
        let rem = pos.tokens_remaining;
        let mut qty = if a.sell_all { rem } else if a.of_remaining { rem * a.fraction } else { (pos.tokens_initial * a.fraction).min(rem) };
        if rem - qty < pos.tokens_initial * 1e-6 {
            qty = rem;
        }
        let mint = pos.mint.clone();
        let out = self.exec.sell(qty, &a.reason, pos.sell_fails, &now_ms, &|| self.current_curve(&mint)).await;
        let now = now_ms();
        let full = out.fill.as_ref().map(|(s, _)| s.tokens >= qty * 0.999).unwrap_or(false);
        let attempts: Vec<String> = out.attempts.iter().map(|x| format!("#{} prio={} qty={:.0} {}", x.attempt, x.priority_sol, x.tokens, x.result)).collect();
        match &out.fill {
            Some((s, f)) => self.hub.log(format!(
                "TRADE[{}] FILL SELL {} reason={} tokens={:.2} avgPrice={} gross={} net={} SOL fees={} SOL attempts=[{}]",
                lane.style, pos.symbol, a.reason, s.tokens, pp(s.price), pf(s.gross_sol), pf(s.net_sol), pf(f.total()), attempts.join("; ")
            )),
            None => self.hub.log(format!("TRADE[{}] SELL FAILED {} reason={} attempts=[{}] (stays OPEN, loop retries with escalated fee)", lane.style, pos.symbol, a.reason, attempts.join("; "))),
        }
        if let Err(e) = pos.end_sell(out.fill, &out.failed_fees, now, &a.reason) {
            self.hub.log(format!("TRADE[{}] internal: {e}", lane.style));
            return;
        }
        if full {
            if let Some(t) = a.tranche {
                pos.tranches_done = t as u32 + 1;
            }
            if a.taper {
                pos.taper_done = true;
            }
        }
        self.db.save_position(&pos);
        lane.live.lock().unwrap().insert(id, pos.clone());
        if pos.state == PosState::Closed {
            self.finalize_close(lane, pos);
        }
    }

    fn finalize_close(&self, lane: &Arc<Lane>, pos: Position) {
        let exit_ms = pos.exit_ms.unwrap_or_else(now_ms);
        let line = TradeLine::from_position(&pos);
        let path = report::append_line(&self.logs_dir, &lane.style, exit_ms, &line);
        {
            let mut r = lane.risk.lock().unwrap();
            let was = r.halt.clone();
            r.record_close(exit_ms, &pos.mint, pos.size_sol, pos.pnl_sol, true);
            if r.halt.is_some() && was.is_none() {
                let h = r.halt.clone().unwrap_or_default();
                self.db.kv_set(&format!("halt:{}", lane.style), &h);
                self.hub.log(format!("TRADE[{}] HALT: {h}. No new entries until a manual restart (--clear-halt).", lane.style));
            }
        }
        let ticks = self.ticks(&pos.mint);
        let view = self.view_of(&pos, pos.last_price, pos.pnl_sol, ticks);
        {
            let mut rc = lane.recent.lock().unwrap();
            rc.push_front(view);
            while rc.len() > 3 {
                rc.pop_back();
            }
        }
        lane.live.lock().unwrap().remove(&pos.id);
        self.release_feed(&pos.mint);
        self.hub.log(format!(
            "TRADE[{}] CLOSED {} reason={} pnl={:+.9} SOL ({:+.2}%) hold={}s fees={} SOL log={}",
            lane.style, pos.symbol, pos.exit_reason, pos.pnl_sol, pos.pnl_pct, line.hold_seconds, pf(pos.fees.total()),
            path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|e| e.clone())
        ));
        self.hub.log(format!("TRADE[{}] JSONL {}", lane.style, serde_json::to_string(&line).unwrap_or_default()));
        if let Ok(s) = report::write_daily_summary(&self.logs_dir, &lane.style, &crate::timefmt::date_local(exit_ms)) {
            self.hub.log(report::print_summary(&lane.style, &s));
        }
        let src = self.learner.features_of(pos.id).map(|f| f.source_url).filter(|u| !u.is_empty());
        self.hub.emit(EngineEvent::Activity { ts: exit_ms, agent: AgentId::HoldTimer, text: format!("[{}] Closed {}: {} ({:+.4} SOL)", lane.style, pos.symbol, pos.exit_reason, pos.pnl_sol), mint: Some(pos.mint.clone()), source_url: src });
        // the learning journal: one entry per closed trade, then a review or a shadow verdict if one is due
        let fx = self.learner.on_close(&pos);
        self.apply_effects(fx);
    }

    // ------------------------------------------------------------ UI snapshots

    fn ticks(&self, mint: &str) -> Vec<[f64; 2]> {
        let mut t = self.feed.store.snapshot(mint);
        t.sort_by_key(|x| (x.slot, x.recv_ms));
        let skip = t.len().saturating_sub(900);
        t.iter().skip(skip).filter(|x| x.price() > 0.0).map(|x| [x.recv_ms as f64, x.price()]).collect()
    }

    fn view_of(&self, p: &Position, cur_price: f64, pnl_sol: f64, ticks: Vec<[f64; 2]>) -> PositionView {
        let lane_cfg = self.lane_cfg(&p.lane);
        let lv = exits::levels(p, &lane_cfg);
        let now = now_ms();
        let closed = p.state == PosState::Closed;
        PositionView {
            id: p.id,
            lane: p.lane.clone(),
            mint: p.mint.clone(),
            symbol: p.symbol.clone(),
            state: p.state.as_str().into(),
            entry_ts: p.entry_ms,
            entry_price: p.entry_price,
            current_price: cur_price,
            size_sol: p.size_sol,
            tokens_remaining: p.tokens_remaining,
            pnl_sol,
            pnl_pct: if p.size_sol > 0.0 { pnl_sol / p.size_sol * 100.0 } else { 0.0 },
            price_pct: p.pct(cur_price),
            hold_seconds: if closed { (p.exit_ms.unwrap_or(now) - p.entry_ms) as f64 / 1000.0 } else { p.hold_seconds(now) },
            next_action: if closed { format!("Closed: {}", p.exit_reason) } else if p.state == PosState::Selling { "Selling now".into() } else { exits::next_action(p, cur_price, now, &lane_cfg) },
            soft_stop: lv.soft_stop,
            hard_stop: lv.hard_stop,
            take_profits: lv.take_profits,
            tranches_done: p.tranches_done,
            exits: p.stages.iter().map(|s| ExitMark { ts: s.ts, price: s.price, reason: s.reason.clone() }).collect(),
            exit_reason: p.exit_reason.clone(),
            ticks,
            replay: self.replay,
        }
    }

    /// Net P/L if the remaining tokens were sold right now through the curve, all fees paid.
    fn open_pnl(&self, p: &Position) -> (f64, f64) {
        let price = p.last_price;
        match self.current_curve(&p.mint) {
            Some(c) if p.tokens_remaining > 0.0 => {
                let (g, _) = c.sell(p.tokens_remaining);
                let net = g * (1.0 - self.fees.pct_per_side() / 100.0) - self.fees.base_tx_sol - self.fees.priority_sol - self.fees.jito_sol;
                (p.open_pnl(net), if g > 0.0 { g / p.tokens_remaining } else { price })
            }
            _ => (p.open_pnl(0.0).max(-p.size_sol), price),
        }
    }

    pub fn positions_event(&self) -> EngineEvent {
        let mut v = vec![];
        for l in &self.lanes {
            let live: Vec<Position> = l.live.lock().unwrap().values().filter(|p| p.state != PosState::Buying).cloned().collect();
            for p in live {
                let (pnl, px) = self.open_pnl(&p);
                v.push(self.view_of(&p, px, pnl, self.ticks(&p.mint)));
            }
            v.extend(l.recent.lock().unwrap().iter().cloned());
        }
        EngineEvent::Positions { ts: now_ms(), positions: v }
    }

    pub fn stats_event(&self) -> EngineEvent {
        let now = now_ms();
        let today = day_of(now);
        let mut lanes = vec![];
        let mut unrealized = 0.0;
        let mut rent_locked = 0.0;
        for (i, l) in self.lanes.iter().enumerate() {
            let closed = self.db.load_closed(&l.style, true);
            let live: Vec<Position> = l.live.lock().unwrap().values().filter(|p| p.state != PosState::Buying).cloned().collect();
            if l.style == SHADOW_LANE && closed.is_empty() && live.is_empty() {
                continue; // idle: no test is running
            }
            let mut st = LaneStats { lane: l.style.clone(), open: live.len() as u32, ..Default::default() };
            let mut lines = vec![];
            let mut eq = vec![];
            let mut cum = self.cfg.paper.bankroll_sol;
            let mut days = HashSet::new();
            for p in &closed {
                let exit = p.exit_ms.unwrap_or(p.entry_ms);
                cum += p.pnl_sol;
                eq.push([exit as f64, cum]);
                st.fees_all.add(&p.fees);
                let is_today = day_of(exit) == today;
                if is_today {
                    st.fees_today.add(&p.fees);
                    st.pnl_today += p.pnl_sol;
                }
                if p.kind == "trade" {
                    lines.push(TradeLine::from_position(p));
                    days.insert(day_of(exit));
                    if is_today {
                        st.trades_today += 1;
                    }
                } else {
                    st.failed_buys += 1;
                }
                if day_of(p.entry_ms) == today && p.kind == "trade" {
                    st.buys_today += 1;
                }
                st.sells_today += p.stages.iter().filter(|s| day_of(s.ts) == today).count() as u32;
            }
            for p in &live {
                st.fees_all.add(&p.fees);
                if day_of(p.entry_ms) == today {
                    st.fees_today.add(&p.fees);
                    st.buys_today += 1;
                    st.trades_today += 1;
                }
                st.sells_today += p.stages.iter().filter(|s| day_of(s.ts) == today).count() as u32;
                if i == 0 {
                    unrealized += self.open_pnl(p).0;
                    rent_locked += p.rent_locked();
                }
            }
            st.rent_locked_sol = live.iter().map(|p| p.rent_locked()).sum();
            let s = report::summarize("all", &lines, days.len().max(1) as f64);
            st.trades = s.trades;
            st.wins = s.wins;
            st.losses = s.losses;
            st.win_rate_pct = s.win_rate_pct;
            st.avg_win_sol = s.avg_win_sol;
            st.avg_loss_sol = s.avg_loss_sol;
            st.profit_factor = s.profit_factor;
            st.max_drawdown_sol = s.max_drawdown_sol;
            st.total_pnl_sol = cum - self.cfg.paper.bankroll_sol;
            st.best_sol = s.best_sol;
            st.worst_sol = s.worst_sol;
            st.trades_per_day = s.trades_per_day;
            st.fee_per_trade = if s.trades > 0 { st.fees_all.total() / s.trades as f64 } else { 0.0 };
            let eq_keep = eq.len().saturating_sub(300);
            st.equity = eq.into_iter().skip(eq_keep).collect();
            let r = l.risk.lock().unwrap();
            st.halted = r.halt.clone();
            st.paused = r.regime_paused(now);
            lanes.push(st);
        }
        let main = lanes.first().map(|l| l.total_pnl_sol).unwrap_or(0.0);
        EngineEvent::Stats { ts: now, balance_sol: self.cfg.paper.bankroll_sol + main + unrealized - rent_locked, bankroll_sol: self.cfg.paper.bankroll_sol, sol_usd: self.sol_usd, replay: self.replay, lanes }
    }

    /// 1 s publisher: positions every tick, stats every 2nd tick, agent statuses.
    pub fn spawn_ticker(self: &Arc<Self>, mut stop: tokio::sync::watch::Receiver<bool>) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut n = 0u64;
            let mut iv = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = stop.changed() => { if *stop.borrow() { return; } }
                    _ = iv.tick() => {}
                }
                me.sync_kill();
                me.hub.emit(me.positions_event());
                me.hub.emit(EngineEvent::Kill(me.kill_state()));
                if n % 2 == 0 {
                    me.hub.emit(me.stats_event());
                }
                n += 1;
                let open: usize = me.lanes.iter().map(|l| l.live.lock().unwrap().len()).sum();
                let halted = me.lanes.first().and_then(|l| l.risk.lock().unwrap().halt.clone());
                let ks = me.kill_state();
                let (c, line) = match (&halted, open) {
                    _ if ks.killed => (StatusColor::Red, ks.text.clone()),
                    (Some(h), _) => (StatusColor::Red, format!("Trading halted: {h}")),
                    (None, 0) => (StatusColor::Grey, "No open positions".to_string()),
                    (None, n) => (StatusColor::Green, format!("Managing {n} open position(s), checking every {} ms", me.cfg.exits.exit_loop_ms)),
                };
                me.hub.emit(EngineEvent::Status(AgentStatus { agent: AgentId::HoldTimer, color: c, line }));
                let (c, line) = match &halted {
                    _ if ks.killed => (StatusColor::Red, ks.text.clone()),
                    Some(h) => (StatusColor::Red, format!("Halted: {h}")),
                    None => (if open > 0 { StatusColor::Green } else { StatusColor::Amber }, format!("Paper executor ready. {open} open")),
                };
                me.hub.emit(EngineEvent::Status(AgentStatus { agent: AgentId::Executor, color: c, line }));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Sim;
    use crate::picker;

    fn test_cfg() -> Config {
        let mut c = Config::from_json(include_str!("../../config.json")).unwrap();
        c.exits.exit_loop_ms = 20;
        c.paper_fill.buy_fail_pct = 0.0;
        c.paper_fill.sell_fail_pct = 0.0;
        c.paper_fill.delay_ms_min = 0;
        c.paper_fill.delay_ms_max = 0;
        c
    }

    fn mk(cfg: Config, db: Arc<Db>, dir: &std::path::Path) -> (Arc<Trader>, Arc<Hub>) {
        let hub = Hub::new();
        let t = Trader::new(cfg, db, hub.clone(), TradeFeed::offline(), None, 150.0, dir.to_path_buf(), true);
        (t, hub)
    }

    fn pick() -> Pick {
        Pick { mint: "TESTMINT".into(), symbol: "TST".into(), creator: "CREATOR".into(), curve_key: "CURVE".into(), score: 90.0, features: EntryFeatures::default(), size_factor: 1.0 }
    }

    /// Put a hot-looking curve into the store (14 SOL in the curve, price up).
    fn seed(t: &Trader, sim: &mut Sim) {
        t.hold_feed("TESTMINT", "CURVE");
        for i in 0..14 {
            sim.trade(i as f64, &format!("b{i}"), true, 1.0);
        }
        push(t, sim);
    }

    /// Move every simulated trade not yet in the store into it, stamped with the current time.
    fn push(t: &Trader, sim: &mut Sim) {
        for (_, tr) in sim.out.drain(..) {
            t.feed.store.add(Trade { recv_ms: now_ms(), ..tr });
        }
    }

    async fn wait_closed(t: &Trader, lane: &str) {
        let l = t.lane(lane).unwrap();
        for _ in 0..400 {
            if l.live.lock().unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("position did not close");
    }

    #[test]
    fn size_factor_halves_a_trade_and_never_raises_it() {
        assert_eq!(scaled_size(0.2, 1.0), 0.2);
        assert_eq!(scaled_size(0.2, 0.5), 0.1);
        assert_eq!(scaled_size(0.1, 0.5), 0.05);
        assert_eq!(scaled_size(0.2, 2.0), 0.2, "a factor above 1 is ignored");
        assert_eq!(scaled_size(0.2, 0.0), 0.2, "a bad factor falls back to normal size");
        assert_eq!(scaled_size(0.2, f64::NAN), 0.2);
    }

    #[tokio::test]
    async fn a_coin_that_skipped_rugcheck_is_bought_at_half_the_normal_size() {
        let dir = std::env::temp_dir().join(format!("ct-half-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _hub) = mk(test_cfg(), db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let normal = t.lane("momentum").unwrap().risk.lock().unwrap().size();
        let half = Pick { size_factor: 0.5, ..pick() };
        let id = t.try_enter("momentum", &half).await.expect("a half-size entry passes the risk gates");
        let p = db.position_by_id(id).unwrap();
        assert!((p.size_sol - normal * 0.5).abs() < 1e-9, "size {} vs normal {normal}", p.size_sol);
        assert!(normal > 0.0 && p.size_sol < normal);
    }

    #[tokio::test]
    async fn full_paper_trade_signal_fill_staged_exits_jsonl() {
        let dir = std::env::temp_dir().join(format!("ct-trader-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _hub) = mk(test_cfg(), db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let id = t.try_enter("momentum", &pick()).await.expect("entry");
        let open = db.position_by_id(id).unwrap();
        assert_eq!(open.state, PosState::Open);
        assert!(open.entry_price > 0.0 && open.tokens_initial > 0.0);
        // price climbs through +20/+40/+60%, then sells take it more than 10% off the peak
        for i in 0..24 {
            sim.trade(30.0 + i as f64, &format!("up{i}"), true, 1.0);
            push(&t, &mut sim);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        for i in 0..10 {
            sim.trade(60.0 + i as f64, &format!("dn{i}"), false, 0.8);
            push(&t, &mut sim);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        wait_closed(&t, "momentum").await;
        let p = db.position_by_id(id).unwrap();
        assert_eq!(p.state, PosState::Closed);
        let reasons: Vec<&str> = p.stages.iter().map(|s| s.reason.as_str()).collect();
        assert_eq!(reasons, vec!["take-profit-1", "take-profit-2", "take-profit-3", "trailing-stop"], "staged exits");
        let sold: f64 = p.stages.iter().map(|s| s.tokens).sum();
        assert!((sold - p.tokens_initial).abs() / p.tokens_initial < 1e-6);
        assert!((p.stages[0].tokens / p.tokens_initial - 0.40).abs() < 1e-9 && (p.stages[1].tokens / p.tokens_initial - 0.30).abs() < 1e-9 && (p.stages[2].tokens / p.tokens_initial - 0.20).abs() < 1e-9);
        // P/L recomputed by hand from the stages
        let net: f64 = p.stages.iter().map(|s| s.net_sol).sum();
        let buy_fixed = 0.000005 + 0.0005;
        assert!((p.pnl_sol - (net - p.size_sol - buy_fixed)).abs() < 1e-12, "{} vs {}", p.pnl_sol, net - p.size_sol - buy_fixed);
        // JSONL
        let day = crate::timefmt::date_local(p.exit_ms.unwrap());
        let text = std::fs::read_to_string(dir.join(format!("{day}.jsonl"))).unwrap();
        assert_eq!(text.lines().count(), 1);
        let line: TradeLine = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line.exit_reason, "trailing-stop");
        assert!((line.pnl_sol - p.pnl_sol).abs() < 1e-8 && line.mode == "paper" && line.side == "buy");
        assert!(line.exit_price > line.entry_price, "a winner");
        assert!(dir.join(format!("{day}-summary.json")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Run one scripted market through the real entry gates, the exit loop and the risk gates with the executor given,
    /// and return every DECISION it made (never the fills): risk-gate answers, exit reasons, how much each exit sold.
    async fn decisions_with(exec: Arc<dyn Executor>, tag: &str) -> Vec<String> {
        let dir = std::env::temp_dir().join(format!("ct-same-{tag}-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let hub = Hub::new();
        let t = Trader::with_executor(test_cfg(), db.clone(), hub, TradeFeed::offline(), None, 150.0, dir.clone(), true, exec);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let mut out = vec![];
        // the picker's own verdict on what the market looks like (pure code, same for any executor)
        let obs = picker::observe(&t.feed.store.snapshot("TESTMINT"), "CREATOR", 0.0, 3.0e-8, 0.0, 60.0, now_ms());
        let sc = picker::score(&obs, &t.cfg);
        out.push(format!("picker score={:.1} picked={} gates={:?}", sc.score, sc.picked, sc.gates));
        let id = t.try_enter("momentum", &pick()).await.expect("entry");
        out.push("entry: allowed".into());
        // the same coin again at once: the risk gates must say no (cooldown / already open), whatever the executor
        let again = t.try_enter("momentum", &pick()).await.err().unwrap_or_default();
        out.push(format!("second entry: {again}"));
        for i in 0..24 {
            sim.trade(30.0 + i as f64, &format!("up{i}"), true, 1.0);
            push(&t, &mut sim);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        for i in 0..10 {
            sim.trade(60.0 + i as f64, &format!("dn{i}"), false, 0.8);
            push(&t, &mut sim);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
        wait_closed(&t, "momentum").await;
        let p = db.position_by_id(id).unwrap();
        for s in &p.stages {
            out.push(format!("exit {} sold {:.0}% of the position", s.reason, s.tokens / p.tokens_initial * 100.0));
        }
        out.push(format!("closed by {}", p.exit_reason));
        let rows: Vec<String> = db.with_conn(|c| {
            let mut st = c.prepare("SELECT stage,outcome,reason FROM decisions ORDER BY id").unwrap();
            st.query_map([], |r| Ok(format!("decision {}/{}/{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))).unwrap().flatten().collect()
        });
        out.extend(rows);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    /// SPEC 4b: paper is practice for live with the same rules. Only the executor differs, so the same market must
    /// produce the same decisions with the paper executor and with a different (scripted) executor.
    #[tokio::test]
    async fn same_market_same_decisions_with_paper_and_scripted_executors() {
        let cfg = test_cfg();
        let paper: Arc<dyn Executor> = {
            let mut e = PaperExecutor::new(&cfg, Rng::new(7));
            e.time_scale = 0.0;
            Arc::new(e)
        };
        let scripted: Arc<dyn Executor> = Arc::new(crate::exec::ScriptedExecutor::new(&cfg));
        assert_eq!((paper.name(), scripted.name()), ("paper", "scripted"));
        let a = decisions_with(paper, "paper").await;
        let b = decisions_with(scripted, "scripted").await;
        assert_eq!(a, b, "decisions differ between executors:\n{a:#?}\n{b:#?}");
        assert!(a.iter().any(|l| l.starts_with("exit take-profit-1")) && a.iter().any(|l| l.starts_with("closed by trailing-stop")), "{a:#?}");
        assert!(a.iter().any(|l| l.starts_with("second entry:") && l.len() > "second entry: ".len()), "the second entry was refused: {a:#?}");
    }

    #[test]
    fn paper_lists_what_it_cannot_run_and_the_scripted_executor_hides_nothing() {
        let cfg = test_cfg();
        let p = PaperExecutor::new(&cfg, Rng::new(1));
        let skipped = Executor::skipped_checks(&p);
        assert!(skipped.iter().any(|c| c.name == "Honeypot sell test"));
        assert!(skipped.iter().all(|c| !c.why.contains('\u{2014}')));
    }

    #[tokio::test]
    async fn a_second_sell_for_the_same_mint_is_refused() {
        let dir = std::env::temp_dir().join(format!("ct-trader2-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _hub) = mk(test_cfg(), db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let id = t.try_enter("momentum", &pick()).await.unwrap();
        let lane = t.lane("momentum").unwrap();
        let all = || ExitAction { reason: "hard-stop".into(), sell_all: true, fraction: 1.0, of_remaining: true, tranche: None, taper: false };
        let (a, b) = tokio::join!(t.do_sell(&lane, id, all()), t.do_sell(&lane, id, all()));
        let _ = (a, b);
        let p = db.position_by_id(id).unwrap();
        assert_eq!(p.stages.len(), 1, "exactly one sell happened");
        assert_eq!(p.state, PosState::Closed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn open_position_survives_a_restart_with_same_entry_and_pnl() {
        let dir = std::env::temp_dir().join(format!("ct-trader3-{}", now_ms()));
        let dbp = std::env::temp_dir().join(format!("ct-trader3-{}.db", now_ms()));
        let (entry, tokens, size);
        {
            let db = Arc::new(Db::open(dbp.to_str().unwrap()).unwrap());
            let (t, _h) = mk(test_cfg(), db.clone(), &dir);
            let mut sim = Sim::new("TESTMINT");
            seed(&t, &mut sim);
            let id = t.try_enter("momentum", &pick()).await.unwrap();
            let p = db.position_by_id(id).unwrap();
            (entry, tokens, size) = (p.entry_price, p.tokens_initial, p.size_sol);
        } // "kill": everything in memory is dropped
        let db2 = Arc::new(Db::open(dbp.to_str().unwrap()).unwrap());
        let (t2, hub2) = mk(test_cfg(), db2.clone(), &dir);
        let mut sim2 = Sim::new("TESTMINT");
        seed(&t2, &mut sim2); // the market is still there after the restart
        assert_eq!(t2.resume(), 1);
        let lane = t2.lane("momentum").unwrap();
        let live: Vec<Position> = lane.live.lock().unwrap().values().cloned().collect();
        assert_eq!(live.len(), 1);
        assert_eq!((live[0].entry_price, live[0].tokens_initial, live[0].size_sol), (entry, tokens, size));
        assert_eq!(lane.risk.lock().unwrap().open.len(), 1, "risk gates know about the resumed position");
        assert!(matches!(t2.positions_event(), EngineEvent::Positions { positions, .. } if positions.len() == 1 && positions[0].entry_price == entry));
        let _ = hub2;
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(dbp);
    }

    #[tokio::test]
    async fn failed_buy_costs_fees_and_leaves_no_trade() {
        let mut c = test_cfg();
        c.paper_fill.buy_fail_pct = 100.0;
        let dir = std::env::temp_dir().join(format!("ct-trader4-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _h) = mk(c, db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let r = t.try_enter("momentum", &pick()).await;
        assert!(r.is_err());
        assert!(t.lane("momentum").unwrap().risk.lock().unwrap().open.is_empty());
        assert!(db.load_closed("momentum", false).is_empty(), "no trade recorded");
        assert!(t.lane("momentum").unwrap().live.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn risk_gate_block_is_recorded_as_a_decision() {
        let dir = std::env::temp_dir().join(format!("ct-trader5-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _h) = mk(test_cfg(), db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        t.lane("momentum").unwrap().risk.lock().unwrap().halt = Some("test halt".into());
        assert_eq!(t.try_enter("momentum", &pick()).await.unwrap_err(), "HALTED");
        assert_eq!(db.scalar("SELECT COUNT(*) FROM decisions WHERE stage='risk-gate' AND reason='HALTED'"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn seed_mint(t: &Trader, sim: &mut Sim, mint: &str) {
        t.hold_feed(mint, "CURVE");
        for i in 0..14 {
            sim.trade(i as f64, &format!("b{i}"), true, 1.0);
        }
        push(t, sim);
    }

    fn pick_of(mint: &str) -> Pick {
        Pick { mint: mint.into(), symbol: "KIL".into(), creator: "CREATOR".into(), curve_key: "CURVE".into(), score: 90.0, features: EntryFeatures::default(), size_factor: 1.0 }
    }

    #[tokio::test]
    async fn kill_switch_closes_every_position_in_every_lane_and_blocks_new_entries() {
        let dir = std::env::temp_dir().join(format!("ct-kill1-{}", now_ms()));
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _h) = mk(test_cfg(), db.clone(), &dir);
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let a = t.try_enter("momentum", &pick()).await.unwrap();
        let b = t.try_enter("dip-40pct", &pick()).await.unwrap();
        assert!(!t.is_killed());
        t.kill("test");
        assert!(t.is_killed() && db.killed_at().is_some(), "persisted");
        // no new entries, in any lane
        assert_eq!(t.try_enter("momentum", &pick_of("OTHER")).await.unwrap_err(), "KILLED");
        assert_eq!(t.try_enter("dip-40pct", &pick_of("OTHER")).await.unwrap_err(), "KILLED");
        wait_closed(&t, "momentum").await;
        wait_closed(&t, "dip-40pct").await;
        for id in [a, b] {
            let p = db.position_by_id(id).unwrap();
            assert_eq!(p.state, PosState::Closed);
            assert_eq!(p.exit_reason, "kill-switch");
            assert!(p.tokens_remaining == 0.0 && p.stages.len() == 1);
        }
        let day = crate::timefmt::date_local(db.position_by_id(a).unwrap().exit_ms.unwrap());
        let text = std::fs::read_to_string(dir.join(format!("{day}.jsonl"))).unwrap();
        assert!(text.contains("\"exitReason\":\"kill-switch\""));
        let ks = t.kill_state();
        assert!(ks.killed && ks.closing == 0 && ks.closed == 2, "{ks:?}");
        assert!(ks.text.contains("all closed"), "{}", ks.text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn killed_state_survives_a_restart_and_resume_needs_its_own_step() {
        let dir = std::env::temp_dir().join(format!("ct-kill2-{}", now_ms()));
        let dbp = std::env::temp_dir().join(format!("ct-kill2-{}.db", now_ms()));
        let id;
        {
            let db = Arc::new(Db::open(dbp.to_str().unwrap()).unwrap());
            let (t, _h) = mk(test_cfg(), db.clone(), &dir);
            let mut sim = Sim::new("TESTMINT");
            seed(&t, &mut sim);
            id = t.try_enter("momentum", &pick()).await.unwrap();
            db.kv_set("killed", &now_ms().to_string()); // what `engine-cli kill` / the button writes
        }
        // restart: still killed, the open position is picked up and sold, nothing is bought
        let db2 = Arc::new(Db::open(dbp.to_str().unwrap()).unwrap());
        let (t2, _h2) = mk(test_cfg(), db2.clone(), &dir);
        assert!(t2.is_killed(), "restart stays stopped");
        let mut sim2 = Sim::new("TESTMINT");
        seed(&t2, &mut sim2);
        assert_eq!(t2.resume(), 1);
        wait_closed(&t2, "momentum").await;
        assert_eq!(db2.position_by_id(id).unwrap().exit_reason, "kill-switch");
        assert_eq!(t2.try_enter("momentum", &pick_of("M2")).await.unwrap_err(), "KILLED");
        // a third start is still killed
        let db3 = Arc::new(Db::open(dbp.to_str().unwrap()).unwrap());
        let (t3, _h3) = mk(test_cfg(), db3.clone(), &dir);
        assert!(t3.is_killed());
        // resume is explicit
        assert_eq!(t3.resume_trading("test"), "trading resumed");
        assert!(!t3.is_killed() && db3.killed_at().is_none());
        let mut sim3 = Sim::new("M3");
        seed_mint(&t3, &mut sim3, "M3");
        assert!(t3.try_enter("momentum", &pick_of("M3")).await.is_ok(), "buying works again after resume");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(dbp);
    }

    #[tokio::test]
    async fn kill_switch_keeps_retrying_failed_sells_until_the_position_is_closed() {
        let dir = std::env::temp_dir().join(format!("ct-kill3-{}", now_ms()));
        let mut c = test_cfg();
        c.paper_fill.sell_fail_pct = 100.0; // every sell attempt fails
        let db = Arc::new(Db::open(":memory:").unwrap());
        let (t, _h) = mk(c, db.clone(), &dir);
        t.exec.set_unconfirmed_landed_pct(0.0); // a failed sell really failed
        let mut sim = Sim::new("TESTMINT");
        seed(&t, &mut sim);
        let id = t.try_enter("momentum", &pick()).await.unwrap();
        t.kill("test");
        tokio::time::sleep(Duration::from_millis(400)).await;
        let p = db.position_by_id(id).unwrap();
        assert_ne!(p.state, PosState::Closed, "still open while every sell fails");
        assert!(p.sell_fails >= 6 && p.tokens_remaining > 0.0, "retried: sell_fails={}", p.sell_fails);
        let ks = t.kill_state();
        assert_eq!(ks.closing, 1);
        assert!(ks.items[0].status.contains("retrying") || ks.items[0].status.contains("selling"), "{:?}", ks.items);
        assert!(ks.text.contains("1 position(s) closing"));
        // the market lets the sell through again: the loop closes it without any new command
        t.exec.set_sell_fail_pct(0.0);
        wait_closed(&t, "momentum").await;
        let p = db.position_by_id(id).unwrap();
        assert_eq!((p.state, p.exit_reason.as_str()), (PosState::Closed, "kill-switch"));
        assert!(p.failed_sol >= 0.0 && p.sell_fails == 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
