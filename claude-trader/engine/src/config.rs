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
    /// Raw feed recorder (SPEC 4f): every decoded trade and launch, kept as compressed daily files for offline replay.
    #[serde(default)]
    pub recorder: RecorderCfg,
    /// Paper-only deep-curve sprint lane (SPEC 6c). The numbers come from one tiny unreplicated backtest: unverified.
    #[serde(default)]
    pub sprint: Sprint,
}

/// The raw feed recorder. Files live in the app data folder under `recordings/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct RecorderCfg {
    pub enabled: bool,
    /// Daily files older than this many days are deleted.
    pub retention_days: u32,
    /// Oldest files are deleted first when the folder grows past this many megabytes.
    pub max_megabytes: u32,
}

impl Default for RecorderCfg {
    fn default() -> Self {
        RecorderCfg { enabled: true, retention_days: 14, max_megabytes: 2000 }
    }
}

/// Deep-curve sprint lane (paper only). Entry: real reserves 39 to 60 SOL, coin 40 to 120 slots old, at least 30 distinct
/// buyers, launch-slot wallets able to pull at most 16 SOL, no single wallet over 11 SOL. Exit: sell at 90% of graduation or
/// when the real reserve falls 20 SOL below entry or 40 SOL below its peak. Source: one 48 hour replay with 7 sprint trades
/// (WhoIsTyfur/makefun-memecoin-backtest-48h): UNVERIFIED, the lane exists to measure it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Sprint {
    pub enabled: bool,
    pub style: String,
    pub real_sol_min: f64,
    pub real_sol_max: f64,
    pub min_age_slots: u64,
    pub max_age_slots: u64,
    pub min_buyers: u32,
    pub max_launch_slot_pull_sol: f64,
    pub max_single_wallet_pull_sol: f64,
    /// Exit when real reserves fall this many SOL below the entry reserve.
    pub stop_below_entry_sol: f64,
    /// Exit when real reserves fall this many SOL below the peak reserve seen since entry.
    pub stop_below_peak_sol: f64,
    /// Longest hold for this lane (the main lane's 300 s is far too short for a coin to climb 39 to 76 SOL).
    pub max_hold_seconds: u32,
    pub max_open: u32,
}

impl Sprint {
    /// The config the sprint lane runs on: the shipped config with the percent stops, the take-profit ladder and the fade exits
    /// switched off and SOL-reserve stops switched on. Still on: dev sell, cabal dump, slow rug, migration and the sell at 90% of
    /// graduation (SPEC 11b #9). Only the exits change here; Scam Shield and the entry gates are never part of a lane's config.
    pub fn lane_config(&self, base: &Config) -> Config {
        let mut c = base.clone();
        let e = &mut c.exits;
        e.tranches = vec![];
        e.trailing_stop_pct = 100.0;
        e.soft_stop_pct = -100.0;
        e.hard_stop_pct = -100.0;
        e.sell_below_average_entry = false;
        e.max_hold_seconds = self.max_hold_seconds;
        e.stall_exit.from_seconds = 1_000_000;
        e.stall_exit.to_seconds = 1_000_001;
        e.bleeder_exit.after_seconds = 1_000_000;
        e.volume_taper.last_below_baseline_pct = 0.0;
        e.momentum_death.min_buys_per_minute = 0.0;
        e.whale_sell_pct_of_curve = 1000.0;
        e.reserve_stops = Some(ReserveStops { below_entry_sol: self.stop_below_entry_sol, below_peak_sol: self.stop_below_peak_sol });
        c
    }
}

impl Default for Sprint {
    fn default() -> Self {
        Sprint {
            enabled: true,
            style: "deep-sprint".into(),
            real_sol_min: 39.0,
            real_sol_max: 60.0,
            min_age_slots: 40,
            max_age_slots: 120,
            min_buyers: 30,
            max_launch_slot_pull_sol: 16.0,
            max_single_wallet_pull_sol: 11.0,
            stop_below_entry_sol: 20.0,
            stop_below_peak_sol: 40.0,
            max_hold_seconds: 600,
            max_open: 3,
        }
    }
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
    /// A change is only applied if its test lane made at least this many trades (the holdout) AND their mean return is above 0
    /// AND the lower end of the 95% bootstrap range of that mean is above 0.
    #[serde(default = "default_promotion_min_trades")]
    pub promotion_min_trades: u32,
    #[serde(default = "default_bootstrap_resamples")]
    pub bootstrap_resamples: u32,
}

