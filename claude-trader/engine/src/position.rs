//! A paper position and its per-mint state machine (SPEC 11b #16): BUYING -> OPEN -> SELLING -> OPEN/CLOSED.
//! Only one sell can be in flight per mint: `begin_sell` fails unless the state is OPEN.

use crate::curve::FeeBreakdown;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PosState {
    /// Written to SQLite before the (paper) buy is attempted.
    Buying,
    Open,
    Selling,
    Closed,
}

impl PosState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PosState::Buying => "BUYING",
            PosState::Open => "OPEN",
            PosState::Selling => "SELLING",
            PosState::Closed => "CLOSED",
        }
    }
    pub fn parse(s: &str) -> PosState {
        match s {
            "BUYING" => PosState::Buying,
            "OPEN" => PosState::Open,
            "SELLING" => PosState::Selling,
            _ => PosState::Closed,
        }
    }
}

/// One sell fill that belongs to a position (a tranche, a stop, a fade exit...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageSell {
    pub ts: i64,
    pub reason: String,
    pub tokens: f64,
    /// Average gross price of this fill (SOL per token, after price impact, before fees).
    pub price: f64,
    pub gross_sol: f64,
    pub net_sol: f64,
    pub fees_sol: f64,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub id: i64,
    /// Entry style that produced it: "momentum" or the paper variant "dip-40pct".
    pub lane: String,
    pub mint: String,
    pub symbol: String,
    pub creator: String,
    pub curve: String,
    pub state: PosState,
    pub entry_ms: i64,
    pub size_sol: f64,
    /// Average fill price of the buy (SOL per token, after price impact, before fees).
    pub entry_price: f64,
    pub entry_mcap_usd: f64,
    pub sol_usd: f64,
    pub tokens_initial: f64,
    pub tokens_remaining: f64,
    /// Net SOL received from all sells so far (after fees).
    pub realized_sol: f64,
    pub gross_exit_sol: f64,
    pub tokens_sold: f64,
    pub fees: FeeBreakdown,
    /// Transaction + priority fee of the buy (paid on top of `size_sol`).
    pub buy_fixed_sol: f64,
    /// Fees of attempts that were paid for but did not trade.
    pub failed_sol: f64,
    pub peak_price: f64,
    pub tranches_done: u32,
    pub soft_since_ms: Option<i64>,
    pub taper_done: bool,
    /// Last known good price. Never 0.0 (SPEC 11): updates are ignored unless the price is positive and finite.
    pub last_price: f64,
    pub sell_fails: u32,
    pub exit_ms: Option<i64>,
    pub exit_reason: String,
    pub stages: Vec<StageSell>,
    pub pnl_sol: f64,
    pub pnl_pct: f64,
    pub slippage_pct: f64,
    /// "trade" or "failed-buy" (a buy that never filled: no trade, only fees).
    pub kind: String,
}

impl Position {
    pub fn new_buying(lane: &str, mint: &str, symbol: &str, creator: &str, curve: &str, size_sol: f64, now_ms: i64, sol_usd: f64) -> Position {
        Position {
            id: 0,
            lane: lane.into(),
            mint: mint.into(),
            symbol: symbol.into(),
            creator: creator.into(),
            curve: curve.into(),
            state: PosState::Buying,
            entry_ms: now_ms,
            size_sol,
            entry_price: 0.0,
            entry_mcap_usd: 0.0,
            sol_usd,
            tokens_initial: 0.0,
            tokens_remaining: 0.0,
            realized_sol: 0.0,
            gross_exit_sol: 0.0,
            tokens_sold: 0.0,
            fees: FeeBreakdown::default(),
            buy_fixed_sol: 0.0,
            failed_sol: 0.0,
            peak_price: 0.0,
            tranches_done: 0,
            soft_since_ms: None,
            taper_done: false,
            last_price: 0.0,
            sell_fails: 0,
            exit_ms: None,
            exit_reason: String::new(),
            stages: vec![],
            pnl_sol: 0.0,
            pnl_pct: 0.0,
            slippage_pct: 0.0,
            kind: "trade".into(),
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self.state, PosState::Open | PosState::Selling | PosState::Buying)
    }

    pub fn hold_seconds(&self, now_ms: i64) -> f64 {
        ((now_ms - self.entry_ms) as f64 / 1000.0).max(0.0)
    }

    /// Record a price only if it is usable. Returns whether it was stored.
    pub fn note_price(&mut self, p: f64) -> bool {
        if p.is_finite() && p > 0.0 {
            self.last_price = p;
            if p > self.peak_price {
                self.peak_price = p;
            }
            true
        } else {
            false
        }
    }

    /// Percent change of `price` against the entry price.
    pub fn pct(&self, price: f64) -> f64 {
        if self.entry_price > 0.0 { (price / self.entry_price - 1.0) * 100.0 } else { 0.0 }
    }

    pub fn fill_buy(&mut self, entry_price: f64, tokens: f64, mcap_usd: f64, fees: &FeeBreakdown, slippage_pct: f64) -> Result<(), String> {
        if self.state != PosState::Buying {
            return Err(format!("buy fill while {}", self.state.as_str()));
        }
        self.entry_price = entry_price;
        self.tokens_initial = tokens;
        self.tokens_remaining = tokens;
        self.entry_mcap_usd = mcap_usd;
        self.fees.add(fees);
        self.buy_fixed_sol = fees.tx + fees.priority;
        self.slippage_pct = slippage_pct;
        self.peak_price = entry_price;
        self.last_price = entry_price;
        self.state = PosState::Open;
        Ok(())
    }

