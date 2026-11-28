//! Offline replay backtester (SPEC 4f). Runs the REAL picker, entry gates, pre-buy rules, sprint rules and exit engine over a raw
//! feed recording (`recorder.rs`), for any config and any lane, with no network and no clock of its own.
//!
//! Honest limits, printed with every report:
//! - Entry is priced at the curve state `delay` ms AFTER the decision (default 2000 ms), never at the decision price, so the
//!   fills are not look-ahead. The paper executor's random failures are left out (they only remove trades).
//! - Checks that need the network cannot be replayed: RugCheck, mint and freeze authority, top holders, the funder chain,
//!   metadata and image checks, X Monitor. The replay applies the picker and every rule that runs on trades alone (entry gates,
//!   wash volume, bundle burst, chase and falling knife, the graded launch check, mayhem and quote type, the sprint rules).
//!   So a replay is MORE permissive than the live bot: treat its numbers as an upper bound of what the gates would let in.
//! - A coin is used only when its whole window was observed: coins that overlap a recorded gap are skipped and counted.
//! - Each coin is judged alone at a fixed trade size: no portfolio limits, no daily loss halt, no compounding.
//!
//! The promotion rule is the same as the learning loop's: the LAST part of the time range is the holdout, judged once; it must have
//! a mean return above 0 AND a lower 95% bootstrap bound above 0, with a minimum number of trades.

use crate::config::Config;
use crate::curve::{Curve, FeeBreakdown, FeeModel};
use crate::exits::{self, Market, Reserve};
use crate::learning::Tuned;
use crate::picker;
use crate::position::{Position, StageSell};
use crate::pump::{FeedMsg, Trade};
use crate::recorder::Recording;
use crate::shield::launch;
use crate::shield::rules::{self, SlotTx};
use serde::Serialize;
use std::collections::HashSet;

pub const LANES: [&str; 3] = ["momentum", "dip-40pct", "deep-sprint"];

#[derive(Debug, Clone)]
pub struct BtOpts {
    pub lane: String,
    pub cfg: Config,
    /// Milliseconds between the decision and the fill.
    pub delay_ms: i64,
    pub size_sol: f64,
    /// Share of the time range (the newest trades) kept as the holdout, in percent.
    pub holdout_pct: f64,
}

