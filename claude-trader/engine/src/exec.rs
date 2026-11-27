//! The executor seam (SPEC section 4b). Entry, Scam Shield, picker, exits and risk are ONE code path. The only
//! thing that differs between paper and live is who fills the order, and that is this trait.
//!
//! - `PaperExecutor` (paper.rs): fills against the bonding-curve math with simulated delay and failures.
//! - `ScriptedExecutor` (below): deterministic fills, used by the same-decisions test.
//! - The future live executor lives only under `engine/src/live/` behind the M4 gate. It does not exist yet.
//!
//! Anything an executor cannot do is listed by `skipped_checks`, so the UI shows "skipped in paper" instead of
//! the two modes differing silently.

use crate::curve::{Curve, FeeBreakdown, FeeModel};
use crate::paper::{BuyFail, BuyFill, SellOutcome};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A check that this executor cannot run, shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedCheck {
    pub name: String,
    pub why: String,
}

#[async_trait]
pub trait Executor: Send + Sync {
    /// "paper", "scripted" or (later) "live".
    fn name(&self) -> &'static str;
    fn fees(&self) -> &FeeModel;
    /// What a buy of `size_sol` would cost against `curve`: (tokens, average price, curve after).
    fn quote_buy(&self, curve: &Curve, size_sol: f64) -> (f64, f64, Curve);
    fn buy_impact_pct(&self, curve: &Curve, size_sol: f64) -> f64;
    async fn buy(&self, size_sol: f64, quoted_price: f64, curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> Result<BuyFill, BuyFail>;
    async fn sell(&self, tokens: f64, reason: &str, start_attempt: u32, now_ms: &(dyn Fn() -> i64 + Sync), curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> SellOutcome;
    /// Checks this executor cannot run. Paper cannot simulate a real sell of a coin it does not hold.
    fn skipped_checks(&self) -> Vec<SkippedCheck>;
    /// Simulation knobs for tests and recalibration. A live executor ignores them.
    fn set_sell_fail_pct(&self, _pct: f64) {}
    fn set_unconfirmed_landed_pct(&self, _pct: f64) {}
    /// The "Paper fill realism" knobs. A live executor has no such thing and ignores it.
    fn set_fill_params(&self, _p: crate::paper::FillParams) {}
}

/// The honest list for paper mode. Shown in Settings under "Paper fill realism" and in the Scam Shield card.
pub fn paper_skipped_checks() -> Vec<SkippedCheck> {
    vec![
        SkippedCheck { name: "Honeypot sell test".into(), why: "Needs a real wallet that holds the coin and a real sell. Paper cannot do this, so it is skipped. Live mode will run it before every buy.".into() },
        SkippedCheck { name: "Real transaction errors".into(), why: "Paper imitates failed and slow trades with random chances. Real ones can fail for other reasons.".into() },
        SkippedCheck { name: "Other buyers racing you".into(), why: "In paper your buy does not move the price for anyone else and nobody beats you to a fill.".into() },
    ]
}

/// Deterministic executor: fills right away at the curve price, never fails, no delay. Same fee model.
/// Used by tests to prove the decision pipeline does not depend on which executor is plugged in.
pub struct ScriptedExecutor {
    fees: FeeModel,
}

impl ScriptedExecutor {
    pub fn new(cfg: &crate::Config) -> Self {
        ScriptedExecutor { fees: FeeModel::from_config(cfg) }
    }
}

#[async_trait]
impl Executor for ScriptedExecutor {
    fn name(&self) -> &'static str {
        "scripted"
    }
    fn fees(&self) -> &FeeModel {
        &self.fees
    }
    fn quote_buy(&self, curve: &Curve, size_sol: f64) -> (f64, f64, Curve) {
        let into_curve = size_sol * (1.0 - self.fees.pct_per_side() / 100.0);
        let (tokens, after) = curve.buy(into_curve);
        (tokens, if tokens > 0.0 { into_curve / tokens } else { 0.0 }, after)
    }
    fn buy_impact_pct(&self, curve: &Curve, size_sol: f64) -> f64 {
        let (_, avg, _) = self.quote_buy(curve, size_sol);
        if curve.price() > 0.0 { (avg / curve.price() - 1.0) * 100.0 } else { 0.0 }
    }
    async fn buy(&self, size_sol: f64, _quoted: f64, curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> Result<BuyFill, BuyFail> {
        let Some(curve) = curve_now().filter(|c| c.valid()) else {
            return Err(BuyFail { kind: crate::paper::FailKind::SubmitFailed, fees: FeeBreakdown::default(), delay_ms: 0, detail: "no curve data".into() });
        };
        let (tokens, avg, after) = self.quote_buy(&curve, size_sol);
        let fees = self.fees.trade_fees(size_sol, self.fees.priority_for_attempt(0));
        Ok(BuyFill { size_sol, tokens, entry_price: avg, fees, slippage_pct: 0.0, delay_ms: 0, curve_after: after })
    }
    async fn sell(&self, tokens: f64, reason: &str, start_attempt: u32, now_ms: &(dyn Fn() -> i64 + Sync), curve_now: &(dyn Fn() -> Option<Curve> + Sync)) -> SellOutcome {
        let Some(curve) = curve_now().filter(|c| c.valid()) else {
            return SellOutcome { fill: None, failed_fees: FeeBreakdown::default(), attempts: vec![] };
        };
        let (gross, _) = curve.sell(tokens);
        let fees = self.fees.trade_fees(gross, self.fees.priority_for_attempt(start_attempt));
        let stage = crate::position::StageSell { ts: now_ms(), reason: reason.to_string(), tokens, price: gross / tokens, gross_sol: gross, net_sol: gross - fees.total(), fees_sol: fees.total() };
        SellOutcome { fill: Some((stage, fees)), failed_fees: FeeBreakdown::default(), attempts: vec![] }
    }
    fn skipped_checks(&self) -> Vec<SkippedCheck> {
        vec![]
    }
}
