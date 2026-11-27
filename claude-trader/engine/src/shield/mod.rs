//! Scam Shield pipeline: cheap checks first, then chain checks, then pre-buy checks (SPEC 7, 11b).
//! Every check returns a `CheckResult`; a required check that is undecided blocks the token.

pub mod net;
pub mod rules;

use crate::config::Config;
use crate::db::Db;
use crate::pump::{FeedMsg, Trade};
use crate::rpc::{now_ms, Rpc};
use rules::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Check budget (SPEC 7: each check has a 2 s budget).
pub const BUDGET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct ShieldOpts {
    /// Sandbox only: treat "metadata host unreachable because of the network policy" as a flagged pass.
    pub relax_egress: bool,
}

pub struct Shield {
    pub cfg: Config,
    pub db: Arc<Db>,
    pub rpc: Arc<Rpc>,
    pub rugcheck: net::RugCheck,
    pub http: reqwest::Client,
    pub opts: ShieldOpts,
}

/// What the chain stage learned, kept for later stages.
#[derive(Debug, Clone, Default)]
pub struct ChainFacts {
    pub holders: Vec<Holder>,
    pub excluded: HashSet<String>,
    /// Website, X and Telegram from the coin's metadata (for the "source" links in the UI).
    pub links: rules::SocialLinks,
}

#[derive(Debug, Clone)]
pub struct Decision {
    /// First blocking result, if any.
    pub block: Option<CheckResult>,
    pub results: Vec<CheckResult>,
    pub verdict: &'static str,
}

pub fn decide(results: Vec<CheckResult>) -> Decision {
    let block = results.iter().find(|r| r.blocks()).cloned();
    let verdict = verdict_for(&results);
    Decision { block, results, verdict }
}

impl Shield {
    pub fn new(cfg: Config, db: Arc<Db>, rpc: Arc<Rpc>, opts: ShieldOpts) -> Self {
        Shield { cfg, db, rpc, rugcheck: net::RugCheck::new(), http: crate::net::http_client(), opts }
    }

    /// Stage A: cheap checks, no RPC and no network. Runs at discovery.
    pub fn stage_a(&self, t: &FeedMsg) -> Vec<CheckResult> {
        let f = &self.cfg.filters;
        let since = now_ms() - 24 * 3600 * 1000;
        vec![
            metadata_present_rule(&t.name, &t.symbol, &t.uri),
            creator_holding_rule(creator_initial_pct(t.initial_buy), f.max_creator_holding_pct, &t.trader_public_key),
            blacklist_rule(self.db.blacklist_reason(&t.trader_public_key), &t.trader_public_key),
            copycat_rule(&t.name, &t.symbol, &self.db.trending_names(since)),
        ]
    }

