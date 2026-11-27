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
    /// Creator sales in the window as % of total supply.
    pub dev_sold_pct: f64,
    /// Creator sales as % of what the creator held (launch buy plus buys in the window). 100 if they sold with nothing known to sell.
    pub dev_sold_pct_of_holding: f64,
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
pub fn observe(trades: &[Trade], creator: &str, creator_initial_tokens: f64, launch_price: f64, launch_real_sol: f64, age_secs: f64, now_ms: i64) -> Observation {
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
    let dev_sold_tokens: f64 = sells.iter().filter(|t| t.user == creator).map(|t| t.tokens).sum();
    let dev_held: f64 = creator_initial_tokens + buys.iter().filter(|t| t.user == creator).map(|t| t.tokens).sum::<f64>();
    let dev_of_holding = if dev_sold_tokens <= 0.0 { 0.0 } else if dev_held <= 0.0 { 100.0 } else { (dev_sold_tokens / dev_held * 100.0).min(100.0) };
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
        dev_sold_pct_of_holding: dev_of_holding,
    }
}

fn frac(v: f64, target: f64) -> f64 {
    if target <= 0.0 { 1.0 } else { (v / target).clamp(0.0, 1.0) }
}

/// Starting weights of the five score parts (they add up to 100). The learning review may move each by up to 20%.
pub const DEFAULT_WEIGHTS: [f64; 5] = [25.0, 25.0, 20.0, 15.0, 15.0];

pub fn score(o: &Observation, cfg: &Config) -> Scored {
    score_with(o, cfg, &DEFAULT_WEIGHTS)
}

