//! Graded launch-slot check (SPEC 7b). The old rule blocked a coin when ANY other wallet traded in its creation slot. The
//! research says professional snipers buy block 0 on most launches (about 62% of new coins have a third-party trade within
//! one second, sniping-lab README, UNVERIFIED), so that rule blocks nearly everything and tells us nothing. Instead we
//! measure the launch group: the non-creator wallets that bought in the creation slot or the next one, what they still hold,
//! and how many SOL they could pull out of the curve by selling all of it right now. A coin is blocked only above limits.
//!
//! Thresholds (from the deep-curve research, makefun backtest, UNVERIFIED): launch-slot wallets able to pull more than 16 SOL
//! in total, one wallet able to pull more than 11 SOL, or the group holding 40% or more of the supply (the "one-slot collapse").

use crate::config::GradedShield;
use crate::curve::Curve;
use crate::pump::{Trade, SUPPLY_TOKENS};
use crate::shield::rules::CheckResult;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaunchExposure {
    pub create_slot: u64,
    /// Distinct non-creator wallets that bought in the creation slot or the next one.
    pub wallets: u32,
    /// Non-creator buy transactions in the creation slot itself.
    pub creation_slot_buys: u32,
    /// Tokens those wallets still hold (bought minus sold in the data we have).
    pub held_tokens: f64,
    pub supply_pct: f64,
    /// SOL they would get for selling everything at once into the current curve (before fees), capped by the real reserve.
    pub total_pull_sol: f64,
    /// Same for the single biggest wallet.
    pub max_single_pull_sol: f64,
    /// False when the data to measure it is missing (no creation slot known, or no curve state).
    pub known: bool,
}

impl LaunchExposure {
    /// One line with every graded value, written to the decisions table for every coin.
    pub fn summary(&self) -> String {
        if !self.known {
            return "launch group not measured (creation slot or curve state unknown)".into();
        }
        format!(
            "{} launch-slot wallet(s), {} buy(s) in the creation slot, could pull {:.2} SOL (biggest {:.2} SOL), hold {:.1}% of supply",
            self.wallets, self.creation_slot_buys, self.total_pull_sol, self.max_single_pull_sol, self.supply_pct
        )
    }
}

/// Measure the launch group from every trade we know (the live store plus the creation-slot trades read from the chain).
/// `curve` is the current curve state.
pub fn launch_exposure(trades: &[Trade], creator: &str, create_slot: Option<u64>, curve: Option<Curve>) -> LaunchExposure {
    let (Some(slot0), Some(curve)) = (create_slot, curve.filter(|c| c.valid())) else { return LaunchExposure { known: false, ..Default::default() } };
    // one trade once: the live feed and the chain read can both hold it
    let mut seen: HashSet<(&str, &str, bool)> = HashSet::new();
    let uniq: Vec<&Trade> = trades.iter().filter(|t| seen.insert((t.signature.as_str(), t.user.as_str(), t.is_buy))).collect();
    let group: HashSet<&str> = uniq.iter().filter(|t| t.is_buy && t.user != creator && t.slot >= slot0 && t.slot <= slot0 + 1).map(|t| t.user.as_str()).collect();
    let creation_slot_buys = uniq.iter().filter(|t| t.is_buy && t.user != creator && t.slot == slot0).count() as u32;
    let mut net: HashMap<&str, f64> = HashMap::new();
    for t in &uniq {
        if group.contains(t.user.as_str()) {
            *net.entry(t.user.as_str()).or_default() += if t.is_buy { t.tokens } else { -t.tokens };
        }
    }
    let held: Vec<f64> = net.values().map(|v| v.max(0.0)).filter(|v| *v > 0.0).collect();
    let total_tokens: f64 = held.iter().sum();
    let cap = curve.real_sol.max(0.0);
    let pull = |tokens: f64| if tokens > 0.0 { curve.sell(tokens).0.min(cap) } else { 0.0 };
    LaunchExposure {
        create_slot: slot0,
        wallets: group.len() as u32,
        creation_slot_buys,
        held_tokens: total_tokens,
        supply_pct: total_tokens / SUPPLY_TOKENS * 100.0,
        total_pull_sol: pull(total_tokens),
        max_single_pull_sol: pull(held.iter().cloned().fold(0.0, f64::max)),
        known: true,
    }
}

