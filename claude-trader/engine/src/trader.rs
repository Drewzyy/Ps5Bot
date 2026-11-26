//! Trader: risk gates -> paper buy -> persisted position -> 1 s exit loop -> paper sells -> JSONL + stats.
//! Paper only. Two lanes run the same engine side by side: `momentum` (the configured entry style) and the
//! paper variant `dip-40pct`; every position, log line and statistic carries its lane so they can be compared.

use crate::config::Config;
use crate::curve::{Curve, FeeBreakdown, FeeModel, Rng};
use crate::db::{Db, DecisionRow};
use crate::events::*;
use crate::exits::{self, ExitAction, Market};
use crate::hub::Hub;
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
}

pub struct Lane {
    pub style: String,
    pub risk: Mutex<RiskState>,
    live: Mutex<HashMap<i64, Position>>,
    recent: Mutex<VecDeque<PositionView>>,
}

pub struct Trader {
    pub cfg: Config,
    pub db: Arc<Db>,
    pub hub: Arc<Hub>,
    pub feed: TradeFeed,
    rpc: Option<Arc<Rpc>>,
    pub exec: PaperExecutor,
    fees: FeeModel,
    pub lanes: Vec<Arc<Lane>>,
    sol_usd: f64,
    logs_dir: PathBuf,
    pub replay: bool,
    feed_refs: Mutex<HashMap<String, u32>>,
    polled: Mutex<HashMap<String, (Curve, i64, bool)>>,
    migrated: Mutex<HashSet<String>>,
    poll_checks: std::sync::atomic::AtomicU32,
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
        let now = now_ms();
        let mut styles = vec!["momentum".to_string()];
        styles.push(cfg.entry.paper_variant.style.clone());
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
                Arc::new(Lane { style: s.clone(), risk: Mutex::new(r), live: Mutex::new(HashMap::new()), recent: Mutex::new(VecDeque::new()) })
            })
            .collect();
        let fees = FeeModel::from_config(&cfg);
        let exec = PaperExecutor::new(&cfg, Rng::from_time());
        Arc::new(Trader {
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
        })
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
        self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::Executor, text: format!("[{lane}] Skipped {}: {detail}", p.symbol), mint: Some(p.mint.clone()) });
    }

    /// Run the risk gates, write the position to SQLite, do the paper buy and start managing it.
    pub async fn try_enter(self: &Arc<Self>, style: &str, p: &Pick) -> Result<i64, String> {
        let Some(lane) = self.lane(style) else { return Err("unknown lane".into()) };
        let now = now_ms();
        let Some(curve) = self.current_curve(&p.mint) else {
            self.block(style, p, "NO_CURVE_DATA", "No curve data for this coin yet.");
            return Err("no curve data".into());
        };
        let size = lane.risk.lock().unwrap().size();
        let (tokens, quote_price, after) = self.exec.quote_buy(&curve, size);
        let sell_impact = if tokens > 0.0 { (1.0 - after.sell(tokens).0 / (after.price() * tokens)) * 100.0 } else { 0.0 };
        let impact = self.exec.buy_impact_pct(&curve, size) + sell_impact;
        let tp1 = self.cfg.exits.tranches.first().map(|t| t.at_pct).unwrap_or(20.0);
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

        let mint = p.mint.clone();
        let res = self.exec.buy(size, quote_price, &|| self.current_curve(&mint)).await;
        let now = now_ms();
        match res {
            Ok(f) => {
                let mcap_usd = f.curve_after.price() * SUPPLY_TOKENS * self.sol_usd;
                pos.entry_ms = now;
                pos.fill_buy(f.entry_price, f.tokens, mcap_usd, &f.fees, f.slippage_pct)?;
                self.db.save_position(&pos);
                lane.live.lock().unwrap().insert(pos.id, pos.clone());
                lane.risk.lock().unwrap().set_last_slippage(f.slippage_pct);
                self.hub.log(format!(
                    "TRADE[{style}] FILL BUY {} mint={} spent={} SOL tokens={:.2} avgPrice={} slippage={:.2}% delay={}ms fees={} SOL (pump.fun {} + PumpPortal {} + tx {} + priority {})",
                    p.symbol, p.mint, pf(size), f.tokens, pp(f.entry_price), f.slippage_pct, f.delay_ms, pf(f.fees.total()), pf(f.fees.pump_fun), pf(f.fees.pump_portal), pf(f.fees.tx), pf(f.fees.priority)
                ));
                self.hub.emit(EngineEvent::Pulse { from: AgentId::Executor, to: AgentId::HoldTimer, text: format!("{} bought, Hold Timer takes over", p.symbol) });
                self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::Executor, text: format!("[{style}] Paper bought {} for {size} SOL at {}", p.symbol, pf(f.entry_price)), mint: Some(p.mint.clone()) });
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
            let action = exits::evaluate(&mut pos, &market, &self.cfg);
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
        self.hub.emit(EngineEvent::Activity { ts: exit_ms, agent: AgentId::HoldTimer, text: format!("[{}] Closed {}: {} ({:+.4} SOL)", lane.style, pos.symbol, pos.exit_reason, pos.pnl_sol), mint: Some(pos.mint.clone()) });
    }

    // ------------------------------------------------------------ UI snapshots

    fn ticks(&self, mint: &str) -> Vec<[f64; 2]> {
        let mut t = self.feed.store.snapshot(mint);
        t.sort_by_key(|x| (x.slot, x.recv_ms));
        let skip = t.len().saturating_sub(900);
        t.iter().skip(skip).filter(|x| x.price() > 0.0).map(|x| [x.recv_ms as f64, x.price()]).collect()
    }

    fn view_of(&self, p: &Position, cur_price: f64, pnl_sol: f64, ticks: Vec<[f64; 2]>) -> PositionView {
        let lv = exits::levels(p, &self.cfg);
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
            next_action: if closed { format!("Closed: {}", p.exit_reason) } else if p.state == PosState::Selling { "Selling now".into() } else { exits::next_action(p, cur_price, now, &self.cfg) },
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
                me.hub.emit(me.positions_event());
                if n % 2 == 0 {
                    me.hub.emit(me.stats_event());
                }
                n += 1;
                let open: usize = me.lanes.iter().map(|l| l.live.lock().unwrap().len()).sum();
                let halted = me.lanes.first().and_then(|l| l.risk.lock().unwrap().halt.clone());
                let (c, line) = match (&halted, open) {
                    (Some(h), _) => (StatusColor::Red, format!("Trading halted: {h}")),
                    (None, 0) => (StatusColor::Grey, "No open positions".to_string()),
                    (None, n) => (StatusColor::Green, format!("Managing {n} open position(s), checking every {} ms", me.cfg.exits.exit_loop_ms)),
                };
                me.hub.emit(EngineEvent::Status(AgentStatus { agent: AgentId::HoldTimer, color: c, line }));
                let (c, line) = match &halted {
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
        Pick { mint: "TESTMINT".into(), symbol: "TST".into(), creator: "CREATOR".into(), curve_key: "CURVE".into(), score: 90.0 }
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
}
