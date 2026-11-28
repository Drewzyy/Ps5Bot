//! Creator funder-chain reputation (SPEC 7c). A fresh creator wallet is funded by someone: the sender of its first incoming SOL
//! transfer. A funder whose OTHER funded creators are serial dead-launchers (at least two launches each, at least 80% of them
//! died) is a strong warning: in the TrenchTruth write-up (15,008 launches, UNVERIFIED) fresh creators funded by such a wallet
//! were 91% dead within 5 minutes against 78% for a clean funder and 59% for a good one.
//!
//! What this file does, and what it cannot: it records who funded each creator we look at and how each coin we watched ended,
//! and it judges a funder from those records. Both tables start empty, so the signal grows with use. A funder that funded a
//! great many creators (an exchange or a faucet) is never judged. No exchange address list is hard-coded (none can be
//! verified here).

use crate::config::FunderCfg;
use crate::db::Db;
use crate::shield::rules::CheckResult;
use serde_json::Value;

/// Smallest incoming transfer that counts as "funding" (0.01 SOL), so dust and rent refunds are not mistaken for a funder.
pub const MIN_FUNDING_LAMPORTS: u64 = 10_000_000;
/// Answers older than this are looked up again (a wallet's first funder never changes, but the judgement of the funder does).
pub const CACHE_OK_MS: i64 = 7 * 86_400_000;
/// A failed lookup is retried after this long.
pub const CACHE_ERR_MS: i64 = 60_000;
/// A creator wallet with this many signatures or more is not new: not judged, and we spend no more RPC calls on it.
pub const FRESH_MAX_SIGNATURES: usize = 25;

/// A coin died, from what the 60 second watch showed: it never got traction (under 5 buyers and no rise above the launch
/// price) or it lost half of its peak. An approximation of the research's "80% dead after 5 minutes" that needs no longer watch.
pub fn launch_died(peak_mcap: f64, end_mcap: f64, launch_mcap: f64, unique_buyers: u32) -> bool {
    if end_mcap <= 0.0 {
        return true;
    }
    let no_traction = unique_buyers < 5 && end_mcap <= launch_mcap * 1.05;
    let collapsed = peak_mcap > 0.0 && end_mcap < peak_mcap * 0.5;
    no_traction || collapsed
}

