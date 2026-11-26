//! Scripted market scenarios. They feed the SAME engine (picker, pre-buy checks, risk gates, paper executor,
//! exit loop, JSONL) through the trade store instead of the live feeds, so a full trade can be shown even when
//! the live market offers no pick. Everything they produce is labelled `replay` in the UI and the logs.
//! The curve is simulated with the real constant-product math; no randomness, so a restart continues the same story.

use crate::curve::Curve;
use crate::pump::{FeedMsg, Trade};

pub struct Scenario {
    pub name: &'static str,
    pub create: FeedMsg,
    /// (offset from creation in ms, trade). Sorted by offset.
    pub events: Vec<(i64, Trade)>,
}

pub const MINT: &str = "ReplayHot1111111111111111111111111111111pump";

pub struct Sim {
    pub c: Curve,
    mint: String,
    pub out: Vec<(i64, Trade)>,
    n: u64,
}

impl Sim {
    pub fn new(mint: &str) -> Sim {
        Sim { c: Curve { v_sol: 30.0, v_tokens: 1_073_000_000.0, real_sol: 0.0 }, mint: mint.into(), out: vec![], n: 0 }
    }
    pub fn trade(&mut self, secs: f64, user: &str, buy: bool, sol: f64) {
        let tokens;
        if buy {
            let (t, after) = self.c.buy(sol);
            tokens = t;
            self.c = after;
        } else {
            // tokens the seller must put in to receive `sol` from the curve
            let k = self.c.v_sol * self.c.v_tokens;
            let t = k / (self.c.v_sol - sol) - self.c.v_tokens;
            let (_, after) = self.c.sell(t);
            tokens = t;
            self.c = after;
        }
        self.n += 1;
        let off = (secs * 1000.0) as i64;
        self.out.push((
            off,
            Trade {
                mint: self.mint.clone(),
                user: user.into(),
                is_buy: buy,
                sol,
                tokens,
                slot: 0, // assigned in time order below (distinct slots: no fake bundles)
                signature: format!("replay-{}-{}", self.mint, self.n),
                recv_ms: 0,
                v_sol: self.c.v_sol,
                v_tokens: self.c.v_tokens,
                real_sol: self.c.real_sol,
            },
        ));
    }
}