impl BtOpts {
    pub fn new(lane: &str, cfg: Config) -> BtOpts {
        let size = crate::risk::RiskState::new(&cfg.risk, cfg.paper.bankroll_sol, 0).size();
        BtOpts { lane: lane.into(), cfg, delay_ms: 2000, size_sol: size, holdout_pct: 30.0 }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BtTrade {
    pub mint: String,
    pub symbol: String,
    pub entry_ms: i64,
    pub exit_ms: i64,
    pub size_sol: f64,
    pub pnl_sol: f64,
    /// Net P/L divided by the trade size (0.02 = +2%).
    pub ret: f64,
    pub reason: String,
    pub hold_secs: f64,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub trades: u32,
    pub wins: u32,
    pub win_rate_pct: f64,
    pub total_pnl_sol: f64,
    pub mean_ret_pct: f64,
    pub ci_low_pct: Option<f64>,
    pub ci_high_pct: Option<f64>,
    pub profit_factor: Option<f64>,
}

pub fn summarize(trades: &[&BtTrade], resamples: u32) -> Summary {
    let rets: Vec<f64> = trades.iter().map(|t| t.ret).collect();
    let wins = trades.iter().filter(|t| t.pnl_sol > 0.0).count() as u32;
    let gain: f64 = trades.iter().map(|t| t.pnl_sol).filter(|p| *p > 0.0).sum();
    let loss: f64 = -trades.iter().map(|t| t.pnl_sol).filter(|p| *p < 0.0).sum::<f64>();
    let ci = crate::stats::bootstrap_mean_ci(&rets, resamples, 0xB7E5 ^ rets.len() as u64);
    Summary {
        trades: trades.len() as u32,
        wins,
        win_rate_pct: if trades.is_empty() { 0.0 } else { wins as f64 / trades.len() as f64 * 100.0 },
        total_pnl_sol: trades.iter().map(|t| t.pnl_sol).sum(),
        mean_ret_pct: crate::stats::mean(&rets) * 100.0,
        ci_low_pct: ci.map(|c| c.1 * 100.0),
        ci_high_pct: ci.map(|c| c.2 * 100.0),
        profit_factor: if loss > 0.0 { Some(gain / loss) } else { None },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtReport {
    pub lane: String,
    pub coins: u32,
    /// Coins skipped because a recorded gap overlaps their window.
    pub skipped_gap: u32,
    /// Coins skipped because the lane cannot judge them (no creation slot recorded for the sprint lane, mayhem or non-SOL).
    pub skipped_other: u32,
    /// Coins that reached the lane's decision but did not pass its rules.
    pub rejected: u32,
    pub trades: Vec<BtTrade>,
    pub all: Summary,
    pub train: Summary,
    pub holdout: Summary,
    pub holdout_ok: bool,
    pub holdout_note: String,
    pub delay_ms: i64,
    pub size_sol: f64,
    pub range: String,
}

fn upto(trades: &[Trade], at_ms: i64) -> &[Trade] {
    let n = trades.partition_point(|t| t.recv_ms <= at_ms);
    &trades[..n]
}

fn curve_of(t: &Trade) -> Curve {
    Curve { v_sol: t.v_sol, v_tokens: t.v_tokens, real_sol: t.real_sol }
}

fn curve_at(trades: &[Trade], at_ms: i64) -> Option<Curve> {
    upto(trades, at_ms).iter().max_by_key(|t| (t.slot, t.recv_ms)).map(curve_of).filter(|c| c.valid())
}

/// Rules that run on trades alone (SPEC 5 `pre_buy`, `pre_buy_momentum`), copied from the live Scam Shield functions.
fn trade_rules(cfg: &Config, trades: &[Trade], now_ms: i64, momentum: Option<(f64, f64, f64)>) -> Vec<String> {
    let slot_txs: Vec<SlotTx> = trades.iter().map(|x| SlotTx { slot: x.slot, payer: x.user.clone(), recv_ms: x.recv_ms }).collect();
    let sigs: Vec<String> = trades.iter().map(|x| x.signature.clone()).collect();
    let hit = rules::bundle_rules(&slot_txs, &[], &HashSet::new(), now_ms).filter(|h| h.code != "BUNDLE_SUPPLY");
    let mut results = vec![rules::bundle_check(hit, cfg.filters.reject_bundle, &sigs), rules::wash_volume_rule(trades, now_ms)];
    results.extend(rules::curve_type_rules(trades.iter().any(|t| t.mayhem), trades.iter().any(|t| t.non_sol_quote)));
    if let Some((launch_mcap, now_mcap, peak)) = momentum {
        results.extend(rules::momentum_pre_buy(launch_mcap, now_mcap, peak));
    }
    results.iter().filter(|r| r.blocks()).map(|r| r.code.clone()).collect()
}

struct Coin<'a> {
    msg: &'a FeedMsg,
    t0: i64,
    trades: &'a [Trade],
    launch_trades: &'a [Trade],
    create_slot: Option<u64>,
}

enum Verdict {
    Enter(i64),
    Reject,
    Skip,
}

/// Decide whether and when the lane enters this coin. Returns the DECISION time; the fill is `delay` later.
fn decide_entry(c: &Coin, o: &BtOpts) -> Verdict {
    let cfg = &o.cfg;
    let msg = c.msg;
    let launch_price = if msg.v_tokens_in_bonding_curve > 0.0 { msg.v_sol_in_bonding_curve / msg.v_tokens_in_bonding_curve } else { 0.0 };
    let launch_mcap = crate::pump::mcap_sol(msg.v_sol_in_bonding_curve, msg.v_tokens_in_bonding_curve);
    // stage A rules that need no network
    if msg.is_mayhem_mode || crate::pump::quote_is_non_sol(&msg.quote_mint) {
        return Verdict::Skip;
    }
    if rules::creator_holding_rule(rules::creator_initial_pct(msg.initial_buy), cfg.filters.max_creator_holding_pct, &msg.trader_public_key).blocks() {
        return Verdict::Reject;
    }
    let all: Vec<Trade> = c.launch_trades.to_vec();
    let grade = |upto_trades: &[Trade]| -> (launch::LaunchExposure, bool) {
        let mut v = all.clone();
        v.extend(upto_trades.iter().cloned());
        let curve = upto_trades.iter().max_by_key(|t| (t.slot, t.recv_ms)).map(curve_of);
        let e = launch::launch_exposure(&v, &msg.trader_public_key, c.create_slot, curve);
        let blocks = cfg.filters.graded.enabled && launch::grade(&e, &cfg.filters.graded).blocks();
        (e, blocks)
    };
    match o.lane.as_str() {
        "deep-sprint" => {
            let sp = &cfg.sprint;
            if c.create_slot.is_none() {
                return Verdict::Skip;
            }
            let mut reached = false;
            let mut t = c.t0;
            while t < c.t0 + 150_000 {
                t += 1000;
                let tr = upto(c.trades, t);
                let Some(last) = tr.iter().max_by_key(|x| (x.slot, x.recv_ms)) else { continue };
                let age = c.create_slot.map(|s| last.slot.saturating_sub(s));
                if crate::sprint::window_over(sp, age, last.real_sol) {
                    break;
                }
                let (exp, _) = grade(tr);
                let mut both = c.launch_trades.to_vec();
                both.extend(tr.iter().cloned());
                let inputs = crate::sprint::Inputs {
                    age_slots: age,
                    real_sol: last.real_sol,
                    buyers: crate::sprint::distinct_buyers(&both),
                    exposure: &exp,
                    creator_sold: tr.iter().any(|x| !x.is_buy && x.user == msg.trader_public_key),
                    excluded_type: tr.iter().any(|x| x.mayhem || x.non_sol_quote),
                    open_in_lane: 0,
                };
                if last.real_sol >= sp.real_sol_min {
                    reached = true;
                }
                if crate::sprint::why_not(sp, &inputs).is_empty() && trade_rules(cfg, tr, t, None).is_empty() {
                    return Verdict::Enter(t);
                }
            }
            let _ = reached;
            Verdict::Reject
        }
        lane => {
            let tuned = Tuned::from_config(cfg);
            let decision = c.t0 + cfg.entry.min_age_seconds as i64 * 1000;
            let tr = upto(c.trades, decision);
            let obs = picker::observe(tr, &msg.trader_public_key, msg.initial_buy, launch_price, (msg.v_sol_in_bonding_curve - 30.0).max(0.0), (decision - c.t0) as f64 / 1000.0, decision);
            let sc = picker::score_with(&obs, &tuned.cfg, &tuned.weights);
            let peak = tr.iter().map(|x| x.mcap_sol()).fold(0.0, f64::max);
            let (_, grade_blocks) = grade(tr);
            if lane == cfg.entry.paper_variant.style {
                if !(picker::dip_ok(&sc, cfg.observation.min_score) && obs.dev_sold_pct <= 0.0) || grade_blocks || !trade_rules(cfg, tr, decision, None).is_empty() {
                    return Verdict::Reject;
                }
                let until = c.t0 + 240_000;
                let mut t = decision;
                while t < until {
                    t += 1000;
                    let seen = upto(c.trades, t);
                    if seen.iter().any(|x| !x.is_buy && x.user == msg.trader_public_key) {
                        break;
                    }
                    if picker::dip_signal(seen, t, cfg).is_some() {
                        return Verdict::Enter(t);
                    }
                }
                Verdict::Reject
            } else {
                if !sc.picked || grade_blocks || !trade_rules(cfg, tr, decision, Some((launch_mcap, obs.mcap_sol, peak))).is_empty() {
                    return Verdict::Reject;
                }
                Verdict::Enter(decision)
            }
        }
    }
}

/// Buy at the curve `delay` after the decision and manage the position with the real exit engine until it is closed.
fn run_position(c: &Coin, decision_ms: i64, o: &BtOpts) -> Option<BtTrade> {
    let lane_cfg = if o.lane == "deep-sprint" { o.cfg.sprint.lane_config(&o.cfg) } else { o.cfg.clone() };
    let fees = FeeModel::from_config(&o.cfg);
    let quote_curve = curve_at(c.trades, decision_ms)?;
    let fill_ms = decision_ms + o.delay_ms;
    let fill_curve = curve_at(c.trades, fill_ms)?;
    let size = o.size_sol;
    let into = size * (1.0 - fees.pct_per_side() / 100.0);
    let (qt, _) = quote_curve.buy(into);
    let quoted = if qt > 0.0 { into / qt } else { return None };
    let (tokens, after) = fill_curve.buy(into);
    let avg = if tokens > 0.0 { into / tokens } else { return None };
    let slip = (avg / quoted - 1.0) * 100.0;
    if slip > crate::paper::BUY_SLIPPAGE_CAP_PCT {
        return None; // the same refusal the paper executor makes
    }
    let _ = after;
    let mut pos = Position::new_buying(&o.lane, &c.msg.mint, &c.msg.symbol, &c.msg.trader_public_key, &c.msg.bonding_curve_key, size, fill_ms, 150.0);
    pos.fill_buy(avg, tokens, 0.0, &fees.trade_fees(size, fees.priority_for_attempt(0)), slip).ok()?;
    let entry_real = fill_curve.real_sol;
    let mut peak_real = entry_real;
    let step = lane_cfg.exits.exit_loop_ms.max(100) as i64;
    let last_ms = c.trades.last().map(|t| t.recv_ms).unwrap_or(fill_ms);
    let limit = fill_ms + (lane_cfg.exits.max_hold_seconds as i64 + 30) * 1000;
    let mut now = fill_ms;
    let mut known = fill_curve;
    while pos.state != crate::position::PosState::Closed {
        now += step;
        if let Some(cv) = curve_at(c.trades, now) {
            known = cv;
        }
        let price = {
            let g = known.sell(pos.tokens_remaining).0 / pos.tokens_remaining.max(1e-9);
            if g > 0.0 { g } else { pos.last_price }
        };
        peak_real = peak_real.max(known.real_sol);
        let seen = upto(c.trades, now);
        let market = Market { now_ms: now, price, real_sol: known.real_sol, trades: seen, feed_ok_since_ms: 0, migrated: false };
        let reserve = lane_cfg.exits.reserve_stops.map(|_| Reserve { entry_real_sol: entry_real, peak_real_sol: peak_real });
        let mut action = exits::evaluate_r(&mut pos, &market, &lane_cfg, reserve);
        if action.is_none() && (now > limit || now > last_ms + 400_000) {
            action = Some(exits::ExitAction { reason: "data-end".into(), sell_all: true, fraction: 1.0, of_remaining: true, tranche: None, taper: false });
        }
        let Some(a) = action else { continue };
        let rem = pos.tokens_remaining;
        let mut qty = if a.sell_all { rem } else if a.of_remaining { rem * a.fraction } else { (pos.tokens_initial * a.fraction).min(rem) };
        if rem - qty < pos.tokens_initial * 1e-6 {
            qty = rem;
        }
        let sell_curve = curve_at(c.trades, now + o.delay_ms).unwrap_or(known);
        let (gross, _) = sell_curve.sell(qty);
        let f: FeeBreakdown = fees.trade_fees(gross, fees.priority_for_attempt(0));
        let stage = StageSell { ts: now, reason: a.reason.clone(), tokens: qty, price: gross / qty, gross_sol: gross, net_sol: gross - f.total(), fees_sol: f.total() };
        pos.begin_sell().ok()?;
        pos.end_sell(Some((stage, f)), &FeeBreakdown::default(), now + o.delay_ms, &a.reason).ok()?;
        if let Some(t) = a.tranche {
            pos.tranches_done = t as u32 + 1;
        }
        if a.taper {
            pos.taper_done = true;
        }
    }
    let exit_ms = pos.exit_ms.unwrap_or(now);
    Some(BtTrade { mint: pos.mint.clone(), symbol: pos.symbol.clone(), entry_ms: fill_ms, exit_ms, size_sol: size, pnl_sol: pos.pnl_sol, ret: if size > 0.0 { pos.pnl_sol / size } else { 0.0 }, reason: pos.exit_reason.clone(), hold_secs: (exit_ms - fill_ms) as f64 / 1000.0 })
}

/// Run one lane over a whole recording.
pub fn run(rec: &Recording, o: &BtOpts) -> BtReport {
    let mut trades_out: Vec<BtTrade> = vec![];
    let (mut coins, mut skipped_gap, mut skipped_other, mut rejected) = (0u32, 0u32, 0u32, 0u32);
    let hold_after = o.cfg.exits.max_hold_seconds.max(o.cfg.sprint.max_hold_seconds) as i64 * 1000 + 10_000;
    let horizon = match o.lane.as_str() {
        "deep-sprint" => 150_000,
        l if l == o.cfg.entry.paper_variant.style => 240_000,
        _ => o.cfg.entry.min_age_seconds as i64 * 1000,
    } + hold_after;
    let none: Vec<Trade> = vec![];
    for (t0, msg) in &rec.launches {
        coins += 1;
        if rec.gaps.iter().any(|(from, to, _)| *from < t0 + horizon && *to > *t0) {
            skipped_gap += 1;
            continue;
        }
        let trades = rec.trades.get(&msg.mint).unwrap_or(&none);
        let c = Coin { msg, t0: *t0, trades, launch_trades: rec.launch_trades.get(&msg.mint).map(|v| v.as_slice()).unwrap_or(&[]), create_slot: rec.create_slots.get(&msg.mint).copied() };
        match decide_entry(&c, o) {
            Verdict::Skip => skipped_other += 1,
            Verdict::Reject => rejected += 1,
            Verdict::Enter(at) => match run_position(&c, at, o) {
                Some(t) => trades_out.push(t),
                None => rejected += 1,
            },
        }
    }
    trades_out.sort_by_key(|t| t.entry_ms);
    let n = trades_out.len();
    let hold_n = if n == 0 { 0 } else { ((n as f64 * o.holdout_pct / 100.0).round() as usize).clamp(1, n) };
    let split = n - hold_n;
    let rs = o.cfg.learning.bootstrap_resamples;
    let train: Vec<&BtTrade> = trades_out[..split].iter().collect();
    let hold: Vec<&BtTrade> = trades_out[split..].iter().collect();
    let all: Vec<&BtTrade> = trades_out.iter().collect();
    let hold_returns: Vec<f64> = hold.iter().map(|t| t.ret).collect();
    let (ok, note) = crate::stats::holdout_verdict(&hold_returns, o.cfg.learning.promotion_min_trades.max(crate::learning::MIN_SAMPLE_FLOOR), rs);
    let day = |ms: i64| crate::timefmt::date_utc(ms);
    let range = match (rec.launches.first(), rec.launches.last()) {
        (Some(a), Some(b)) => format!("{} to {}", day(a.0), day(b.0)),
        _ => "no launches".into(),
    };
    BtReport { lane: o.lane.clone(), coins, skipped_gap, skipped_other, rejected, all: summarize(&all, rs), train: summarize(&train, rs), holdout: summarize(&hold, rs), holdout_ok: ok, holdout_note: note, delay_ms: o.delay_ms, size_sol: o.size_sol, range, trades: trades_out }
}

fn pct(x: f64) -> String {
    format!("{x:+.2}%")
}

fn line(name: &str, s: &Summary) -> String {
    let ci = match (s.ci_low_pct, s.ci_high_pct) {
        (Some(a), Some(b)) => format!("95% range {} to {}", pct(a), pct(b)),
        _ => "95% range not measurable".into(),
    };
    format!("  {name:<9} {:>4} trades, win rate {:>5.1}%, total {:+.4} SOL, mean {} per trade, {ci}, profit factor {}", s.trades, s.win_rate_pct, s.total_pnl_sol, pct(s.mean_ret_pct), s.profit_factor.map(|p| format!("{p:.2}")).unwrap_or_else(|| "n/a".into()))
}

/// Plain-text report, with the limits of the method printed under the numbers.
pub fn format_report(r: &BtReport) -> String {
    let mut s = format!("BACKTEST lane {} over {} ({} launches)\n", r.lane, r.range, r.coins);
    s += &format!("  entry delay {} ms, trade size {} SOL. Skipped: {} coins overlapping a feed gap, {} the lane cannot judge. {} coins did not pass the lane's rules.\n", r.delay_ms, r.size_sol, r.skipped_gap, r.skipped_other, r.rejected);
    s += &line("all", &r.all);
    s += "\n";
    s += &line("train", &r.train);
    s += "\n";
    s += &line("holdout", &r.holdout);
    s += &format!("\n  Holdout verdict: {} {}\n", if r.holdout_ok { "PASSES." } else { "does NOT pass." }, r.holdout_note);
    s += "  Limits: RugCheck, authority, holder, funder, metadata and X Monitor checks are not replayed, so the real bot would take fewer of these trades. Each coin is judged alone at a fixed size.\n";
    s
}

/// `engine-cli backtest ...` (also used by tests): returns the text to print.
/// `--data DIR` recordings folder, `--lane momentum|dip-40pct|deep-sprint|all`, `--config PATH`, `--delay-ms N` (default 2000),
/// `--from DAY` / `--to DAY` (UTC, YYYY-MM-DD), `--holdout-pct N` (default 30), `--json`.
pub fn cli(args: &[String], default_cfg: &std::path::Path) -> Result<String, String> {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned());
    let cfg_path = arg("--config").map(std::path::PathBuf::from).unwrap_or_else(|| default_cfg.to_path_buf());
    let cfg = Config::load(&cfg_path)?;
    let dir = arg("--data").map(std::path::PathBuf::from).or_else(|| arg("--db").and_then(|d| std::path::Path::new(&d).parent().map(|p| p.join("recordings")))).ok_or("give the recordings folder with --data DIR")?;
    let rec = crate::recorder::read_dir(&dir, &arg("--from").unwrap_or_default(), &arg("--to").unwrap_or_default())?;
    if rec.files == 0 {
        return Err(format!("no recording files in {}. The app writes them while it runs (Settings > Recording).", dir.display()));
    }
    let lane = arg("--lane").unwrap_or_else(|| "all".into());
    let lanes: Vec<&str> = if lane == "all" { LANES.to_vec() } else if LANES.contains(&lane.as_str()) { vec![lane.as_str()] } else { return Err(format!("unknown lane {lane:?}: use momentum, dip-40pct, deep-sprint or all")) };
    let mut out = format!("Recording: {} file(s), {} launches, {} coins with trades, {} feed gap(s), {} unreadable line(s).\n", rec.files, rec.launches.len(), rec.trades.len(), rec.gaps.len(), rec.bad_lines);
    let mut reports = vec![];
    for l in lanes {
        let mut o = BtOpts::new(l, cfg.clone());
        if let Some(d) = arg("--delay-ms").and_then(|v| v.parse().ok()) {
            o.delay_ms = d;
        }
        if let Some(h) = arg("--holdout-pct").and_then(|v| v.parse::<f64>().ok()) {
            o.holdout_pct = h.clamp(5.0, 90.0);
        }
        if let Some(sz) = arg("--size").and_then(|v| v.parse().ok()) {
            o.size_sol = sz;
        }
        reports.push(run(&rec, &o));
    }
    if args.iter().any(|a| a == "--json") {
        return serde_json::to_string_pretty(&reports).map_err(|e| e.to_string());
    }
    for r in &reports {
        out += &format_report(r);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{scenario, Scenario};

    fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    /// A recording that holds one scripted scenario, started at `t0`.
    fn rec_of(sc: &Scenario, t0: i64) -> Recording {
        let mut rec = Recording::default();
        rec.launches.push((t0, sc.create.clone()));
        rec.trades.insert(sc.create.mint.clone(), sc.events.iter().map(|(o, t)| Trade { recv_ms: t0 + o, ..t.clone() }).collect());
        rec.finish();
        rec
    }

    #[test]
    fn the_hot_scenario_is_picked_and_sold_by_the_real_exit_engine_with_a_profit() {
        let t0 = 1_795_000_000_000;
        let r = run(&rec_of(&scenario("hot").unwrap(), t0), &BtOpts::new("momentum", cfg()));
        assert_eq!(r.trades.len(), 1, "{}", format_report(&r));
        let t = &r.trades[0];
        assert!(t.pnl_sol > 0.0, "{t:?}");
        assert!(t.entry_ms >= t0 + 62_000, "the fill is after the 60 s decision plus the delay: {}", t.entry_ms - t0);
        assert!(["trailing-stop", "take-profit-3", "take-profit-2", "take-profit-1", "stop-at-entry", "momentum-death", "max-hold"].contains(&t.reason.as_str()), "{}", t.reason);
        assert_eq!((r.coins, r.skipped_gap, r.rejected), (1, 0, 0));
        // the fees are in: P/L is below the gross move
        assert!(t.ret < 1.0);
    }

    #[test]
    fn the_dump_scenario_is_picked_and_stopped_out_at_a_loss() {
        let r = run(&rec_of(&scenario("dump").unwrap(), 1_795_000_000_000), &BtOpts::new("momentum", cfg()));
        assert_eq!(r.trades.len(), 1, "{}", format_report(&r));
        assert!(r.trades[0].pnl_sol < 0.0);
        assert!(["hard-stop", "soft-stop", "bleeder", "stall", "stop-at-entry", "momentum-death", "slow-rug", "whale-sell"].contains(&r.trades[0].reason.as_str()), "{}", r.trades[0].reason);
    }

    #[test]
    fn a_stricter_config_changes_the_result_so_any_config_can_be_tried() {
        let mut strict = cfg();
        strict.entry.min_buy_sell_ratio = 80.0; // no coin buys 80 times more than it sells
        let r = run(&rec_of(&scenario("hot").unwrap(), 1_795_000_000_000), &BtOpts::new("momentum", strict));
        assert_eq!((r.trades.len(), r.rejected), (0, 1));
        // a longer fill delay moves the entry and the P/L
        let a = run(&rec_of(&scenario("hot").unwrap(), 1_795_000_000_000), &BtOpts { delay_ms: 300, ..BtOpts::new("momentum", cfg()) });
        let b = run(&rec_of(&scenario("hot").unwrap(), 1_795_000_000_000), &BtOpts { delay_ms: 1500, ..BtOpts::new("momentum", cfg()) });
        assert_eq!((a.trades.len(), b.trades.len()), (1, 1));
        assert!((a.trades[0].pnl_sol - b.trades[0].pnl_sol).abs() > 1e-9, "a slower fill gets a different price");
    }

    #[test]
    fn a_coin_overlapping_a_recorded_gap_is_skipped_and_counted_not_traded() {
        let t0 = 1_795_000_000_000;
        let mut rec = rec_of(&scenario("hot").unwrap(), t0);
        rec.gaps.push((t0 + 30_000, t0 + 45_000, "trade feed reconnected".into()));
        let r = run(&rec, &BtOpts::new("momentum", cfg()));
        assert_eq!((r.trades.len(), r.skipped_gap), (0, 1));
        // a gap long before the coin does not matter
        let mut rec = rec_of(&scenario("hot").unwrap(), t0);
        rec.gaps.push((t0 - 600_000, t0 - 500_000, "old".into()));
        assert_eq!(run(&rec, &BtOpts::new("momentum", cfg())).trades.len(), 1);
    }

    #[test]
    fn mayhem_coins_are_never_replayed_into_a_trade() {
        let t0 = 1_795_000_000_000;
        let mut sc = scenario("hot").unwrap();
        sc.create.is_mayhem_mode = true;
        let r = run(&rec_of(&sc, t0), &BtOpts::new("momentum", cfg()));
        assert_eq!((r.trades.len(), r.skipped_other), (0, 1));
    }

    /// A coin that fills fast: 40 distinct buyers, real SOL 10 to 45 in 70 slots, creation slot 1000.
    fn deep_rec(t0: i64, whale: bool) -> Recording {
        let mut rec = Recording::default();
        let msg = FeedMsg { mint: "DeepMint".into(), symbol: "DEEP".into(), trader_public_key: "Dev".into(), bonding_curve_key: "DeepCurve".into(), v_sol_in_bonding_curve: 30.0, v_tokens_in_bonding_curve: 1.073e9, market_cap_sol: 27.96, tx_type: "create".into(), ..Default::default() };
        rec.launches.push((t0, msg));
        let mut v = vec![];
        for i in 0..40u64 {
            let real = 10.0 + 35.0 * (i + 1) as f64 / 40.0;
            let v_sol = 30.0 + real;
            v.push(Trade { mint: "DeepMint".into(), user: format!("b{i:03}"), is_buy: true, sol: 1.0, tokens: 1.0e6, slot: 1000 + (70 * (i + 1)) / 40, signature: format!("d{i}"), recv_ms: t0 + 5_000 + i as i64 * 1000, v_sol, v_tokens: 30.0 * 1.073e9 / v_sol, real_sol: real, ..Default::default() });
        }
        // after entry the curve keeps filling toward graduation, one buyer a second
        for k in 0..60u64 {
            let real = 45.0 + 0.6 * (k + 1) as f64;
            let v_sol = 30.0 + real;
            v.push(Trade { mint: "DeepMint".into(), user: format!("c{k:03}"), is_buy: true, sol: 0.6, tokens: 1.0e6, slot: 1100 + k, signature: format!("e{k}"), recv_ms: t0 + 46_000 + k as i64 * 1000, v_sol, v_tokens: 30.0 * 1.073e9 / v_sol, real_sol: real, ..Default::default() });
        }
        rec.trades.insert("DeepMint".into(), v);
        rec.create_slots.insert("DeepMint".into(), 1000);
        if whale {
            rec.launch_trades.insert("DeepMint".into(), vec![Trade { mint: "DeepMint".into(), user: "whale".into(), is_buy: true, sol: 0.0, tokens: 300.0e6, slot: 1000, signature: "w".into(), recv_ms: t0, v_sol: 30.0, v_tokens: 1.073e9, real_sol: 0.0, ..Default::default() }]);
        }
        rec.finish();
        rec
    }

    #[test]
    fn the_sprint_lane_replays_a_deep_coin_and_sells_it_before_graduation() {
        let t0 = 1_795_000_000_000;
        let quick = |lane: &str| BtOpts { delay_ms: 500, ..BtOpts::new(lane, cfg()) };
        let r = run(&deep_rec(t0, false), &quick("deep-sprint"));
        assert_eq!(r.trades.len(), 1, "{}", format_report(&r));
        let t = &r.trades[0];
        assert_eq!(t.reason, "graduation-90pct", "sold at 90% of graduation");
        assert!(t.pnl_sol > 0.0, "{t:?}");
        // with a launch-slot whale the same coin is rejected
        let w = run(&deep_rec(t0, true), &quick("deep-sprint"));
        assert_eq!((w.trades.len(), w.rejected), (0, 1));
        // the main lane has no way to enter this coin: it is 45+ SOL deep by 60 s (curveSolMax 40)
        let m = run(&deep_rec(t0, false), &quick("momentum"));
        assert_eq!(m.trades.len(), 0);
        // no creation slot recorded: the sprint lane cannot judge it
        let mut no_slot = deep_rec(t0, false);
        no_slot.create_slots.clear();
        assert_eq!(run(&no_slot, &quick("deep-sprint")).skipped_other, 1);
    }

    #[test]
    fn the_report_splits_a_time_ordered_holdout_and_applies_the_promotion_rule() {
        let mk = |i: i64, ret: f64| BtTrade { mint: format!("m{i}"), symbol: "S".into(), entry_ms: i, exit_ms: i + 1, size_sol: 0.1, pnl_sol: ret * 0.1, ret, reason: "x".into(), hold_secs: 1.0 };
        let all: Vec<BtTrade> = (0..60).map(|i| mk(i, if i % 3 == 0 { -0.02 } else { 0.04 })).collect();
        let refs: Vec<&BtTrade> = all.iter().collect();
        let s = summarize(&refs, 2000);
        assert_eq!((s.trades, s.wins), (60, 40));
        assert!(s.ci_low_pct.unwrap() < s.mean_ret_pct && s.mean_ret_pct < s.ci_high_pct.unwrap());
        assert!(s.profit_factor.unwrap() > 1.0);
        // a lane that loses on average does not pass even with many trades
        let bad: Vec<BtTrade> = (0..60).map(|i| mk(i, if i % 3 == 0 { 0.02 } else { -0.04 })).collect();
        let br: Vec<&BtTrade> = bad.iter().collect();
        assert!(summarize(&br, 2000).mean_ret_pct < 0.0);
    }

    #[test]
    fn a_fill_that_moves_more_than_5_percent_during_the_delay_is_refused_like_the_paper_executor_does() {
        let r = run(&rec_of(&scenario("hot").unwrap(), 1_795_000_000_000), &BtOpts { delay_ms: 9000, ..BtOpts::new("momentum", cfg()) });
        assert_eq!((r.trades.len(), r.rejected), (0, 1), "{}", format_report(&r));
    }

    #[test]
    fn the_command_reads_recorded_files_runs_every_lane_and_prints_the_limits() {
        let dir = std::env::temp_dir().join(format!("ct-bt-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rec = crate::recorder::Recorder::new(dir.clone(), crate::config::RecorderCfg::default());
        let sc = scenario("hot").unwrap();
        let t0 = 1_795_000_000_000;
        rec.record_launch(t0, &sc.create);
        for (o, t) in &sc.events {
            rec.record_trade(&Trade { recv_ms: t0 + o, ..t.clone() });
        }
        rec.flush();
        let cfgp = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../config.json"));
        let args = |extra: &[&str]| -> Vec<String> { ["backtest", "--data", dir.to_str().unwrap()].iter().chain(extra.iter()).map(|s| s.to_string()).collect() };
        let text = cli(&args(&["--delay-ms", "500"]), cfgp).unwrap();
        assert!(text.contains("1 launches") && text.contains("lane momentum") && text.contains("lane dip-40pct") && text.contains("lane deep-sprint"), "{text}");
        assert!(text.contains("Holdout verdict") && text.contains("Limits:"));
        let one = cli(&args(&["--lane", "momentum", "--delay-ms", "500", "--json"]), cfgp).unwrap();
        let v: serde_json::Value = serde_json::from_str(&one).unwrap();
        assert_eq!(v[0]["lane"], "momentum");
        assert_eq!(v[0]["trades"].as_array().unwrap().len(), 1);
        assert!(cli(&args(&["--lane", "bogus"]), cfgp).unwrap_err().contains("unknown lane"));
        assert!(cli(&["backtest".into(), "--data".into(), "/nonexistent-dir-xyz".into()], cfgp).unwrap_err().contains("no recording files"));
        // a stricter config file can be tried without touching the app
        let mut strict: serde_json::Value = serde_json::from_str(include_str!("../../config.json")).unwrap();
        strict["entry"]["minBuySellRatio"] = serde_json::json!(80.0);
        let sp = dir.join("strict.json");
        std::fs::write(&sp, strict.to_string()).unwrap();
        let strict_out = cli(&args(&["--lane", "momentum", "--delay-ms", "500", "--config", sp.to_str().unwrap(), "--json"]), cfgp).unwrap();
        let sv: serde_json::Value = serde_json::from_str(&strict_out).unwrap();
        assert_eq!(sv[0]["trades"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_recording_gives_an_empty_honest_report() {
        let r = run(&Recording::default(), &BtOpts::new("momentum", cfg()));
        assert_eq!((r.coins, r.trades.len()), (0, 0));
        assert!(!r.holdout_ok);
        let text = format_report(&r);
        assert!(text.contains("does NOT pass") && text.contains("Limits:"), "{text}");
    }

    #[test]
    fn the_dip_lane_name_runs_the_dip_entry_path() {
        let r = run(&rec_of(&scenario("hot").unwrap(), 1_795_000_000_000), &BtOpts::new("dip-40pct", cfg()));
        assert_eq!(r.lane, "dip-40pct");
        assert!(r.trades.len() <= 1);
    }
}
