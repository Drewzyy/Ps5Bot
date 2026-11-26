//! Exit engine (Hold Timer), SPEC 6 `exits` and 11b #9-#11. Pure: it looks at one position, the market and the
//! clock and says what to sell. The 1 s loop in `trader.rs` calls it with fresh curve data.
//!
//! Percent moves are measured on the executable price: the average price the remaining tokens would fetch on the
//! curve right now (price impact included, fees excluded) against our average buy price. P/L itself is always
//! booked net of every fee. Ideas for the fade exits come from the DoradoDevs smart_exit.py named in the SPEC.

use crate::config::Config;
use crate::curve::GRADUATION_REAL_SOL;
use crate::position::Position;
use crate::pump::{Trade, SUPPLY_TOKENS};

#[derive(Debug, Clone, PartialEq)]
pub struct ExitAction {
    pub reason: String,
    /// Sell everything that is left.
    pub sell_all: bool,
    /// Otherwise: fraction (0..1) of the ORIGINAL tokens to sell now (tranche) or of the remaining ones for `fraction_of_remaining`.
    pub fraction: f64,
    pub of_remaining: bool,
    /// Tranche index that this action completes (take-profit ladder).
    pub tranche: Option<usize>,
    pub taper: bool,
}

impl ExitAction {
    fn all(reason: &str) -> Self {
        ExitAction { reason: reason.into(), sell_all: true, fraction: 1.0, of_remaining: true, tranche: None, taper: false }
    }
}

pub struct Market<'a> {
    pub now_ms: i64,
    /// Executable price of the remaining tokens (see module doc). Must be > 0 or the caller passes the last good one.
    pub price: f64,
    pub real_sol: f64,
    pub trades: &'a [Trade],
    /// The trade feed has been continuously connected since this time. Rules that read the ABSENCE of trades
    /// (momentum death, taper, slow rug) only run when the lookback is fully covered.
    pub feed_ok_since_ms: i64,
    pub migrated: bool,
}

fn buy_sol(trades: &[Trade], from: i64, to: i64) -> f64 {
    trades.iter().filter(|t| t.is_buy && t.recv_ms >= from && t.recv_ms < to).map(|t| t.sol).sum()
}

