//! Deep-curve sprint lane, PAPER ONLY (SPEC 6c).
//!
//! Idea (one tiny, unreplicated backtest, UNVERIFIED): coins that fill fast and are already deep on the curve (39 to 60 SOL of the
//! 85 SOL needed to graduate) have a price that still rises about 2.4x from 40 SOL to our exit at 90% of graduation, while the
//! main lane's entry window (4 to 40 SOL, 60 s old) never reaches them. Entry here: real reserves 39 to 60 SOL, the coin
//! 40 to 120 slots old, at least 30 distinct buyer wallets, launch-slot wallets able to pull at most 16 SOL and no single wallet
//! able to pull more than 11 SOL, plus the usual Scam Shield. Exit: the percent stops are off; the stops are in SOL of real
//! reserve (20 below entry, 40 below the peak) and the sell at 90% of graduation stays.
//!
//! The lane exists to MEASURE this: its stats are kept apart like the dip lane's, and it never changes what the main lane does.

use crate::config::Sprint;
use crate::pump::Trade;
use crate::shield::launch::LaunchExposure;
use std::collections::HashSet;

/// Everything the entry rule needs, measured by the watcher on each tick.
#[derive(Debug, Clone)]
pub struct Inputs<'a> {
    /// Slots since the coin was created (latest trade slot minus creation slot). None when either is unknown.
    pub age_slots: Option<u64>,
    /// Real SOL reserve of the curve now.
    pub real_sol: f64,
    pub buyers: u32,
    pub exposure: &'a LaunchExposure,
    pub creator_sold: bool,
    /// The coin showed itself as mayhem or non-SOL quote.
    pub excluded_type: bool,
    pub open_in_lane: usize,
}

pub fn distinct_buyers(trades: &[Trade]) -> u32 {
    trades.iter().filter(|t| t.is_buy).map(|t| t.user.as_str()).collect::<HashSet<_>>().len() as u32
}

/// Why the coin is not an entry right now (plain words). Empty = every sprint rule holds.
pub fn why_not(s: &Sprint, i: &Inputs) -> Vec<String> {
    let mut v = vec![];
    if i.excluded_type {
        v.push("mayhem or non-SOL coin".to_string());
    }
    match i.age_slots {
        None => v.push("age in slots unknown".into()),
        Some(a) if a < s.min_age_slots => v.push(format!("only {a} slots old (needs {} to {})", s.min_age_slots, s.max_age_slots)),
        Some(a) if a > s.max_age_slots => v.push(format!("{a} slots old (needs {} to {})", s.min_age_slots, s.max_age_slots)),
        _ => {}
    }
    if i.real_sol < s.real_sol_min || i.real_sol > s.real_sol_max {
        v.push(format!("curve holds {:.1} SOL (needs {} to {})", i.real_sol, s.real_sol_min, s.real_sol_max));
    }
    if i.buyers < s.min_buyers {
        v.push(format!("{} buyer wallets (needs {})", i.buyers, s.min_buyers));
    }
    if !i.exposure.known {
        v.push("launch group not measured".into());
    } else {
        if i.exposure.total_pull_sol > s.max_launch_slot_pull_sol {
            v.push(format!("launch-slot wallets could pull {:.1} SOL (limit {})", i.exposure.total_pull_sol, s.max_launch_slot_pull_sol));
        }
        if i.exposure.max_single_pull_sol > s.max_single_wallet_pull_sol {
            v.push(format!("one launch-slot wallet could pull {:.1} SOL (limit {})", i.exposure.max_single_pull_sol, s.max_single_wallet_pull_sol));
        }
    }
    if i.creator_sold {
        v.push("creator has sold".into());
    }
    if i.open_in_lane >= s.max_open as usize {
        v.push(format!("{} sprint positions already open", i.open_in_lane));
    }
    v
}

