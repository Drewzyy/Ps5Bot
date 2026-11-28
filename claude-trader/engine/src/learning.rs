//! Learning journal ("What it learned").
//!
//! RULE ZERO: nothing here is made up. Every sentence is built from numbers that were recorded for real trades
//! (positions, entry features, fills). With no data the journal says so. There is no free text from a model.
//!
//! 1. After each closed trade a journal entry is written: P/L net of fees, hold time, exit reason, entry features
//!    and factual observations computed from the data (see `observations`).
//! 2. Every N closed trades (config `learning.reviewEveryTrades`, 30) a review groups trades by feature buckets and
//!    exit reasons. A pattern is reported only when the bucket AND the rest each have at least `minSample` (10)
//!    trades and the win-rate gap is at least `minWinRateGapPts`. The review shows the counts and proposes a change.
//! 3. A proposed change runs in a shadow paper lane for the next `shadowTrades` (30) main-lane trades, side by side. The shadow
//!    trades are the HOLDOUT: the change was found on earlier trades and has never seen these. It is applied only if, on that
//!    holdout, there are at least `promotionMinTrades` (20) trades, the mean return per trade is above 0 AND the lower end of its
//!    95% bootstrap range is above 0 (a result that could be luck is not promoted), and the mean is not worse than the main
//!    lane's. In live mode it becomes a proposal that needs the user's approval.
//! 4. Hard limits (not settings): only the whitelisted entry/exit thresholds and score weights can change, never by
//!    more than 20% from the shipped value. Scam Shield, trade size, fees and loss limits are never touched.
//! 5. Every applied change is logged with before/after, evidence and results, and can be undone.
//!
//! Nothing in this file signs or sends anything.

use crate::config::{Config, Mode};
use crate::db::Db;
use crate::events::EngineEvent;
use crate::hub::Hub;
use crate::position::Position;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_CHANGE_PCT: f64 = 20.0;
pub const MIN_SAMPLE_FLOOR: u32 = 10;
/// A coin scored at least this and still not bought counts as "missed" in the efficiency notes.
pub const MISSED_MIN_SCORE: f64 = 80.0;
pub const MISSED_WINDOW_MS: i64 = 300_000;

// ============================================================================ tunable parameters

/// Config values plus the five score weights: everything the learning loop is allowed to touch.
#[derive(Debug, Clone)]
pub struct Tuned {
    pub cfg: Config,
    pub weights: [f64; 5],
}

impl Tuned {
    pub fn from_config(cfg: &Config) -> Tuned {
        Tuned { cfg: cfg.clone(), weights: crate::picker::DEFAULT_WEIGHTS }
    }
}

pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    pub integer: bool,
    pub get: fn(&Tuned) -> f64,
    pub set: fn(&mut Tuned, f64),
}

/// The whole whitelist. Nothing under `filters.` (Scam Shield), `risk.` (size, loss limits), `fees.` or `paper.`
/// is here, so nothing there can ever be changed by the learning loop.
pub const PARAMS: &[Param] = &[
    Param { key: "observation.minScore", label: "Minimum score to buy", integer: false, get: |t| t.cfg.observation.min_score, set: |t, v| t.cfg.observation.min_score = v },
    Param { key: "observation.minUniqueBuyers", label: "Unique buyers needed while watching", integer: true, get: |t| t.cfg.observation.min_unique_buyers as f64, set: |t, v| t.cfg.observation.min_unique_buyers = v as u32 },
    Param { key: "observation.minBuySellRatio", label: "Buy/sell ratio for a full score", integer: false, get: |t| t.cfg.observation.min_buy_sell_ratio, set: |t, v| t.cfg.observation.min_buy_sell_ratio = v },
    Param { key: "observation.minPriceChangePct", label: "Price rise for a full score (%)", integer: false, get: |t| t.cfg.observation.min_price_change_pct, set: |t, v| t.cfg.observation.min_price_change_pct = v },
    Param { key: "entry.minBuySellRatio", label: "Buy/sell ratio needed to buy", integer: false, get: |t| t.cfg.entry.min_buy_sell_ratio, set: |t, v| t.cfg.entry.min_buy_sell_ratio = v },
    Param { key: "entry.minUniqueBuyers30s", label: "Unique buyers needed in 30 s", integer: true, get: |t| t.cfg.entry.min_unique_buyers30s as f64, set: |t, v| t.cfg.entry.min_unique_buyers30s = v as u32 },
    Param { key: "entry.curveSolMin", label: "Lowest SOL in the curve to buy", integer: false, get: |t| t.cfg.entry.curve_sol_min, set: |t, v| t.cfg.entry.curve_sol_min = v },
    Param { key: "entry.curveSolMax", label: "Highest SOL in the curve to buy", integer: false, get: |t| t.cfg.entry.curve_sol_max, set: |t, v| t.cfg.entry.curve_sol_max = v },
    Param { key: "entry.minHolders", label: "Wallets needed for a full score", integer: true, get: |t| t.cfg.entry.min_holders as f64, set: |t, v| t.cfg.entry.min_holders = v as u32 },
    Param { key: "exits.trailingStopPct", label: "Trailing stop (%)", integer: false, get: |t| t.cfg.exits.trailing_stop_pct, set: |t, v| t.cfg.exits.trailing_stop_pct = v },
    Param { key: "exits.softStopPct", label: "Soft stop (%)", integer: false, get: |t| t.cfg.exits.soft_stop_pct, set: |t, v| t.cfg.exits.soft_stop_pct = v },
    Param { key: "exits.hardStopPct", label: "Hard stop (%)", integer: false, get: |t| t.cfg.exits.hard_stop_pct, set: |t, v| t.cfg.exits.hard_stop_pct = v },
    Param { key: "weights.ratio", label: "Score weight: buy/sell ratio", integer: false, get: |t| t.weights[0], set: |t, v| t.weights[0] = v },
    Param { key: "weights.buyers", label: "Score weight: unique buyers", integer: false, get: |t| t.weights[1], set: |t, v| t.weights[1] = v },
    Param { key: "weights.priceChange", label: "Score weight: price change", integer: false, get: |t| t.weights[2], set: |t, v| t.weights[2] = v },
    Param { key: "weights.slope", label: "Score weight: volume slope", integer: false, get: |t| t.weights[3], set: |t, v| t.weights[3] = v },
    Param { key: "weights.wallets", label: "Score weight: wallets", integer: false, get: |t| t.weights[4], set: |t, v| t.weights[4] = v },
];

pub fn param(key: &str) -> Option<&'static Param> {
    PARAMS.iter().find(|p| p.key == key)
}

/// Allowed range of a parameter: within 20% of the shipped (baseline) value, in either direction.
pub fn bounds(baseline: &Tuned, key: &str) -> Option<(f64, f64)> {
    let p = param(key)?;
    let b = (p.get)(baseline);
    let (x, y) = (b * (1.0 - MAX_CHANGE_PCT / 100.0), b * (1.0 + MAX_CHANGE_PCT / 100.0));
    let (lo, hi) = (x.min(y), x.max(y));
    // integers: only whole numbers inside the range count
    Some(if p.integer { (lo.ceil(), hi.floor()) } else { (lo, hi) })
}

/// Pull a wanted value inside the allowed range (and round whole-number settings).
pub fn clamp_to_bounds(baseline: &Tuned, key: &str, wanted: f64) -> Option<f64> {
    let p = param(key)?;
    let (lo, hi) = bounds(baseline, key)?;
    let v = wanted.clamp(lo, hi);
    Some(if p.integer { v.round() } else { (v * 1000.0).round() / 1000.0 })
}

/// The gatekeeper. Refuses anything that is not whitelisted, outside +-20% of the baseline, or that would make
/// the config invalid. Returns the value to store.
pub fn check_proposal(baseline: &Tuned, current: &Tuned, key: &str, new: f64) -> Result<f64, String> {
    let Some(p) = param(key) else {
        return Err(format!("{key} is not something the learning loop may change (Scam Shield, trade size, fees and loss limits are off limits)."));
    };
    if !new.is_finite() {
        return Err("The new value is not a number.".into());
    }
    let (lo, hi) = bounds(baseline, key).ok_or("unknown setting")?;
    let v = if p.integer { new.round() } else { (new * 1000.0).round() / 1000.0 };
    if v < lo - 1e-9 || v > hi + 1e-9 {
        return Err(format!("{v} is outside the allowed range {lo:.3} to {hi:.3} (20% from the shipped value)."));
    }
    let mut t = current.clone();
    (p.set)(&mut t, v);
    t.cfg.validate()?;
    if t.weights.iter().any(|w| *w <= 0.0) {
        return Err("A score weight cannot reach zero.".into());
    }
    Ok(v)
}

/// Baseline plus every applied change, in the order they were applied. Changes that no longer pass the checks are skipped.
pub fn tuned_from(baseline: &Tuned, applied: &[(String, f64)]) -> Tuned {
    let mut t = baseline.clone();
    for (k, v) in applied {
        if let Ok(v) = check_proposal(baseline, &t, k, *v) {
            if let Some(p) = param(k) {
                (p.set)(&mut t, v);
            }
        }
    }
    t
}

// ============================================================================ records

/// What the picker saw when it picked the coin. Stored with the position.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntryFeatures {
    pub score: f64,
    pub buyers: u32,
    pub buys: u32,
    pub sells: u32,
    pub ratio: f64,
    pub curve_sol: f64,
    pub age_seconds: f64,
    pub mcap_sol: f64,
    /// When the pick was made (ms). The fill time minus this is the pick-to-fill delay.
    pub picked_ms: i64,
    /// Where to read about the coin (its website, X or Telegram from the metadata). Empty if unknown.
    pub source_url: String,
    /// Every part of the X Monitor score for this coin (None before X Monitor existed).
    #[serde(default)]
    pub x_community: Option<crate::xmon::XLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JournalRow {
    pub id: i64,
    pub ts: i64,
    pub position_id: i64,
    pub lane: String,
    pub trade_no: u32,
    pub symbol: String,
    pub mint: String,
    pub pnl_sol: f64,
    pub pnl_pct: f64,
    pub fees_sol: f64,
    pub gross_sol: f64,
    pub hold_seconds: f64,
    pub exit_reason: String,
    pub score: f64,
    pub buyers: u32,
    pub ratio: f64,
    pub curve_sol: f64,
    pub age_seconds: f64,
    pub peak_pct: f64,
    pub exit_pct: f64,
    pub fill_seconds: f64,
    pub observations: Vec<String>,
    pub source_url: String,
    pub headline: String,
}