fn default_promotion_min_trades() -> u32 {
    20
}
fn default_bootstrap_resamples() -> u32 {
    4000
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
    /// Graded launch-slot checks (SPEC 7b): count the wallets that bought in the first two slots and how much SOL they could
    /// pull out, and block only above these limits instead of on any first-block buyer.
    #[serde(default)]
    pub graded: GradedShield,
    /// Creator funder-chain reputation (SPEC 7c).
    #[serde(default)]
    pub funder: FunderCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct GradedShield {
    /// Off = the old hard block on any first-block buyer.
    pub enabled: bool,
    /// Block when the wallets that bought in the creation slot or the next one could sell everything for more than this many SOL.
    pub max_launch_slot_pull_sol: f64,
    /// Block when one such wallet alone could pull more than this many SOL.
    pub max_single_wallet_pull_sol: f64,
    /// Block when the launch-slot wallets hold at least this share of the supply.
    pub max_launch_group_supply_pct: f64,
}

impl Default for GradedShield {
    fn default() -> Self {
        GradedShield { enabled: true, max_launch_slot_pull_sol: 16.0, max_single_wallet_pull_sol: 11.0, max_launch_group_supply_pct: 40.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct FunderCfg {
    pub enabled: bool,
    /// A creator "launched coins that died" when it has at least this many launches in our records...
    pub min_launches: u32,
    /// ...and at least this share of them died.
    pub dead_share: f64,
    /// A funder is a serial dead-launcher funder when at least this many OTHER creators it funded are such creators.
    pub min_bad_creators: u32,
    /// A funder that funded more than this many creators is a hub (exchange, faucet): never judged.
    pub hub_creators: u32,
    /// A wallet counts as fresh when its first transaction is less than this many hours old.
    pub fresh_hours: u32,
    /// Time allowed for the lookup. If it runs out the coin is not blocked (the signal is "unknown").
    pub budget_ms: u64,
}

impl Default for FunderCfg {
    fn default() -> Self {
        FunderCfg { enabled: true, min_launches: 2, dead_share: 0.8, min_bad_creators: 2, hub_creators: 25, fresh_hours: 48, budget_ms: 4000 }
    }
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
    /// Stops measured in SOL of real curve reserve instead of percent of price (used by the deep-curve sprint lane only).
    #[serde(default)]
    pub reserve_stops: Option<ReserveStops>,
}

/// Sell everything when the real SOL reserve of the curve falls `below_entry_sol` under its value at entry, or `below_peak_sol`
/// under the highest value seen since entry. The research found graduating curves gave back up to 32 SOL on the way, so tighter
/// stops sold winners early (UNVERIFIED, one small replay).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReserveStops {
    pub below_entry_sol: f64,
    pub below_peak_sol: f64,
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
    /// Block 0 tax read from the pump Global account (basis points, 0 today). Extra protocol fee for non-creators who trade in
    /// the coin's creation slot. Set by the engine at start, not by hand. No lane trades in the creation slot today.
    #[serde(default)]
    pub block0_tax_bps: u64,
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
        if self.learning.promotion_min_trades < 10 {
            return Err("learning.promotionMinTrades cannot be below 10 trades".into());
        }
        if self.learning.bootstrap_resamples < 200 {
            return Err("learning.bootstrapResamples cannot be below 200".into());
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
        let sp = &self.sprint;
        if sp.real_sol_min >= sp.real_sol_max || sp.min_age_slots >= sp.max_age_slots || sp.real_sol_max >= 0.9 * crate::curve::GRADUATION_REAL_SOL {
            return Err("sprint: the SOL range and the age range must be from low to high, and the top of the range must stay under 90% of graduation (76.5 SOL)".into());
        }
        if sp.stop_below_entry_sol <= 0.0 || sp.stop_below_peak_sol <= 0.0 || sp.max_open == 0 || sp.max_hold_seconds == 0 {
            return Err("sprint: the stops, the open limit and the hold time must be above 0".into());
        }
        if self.filters.graded.max_launch_slot_pull_sol < 0.0 || self.filters.graded.max_single_wallet_pull_sol < 0.0 || !(0.0..=100.0).contains(&self.filters.graded.max_launch_group_supply_pct) {
            return Err("filters.graded: limits cannot be negative and the supply share is a percent from 0 to 100".into());
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
    fn the_new_sections_have_the_researched_defaults_and_old_configs_without_them_still_load() {
        let c = Config::from_json(CONFIG).unwrap();
        assert_eq!((c.sprint.real_sol_min, c.sprint.real_sol_max, c.sprint.min_age_slots, c.sprint.max_age_slots, c.sprint.min_buyers), (39.0, 60.0, 40, 120, 30));
        assert_eq!((c.sprint.max_launch_slot_pull_sol, c.sprint.max_single_wallet_pull_sol, c.sprint.stop_below_entry_sol, c.sprint.stop_below_peak_sol), (16.0, 11.0, 20.0, 40.0));
        assert_eq!((c.filters.graded.max_launch_slot_pull_sol, c.filters.graded.max_single_wallet_pull_sol, c.filters.graded.max_launch_group_supply_pct), (16.0, 11.0, 40.0));
        assert_eq!((c.learning.promotion_min_trades, c.recorder.retention_days), (20, 14));
        // a config written before these sections existed
        let mut v: serde_json::Value = serde_json::from_str(CONFIG).unwrap();
        for k in ["sprint", "recorder"] {
            v.as_object_mut().unwrap().remove(k);
        }
        v["filters"].as_object_mut().unwrap().remove("graded");
        v["filters"].as_object_mut().unwrap().remove("funder");
        v["fees"].as_object_mut().unwrap().remove("block0TaxBps");
        v["learning"].as_object_mut().unwrap().remove("promotionMinTrades");
        v["learning"].as_object_mut().unwrap().remove("bootstrapResamples");
        let old = Config::from_json(&v.to_string()).unwrap();
        assert_eq!(old.sprint, c.sprint);
        assert_eq!(old.filters.graded, c.filters.graded);
        assert_eq!(old.learning.promotion_min_trades, 20);
    }

    #[test]
    fn nonsense_sprint_and_learning_values_are_refused() {
        let mut c = Config::from_json(CONFIG).unwrap();
        c.sprint.real_sol_max = 80.0; // above 90% of graduation
        assert!(c.validate().is_err());
        let mut c = Config::from_json(CONFIG).unwrap();
        c.sprint.min_age_slots = 130;
        assert!(c.validate().is_err());
        let mut c = Config::from_json(CONFIG).unwrap();
        c.learning.promotion_min_trades = 5;
        assert!(c.validate().is_err());
        let mut c = Config::from_json(CONFIG).unwrap();
        c.filters.graded.max_launch_group_supply_pct = 140.0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn the_sprint_lane_config_swaps_percent_stops_for_reserve_stops_and_keeps_the_insider_exits() {
        let base = Config::from_json(CONFIG).unwrap();
        let l = base.sprint.lane_config(&base);
        assert!(l.exits.tranches.is_empty() && l.exits.reserve_stops == Some(ReserveStops { below_entry_sol: 20.0, below_peak_sol: 40.0 }));
        assert_eq!((l.exits.max_hold_seconds, l.exits.hard_stop_pct), (600, -100.0));
        assert!(l.exits.exit_on_dev_sell && l.exits.sell_on_migration);
        assert_eq!(base.exits.max_hold_seconds, 300, "the main lane keeps its 5 minutes");
        assert!(base.exits.reserve_stops.is_none());
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