/// Same scoring with explicit weights [ratio, buyers, price change, volume slope, wallets]. The result is scaled so a
/// perfect coin is always 100, whatever the weights add up to.
pub fn score_with(o: &Observation, cfg: &Config, w: &[f64; 5]) -> Scored {
    let ob = &cfg.observation;
    let scale = { let t: f64 = w.iter().sum(); if t > 0.0 { 100.0 / t } else { 1.0 } };
    let en = &cfg.entry;
    let slope_ratio = if o.buys_first_half == 0 { if o.buys_second_half > 0 { 1.0 } else { 0.0 } } else { o.buys_second_half as f64 / o.buys_first_half as f64 };
    let parts = vec![
        ("buy/sell ratio", w[0] * scale * frac(o.ratio, ob.min_buy_sell_ratio)),
        ("unique buyers", w[1] * scale * frac(o.unique_buyers as f64, ob.min_unique_buyers as f64)),
        ("price change", w[2] * scale * frac(o.price_change_pct, ob.min_price_change_pct)),
        ("volume slope", w[3] * scale * slope_ratio.clamp(0.0, 1.0)),
        ("wallets", w[4] * scale * frac(o.wallets as f64, en.min_holders as f64)),
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
    if o.mcap_sol < cfg.feed.min_market_cap_sol {
        gates.push(format!("market cap {:.1} SOL below {:.1} SOL", o.mcap_sol, cfg.feed.min_market_cap_sol));
    }
    // Tiered: small creator sales are tolerated (SPEC 6). Allowed when sold <= 1% of supply AND < 25% of their own holding.
    if o.dev_sold_pct > en.max_creator_sold_pct_of_supply || o.dev_sold_pct_of_holding >= en.max_creator_sold_pct_of_holding {
        gates.push(format!("creator is selling ({:.2}% of supply, {:.0}% of their holding)", o.dev_sold_pct, o.dev_sold_pct_of_holding));
    }
    let picked = total >= ob.min_score && gates.is_empty();
    Scored { score: (total * 10.0).round() / 10.0, parts, gates, picked }
}

/// Paper-only variant `dip-40pct` (SPEC 6): buy a coin that already passed the observation gate after it fell
/// 40-50% from its peak while buy volume is expanding again (last 15 s of buying above the 15 s before, with at
/// least 2 buys in the last 15 s). Returns the dip in percent when the signal fires.
pub fn dip_signal(trades: &[Trade], now_ms: i64, cfg: &Config) -> Option<f64> {
    let v = &cfg.entry.paper_variant;
    let peak = trades.iter().map(|t| t.price()).fold(0.0, f64::max);
    let last = trades.iter().max_by_key(|t| (t.slot, t.recv_ms))?;
    if peak <= 0.0 || last.price() <= 0.0 {
        return None;
    }
    let dip = (1.0 - last.price() / peak) * 100.0;
    if dip < v.min_dip_pct || dip > v.max_dip_pct {
        return None;
    }
    if v.confirm_volume_expanding {
        let vol = |from: i64, to: i64| trades.iter().filter(|t| t.is_buy && t.recv_ms >= from && t.recv_ms < to).map(|t| t.sol).sum::<f64>();
        let recent = vol(now_ms - 15_000, now_ms + 1);
        let before = vol(now_ms - 30_000, now_ms - 15_000);
        let n = trades.iter().filter(|t| t.is_buy && now_ms - t.recv_ms <= 15_000).count();
        if n < 2 || recent <= before {
            return None;
        }
    }
    Some(dip)
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
        let o = observe(&t, "creator", 0.0, launch_price, 0.0, 60.0, 61_000);
        assert!(o.unique_buyers >= 12 && o.ratio >= 2.0 && o.price_change_pct > 5.0);
        let s = score(&o, &cfg());
        assert!(s.score >= 80.0, "score {} parts {:?}", s.score, s.parts);
        assert!(s.gates.is_empty(), "{:?}", s.gates);
        assert!(s.picked);
    }

    #[test]
    fn market_cap_gate_is_in_sol_and_fits_under_the_chase_rule() {
        use crate::shield::rules::chase_rule;
        use crate::shield::rules::Outcome;
        let c = cfg();
        assert_eq!(c.feed.min_market_cap_sol, 30.0);
        // A fresh launch starts near 28 SOL. A coin at 45 SOL after 60 s (1.6x) clears both rules.
        let launch = 27.96;
        let mut o = observe(&hot_trades(), "creator", 0.0, 30.0 / 1.073e9, 0.0, 60.0, 61_000);
        o.mcap_sol = 45.0;
        let s = score(&o, &c);
        assert!(!s.gates.iter().any(|g| g.contains("market cap")), "{:?}", s.gates);
        assert_eq!(chase_rule(launch, o.mcap_sol).outcome, Outcome::Pass);
        // Below the minimum is gated, and the number never depends on any dollar price.
        o.mcap_sol = 29.9;
        assert!(score(&o, &c).gates.iter().any(|g| g.contains("market cap")));
        // The whole passing band (30 to 3x launch) is non-empty.
        assert!(c.feed.min_market_cap_sol < 3.0 * launch);
        // And the gate is the same at any SOL price: score() takes no price argument.
        o.mcap_sol = 30.0;
        assert!(!score(&o, &c).gates.iter().any(|g| g.contains("market cap")));
        assert_eq!(chase_rule(launch, 30.0).outcome, Outcome::Pass);
    }

    #[test]
    fn dead_token_scores_low() {
        let o = observe(&[], "creator", 0.0, 30.0 / 1.073e9, 0.0, 60.0, 61_000);
        let s = score(&o, &cfg());
        assert!(s.score < 10.0);
        assert!(!s.picked);
        assert!(!s.gates.is_empty());
    }

    #[test]
    fn young_token_fails_age_gate_even_with_high_score() {
        let t = hot_trades();
        let o = observe(&t, "creator", 0.0, 30.0 / 1.073e9, 0.0, 30.0, 61_000);
        let s = score(&o, &cfg());
        assert!(s.gates.iter().any(|g| g.contains("younger")));
        assert!(!s.picked);
    }

    #[test]
    fn creator_selling_blocks_pick() {
        let mut t = hot_trades();
        t.push(tr("creator", false, 45_000, 50.0, 20.0));
        let o = observe(&t, "creator", 0.0, 30.0 / 1.073e9, 0.0, 60.0, 61_000);
        assert!(o.dev_sold_pct > 0.0);
        let s = score(&o, &cfg());
        assert!(s.gates.iter().any(|g| g.contains("creator is selling")));
        assert!(!s.picked);
    }

    #[test]
    fn score_parts_sum_to_total_and_cap_at_100() {
        let o = Observation { age_secs: 60.0, buys: 100, sells: 1, unique_buyers: 100, buyers_30s: 50, wallets: 100, ratio: 100.0, price_change_pct: 500.0, curve_sol: 20.0, mcap_sol: 100.0, buys_first_half: 10, buys_second_half: 90, dev_sold_pct: 0.0, dev_sold_pct_of_holding: 0.0 };
        let s = score(&o, &cfg());
        assert_eq!(s.score, 100.0);
        assert!((s.parts.iter().map(|p| p.1).sum::<f64>() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn dip_signal_needs_a_40_to_50_percent_dip_and_expanding_volume() {
        let mut t = hot_trades(); // price rises to the peak
        let peak = t.iter().map(|x| x.price()).fold(0.0, f64::max);
        // price falls ~45% from the peak, then buying picks up in the last 15 s
        let mut dip = tr("d0", false, 100_000, 30.0, 10.0);
        dip.v_sol = 40.0;
        dip.v_tokens = dip.v_sol / (peak * 0.55);
        t.push(dip);
        for i in 0..3 {
            let mut b = tr(&format!("nb{i}"), true, 110_000 + i * 2000, 30.0, 10.0);
            b.v_sol = 40.0;
            b.v_tokens = b.v_sol / (peak * 0.55);
            b.sol = 1.0;
            t.push(b);
        }
        let d = dip_signal(&t, 118_000, &cfg()).expect("dip with expanding volume");
        assert!((d - 45.0).abs() < 0.5, "{d}");
        // no expanding volume: same dip but the buys are older than 30 s
        assert_eq!(dip_signal(&t, 200_000, &cfg()), None);
        // too shallow a dip
        let mut shallow = hot_trades();
        let last = shallow.last_mut().unwrap();
        last.v_tokens = last.v_sol / (peak * 0.9);
        assert_eq!(dip_signal(&shallow, 61_000, &cfg()), None);
    }

    fn dev_obs(sold_tokens: f64, initial_tokens: f64) -> Observation {
        let mut t = hot_trades();
        if sold_tokens > 0.0 {
            let mut x = tr("creator", false, 45_000, 50.0, 20.0);
            x.tokens = sold_tokens;
            t.push(x);
        }
        observe(&t, "creator", initial_tokens, 30.0 / 1.073e9, 0.0, 60.0, 61_000)
    }

    fn creator_gate(o: &Observation) -> bool {
        score(o, &cfg()).gates.iter().any(|g| g.contains("creator is selling"))
    }

    #[test]
    fn creator_selling_is_tiered_one_percent_of_supply_and_under_25_percent_of_holding() {
        let c = cfg();
        assert_eq!((c.entry.max_creator_sold_pct_of_supply, c.entry.max_creator_sold_pct_of_holding), (1.0, 25.0));
        let supply = crate::pump::SUPPLY_TOKENS;
        let init = supply * 0.08; // creator launched with 8% of supply
        // no sale: passes
        assert!(!creator_gate(&dev_obs(0.0, init)));
        // 10% of their bag = 0.8% of supply: passes both limits
        assert!(!creator_gate(&dev_obs(init * 0.10, init)));
        // 24.9% of the bag = 1.99% of supply: over the 1% of supply limit, blocked
        assert!(creator_gate(&dev_obs(init * 0.249, init)));
        // small bag: 3% of supply launch, sells 0.9% of supply = 30% of bag: blocked by the holding limit
        assert!(creator_gate(&dev_obs(supply * 0.009, supply * 0.03)));
        // small bag, small sale: 3% of supply launch, sells 0.5% of supply = 16.7% of bag: passes
        assert!(!creator_gate(&dev_obs(supply * 0.005, supply * 0.03)));
        // boundaries: exactly 1.0% of supply passes (needs a big enough bag), exactly 25% of the bag is blocked
        assert!(!creator_gate(&dev_obs(supply * 0.01, supply * 0.10)));
        assert!(creator_gate(&dev_obs(supply * 0.0100001, supply * 0.10)));
        assert!(creator_gate(&dev_obs(supply * 0.0075, supply * 0.03)));
        assert!(!creator_gate(&dev_obs(supply * 0.0074, supply * 0.03)));
        // sold with nothing known to sell (holding unknown): treated as 100% of the bag, blocked
        assert!(creator_gate(&dev_obs(1_000_000.0, 0.0)));
    }
}
