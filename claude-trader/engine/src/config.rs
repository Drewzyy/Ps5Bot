use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub mode: Mode,
    pub paper: Paper,
    pub feed: Feed,
    pub filters: Filters,
    pub observation: Observation,
    pub entry: Entry,
    pub exits: Exits,
    pub fees: Fees,
    pub paper_fill: PaperFill,
    pub risk: Risk,
    pub learning: Learning,
    /// X Monitor and Thesis Finder (SPEC 4e). The weights are starting guesses, not measured values.
    #[serde(default)]
    pub x_monitor: XMonitor,
}

/// X Monitor settings. Every number is a starting guess from research (SPEC 4e) with no tested backing; the journal logs
/// every component so the weights can be tuned once there are enough paper trades.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct XMonitor {
    pub weights: XWeights,
    /// Points added to the token score per community point (0.15 means a community score of 100 adds 15).
    pub points_factor: f64,
    /// Never more than this many points, and never above 15 (checked at start): X alone can not buy.
    pub max_points: f64,
    /// Block when this share of the sampled replies looks like bots.
    pub block_bot_reply_pct: f64,
    /// Smallest reply sample that counts for the bot test and the reply score.
    pub min_reply_sample: u32,
    /// Block when this coin is later than this among coins linking the same tweet or community.
    pub max_clone_rank: u32,
    /// Block when more than this many coins link the same tweet or community.
    pub max_clones: u32,
    /// How far back counts as "the same story" for copy ranking (hours).
    pub clone_window_hours: u32,
    /// A tweet author with at least this many followers is a big account (the tweet must then name the coin).
    pub big_account_followers: u64,
    /// One smart follower with this many followers counts as a top-tier KOL.
    pub top_tier_followers: u64,
    /// Several coins from the same tweet or author within the window make a trending narrative.
    pub trend_min_coins: u32,
    pub trend_window_minutes: u32,
    /// X data older than this is stale: it counts as unknown, never as a pass.
    pub max_age_minutes: u32,
    /// A queued X job that has not started after this long is dropped as stale.
    pub job_wait_secs: u32,
    /// Block a coin that has no X link at all (default off: no link just means no X points).
    pub block_when_no_x_link: bool,
    pub pacing: XPacing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct XWeights {
    pub kol: f64,
    pub engagement: f64,
    pub replies: f64,
    pub growth: f64,
    pub integrity: f64,
    pub community: f64,
    pub narrative: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct XPacing {
    pub min_gap_secs: u32,
    pub max_gap_secs: u32,
    pub max_loads_per_hour: u32,
    pub cache_minutes: u32,
}

impl Default for XWeights {
    fn default() -> Self {
        XWeights { kol: 30.0, engagement: 20.0, replies: 10.0, growth: 10.0, integrity: 10.0, community: 10.0, narrative: 10.0 }
    }
}

impl Default for XPacing {
    fn default() -> Self {
        XPacing { min_gap_secs: 8, max_gap_secs: 15, max_loads_per_hour: 120, cache_minutes: 30 }
    }
}

impl Default for XMonitor {
    fn default() -> Self {
        XMonitor {
            weights: XWeights::default(),
            points_factor: 0.15,
            max_points: 15.0,
            block_bot_reply_pct: 30.0,
            min_reply_sample: 8,
            max_clone_rank: 5,
            max_clones: 10,
            clone_window_hours: 24,
            big_account_followers: 50_000,
            top_tier_followers: 200_000,
            trend_min_coins: 3,
            trend_window_minutes: 60,
            max_age_minutes: 30,
            job_wait_secs: 120,
            block_when_no_x_link: false,
            pacing: XPacing::default(),
        }
    }
}

/// The learning journal and the review loop (see learning.rs). The hard limits (10 trades minimum,
/// 20% change cap, never touching Scam Shield, trade size or loss limits) are not settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Learning {
    pub enabled: bool,
    pub review_every_trades: u32,
    pub shadow_trades: u32,
    pub min_sample: u32,
    pub min_win_rate_gap_pts: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Paper,
    Live,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Paper {
    pub bankroll_sol: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Feed {
    pub min_market_cap_sol: f64,
    pub launchpads: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filters {
    pub mint_authority_revoked: bool,
    pub freeze_authority_revoked: bool,
    pub max_creator_holding_pct: f64,
    pub max_insiders_pct: f64,
    pub max_single_holder_pct: f64,
    pub max_top_holders_combined_pct: f64,
    pub exclude_curve_and_pool_accounts: bool,
    pub reject_dev_snipe_first_block: bool,
    pub reject_bundle: bool,
    /// When true, a coin RugCheck cannot report on is blocked (the old behavior). When false (default), the app
    /// falls back to its own on-chain checks and buys at half size (SPEC 11b #29). A RugCheck danger flag always blocks.
    #[serde(default)]
    pub require_rugcheck: bool,
    pub require_community_checks: CommunityChecks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommunityChecks {
    pub min_social_links: u32,
    pub require_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub min_score: f64,
    pub min_unique_buyers: u32,
    pub min_buy_sell_ratio: f64,
    pub min_price_change_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub style: String,
    pub min_buy_sell_ratio: f64,
    pub min_unique_buyers30s: u32,
    pub require_rising_volume_slope: bool,
    pub curve_sol_min: f64,
    pub curve_sol_max: f64,
    pub min_holders: u32,
    /// Entry gate: the creator may have sold at most this share of the total supply during the observation window.
    pub max_creator_sold_pct_of_supply: f64,
    /// Entry gate: and must have sold less than this share of their own holding (launch buy plus window buys).
    pub max_creator_sold_pct_of_holding: f64,
    pub min_age_seconds: u32,
    pub paper_variant: PaperVariant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaperVariant {
    pub style: String,
    pub min_dip_pct: f64,
    pub max_dip_pct: f64,
    pub confirm_volume_expanding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tranche {
    pub at_pct: f64,
    pub sell_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StallExit {
    pub from_seconds: u32,
    pub to_seconds: u32,
    pub min_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BleederExit {
    pub after_seconds: u32,
    pub below_pct: f64,
    pub peak_never_above_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VolumeTaper {
    pub window_seconds: u32,
    pub declining_windows: u32,
    pub last_below_baseline_pct: f64,
    pub sell_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MomentumDeath {
    pub after_hold_seconds: u32,
    pub lookback_seconds: u32,
    pub min_buys_per_minute: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exits {
    pub tranches: Vec<Tranche>,
    pub trailing_stop_pct: f64,
    pub soft_stop_pct: f64,
    pub soft_stop_recovery_seconds: u32,
    pub hard_stop_pct: f64,
    pub sell_below_average_entry: bool,
    pub max_hold_seconds: u32,
    pub stall_exit: StallExit,
    pub bleeder_exit: BleederExit,
    pub volume_taper: VolumeTaper,
    pub momentum_death: MomentumDeath,
    pub whale_sell_pct_of_curve: f64,
    pub exit_on_dev_sell: bool,
    pub sell_on_migration: bool,
    pub exit_loop_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fees {
    pub model_pump_fun_pct_per_side: f64,
    /// Part of `model_pump_fun_pct_per_side` that goes to the token creator (the rest is the protocol fee).
    pub pump_fun_creator_pct_per_side: f64,
    pub pump_portal_pct_per_side: f64,
    pub priority_micro_lamports: u64,
    pub compute_units: u64,
    pub pump_portal_priority_fee_sol: f64,
    pub pump_portal_priority_fee_cap_sol: f64,
    /// Jito bundle tip per transaction. 0 = not used (PumpPortal local trades land with the priority fee only).
    pub jito_tip_sol: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaperFill {
    pub buy_fail_pct: f64,
    pub sell_fail_pct: f64,
    pub delay_ms_min: u32,
    pub delay_ms_max: u32,
    pub slippage_cap_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Risk {
    pub max_pct_bankroll_per_trade: f64,
    pub min_trade_sol: f64,
    pub max_trade_sol: f64,
    pub max_open_positions: u32,
    pub max_total_exposure_pct: f64,
    pub daily_loss_limit_pct: f64,
    pub consecutive_loss_stop: u32,
    pub reduce_size_after_wins: u32,
    pub cooldown_seconds_per_token: u32,
    pub live_min_sol_reserve: f64,
}

impl Config {
    pub fn from_json(s: &str) -> Result<Config, String> {
        let c: Config = serde_json::from_str(s).map_err(|e| format!("config.json is not valid: {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn load(path: &std::path::Path) -> Result<Config, String> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        Config::from_json(&s)
    }

    /// Sanity checks. Plain-language messages.
    pub fn validate(&self) -> Result<(), String> {
        if self.paper.bankroll_sol <= 0.0 {
            return Err("paper.bankrollSol must be above 0".into());
        }
        if self.fees.pump_fun_creator_pct_per_side < 0.0 || self.fees.pump_fun_creator_pct_per_side > self.fees.model_pump_fun_pct_per_side {
            return Err("fees.pumpFunCreatorPctPerSide must be between 0 and fees.modelPumpFunPctPerSide".into());
        }
        if self.risk.min_trade_sol > self.risk.max_trade_sol {
            return Err("risk.minTradeSol cannot be above risk.maxTradeSol".into());
        }
        if self.exits.hard_stop_pct >= 0.0 || self.exits.soft_stop_pct >= 0.0 {
            return Err("stop levels must be negative numbers".into());
        }
        if self.exits.hard_stop_pct > self.exits.soft_stop_pct {
            return Err("exits.hardStopPct must be at or below exits.softStopPct".into());
        }
        if self.exits.tranches.is_empty() {
            return Err("exits.tranches needs at least one step".into());
        }
        let total: f64 = self.exits.tranches.iter().map(|t| t.sell_pct).sum();
        if total > 100.0 {
            return Err("exits.tranches sell more than 100% in total".into());
        }
        if self.exits.max_hold_seconds == 0 {
            return Err("exits.maxHoldSeconds must be above 0".into());
        }
        if self.entry.curve_sol_min > self.entry.curve_sol_max {
            return Err("entry.curveSolMin cannot be above entry.curveSolMax".into());
        }
        if self.paper_fill.delay_ms_min > self.paper_fill.delay_ms_max {
            return Err("paperFill.delayMsMin cannot be above delayMsMax".into());
        }
        if self.learning.review_every_trades == 0 || self.learning.shadow_trades == 0 {
            return Err("learning.reviewEveryTrades and learning.shadowTrades must be above 0".into());
        }
        if self.learning.min_sample < 10 {
            return Err("learning.minSample cannot be below 10 trades".into());
        }
        if self.learning.min_win_rate_gap_pts < 20.0 {
            return Err("learning.minWinRateGapPts cannot be below 20 points".into());
        }
        let w = &self.x_monitor.weights;
        let ws = [w.kol, w.engagement, w.replies, w.growth, w.integrity, w.community, w.narrative];
        if ws.iter().any(|x| !x.is_finite() || *x < 0.0) || ws.iter().sum::<f64>() <= 0.0 {
            return Err("xMonitor.weights must be zero or above, and not all zero".into());
        }
        if !(0.0..=15.0).contains(&self.x_monitor.max_points) || !(0.0..=0.15).contains(&self.x_monitor.points_factor) {
            return Err("xMonitor.maxPoints cannot be above 15 and xMonitor.pointsFactor cannot be above 0.15: X alone must never be able to buy".into());
        }
        let p = &self.x_monitor.pacing;
        if p.min_gap_secs < 5 || p.max_gap_secs < p.min_gap_secs || p.max_loads_per_hour == 0 || p.max_loads_per_hour > 200 {
            return Err("xMonitor.pacing: gaps must be at least 5 s (max not below min) and loads per hour between 1 and 200".into());
        }
        const STYLES: [&str; 5] = ["momentum", "snipe", "confirmation", "dip-40pct", "graduation-dip"];
        if !STYLES.contains(&self.entry.style.as_str()) {
            return Err(format!("entry.style must be one of {STYLES:?}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = include_str!("../../config.json");

    #[test]
    fn shipped_config_parses_and_validates() {
        let c = Config::from_json(CONFIG).expect("config.json should parse");
        assert_eq!(c.mode, Mode::Paper);
        assert_eq!(c.paper.bankroll_sol, 10.0);
        assert_eq!(c.exits.tranches.len(), 3);
        assert_eq!(c.exits.max_hold_seconds, 300);
        assert_eq!(c.exits.hard_stop_pct, -15.0);
        assert_eq!(c.entry.style, "momentum");
    }

    #[test]
    fn roundtrip_is_stable() {
        let c = Config::from_json(CONFIG).unwrap();
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(Config::from_json(&s).unwrap(), c);
    }

    #[test]
    fn rejects_bad_values() {
        let bad = CONFIG.replace("\"hardStopPct\": -15", "\"hardStopPct\": 5");
        assert!(Config::from_json(&bad).is_err());
        let unknown = CONFIG.replace("\"mode\": \"paper\"", "\"mode\": \"paper\", \"oops\": 1");
        assert!(Config::from_json(&unknown).is_err());
    }
}
