//! Risk gates (SPEC 6 `risk`, 11b #13/#14/#25/#27). Pure state machine: every method takes the clock.
//! Halts (daily loss, consecutive losses) are sticky: they need a manual restart (`engine-cli feed --clear-halt`).

use crate::config::Risk;
use crate::curve::{break_even_pct, FeeModel};
use std::collections::{HashMap, VecDeque};

pub const MAX_TRADES_PER_DAY: u32 = 30;
/// Smallest size after the "halve after 3 wins" rule (the SPEC 6 floor; the normal minimum is `minTradeSol`).
pub const HALVED_FLOOR_SOL: f64 = 0.05;
/// TP1 must beat break-even by at least this many percentage points.
pub const BREAK_EVEN_MARGIN_PCT: f64 = 5.0;
pub const REGIME_WINDOW: usize = 20;
pub const REGIME_MIN_PF: f64 = 0.8;
pub const REGIME_PAUSE_MS: i64 = 30 * 60_000;
pub const SLIPPAGE_WINDOW: usize = 10;
pub const SLIPPAGE_PAUSE_PCT: f64 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub enum GateBlock {
    Halted(String),
    AlreadyOpen,
    MaxOpen,
    Exposure,
    Cooldown(i64),
    TradesPerDay,
    RegimePause,
    SlippagePause,
    BreakEven(f64, f64),
    FeeGate(f64, f64),
}