/// Who funded a wallet, from the transaction that created it (first incoming system transfer of at least 0.01 SOL).
/// `tx` is a `getTransaction` answer with `jsonParsed` encoding.
pub fn funding_source_from_tx(tx: &Value, wallet: &str) -> Option<(String, u64)> {
    let mut ixs: Vec<&Value> = vec![];
    if let Some(a) = tx["transaction"]["message"]["instructions"].as_array() {
        ixs.extend(a.iter());
    }
    if let Some(inner) = tx["meta"]["innerInstructions"].as_array() {
        for g in inner {
            if let Some(a) = g["instructions"].as_array() {
                ixs.extend(a.iter());
            }
        }
    }
    ixs.into_iter().find_map(|ix| {
        if ix["program"].as_str() != Some("system") {
            return None;
        }
        let p = &ix["parsed"];
        let kind = p["type"].as_str()?;
        if kind != "transfer" && kind != "transferWithSeed" && kind != "createAccount" {
            return None;
        }
        let info = &p["info"];
        let (src, dst) = (info["source"].as_str()?, info["destination"].as_str().or_else(|| info["newAccount"].as_str())?);
        let lamports = info["lamports"].as_u64()?;
        (dst == wallet && src != wallet && lamports >= MIN_FUNDING_LAMPORTS).then(|| (src.to_string(), lamports))
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunderVerdict {
    pub funder: String,
    /// Creators this funder funded that we know of (not counting the one being judged).
    pub funded: u32,
    /// Of those, how many are serial dead-launchers.
    pub bad: u32,
    pub hub: bool,
    pub serial: bool,
}

/// Judge a funder from our own records.
pub fn judge_funder(db: &Db, cfg: &FunderCfg, funder: &str, this_creator: &str) -> FunderVerdict {
    let funded: Vec<String> = db.wallets_funded_by(funder).into_iter().filter(|w| w != this_creator).collect();
    let hub = funded.len() as u32 > cfg.hub_creators;
    let bad = funded
        .iter()
        .filter(|w| {
            let (n, dead) = db.creator_outcomes(w);
            n >= cfg.min_launches && n > 0 && dead as f64 / n as f64 >= cfg.dead_share
        })
        .count() as u32;
    FunderVerdict { funder: funder.to_string(), funded: funded.len() as u32, bad, hub, serial: !hub && bad >= cfg.min_bad_creators }
}

/// The Scam Shield result for one creator, from a cached or fresh funder lookup. Unknown, old and failed lookups PASS with a
/// note (this is an extra signal, not a required check), and never block.
pub fn funder_result(db: &Db, cfg: &FunderCfg, creator: &str, status: &str, funder: &str) -> CheckResult {
    let name = "funder";
    match status {
        "ok" if !funder.is_empty() => {
            let v = judge_funder(db, cfg, funder, creator);
            if v.serial {
                CheckResult::block(name, "FUNDER_SERIAL_DEAD", format!("The creator wallet was funded by {}, which also funded {} other creator(s) whose coins mostly died ({} of {} are serial dead launchers).", crate::shield::rules::short(funder), v.funded, v.bad, v.funded))
                    .with_evidence(vec![crate::shield::rules::ev("Funder on Solscan", crate::pump::solscan_account(funder))])
            } else if v.hub {
                CheckResult::pass(name, format!("Funded by {}, which funded {} creators (likely an exchange or faucet): not judged.", crate::shield::rules::short(funder), v.funded))
            } else {
                CheckResult::pass(name, format!("Funded by {}; {} of {} other creator(s) it funded are serial dead launchers.", crate::shield::rules::short(funder), v.bad, v.funded))
            }
        }
        "old" => CheckResult::pass(name, "The creator wallet is not new: funder not checked."),
        "none" => CheckResult::pass(name, "No funding transfer found for the creator wallet: unknown, not a pass or a block."),
        _ => CheckResult::pass(name, "Funder lookup failed or was skipped: unknown, not a block."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> FunderCfg {
        FunderCfg::default()
    }

    fn transfer_tx(src: &str, dst: &str, lamports: u64, inner: bool) -> Value {
        let ix = json!({"program":"system","programId":"1111","parsed":{"type":"transfer","info":{"source":src,"destination":dst,"lamports":lamports}}});
        if inner {
            json!({"transaction":{"message":{"instructions":[]}},"meta":{"innerInstructions":[{"index":0,"instructions":[ix]}]}})
        } else {
            json!({"transaction":{"message":{"instructions":[ix]}},"meta":{"innerInstructions":[]}})
        }
    }

    #[test]
    fn the_funder_is_the_sender_of_the_first_real_incoming_transfer() {
        assert_eq!(funding_source_from_tx(&transfer_tx("FUNDER", "CREATOR", 500_000_000, false), "CREATOR"), Some(("FUNDER".into(), 500_000_000)));
        assert_eq!(funding_source_from_tx(&transfer_tx("FUNDER", "CREATOR", 500_000_000, true), "CREATOR").unwrap().0, "FUNDER", "also found inside inner instructions (a funder program)");
        assert_eq!(funding_source_from_tx(&transfer_tx("FUNDER", "CREATOR", 5_000, false), "CREATOR"), None, "dust is not funding");
        assert_eq!(funding_source_from_tx(&transfer_tx("FUNDER", "SOMEONE_ELSE", 500_000_000, false), "CREATOR"), None);
        assert_eq!(funding_source_from_tx(&transfer_tx("CREATOR", "CREATOR", 500_000_000, false), "CREATOR"), None);
        assert_eq!(funding_source_from_tx(&json!({}), "CREATOR"), None);
    }

    #[test]
    fn what_counts_as_a_dead_launch() {
        assert!(launch_died(30.0, 29.0, 28.0, 2), "no buyers, no rise");
        assert!(launch_died(60.0, 20.0, 28.0, 12), "lost more than half of its peak");
        assert!(!launch_died(45.0, 44.0, 28.0, 12), "traction and holding");
        assert!(launch_died(28.4, 28.4, 28.0, 3), "3 buyers and flat is dead");
        assert!(!launch_died(33.0, 32.0, 28.0, 3), "up 14% is not dead yet");
        assert!(launch_died(0.0, 0.0, 28.0, 0));
    }

    fn seed(db: &Db, creator: &str, funder: &str, launches: u32, dead: u32) {
        db.funder_put(creator, funder, 1, 1_000_000_000, 1, "ok");
        for i in 0..launches {
            db.outcome_put(&format!("{creator}-{i}"), creator, 1, i < dead, 30.0, 10.0, 2);
        }
    }

    #[test]
    fn a_funder_of_serial_dead_launchers_is_blocked_and_the_reason_names_the_numbers() {
        let db = Db::open(":memory:").unwrap();
        seed(&db, "bad1", "F", 3, 3);
        seed(&db, "bad2", "F", 2, 2);
        seed(&db, "ok1", "F", 3, 1);
        let r = funder_result(&db, &cfg(), "fresh", "ok", "F");
        assert!(r.blocks() && r.code == "FUNDER_SERIAL_DEAD", "{r:?}");
        assert!(r.detail.contains("2 of 3"), "{}", r.detail);
        assert!(!r.evidence.is_empty());
    }

    #[test]
    fn one_bad_creator_is_not_enough_and_under_two_launches_does_not_count() {
        let db = Db::open(":memory:").unwrap();
        seed(&db, "bad1", "F", 3, 3);
        seed(&db, "oneshot", "F", 1, 1); // a single launch is not a pattern
        seed(&db, "mixed", "F", 4, 2); // 50% dead
        let v = judge_funder(&db, &cfg(), "F", "fresh");
        assert_eq!((v.funded, v.bad, v.serial), (3, 1, false));
        assert!(!funder_result(&db, &cfg(), "fresh", "ok", "F").blocks());
    }

    #[test]
    fn a_hub_funder_such_as_an_exchange_is_never_judged() {
        let db = Db::open(":memory:").unwrap();
        for i in 0..30 {
            seed(&db, &format!("c{i}"), "EXCHANGE", 3, 3);
        }
        let r = funder_result(&db, &cfg(), "fresh", "ok", "EXCHANGE");
        assert!(!r.blocks() && r.detail.contains("exchange"), "{r:?}");
    }

    #[test]
    fn the_creator_being_judged_is_not_counted_against_its_own_funder() {
        let db = Db::open(":memory:").unwrap();
        seed(&db, "bad1", "F", 3, 3);
        seed(&db, "me", "F", 3, 3);
        let v = judge_funder(&db, &cfg(), "F", "me");
        assert_eq!((v.funded, v.bad), (1, 1));
    }

    #[test]
    fn unknown_old_failed_and_missing_lookups_pass_with_a_note_and_never_block() {
        let db = Db::open(":memory:").unwrap();
        for st in ["old", "none", "error", "skipped"] {
            let r = funder_result(&db, &cfg(), "x", st, "");
            assert!(!r.blocks() && r.outcome == crate::shield::rules::Outcome::Pass, "{st}: {r:?}");
        }
    }

    #[test]
    fn lookups_are_cached_in_the_database() {
        let db = Db::open(":memory:").unwrap();
        assert!(db.funder_get("W").is_none());
        db.funder_put("W", "F", 5, 700_000_000, 10, "ok");
        assert_eq!(db.funder_get("W"), Some(("F".into(), 5, 10, "ok".into())));
        db.funder_put("W", "", 0, 0, 20, "error");
        assert_eq!(db.funder_get("W").unwrap().3, "error");
        assert_eq!(db.wallets_funded_by("F"), Vec::<String>::new(), "only status ok counts as funded");
    }
}