/// Decide the next exit, if any. Updates the position's peak price and soft-stop timer.
pub fn evaluate(pos: &mut Position, m: &Market, cfg: &Config) -> Option<ExitAction> {
    let ex = &cfg.exits;
    if !pos.note_price(m.price) && pos.last_price <= 0.0 {
        return None;
    }
    let price = if m.price > 0.0 && m.price.is_finite() { m.price } else { pos.last_price };
    let pct = pos.pct(price);
    let hold = pos.hold_seconds(m.now_ms);
    let since: Vec<&Trade> = m.trades.iter().filter(|t| t.recv_ms >= pos.entry_ms).collect();

    // 1. Hard limits that never wait: max hold, migration, graduation.
    if hold >= ex.max_hold_seconds as f64 {
        return Some(ExitAction::all("max-hold"));
    }
    if ex.sell_on_migration && m.migrated {
        return Some(ExitAction::all("migration"));
    }
    if m.real_sol >= 0.9 * GRADUATION_REAL_SOL {
        return Some(ExitAction::all("graduation-90pct"));
    }

    // 2. Insiders: any dev sell, a whale sell of >= whaleSellPctOfCurve of the curve, a cabal dump, a slow rug.
    if ex.exit_on_dev_sell && since.iter().any(|t| !t.is_buy && t.user == pos.creator) {
        return Some(ExitAction::all("dev-sell"));
    }
    if since.iter().any(|t| !t.is_buy && t.sol / (t.real_sol + t.sol).max(1e-9) * 100.0 >= ex.whale_sell_pct_of_curve) {
        return Some(ExitAction::all("whale-sell"));
    }
    let big_sells = m.trades.iter().filter(|t| !t.is_buy && t.recv_ms >= pos.entry_ms && m.now_ms - t.recv_ms <= 10_000 && t.tokens >= 0.03 * SUPPLY_TOKENS).count();
    if big_sells >= 3 {
        return Some(ExitAction::all("cabal-dump"));
    }
    if hold >= 60.0 && m.now_ms - m.feed_ok_since_ms >= 60_000 {
        let share = |from: i64, to: i64| -> Option<f64> {
            let w: Vec<&Trade> = m.trades.iter().filter(|t| t.recv_ms >= from && t.recv_ms < to).collect();
            if w.len() < 3 {
                return None;
            }
            let total: f64 = w.iter().map(|t| t.sol).sum();
            let sells: f64 = w.iter().filter(|t| !t.is_buy).map(|t| t.sol).sum();
            if total > 0.0 { Some(sells / total) } else { None }
        };
        let a = share(m.now_ms - 60_000, m.now_ms - 30_000);
        let b = share(m.now_ms - 30_000, m.now_ms + 1);
        if matches!((a, b), (Some(x), Some(y)) if x > 0.60 && y > 0.60) {
            return Some(ExitAction::all("slow-rug"));
        }
    }

    // 3. Two-layer stop. Hard stop sells at once. Soft stop starts a recovery timer; if the price is still under
    //    the soft level when the timer ends, sell.
    if pct <= ex.hard_stop_pct {
        return Some(ExitAction::all("hard-stop"));
    }
    if ex.sell_below_average_entry && pos.tranches_done > 0 && price < pos.entry_price {
        return Some(ExitAction::all("stop-at-entry"));
    }
    if pct <= ex.soft_stop_pct {
        let started = *pos.soft_since_ms.get_or_insert(m.now_ms);
        if m.now_ms - started >= ex.soft_stop_recovery_seconds as i64 * 1000 {
            return Some(ExitAction::all("soft-stop"));
        }
    } else {
        pos.soft_since_ms = None;
    }

    // 4. Take-profit ladder, then the trailing stop on the last piece.
    let done = pos.tranches_done as usize;
    if done < ex.tranches.len() {
        let t = &ex.tranches[done];
        if pct >= t.at_pct {
            return Some(ExitAction { reason: format!("take-profit-{}", done + 1), sell_all: false, fraction: t.sell_pct / 100.0, of_remaining: false, tranche: Some(done), taper: false });
        }
    } else if pos.peak_price > 0.0 && price <= pos.peak_price * (1.0 - ex.trailing_stop_pct / 100.0) {
        return Some(ExitAction::all("trailing-stop"));
    }

    // 5. Fade exits.
    let st = &ex.stall_exit;
    if pos.tranches_done == 0 && hold >= st.from_seconds as f64 && hold <= st.to_seconds as f64 && pct < st.min_pct {
        return Some(ExitAction::all("stall"));
    }
    let b = &ex.bleeder_exit;
    if hold > b.after_seconds as f64 && pct < b.below_pct && pos.pct(pos.peak_price) <= b.peak_never_above_pct {
        return Some(ExitAction::all("bleeder"));
    }
    let vt = &ex.volume_taper;
    let w = vt.window_seconds as i64 * 1000;
    let k = vt.declining_windows.max(2) as i64;
    if !pos.taper_done && hold >= (k * vt.window_seconds as i64) as f64 && m.now_ms - m.feed_ok_since_ms >= k * w {
        // windows oldest -> newest
        let vols: Vec<f64> = (0..k).map(|i| buy_sol(m.trades, m.now_ms - (k - i) * w, m.now_ms - (k - i - 1) * w + if i == k - 1 { 1 } else { 0 })).collect();
        let declining = vols.windows(2).all(|p| p[1] < p[0]);
        if declining && vols[0] > 0.0 && *vols.last().unwrap() < vols[0] * vt.last_below_baseline_pct / 100.0 {
            return Some(ExitAction { reason: "volume-taper".into(), sell_all: false, fraction: vt.sell_pct / 100.0, of_remaining: true, tranche: None, taper: true });
        }
    }
    let md = &ex.momentum_death;
    let look = md.lookback_seconds as i64 * 1000;
    if hold >= md.after_hold_seconds as f64 && m.now_ms - m.feed_ok_since_ms >= look {
        let buys = m.trades.iter().filter(|t| t.is_buy && m.now_ms - t.recv_ms <= look && m.now_ms >= t.recv_ms).count() as f64;
        let per_min = buys * 60_000.0 / look as f64;
        if per_min < md.min_buys_per_minute {
            return Some(ExitAction::all("momentum-death"));
        }
    }
    None
}