pub fn fmt_sol(x: f64) -> String {
    if x.abs() < 0.0005 { format!("{x:+.5}") } else { format!("{x:+.3}") }
}

fn human_reason(r: &str) -> String {
    match r {
        "trailing-stop" => "the trailing stop".into(),
        "hard-stop" => "the hard stop".into(),
        "soft-stop" => "the soft stop".into(),
        other => other.replace('-', " "),
    }
}

/// Bucket tables for the review. (label, low inclusive, high exclusive)
pub const CURVE_BUCKETS: &[(&str, f64, f64)] = &[("under 5 SOL", 0.0, 5.0), ("5 to 10 SOL", 5.0, 10.0), ("10 to 20 SOL", 10.0, 20.0), ("20 SOL and up", 20.0, f64::MAX)];
pub const RATIO_BUCKETS: &[(&str, f64, f64)] = &[("under 3", 0.0, 3.0), ("3 to 5", 3.0, 5.0), ("5 and up", 5.0, f64::MAX)];
pub const BUYER_BUCKETS: &[(&str, f64, f64)] = &[("under 8", 0.0, 8.0), ("8 to 11", 8.0, 12.0), ("12 and up", 12.0, f64::MAX)];
pub const SCORE_BUCKETS: &[(&str, f64, f64)] = &[("under 85", 0.0, 85.0), ("85 to 89", 85.0, 90.0), ("90 and up", 90.0, f64::MAX)];

fn bucket_of(table: &[(&'static str, f64, f64)], v: f64) -> Option<usize> {
    table.iter().position(|(_, lo, hi)| v >= *lo && v < *hi)
}

/// Factual observations about one closed trade. Every sentence carries the numbers it comes from.
/// `history` is this lane's earlier journal rows (not including this trade).
pub fn observations(pos: &Position, f: &EntryFeatures, history: &[JournalRow]) -> Vec<String> {
    let mut out = vec![];
    // 1. the exit path and what the peak was
    let peak_pct = pos.pct(pos.peak_price);
    let last_pct = pos.stages.last().map(|s| pos.pct(s.price)).unwrap_or(0.0);
    if !pos.stages.is_empty() {
        let names: Vec<String> = {
            let mut seen: Vec<String> = vec![];
            for s in &pos.stages {
                let n = human_reason(&s.reason);
                if seen.last() != Some(&n) {
                    seen.push(n);
                }
            }
            seen
        };
        let path = match names.len() {
            1 => format!("Exit fired at {}", names[0]),
            _ => format!("Exit fired at {} then {}", names[..names.len() - 1].join(", "), names[names.len() - 1]),
        };
        let mut s = format!("{path}; the peak was {peak_pct:+.0}% and the last sell was at {last_pct:+.0}%");
        if pos.exit_reason == "trailing-stop" {
            s.push_str(&format!(", so the trailing stop gave back {:.0} points", (peak_pct - last_pct).max(0.0)));
        }
        s.push('.');
        out.push(s);
    }
    // 2. fees against the price move
    let fees = pos.fees.total();
    let gross = pos.pnl_sol + fees;
    if gross > 0.0 && pos.pnl_sol != 0.0 {
        out.push(format!("Fees were {:.0}% of the gross profit ({:.4} of {:.4} SOL).", fees / gross * 100.0, fees, gross));
    } else if gross <= 0.0 {
        out.push(format!("The price move alone made {} SOL; fees of {:.4} SOL took the result to {} SOL.", fmt_sol(gross), fees, fmt_sol(pos.pnl_sol)));
    } else {
        out.push(format!("Fees were {:.4} SOL.", fees));
    }
    // 3. this lane's record in the same curve range
    if let Some(b) = bucket_of(CURVE_BUCKETS, f.curve_sol) {
        let (mut w, mut l) = (if pos.pnl_sol > 0.0 { 1 } else { 0 }, if pos.pnl_sol > 0.0 { 0 } else { 1 });
        let mut prior = 0;
        for h in history.iter().filter(|h| h.lane == pos.lane && bucket_of(CURVE_BUCKETS, h.curve_sol) == Some(b)) {
            prior += 1;
            if h.pnl_sol > 0.0 { w += 1 } else { l += 1 }
        }
        if prior > 0 {
            out.push(format!("Entered at curve {:.1} SOL; this lane's trades {} are {w}W/{l}L so far (counting this one).", f.curve_sol, CURVE_BUCKETS[b].0));
        } else {
            out.push(format!("Entered at curve {:.1} SOL; this is this lane's first trade {}.", f.curve_sol, CURVE_BUCKETS[b].0));
        }
    }
    // 4. speed of the fill
    if f.picked_ms > 0 && pos.entry_ms >= f.picked_ms {
        let secs = (pos.entry_ms - f.picked_ms) as f64 / 1000.0;
        out.push(format!("The buy filled {secs:.1} s after the pick, {:.2}% above the quote.", pos.slippage_pct));
    }
    // 5. hold time and failed transactions
    let hold = (pos.exit_ms.unwrap_or(pos.entry_ms) - pos.entry_ms) as f64 / 1000.0;
    out.push(format!("Held for {hold:.0} s."));
    if pos.failed_sol > 0.0 {
        out.push(format!("Failed transactions cost {:.5} SOL on this trade.", pos.failed_sol));
    }
    // 6. what the X Monitor saw, every part, so the weights can be tuned once there are enough trades
    if let Some(x) = &f.x_community {
        out.push(x.summary());
    }
    out
}

pub fn journal_row(pos: &Position, f: &EntryFeatures, history: &[JournalRow], trade_no: u32) -> JournalRow {
    let fees = pos.fees.total();
    let exit_ms = pos.exit_ms.unwrap_or(pos.entry_ms);
    JournalRow {
        id: 0,
        ts: exit_ms,
        position_id: pos.id,
        lane: pos.lane.clone(),
        trade_no,
        symbol: pos.symbol.clone(),
        mint: pos.mint.clone(),
        pnl_sol: pos.pnl_sol,
        pnl_pct: pos.pnl_pct,
        fees_sol: fees,
        gross_sol: pos.pnl_sol + fees,
        hold_seconds: (exit_ms - pos.entry_ms) as f64 / 1000.0,
        exit_reason: pos.exit_reason.clone(),
        score: f.score,
        buyers: f.buyers,
        ratio: f.ratio,
        curve_sol: f.curve_sol,
        age_seconds: f.age_seconds,
        peak_pct: pos.pct(pos.peak_price),
        exit_pct: pos.stages.last().map(|s| pos.pct(s.price)).unwrap_or(0.0),
        fill_seconds: if f.picked_ms > 0 && pos.entry_ms >= f.picked_ms { (pos.entry_ms - f.picked_ms) as f64 / 1000.0 } else { 0.0 },
        observations: observations(pos, f, history),
        source_url: f.source_url.clone(),
        headline: format!("Trade #{trade_no}: {} SOL.", fmt_sol(pos.pnl_sol)),
    }
}

// ============================================================================ review

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub feature: String,
    pub bucket: String,
    /// "weak" (this bucket does worse than the rest) or "strong".
    pub direction: String,
    pub n: u32,
    pub win_rate_pct: f64,
    pub avg_pnl_sol: f64,
    pub rest_n: u32,
    pub rest_win_rate_pct: f64,
    pub rest_avg_pnl_sol: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub key: String,
    pub label: String,
    pub before: f64,
    pub after: f64,
    pub why: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub trades_reviewed: u32,
    pub findings: Vec<Finding>,
    pub exit_lines: Vec<String>,
    pub proposal: Option<Proposal>,
    pub notes: Vec<String>,
}

struct Agg {
    n: u32,
    wins: u32,
    sum: f64,
}

fn agg<'a>(rows: impl Iterator<Item = &'a JournalRow>) -> Agg {
    let mut a = Agg { n: 0, wins: 0, sum: 0.0 };
    for r in rows {
        a.n += 1;
        a.sum += r.pnl_sol;
        if r.pnl_sol > 0.0 {
            a.wins += 1;
        }
    }
    a
}
fn wr(a: &Agg) -> f64 {
    if a.n == 0 { 0.0 } else { a.wins as f64 / a.n as f64 * 100.0 }
}
fn av(a: &Agg) -> f64 {
    if a.n == 0 { 0.0 } else { a.sum / a.n as f64 }
}

/// Settings of the review, from config.
#[derive(Debug, Clone, Copy)]
pub struct ReviewRules {
    pub min_sample: u32,
    pub min_gap_pts: f64,
}

impl ReviewRules {
    pub fn from_config(c: &Config) -> Self {
        // the floors are hard limits: a config cannot lower them
        ReviewRules { min_sample: c.learning.min_sample.max(MIN_SAMPLE_FLOOR), min_gap_pts: c.learning.min_win_rate_gap_pts.max(20.0) }
    }
}

type Feature = (&'static str, &'static str, &'static [(&'static str, f64, f64)], fn(&JournalRow) -> f64);

fn features() -> [Feature; 4] {
    [
        ("curve SOL at entry", "entry.curveSolMin", CURVE_BUCKETS, |r| r.curve_sol),
        ("buy/sell ratio at entry", "entry.minBuySellRatio", RATIO_BUCKETS, |r| r.ratio),
        ("unique buyers at entry", "observation.minUniqueBuyers", BUYER_BUCKETS, |r| r.buyers as f64),
        ("score at entry", "observation.minScore", SCORE_BUCKETS, |r| r.score),
    ]
}