pub fn scenario(name: &str) -> Option<Scenario> {
    let hot = match name {
        "hot" => true,
        "dump" => false,
        _ => return None,
    };
    let mint = if hot { MINT.to_string() } else { "ReplayDump111111111111111111111111111111pump".to_string() };
    let mut s = Sim::new(&mint);
    // Observation window (0-60 s): 18 distinct buyers, the second half busier than the first, 2 small sells.
    for (i, t) in [3.0, 8.0, 13.0, 18.0, 23.0, 28.0].iter().enumerate() {
        s.trade(*t, &format!("EarlyBuyer{i:02}"), true, 0.5);
    }
    s.trade(20.0, "SellerA", false, 0.15);
    for i in 0..12 {
        s.trade(31.0 + i as f64 * 2.4, &format!("LateBuyer{i:02}"), true, 0.9);
    }
    s.trade(45.0, "SellerB", false, 0.15);
    if hot {
        // 62-90 s: steady buying lifts the price through +20% and +40%; 90-105 s: through +60% to the peak.
        for i in 0..10 {
            s.trade(62.0 + i as f64 * 3.0, &format!("Runner{i:02}"), true, 1.0);
        }
        for i in 0..8 {
            s.trade(91.0 + i as f64 * 2.0, &format!("Chaser{i:02}"), true, 1.0);
        }
        // 107 s on: small sells (2.4% of the curve each, under the whale limit) take the price off its peak.
        for i in 0..6 {
            s.trade(107.0 + i as f64 * 2.0, &format!("Taker{i:02}"), false, 0.8);
        }
    } else {
        // after the pick the price slides: small sells every 2 s (a slow bleed to the hard stop)
        for i in 0..10 {
            s.trade(64.0 + i as f64 * 2.0, &format!("Dumper{i:02}"), false, 0.8);
        }
        s.trade(90.0, "LateBuyer99", true, 0.3);
    }
    s.out.sort_by_key(|e| e.0);
    for (i, e) in s.out.iter_mut().enumerate() {
        e.1.slot = 400_000_000 + (e.0 / 400) as u64 + i as u64;
    }
    let v0 = FeedMsg {
        mint: mint.clone(),
        trader_public_key: "ReplayCreator11111111111111111111111111111".into(),
        tx_type: "create".into(),
        bonding_curve_key: "ReplayCurve1111111111111111111111111111111".into(),
        v_tokens_in_bonding_curve: 1_073_000_000.0,
        v_sol_in_bonding_curve: 30.0,
        market_cap_sol: 27.96,
        name: if hot { "Replay Hot Coin".into() } else { "Replay Dump Coin".into() },
        symbol: if hot { "RHOT".into() } else { "RDMP".into() },
        pool: "pump".into(),
        ..Default::default()
    };
    Some(Scenario { name: if hot { "hot" } else { "dump" }, create: v0, events: s.out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{picker, Config};

    fn cfg() -> Config {
        Config::from_json(include_str!("../../config.json")).unwrap()
    }

    /// Trades up to `secs` as the engine would see them if the scenario started at `t0`.
    fn upto(sc: &Scenario, secs: i64, t0: i64) -> Vec<Trade> {
        sc.events.iter().filter(|(o, _)| *o <= secs * 1000).map(|(o, t)| Trade { recv_ms: t0 + o, ..t.clone() }).collect()
    }

    #[test]
    fn hot_scenario_is_picked_by_the_real_picker_at_60_seconds() {
        let sc = scenario("hot").unwrap();
        let t0 = 1_000_000;
        let trades = upto(&sc, 59, t0);
        let launch = 30.0 / 1.073e9;
        let o = picker::observe(&trades, &sc.create.trader_public_key, launch, 0.0, 60.0, t0 + 60_000);
        let s = picker::score(&o, &cfg(), 150.0);
        assert!(s.picked, "score {} parts {:?} gates {:?} obs {:?}", s.score, s.parts, s.gates, o);
        assert!(s.score >= 80.0);
    }

    #[test]
    fn hot_scenario_walks_through_the_whole_ladder_and_trailing() {
        let sc = scenario("hot").unwrap();
        let price_at = |secs: i64| upto(&sc, secs, 0).last().unwrap().price();
        let entry = price_at(62);

        let peak = sc.events.iter().map(|e| e.1.price()).fold(0.0, f64::max);
        assert!(peak / entry > 1.9, "peak {:.2}x entry", peak / entry);
        assert!(price_at(78) / entry > 1.2 && price_at(92) / entry > 1.4 && price_at(100) / entry > 1.6);
        let last = sc.events.last().unwrap().1.price();
        assert!(last < peak * 0.88, "ends more than 10% off the peak");
        // no whale-size sells: each is under 3% of the curve
        for (o, t) in &sc.events {
            if !t.is_buy && *o > 62_000 {
                assert!(t.sol / (t.real_sol + t.sol) * 100.0 < 3.0, "{} of {}", t.sol, t.real_sol);
            }
        }
        assert!(sc.events.iter().all(|(_, t)| t.real_sol < 76.0), "never near graduation");
    }

    #[test]
    fn dump_scenario_is_picked_then_falls_through_the_hard_stop() {
        let sc = scenario("dump").unwrap();
        let t0 = 5_000;
        let trades = upto(&sc, 59, t0);
        let o = picker::observe(&trades, &sc.create.trader_public_key, 30.0 / 1.073e9, 0.0, 60.0, t0 + 60_000);
        assert!(picker::score(&o, &cfg(), 150.0).picked);
        let entry = upto(&sc, 62, 0).last().unwrap().price();
        let low = sc.events.iter().map(|e| e.1.price()).fold(f64::MAX, f64::min);
        assert!(low / entry < 0.80, "{}", low / entry);
    }
}