/// Has the window closed for good (too old, or too deep to enter)? The watcher stops then.
pub fn window_over(s: &Sprint, age_slots: Option<u64>, real_sol: f64) -> bool {
    age_slots.map(|a| a > s.max_age_slots).unwrap_or(false) || real_sol > s.real_sol_max
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(total: f64, single: f64) -> LaunchExposure {
        LaunchExposure { create_slot: 100, wallets: 3, creation_slot_buys: 2, held_tokens: 1.0, supply_pct: 2.0, total_pull_sol: total, max_single_pull_sol: single, known: true }
    }
    fn good<'a>(e: &'a LaunchExposure) -> Inputs<'a> {
        Inputs { age_slots: Some(80), real_sol: 45.0, buyers: 34, exposure: e, creator_sold: false, excluded_type: false, open_in_lane: 0 }
    }

    #[test]
    fn a_coin_that_meets_every_rule_enters() {
        let e = exp(6.0, 3.0);
        assert!(why_not(&Sprint::default(), &good(&e)).is_empty());
    }

    #[test]
    fn each_rule_alone_stops_the_entry_with_a_plain_reason() {
        let s = Sprint::default();
        let e = exp(6.0, 3.0);
        let mut cases: Vec<(Inputs, &str)> = vec![];
        let mut i = good(&e);
        i.real_sol = 38.9;
        cases.push((i.clone(), "curve holds 38.9 SOL"));
        i.real_sol = 60.1;
        cases.push((i.clone(), "curve holds 60.1 SOL"));
        let mut i = good(&e);
        i.age_slots = Some(39);
        cases.push((i.clone(), "39 slots old"));
        i.age_slots = Some(121);
        cases.push((i.clone(), "121 slots old"));
        i.age_slots = None;
        cases.push((i.clone(), "unknown"));
        let mut i = good(&e);
        i.buyers = 29;
        cases.push((i.clone(), "29 buyer wallets"));
        let mut i = good(&e);
        i.creator_sold = true;
        cases.push((i.clone(), "creator has sold"));
        let mut i = good(&e);
        i.open_in_lane = 3;
        cases.push((i.clone(), "3 sprint positions"));
        let mut i = good(&e);
        i.excluded_type = true;
        cases.push((i, "mayhem"));
        for (inp, want) in cases {
            let w = why_not(&s, &inp);
            assert!(w.iter().any(|x| x.contains(want)), "{want}: {w:?}");
        }
        // edges that are inside the window
        for (real, age, buyers) in [(39.0, 40u64, 30u32), (60.0, 120, 30)] {
            let mut i = good(&e);
            i.real_sol = real;
            i.age_slots = Some(age);
            i.buyers = buyers;
            assert!(why_not(&s, &i).is_empty(), "{real} {age} {buyers}");
        }
    }

    #[test]
    fn launch_slot_wallets_that_could_pull_more_than_16_sol_or_11_alone_stop_it() {
        let s = Sprint::default();
        let big = exp(16.5, 4.0);
        assert!(why_not(&s, &good(&big)).iter().any(|x| x.contains("could pull 16.5 SOL")));
        let one = exp(12.0, 11.5);
        assert!(why_not(&s, &good(&one)).iter().any(|x| x.contains("one launch-slot wallet")));
        let edge = exp(16.0, 11.0);
        assert!(why_not(&s, &good(&edge)).is_empty(), "exactly at the limits is allowed");
        let unknown = LaunchExposure { known: false, ..Default::default() };
        assert!(why_not(&s, &good(&unknown)).iter().any(|x| x.contains("not measured")), "an unmeasured launch group is not entered");
    }

    #[test]
    fn the_watch_ends_once_the_coin_is_too_old_or_too_deep() {
        let s = Sprint::default();
        assert!(!window_over(&s, Some(100), 45.0));
        assert!(window_over(&s, Some(121), 45.0));
        assert!(window_over(&s, Some(80), 61.0));
        assert!(!window_over(&s, None, 20.0), "unknown age keeps watching until the 2 minute cap");
    }

    #[test]
    fn buyers_are_distinct_wallets_that_bought() {
        let t = |u: &str, b: bool| Trade { user: u.into(), is_buy: b, ..Default::default() };
        assert_eq!(distinct_buyers(&[t("a", true), t("a", true), t("b", true), t("c", false)]), 2);
    }
}