fn weight_key(feature: &str) -> Option<&'static str> {
    match feature {
        "buy/sell ratio at entry" => Some("weights.ratio"),
        "unique buyers at entry" => Some("weights.buyers"),
        _ => None,
    }
}

/// Find patterns in the closed trades of one lane and (maybe) propose ONE change. Pure: same data, same answer.
/// `baseline` is the shipped config; `current` is what the lane runs now.
pub fn review(rows: &[JournalRow], baseline: &Tuned, current: &Tuned, rules: ReviewRules) -> ReviewResult {
    let mut res = ReviewResult { trades_reviewed: rows.len() as u32, ..Default::default() };
    if rows.len() < (rules.min_sample as usize) * 2 {
        res.notes.push(format!("Only {} closed trades so far. A review needs at least {} to compare groups fairly, so nothing is claimed.", rows.len(), rules.min_sample * 2));
        return res;
    }
    // feature buckets
    let mut candidates: Vec<(f64, Finding, Option<(String, f64)>)> = vec![];
    for (fname, thr_key, table, get) in features() {
        for (bi, (label, lo, hi)) in table.iter().enumerate() {
            let inb = |r: &&JournalRow| bucket_of(table, get(r)) == Some(bi);
            let a = agg(rows.iter().filter(inb));
            let rest = agg(rows.iter().filter(|r| bucket_of(table, get(r)) != Some(bi)));
            if a.n < rules.min_sample || rest.n < rules.min_sample {
                continue;
            }
            let gap = wr(&a) - wr(&rest);
            if gap.abs() < rules.min_gap_pts || (gap < 0.0) != (av(&a) < av(&rest)) {
                continue; // the average P/L must point the same way as the win rate
            }
            let weak = gap < 0.0;
            let text = format!(
                "{fname}, {label}: {} trades, {:.0}% won, average {} SOL. All other trades: {}, {:.0}% won, average {} SOL.",
                a.n, wr(&a), fmt_sol(av(&a)), rest.n, wr(&rest), fmt_sol(av(&rest))
            );
            let finding = Finding { feature: fname.into(), bucket: (*label).into(), direction: if weak { "weak" } else { "strong" }.into(), n: a.n, win_rate_pct: wr(&a), avg_pnl_sol: av(&a), rest_n: rest.n, rest_win_rate_pct: wr(&rest), rest_avg_pnl_sol: av(&rest), text };
            // map the pattern onto a whitelisted change
            let mut change = None;
            let is_low = bi == 0 && *lo == 0.0;
            let is_top = bi == table.len() - 1;
            if weak && is_low {
                if let Some(p) = param(thr_key) {
                    // raise the entry bar up to the top of the weak bucket (the most the bounds allow)
                    let want = if *hi < f64::MAX { *hi } else { (p.get)(current) };
                    change = clamp_to_bounds(baseline, thr_key, want).filter(|v| *v > (p.get)(current) + 1e-9).map(|v| (thr_key.to_string(), v));
                }
            } else if weak && is_top && thr_key == "entry.curveSolMin" {
                change = clamp_to_bounds(baseline, "entry.curveSolMax", *lo).filter(|v| *v < (param("entry.curveSolMax").unwrap().get)(current) - 1e-9).map(|v| ("entry.curveSolMax".to_string(), v));
            } else if !weak && is_top {
                if let Some(wk) = weight_key(fname) {
                    let cur = (param(wk).unwrap().get)(current);
                    change = clamp_to_bounds(baseline, wk, cur * 1.2).filter(|v| *v > cur + 1e-9).map(|v| (wk.to_string(), v));
                }
            }
            candidates.push((gap.abs(), finding, change));
        }
    }
    // exit reasons (counts only, plus the trailing stop give-back)
    let mut reasons: Vec<String> = rows.iter().map(|r| r.exit_reason.clone()).collect();
    reasons.sort();
    reasons.dedup();
    let mut trail_change: Option<(f64, String, (String, f64))> = None;
    for reason in &reasons {
        let a = agg(rows.iter().filter(|r| &r.exit_reason == reason));
        if a.n >= rules.min_sample {
            res.exit_lines.push(format!("Exit {}: {} trades, {:.0}% won, average {} SOL.", human_reason(reason), a.n, wr(&a), fmt_sol(av(&a))));
        }
        if reason == "trailing-stop" && a.n >= rules.min_sample {
            let sel: Vec<&JournalRow> = rows.iter().filter(|r| &r.exit_reason == reason).collect();
            let give: f64 = sel.iter().map(|r| (r.peak_pct - r.exit_pct).max(0.0)).sum::<f64>() / sel.len() as f64;
            // what the rule is meant to give back: trailing % of the peak price
            let intended: f64 = sel.iter().map(|r| current.cfg.exits.trailing_stop_pct * (1.0 + r.peak_pct / 100.0)).sum::<f64>() / sel.len() as f64;
            if give - intended >= 3.0 {
                let cur = current.cfg.exits.trailing_stop_pct;
                if let Some(v) = clamp_to_bounds(baseline, "exits.trailingStopPct", cur * 0.8).filter(|v| *v < cur - 1e-9) {
                    let line = format!("Trailing stop exits: {} trades gave back {:.1} points on average from the peak, {:.1} more than the {:.0}% rule intends.", a.n, give, give - intended, cur);
                    trail_change = Some((give - intended, line, ("exits.trailingStopPct".into(), v)));
                }
            }
        }
    }
    // keep the strongest evidence-backed change
    let mut best: Option<(f64, String, (String, f64), Option<Finding>)> = None;
    for (g, f, c) in candidates.iter() {
        res.findings.push(f.clone());
        if let Some(c) = c {
            if best.as_ref().map(|b| *g > b.0).unwrap_or(true) {
                best = Some((*g, f.text.clone(), c.clone(), Some(f.clone())));
            }
        }
    }
    if let Some((g, line, c)) = trail_change {
        res.findings.push(Finding { feature: "trailing stop give-back".into(), bucket: "trailing-stop exits".into(), direction: "weak".into(), n: 0, win_rate_pct: 0.0, avg_pnl_sol: 0.0, rest_n: 0, rest_win_rate_pct: 0.0, rest_avg_pnl_sol: 0.0, text: line.clone() });
        if best.as_ref().map(|b| g * 5.0 > b.0).unwrap_or(true) {
            best = Some((g, line, c, None));
        }
    }
    res.findings.sort_by(|a, b| (b.win_rate_pct - b.rest_win_rate_pct).abs().partial_cmp(&(a.win_rate_pct - a.rest_win_rate_pct).abs()).unwrap_or(std::cmp::Ordering::Equal));
    if res.findings.is_empty() {
        res.notes.push(format!("No pattern was big enough. A group needs at least {} trades, and {} for the rest, with a gap of at least {:.0} points in win rate.", rules.min_sample, rules.min_sample, rules.min_gap_pts));
    }
    if let Some((_, text, (key, after), f)) = best {
        let p = param(&key).unwrap();
        let before = (p.get)(current);
        let why = match f.as_ref().map(|f| f.direction.as_str()) {
            Some("strong") => format!("Coins in the top range do better, so give {} more say (the most allowed is 20% from the shipped value).", p.label.to_lowercase()),
            _ => format!("Change {} from {} to {} (the most allowed is 20% from the shipped value).", p.label.to_lowercase(), trim(before), trim(after)),
        };
        res.proposal = Some(Proposal { key, label: p.label.into(), before, after, why, evidence: vec![text] });
    }
    res
}

