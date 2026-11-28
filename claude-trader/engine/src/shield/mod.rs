//! Scam Shield pipeline: cheap checks first, then chain checks, then pre-buy checks (SPEC 7, 11b).
//! Every check returns a `CheckResult`; a required check that is undecided blocks the token.

pub mod launch;
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
use tokio::sync::watch;
use tokio::time::timeout;

/// Most creation-slot transactions read per coin to learn what the launch group bought (each is one RPC call).
pub const LAUNCH_READ_CAP: usize = 8;

/// Check budget (SPEC 7: each check has a 2 s budget).
pub const BUDGET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct ShieldOpts {
    /// Sandbox only: treat "metadata host unreachable because of the network policy" as a flagged pass.
    pub relax_egress: bool,
}

/// Words recorded when RugCheck gave no answer and the coin passed every on-chain check instead (SPEC 11b #29).
pub const RUG_FALLBACK_NOTE: &str = "RugCheck unavailable, passed on-chain checks";
/// Trade size multiplier for such a coin.
pub const FALLBACK_SIZE_FACTOR: f64 = 0.5;

/// Where the RugCheck request for one coin stands. It runs in parallel with the 60 s watch.
#[derive(Debug, Clone)]
pub enum RugOutcome {
    Pending,
    Report(Box<RugReport>),
    /// Every try in the retry schedule failed. The string is the last error.
    Unavailable(String),
}

#[derive(Debug, Clone)]
pub struct RugProgress {
    pub outcome: RugOutcome,
    /// Requests made so far for this coin.
    pub attempts: u32,
    pub last_error: String,
}

pub type RugRx = watch::Receiver<RugProgress>;

pub struct Shield {
    pub cfg: Config,
    pub db: Arc<Db>,
    pub rpc: Arc<Rpc>,
    /// One client shared by the whole app (2 in flight, rate limited, cached per mint).
    pub rugcheck: Arc<net::RugCheck>,
    pub http: reqwest::Client,
    pub opts: ShieldOpts,
    /// Tests only: answer for the on-chain holder list instead of asking the RPC.
    pub holders_stub: std::sync::Mutex<Option<Result<Vec<Holder>, String>>>,
    /// At most 2 funder lookups at a time, and none while the RPC is rate limiting us.
    funder_sem: tokio::sync::Semaphore,
    funder_backoff_until_ms: std::sync::atomic::AtomicI64,
}

/// What the RugCheck step decides for a coin that would otherwise be picked.
#[derive(Debug, Clone, PartialEq)]
pub enum RugGate {
    /// RugCheck's report arrived and passed (or the run has no RugCheck): normal size.
    Full,
    /// RugCheck gave no answer; our own on-chain checks all passed: half size.
    Half,
    /// Blocked. The string is the machine code.
    Block(String),
}

/// The chain-stage results the fallback relies on. Each of `dev_snipe` and `bundle` must be present and must have passed.
pub fn fallback_core_gate(core: &[CheckResult]) -> Vec<CheckResult> {
    let mut out = vec![];
    for name in ["dev_snipe", "bundle"] {
        match core.iter().find(|r| r.name == name) {
            Some(r) => out.push(r.clone()),
            None => out.push(CheckResult::undecided(if name == "dev_snipe" { "dev_snipe" } else { "bundle" }, "this check did not run")),
        }
    }
    // a "pass" must really be a pass: anything else (even a non-required flag) stops the fallback
    for r in out.iter_mut() {
        if r.outcome != Outcome::Pass && r.outcome != Outcome::Block {
            r.required = true;
            r.outcome = Outcome::Undecided;
            r.code = format!("UNDECIDED_{}", r.name.to_ascii_uppercase());
        }
    }
    out
}

