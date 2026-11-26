//! Token Picker: observation-window scoring (SPEC 6 `observation` + `entry`).
//!
//! Score 0-100 from five parts, each at full weight once its config threshold is met and proportional below:
//!   buy/sell ratio 25, unique buyers 25, price change 20, rising volume slope 15, distinct wallets (holders proxy) 15.
//! The weights are Claude's starting values (tuned in paper mode). A token is "picked" only if the score reaches
//! `observation.minScore` AND every entry gate holds.

use crate::config::Config;
use crate::pump::Trade;
use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observation {
    pub age_secs: f64,
    pub buys: u32,
    pub sells: u32,
    pub unique_buyers: u32,
    pub buyers_30s: u32,
    pub wallets: u32,
    pub ratio: f64,
    pub price_change_pct: f64,
    pub curve_sol: f64,
    pub mcap_sol: f64,
    pub buys_first_half: u32,
    pub buys_second_half: u32,
    pub dev_sold_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub score: f64,
    pub parts: Vec<(&'static str, f64)>,
    /// Entry gates that failed (plain language). Empty = all gates hold.
    pub gates: Vec<String>,
    pub picked: bool,
}

/// Summarise the trades seen for one token. `launch_price` and `launch_curve_sol` come from the create message.
pub fn observe(trades: &[Trade], creator: &str, launch_price: f64, launch_real_sol: f64, age_secs: f64, now_ms: i64) -> Observation {
    let buys: Vec<&Trade> = trades.iter().filter(|t| t.is_buy).collect();
    let sells: Vec<&Trade> = trades.iter().filter(|t| !t.is_buy).collect();
    let uniq: HashSet<&str> = buys.iter().map(|t| t.user.as_str()).collect();
    let b30: HashSet<&str> = buys.iter().filter(|t| now_ms - t.recv_ms <= 30_000).map(|t| t.user.as_str()).collect();
    let wallets: HashSet<&str> = trades.iter().map(|t| t.user.as_str()).collect();
    let half = now_ms - (age_secs * 500.0) as i64; // midpoint of the observed window
    let first = buys.iter().filter(|t| t.recv_ms < half).count() as u32;
    let second = buys.iter().filter(|t| t.recv_ms >= half).count() as u32;
    let last = trades.iter().max_by_key(|t| (t.slot, t.recv_ms));
    let price_change = match last {
        Some(l) if launch_price > 0.0 => (l.price() / launch_price - 1.0) * 100.0,
        _ => 0.0,
    };
    let curve_sol = last.map(|l| l.real_sol).unwrap_or(launch_real_sol);
    let mcap = last.map(|l| l.mcap_sol()).unwrap_or(0.0);
    let ratio = if sells.is_empty() { buys.len() as f64 } else { buys.len() as f64 / sells.len() as f64 };
    // creator sells, as % of the tokens they bought in the window is unknown; use share of supply sold
    let dev_sold: f64 = sells.iter().filter(|t| t.user == creator).map(|t| t.tokens).sum::<f64>() / crate::pump::SUPPLY_TOKENS * 100.0;
    Observation {
        age_secs,
        buys: buys.len() as u32,
        sells: sells.len() as u32,
        unique_buyers: uniq.len() as u32,
        buyers_30s: b30.len() as u32,
        wallets: wallets.len() as u32,
        ratio,
        price_change_pct: price_change,
        curve_sol,
        mcap_sol: mcap,
        buys_first_half: first,
        buys_second_half: second,
        dev_sold_pct: dev_sold,
    }
}

fn frac(v: f64, target: f64) -> f64 {
    if target <= 0.0 { 1.0 } else { (v / target).clamp(0.0, 1.0) }
}

pub fn score(o: &Observation, cfg: &Config, sol_usd: f64) -> Scored {
    let ob = &cfg.observation;
    let en = &cfg.entry;
    let slope_ratio = if o.buys_first_half == 0 { if o.buys_second_half > 0 { 1.0 } else { 0.0 } } else { o.buys_second_half as f64 / o.buys_first_half as f64 };
    let parts = vec![
        ("buy/sell ratio", 25.0 * frac(o.ratio, ob.min_buy_sell_ratio)),
        ("unique buyers", 25.0 * frac(o.unique_buyers as f64, ob.min_unique_buyers as f64)),
        ("price change", 20.0 * frac(o.price_change_pct, ob.min_price_change_pct)),
        ("volume slope", 15.0 * slope_ratio.clamp(0.0, 1.0)),
        ("wallets", 15.0 * frac(o.wallets as f64, en.min_holders as f64)),
    ];
    let total: f64 = parts.iter().map(|p| p.1).sum();
    let mut gates = vec![];
    if o.age_secs < en.min_age_seconds as f64 {
        gates.push(format!("younger than {} s", en.min_age_seconds));
    }
    if o.curve_sol < en.curve_sol_min || o.curve_sol > en.curve_sol_max {
        gates.push(format!("curve holds {:.1} SOL (needs {}-{})", o.curve_sol, en.curve_sol_min, en.curve_sol_max));
    }
    if o.ratio < en.min_buy_sell_ratio {
        gates.push(format!("buy/sell {:.1} below {}", o.ratio, en.min_buy_sell_ratio));
    }
    if o.buyers_30s < en.min_unique_buyers30s {
        gates.push(format!("{} buyers in 30 s (needs {})", o.buyers_30s, en.min_unique_buyers30s));
    }
    if en.require_rising_volume_slope && o.buys_second_half <= o.buys_first_half {
        gates.push("volume not rising".into());
    }
    if o.mcap_sol * sol_usd < cfg.feed.min_market_cap_usd {
        gates.push(format!("market cap ${:.0} below ${:.0}", o.mcap_sol * sol_usd, cfg.feed.min_market_cap_usd));
    }
    if o.dev_sold_pct > 0.0 {
        gates.push("creator is selling".into());
    }
    let picked = total >= ob.min_score && gates.is_empty();
    Scored { score: (total * 10.0).round() / 10.0, parts, gates, picked }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    fn tr(user: &str, buy: bool, ms: i64, v_sol: f64, real: f64) -> Trade {
        Trade { mint: "m".into(), user: user.into(), is_buy: buy, sol: 0.5, tokens: 1_000_000.0, slot: (ms / 400) as u64, signature: format!("s{ms}{user}"), recv_ms: ms, v_sol, v_tokens: 1.073e9 * 30.0 / v_sol, real_sol: real }
    }

    /// 12 distinct buyers, 2 sells, price up 40%, second half busier, 20 SOL in curve.
    fn hot_trades() -> Vec<Trade> {
        let mut v = vec![];
        for i in 0..4 {
            v.push(tr(&format!("b{i}"), true, 1_000 + i * 1000, 31.0 + i as f64, 1.0 + i as f64));
        }
        for i in 4..16 {
            v.push(tr(&format!("b{i}"), true, 31_000 + (i - 4) * 2000, 40.0 + i as f64, 8.0 + i as f64));
        }
        v.push(tr("s1", false, 40_000, 55.0, 24.0));
        v.push(tr("s2", false, 50_000, 56.0, 25.0));
        v
    }

    #[test]
    fn hot_token_scores_high_and_is_picked() {
        let t = hot_trades();
        let launch_price = 30.0 / 1.073e9;
        let o = observe(&t, "creator", launch_price, 0.0, 60.0, 61_000);
        assert!(o.unique_buyers >= 12 && o.ratio >= 2.0 && o.price_change_pct > 5.0);
        let s = score(&o, &cfg(), 150.0);
        assert!(s.score >= 80.0, "score {} parts {:?}", s.score, s.parts);
        assert!(s.gates.is_empty(), "{:?}", s.gates);
        assert!(s.picked);
    }

    #[test]
    fn dead_token_scores_low() {
        let o = observe(&[], "creator", 30.0 / 1.073e9, 0.0, 60.0, 61_000);
        let s = score(&o, &cfg(), 150.0);
        assert!(s.score < 10.0);
        assert!(!s.picked);
        assert!(!s.gates.is_empty());
    }

    #[test]
    fn young_token_fails_age_gate_even_with_high_score() {
        let t = hot_trades();
        let o = observe(&t, "creator", 30.0 / 1.073e9, 0.0, 30.0, 61_000);
        let s = score(&o, &cfg(), 150.0);
        assert!(s.gates.iter().any(|g| g.contains("younger")));
        assert!(!s.picked);
    }

    #[test]
    fn creator_selling_blocks_pick() {
        let mut t = hot_trades();
        t.push(tr("creator", false, 45_000, 50.0, 20.0));
        let o = observe(&t, "creator", 30.0 / 1.073e9, 0.0, 60.0, 61_000);
        assert!(o.dev_sold_pct > 0.0);
        let s = score(&o, &cfg(), 150.0);
        assert!(s.gates.iter().any(|g| g.contains("creator is selling")));
        assert!(!s.picked);
    }

    #[test]
    fn score_parts_sum_to_total_and_cap_at_100() {
        let o = Observation { age_secs: 60.0, buys: 100, sells: 1, unique_buyers: 100, buyers_30s: 50, wallets: 100, ratio: 100.0, price_change_pct: 500.0, curve_sol: 20.0, mcap_sol: 100.0, buys_first_half: 10, buys_second_half: 90, dev_sold_pct: 0.0 };
        let s = score(&o, &cfg(), 150.0);
        assert_eq!(s.score, 100.0);
        assert!((s.parts.iter().map(|p| p.1).sum::<f64>() - 100.0).abs() < 1e-9);
    }
}