/// The decision: block only above the limits. Never blocks when the group could not be measured (that is not a scam signal;
/// the other Scam Shield checks still apply and the missing measurement is logged).
pub fn grade(e: &LaunchExposure, g: &GradedShield) -> CheckResult {
    let name = "launch_grade";
    if !e.known {
        return CheckResult::not_available(name, e.summary());
    }
    if e.total_pull_sol > g.max_launch_slot_pull_sol {
        return CheckResult::block(name, "LAUNCH_SLOT_PULL", format!("Launch-slot wallets could pull {:.1} SOL out of the curve (limit {:.0} SOL). {}", e.total_pull_sol, g.max_launch_slot_pull_sol, e.summary()));
    }
    if e.max_single_pull_sol > g.max_single_wallet_pull_sol {
        return CheckResult::block(name, "LAUNCH_SLOT_SINGLE", format!("One launch-slot wallet could pull {:.1} SOL (limit {:.0} SOL). {}", e.max_single_pull_sol, g.max_single_wallet_pull_sol, e.summary()));
    }
    if e.supply_pct >= g.max_launch_group_supply_pct {
        return CheckResult::block(name, "LAUNCH_GROUP_SUPPLY", format!("Launch-slot wallets hold {:.0}% of the supply (limit {:.0}%). {}", e.supply_pct, g.max_launch_group_supply_pct, e.summary()));
    }
    CheckResult::pass(name, e.summary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shield::rules::Outcome;

    fn t(user: &str, buy: bool, slot: u64, tokens_m: f64, sig: &str) -> Trade {
        Trade { mint: "m".into(), user: user.into(), is_buy: buy, sol: 0.0, tokens: tokens_m * 1e6, slot, signature: sig.into(), recv_ms: 1, v_sol: 30.0, v_tokens: 1.073e9, real_sol: 0.0, ..Default::default() }
    }
    fn curve() -> Curve {
        // 45 real SOL in the curve: v_sol 75, v_tokens from the 32,190,000,000,000 k constant
        let v_sol = 75.0;
        Curve { v_sol, v_tokens: 30.0 * 1.073e9 / v_sol, real_sol: 45.0 }
    }
    fn g() -> GradedShield {
        GradedShield::default()
    }

    #[test]
    fn a_coin_with_a_couple_of_small_launch_slot_buyers_passes_instead_of_being_hard_blocked() {
        let trades = vec![t("snipe1", true, 100, 5.0, "a"), t("snipe2", true, 101, 5.0, "b"), t("later", true, 150, 50.0, "c")];
        let e = launch_exposure(&trades, "dev", Some(100), Some(curve()));
        assert_eq!((e.wallets, e.creation_slot_buys), (2, 1));
        assert!(e.total_pull_sol > 0.0 && e.total_pull_sol < 16.0, "{e:?}");
        assert!(e.supply_pct < 1.5);
        assert_eq!(grade(&e, &g()).outcome, Outcome::Pass);
    }

    #[test]
    fn a_launch_group_that_could_pull_more_than_16_sol_is_blocked() {
        // 12 wallets holding 5% each = 60% of supply, but the pull limit trips first
        let trades: Vec<Trade> = (0..12).map(|i| t(&format!("w{i}"), true, 100, 5.0 * 10.0, &format!("s{i}"))).collect();
        let e = launch_exposure(&trades, "dev", Some(100), Some(curve()));
        assert!(e.total_pull_sol > 16.0, "{e:?}");
        let r = grade(&e, &g());
        assert!(r.blocks() && r.code == "LAUNCH_SLOT_PULL", "{r:?}");
    }

    #[test]
    fn one_big_wallet_is_blocked_on_its_own_even_when_the_total_limit_is_raised() {
        let trades = vec![t("whale", true, 100, 250.0, "a")];
        let e = launch_exposure(&trades, "dev", Some(100), Some(curve()));
        let mut lim = g();
        lim.max_launch_slot_pull_sol = 1000.0;
        lim.max_launch_group_supply_pct = 100.0;
        assert!(e.max_single_pull_sol > 11.0, "{e:?}");
        assert_eq!(grade(&e, &lim).code, "LAUNCH_SLOT_SINGLE");
    }

    #[test]
    fn supply_share_blocks_only_when_the_pull_limits_are_loose() {
        let trades = vec![t("a", true, 100, 200.0, "a"), t("b", true, 101, 220.0, "b")];
        let e = launch_exposure(&trades, "dev", Some(100), Some(curve()));
        assert!(e.supply_pct >= 40.0, "{e:?}");
        let mut lim = g();
        lim.max_launch_slot_pull_sol = 1000.0;
        lim.max_single_wallet_pull_sol = 1000.0;
        assert_eq!(grade(&e, &lim).code, "LAUNCH_GROUP_SUPPLY");
    }

    #[test]
    fn wallets_that_already_sold_do_not_count_and_the_creator_never_counts() {
        let trades = vec![t("a", true, 100, 400.0, "a"), t("a", false, 120, 400.0, "a2"), t("dev", true, 100, 400.0, "d")];
        let e = launch_exposure(&trades, "dev", Some(100), Some(curve()));
        assert_eq!((e.wallets, e.total_pull_sol), (1, 0.0));
        assert_eq!(grade(&e, &g()).outcome, Outcome::Pass);
    }

    #[test]
    fn the_same_trade_from_the_live_feed_and_the_chain_read_counts_once() {
        let a = t("a", true, 100, 50.0, "same");
        let once = launch_exposure(&[a.clone()], "dev", Some(100), Some(curve()));
        let twice = launch_exposure(&[a.clone(), a], "dev", Some(100), Some(curve()));
        assert_eq!(once, twice);
    }

    #[test]
    fn nothing_known_is_not_a_block_but_is_logged_as_not_measured() {
        let e = launch_exposure(&[], "dev", None, Some(curve()));
        assert!(!e.known);
        let r = grade(&e, &g());
        assert!(!r.blocks());
        assert!(r.detail.contains("not measured"));
        assert!(!launch_exposure(&[], "dev", Some(100), None).known);
    }

    #[test]
    fn pull_is_capped_by_the_real_reserve() {
        let tiny = Curve { v_sol: 32.0, v_tokens: 1.0e9, real_sol: 0.5 };
        let e = launch_exposure(&[t("a", true, 100, 400.0, "a")], "dev", Some(100), Some(tiny));
        assert!(e.total_pull_sol <= 0.5 + 1e-9);
    }
}