/// What the chain stage learned, kept for later stages.
#[derive(Debug, Clone, Default)]
pub struct ChainFacts {
    pub holders: Vec<Holder>,
    pub excluded: HashSet<String>,
    /// The mint account as read on-chain (supply, authorities, extensions).
    pub mint_info: Option<MintInfo>,
    /// The chain-stage results that need no RugCheck (mint account, dev snipe, bundle burst ...). The on-chain fallback
    /// re-reads them: all must have passed.
    pub core: Vec<CheckResult>,
    /// One entry per recent transaction on the curve, kept to re-run the supply-bundle test once holders are known.
    pub burst: Vec<SlotTx>,
    /// The RugCheck request for this coin, running in parallel with the watch. None in replays and tests.
    pub rug: Option<RugRx>,
    /// Set once the RugCheck report has arrived and been judged (pass or block).
    pub rug_done: Option<Decision>,
    /// Website, X and Telegram from the coin's metadata (for the "source" links in the UI).
    pub links: rules::SocialLinks,
    /// The bonding-curve account as read at the chain stage (mayhem flag, quote mint, holder rewards, creation slot).
    pub curve_account: Option<crate::rpc::CurveAccount>,
    /// Slot the coin was created in (from the signature list).
    pub create_slot: Option<u64>,
    /// Trades of the creation slot and the next one, read from the chain with `getTransaction` (the live feed starts too late
    /// to see them). Kept apart from the live store so they never change the observation counts.
    pub launch_trades: Vec<Trade>,
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
        Self::with_rugcheck(cfg, db, rpc, opts, Arc::new(net::RugCheck::new()))
    }

    /// A Shield that shares an existing RugCheck client (the app makes one and hands it to every Shield).
    pub fn with_rugcheck(cfg: Config, db: Arc<Db>, rpc: Arc<Rpc>, opts: ShieldOpts, rugcheck: Arc<net::RugCheck>) -> Self {
        Shield { cfg, db, rpc, rugcheck, http: crate::net::http_client(), opts, holders_stub: std::sync::Mutex::new(None), funder_sem: tokio::sync::Semaphore::new(2), funder_backoff_until_ms: Default::default() }
    }

    /// Start the RugCheck request for a coin in the background and return a handle to watch. It retries after 3, 8 and 15 s
    /// (HTTP 502, network error, timeout, 429, "no report yet"), so the coin keeps being watched while the report is pending.
    /// The task stops by itself when every receiver is dropped.
    pub fn start_rugcheck(&self, mint: &str) -> RugRx {
        let (tx, rx) = watch::channel(RugProgress { outcome: RugOutcome::Pending, attempts: 0, last_error: String::new() });
        let rc = self.rugcheck.clone();
        let mint = mint.to_string();
        tokio::spawn(async move {
            let work = async {
                let tx_ref = &tx;
                let res = rc
                    .report_with_retry(&mint, |n, err| {
                        tx_ref.send_modify(|p| {
                            p.attempts = n;
                            p.last_error = err.to_string();
                        });
                    })
                    .await;
                let outcome = match res {
                    Ok(v) => RugOutcome::Report(Box::new(parse_rugcheck(&v))),
                    Err(e) => RugOutcome::Unavailable(e),
                };
                tx_ref.send_modify(|p| p.outcome = outcome);
            };
            tokio::select! {
                _ = work => {}
                _ = tx.closed() => {}
            }
        });
        rx
    }

    /// Stage A: cheap checks, no RPC and no network. Runs at discovery.
    pub fn stage_a(&self, t: &FeedMsg) -> Vec<CheckResult> {
        let f = &self.cfg.filters;
        let since = now_ms() - 24 * 3600 * 1000;
        let mut v = vec![
            metadata_present_rule(&t.name, &t.symbol, &t.uri),
            creator_holding_rule(creator_initial_pct(t.initial_buy), f.max_creator_holding_pct, &t.trader_public_key),
            blacklist_rule(self.db.blacklist_reason(&t.trader_public_key), &t.trader_public_key),
            copycat_rule(&t.name, &t.symbol, &self.db.trending_names(since)),
        ];
        // the feed already says whether the coin is a mayhem coin (and sometimes the quote asset): no RPC needed
        v.extend(curve_type_rules(t.is_mayhem_mode, crate::pump::quote_is_non_sol(&t.quote_mint)));
        v
    }

    /// Creator funder-chain check (SPEC 7c). Cached in SQLite, at most 2 lookups at once, skipped for 30 s after the RPC says 429,
    /// and never blocks on anything but a known serial-dead-launcher funder.
    pub async fn funder_check(&self, creator: &str) -> CheckResult {
        let cfg = &self.cfg.filters.funder;
        if !cfg.enabled || creator.is_empty() {
            return CheckResult::pass("funder", "Funder check is off.");
        }
        let now = now_ms();
        if let Some((funder, _, checked, status)) = self.db.funder_get(creator) {
            let age = now - checked;
            let fresh = if status == "error" { age < crate::funder::CACHE_ERR_MS } else { age < crate::funder::CACHE_OK_MS };
            if fresh {
                return crate::funder::funder_result(&self.db, cfg, creator, &status, &funder);
            }
        }
        if now < self.funder_backoff_until_ms.load(std::sync::atomic::Ordering::Relaxed) {
            return crate::funder::funder_result(&self.db, cfg, creator, "skipped", "");
        }
        let Ok(_permit) = tokio::time::timeout(Duration::from_millis(cfg.budget_ms), self.funder_sem.acquire()).await else {
            return crate::funder::funder_result(&self.db, cfg, creator, "skipped", "");
        };
        let res = tokio::time::timeout(Duration::from_millis(cfg.budget_ms), self.rpc.first_funding(creator, cfg.fresh_hours)).await;
        match res {
            Ok(Ok((status, funder, ms, lamports))) => {
                self.db.funder_put(creator, &funder, ms, lamports, now_ms(), &status);
                crate::funder::funder_result(&self.db, cfg, creator, &status, &funder)
            }
            Ok(Err(e)) => {
                if e.contains("429") {
                    self.funder_backoff_until_ms.store(now_ms() + 30_000, std::sync::atomic::Ordering::Relaxed);
                }
                self.db.funder_put(creator, "", 0, 0, now_ms(), "error");
                crate::funder::funder_result(&self.db, cfg, creator, "error", "")
            }
            Err(_) => crate::funder::funder_result(&self.db, cfg, creator, "skipped", ""),
        }
    }

    /// Stage B: chain checks. Each runs with a 2 s budget (the mint poll loop is allowed its 8 polls).
    pub async fn stage_b(&self, t: &FeedMsg) -> (Vec<CheckResult>, ChainFacts) {
        let f = &self.cfg.filters;
        let mint = t.mint.as_str();
        let relax = self.opts.relax_egress;

        let mint_fut = async {
            match timeout(Duration::from_secs(8), self.rpc.get_mint(mint)).await {
                Ok(Ok(m)) => (mint_rule(&m, f.mint_authority_revoked, f.freeze_authority_revoked, mint), Some(m)),
                Ok(Err(e)) => (CheckResult::undecided("mint_account", e), None),
                Err(_) => (CheckResult::undecided("mint_account", "timed out"), None),
            }
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

        // RugCheck is NOT waited for here: it runs in the background and is judged during the watch (SPEC 11b #29).
        let curve_fut = async { timeout(BUDGET, self.rpc.curve_account(&t.bonding_curve_key)).await };
        let funder_fut = self.funder_check(&t.trader_public_key);
        let ((mint_res, mint_info), sigs, (meta, links), curve_acc, funder_res) = tokio::join!(mint_fut, sig_fut, meta_fut, curve_fut, funder_fut);
        let mut out = vec![mint_res, honeypot_stub()];
        if let Ok(Ok(acc)) = &curve_acc {
            out.extend(curve_type_rules(acc.is_mayhem, acc.non_sol_quote));
        }
        out.push(funder_res);
        let mut facts = ChainFacts::default();
        facts.excluded.insert(t.bonding_curve_key.clone());
        facts.links = links;
        facts.mint_info = mint_info;
        if let Ok(Ok(acc)) = curve_acc {
            facts.create_slot = Some(acc.created_at_slot).filter(|s| *s > 0);
            facts.curve_account = Some(acc);
        }

        // The image can also be vouched for by RugCheck, but only if its report is already here; otherwise the metadata file decides.
        out.push(community_rule(&meta, &RugImage::Unknown, f.require_community_checks.min_social_links, f.require_community_checks.require_image, relax));
        out.push(metadata_uri_rule(&meta, relax));

        match &sigs {
            Ok(list) if f.graded.enabled => {
                // Graded (SPEC 7b): count the launch-slot transactions now, read what those wallets bought, and judge at the end of
                // the watch (`launch_grade`). Only the extreme burst and supply tests below still block here.
                let (res, slot0, others) = dev_snipe_graded(list, &t.signature);
                out.push(res);
                if let Some(s) = slot0 {
                    facts.create_slot = Some(s);
                }
                if !others.is_empty() {
                    let read = async {
                        let mut v = vec![];
                        for sig in others.iter().take(LAUNCH_READ_CAP) {
                            if let Ok(ts) = self.rpc.transaction_trades(sig).await {
                                v.extend(ts.into_iter().filter(|x| x.mint == t.mint));
                            }
                        }
                        v
                    };
                    facts.launch_trades = timeout(Duration::from_secs(4), read).await.unwrap_or_default();
                }
                let burst_only: Vec<SlotTx> = list.iter().filter(|s| !s.2).map(|s| SlotTx { slot: s.1, payer: s.0.clone(), recv_ms: now_ms() }).collect();
                let hit = bundle_rules(&burst_only, &facts.holders, &facts.excluded, now_ms()).filter(|h| h.code == "BUNDLE_BURST" || h.code == "BUNDLE_SUPPLY");
                facts.burst = burst_only;
                let sig_list: Vec<String> = list.iter().map(|s| s.0.clone()).collect();
                out.push(bundle_check(hit, f.reject_bundle, &sig_list));
            }
            Ok(list) => {
                out.push(dev_snipe_rule(list, &t.signature, f.reject_dev_snipe_first_block));
                // Burst test over every successful tx on the curve (one entry per signature; old txs have no known payer,
                // so the signature stands in). Payer-count rules run later in pre_buy on real wallets.
                let burst_only: Vec<SlotTx> = list.iter().filter(|s| !s.2).map(|s| SlotTx { slot: s.1, payer: s.0.clone(), recv_ms: now_ms() }).collect();
                let hit = bundle_rules(&burst_only, &facts.holders, &facts.excluded, now_ms()).filter(|h| h.code == "BUNDLE_BURST" || h.code == "BUNDLE_SUPPLY");
                facts.burst = burst_only;
                let sig_list: Vec<String> = list.iter().map(|s| s.0.clone()).collect();
                out.push(bundle_check(hit, f.reject_bundle, &sig_list));
            }
            Err(e) => {
                out.push(CheckResult::undecided("dev_snipe", e.clone()));
                out.push(CheckResult::undecided("bundle", e.clone()));
            }
        }
        facts.core = out.iter().filter(|r| matches!(r.name, "mint_account" | "dev_snipe" | "bundle")).cloned().collect();
        (out, facts)
    }

    /// If RugCheck's report has arrived and was not judged yet, judge it now (rugged and danger flags block; so do holder
    /// concentration and the creator's live balance). Returns the decision once, the first time.
    pub fn judge_report_if_ready(&self, t: &FeedMsg, facts: &mut ChainFacts) -> Option<Decision> {
        if facts.rug_done.is_some() {
            return None;
        }
        let prog = facts.rug.as_ref()?.borrow().clone();
        let RugOutcome::Report(r) = &prog.outcome else { return None };
        let d = decide(self.rug_results(r, t, facts));
        facts.rug_done = Some(d.clone());
        Some(d)
    }

    /// The RugCheck verdict for a coin that would otherwise be picked (SPEC 11b #29). Returns the gate and the
    /// decisions to record, as (stage, decision).
    ///  - report judged OK: full size.
    ///  - report says danger: block (always, whatever `requireRugcheck` says).
    ///  - no report: `requireRugcheck` on blocks it; off runs the on-chain fallback, which blocks on any failure and
    ///    otherwise gives half size.
    pub async fn rug_gate(&self, t: &FeedMsg, facts: &mut ChainFacts) -> (RugGate, Vec<(&'static str, Decision)>) {
        let mut rec = vec![];
        let Some(rx) = facts.rug.clone() else { return (RugGate::Full, rec) };
        if let Some(d) = self.judge_report_if_ready(t, facts) {
            rec.push(("rugcheck", d));
        }
        if let Some(d) = &facts.rug_done {
            return match &d.block {
                Some(b) => (RugGate::Block(b.code.clone()), rec),
                None => (RugGate::Full, rec),
            };
        }
        let prog = rx.borrow().clone();
        let why = match &prog.outcome {
            RugOutcome::Unavailable(e) => e.clone(),
            _ if !prog.last_error.is_empty() => format!("{} (still retrying)", prog.last_error),
            _ => "no answer in time".to_string(),
        };
        if self.cfg.filters.require_rugcheck {
            let d = decide(vec![CheckResult::undecided("rugcheck", format!("RugCheck gave no answer ({why}) and requireRugcheck is on."))]);
            let code = d.block.as_ref().map(|b| b.code.clone()).unwrap_or_default();
            rec.push(("rugcheck", d));
            return (RugGate::Block(code), rec);
        }
        let d = decide(self.onchain_fallback(t, facts, &why).await);
        let out = match &d.block {
            Some(b) => RugGate::Block(b.code.clone()),
            None => RugGate::Half,
        };
        rec.push(("rugcheck-fallback", d));
        (out, rec)
    }

    /// Checks that need the RugCheck report: its rugged and danger flags (these always block), holder concentration,
    /// the creator's live balance and the supply-bundle test (which needs the holder list).
    pub fn rug_results(&self, r: &RugReport, t: &FeedMsg, facts: &mut ChainFacts) -> Vec<CheckResult> {
        let f = &self.cfg.filters;
        let mint = t.mint.as_str();
        facts.holders = r.holders.clone();
        facts.excluded.extend(r.known_pools.iter().cloned());
        let ex = if f.exclude_curve_and_pool_accounts { facts.excluded.clone() } else { HashSet::new() };
        let mut out = vec![rugcheck_rule(r, mint), holder_rule(&r.holders, &ex, f.max_single_holder_pct, f.max_top_holders_combined_pct, f.max_insiders_pct, mint)];
        if let Some(p) = r.creator_balance_pct {
            let mut c = creator_holding_rule(p, f.max_creator_holding_pct, &t.trader_public_key);
            c.name = "creator_holding_live";
            out.push(c);
        }
        let sigs: Vec<String> = facts.burst.iter().map(|s| s.payer.clone()).collect();
        let hit = bundle_rules(&facts.burst, &r.holders, &ex, now_ms()).filter(|h| h.code == "BUNDLE_SUPPLY");
        if hit.is_some() {
            out.push(bundle_check(hit, f.reject_bundle, &sigs));
        }
        out
    }

    /// RugCheck gave no answer in time. Judge the coin on our own chain data. ALL of these must pass:
    /// mint authority revoked, freeze authority revoked, Token-2022 extensions, dev snipe (and burst bundle), top holders,
    /// creator holding (launch buy and live balance), creator blacklist. Any block, or any check that cannot be run, stops the coin.
    pub async fn onchain_fallback(&self, t: &FeedMsg, facts: &mut ChainFacts, why: &str) -> Vec<CheckResult> {
        let f = &self.cfg.filters;
        let mint = t.mint.as_str();
        let mut out = vec![];
        // mint authority, freeze authority and Token-2022 extensions: re-judged from the account as read on-chain
        out.push(match &facts.mint_info {
            Some(m) => mint_rule(m, f.mint_authority_revoked, f.freeze_authority_revoked, mint),
            None => CheckResult::undecided("mint_account", "mint account was never read"),
        });
        // dev snipe and burst bundle from the chain stage must exist and have passed
        out.extend(fallback_core_gate(&facts.core));
        // creator blacklist, again at decision time
        out.push(blacklist_rule(self.db.blacklist_reason(&t.trader_public_key), &t.trader_public_key));
        // creator's launch buy
        out.push(creator_holding_rule(creator_initial_pct(t.initial_buy), f.max_creator_holding_pct, &t.trader_public_key));
        // top holders, straight from the chain
        let supply = facts.mint_info.as_ref().map(|m| m.supply).unwrap_or(0.0);
        let stub = self.holders_stub.lock().unwrap().clone();
        let fetched = match stub {
            Some(r) => Ok(r),
            None => timeout(Duration::from_secs(6), self.rpc.top_holders(mint, supply)).await,
        };
        match fetched {
            Ok(Ok(h)) => {
                let ex = if f.exclude_curve_and_pool_accounts { facts.excluded.clone() } else { HashSet::new() };
                out.push(holder_rule(&h, &ex, f.max_single_holder_pct, f.max_top_holders_combined_pct, f.max_insiders_pct, mint));
                // the creator's live balance, if they are among the biggest holders
                let live = h.iter().filter(|x| x.owner == t.trader_public_key).map(|x| x.pct).fold(0.0, f64::max);
                let mut c = creator_holding_rule(live, f.max_creator_holding_pct, &t.trader_public_key);
                c.name = "creator_holding_live";
                out.push(c);
                facts.holders = h;
            }
            Ok(Err(e)) => out.push(CheckResult::undecided("holders", format!("on-chain holder list failed: {e}"))),
            Err(_) => out.push(CheckResult::undecided("holders", "on-chain holder list timed out")),
        }
        if !out.iter().any(|r| r.blocks()) {
            out.push(CheckResult::not_available("rugcheck", format!("{RUG_FALLBACK_NOTE} (RugCheck: {why})")));
        }
        out
    }

    /// Scam check for ONE coin the user is looking at (Chrome extension "Check this coin"). It runs only the checks that work
    /// for a coin of any age (mint account, RugCheck report, holders) and returns results. It never scores, never picks and has
    /// no way to reach the Executor: the only thing that can come out of it is a verdict.
    pub async fn manual_check(&self, mint: &str) -> Vec<CheckResult> {
        let f = &self.cfg.filters;
        let mint_fut = async {
            match timeout(Duration::from_secs(8), self.rpc.get_mint(mint)).await {
                Ok(Ok(m)) => mint_rule(&m, f.mint_authority_revoked, f.freeze_authority_revoked, mint),
                Ok(Err(e)) => CheckResult::undecided("mint_account", e),
                Err(_) => CheckResult::undecided("mint_account", "timed out"),
            }
        };
        let rug_fut = async {
            // the shared client: 2 in flight, rate limited, cached; a few tries so a busy RugCheck is not a verdict
            match timeout(Duration::from_secs(14), self.rugcheck.report_with_retry(mint, |_, _| {})).await {
                Ok(Ok(v)) => Ok(parse_rugcheck(&v)),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("timed out".to_string()),
            }
        };
        let (mint_res, rug) = tokio::join!(mint_fut, rug_fut);
        let mut out = vec![mint_res, honeypot_stub()];
        match &rug {
            Ok(r) => {
                out.push(rugcheck_rule(r, mint));
                let ex: HashSet<String> = r.known_pools.iter().cloned().collect();
                out.push(holder_rule(&r.holders, &ex, f.max_single_holder_pct, f.max_top_holders_combined_pct, f.max_insiders_pct, mint));
            }
            Err(e) => {
                out.push(CheckResult::undecided("rugcheck", e.clone()));
                out.push(CheckResult::undecided("holders", format!("holders come from RugCheck: {e}")));
            }
        }
        out
    }

    /// Graded launch-slot check (SPEC 7b) on everything we know: the live trades plus the creation-slot trades read from the chain.
    /// Returns the measured group (logged for every coin) and the check result (a block only above the limits).
    pub fn launch_grade(&self, creator: &str, trades: &[Trade], facts: &ChainFacts, curve: Option<crate::curve::Curve>) -> (launch::LaunchExposure, CheckResult) {
        let g = &self.cfg.filters.graded;
        let mut all: Vec<Trade> = facts.launch_trades.clone();
        all.extend(trades.iter().cloned());
        // the creation slot also falls out of the earliest trade slot when the chain read gave nothing
        let slot0 = facts.create_slot.or_else(|| trades.iter().map(|t| t.slot).filter(|s| *s > 0).min());
        let curve = curve.or_else(|| trades.iter().max_by_key(|t| (t.slot, t.recv_ms)).map(|t| crate::curve::Curve { v_sol: t.v_sol, v_tokens: t.v_tokens, real_sol: t.real_sol }));
        let e = launch::launch_exposure(&all, creator, slot0, curve);
        let r = if g.enabled { launch::grade(&e, g) } else { CheckResult::pass("launch_grade", "Graded launch check is off (the hard first-block rule applies).") };
        (e, r)
    }

    /// Pre-buy checks on the full observation data: wash volume, chase risk, bundle patterns on real payers.
    pub fn pre_buy(&self, trades: &[Trade], facts: &ChainFacts) -> Vec<CheckResult> {
        let now = now_ms();
        let slot_txs: Vec<SlotTx> = trades.iter().map(|x| SlotTx { slot: x.slot, payer: x.user.clone(), recv_ms: x.recv_ms }).collect();
        let ex = if self.cfg.filters.exclude_curve_and_pool_accounts { facts.excluded.clone() } else { HashSet::new() };
        let sigs: Vec<String> = trades.iter().map(|x| x.signature.clone()).collect();
        let hit = bundle_rules(&slot_txs, &[], &ex, now).filter(|h| h.code != "BUNDLE_SUPPLY");
        let mut v = vec![bundle_check(hit, self.cfg.filters.reject_bundle, &sigs), wash_volume_rule(trades, now)];
        // a coin that turns out to be mayhem or non-SOL quote in its own trade events is excluded too
        v.extend(curve_type_rules(trades.iter().any(|t| t.mayhem), trades.iter().any(|t| t.non_sol_quote)));
        v
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

    // ---------------------------------------------------------------- RugCheck outage handling (SPEC 11b #29)

    use crate::pump::tests::REAL_CREATE;

    fn create_msg() -> FeedMsg {
        match crate::pump::parse_feed(REAL_CREATE) {
            crate::pump::Parsed::Create(m) => m,
            _ => panic!("not a create"),
        }
    }

    fn shield(answers: Vec<Result<Option<serde_json::Value>, String>>, require_rugcheck: bool) -> Shield {
        let mut cfg = Config::from_json(include_str!("../../../config.json")).unwrap();
        cfg.filters.require_rugcheck = require_rugcheck;
        let rc = Arc::new(net::RugCheck::scripted(answers, vec![Duration::from_millis(5), Duration::from_millis(5), Duration::from_millis(5)]));
        Shield::with_rugcheck(cfg, Arc::new(Db::open(":memory:").unwrap()), Arc::new(Rpc::new("http://127.0.0.1:9", 50.0)), ShieldOpts { relax_egress: false }, rc)
    }

    fn clean_mint() -> MintInfo {
        MintInfo { program: "spl-token-2022".into(), supply: 1.0e15, decimals: 6, ..Default::default() }
    }

    /// Facts as the chain stage leaves them: mint read and clean, dev snipe and bundle passed.
    fn facts() -> ChainFacts {
        let mut f = ChainFacts::default();
        f.excluded.insert(create_msg().bonding_curve_key);
        f.mint_info = Some(clean_mint());
        f.core = vec![CheckResult::pass("mint_account", "ok"), CheckResult::pass("dev_snipe", "none"), CheckResult::pass("bundle", "none")];
        f
    }

    fn holders(extra: Vec<(&str, f64)>) -> Result<Vec<Holder>, String> {
        let mut v = vec![Holder { owner: create_msg().bonding_curve_key, pct: 62.0, insider: false }];
        v.extend(extra.into_iter().map(|(o, p)| Holder { owner: o.into(), pct: p, insider: false }));
        Ok(v)
    }

    async fn settled(s: &Shield, f: &mut ChainFacts) {
        let mut rx = s.start_rugcheck(&create_msg().mint);
        for _ in 0..200 {
            if !matches!(rx.borrow().outcome, RugOutcome::Pending) {
                break;
            }
            let _ = tokio::time::timeout(Duration::from_millis(20), rx.changed()).await;
        }
        f.rug = Some(rx);
    }

    fn five_502() -> Vec<Result<Option<serde_json::Value>, String>> {
        (0..4).map(|_| Err("rugcheck http 502".to_string())).collect()
    }

    fn report(risks: serde_json::Value) -> serde_json::Value {
        json!({"rugged": false, "creator": "c", "token": {"supply": 1.0e15}, "topHolders": [{"owner": "w1", "pct": 2.0}], "risks": risks, "fileMeta": {"image": "https://i/x.png"}})
    }

    #[tokio::test]
    async fn rugcheck_runs_in_the_background_and_a_late_report_is_judged_at_full_size() {
        let s = shield(vec![Err("rugcheck http 502".into()), Ok(None), Ok(Some(report(json!([]))))], false);
        let t = create_msg();
        let mut f = facts();
        let mut rx = s.start_rugcheck(&t.mint);
        assert!(matches!(rx.borrow().outcome, RugOutcome::Pending), "the caller is not blocked while RugCheck retries");
        for _ in 0..200 {
            if matches!(rx.borrow().outcome, RugOutcome::Report(_)) {
                break;
            }
            let _ = tokio::time::timeout(Duration::from_millis(20), rx.changed()).await;
        }
        assert!(matches!(rx.borrow().outcome, RugOutcome::Report(_)));
        assert_eq!(rx.borrow().attempts, 2, "two failed tries were counted before the report");
        f.rug = Some(rx);
        let (gate, rec) = s.rug_gate(&t, &mut f).await;
        assert_eq!(gate, RugGate::Full);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].0, "rugcheck");
        assert!(rec[0].1.block.is_none());
    }

    #[tokio::test]
    async fn no_rugcheck_answer_and_every_onchain_check_passes_gives_half_size_and_the_recorded_verdict() {
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 3.0), ("w2", 2.0)]));
        let t = create_msg();
        let mut f = facts();
        settled(&s, &mut f).await;
        assert!(matches!(f.rug.as_ref().unwrap().borrow().outcome, RugOutcome::Unavailable(_)));
        let (gate, rec) = s.rug_gate(&t, &mut f).await;
        assert_eq!(gate, RugGate::Half);
        assert_eq!(FALLBACK_SIZE_FACTOR, 0.5);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].0, "rugcheck-fallback");
        let d = &rec[0].1;
        assert!(d.block.is_none());
        assert_eq!(d.verdict, "ELEVATED", "flagged, not LOW_SIGNAL");
        assert!(d.results.iter().any(|r| r.name == "rugcheck" && r.detail.contains(RUG_FALLBACK_NOTE)));
        // every on-chain check named in the spec really ran
        for name in ["mint_account", "dev_snipe", "bundle", "creator_blacklist", "creator_holding", "holders", "creator_holding_live"] {
            assert!(d.results.iter().any(|r| r.name == name || (name == "creator_blacklist" && r.name.contains("blacklist"))), "{name} did not run: {:?}", d.results.iter().map(|r| r.name).collect::<Vec<_>>());
        }
    }

    #[tokio::test]
    async fn a_rugcheck_danger_flag_still_blocks_even_though_onchain_checks_would_pass() {
        let danger = report(json!([{"name": "Copycat token", "level": "danger", "description": "Looks like another token"}]));
        let s = shield(vec![Ok(Some(danger))], false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 3.0)]));
        let t = create_msg();
        let mut f = facts();
        let mut rx = s.start_rugcheck(&t.mint);
        for _ in 0..200 {
            if matches!(rx.borrow().outcome, RugOutcome::Report(_)) { break; }
            let _ = tokio::time::timeout(Duration::from_millis(20), rx.changed()).await;
        }
        f.rug = Some(rx);
        let (gate, rec) = s.rug_gate(&t, &mut f).await;
        assert_eq!(gate, RugGate::Block("RUGCHECK_DANGER".into()));
        assert_eq!(rec[0].0, "rugcheck");
        assert_eq!(rec[0].1.verdict, "HIGH");
        // and the same danger report blocks when requireRugcheck is on too
        let s2 = shield(vec![Ok(Some(report(json!([{"name": "Copycat token", "level": "danger", "description": "x"}]))))], true);
        let mut f2 = facts();
        let mut rx2 = s2.start_rugcheck(&t.mint);
        for _ in 0..200 {
            if matches!(rx2.borrow().outcome, RugOutcome::Report(_)) { break; }
            let _ = tokio::time::timeout(Duration::from_millis(20), rx2.changed()).await;
        }
        f2.rug = Some(rx2);
        assert_eq!(s2.rug_gate(&t, &mut f2).await.0, RugGate::Block("RUGCHECK_DANGER".into()));
    }

    #[tokio::test]
    async fn require_rugcheck_on_blocks_a_coin_rugcheck_cannot_report_on() {
        let s = shield(five_502(), true);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 3.0)]));
        let mut f = facts();
        settled(&s, &mut f).await;
        let (gate, rec) = s.rug_gate(&create_msg(), &mut f).await;
        assert_eq!(gate, RugGate::Block("UNDECIDED_RUGCHECK".into()));
        assert_eq!(rec[0].0, "rugcheck");
    }

    async fn fallback_gate_with(s: &Shield, mut f: ChainFacts) -> RugGate {
        settled(s, &mut f).await;
        s.rug_gate(&create_msg(), &mut f).await.0
    }

    #[tokio::test]
    async fn the_fallback_blocks_on_each_onchain_failure() {
        // one wallet over 5%
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 9.0)]));
        assert_eq!(fallback_gate_with(&s, facts()).await, RugGate::Block("SINGLE_HOLDER".into()));
        // top 10 over 15% combined (each under 5%)
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders((0..6).map(|i| (["a", "b", "c", "d", "e", "f"][i], 4.0)).collect()));
        assert_eq!(fallback_gate_with(&s, facts()).await, RugGate::Block("TOP_HOLDERS".into()));
        // holder list unreadable
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(Err("rpc down".into()));
        assert_eq!(fallback_gate_with(&s, facts()).await, RugGate::Block("UNDECIDED_HOLDERS".into()));
        // mint authority still set
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![]));
        let mut f = facts();
        f.mint_info.as_mut().unwrap().mint_authority = Some("AuthAuthAuthAuthAuthAuth11111111111111111111".into());
        assert_eq!(fallback_gate_with(&s, f).await, RugGate::Block("MINT_AUTHORITY".into()));
        // freeze authority still set
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![]));
        let mut f = facts();
        f.mint_info.as_mut().unwrap().freeze_authority = Some("AuthAuthAuthAuthAuthAuth11111111111111111111".into());
        assert_eq!(fallback_gate_with(&s, f).await, RugGate::Block("FREEZE_AUTHORITY".into()));
        // a Token-2022 transfer hook
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![]));
        let mut f = facts();
        f.mint_info.as_mut().unwrap().transfer_hook = Some("HookHookHookHookHookHookHook1111111111111111".into());
        assert_eq!(fallback_gate_with(&s, f).await, RugGate::Block("TRANSFER_HOOK".into()));
        // the creator still holds more than 10% right now
        let s = shield(five_502(), false);
        let t = create_msg();
        *s.holders_stub.lock().unwrap() = Some(holders(vec![(t.trader_public_key.as_str(), 12.0)]));
        let g = fallback_gate_with(&s, facts()).await;
        assert!(matches!(&g, RugGate::Block(c) if c == "SINGLE_HOLDER" || c == "CREATOR_HOLDING"), "{g:?}");
        // creator on the blacklist
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 2.0)]));
        s.db.blacklist_creator(&t.trader_public_key, "rugged before", now_ms());
        assert_eq!(fallback_gate_with(&s, facts()).await, RugGate::Block("CREATOR_BLACKLISTED".into()));
        // the chain stage never ran dev snipe
        let s = shield(five_502(), false);
        *s.holders_stub.lock().unwrap() = Some(holders(vec![("w1", 2.0)]));
        let mut f = facts();
        f.core.retain(|r| r.name != "dev_snipe");
        assert_eq!(fallback_gate_with(&s, f).await, RugGate::Block("UNDECIDED_DEV_SNIPE".into()));
    }

    #[test]
    fn core_gate_needs_dev_snipe_and_bundle_to_have_passed() {
        let ok = vec![CheckResult::pass("dev_snipe", "x"), CheckResult::pass("bundle", "x")];
        assert!(fallback_core_gate(&ok).iter().all(|r| !r.blocks()));
        assert!(fallback_core_gate(&[]).iter().all(|r| r.blocks()));
        let snipe = vec![CheckResult::block("dev_snipe", "DEV_SNIPE", "x"), CheckResult::pass("bundle", "x")];
        assert!(fallback_core_gate(&snipe).iter().any(|r| r.blocks()));
        let flagged = vec![CheckResult::not_available("dev_snipe", "x"), CheckResult::pass("bundle", "x")];
        assert!(fallback_core_gate(&flagged).iter().any(|r| r.blocks()), "a flag is not a pass");
    }

    #[test]
    fn a_run_without_rugcheck_handle_is_unchanged() {
        // replays and old callers have no RugCheck handle: full size, no records
        let s = shield(vec![], false);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (g, rec) = rt.block_on(s.rug_gate(&create_msg(), &mut ChainFacts::default()));
        assert_eq!(g, RugGate::Full);
        assert!(rec.is_empty());
    }
}
