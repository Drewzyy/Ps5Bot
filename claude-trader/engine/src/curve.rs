//! Bonding-curve constant-product math and the fee model (SPEC 8, 6 `fees`).
//!
//! pump.fun sells tokens along x*y=k with VIRTUAL reserves (vSol, vTokens). A buy of `s` SOL (after fees) pays
//! `vTokens - k/(vSol+s)` tokens; a sell of `t` tokens returns `vSol - k/(vTokens+t)` SOL (before fees).
//! All amounts are in whole units (SOL, tokens). Paper fills use this and never the mid price.

use crate::config::Config;
use serde::{Deserialize, Serialize};

/// Base network fee of one transaction (5000 lamports).
pub const BASE_TX_FEE_SOL: f64 = 0.000005;
/// Rent of a token account (recovered when the position is fully sold and the account is closed, SPEC 11b #15).
pub const TOKEN_ACCOUNT_RENT_SOL: f64 = 0.00203928;
/// Real SOL in the curve when a pump.fun coin graduates (migrates to the DEX). 90% of this triggers our sell (SPEC 11b #9).
pub const GRADUATION_REAL_SOL: f64 = 85.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub v_sol: f64,
    pub v_tokens: f64,
    pub real_sol: f64,
}

impl Curve {
    pub fn price(&self) -> f64 {
        if self.v_tokens > 0.0 { self.v_sol / self.v_tokens } else { 0.0 }
    }
    pub fn valid(&self) -> bool {
        self.v_sol > 0.0 && self.v_tokens > 0.0 && self.price().is_finite() && self.price() > 0.0
    }
    /// Spend `sol_in` SOL on the curve (fees already taken). Returns tokens received and the curve afterwards.
    pub fn buy(&self, sol_in: f64) -> (f64, Curve) {
        let k = self.v_sol * self.v_tokens;
        let new_sol = self.v_sol + sol_in;
        let new_tokens = k / new_sol;
        (self.v_tokens - new_tokens, Curve { v_sol: new_sol, v_tokens: new_tokens, real_sol: self.real_sol + sol_in })
    }
    /// Sell `tokens` into the curve. Returns gross SOL (before fees) and the curve afterwards.
    pub fn sell(&self, tokens: f64) -> (f64, Curve) {
        let k = self.v_sol * self.v_tokens;
        let new_tokens = self.v_tokens + tokens;
        let new_sol = k / new_tokens;
        let out = self.v_sol - new_sol;
        (out, Curve { v_sol: new_sol, v_tokens: new_tokens, real_sol: (self.real_sol - out).max(0.0) })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeBreakdown {
    pub pump_fun: f64,
    pub pump_portal: f64,
    pub tx: f64,
    pub priority: f64,
    pub jito: f64,
}

impl FeeBreakdown {
    pub fn total(&self) -> f64 {
        self.pump_fun + self.pump_portal + self.tx + self.priority + self.jito
    }
    pub fn add(&mut self, o: &FeeBreakdown) {
        self.pump_fun += o.pump_fun;
        self.pump_portal += o.pump_portal;
        self.tx += o.tx;
        self.priority += o.priority;
        self.jito += o.jito;
    }
}

#[derive(Debug, Clone)]
pub struct FeeModel {
    pub pump_pct: f64,
    pub portal_pct: f64,
    pub priority_sol: f64,
    pub priority_cap_sol: f64,
    pub base_tx_sol: f64,
}

impl FeeModel {
    pub fn from_config(c: &Config) -> Self {
        FeeModel {
            pump_pct: c.fees.model_pump_fun_pct_per_side,
            portal_pct: c.fees.pump_portal_pct_per_side,
            priority_sol: c.fees.pump_portal_priority_fee_sol,
            priority_cap_sol: c.fees.pump_portal_priority_fee_cap_sol,
            base_tx_sol: BASE_TX_FEE_SOL,
        }
    }
    pub fn pct_per_side(&self) -> f64 {
        self.pump_pct + self.portal_pct
    }
    /// Priority fee for attempt number `attempt` (0 = first): doubles each retry, capped (SPEC 11b #17).
    pub fn priority_for_attempt(&self, attempt: u32) -> f64 {
        (self.priority_sol * 2f64.powi(attempt.min(2) as i32)).min(self.priority_cap_sol)
    }
    /// Fees of a landed trade of `gross_sol` (percent fees on that amount) plus one transaction.
    pub fn trade_fees(&self, gross_sol: f64, priority: f64) -> FeeBreakdown {
        FeeBreakdown {
            pump_fun: gross_sol * self.pump_pct / 100.0,
            pump_portal: gross_sol * self.portal_pct / 100.0,
            tx: self.base_tx_sol,
            priority,
            jito: 0.0,
        }
    }
    /// Fees of a transaction that was paid for but did not trade (reverted on chain).
    pub fn failed_tx_fees(&self, priority: f64) -> FeeBreakdown {
        FeeBreakdown { tx: self.base_tx_sol, priority, ..Default::default() }
    }
}

/// Break-even move (in percent of the entry price) for a round trip of `size_sol`:
/// both sides' percent fees, both transactions' fixed cost, the token-account rent that is still unrecovered at
/// the first take-profit, and the price impact of getting in and out (`impact_pct`, both sides summed).
pub fn break_even_pct(size_sol: f64, fees: &FeeModel, impact_pct: f64) -> f64 {
    if size_sol <= 0.0 {
        return f64::INFINITY;
    }
    let percent = 2.0 * fees.pct_per_side();
    let fixed = 2.0 * (fees.base_tx_sol + fees.priority_sol) + TOKEN_ACCOUNT_RENT_SOL;
    percent + fixed / size_sol * 100.0 + impact_pct
}

/// Tiny deterministic RNG (xorshift64*) so paper fills are reproducible in tests.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }
    pub fn from_time() -> Self {
        let t = crate::rpc::now_ms() as u64;
        Rng::new(t.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ std::process::id() as u64)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in [0,1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo { lo } else { lo + (self.f64() * (hi - lo + 1) as f64) as u32 % (hi - lo + 1) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Curve {
        Curve { v_sol: 30.0, v_tokens: 1_073_000_000.0, real_sol: 0.0 }
    }

    #[test]
    fn constant_product_holds_and_buy_sell_round_trip_returns_the_input() {
        let c = fresh();
        let k = c.v_sol * c.v_tokens;
        let (tok, after) = c.buy(1.0);
        assert!((after.v_sol * after.v_tokens - k).abs() / k < 1e-12);
        assert!(tok > 0.0 && after.price() > c.price());
        let (out, back) = after.sell(tok);
        assert!((out - 1.0).abs() < 1e-9, "{out}");
        assert!((back.v_sol - c.v_sol).abs() < 1e-9);
    }

    #[test]
    fn price_impact_grows_with_size() {
        let c = Curve { v_sol: 60.0, v_tokens: 5.0e8, real_sol: 30.0 };
        let avg = |s: f64| s / c.buy(s).0;
        assert!(avg(1.0) > c.price());
        assert!(avg(5.0) > avg(1.0));
    }

    #[test]
    fn fee_model_matches_spec_and_escalates_with_cap() {
        let cfg = crate::Config::from_json(include_str!("../../config.json")).unwrap();
        let f = FeeModel::from_config(&cfg);
        assert!((f.pct_per_side() - 1.75).abs() < 1e-12);
        let t = f.trade_fees(0.2, 0.0005);
        assert!((t.pump_fun - 0.0025).abs() < 1e-12 && (t.pump_portal - 0.001).abs() < 1e-12);
        assert!((f.priority_for_attempt(0) - 0.0005).abs() < 1e-12);
        assert!((f.priority_for_attempt(1) - 0.001).abs() < 1e-12);
        assert!((f.priority_for_attempt(2) - 0.002).abs() < 1e-12);
        assert!((f.priority_for_attempt(9) - 0.002).abs() < 1e-12);
    }

    #[test]
    fn rng_is_deterministic_and_roughly_uniform() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        let n = 20000;
        let hits = (0..n).filter(|_| a.f64() < 0.1).count() as f64 / n as f64;
        assert!((hits - 0.1).abs() < 0.015, "{hits}");
    }
}