/// Price levels for the chart and the Open Positions row.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
    pub soft_stop: f64,
    pub hard_stop: f64,
    pub take_profits: Vec<f64>,
}

pub fn levels(pos: &Position, cfg: &Config) -> Levels {
    let e = pos.entry_price;
    Levels {
        soft_stop: e * (1.0 + cfg.exits.soft_stop_pct / 100.0),
        hard_stop: e * (1.0 + cfg.exits.hard_stop_pct / 100.0),
        take_profits: cfg.exits.tranches.iter().map(|t| e * (1.0 + t.at_pct / 100.0)).collect(),
    }
}

/// Plain-language "what happens next" for the Open Positions widget.
pub fn next_action(pos: &Position, price: f64, now_ms: i64, cfg: &Config) -> String {
    let ex = &cfg.exits;
    let pct = pos.pct(price);
    let hold = pos.hold_seconds(now_ms);
    let left = (ex.max_hold_seconds as f64 - hold).max(0.0);
    if let Some(s) = pos.soft_since_ms {
        let wait = (ex.soft_stop_recovery_seconds as f64 - (now_ms - s) as f64 / 1000.0).max(0.0);
        return format!("Under the soft stop: sells in {wait:.0} s unless it recovers above {:+.0}%", ex.soft_stop_pct);
    }
    let done = pos.tranches_done as usize;
    if done < ex.tranches.len() {
        let t = &ex.tranches[done];
        format!("Sell {:.0}% at {:+.0}% (now {pct:+.1}%). Hard stop {:+.0}%. Max hold in {left:.0} s", t.sell_pct, t.at_pct, ex.hard_stop_pct)
    } else {
        format!("Trailing stop: sells the last piece if the price drops {:.0}% from its peak. Max hold in {left:.0} s", ex.trailing_stop_pct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::PosState;

    pub fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    const T0: i64 = 1_000_000;

    pub fn pos() -> Position {
        let mut p = Position::new_buying("momentum", "MINT", "TST", "CREATOR", "CURVE", 0.1, T0, 150.0);
        p.fill_buy(1.0e-7, 1_000_000.0, 7000.0, &Default::default(), 0.0).unwrap();
        p.entry_ms = T0;
        p
    }

    fn tr(user: &str, buy: bool, ms: i64, sol: f64, real: f64) -> Trade {
        Trade { mint: "MINT".into(), user: user.into(), is_buy: buy, sol, tokens: sol * 1.0e7, slot: (ms / 400) as u64, signature: format!("{ms}{user}{buy}"), recv_ms: ms, v_sol: 40.0, v_tokens: 7.0e8, real_sol: real }
    }

    fn run(p: &mut Position, secs: f64, pct: f64, trades: &[Trade]) -> Option<ExitAction> {
        let m = Market { now_ms: T0 + (secs * 1000.0) as i64, price: p.entry_price * (1.0 + pct / 100.0), real_sol: 20.0, trades, feed_ok_since_ms: 0, migrated: false };
        evaluate(p, &m, &cfg())
    }

    /// A steady stream of buys so the fade exits stay quiet: one buy every 2 s from T0 on.
    fn busy(until_s: i64) -> Vec<Trade> {
        (0..until_s / 2).map(|i| tr(&format!("w{i}"), true, T0 + i * 2000, 0.5, 20.0)).collect()
    }

    #[test]
    fn hard_stop_sells_at_once() {
        let mut p = pos();
        let tr = busy(30);
        assert_eq!(run(&mut p, 5.0, -14.0, &tr), None);
        let a = run(&mut p, 6.0, -15.0, &tr).unwrap();
        assert_eq!(a.reason, "hard-stop");
        assert!(a.sell_all);
    }

    #[test]
    fn soft_stop_waits_15_seconds_then_sells_and_recovery_resets_timer() {
        let mut p = pos();
        let tr = busy(60);
        assert_eq!(run(&mut p, 10.0, -11.0, &tr), None); // timer starts
        assert_eq!(p.soft_since_ms, Some(T0 + 10_000));
        assert_eq!(run(&mut p, 24.9, -12.0, &tr), None); // 14.9 s: still waiting
        let a = run(&mut p, 25.0, -12.0, &tr).unwrap(); // 15 s
        assert_eq!(a.reason, "soft-stop");
        // recovery above -10 before 15 s resets the timer
        let mut q = pos();
        assert_eq!(run(&mut q, 10.0, -11.0, &tr), None);
        assert_eq!(run(&mut q, 18.0, -5.0, &tr), None);
        assert_eq!(q.soft_since_ms, None);
        assert_eq!(run(&mut q, 20.0, -11.0, &tr), None);
        assert_eq!(run(&mut q, 34.0, -11.0, &tr), None); // only 14 s since the new start
        assert_eq!(run(&mut q, 35.0, -11.0, &tr).unwrap().reason, "soft-stop");
    }

    #[test]
    fn hard_stop_beats_soft_timer() {
        let mut p = pos();
        let tr = busy(60);
        assert_eq!(run(&mut p, 10.0, -11.0, &tr), None);
        assert_eq!(run(&mut p, 11.0, -16.0, &tr).unwrap().reason, "hard-stop");
    }

    #[test]
    fn take_profit_ladder_40_30_20_then_trailing_10() {
        let mut p = pos();
        let tr = busy(200);
        assert_eq!(run(&mut p, 10.0, 19.0, &tr), None);
        let a = run(&mut p, 11.0, 20.0, &tr).unwrap();
        assert_eq!((a.reason.as_str(), a.fraction, a.of_remaining, a.tranche), ("take-profit-1", 0.40, false, Some(0)));
        p.tranches_done = 1;
        assert_eq!(run(&mut p, 12.0, 39.0, &tr), None);
        let a = run(&mut p, 13.0, 40.0, &tr).unwrap();
        assert_eq!((a.reason.as_str(), a.fraction), ("take-profit-2", 0.30));
        p.tranches_done = 2;
        let a = run(&mut p, 14.0, 61.0, &tr).unwrap();
        assert_eq!((a.reason.as_str(), a.fraction), ("take-profit-3", 0.20));
        p.tranches_done = 3;
        // last 10%: trailing stop 10% from the peak
        assert_eq!(run(&mut p, 15.0, 80.0, &tr), None); // new peak at +80%
        assert_eq!(run(&mut p, 16.0, 73.0, &tr), None); // -8.9% from the peak
        let a = run(&mut p, 17.0, 70.0, &tr).unwrap(); // 1.8/1.7 -> -10.5% from the peak
        assert_eq!(a.reason, "trailing-stop");
        assert!(a.sell_all);
        // the ladder sums to 90%, leaving 10% for the trailing stop
        let sum: f64 = cfg().exits.tranches.iter().map(|t| t.sell_pct).sum();
        assert_eq!(sum, 90.0);
    }

    #[test]
    fn max_hold_300_seconds_sells_everything() {
        let mut p = pos();
        let tr = busy(400);
        assert_eq!(run(&mut p, 299.0, 5.0, &tr), None);
        let a = run(&mut p, 300.0, 5.0, &tr).unwrap();
        assert_eq!(a.reason, "max-hold");
        assert!(a.sell_all);
        // even a position that is up 15% is sold at 300 s (no exceptions)
        let mut q = pos();
        assert_eq!(run(&mut q, 301.0, 15.0, &tr).unwrap().reason, "max-hold");
    }

    #[test]
    fn stall_exit_between_60_and_90_seconds_under_3_pct() {
        let tr = busy(200);
        let mut p = pos();
        assert_eq!(run(&mut p, 59.0, 2.0, &tr), None);
        assert_eq!(run(&mut p, 60.0, 2.0, &tr).unwrap().reason, "stall");
        let mut q = pos();
        assert_eq!(run(&mut q, 70.0, 3.5, &tr), None);
        let mut r = pos();
        assert_eq!(run(&mut r, 91.0, 2.0, &tr), None); // window over
    }

    #[test]
    fn bleeder_exit_after_120_s_below_minus_20_with_peak_never_above_20() {
        // hard stop would normally fire first, so run with the stops far away
        let mut c = cfg();
        c.exits.hard_stop_pct = -50.0;
        c.exits.soft_stop_pct = -45.0;
        let tr = busy(200);
        let mut p = pos();
        let m = |secs: f64, pct: f64| Market { now_ms: T0 + (secs * 1000.0) as i64, price: 1.0e-7 * (1.0 + pct / 100.0), real_sol: 20.0, trades: &tr, feed_ok_since_ms: 0, migrated: false };
        assert_eq!(evaluate(&mut p, &m(100.0, -25.0), &c), None);
        assert_eq!(evaluate(&mut p, &m(121.0, -19.0), &c), None);
        assert_eq!(evaluate(&mut p, &m(122.0, -21.0), &c).unwrap().reason, "bleeder");
        // if the peak was ever above +20% the rule does not fire
        let mut q = pos();
        evaluate(&mut q, &m(30.0, 25.0), &c);
        assert!(q.pct(q.peak_price) > 20.0);
        let r = evaluate(&mut q, &m(130.0, -21.0), &c);
        assert_ne!(r.map(|a| a.reason), Some("bleeder".to_string()));
    }

    #[test]
    fn volume_taper_sells_half_after_three_falling_windows() {
        // buy volume per 10 s window: 3.0, 2.0, 1.0 (last is 33% of baseline, under 50%)
        let now = T0 + 100_000;
        let mut tr = vec![];
        for (w, vol) in [(3, 3.0), (2, 2.0), (1, 1.0)] {
            for i in 0..3 {
                tr.push(tr_at(now - w * 10_000 + i * 2_000 + 500, vol / 3.0));
            }
        }
        // keep momentum-death quiet: plenty of buys inside 45 s
        let mut p = pos();
        let m = Market { now_ms: now, price: 1.05e-7, real_sol: 20.0, trades: &tr, feed_ok_since_ms: 0, migrated: false };
        let a = evaluate(&mut p, &m, &cfg()).unwrap();
        assert_eq!(a.reason, "volume-taper");
        assert!(a.taper && !a.sell_all && a.of_remaining);
        assert_eq!(a.fraction, 0.5);
        // not falling enough: 3.0, 2.0, 1.8 (60% of baseline)
        let mut tr2 = vec![];
        for (w, vol) in [(3, 3.0), (2, 2.0), (1, 1.8)] {
            for i in 0..3 {
                tr2.push(tr_at(now - w * 10_000 + i * 2_000 + 500, vol / 3.0));
            }
        }
        let mut q = pos();
        let m2 = Market { now_ms: now, price: 1.05e-7, real_sol: 20.0, trades: &tr2, feed_ok_since_ms: 0, migrated: false };
        assert_eq!(evaluate(&mut q, &m2, &cfg()), None);
        // once done it does not repeat
        let mut r = pos();
        r.taper_done = true;
        assert_eq!(evaluate(&mut r, &m, &cfg()), None);
    }

    fn tr_at(ms: i64, sol: f64) -> Trade {
        tr("buyer", true, ms, sol, 20.0)
    }

    #[test]
    fn momentum_death_after_90_s_with_under_half_a_buy_per_minute() {
        let mut p = pos();
        // one buy at 30 s only: nothing in the last 45 s of a 100 s hold
        let tr = vec![tr_at(T0 + 30_000, 0.5)];
        assert_eq!(run(&mut p, 80.0, 10.0, &tr), None); // before 90 s
        let a = run(&mut p, 100.0, 10.0, &tr).unwrap();
        assert_eq!(a.reason, "momentum-death");
        // a buy inside the last 45 s keeps it alive (1 buy/45 s = 1.33 per minute)
        let tr2 = vec![tr_at(T0 + 70_000, 0.5)];
        let mut q = pos();
        assert_eq!(run(&mut q, 100.0, 10.0, &tr2), None);
        // a feed gap (not connected for the whole lookback) suppresses the rule
        let mut r = pos();
        let m = Market { now_ms: T0 + 100_000, price: 1.1e-7, real_sol: 20.0, trades: &tr, feed_ok_since_ms: T0 + 80_000, migrated: false };
        assert_eq!(evaluate(&mut r, &m, &cfg()), None);
    }

    #[test]
    fn dev_sell_whale_sell_cabal_and_slow_rug_exit_at_once() {
        let tr_ok = busy(120);
        // any dev sell
        let mut p = pos();
        let mut t = tr_ok.clone();
        t.push(tr("CREATOR", false, T0 + 20_000, 0.01, 20.0));
        assert_eq!(run(&mut p, 21.0, 5.0, &t).unwrap().reason, "dev-sell");
        // a dev sell before entry is ignored
        let mut q = pos();
        let mut t2 = tr_ok.clone();
        t2.push(tr("CREATOR", false, T0 - 5_000, 0.5, 20.0));
        assert_eq!(run(&mut q, 21.0, 5.0, &t2), None);
        // whale: sells 3% of the curve (0.6 SOL of 20.6)
        let mut w = pos();
        let mut t3 = tr_ok.clone();
        t3.push(tr("whale", false, T0 + 20_000, 0.7, 20.0));
        assert_eq!(run(&mut w, 21.0, 5.0, &t3).unwrap().reason, "whale-sell");
        let mut w2 = pos();
        let mut t4 = tr_ok.clone();
        t4.push(tr("small", false, T0 + 20_000, 0.3, 20.0)); // 1.5%
        assert_eq!(run(&mut w2, 21.0, 5.0, &t4), None);
        // cabal: three sells each over 3% of supply (30M tokens) inside 10 s
        let mut c = pos();
        let mut t5 = busy(60);
        for i in 0..3 {
            let mut s = tr(&format!("cab{i}"), false, T0 + 20_000 + i * 1000, 0.1, 20.0);
            s.tokens = 31_000_000.0;
            t5.push(s);
        }
        assert_eq!(run(&mut c, 25.0, 5.0, &t5).unwrap().reason, "cabal-dump");
        // slow rug: sells dominate two consecutive 30 s windows
        let mut s = pos();
        let mut t6 = vec![];
        for i in 0..8 {
            t6.push(tr(&format!("a{i}"), false, T0 + 5_000 + i * 3_000, 0.2, 20.0));
            t6.push(tr(&format!("b{i}"), false, T0 + 30_000 + i * 3_000, 0.2, 20.0));
        }
        t6.push(tr("buyer", true, T0 + 40_000, 0.1, 20.0));
        t6.push(tr("buyer2", true, T0 + 60_000, 0.1, 20.0));
        assert_eq!(run(&mut s, 64.0, 2.0, &t6).map(|a| a.reason), Some("slow-rug".to_string()));
    }

    #[test]
    fn graduation_migration_and_stop_at_entry() {
        let tr = busy(100);
        let mut p = pos();
        let m = Market { now_ms: T0 + 10_000, price: 1.2e-7, real_sol: 76.6, trades: &tr, feed_ok_since_ms: 0, migrated: false };
        assert_eq!(evaluate(&mut p, &m, &cfg()).unwrap().reason, "graduation-90pct");
        let mut q = pos();
        let m = Market { now_ms: T0 + 10_000, price: 1.0e-7, real_sol: 20.0, trades: &tr, feed_ok_since_ms: 0, migrated: true };
        assert_eq!(evaluate(&mut q, &m, &cfg()).unwrap().reason, "migration");
        // after TP1, falling under the entry price sells the rest
        let mut r = pos();
        r.tranches_done = 1;
        assert_eq!(run(&mut r, 20.0, -1.0, &tr).unwrap().reason, "stop-at-entry");
    }

    #[test]
    fn zero_price_never_overwrites_last_price() {
        let mut p = pos();
        let tr = busy(20);
        run(&mut p, 5.0, 5.0, &tr);
        let good = p.last_price;
        assert!(good > 0.0);
        let m = Market { now_ms: T0 + 6_000, price: 0.0, real_sol: 20.0, trades: &tr, feed_ok_since_ms: 0, migrated: false };
        let _ = evaluate(&mut p, &m, &cfg());
        assert_eq!(p.last_price, good);
        assert!(!p.note_price(0.0) && !p.note_price(f64::NAN));
        assert_eq!(p.state, PosState::Open);
    }

    #[test]
    fn next_action_text_and_levels() {
        let p = pos();
        let l = levels(&p, &cfg());
        assert!((l.hard_stop - 0.85e-7).abs() < 1e-15 && (l.take_profits[0] - 1.2e-7).abs() < 1e-15);
        let s = next_action(&p, 1.05e-7, T0 + 10_000, &cfg());
        assert!(s.contains("Sell 40% at +20%"), "{s}");
    }
}