impl GateBlock {
    pub fn code(&self) -> &'static str {
        match self {
            GateBlock::Halted(_) => "HALTED",
            GateBlock::AlreadyOpen => "ALREADY_OPEN",
            GateBlock::MaxOpen => "MAX_OPEN",
            GateBlock::Exposure => "EXPOSURE",
            GateBlock::Cooldown(_) => "COOLDOWN",
            GateBlock::TradesPerDay => "TRADES_PER_DAY",
            GateBlock::RegimePause => "REGIME_PAUSE",
            GateBlock::SlippagePause => "SLIPPAGE_PAUSE",
            GateBlock::BreakEven(..) => "BREAK_EVEN",
            GateBlock::FeeGate(..) => "FEE_GATE",
        }
    }
    pub fn detail(&self) -> String {
        match self {
            GateBlock::Halted(w) => format!("Trading is halted: {w}. Needs a manual restart."),
            GateBlock::AlreadyOpen => "A position in this coin is already open.".into(),
            GateBlock::MaxOpen => "The maximum number of open positions is reached.".into(),
            GateBlock::Exposure => "Total exposure would go over the limit.".into(),
            GateBlock::Cooldown(s) => format!("This coin was traded {s} s ago; cooldown still running."),
            GateBlock::TradesPerDay => format!("Daily limit of {MAX_TRADES_PER_DAY} trades reached."),
            GateBlock::RegimePause => "Paused: the last 20 trades have a profit factor under 0.8.".into(),
            GateBlock::SlippagePause => "Paused: average buy slippage over the last 10 trades is above 6%.".into(),
            GateBlock::BreakEven(be, tp) => format!("Break-even is {be:.1}% but the first take-profit is only {tp:.0}% (needs {BREAK_EVEN_MARGIN_PCT:.0} points of margin)."),
            GateBlock::FeeGate(f, lim) => format!("Priority fee + tip is {f:.3} SOL, over 1% of the trade size ({lim:.4} SOL)."),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskState {
    cfg: Risk,
    pub bankroll_start: f64,
    pub realized_total: f64,
    pub day: i64,
    pub day_start_equity: f64,
    pub day_pnl: f64,
    pub trades_today: u32,
    pub consec_losses: u32,
    pub consec_wins: u32,
    last_size: Option<f64>,
    last_was_loss: bool,
    pub halt: Option<String>,
    cooldowns: HashMap<String, i64>,
    pub open: Vec<(String, f64)>,
    pnls: VecDeque<f64>,
    regime_pause_until: i64,
    slippages: VecDeque<f64>,
}

impl RiskState {
    pub fn new(cfg: &Risk, bankroll: f64, now_ms: i64) -> Self {
        RiskState {
            cfg: cfg.clone(),
            bankroll_start: bankroll,
            realized_total: 0.0,
            day: day_of(now_ms),
            day_start_equity: bankroll,
            day_pnl: 0.0,
            trades_today: 0,
            consec_losses: 0,
            consec_wins: 0,
            last_size: None,
            last_was_loss: false,
            halt: None,
            cooldowns: HashMap::new(),
            open: vec![],
            pnls: VecDeque::new(),
            regime_pause_until: 0,
            slippages: VecDeque::new(),
        }
    }

    pub fn equity(&self) -> f64 {
        self.bankroll_start + self.realized_total
    }

    fn roll_day(&mut self, now_ms: i64) {
        let d = day_of(now_ms);
        if d != self.day {
            self.day = d;
            self.day_start_equity = self.equity();
            self.day_pnl = 0.0;
            self.trades_today = 0;
        }
    }

    /// Next trade size: 1% of equity clamped to the min/max; halved after N wins in a row; never above the
    /// previous size after a loss (SPEC 11b #26).
    pub fn size(&self) -> f64 {
        let mut s = (self.equity() * self.cfg.max_pct_bankroll_per_trade / 100.0).clamp(self.cfg.min_trade_sol, self.cfg.max_trade_sol);
        if self.cfg.reduce_size_after_wins > 0 && self.consec_wins >= self.cfg.reduce_size_after_wins {
            s = (s / 2.0).max(HALVED_FLOOR_SOL.min(s));
        }
        if self.last_was_loss {
            if let Some(l) = self.last_size {
                s = s.min(l);
            }
        }
        (s * 1e9).round() / 1e9
    }

    /// All gates for a new entry. `tp1_pct` is the first take-profit level, `impact_pct` the round-trip price impact.
    pub fn check_entry(&mut self, now_ms: i64, mint: &str, size: f64, fees: &FeeModel, tp1_pct: f64, impact_pct: f64) -> Result<(), GateBlock> {
        self.roll_day(now_ms);
        if let Some(h) = &self.halt {
            return Err(GateBlock::Halted(h.clone()));
        }
        if self.open.iter().any(|(m, _)| m == mint) {
            return Err(GateBlock::AlreadyOpen);
        }
        if let Some(t) = self.cooldowns.get(mint) {
            let ago = (now_ms - t) / 1000;
            if ago < self.cfg.cooldown_seconds_per_token as i64 {
                return Err(GateBlock::Cooldown(ago));
            }
        }
        if self.open.len() as u32 >= self.cfg.max_open_positions {
            return Err(GateBlock::MaxOpen);
        }
        let exposure: f64 = self.open.iter().map(|(_, s)| s).sum::<f64>() + size;
        if exposure > self.equity() * self.cfg.max_total_exposure_pct / 100.0 + 1e-9 {
            return Err(GateBlock::Exposure);
        }
        if self.trades_today >= MAX_TRADES_PER_DAY {
            return Err(GateBlock::TradesPerDay);
        }
        if now_ms < self.regime_pause_until {
            return Err(GateBlock::RegimePause);
        }
        if self.slippages.len() >= SLIPPAGE_WINDOW && self.slippages.iter().sum::<f64>() / self.slippages.len() as f64 > SLIPPAGE_PAUSE_PCT {
            return Err(GateBlock::SlippagePause);
        }
        let fee_sol = fees.priority_sol; // no Jito tip in paper
        if fee_sol > size * 0.01 + 1e-12 {
            return Err(GateBlock::FeeGate(fee_sol, size * 0.01));
        }
        let be = break_even_pct(size, fees, impact_pct);
        if tp1_pct < be + BREAK_EVEN_MARGIN_PCT {
            return Err(GateBlock::BreakEven(be, tp1_pct));
        }
        Ok(())
    }

    pub fn record_open(&mut self, now_ms: i64, mint: &str, size: f64, slippage_pct: f64) {
        self.roll_day(now_ms);
        self.open.push((mint.to_string(), size));
        self.trades_today += 1;
        self.slippages.push_back(slippage_pct);
        while self.slippages.len() > SLIPPAGE_WINDOW {
            self.slippages.pop_front();
        }
    }

    /// An entry that was reserved but never filled.
    pub fn cancel_open(&mut self, mint: &str) {
        if let Some(i) = self.open.iter().position(|(m, _)| m == mint) {
            self.open.remove(i);
            self.trades_today = self.trades_today.saturating_sub(1);
            self.slippages.pop_back();
        }
    }

    /// Book a closed trade. `derive_halts` is false when rebuilding from history (halts are stored separately).
    pub fn record_close(&mut self, now_ms: i64, mint: &str, size: f64, pnl: f64, derive_halts: bool) {
        self.roll_day(now_ms);
        self.open.retain(|(m, _)| m != mint);
        self.cooldowns.insert(mint.to_string(), now_ms);
        self.realized_total += pnl;
        self.day_pnl += pnl;
        self.last_size = Some(size);
        self.last_was_loss = pnl < 0.0;
        if pnl < 0.0 {
            self.consec_losses += 1;
            self.consec_wins = 0;
        } else {
            self.consec_losses = 0;
            self.consec_wins += 1;
        }
        if self.cfg.reduce_size_after_wins > 0 && self.consec_wins > self.cfg.reduce_size_after_wins {
            // the halved trade has been made: start counting again
            self.consec_wins = 1;
        }
        self.pnls.push_back(pnl);
        while self.pnls.len() > REGIME_WINDOW {
            self.pnls.pop_front();
        }
        if derive_halts {
            if self.day_pnl <= -(self.day_start_equity * self.cfg.daily_loss_limit_pct / 100.0) && self.halt.is_none() {
                self.halt = Some(format!("daily loss limit of {}% reached", self.cfg.daily_loss_limit_pct));
            }
            if self.consec_losses >= self.cfg.consecutive_loss_stop && self.halt.is_none() {
                self.halt = Some(format!("{} losing trades in a row", self.consec_losses));
            }
            if self.pnls.len() >= REGIME_WINDOW {
                let w: f64 = self.pnls.iter().filter(|p| **p > 0.0).sum();
                let l: f64 = -self.pnls.iter().filter(|p| **p < 0.0).sum::<f64>();
                if l > 0.0 && w / l < REGIME_MIN_PF {
                    self.regime_pause_until = now_ms + REGIME_PAUSE_MS;
                    self.pnls.clear(); // judge the next 20 trades fresh when the pause ends
                }
            }
        }
    }

    /// Fees of an attempt that never became a trade (a failed buy) still cost money.
    pub fn add_cost(&mut self, now_ms: i64, cost: f64) {
        self.roll_day(now_ms);
        self.realized_total -= cost;
        self.day_pnl -= cost;
    }

    /// Replace the slippage recorded for the newest entry once the real fill is known.
    pub fn set_last_slippage(&mut self, slippage_pct: f64) {
        if let Some(l) = self.slippages.back_mut() {
            *l = slippage_pct;
        }
    }

    pub fn clear_halt(&mut self) {
        self.halt = None;
        self.consec_losses = 0;
        self.day_start_equity = self.equity();
        self.day_pnl = 0.0;
    }

    pub fn regime_paused(&self, now_ms: i64) -> bool {
        now_ms < self.regime_pause_until
    }
    pub fn exposure(&self) -> f64 {
        self.open.iter().map(|(_, s)| s).sum()
    }
}

pub fn day_of(ms: i64) -> i64 {
    (ms + crate::timefmt::offset_ms()).div_euclid(86_400_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: i64 = 1_800_000_000_000;

    fn cfgs() -> (crate::Config, Risk) {
        let c = crate::Config::from_json(include_str!("../../config.json")).unwrap();
        let r = c.risk.clone();
        (c, r)
    }

    fn st() -> (RiskState, FeeModel) {
        let (c, r) = cfgs();
        (RiskState::new(&r, 10.0, T), FeeModel::from_config(&c))
    }

    #[test]
    fn size_is_one_percent_clamped() {
        let (c, r) = cfgs();
        assert_eq!(RiskState::new(&r, 10.0, T).size(), 0.1);
        assert_eq!(RiskState::new(&r, 5.0, T).size(), 0.1); // 0.05 clamps up to the 0.1 minimum
        assert_eq!(RiskState::new(&r, 100.0, T).size(), 0.25); // 1.0 clamps down
        assert_eq!(RiskState::new(&r, 20.0, T).size(), 0.2);
        let _ = c;
    }

    #[test]
    fn break_even_gate_rejects_when_tp1_does_not_clear_costs() {
        let (mut s, f) = st();
        assert!(s.check_entry(T, "m", 0.1, &f, 20.0, 0.6).is_ok());
        // a 7% first take-profit does not clear ~7% break-even plus margin
        let e = s.check_entry(T, "m", 0.1, &f, 7.0, 0.6).unwrap_err();
        assert_eq!(e.code(), "BREAK_EVEN");
        // a tiny size has far higher fixed-cost drag: 0.02 SOL needs > 20% to break even
        let e = s.check_entry(T, "m", 0.02, &f, 20.0, 0.6).unwrap_err();
        assert!(matches!(e.code(), "BREAK_EVEN" | "FEE_GATE"), "{e:?}");
        let be = break_even_pct(0.1, &f, 0.6);
        assert!(be > 6.0 && be < 9.0, "{be}");
    }

    #[test]
    fn fee_gate_rejects_priority_over_one_percent_of_size() {
        let (mut s, mut f) = st();
        f.priority_sol = 0.0011; // 1.1% of 0.1
        assert_eq!(s.check_entry(T, "m", 0.1, &f, 40.0, 0.5).unwrap_err().code(), "FEE_GATE");
        f.priority_sol = 0.001; // exactly 1%
        assert!(s.check_entry(T, "m", 0.1, &f, 40.0, 0.5).is_ok());
    }

    #[test]
    fn daily_loss_halt_at_7_5_percent_is_sticky_until_cleared() {
        let (mut s, f) = st();
        // 10 SOL bankroll: limit is -0.75 SOL. Alternate wins so the 3-loss stop does not fire first.
        for (i, pnl) in [-0.3, 0.05, -0.3, 0.05, -0.3].iter().enumerate() {
            s.record_open(T + i as i64, &format!("m{i}"), 0.1, 1.0);
            s.record_close(T + i as i64 + 1000, &format!("m{i}"), 0.1, *pnl, true);
            if i < 4 {
                assert!(s.halt.is_none(), "step {i}: {:?}", s.halt);
            }
        }
        assert!(s.halt.as_deref().unwrap().contains("daily loss limit"), "{:?}", s.halt);
        assert_eq!(s.check_entry(T + 999_999, "new", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "HALTED");
        // still halted the next day: it needs a manual restart
        assert_eq!(s.check_entry(T + 2 * 86_400_000, "new", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "HALTED");
        s.clear_halt();
        assert!(s.check_entry(T + 2 * 86_400_000, "new", 0.1, &f, 20.0, 0.5).is_ok());
    }

    #[test]
    fn three_losses_in_a_row_halt_and_a_win_resets_the_count() {
        let (mut s, f) = st();
        for i in 0..2 {
            s.record_close(T + i, &format!("m{i}"), 0.1, -0.01, true);
        }
        assert!(s.halt.is_none());
        s.record_close(T + 5, "w", 0.1, 0.02, true);
        assert_eq!(s.consec_losses, 0);
        for i in 0..3 {
            s.record_close(T + 10 + i, &format!("n{i}"), 0.1, -0.01, true);
        }
        assert!(s.halt.as_deref().unwrap().contains("3 losing trades in a row"));
        assert_eq!(s.check_entry(T + 100, "x", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "HALTED");
    }

    #[test]
    fn size_never_increases_after_a_loss_and_halves_after_three_wins() {
        let (_, r) = cfgs();
        let mut s = RiskState::new(&r, 20.0, T); // 1% = 0.2
        assert_eq!(s.size(), 0.2);
        s.record_close(T, "a", 0.2, -0.05, true);
        // the bankroll jumps (deposit / config change) but the size must not go above the last one
        s.bankroll_start = 100.0;
        assert_eq!(s.size(), 0.2, "capped at the previous size");
        s.record_close(T + 1, "b", 0.2, 0.05, true); // a win lifts the cap
        assert_eq!(s.size(), 0.25);
        // three wins in a row: halve for the next trade only
        let mut w = RiskState::new(&r, 20.0, T);
        for i in 0..3 {
            w.record_close(T + i, &format!("w{i}"), 0.2, 0.01, true);
        }
        assert!((w.size() - 0.1).abs() < 0.001, "{}", w.size());
        w.record_close(T + 9, "w9", 0.1, 0.01, true);
        assert!((w.size() - 0.2).abs() < 0.002, "back to normal after the halved trade");
        // property: after any loss the next size is never larger
        let mut p = RiskState::new(&r, 20.0, T);
        let mut rng = crate::curve::Rng::new(3);
        for i in 0..300 {
            let before = p.size();
            let pnl = if rng.f64() < 0.5 { -0.01 } else { 0.02 };
            p.record_close(T + i, &format!("p{i}"), before, pnl, false);
            if pnl < 0.0 {
                assert!(p.size() <= before + 1e-12, "size grew after a loss");
            }
        }
    }

    #[test]
    fn open_exposure_cooldown_and_daily_trade_limits() {
        let (mut s, f) = st();
        for i in 0..5 {
            assert!(s.check_entry(T, &format!("m{i}"), 0.1, &f, 20.0, 0.5).is_ok());
            s.record_open(T, &format!("m{i}"), 0.1, 1.0);
        }
        // 5 open at 0.1 = 0.5 SOL = exactly 5% of 10 SOL: the 6th hits max-open first
        assert_eq!(s.check_entry(T, "m6", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "MAX_OPEN");
        let (_, r) = cfgs();
        let mut e = RiskState::new(&r, 10.0, T);
        e.record_open(T, "a", 0.25, 1.0);
        e.record_open(T, "b", 0.25, 1.0);
        assert_eq!(e.check_entry(T, "c", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "EXPOSURE");
        // cooldown
        let mut c = RiskState::new(&r, 10.0, T);
        c.record_close(T, "z", 0.1, 0.01, true);
        assert_eq!(c.check_entry(T + 299_000, "z", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "COOLDOWN");
        assert!(c.check_entry(T + 300_000, "z", 0.1, &f, 20.0, 0.5).is_ok());
        // 30 trades a day
        let mut d = RiskState::new(&r, 1000.0, T);
        for i in 0..30 {
            d.record_open(T, &format!("t{i}"), 0.1, 1.0);
            d.record_close(T, &format!("t{i}"), 0.1, 0.001, false);
        }
        assert_eq!(d.check_entry(T + 301_000, "t31", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "TRADES_PER_DAY");
        assert!(d.check_entry(T + 86_400_000, "t31", 0.1, &f, 20.0, 0.5).is_ok(), "new day");
    }

    #[test]
    fn regime_pause_after_20_trades_with_profit_factor_under_0_8() {
        let (_, r) = cfgs();
        let f = FeeModel::from_config(&cfgs().0);
        let mut s = RiskState::new(&r, 1000.0, T);
        // 10 wins of +0.01 and 10 losses of -0.02: PF 0.5 (alternate so the 3-loss stop never fires)
        for i in 0..20 {
            let pnl = if i % 2 == 0 { 0.01 } else { -0.02 };
            s.record_close(T + i * 10, &format!("g{i}"), 0.1, pnl, true);
        }
        assert!(s.regime_paused(T + 1000));
        assert_eq!(s.check_entry(T + 1_000, "n", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "REGIME_PAUSE");
        assert!(s.check_entry(T + REGIME_PAUSE_MS + 1000, "n", 0.1, &f, 20.0, 0.5).is_ok());
    }

    #[test]
    fn slippage_pause_when_average_buy_slippage_exceeds_6_percent() {
        let (_, r) = cfgs();
        let f = FeeModel::from_config(&cfgs().0);
        let mut s = RiskState::new(&r, 1000.0, T);
        for i in 0..10 {
            s.record_open(T, &format!("s{i}"), 0.1, 7.0);
            s.record_close(T, &format!("s{i}"), 0.1, 0.001, false);
        }
        assert_eq!(s.check_entry(T + 400_000, "q", 0.1, &f, 20.0, 0.5).unwrap_err().code(), "SLIPPAGE_PAUSE");
    }
}
