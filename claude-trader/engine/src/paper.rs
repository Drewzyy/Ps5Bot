//! Paper executor (SPEC 8, 11b #12/#16/#17). Fills against the bonding-curve constant-product formula with the
//! real fee model, price impact, a 400-1200 ms delay during which the price keeps moving, and random failures.
//! It never signs or sends anything. Never a mid-price mark: every fill walks the curve.

use crate::config::Config;
use crate::curve::{Curve, FeeBreakdown, FeeModel, Rng};
use crate::position::StageSell;
use std::sync::Mutex;
use std::time::Duration;

/// Buy slippage cap (SPEC 11b #12). `paperFill.slippageCapPct` (15) is the cap for sells.
pub const BUY_SLIPPAGE_CAP_PCT: f64 = 5.0;
/// Wait for an unconfirmed transaction before checking the wallet balance (SPEC 11b #16).
pub const CONFIRM_WAIT_MS: u64 = 20_000;
/// Most attempts for one sell call; the position stays OPEN and the 1 s loop tries again after that.
pub const MAX_SELL_ATTEMPTS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailKind {
    /// Never reached the network. Free. Safe to retry.
    SubmitFailed,
    /// Landed and failed on chain (fees paid). Safe to retry after a balance check.
    Reverted,
    /// No answer within 20 s. Only retried after the balance check shows the sell did not land.
    Unconfirmed,
    /// Buy only: the price moved against us during the delay.
    Slippage,
}

#[derive(Debug, Clone)]
pub struct BuyFill {
    pub size_sol: f64,
    pub tokens: f64,
    pub entry_price: f64,
    pub fees: FeeBreakdown,
    pub slippage_pct: f64,
    pub delay_ms: u32,
    pub curve_after: Curve,
}

#[derive(Debug, Clone)]
pub struct BuyFail {
    pub kind: FailKind,
    pub fees: FeeBreakdown,
    pub delay_ms: u32,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SellOutcome {
    pub fill: Option<(StageSell, FeeBreakdown)>,
    /// Fees of attempts that were paid for but did not trade.
    pub failed_fees: FeeBreakdown,
    pub attempts: Vec<AttemptLog>,
}

#[derive(Debug, Clone)]
pub struct AttemptLog {
    pub attempt: u32,
    pub priority_sol: f64,
    pub tokens: f64,
    pub result: String,
}

/// The knobs of "Paper fill realism". They can change while the engine runs (Settings preset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillParams {
    pub buy_fail_pct: f64,
    pub sell_fail_pct: f64,
    pub delay_ms_min: u32,
    pub delay_ms_max: u32,
    pub slippage_cap_pct: f64,
}

impl FillParams {
    pub fn from_config(c: &Config) -> Self {
        let p = &c.paper_fill;
        FillParams { buy_fail_pct: p.buy_fail_pct, sell_fail_pct: p.sell_fail_pct, delay_ms_min: p.delay_ms_min, delay_ms_max: p.delay_ms_max, slippage_cap_pct: p.slippage_cap_pct }
    }
    pub fn check(&self) -> Result<(), String> {
        if !(0.0..=100.0).contains(&self.buy_fail_pct) || !(0.0..=100.0).contains(&self.sell_fail_pct) {
            return Err("Fail chances must be between 0 and 100 percent.".into());
        }
        if self.delay_ms_min > self.delay_ms_max || self.delay_ms_max > 10_000 {
            return Err("The delay range must go from a smaller to a larger number, at most 10000 ms.".into());
        }
        if !(1.0..=50.0).contains(&self.slippage_cap_pct) {
            return Err("The slippage cap must be between 1 and 50 percent.".into());
        }
        Ok(())
    }
}

pub struct PaperExecutor {
    fees: FeeModel,
    params: Mutex<FillParams>,
    /// Chance (percent) that an unconfirmed sell turns out to have landed after the 20 s balance check.
    unconfirmed_landed_pct: Mutex<f64>,
    rng: Mutex<Rng>,
    /// 1.0 = real waits. Tests use 0.0.
    pub time_scale: f64,
}