    /// Stage B: chain checks. Each runs with a 2 s budget (the mint poll loop is allowed its 8 polls).
    pub async fn stage_b(&self, t: &FeedMsg) -> (Vec<CheckResult>, ChainFacts) {
        let f = &self.cfg.filters;
        let mint = t.mint.as_str();
        let relax = self.opts.relax_egress;

        let mint_fut = async {
            match timeout(Duration::from_secs(8), self.rpc.get_mint(mint)).await {
                Ok(Ok(m)) => mint_rule(&m, f.mint_authority_revoked, f.freeze_authority_revoked, mint),
                Ok(Err(e)) => CheckResult::undecided("mint_account", e),
                Err(_) => CheckResult::undecided("mint_account", "timed out"),
            }
        };
        let rug_fut = async {
            // New tokens can be a moment late in RugCheck (404): up to 4 attempts, each inside the 2 s budget.
            for attempt in 0..4 {
                match timeout(BUDGET, self.rugcheck.report(mint)).await {
                    Ok(Ok(Some(v))) => return Ok(parse_rugcheck(&v)),
                    Ok(Ok(None)) if attempt < 3 => tokio::time::sleep(Duration::from_millis(1000)).await,
                    Ok(Ok(None)) => return Err("RugCheck has no report yet".to_string()),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => return Err("timed out".to_string()),
                }
            }
            Err("no report".to_string())
        };
        let sig_fut = async {
            // The create tx is not always in the `confirmed` signature list yet: up to 3 attempts, each inside the 2 s budget.
            let mut last = Err("no attempt".to_string());
            for attempt in 0..3 {
                last = match timeout(BUDGET, self.rpc.signatures(&t.bonding_curve_key, 100)).await {
                    Ok(r) => r,
                    Err(_) => Err("timed out".to_string()),
                };
                if matches!(&last, Ok(l) if l.iter().any(|s| s.0 == t.signature)) {
                    break;
                }
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_millis(800)).await;
                }
            }
            last
        };
        let meta_fut = async { timeout(BUDGET + Duration::from_millis(300), net::fetch_metadata_links(&self.http, &t.uri)).await.unwrap_or((MetaFetch::Failed("timeout".into()), rules::SocialLinks::default())) };

        let (mint_res, rug, sigs, (meta, links)) = tokio::join!(mint_fut, rug_fut, sig_fut, meta_fut);
        let mut out = vec![mint_res, honeypot_stub()];
        let mut facts = ChainFacts::default();
        facts.excluded.insert(t.bonding_curve_key.clone());
        facts.links = links;

        match &rug {
            Ok(r) => {
                out.push(rugcheck_rule(r, mint));
                facts.holders = r.holders.clone();
                facts.excluded.extend(r.known_pools.iter().cloned());
                let ex = if f.exclude_curve_and_pool_accounts { facts.excluded.clone() } else { HashSet::new() };
                out.push(holder_rule(&r.holders, &ex, f.max_single_holder_pct, f.max_top_holders_combined_pct, f.max_insiders_pct, mint));
                // second opinion on the creator's bag, from the chain-derived RugCheck balance
                if let Some(p) = r.creator_balance_pct {
                    let mut c = creator_holding_rule(p, f.max_creator_holding_pct, &t.trader_public_key);
                    c.name = "creator_holding_live";
                    out.push(c);
                }
                out.push(community_rule(&meta, &rug_image(r), f.require_community_checks.min_social_links, f.require_community_checks.require_image, relax));
            }
            Err(e) => {
                out.push(CheckResult::undecided("rugcheck", e.clone()));
                out.push(CheckResult::undecided("holders", format!("holders come from RugCheck: {e}")));
                out.push(community_rule(&meta, &RugImage::Unknown, f.require_community_checks.min_social_links, f.require_community_checks.require_image, relax));
            }
        }
        out.push(metadata_uri_rule(&meta, relax));

        match &sigs {
            Ok(list) => {
                out.push(dev_snipe_rule(list, &t.signature, f.reject_dev_snipe_first_block));
                // Burst test over every successful tx on the curve (one entry per signature; old txs have no known payer,
                // so the signature stands in). Payer-count rules run later in pre_buy on real wallets.
                let burst_only: Vec<SlotTx> = list.iter().filter(|s| !s.2).map(|s| SlotTx { slot: s.1, payer: s.0.clone(), recv_ms: now_ms() }).collect();
                let hit = bundle_rules(&burst_only, &facts.holders, &facts.excluded, now_ms()).filter(|h| h.code == "BUNDLE_BURST" || h.code == "BUNDLE_SUPPLY");
                let sig_list: Vec<String> = list.iter().map(|s| s.0.clone()).collect();
                out.push(bundle_check(hit, f.reject_bundle, &sig_list));
            }
            Err(e) => {
                out.push(CheckResult::undecided("dev_snipe", e.clone()));
                out.push(CheckResult::undecided("bundle", e.clone()));
            }
        }
        (out, facts)
    }

    /// Pre-buy checks on the full observation data: wash volume, chase risk, bundle patterns on real payers.
    pub fn pre_buy(&self, trades: &[Trade], facts: &ChainFacts) -> Vec<CheckResult> {
        let now = now_ms();
        let slot_txs: Vec<SlotTx> = trades.iter().map(|x| SlotTx { slot: x.slot, payer: x.user.clone(), recv_ms: x.recv_ms }).collect();
        let ex = if self.cfg.filters.exclude_curve_and_pool_accounts { facts.excluded.clone() } else { HashSet::new() };
        let sigs: Vec<String> = trades.iter().map(|x| x.signature.clone()).collect();
        let hit = bundle_rules(&slot_txs, &[], &ex, now).filter(|h| h.code != "BUNDLE_SUPPLY");
        vec![
            bundle_check(hit, self.cfg.filters.reject_bundle, &sigs),
            wash_volume_rule(trades, now),
        ]
    }

    /// Momentum-lane only pre-buy checks (the dip-40pct variant waits for a dip and has its own logic):
    /// chase risk (price now more than 3x launch) and falling knife (more than 40% below the window's peak).
    pub fn pre_buy_momentum(&self, t: &FeedMsg, now_mcap_sol: f64, peak_mcap_sol: f64) -> Vec<CheckResult> {
        momentum_pre_buy(crate::pump::mcap_sol(t.v_sol_in_bonding_curve, t.v_tokens_in_bonding_curve), now_mcap_sol, peak_mcap_sol)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mint_fixture(mint_auth: serde_json::Value) -> serde_json::Value {
        json!({"data":{"program":"spl-token-2022","parsed":{"type":"mint","info":{"decimals":6,"supply":"1000000000000000","mintAuthority":mint_auth,"freezeAuthority":null,"extensions":[]}}}})
    }

    #[test]
    fn known_bad_fixture_is_blocked_as_high_with_mint_authority_reason() {
        let m = parse_mint_account(&mint_fixture(json!("AuthAuthAuthAuthAuthAuth11111111111111111111"))).unwrap();
        let d = decide(vec![mint_rule(&m, true, true, "MINT"), honeypot_stub()]);
        let b = d.block.expect("must block");
        assert_eq!(b.code, "MINT_AUTHORITY");
        assert_eq!(d.verdict, "HIGH");
        assert!(!b.evidence.is_empty());
    }

    #[test]
    fn known_clean_fixture_passes_with_low_signal() {
        let m = parse_mint_account(&mint_fixture(serde_json::Value::Null)).unwrap();
        let d = decide(vec![mint_rule(&m, true, true, "MINT"), honeypot_stub(), creator_holding_rule(0.0, 10.0, "c")]);
        assert!(d.block.is_none());
        assert_eq!(d.verdict, "LOW_SIGNAL");
    }

    #[test]
    fn undecided_required_blocks_but_optional_does_not() {
        let d = decide(vec![CheckResult::undecided("rugcheck", "timeout")]);
        assert_eq!(d.block.unwrap().code, "UNDECIDED_RUGCHECK");
        let d2 = decide(vec![CheckResult::undecided("rugcheck", "timeout").with_required(false)]);
        assert!(d2.block.is_none());
    }
}