    /// OPEN -> SELLING. Anything else is refused, so a second sell for the same mint cannot start.
    pub fn begin_sell(&mut self) -> Result<(), String> {
        if self.state != PosState::Open {
            return Err(format!("sell refused: state is {}", self.state.as_str()));
        }
        if self.tokens_remaining <= 0.0 {
            return Err("sell refused: nothing left to sell".into());
        }
        self.state = PosState::Selling;
        Ok(())
    }

    /// SELLING -> OPEN again (the sell failed or only part of the position went), or CLOSED when empty.
    pub fn end_sell(&mut self, fill: Option<(StageSell, FeeBreakdown)>, failed_fees: &FeeBreakdown, now_ms: i64, reason: &str) -> Result<(), String> {
        if self.state != PosState::Selling {
            return Err(format!("sell result while {}", self.state.as_str()));
        }
        self.fees.add(failed_fees);
        self.failed_sol += failed_fees.total();
        if let Some((f, ff)) = fill {
            self.fees.add(&ff);
            self.tokens_remaining = (self.tokens_remaining - f.tokens).max(0.0);
            self.tokens_sold += f.tokens;
            self.realized_sol += f.net_sol;
            self.gross_exit_sol += f.gross_sol;
            self.stages.push(f);
            self.sell_fails = 0;
        } else {
            self.sell_fails += 1;
        }
        // dust left behind by float math counts as empty
        if self.tokens_remaining <= self.tokens_initial * 1e-9 {
            self.tokens_remaining = 0.0;
            self.state = PosState::Closed;
            self.exit_ms = Some(now_ms);
            self.exit_reason = reason.to_string();
            self.finish_pnl();
        } else {
            self.state = PosState::Open;
        }
        Ok(())
    }

    /// Net P/L = net sell proceeds - SOL spent on the buy (which already holds its percent fees)
    /// - the buy's transaction/priority fee - every failed attempt. Sell-side fees are netted in `realized_sol`.
    fn finish_pnl(&mut self) {
        self.pnl_sol = self.realized_sol - self.size_sol - self.buy_fixed_sol - self.failed_sol;
        self.pnl_pct = if self.size_sol > 0.0 { self.pnl_sol / self.size_sol * 100.0 } else { 0.0 };
    }

    /// A buy that never filled: only the failed attempt's fees were paid.
    pub fn fail_buy(&mut self, failed_fees: &FeeBreakdown, now_ms: i64, why: &str) {
        self.fees.add(failed_fees);
        self.failed_sol += failed_fees.total();
        self.state = PosState::Closed;
        self.kind = "failed-buy".into();
        self.exit_ms = Some(now_ms);
        self.exit_reason = why.to_string();
        self.pnl_sol = -self.failed_sol;
        self.pnl_pct = 0.0;
    }

    /// Unrealized net P/L if the remaining tokens were sold for `net_sol_if_sold_now` (fees already deducted).
    pub fn open_pnl(&self, net_sol_if_sold_now: f64) -> f64 {
        self.realized_sol + net_sol_if_sold_now - self.size_sol - self.buy_fixed_sol - self.failed_sol
    }

    pub fn avg_exit_price(&self) -> f64 {
        if self.tokens_sold > 0.0 { self.gross_exit_sol / self.tokens_sold } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_pos() -> Position {
        let mut p = Position::new_buying("momentum", "M", "T", "C", "CV", 0.1, 0, 150.0);
        p.fill_buy(1e-7, 1_000_000.0, 7000.0, &FeeBreakdown::default(), 0.0).unwrap();
        p
    }

    fn fill(tokens: f64) -> (StageSell, FeeBreakdown) {
        (StageSell { ts: 1, reason: "r".into(), tokens, price: 1e-7, gross_sol: 0.1, net_sol: 0.098, fees_sol: 0.002 }, FeeBreakdown::default())
    }

    #[test]
    fn state_machine_allows_open_selling_open_closed_and_nothing_else() {
        let mut p = open_pos();
        assert!(p.end_sell(None, &Default::default(), 1, "x").is_err(), "no sell result without a sell in flight");
        p.begin_sell().unwrap();
        assert_eq!(p.state, PosState::Selling);
        assert!(p.begin_sell().is_err(), "no double sell while SELLING");
        p.end_sell(Some(fill(400_000.0)), &Default::default(), 2, "take-profit-1").unwrap();
        assert_eq!(p.state, PosState::Open, "partial fill goes back to OPEN");
        p.begin_sell().unwrap();
        p.end_sell(Some(fill(600_000.0)), &Default::default(), 3, "max-hold").unwrap();
        assert_eq!(p.state, PosState::Closed);
        assert_eq!(p.exit_reason, "max-hold");
        assert!(p.begin_sell().is_err(), "CLOSED cannot be sold again");
    }

    #[test]
    fn a_failed_sell_returns_to_open_and_counts_the_failure() {
        let mut p = open_pos();
        p.begin_sell().unwrap();
        let fee = FeeBreakdown { tx: 0.000005, priority: 0.0005, ..Default::default() };
        p.end_sell(None, &fee, 2, "hard-stop").unwrap();
        assert_eq!((p.state, p.sell_fails), (PosState::Open, 1));
        assert!((p.failed_sol - 0.000505).abs() < 1e-12);
    }

    #[test]
    fn pnl_nets_every_fee() {
        let mut p = open_pos();
        p.buy_fixed_sol = 0.0005;
        p.begin_sell().unwrap();
        p.end_sell(Some(fill(1_000_000.0)), &FeeBreakdown { tx: 0.000005, ..Default::default() }, 9, "x").unwrap();
        // realized 0.098 - size 0.1 - buy fixed 0.0005 - failed 0.000005
        assert!((p.pnl_sol - (0.098 - 0.1 - 0.0005 - 0.000005)).abs() < 1e-12);
    }
}