impl PaperExecutor {
    pub fn new(cfg: &Config, rng: Rng) -> Self {
        PaperExecutor {
            fees: FeeModel::from_config(cfg),
            params: Mutex::new(FillParams::from_config(cfg)),
            unconfirmed_landed_pct: Mutex::new(40.0),
            rng: Mutex::new(rng),
            time_scale: 1.0,
        }
    }

    /// Change the simulated sell failure rate while running (tests, and a later live-vs-paper recalibration).
    pub fn set_sell_fail_pct(&self, pct: f64) {
        self.params.lock().unwrap().sell_fail_pct = pct;
    }

    /// Swap the realism knobs while running (the Settings preset).
    pub fn set_fill_params(&self, p: FillParams) {
        *self.params.lock().unwrap() = p;
    }

    fn p(&self) -> FillParams {
        *self.params.lock().unwrap()
    }

    pub fn set_unconfirmed_landed_pct(&self, pct: f64) {
        *self.unconfirmed_landed_pct.lock().unwrap() = pct;
    }

    pub fn fees(&self) -> &FeeModel {
        &self.fees
    }

    fn roll(&self) -> f64 {
        self.rng.lock().unwrap().f64()
    }

    fn delay(&self) -> u32 {
        { let p = self.p(); self.rng.lock().unwrap().range_u32(p.delay_ms_min, p.delay_ms_max) }
    }

    async fn wait(&self, ms: u64) {
        if self.time_scale > 0.0 && ms > 0 {
            tokio::time::sleep(Duration::from_millis((ms as f64 * self.time_scale) as u64)).await;
        }
    }

    fn fail_kind(&self, with_unconfirmed: bool) -> FailKind {
        let r = self.roll();
        if r < 0.50 {
            FailKind::SubmitFailed
        } else if r < 0.80 || !with_unconfirmed {
            FailKind::Reverted
        } else {
            FailKind::Unconfirmed
        }
    }

    /// What a buy of `size_sol` would cost against `curve`: (tokens, average price, curve after).
    pub fn quote_buy(&self, curve: &Curve, size_sol: f64) -> (f64, f64, Curve) {
        let into_curve = size_sol * (1.0 - self.fees.pct_per_side() / 100.0);
        let (tokens, after) = curve.buy(into_curve);
        (tokens, if tokens > 0.0 { into_curve / tokens } else { 0.0 }, after)
    }

    /// Price impact of a buy of `size_sol` as percent over the spot price.
    pub fn buy_impact_pct(&self, curve: &Curve, size_sol: f64) -> f64 {
        let (_, avg, _) = self.quote_buy(curve, size_sol);
        if curve.price() > 0.0 { (avg / curve.price() - 1.0) * 100.0 } else { 0.0 }
    }