pub fn trim(x: f64) -> String {
    let s = format!("{x:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ============================================================================ shadow comparison

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArmStat {
    pub trades: u32,
    pub net_sol: f64,
    pub wins: u32,
    /// None when there are wins and no losses (no upper limit).
    pub profit_factor: Option<f64>,
}

pub fn arm_stat(pnls: &[f64]) -> ArmStat {
    let wins: f64 = pnls.iter().filter(|p| **p > 0.0).sum();
    let losses: f64 = -pnls.iter().filter(|p| **p < 0.0).sum::<f64>();
    ArmStat {
        trades: pnls.len() as u32,
        net_sol: pnls.iter().sum(),
        wins: pnls.iter().filter(|p| **p > 0.0).count() as u32,
        profit_factor: if losses > 0.0 { Some(wins / losses) } else if wins > 0.0 { None } else { Some(0.0) },
    }
}

fn pf_value(a: &ArmStat) -> f64 {
    a.profit_factor.unwrap_or(f64::INFINITY)
}

/// Applied only if the shadow lane beats the main lane on net P/L AND profit factor, with enough trades to mean something.
pub fn shadow_beats(main: &ArmStat, shadow: &ArmStat, min_sample: u32) -> (bool, String) {
    let show = |a: &ArmStat| format!("{} trades, net {} SOL, profit factor {}", a.trades, fmt_sol(a.net_sol), a.profit_factor.map(|p| format!("{p:.2}")).unwrap_or_else(|| "no losses".into()));
    if shadow.trades < min_sample.max(MIN_SAMPLE_FLOOR) {
        return (false, format!("Not enough trades to judge: the test lane made {} (needs {}). Test lane: {}. Main lane: {}.", shadow.trades, min_sample.max(MIN_SAMPLE_FLOOR), show(shadow), show(main)));
    }
    let net = shadow.net_sol > main.net_sol;
    let pf = pf_value(shadow) > pf_value(main);
    let verdict = if net && pf { "The test lane won on both." } else if net { "The test lane made more but its profit factor was not better." } else if pf { "The test lane had a better profit factor but made less." } else { "The main lane did better." };
    (net && pf, format!("{verdict} Test lane: {}. Main lane: {}.", show(shadow), show(main)))
}

/// The promotion rule (SPEC 4c): the holdout (shadow lane) trades must make money on average AND the lower 95% bootstrap bound of
/// their mean return must be above 0, with at least `min_trades` trades. The shadow mean must also not be below the main lane's.
/// Returns are fractions of the trade size (0.02 = +2%).
pub fn promotion_verdict(main_returns: &[f64], shadow_returns: &[f64], min_trades: u32, resamples: u32) -> (bool, String) {
    let min = min_trades.max(MIN_SAMPLE_FLOOR);
    let (ok, why) = crate::stats::holdout_verdict(shadow_returns, min, resamples);
    let main_mean = crate::stats::mean(main_returns);
    let shadow_mean = crate::stats::mean(shadow_returns);
    let worse = ok && shadow_mean < main_mean;
    let text = format!("Test lane (holdout): {why} Main lane over the same period: {} trades, mean {:+.2}% per trade.{}", main_returns.len(), main_mean * 100.0, if worse { " The test lane's mean is below the main lane's, so it is not applied." } else { "" });
    (ok && !worse, text)
}

// ============================================================================ the stateful part

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub id: i64,
    pub ts: i64,
    pub key: String,
    pub label: String,
    pub before: f64,
    pub after: f64,
    /// shadow, applied, rejected, undone, awaiting-approval
    pub status: String,
    pub why: String,
    pub evidence: Vec<String>,
    pub result: String,
    pub start_ms: i64,
    pub main_start: u32,
    pub target: u32,
    pub done_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteRow {
    pub id: i64,
    pub ts: i64,
    pub kind: String,
    pub text: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Efficiency {
    pub fills: u32,
    pub avg_pick_to_fill_s: f64,
    pub failed_buys: u32,
    pub failed_tx_cost_sol: f64,
    pub fees_sol: f64,
    pub gross_profit_sol: f64,
    /// Fees as a share of the gross profit of winning trades. None with no winners.
    pub fee_share_pct: Option<f64>,
    pub missed: u32,
    pub notes: Vec<NoteRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewView {
    pub ts: i64,
    pub trades_reviewed: u32,
    pub result: ReviewResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningSnapshot {
    pub ts: i64,
    pub enabled: bool,
    pub live_mode: bool,
    /// True only for the screenshots' seeded data.
    pub demo: bool,
    pub closed_trades: u32,
    pub review_every: u32,
    pub next_review_in: u32,
    pub journal: Vec<JournalRow>,
    pub changes: Vec<ChangeRow>,
    pub shadow: Option<ChangeRow>,
    pub shadow_progress: Option<ShadowProgress>,
    pub last_review: Option<ReviewView>,
    pub efficiency: Efficiency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShadowProgress {
    pub main_done: u32,
    pub target: u32,
    pub shadow: ArmStat,
    pub main: ArmStat,
}

/// What the trader must do after a learning step.
#[derive(Debug, Clone)]
pub enum Effect {
    SetMain(Box<Tuned>),
    /// Some = start or update the shadow lane with this tuning, None = stop it.
    SetShadow(Option<Box<Tuned>>),
    Log(String),
}

pub fn init(db: &Db) {
    db.with_conn(|c| {
        let _ = c.execute_batch(
            "CREATE TABLE IF NOT EXISTS trade_features(position_id INTEGER PRIMARY KEY, lane TEXT, json TEXT);
             CREATE TABLE IF NOT EXISTS journal(id INTEGER PRIMARY KEY AUTOINCREMENT, position_id INTEGER UNIQUE, lane TEXT, ts INTEGER, json TEXT);
             CREATE TABLE IF NOT EXISTS reviews(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, trades INTEGER, json TEXT);
             CREATE TABLE IF NOT EXISTS changes(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, json TEXT);
             CREATE TABLE IF NOT EXISTS learning_notes(id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, kind TEXT, text TEXT, source_url TEXT, mint TEXT);",
        );
    });
}

// ---- read-only views of the learning tables (the Obsidian vault writes notes from these; nothing is created here)

pub fn journal_row_for(db: &Db, position_id: i64) -> Option<JournalRow> {
    db.with_conn(|c| c.query_row("SELECT json FROM journal WHERE position_id=?", params![position_id], |r| r.get::<_, String>(0)).optional().ok().flatten()).and_then(|j| serde_json::from_str(&j).ok())
}

pub fn features_for(db: &Db, position_id: i64) -> Option<EntryFeatures> {
    db.with_conn(|c| c.query_row("SELECT json FROM trade_features WHERE position_id=?", params![position_id], |r| r.get::<_, String>(0)).optional().ok().flatten()).and_then(|j| serde_json::from_str(&j).ok())
}

/// (review id, ts, trades, result)
pub fn reviews_all(db: &Db) -> Vec<(i64, i64, u32, ReviewResult)> {
    db.with_conn(|c| {
        let Ok(mut st) = c.prepare("SELECT id,ts,trades,json FROM reviews ORDER BY id") else { return vec![] };
        let rows: Vec<(i64, i64, u32, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<u32>>(2)?.unwrap_or(0), r.get(3)?))).map(|it| it.flatten().collect()).unwrap_or_default();
        rows.into_iter().filter_map(|(id, ts, n, j)| serde_json::from_str::<ReviewResult>(&j).ok().map(|r| (id, ts, n, r))).collect()
    })
}

pub fn changes_all(db: &Db) -> Vec<ChangeRow> {
    db.with_conn(|c| {
        let Ok(mut st) = c.prepare("SELECT id,json FROM changes ORDER BY id") else { return vec![] };
        let rows: Vec<(i64, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|it| it.flatten().collect()).unwrap_or_default();
        rows.into_iter().filter_map(|(id, j)| serde_json::from_str::<ChangeRow>(&j).ok().map(|mut r| { r.id = id; r })).collect()
    })
}

/// (ts, kind, text, source_url, mint), oldest first.
pub fn notes_all(db: &Db) -> Vec<(i64, String, String, String, String)> {
    db.with_conn(|c| {
        let Ok(mut st) = c.prepare("SELECT ts,kind,text,COALESCE(source_url,''),COALESCE(mint,'') FROM learning_notes ORDER BY id") else { return vec![] };
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map(|it| it.flatten().collect()).unwrap_or_default()
    })
}

pub struct Learner {
    db: Arc<Db>,
    hub: Option<Arc<Hub>>,
    baseline: Tuned,
    rules: ReviewRules,
    review_every: u32,
    shadow_trades: u32,
    promotion_min_trades: u32,
    resamples: u32,
    enabled: bool,
    live: bool,
}

pub const MAIN_LANE: &str = "momentum";
pub const SHADOW_LANE: &str = "shadow";

impl Learner {
    pub fn new(db: Arc<Db>, hub: Option<Arc<Hub>>, baseline_cfg: &Config) -> Learner {
        init(&db);
        Learner {
            db,
            hub,
            baseline: Tuned::from_config(baseline_cfg),
            rules: ReviewRules::from_config(baseline_cfg),
            review_every: baseline_cfg.learning.review_every_trades.max(1),
            shadow_trades: baseline_cfg.learning.shadow_trades.max(1),
            promotion_min_trades: baseline_cfg.learning.promotion_min_trades,
            resamples: baseline_cfg.learning.bootstrap_resamples.max(200),
            enabled: baseline_cfg.learning.enabled,
            live: baseline_cfg.mode == Mode::Live,
        }
    }

    pub fn baseline(&self) -> &Tuned {
        &self.baseline
    }

    // ---------------------------------------------------------------- storage helpers

    pub fn record_features(&self, position_id: i64, lane: &str, f: &EntryFeatures) {
        let json = serde_json::to_string(f).unwrap_or_default();
        self.db.with_conn(|c| {
            let _ = c.execute("INSERT OR REPLACE INTO trade_features(position_id,lane,json) VALUES(?,?,?)", params![position_id, lane, json]);
        });
    }

    pub fn features_of(&self, position_id: i64) -> Option<EntryFeatures> {
        self.db.with_conn(|c| c.query_row("SELECT json FROM trade_features WHERE position_id=?", params![position_id], |r| r.get::<_, String>(0)).optional().ok().flatten()).and_then(|j| serde_json::from_str(&j).ok())
    }

    fn journal_rows(&self, lane: Option<&str>) -> Vec<JournalRow> {
        self.db.with_conn(|c| {
            let sql = if lane.is_some() { "SELECT id,json FROM journal WHERE lane=? ORDER BY ts,id" } else { "SELECT id,json FROM journal ORDER BY ts,id" };
            let Ok(mut st) = c.prepare(sql) else { return vec![] };
            let map = |r: &rusqlite::Row| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?));
            let rows: Vec<(i64, String)> = match lane {
                Some(l) => st.query_map(params![l], map).map(|it| it.flatten().collect()).unwrap_or_default(),
                None => st.query_map([], map).map(|it| it.flatten().collect()).unwrap_or_default(),
            };
            rows.into_iter().filter_map(|(id, j)| serde_json::from_str::<JournalRow>(&j).ok().map(|mut r| { r.id = id; r })).collect()
        })
    }

    fn changes(&self) -> Vec<ChangeRow> {
        self.db.with_conn(|c| {
            let Ok(mut st) = c.prepare("SELECT id,json FROM changes ORDER BY id") else { return vec![] };
            let rows: Vec<(i64, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|it| it.flatten().collect()).unwrap_or_default();
            rows.into_iter().filter_map(|(id, j)| serde_json::from_str::<ChangeRow>(&j).ok().map(|mut r| { r.id = id; r })).collect()
        })
    }

    fn save_change(&self, ch: &ChangeRow) -> i64 {
        let json = serde_json::to_string(ch).unwrap_or_default();
        self.db.with_conn(|c: &Connection| {
            if ch.id > 0 {
                let _ = c.execute("UPDATE changes SET json=? WHERE id=?", params![json, ch.id]);
                ch.id
            } else {
                let _ = c.execute("INSERT INTO changes(ts,json) VALUES(?,?)", params![ch.ts, json]);
                c.last_insert_rowid()
            }
        })
    }

    pub fn add_note(&self, ts: i64, kind: &str, text: &str, source_url: &str, mint: &str) {
        self.db.with_conn(|c| {
            let _ = c.execute("INSERT INTO learning_notes(ts,kind,text,source_url,mint) VALUES(?,?,?,?,?)", params![ts, kind, text, source_url, mint]);
        });
    }

    fn notes(&self, limit: u32) -> Vec<NoteRow> {
        self.db.with_conn(|c| {
            let Ok(mut st) = c.prepare("SELECT id,ts,kind,text,source_url FROM learning_notes ORDER BY id DESC LIMIT ?") else { return vec![] };
            st.query_map(params![limit], |r| Ok(NoteRow { id: r.get(0)?, ts: r.get(1)?, kind: r.get(2)?, text: r.get(3)?, source_url: r.get::<_, Option<String>>(4)?.unwrap_or_default() })).map(|it| it.flatten().collect()).unwrap_or_default()
        })
    }

    fn kv_u32(&self, k: &str) -> u32 {
        self.db.kv_get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// The tuning that applied changes add up to.
    pub fn current_main(&self) -> Tuned {
        let applied: Vec<(String, f64)> = self.changes().into_iter().filter(|c| c.status == "applied").map(|c| (c.key, c.after)).collect();
        tuned_from(&self.baseline, &applied)
    }

    fn active_shadow(&self) -> Option<ChangeRow> {
        self.changes().into_iter().rev().find(|c| c.status == "shadow")
    }

    pub fn shadow_tuned(&self) -> Option<Tuned> {
        let ch = self.active_shadow()?;
        let cur = self.current_main();
        let v = check_proposal(&self.baseline, &cur, &ch.key, ch.after).ok()?;
        let mut t = cur;
        (param(&ch.key)?.set)(&mut t, v);
        Some(t)
    }

    /// On start: put the applied tuning and any running shadow test back on the lanes.
    pub fn restore(&self) -> Vec<Effect> {
        let mut fx = vec![Effect::SetMain(Box::new(self.current_main()))];
        fx.push(Effect::SetShadow(self.shadow_tuned().map(Box::new)));
        fx
    }

    // ---------------------------------------------------------------- the trade-close hook

    /// Called for every closed real trade of any lane. Returns what the trader must change.
    pub fn on_close(&self, pos: &Position) -> Vec<Effect> {
        let mut fx = vec![];
        if !self.enabled || pos.kind != "trade" {
            return fx;
        }
        let now = pos.exit_ms.unwrap_or(pos.entry_ms);
        if pos.lane != SHADOW_LANE {
            let f = self.features_of(pos.id).unwrap_or_default();
            let hist = self.journal_rows(Some(&pos.lane));
            let no = hist.len() as u32 + 1;
            let row = journal_row(pos, &f, &hist, no);
            let json = serde_json::to_string(&row).unwrap_or_default();
            self.db.with_conn(|c| {
                let _ = c.execute("INSERT OR IGNORE INTO journal(position_id,lane,ts,json) VALUES(?,?,?,?)", params![pos.id, pos.lane, row.ts, json]);
            });
        }
        if pos.lane == MAIN_LANE || pos.lane == SHADOW_LANE {
            fx.extend(self.conclude_shadow_if_ready(now));
        }
        if pos.lane == MAIN_LANE {
            fx.extend(self.review_if_due(now));
        }
        self.emit(now);
        fx
    }

    fn main_closed_count(&self) -> u32 {
        self.journal_rows(Some(MAIN_LANE)).len() as u32
    }

    fn review_if_due(&self, now: i64) -> Vec<Effect> {
        let n = self.main_closed_count();
        let last = self.kv_u32("learn.last_review_count");
        if n < last + self.review_every {
            return vec![];
        }
        self.db.kv_set("learn.last_review_count", &n.to_string());
        let rows = self.journal_rows(Some(MAIN_LANE));
        let cur = self.current_main();
        let mut res = review(&rows, &self.baseline, &cur, self.rules);
        let busy = self.changes().iter().any(|c| c.status == "shadow" || c.status == "awaiting-approval");
        let mut fx = vec![];
        if let Some(p) = res.proposal.clone() {
            if busy {
                res.notes.push("A change is already being tested or waiting for approval, so this proposal was not started.".into());
                res.proposal = None;
            } else if let Ok(after) = check_proposal(&self.baseline, &cur, &p.key, p.after) {
                let ch = ChangeRow { id: 0, ts: now, key: p.key.clone(), label: p.label.clone(), before: p.before, after, status: "shadow".into(), why: p.why.clone(), evidence: p.evidence.clone(), result: String::new(), start_ms: now, main_start: n, target: self.shadow_trades, done_ms: 0 };
                let mut ch2 = ch.clone();
                ch2.id = self.save_change(&ch);
                fx.push(Effect::Log(format!("LEARNING review done on {n} trades. Testing {} {} -> {} in the shadow lane for the next {} trades.", p.key, trim(p.before), trim(after), self.shadow_trades)));
                fx.push(Effect::SetShadow(self.shadow_tuned().map(Box::new)));
            } else {
                res.proposal = None;
                res.notes.push("The proposed change was refused by the safety limits.".into());
            }
        }
        let json = serde_json::to_string(&res).unwrap_or_default();
        self.db.with_conn(|c| {
            let _ = c.execute("INSERT INTO reviews(ts,trades,json) VALUES(?,?,?)", params![now, n, json]);
        });
        fx.push(Effect::Log(format!("LEARNING review: {} pattern(s) found in {n} trades, proposal: {}", res.findings.len(), res.proposal.as_ref().map(|p| format!("{} {} -> {}", p.key, trim(p.before), trim(p.after))).unwrap_or_else(|| "none".into()))));
        fx
    }

    fn arms_since(&self, ch: &ChangeRow) -> (ArmStat, ArmStat) {
        let pn = |lane: &str| -> Vec<f64> { self.db.load_closed(lane, false).into_iter().filter(|p| p.exit_ms.unwrap_or(0) >= ch.start_ms && p.entry_ms >= ch.start_ms).map(|p| p.pnl_sol).collect() };
        (arm_stat(&pn(MAIN_LANE)), arm_stat(&pn(SHADOW_LANE)))
    }

    /// Per-trade returns (net P/L divided by the trade size) of the main and the shadow lane since the test started.
    fn returns_since(&self, ch: &ChangeRow) -> (Vec<f64>, Vec<f64>) {
        let rt = |lane: &str| -> Vec<f64> { self.db.load_closed(lane, false).into_iter().filter(|p| p.exit_ms.unwrap_or(0) >= ch.start_ms && p.entry_ms >= ch.start_ms && p.size_sol > 0.0).map(|p| p.pnl_sol / p.size_sol).collect() };
        (rt(MAIN_LANE), rt(SHADOW_LANE))
    }

    fn conclude_shadow_if_ready(&self, now: i64) -> Vec<Effect> {
        let Some(mut ch) = self.active_shadow() else { return vec![] };
        let (main, shadow) = self.arms_since(&ch);
        if main.trades < ch.target {
            return vec![];
        }
        let _ = shadow;
        let (main_r, shadow_r) = self.returns_since(&ch);
        let (beats, text) = promotion_verdict(&main_r, &shadow_r, self.promotion_min_trades, self.resamples);
        ch.result = text;
        ch.done_ms = now;
        let mut fx = vec![Effect::SetShadow(None)];
        if beats && self.live {
            ch.status = "awaiting-approval".into();
            fx.push(Effect::Log(format!("LEARNING test of {} won. Live mode: waiting for your approval.", ch.key)));
        } else if beats {
            ch.status = "applied".into();
            self.save_change(&ch);
            fx.push(Effect::SetMain(Box::new(self.current_main())));
            fx.push(Effect::Log(format!("LEARNING applied {} {} -> {}. {}", ch.key, trim(ch.before), trim(ch.after), ch.result)));
            return fx;
        } else {
            ch.status = "rejected".into();
            fx.push(Effect::Log(format!("LEARNING dropped {} {} -> {}. {}", ch.key, trim(ch.before), trim(ch.after), ch.result)));
        }
        self.save_change(&ch);
        fx
    }

    // ---------------------------------------------------------------- commands (undo, approve)

    pub fn command(&self, cmd: &str) -> (String, Vec<Effect>) {
        let now = crate::rpc::now_ms();
        let (verb, id) = match cmd.split_once(':') {
            Some((v, i)) => (v, i.parse::<i64>().unwrap_or(0)),
            None => (cmd, 0),
        };
        let Some(mut ch) = self.changes().into_iter().find(|c| c.id == id) else { return ("No such change.".into(), vec![]) };
        match (verb, ch.status.as_str()) {
            ("undo-change", "applied") => {
                ch.status = "undone".into();
                ch.done_ms = now;
                self.save_change(&ch);
                let fx = vec![Effect::SetMain(Box::new(self.current_main())), Effect::Log(format!("LEARNING undid {} (back to {})", ch.key, trim(ch.before)))];
                self.emit(now);
                (format!("Undone. {} is back to {}.", ch.label, trim(ch.before)), fx)
            }
            ("approve-change", "awaiting-approval") => {
                ch.status = "applied".into();
                ch.done_ms = now;
                self.save_change(&ch);
                let fx = vec![Effect::SetMain(Box::new(self.current_main())), Effect::Log(format!("LEARNING you approved {} {} -> {}", ch.key, trim(ch.before), trim(ch.after)))];
                self.emit(now);
                (format!("Approved. {} is now {}.", ch.label, trim(ch.after)), fx)
            }
            ("undo-change", "awaiting-approval") | ("undo-change", "shadow") => {
                ch.status = "rejected".into();
                ch.done_ms = now;
                self.save_change(&ch);
                let fx = vec![Effect::SetShadow(None)];
                self.emit(now);
                ("Dropped. Nothing was changed.".into(), fx)
            }
            _ => (format!("That change is {}, so this does nothing.", ch.status), vec![]),
        }
    }

    // ---------------------------------------------------------------- efficiency

    fn efficiency(&self) -> Efficiency {
        let mut e = Efficiency::default();
        let mut delays = vec![];
        for r in self.journal_rows(None) {
            if r.fill_seconds > 0.0 {
                delays.push(r.fill_seconds);
            }
            e.fees_sol += r.fees_sol;
            if r.gross_sol > 0.0 && r.pnl_sol > 0.0 {
                e.gross_profit_sol += r.gross_sol;
            }
        }
        e.fills = delays.len() as u32;
        e.avg_pick_to_fill_s = if delays.is_empty() { 0.0 } else { delays.iter().sum::<f64>() / delays.len() as f64 };
        let (fb, fc): (u32, f64) = self.db.with_conn(|c| c.query_row("SELECT COUNT(*), COALESCE(SUM(failed_sol),0) FROM positions WHERE kind='failed-buy'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, 0.0)));
        let sells: f64 = self.db.with_conn(|c| c.query_row("SELECT COALESCE(SUM(failed_sol),0) FROM positions WHERE kind='trade' AND lane!='shadow'", [], |r| r.get(0)).unwrap_or(0.0));
        e.failed_buys = fb;
        e.failed_tx_cost_sol = fc + sells;
        // fee share only over winning trades, so it compares fees with the gross profit they came out of
        let win_fees: f64 = self.journal_rows(None).iter().filter(|r| r.pnl_sol > 0.0).map(|r| r.fees_sol).sum();
        e.fee_share_pct = if e.gross_profit_sol > 0.0 { Some(win_fees / e.gross_profit_sol * 100.0) } else { None };
        e.missed = self.db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM learning_notes WHERE kind='missed'", [], |r| r.get(0)).unwrap_or(0));
        e.notes = self.notes(6);
        e
    }

    // ---------------------------------------------------------------- snapshot for the dashboard

    pub fn snapshot(&self, now: i64) -> LearningSnapshot {
        let main_rows = self.journal_rows(Some(MAIN_LANE));
        let n = main_rows.len() as u32;
        let last = self.kv_u32("learn.last_review_count");
        let mut journal: Vec<JournalRow> = self.journal_rows(None);
        journal.reverse();
        journal.truncate(8);
        let changes: Vec<ChangeRow> = { let mut c = self.changes(); c.reverse(); c.truncate(10); c };
        let shadow = self.active_shadow();
        let shadow_progress = shadow.as_ref().map(|ch| {
            let (m, s) = self.arms_since(ch);
            ShadowProgress { main_done: m.trades.min(ch.target), target: ch.target, shadow: s, main: m }
        });
        let last_review = self.db.with_conn(|c| c.query_row("SELECT ts,trades,json FROM reviews ORDER BY id DESC LIMIT 1", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u32>(1)?, r.get::<_, String>(2)?))).optional().ok().flatten()).and_then(|(ts, trades, j)| serde_json::from_str::<ReviewResult>(&j).ok().map(|result| ReviewView { ts, trades_reviewed: trades, result }));
        LearningSnapshot {
            ts: now,
            enabled: self.enabled,
            live_mode: self.live,
            demo: false,
            closed_trades: n,
            review_every: self.review_every,
            next_review_in: (last + self.review_every).saturating_sub(n),
            journal,
            changes,
            shadow,
            shadow_progress,
            last_review,
            efficiency: self.efficiency(),
        }
    }

    pub fn emit(&self, now: i64) {
        if let Some(h) = &self.hub {
            h.emit(EngineEvent::Learning(Box::new(self.snapshot(now))));
        }
    }
}

// ============================================================================ missed coins

/// What the price did in the 5 minutes after a coin was skipped. None when no trades were seen afterwards.
pub fn missed_outcome(trades: &[crate::pump::Trade], base_price: f64, from_ms: i64) -> Option<(f64, f64, usize)> {
    if base_price <= 0.0 {
        return None;
    }
    let mut after: Vec<&crate::pump::Trade> = trades.iter().filter(|t| t.recv_ms > from_ms && t.recv_ms <= from_ms + MISSED_WINDOW_MS && t.price() > 0.0).collect();
    if after.is_empty() {
        return None;
    }
    after.sort_by_key(|t| (t.slot, t.recv_ms));
    let end = after.last().unwrap().price();
    let peak = after.iter().map(|t| t.price()).fold(0.0, f64::max);
    Some(((end / base_price - 1.0) * 100.0, (peak / base_price - 1.0) * 100.0, after.len()))
}

pub fn missed_text(symbol: &str, score: f64, blocked_by: &str, outcome: Option<(f64, f64, usize)>) -> String {
    match outcome {
        Some((end, peak, n)) => format!("Missed {symbol} (score {score:.1}): blocked by {blocked_by}. Over the next 5 minutes ({n} trades seen) the price ended {end:+.0}% and peaked at {peak:+.0}%."),
        None => format!("Missed {symbol} (score {score:.1}): blocked by {blocked_by}. No price data was seen in the next 5 minutes, so what it did is unknown."),
    }
}

// ============================================================================ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::FeeBreakdown;
    use crate::position::{Position, StageSell};

    fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    fn closed_pos(id: i64, lane: &str, pnl: f64, reason: &str, peak_mult: f64, exit_mult: f64) -> Position {
        let mut p = Position::new_buying(lane, &format!("M{id}"), &format!("T{id}"), "C", "CV", 0.1, 1_000_000, 150.0);
        p.id = id;
        p.fill_buy(1e-7, 1_000_000.0, 7000.0, &FeeBreakdown { pump_fun: 0.001, creator: 0.0003, pump_portal: 0.001, tx: 0.000005, priority: 0.0005, jito: 0.0 }, 1.2).unwrap();
        p.peak_price = 1e-7 * peak_mult;
        p.stages.push(StageSell { ts: 1_050_000, reason: reason.into(), tokens: 1_000_000.0, price: 1e-7 * exit_mult, gross_sol: 0.1 * exit_mult, net_sol: 0.1 * exit_mult - 0.002, fees_sol: 0.002 });
        p.state = crate::position::PosState::Closed;
        p.exit_ms = Some(1_000_000 + 53_000);
        p.exit_reason = reason.into();
        p.pnl_sol = pnl;
        p.pnl_pct = pnl / 0.1 * 100.0;
        p.fees.tx += 0.0;
        p
    }

    fn feats(curve: f64, ratio: f64, buyers: u32, score: f64) -> EntryFeatures {
        EntryFeatures { score, buyers, buys: 20, sells: 4, ratio, curve_sol: curve, age_seconds: 60.0, mcap_sol: 40.0, picked_ms: 999_000, source_url: "https://example.org/coin".into(), ..Default::default() }
    }

    fn row(i: u32, pnl: f64, curve: f64, ratio: f64, buyers: u32, score: f64, reason: &str) -> JournalRow {
        let p = closed_pos(i as i64, MAIN_LANE, pnl, reason, 1.34, 1.25);
        let mut r = journal_row(&p, &feats(curve, ratio, buyers, score), &[], i);
        r.exit_reason = reason.into();
        r
    }

    fn base() -> Tuned {
        Tuned::from_config(&cfg())
    }

    // ---- observations come from data

    #[test]
    fn observation_numbers_match_the_trade_and_nothing_is_invented() {
        let p = closed_pos(7, MAIN_LANE, 0.012, "trailing-stop", 1.34, 1.25);
        let o = observations(&p, &feats(4.2, 3.0, 9, 88.0), &[]);
        let all = o.join(" ");
        assert!(all.contains("the peak was +34%") && all.contains("last sell was at +25%") && all.contains("gave back 9 points"), "{all}");
        let fees = p.fees.total();
        assert!(all.contains(&format!("Fees were {:.0}% of the gross profit", fees / (0.012 + fees) * 100.0)), "{all}");
        assert!(all.contains("Entered at curve 4.2 SOL"), "{all}");
        assert!(all.contains("filled 1.0 s after the pick"), "{all}");
        // a different trade gives different sentences: they are computed, not canned
        let q = closed_pos(8, MAIN_LANE, -0.02, "hard-stop", 1.05, 0.85);
        let o2 = observations(&q, &feats(12.0, 3.0, 9, 88.0), &[]).join(" ");
        assert!(o2.contains("the peak was +5%") && o2.contains("hard stop") && !o2.contains("gave back"), "{o2}");
        assert!(o2.contains("fees of") && o2.contains("took the result to"), "{o2}");
    }

    #[test]
    fn the_curve_range_record_counts_real_prior_trades() {
        let hist: Vec<JournalRow> = (1..=7).map(|i| row(i, if i <= 2 { 0.01 } else { -0.01 }, 3.0, 3.0, 8, 85.0, "hard-stop")).collect();
        let p = closed_pos(8, MAIN_LANE, 0.012, "take-profit-1", 1.3, 1.25);
        let o = observations(&p, &feats(4.2, 3.0, 9, 88.0), &hist).join(" ");
        // 2 earlier wins + this win = 3W, 5 earlier losses = 5L
        assert!(o.contains("under 5 SOL are 3W/5L so far"), "{o}");
        let first = observations(&p, &feats(4.2, 3.0, 9, 88.0), &[]).join(" ");
        assert!(first.contains("first trade under 5 SOL"), "{first}");
    }

    #[test]
    fn no_data_means_no_claims() {
        let r = review(&[], &base(), &base(), ReviewRules::from_config(&cfg()));
        assert!(r.findings.is_empty() && r.proposal.is_none());
        assert!(r.notes[0].contains("nothing is claimed"));
        let few: Vec<JournalRow> = (1..=12).map(|i| row(i, if i % 2 == 0 { 0.01 } else { -0.01 }, 3.0, 3.0, 8, 85.0, "hard-stop")).collect();
        let r = review(&few, &base(), &base(), ReviewRules::from_config(&cfg()));
        assert!(r.proposal.is_none(), "12 trades are below 2 x 10, no proposal");
    }

    // ---- review thresholds

    fn mixed(low_n: u32, low_wins: u32, rest_n: u32, rest_wins: u32) -> Vec<JournalRow> {
        let mut v = vec![];
        let mut i = 0;
        for k in 0..low_n {
            i += 1;
            v.push(row(i, if k < low_wins { 0.01 } else { -0.012 }, 4.2, 3.0, 9, 88.0, "hard-stop"));
        }
        for k in 0..rest_n {
            i += 1;
            v.push(row(i, if k < rest_wins { 0.012 } else { -0.01 }, 8.0, 3.0, 9, 88.0, "take-profit-1"));
        }
        v
    }

    #[test]
    fn a_pattern_needs_ten_trades_in_the_group_and_a_big_gap() {
        let rules = ReviewRules::from_config(&cfg());
        // 9 low-curve trades, all losses: below the sample of 10 -> not reported
        let r = review(&mixed(9, 0, 21, 15), &base(), &base(), rules);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        // 10 low-curve trades, 2 wins (20%) vs rest 20 trades 14 wins (70%): gap 50 -> reported
        let r = review(&mixed(10, 2, 20, 14), &base(), &base(), rules);
        let f = r.findings.iter().find(|f| f.bucket == "under 5 SOL").expect("reported");
        assert_eq!((f.n, f.rest_n), (10, 20));
        assert!((f.win_rate_pct - 20.0).abs() < 1e-9 && (f.rest_win_rate_pct - 70.0).abs() < 1e-9);
        assert!(f.text.contains("10 trades, 20% won") && f.text.contains("20, 70% won"), "{}", f.text);
        // same sample but a small gap (40% vs 50%): not reported
        let r = review(&mixed(10, 4, 20, 10), &base(), &base(), rules);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }

    #[test]
    fn config_cannot_lower_the_hard_floors() {
        let mut c = cfg();
        c.learning.min_sample = 3;
        c.learning.min_win_rate_gap_pts = 5.0;
        let r = ReviewRules::from_config(&c);
        assert_eq!((r.min_sample, r.min_gap_pts), (10, 20.0));
        assert!(c.validate().is_err());
    }

    #[test]
    fn the_proposal_stays_within_20_percent_of_the_shipped_value() {
        let rules = ReviewRules::from_config(&cfg());
        let r = review(&mixed(12, 2, 24, 17), &base(), &base(), rules);
        let p = r.proposal.expect("a proposal");
        assert_eq!(p.key, "entry.curveSolMin");
        // shipped 4.0, weak bucket ends at 5.0 -> +25% wanted, clamped to +20% = 4.8
        assert!((p.before - 4.0).abs() < 1e-9 && (p.after - 4.8).abs() < 1e-9, "{p:?}");
        assert!(p.evidence[0].contains("12 trades"));
        // already at the limit: no second step beyond 20%
        let mut cur = base();
        cur.cfg.entry.curve_sol_min = 4.8;
        let r2 = review(&mixed(12, 2, 24, 17), &base(), &cur, rules);
        assert!(r2.proposal.is_none(), "{:?}", r2.proposal);
    }

    #[test]
    fn bounds_check_refuses_anything_outside_or_off_the_list() {
        let b = base();
        assert!(check_proposal(&b, &b, "entry.curveSolMin", 4.8).is_ok());
        assert!(check_proposal(&b, &b, "entry.curveSolMin", 4.81).is_err());
        assert!(check_proposal(&b, &b, "entry.curveSolMin", 3.2).is_ok());
        assert!(check_proposal(&b, &b, "entry.curveSolMin", 3.19).is_err());
        assert!(check_proposal(&b, &b, "exits.softStopPct", -12.0).is_ok());
        assert!(check_proposal(&b, &b, "exits.softStopPct", -12.5).is_err());
        assert!(check_proposal(&b, &b, "exits.hardStopPct", -18.0).is_ok());
        assert!(check_proposal(&b, &b, "observation.minUniqueBuyers", 6.0).is_ok());
        assert!(check_proposal(&b, &b, "observation.minUniqueBuyers", 7.0).is_err());
        assert!(check_proposal(&b, &b, "weights.ratio", 30.0).is_ok());
        assert!(check_proposal(&b, &b, "weights.ratio", 31.0).is_err());
        assert!(check_proposal(&b, &b, "entry.curveSolMin", f64::NAN).is_err());
        // a stop that would cross the other stop is refused by the config checks
        let mut cur = b.clone();
        cur.cfg.exits.soft_stop_pct = -12.0;
        cur.cfg.exits.hard_stop_pct = -12.0;
        assert!(check_proposal(&b, &cur, "exits.hardStopPct", -12.0).is_ok());
        cur.cfg.exits.soft_stop_pct = -14.0;
        assert!(check_proposal(&b, &cur, "exits.hardStopPct", -12.0).is_err());
    }

    #[test]
    fn scam_shield_trade_size_fees_and_loss_limits_can_never_be_changed() {
        let b = base();
        for k in [
            "filters.mintAuthorityRevoked", "filters.freezeAuthorityRevoked", "filters.maxCreatorHoldingPct", "filters.maxInsidersPct", "filters.maxSingleHolderPct",
            "filters.maxTopHoldersCombinedPct", "filters.rejectDevSnipeFirstBlock", "filters.rejectBundle", "filters.requireCommunityChecks.minSocialLinks",
            "risk.maxTradeSol", "risk.minTradeSol", "risk.maxPctBankrollPerTrade", "risk.dailyLossLimitPct", "risk.consecutiveLossStop", "risk.maxOpenPositions",
            "risk.maxTotalExposurePct", "fees.pumpPortalPctPerSide", "paper.bankrollSol", "mode", "exits.maxHoldSeconds",
        ] {
            assert!(param(k).is_none(), "{k} must not be tunable");
            assert!(check_proposal(&b, &b, k, 1.0).is_err(), "{k}");
        }
        assert!(PARAMS.iter().all(|p| !p.key.starts_with("filters.") && !p.key.starts_with("risk.") && !p.key.starts_with("fees.") && !p.key.starts_with("paper.")));
        // applying a stored change for a forbidden key does nothing
        let t = tuned_from(&b, &[("filters.maxCreatorHoldingPct".into(), 50.0), ("risk.maxTradeSol".into(), 5.0)]);
        assert_eq!(t.cfg.filters, b.cfg.filters);
        assert_eq!(t.cfg.risk, b.cfg.risk);
        // and chained changes cannot drift past 20% of the shipped value
        let t = tuned_from(&b, &[("entry.curveSolMin".into(), 4.8), ("entry.curveSolMin".into(), 5.76)]);
        assert!((t.cfg.entry.curve_sol_min - 4.8).abs() < 1e-9);
    }

    // ---- shadow test

    #[test]
    fn a_change_is_applied_only_if_it_beats_main_on_net_and_profit_factor() {
        let main = arm_stat(&[0.01, -0.01, 0.02, -0.02, 0.01, -0.005, 0.01, -0.01, 0.0, 0.0, 0.01, -0.01]);
        let better = arm_stat(&[0.02, -0.005, 0.02, -0.005, 0.03, -0.005, 0.02, -0.005, 0.02, -0.005]);
        assert!(shadow_beats(&main, &better, 10).0);
        let more_but_worse_pf = arm_stat(&[0.1, 0.1, 0.1, -0.04, -0.04, -0.04, -0.04, -0.04, -0.04, -0.04]);
        assert!(more_but_worse_pf.net_sol > main.net_sol);
        assert!(!shadow_beats(&main, &more_but_worse_pf, 10).0);
        let worse = arm_stat(&[-0.01; 12]);
        assert!(!shadow_beats(&main, &worse, 10).0);
        let few = arm_stat(&[0.05, 0.05, 0.05]);
        let (ok, why) = shadow_beats(&main, &few, 10);
        assert!(!ok && why.contains("Not enough trades"), "{why}");
        // both without losses: not strictly better
        let a = arm_stat(&[0.01; 10]);
        assert!(!shadow_beats(&a, &arm_stat(&[0.02; 10]), 10).0);
    }

    #[test]
    fn promotion_needs_a_holdout_mean_above_zero_and_a_lower_bound_above_zero_with_enough_trades() {
        let main: Vec<f64> = vec![-0.01; 25];
        let strong: Vec<f64> = (0..30).map(|i| if i % 5 == 0 { -0.02 } else { 0.04 }).collect();
        let (ok, why) = promotion_verdict(&main, &strong, 20, 4000);
        assert!(ok, "{why}");
        // the old 10 trade rule would have promoted this: 10 trades, 6 wins, better than main
        let luck = [0.04, -0.03, 0.05, -0.04, 0.03, -0.02, 0.06, 0.02, -0.05, -0.01];
        assert!(!promotion_verdict(&main, &luck, 10, 4000).0, "10 trades with a mean above 0 is not proof");
        // enough trades but the mean is below 0
        assert!(!promotion_verdict(&main, &[-0.01; 30], 20, 4000).0);
        // fewer trades than the minimum, however good
        let (ok, why) = promotion_verdict(&main, &[0.05; 12], 20, 4000);
        assert!(!ok && why.contains("Not enough holdout trades"), "{why}");
        // a configured minimum below the hard floor of 10 is raised to 10
        assert!(!promotion_verdict(&main, &[0.05; 9], 3, 4000).0);
        // better than 0 but worse than the main lane: not applied
        let strong_main: Vec<f64> = vec![0.10; 25];
        let (ok, why) = promotion_verdict(&strong_main, &strong, 20, 4000);
        assert!(!ok && why.contains("below the main lane"), "{why}");
    }

    // ---- the learner with a database

    fn learner(c: &Config) -> (Arc<Db>, Learner) {
        let db = Arc::new(Db::open(":memory:").unwrap());
        let l = Learner::new(db.clone(), None, c);
        (db, l)
    }

    fn put_trade(db: &Db, l: &Learner, id: i64, lane: &str, pnl: f64, curve: f64, start_ms: i64) -> Position {
        let mut p = closed_pos(id, lane, pnl, "hard-stop", 1.2, 1.1);
        p.entry_ms = start_ms + id;
        p.exit_ms = Some(start_ms + id + 50);
        p.id = db.insert_position(&p);
        db.save_position(&p);
        let mut f = feats(curve, 3.0, 9, 88.0);
        f.picked_ms = p.entry_ms - 1000;
        l.record_features(p.id, lane, &f);
        p
    }

    #[test]
    fn journal_entries_are_written_per_closed_trade_with_numbers() {
        let (db, l) = learner(&cfg());
        let p = put_trade(&db, &l, 1, MAIN_LANE, 0.012, 4.2, 0);
        l.on_close(&p);
        let s = l.snapshot(1);
        assert_eq!(s.journal.len(), 1);
        assert_eq!(s.journal[0].headline, "Trade #1: +0.012 SOL.");
        assert_eq!(s.journal[0].source_url, "https://example.org/coin");
        assert_eq!(s.next_review_in, 29);
        // an empty journal says so in the numbers: nothing is invented
        let (_db2, l2) = learner(&cfg());
        let e = l2.snapshot(1);
        assert!(e.journal.is_empty() && e.changes.is_empty() && e.last_review.is_none() && e.efficiency.fee_share_pct.is_none());
    }

    fn feed_trades(db: &Db, l: &Learner, n: u32, lane_pnl: impl Fn(u32) -> f64, curve: impl Fn(u32) -> f64, first_id: i64) -> Vec<Effect> {
        let mut fx = vec![];
        for i in 0..n {
            let p = put_trade(db, l, first_id + i as i64, MAIN_LANE, lane_pnl(i), curve(i), 0);
            fx.extend(l.on_close(&p));
        }
        fx
    }

    #[test]
    fn a_review_runs_every_30_trades_then_a_shadow_test_applies_or_drops_and_undo_restores() {
        let c = cfg();
        let (db, l) = learner(&c);
        // 30 trades: 12 at curve 4.2 (2 wins), 18 at curve 8 (13 wins)
        let fx = feed_trades(&db, &l, 30, |i| if i < 12 { if i < 2 { 0.01 } else { -0.012 } } else if i - 12 < 13 { 0.012 } else { -0.01 }, |i| if i < 12 { 4.2 } else { 8.0 }, 1);
        let s = l.snapshot(10);
        assert_eq!(s.closed_trades, 30);
        assert_eq!(s.next_review_in, 30);
        let ch = s.shadow.clone().expect("a shadow test started");
        assert_eq!((ch.key.as_str(), ch.status.as_str()), ("entry.curveSolMin", "shadow"));
        assert!((ch.after - 4.8).abs() < 1e-9 && !ch.evidence.is_empty());
        assert!(fx.iter().any(|e| matches!(e, Effect::SetShadow(Some(_)))));
        assert!(s.last_review.as_ref().unwrap().result.proposal.is_some());
        // main is still unchanged while the test runs
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.0).abs() < 1e-9);
        // the shadow lane's tuning has the candidate value
        assert!((l.shadow_tuned().unwrap().cfg.entry.curve_sol_min - 4.8).abs() < 1e-9);

        // next 30 main trades: the shadow lane (the holdout) did clearly better (24 trades, 20 wins)
        let start = ch.start_ms;
        for i in 0..24 {
            let mut p = closed_pos(500 + i, SHADOW_LANE, if i < 20 { 0.02 } else { -0.01 }, "take-profit-1", 1.3, 1.25);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 60 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
        }
        let mut last_fx = vec![];
        for i in 0..30 {
            let mut p = closed_pos(100 + i, MAIN_LANE, if i % 2 == 0 { 0.004 } else { -0.006 }, "hard-stop", 1.1, 0.9);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 70 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
            l.record_features(p.id, MAIN_LANE, &feats(8.0, 3.0, 9, 88.0));
            last_fx = l.on_close(&p);
        }
        let s = l.snapshot(20);
        let applied = s.changes.iter().find(|c| c.status == "applied").expect("applied");
        assert!(applied.result.contains("the lower end is above 0"), "{}", applied.result);
        assert!(s.shadow.is_none());
        assert!(last_fx.iter().any(|e| matches!(e, Effect::SetMain(_))));
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.8).abs() < 1e-9);
        // undo puts it back
        let (msg, fx) = l.command(&format!("undo-change:{}", applied.id));
        assert!(msg.starts_with("Undone"), "{msg}");
        assert!(fx.iter().any(|e| matches!(e, Effect::SetMain(t) if (t.cfg.entry.curve_sol_min - 4.0).abs() < 1e-9)));
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.0).abs() < 1e-9);
        assert_eq!(l.snapshot(30).changes.iter().find(|c| c.id == applied.id).unwrap().status, "undone");
        // a second undo does nothing
        assert!(l.command(&format!("undo-change:{}", applied.id)).0.contains("does nothing"));
    }

    #[test]
    fn a_losing_shadow_test_is_dropped_and_main_never_changes() {
        let (db, l) = learner(&cfg());
        feed_trades(&db, &l, 30, |i| if i < 12 { if i < 2 { 0.01 } else { -0.012 } } else if i - 12 < 13 { 0.012 } else { -0.01 }, |i| if i < 12 { 4.2 } else { 8.0 }, 1);
        let start = l.snapshot(1).shadow.unwrap().start_ms;
        for i in 0..22 {
            let mut p = closed_pos(600 + i, SHADOW_LANE, -0.02, "hard-stop", 1.0, 0.85);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 60 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
        }
        for i in 0..30 {
            let mut p = closed_pos(200 + i, MAIN_LANE, 0.01, "take-profit-1", 1.3, 1.2);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 70 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
            l.record_features(p.id, MAIN_LANE, &feats(8.0, 3.0, 9, 88.0));
            l.on_close(&p);
        }
        let s = l.snapshot(5);
        assert!(s.changes.iter().any(|c| c.status == "rejected" && c.result.contains("the mean is not above 0")));
        assert!(s.changes.iter().all(|c| c.status != "applied"));
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.0).abs() < 1e-9);
    }

    #[test]
    fn in_live_mode_a_winning_change_waits_for_approval() {
        let mut c = cfg();
        c.mode = Mode::Live;
        let (db, l) = learner(&c);
        feed_trades(&db, &l, 30, |i| if i < 12 { if i < 2 { 0.01 } else { -0.012 } } else if i - 12 < 13 { 0.012 } else { -0.01 }, |i| if i < 12 { 4.2 } else { 8.0 }, 1);
        let start = l.snapshot(1).shadow.unwrap().start_ms;
        for i in 0..24 {
            let mut p = closed_pos(700 + i, SHADOW_LANE, if i < 20 { 0.02 } else { -0.01 }, "take-profit-1", 1.3, 1.25);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 60 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
        }
        for i in 0..30 {
            let mut p = closed_pos(300 + i, MAIN_LANE, if i % 2 == 0 { 0.004 } else { -0.006 }, "hard-stop", 1.1, 0.9);
            p.entry_ms = start + 10 + i; p.exit_ms = Some(start + 70 + i);
            p.id = db.insert_position(&p); db.save_position(&p);
            l.record_features(p.id, MAIN_LANE, &feats(8.0, 3.0, 9, 88.0));
            l.on_close(&p);
        }
        let s = l.snapshot(5);
        let ch = s.changes.iter().find(|c| c.status == "awaiting-approval").expect("waiting");
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.0).abs() < 1e-9, "not applied without approval");
        let (msg, _) = l.command(&format!("approve-change:{}", ch.id));
        assert!(msg.starts_with("Approved"));
        assert!((l.current_main().cfg.entry.curve_sol_min - 4.8).abs() < 1e-9);
    }

    #[test]
    fn restore_puts_applied_changes_back_after_a_restart() {
        let (db, l) = learner(&cfg());
        let ch = ChangeRow { id: 0, ts: 1, key: "entry.minBuySellRatio".into(), label: "x".into(), before: 2.0, after: 2.4, status: "applied".into(), why: String::new(), evidence: vec![], result: String::new(), start_ms: 0, main_start: 0, target: 30, done_ms: 1 };
        l.save_change(&ch);
        let l2 = Learner::new(db, None, &cfg());
        match &l2.restore()[0] {
            Effect::SetMain(t) => assert!((t.cfg.entry.min_buy_sell_ratio - 2.4).abs() < 1e-9),
            _ => panic!(),
        }
    }

    // ---- missed coins and efficiency

    #[test]
    fn missed_coin_text_uses_the_price_data_or_says_there_is_none() {
        use crate::pump::Trade;
        let t = |ms: i64, v_sol: f64| Trade { mint: "m".into(), user: "u".into(), is_buy: true, sol: 1.0, tokens: 1.0, slot: ms as u64, signature: format!("s{ms}"), recv_ms: ms, v_sol, v_tokens: 1_000_000_000.0, real_sol: 10.0, ..Default::default() };
        let base_price = t(0, 40.0).price();
        let trades = vec![t(10_000, 44.0), t(200_000, 50.0), t(250_000, 46.0), t(400_000, 90.0)];
        let (end, peak, n) = missed_outcome(&trades, base_price, 0).unwrap();
        assert_eq!(n, 3, "the trade after 5 minutes is not counted");
        assert!((end - 15.0).abs() < 1e-9 && (peak - 25.0).abs() < 1e-9, "{end} {peak}");
        let txt = missed_text("HOMER", 99.0, "curve holds 5.2 SOL (needs 4-40)", Some((end, peak, n)));
        assert!(txt.contains("ended +15%") && txt.contains("peaked at +25%") && txt.contains("3 trades seen"), "{txt}");
        assert!(missed_outcome(&[], base_price, 0).is_none());
        assert!(missed_text("X", 81.0, "gate", None).contains("unknown"));
        assert!(!txt.contains('\u{2014}'));
    }

    #[test]
    fn efficiency_numbers_come_from_recorded_trades() {
        let (db, l) = learner(&cfg());
        for (i, pnl) in [(1, 0.02), (2, -0.01)] {
            let p = put_trade(&db, &l, i, MAIN_LANE, pnl, 6.0, 10_000_000);
            l.on_close(&p);
        }
        let e = l.snapshot(1).efficiency;
        assert_eq!(e.fills, 2);
        assert!((e.avg_pick_to_fill_s - 1.0).abs() < 0.01 || e.avg_pick_to_fill_s > 0.0);
        assert!(e.fee_share_pct.unwrap() > 0.0);
        l.add_note(5, "missed", "Missed ABC", "", "m");
        assert_eq!(l.snapshot(1).efficiency.missed, 1);
    }
}
