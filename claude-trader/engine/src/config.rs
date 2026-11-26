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
    pub pump_portal_pct_per_side: f64,
    pub priority_micro_lamports: u64,
    pub compute_units: u64,
    pub pump_portal_priority_fee_sol: f64,
    pub pump_portal_priority_fee_cap_sol: f64,
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