    /// Buy. `quoted_price` is what the decision was based on; `curve_now` is read AFTER the delay, so the price
    /// can have moved. A fill that ends more than 5% above the quote is refused (slippage cap).
    pub async fn buy(&self, size_sol: f64, quoted_price: f64, curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> Result<BuyFill, BuyFail> {
        let delay_ms = self.delay();
        self.wait(delay_ms as u64).await;
        let priority = self.fees.priority_for_attempt(0);
        let Some(curve) = curve_now().filter(|c| c.valid()) else {
            return Err(BuyFail { kind: FailKind::SubmitFailed, fees: FeeBreakdown::default(), delay_ms, detail: "no curve data at fill time".into() });
        };
        let (tokens, avg, after) = self.quote_buy(&curve, size_sol);
        let slip = if quoted_price > 0.0 { (avg / quoted_price - 1.0) * 100.0 } else { 0.0 };
        if slip > BUY_SLIPPAGE_CAP_PCT {
            return Err(BuyFail { kind: FailKind::Slippage, fees: self.fees.failed_tx_fees(priority), delay_ms, detail: format!("price moved {slip:.1}% during the {delay_ms} ms delay, cap is {BUY_SLIPPAGE_CAP_PCT:.0}%") });
        }
        let buy_fail = self.p().buy_fail_pct;
        if self.roll() * 100.0 < buy_fail {
            let kind = self.fail_kind(false);
            let fees = if kind == FailKind::SubmitFailed { FeeBreakdown::default() } else { self.fees.failed_tx_fees(priority) };
            return Err(BuyFail { kind, fees, delay_ms, detail: format!("simulated {kind:?} ({}% fail rate)", buy_fail) });
        }
        if tokens <= 0.0 || avg <= 0.0 {
            return Err(BuyFail { kind: FailKind::SubmitFailed, fees: FeeBreakdown::default(), delay_ms, detail: "empty fill".into() });
        }
        let fees = self.fees.trade_fees(size_sol, priority);
        Ok(BuyFill { size_sol, tokens, entry_price: avg, fees, slippage_pct: slip, delay_ms, curve_after: after })
    }

    /// Sell `tokens` with the SPEC 11b #17 escalation: a failed sell retries with priority fee x2 then x4 (capped);
    /// after 3 failures the next attempt sells only half. `start_attempt` carries the failure count between calls
    /// so the escalation continues across the 1 s loop. An UNCONFIRMED attempt waits 20 s and checks the balance:
    /// if the sell landed it is NOT sold again.
    pub async fn sell(&self, tokens: f64, reason: &str, start_attempt: u32, now_ms: &(dyn Fn() -> i64 + Sync), curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> SellOutcome {
        let mut failed = FeeBreakdown::default();
        let mut attempts = vec![];
        for i in 0..MAX_SELL_ATTEMPTS {
            let attempt = start_attempt + i;
            let priority = self.fees.priority_for_attempt(attempt);
            let qty = if attempt >= 3 { tokens * 0.5 } else { tokens };
            let delay_ms = self.delay();
            self.wait(delay_ms as u64).await;
            let Some(curve) = curve_now().filter(|c| c.valid()) else {
                attempts.push(AttemptLog { attempt, priority_sol: priority, tokens: qty, result: "no curve data".into() });
                continue;
            };
            let (gross, _after) = curve.sell(qty);
            let spot_out = curve.price() * qty;
            let slip = if spot_out > 0.0 { (1.0 - gross / spot_out) * 100.0 } else { 0.0 };
            let mut landed = false;
            let mut result = String::new();
            let cap = self.p().slippage_cap_pct;
            if slip > cap {
                // the price fell too far: the sell reverts on chain and costs fees
                failed.add(&self.fees.failed_tx_fees(priority));
                result = format!("reverted: {slip:.1}% slippage over the {:.0}% cap", cap);
            } else if self.roll() * 100.0 < self.p().sell_fail_pct {
                match self.fail_kind(true) {
                    FailKind::SubmitFailed => result = "submit failed (never sent, free)".into(),
                    FailKind::Reverted => {
                        failed.add(&self.fees.failed_tx_fees(priority));
                        result = "reverted on chain (fees paid)".into();
                    }
                    FailKind::Unconfirmed => {
                        self.wait(CONFIRM_WAIT_MS).await;
                        // balance check after the wait: a coin flip whether it actually landed
                        if self.roll() * 100.0 < *self.unconfirmed_landed_pct.lock().unwrap() {
                            landed = true;
                            result = "unconfirmed, balance check shows it landed: not sold again".into();
                        } else {
                            result = "unconfirmed, balance check shows it did not land".into();
                        }
                    }
                    FailKind::Slippage => {}
                }
            } else {
                landed = true;
                result = "filled".into();
            }
            attempts.push(AttemptLog { attempt, priority_sol: priority, tokens: qty, result });
            if landed {
                let gross_now = gross;
                let fees = self.fees.trade_fees(gross_now, priority);
                let net = gross_now - fees.total();
                let stage = StageSell { ts: now_ms(), reason: reason.to_string(), tokens: qty, price: gross_now / qty, gross_sol: gross_now, net_sol: net, fees_sol: fees.total() };
                return SellOutcome { fill: Some((stage, fees)), failed_fees: failed, attempts };
            }
        }
        SellOutcome { fill: None, failed_fees: failed, attempts }
    }
}

#[async_trait::async_trait]
impl crate::exec::Executor for PaperExecutor {
    fn name(&self) -> &'static str {
        "paper"
    }
    fn fees(&self) -> &FeeModel {
        PaperExecutor::fees(self)
    }
    fn quote_buy(&self, curve: &Curve, size_sol: f64) -> (f64, f64, Curve) {
        PaperExecutor::quote_buy(self, curve, size_sol)
    }
    fn buy_impact_pct(&self, curve: &Curve, size_sol: f64) -> f64 {
        PaperExecutor::buy_impact_pct(self, curve, size_sol)
    }
    async fn buy(&self, size_sol: f64, quoted_price: f64, curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> Result<BuyFill, BuyFail> {
        PaperExecutor::buy(self, size_sol, quoted_price, curve_now).await
    }
    async fn sell(&self, tokens: f64, reason: &str, start_attempt: u32, now_ms: &(dyn Fn() -> i64 + Sync), curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> SellOutcome {
        PaperExecutor::sell(self, tokens, reason, start_attempt, now_ms, curve_now).await
    }
    fn skipped_checks(&self) -> Vec<crate::exec::SkippedCheck> {
        crate::exec::paper_skipped_checks()
    }
    fn set_sell_fail_pct(&self, pct: f64) {
        PaperExecutor::set_sell_fail_pct(self, pct)
    }
    fn set_fill_params(&self, p: FillParams) {
        PaperExecutor::set_fill_params(self, p)
    }
    fn set_unconfirmed_landed_pct(&self, pct: f64) {
        PaperExecutor::set_unconfirmed_landed_pct(self, pct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    fn curve() -> Curve {
        Curve { v_sol: 45.0, v_tokens: 7.1e8, real_sol: 15.0 }
    }

    fn exec(seed: u64, c: &Config) -> PaperExecutor {
        let mut e = PaperExecutor::new(c, Rng::new(seed));
        e.time_scale = 0.0;
        e
    }

    #[tokio::test]
    async fn buy_walks_the_curve_and_charges_every_fee() {
        let mut c = cfg();
        c.paper_fill.buy_fail_pct = 0.0;
        let e = exec(1, &c);
        let cv = curve();
        let f = e.buy(0.1, cv.price(), &|| Some(cv)).await.unwrap();
        // 2.25% of 0.1 goes to fees (protocol 0.95, creator 0.30, PumpPortal 1.0), the rest into the curve
        assert!((f.fees.pump_fun - 0.00095).abs() < 1e-12 && (f.fees.creator - 0.0003).abs() < 1e-12 && (f.fees.pump_portal - 0.001).abs() < 1e-12);
        assert!((f.fees.priority - 0.0005).abs() < 1e-12 && (f.fees.tx - 0.000005).abs() < 1e-12);
        let into = 0.1 * (1.0 - 0.0225);
        assert!((f.entry_price - into / f.tokens).abs() < 1e-18);
        // never the mid price: the fill is worse than spot
        assert!(f.entry_price > cv.price());
        assert!(f.delay_ms >= 400 && f.delay_ms <= 1200);
        assert!((f.curve_after.v_sol - (cv.v_sol + into)).abs() < 1e-12);
    }

    #[tokio::test]
    async fn price_moving_during_the_delay_is_used_and_over_5_percent_slippage_is_refused() {
        let mut c = cfg();
        c.paper_fill.buy_fail_pct = 0.0;
        let e = exec(2, &c);
        let quote = curve();
        let moved = Curve { v_sol: quote.v_sol * 1.03, v_tokens: quote.v_tokens / 1.03, real_sol: 16.0 }; // price +6.1%
        let r = e.buy(0.1, quote.price(), &|| Some(moved)).await;
        let err = r.unwrap_err();
        assert_eq!(err.kind, FailKind::Slippage);
        assert!(err.fees.total() > 0.0, "a reverted buy still pays fees");
        let small = Curve { v_sol: quote.v_sol * 1.01, v_tokens: quote.v_tokens / 1.01, real_sol: 15.5 }; // +2%
        let ok = e.buy(0.1, quote.price(), &|| Some(small)).await.unwrap();
        assert!(ok.slippage_pct > 1.9 && ok.slippage_pct < 3.0, "{}", ok.slippage_pct);
    }

    #[tokio::test]
    async fn failure_rates_are_about_10_and_15_percent() {
        let c = cfg();
        let e = exec(42, &c);
        let cv = curve();
        let mut fails = 0;
        for _ in 0..2000 {
            if e.buy(0.1, cv.price(), &|| Some(cv)).await.is_err() {
                fails += 1;
            }
        }
        assert!((fails as f64 / 2000.0 - 0.10).abs() < 0.03, "{fails}");
        let now = || 0i64;
        let mut first_try_fail = 0;
        for _ in 0..2000 {
            let o = e.sell(1_000_000.0, "t", 0, &now, &|| Some(cv)).await;
            if o.attempts.first().map(|a| a.result != "filled").unwrap_or(false) {
                first_try_fail += 1;
            }
        }
        assert!((first_try_fail as f64 / 2000.0 - 0.15).abs() < 0.03, "{first_try_fail}");
    }

    #[tokio::test]
    async fn sell_escalates_priority_2x_4x_and_halves_after_three_failures() {
        let mut c = cfg();
        c.paper_fill.sell_fail_pct = 100.0; // every attempt fails
        let e = exec(5, &c);
        let cv = curve();
        let now = || 0i64;
        let o = e.sell(1_000_000.0, "hard-stop", 0, &now, &|| Some(cv)).await;
        assert!(o.fill.is_none());
        assert_eq!(o.attempts.len() as u32, MAX_SELL_ATTEMPTS);
        let p: Vec<f64> = o.attempts.iter().map(|a| a.priority_sol).collect();
        assert_eq!(&p[..4], &[0.0005, 0.001, 0.002, 0.002]);
        assert!(p.iter().all(|x| *x <= 0.005));
        let q: Vec<f64> = o.attempts.iter().map(|a| a.tokens).collect();
        assert_eq!(&q[..4], &[1_000_000.0, 1_000_000.0, 1_000_000.0, 500_000.0]);
        // escalation continues across calls via start_attempt
        let o2 = e.sell(1_000_000.0, "hard-stop", 4, &now, &|| Some(cv)).await;
        assert_eq!(o2.attempts[0].tokens, 500_000.0);
    }

    #[tokio::test]
    async fn successful_sell_nets_fees_and_impact() {
        let mut c = cfg();
        c.paper_fill.sell_fail_pct = 0.0;
        let e = exec(9, &c);
        let cv = curve();
        let now = || 77i64;
        let o = e.sell(2_000_000.0, "take-profit-1", 0, &now, &|| Some(cv)).await;
        let (s, fees) = o.fill.unwrap();
        let (gross, _) = cv.sell(2_000_000.0);
        assert!((s.gross_sol - gross).abs() < 1e-12);
        assert!(s.gross_sol < cv.price() * 2_000_000.0, "sells below spot");
        assert!((fees.pump_fun - gross * 0.0095).abs() < 1e-12 && (fees.creator - gross * 0.003).abs() < 1e-12 && (fees.pump_portal - gross * 0.01).abs() < 1e-12);
        assert!((s.net_sol - (gross - fees.total())).abs() < 1e-12);
        assert_eq!(s.ts, 77);
    }

    /// Lamports: round to 1e-9 SOL.
    fn lam(x: f64) -> i64 {
        (x * 1e9).round() as i64
    }

    #[tokio::test]
    async fn round_trip_balance_change_equals_proceeds_minus_every_cost_to_the_lamport() {
        use crate::position::{PosState, Position};
        let mut c = cfg();
        c.paper_fill.buy_fail_pct = 0.0;
        c.paper_fill.sell_fail_pct = 0.0;
        c.fees.jito_tip_sol = 0.0003; // prove a Jito tip is charged on both sides when set
        let e = exec(5, &c);
        let cv = curve();
        let size = 0.1;
        let b = e.buy(size, cv.price(), &|| Some(cv)).await.unwrap();
        let mut p = Position::new_buying("momentum", "M", "T", "C", "CV", size, 0, 150.0);
        p.fill_buy(b.entry_price, b.tokens, 1.0, &b.fees, b.slippage_pct).unwrap();
        // while open: spend + fixed buy fees + locked rent are out of the wallet
        let fixed_buy = b.fees.tx + b.fees.priority + b.fees.jito;
        assert_eq!(lam(p.balance_delta()), lam(-(size + fixed_buy + crate::curve::TOKEN_ACCOUNT_RENT_SOL)));
        assert_eq!(lam(p.rent_locked()), lam(0.00203928));
        p.begin_sell().unwrap();
        let now = || 1i64;
        let o = e.sell(b.tokens, "take-profit-1", 0, &now, &|| Some(b.curve_after)).await;
        let (stage, sfees) = o.fill.clone().unwrap();
        let (gross, _) = b.curve_after.sell(b.tokens);
        // independent recompute of the sell-side cost: 0.95 + 0.30 + 1.0 percent of the SOL actually received, tx, priority, jito
        let expect_sell_fees = gross * 0.0225 + 0.000005 + 0.0005 + 0.0003;
        assert_eq!(lam(sfees.total()), lam(expect_sell_fees));
        assert_eq!(lam(stage.net_sol), lam(gross - expect_sell_fees));
        p.end_sell(o.fill, &o.failed_fees, 2, "take-profit-1").unwrap();
        assert_eq!(p.state, PosState::Closed);
        // rent is back once the account is closed
        assert_eq!(p.rent_locked(), 0.0);
        let expected = (gross - expect_sell_fees) - (size + 0.000005 + 0.0005 + 0.0003);
        assert_eq!(lam(p.balance_delta()), lam(expected));
        assert_eq!(lam(p.pnl_sol), lam(expected));
        // the fee components on the position add up to every cost except the price impact already inside the curve fill
        let all_fees = size * 0.0225 + 0.000005 + 0.0005 + 0.0003 + expect_sell_fees;
        assert_eq!(lam(p.fees.total()), lam(all_fees));
        assert_eq!(lam(p.fees.creator), lam(size * 0.003 + gross * 0.003));
        assert_eq!(lam(p.fees.jito), lam(0.0006));
    }

    #[tokio::test]
    async fn failed_or_reverted_transactions_still_cost_fees() {
        use crate::position::Position;
        let mut c = cfg();
        c.paper_fill.buy_fail_pct = 100.0;
        c.fees.jito_tip_sol = 0.0003;
        let cv = curve();
        // find a seed whose failure is an on-chain revert (paid), then check the cost lands in the balance
        let mut paid = None;
        for seed in 1..200u64 {
            let e = exec(seed, &c);
            if let Err(f) = e.buy(0.1, cv.price(), &|| Some(cv)).await {
                if f.kind == FailKind::Reverted {
                    paid = Some(f);
                    break;
                }
            }
        }
        let f = paid.expect("a reverted buy shows up within 200 seeds");
        assert_eq!(lam(f.fees.total()), lam(0.000005 + 0.0005 + 0.0003), "base fee + priority + jito, no percent fee");
        let mut p = Position::new_buying("momentum", "M", "T", "C", "CV", 0.1, 0, 150.0);
        p.fail_buy(&f.fees, 1, "buy-failed");
        assert_eq!(lam(p.pnl_sol), lam(-(0.000005 + 0.0005 + 0.0003)));
        assert_eq!(p.rent_locked(), 0.0, "no token account was opened");
    }
}
