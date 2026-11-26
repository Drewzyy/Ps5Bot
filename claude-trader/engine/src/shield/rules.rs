//! Scam Shield rules as pure functions over plain data, so each one is unit-tested with fixtures.
//! SPEC sections 7 and 11b. Thresholds come from config.json (`filters`) or are the [est] values from the SPEC.

use crate::events::Evidence;
use crate::pump::{solscan_account, solscan_token, solscan_tx, Trade};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Block,
    /// Timed out or could not be determined. Blocks if the check is required.
    Undecided,
    /// Cannot run in this build/mode (for example honeypot simulation needs a wallet). Never blocks, always flagged.
    NotAvailable,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: &'static str,
    pub required: bool,
    pub outcome: Outcome,
    /// Machine code, e.g. MINT_AUTHORITY. Empty on pass.
    pub code: String,
    pub detail: String,
    pub evidence: Vec<Evidence>,
}

impl CheckResult {
    pub fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        CheckResult { name, required: true, outcome: Outcome::Pass, code: String::new(), detail: detail.into(), evidence: vec![] }
    }
    pub fn block(name: &'static str, code: &str, detail: impl Into<String>) -> Self {
        CheckResult { name, required: true, outcome: Outcome::Block, code: code.into(), detail: detail.into(), evidence: vec![] }
    }
    pub fn undecided(name: &'static str, detail: impl Into<String>) -> Self {
        CheckResult { name, required: true, outcome: Outcome::Undecided, code: format!("UNDECIDED_{}", name.to_ascii_uppercase()), detail: detail.into(), evidence: vec![] }
    }
    pub fn not_available(name: &'static str, detail: impl Into<String>) -> Self {
        CheckResult { name, required: false, outcome: Outcome::NotAvailable, code: String::new(), detail: detail.into(), evidence: vec![] }
    }
    pub fn with_required(mut self, r: bool) -> Self {
        self.required = r;
        self
    }
    pub fn with_evidence(mut self, e: Vec<Evidence>) -> Self {
        self.evidence = e;
        self
    }
    /// True if this result stops the token.
    pub fn blocks(&self) -> bool {
        match self.outcome {
            Outcome::Block => true,
            Outcome::Undecided => self.required,
            _ => false,
        }
    }
}

pub fn ev(label: &str, url: String) -> Evidence {
    Evidence { label: label.into(), url }
}

// ---------------------------------------------------------------- 1. mint account (SPEC 11b #1)

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MintInfo {
    pub program: String,
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    pub supply: f64,
    pub decimals: u8,
    /// transferHook extension: program id (None = extension absent, Some("") = extension present with no program)
    pub transfer_hook: Option<String>,
    pub permanent_delegate: Option<String>,
    pub transfer_fee_bps: u32,
    pub close_authority: Option<String>,
}

/// Parse the `jsonParsed` result of getAccountInfo on a mint account, including Token-2022 extensions.
/// `value` is the `result.value` object. Returns Err if it is not a mint account.
pub fn parse_mint_account(value: &Value) -> Result<MintInfo, String> {
    let parsed = &value["data"]["parsed"];
    if parsed["type"].as_str() != Some("mint") {
        return Err("account is not a token mint".into());
    }
    let info = &parsed["info"];
    let s = |v: &Value| v.as_str().filter(|x| !x.is_empty()).map(|x| x.to_string());
    let mut m = MintInfo {
        program: value["data"]["program"].as_str().unwrap_or("").to_string(),
        mint_authority: s(&info["mintAuthority"]),
        freeze_authority: s(&info["freezeAuthority"]),
        supply: info["supply"].as_str().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0),
        decimals: info["decimals"].as_u64().unwrap_or(0) as u8,
        ..Default::default()
    };
    if let Some(exts) = info["extensions"].as_array() {
        for e in exts {
            let st = &e["state"];
            match e["extension"].as_str().unwrap_or("") {
                "transferHook" => {
                    m.transfer_hook = Some(s(&st["programId"]).unwrap_or_default());
                }
                "permanentDelegate" => {
                    m.permanent_delegate = Some(s(&st["delegate"]).unwrap_or_default());
                }
                "transferFeeConfig" => {
                    let older = st["olderTransferFee"]["transferFeeBasisPoints"].as_u64().unwrap_or(0);
                    let newer = st["newerTransferFee"]["transferFeeBasisPoints"].as_u64().unwrap_or(0);
                    m.transfer_fee_bps = older.max(newer) as u32;
                }
                "mintCloseAuthority" => {
                    m.close_authority = s(&st["closeAuthority"]);
                }
                _ => {}
            }
        }
    }
    Ok(m)
}

/// Reject if: mint authority set, freeze authority set, transferHook with a program, permanentDelegate,
/// transferFee > 0 bps, closeAuthority. Token-2022 itself is fine (pump `create_v2` coins all use it).
pub fn mint_rule(m: &MintInfo, require_mint_revoked: bool, require_freeze_revoked: bool, mint: &str) -> CheckResult {
    let name = "mint_account";
    let evid = vec![ev("Mint on Solscan", solscan_token(mint))];
    let block = |code: &str, d: String| CheckResult::block(name, code, d).with_evidence(evid.clone());
    if require_mint_revoked {
        if let Some(a) = &m.mint_authority {
            return block("MINT_AUTHORITY", format!("Mint authority is still set ({}). The creator can print unlimited new coins.", short(a)));
        }
    }
    if require_freeze_revoked {
        if let Some(a) = &m.freeze_authority {
            return block("FREEZE_AUTHORITY", format!("Freeze authority is still set ({}). Your coins could be frozen so you cannot sell.", short(a)));
        }
    }
    if let Some(p) = &m.transfer_hook {
        if !p.is_empty() {
            return block("TRANSFER_HOOK", format!("Token-2022 transfer hook runs program {}. It can block or tax sells.", short(p)));
        }
    }
    if let Some(d) = &m.permanent_delegate {
        return block("PERMANENT_DELEGATE", format!("Token-2022 permanent delegate {} can move or burn anyone's coins.", short(d)));
    }
    if m.transfer_fee_bps > 0 {
        return block("TRANSFER_FEE", format!("Token-2022 transfer fee of {} bps is charged on every transfer.", m.transfer_fee_bps));
    }
    if let Some(c) = &m.close_authority {
        return block("CLOSE_AUTHORITY", format!("Mint close authority {} is set.", short(c)));
    }
    CheckResult::pass(name, format!("Mint and freeze authority revoked; no dangerous extensions ({}).", if m.program.contains("2022") { "Token-2022" } else { "SPL Token" })).with_evidence(evid)
}

pub fn short(a: &str) -> String {
    if a.len() > 10 { format!("{}..{}", &a[..4], &a[a.len() - 4..]) } else { a.to_string() }
}

// ---------------------------------------------------------------- 2. honeypot (SPEC 11b #2) STUB

/// Honeypot sell simulation. A faithful simulation needs a funded token account that owns the coin and a
/// transaction built by the PumpPortal local-trade API (signing-adjacent code), which only exists in M4. Not run in paper mode.
pub fn honeypot_stub() -> CheckResult {
    CheckResult::not_available("honeypot", "Sell simulation is not available in paper mode (needs a wallet that holds the coin). Re-checked before M4.")
}

// ---------------------------------------------------------------- 3. creator holding (SPEC 11b #3)

/// Creator's share of supply from the create message: initialBuy (tokens) / 1B supply.
pub fn creator_initial_pct(initial_buy_tokens: f64) -> f64 {
    initial_buy_tokens / crate::pump::SUPPLY_TOKENS * 100.0
}

pub fn creator_holding_rule(pct: f64, max_pct: f64, creator: &str) -> CheckResult {
    let name = "creator_holding";
    if pct > max_pct {
        CheckResult::block(name, "CREATOR_HOLDING", format!("Creator holds {pct:.1}% of the supply (limit {max_pct}%)."))
            .with_evidence(vec![ev("Creator on Solscan", solscan_account(creator))])
    } else {
        CheckResult::pass(name, format!("Creator holds {pct:.1}% (limit {max_pct}%)."))
    }
}

// ---------------------------------------------------------------- holder concentration (SPEC 6)

#[derive(Debug, Clone, PartialEq)]
pub struct Holder {
    pub owner: String,
    pub pct: f64,
    pub insider: bool,
}

/// Single holder <= max_single, top-10 combined <= max_combined, insiders <= max_insiders.
/// Bonding-curve and pool accounts are excluded from every number (`excluded` owners).
pub fn holder_rule(holders: &[Holder], excluded: &HashSet<String>, max_single: f64, max_combined: f64, max_insiders: f64, mint: &str) -> CheckResult {
    let name = "holders";
    let mut real: Vec<&Holder> = holders.iter().filter(|h| !excluded.contains(&h.owner)).collect();
    real.sort_by(|a, b| b.pct.partial_cmp(&a.pct).unwrap_or(std::cmp::Ordering::Equal));
    let evid = vec![ev("Holders on Solscan", format!("{}#holders", solscan_token(mint)))];
    if let Some(top) = real.first() {
        if top.pct > max_single {
            return CheckResult::block(name, "SINGLE_HOLDER", format!("One wallet ({}) holds {:.1}% of supply (limit {max_single}%), curve and pool accounts excluded.", short(&top.owner), top.pct)).with_evidence(evid);
        }
    }
    let top10: f64 = real.iter().take(10).map(|h| h.pct).sum();
    if top10 > max_combined {
        return CheckResult::block(name, "TOP_HOLDERS", format!("Top 10 wallets hold {top10:.1}% combined (limit {max_combined}%), curve and pool accounts excluded.")).with_evidence(evid);
    }
    let insiders: f64 = real.iter().filter(|h| h.insider).map(|h| h.pct).sum();
    if insiders > max_insiders {
        return CheckResult::block(name, "INSIDERS", format!("Wallets flagged as insiders hold {insiders:.1}% (limit {max_insiders}%).")).with_evidence(evid);
    }
    CheckResult::pass(name, format!("Largest wallet {:.1}%, top 10 {top10:.1}%, insiders {insiders:.1}% (curve/pool excluded).", real.first().map(|h| h.pct).unwrap_or(0.0))).with_evidence(evid)
}

// ---------------------------------------------------------------- RugCheck report (SPEC 7)

#[derive(Debug, Clone, Default)]
pub struct RugReport {
    pub rugged: bool,
    pub creator: String,
    pub creator_balance_pct: Option<f64>,
    pub image: Option<String>,
    /// True when RugCheck has read the token's metadata file (fileMeta present). New coins are often not read yet.
    pub meta_known: bool,
    pub holders: Vec<Holder>,
    /// Owners that RugCheck itself labels as AMM / pool / curve / locker.
    pub known_pools: HashSet<String>,
    pub danger_risks: Vec<String>,
    pub warn_risks: Vec<String>,
    pub insider_networks: usize,
    pub mint_authority_set: bool,
    pub freeze_authority_set: bool,
}

pub fn parse_rugcheck(v: &Value) -> RugReport {
    let mut r = RugReport {
        rugged: v["rugged"].as_bool().unwrap_or(false),
        creator: v["creator"].as_str().unwrap_or("").to_string(),
        image: v["fileMeta"]["image"].as_str().filter(|s| !s.is_empty()).map(String::from),
        meta_known: v["fileMeta"].is_object(),
        mint_authority_set: !v["mintAuthority"].is_null(),
        freeze_authority_set: !v["freezeAuthority"].is_null(),
        insider_networks: v["insiderNetworks"].as_array().map(|a| a.len()).unwrap_or(0),
        ..Default::default()
    };
    let supply = v["token"]["supply"].as_f64().unwrap_or(0.0);
    if let Some(b) = v["creatorBalance"].as_f64() {
        if supply > 0.0 {
            r.creator_balance_pct = Some(b / supply * 100.0);
        }
    }
    if let Some(hs) = v["topHolders"].as_array() {
        for h in hs {
            r.holders.push(Holder {
                owner: h["owner"].as_str().or_else(|| h["address"].as_str()).unwrap_or("").to_string(),
                pct: h["pct"].as_f64().unwrap_or(0.0),
                insider: h["insider"].as_bool().unwrap_or(false),
            });
        }
    }
    if let Some(ka) = v["knownAccounts"].as_object() {
        for (k, val) in ka {
            let ty = val["type"].as_str().unwrap_or("");
            if matches!(ty, "AMM" | "LOCKER" | "POOL" | "CURVE") {
                r.known_pools.insert(k.clone());
            }
        }
    }
    if let Some(risks) = v["risks"].as_array() {
        for k in risks {
            let label = format!("{}: {}", k["name"].as_str().unwrap_or("risk"), k["description"].as_str().unwrap_or(""));
            // RugCheck judges bonding-curve coins as pools: every new pump coin has "low liquidity", and its holder
            // percentages include the curve account. Our own rules (holders excl. curve) cover those, so they
            // are kept as warnings only. Everything else at "danger" level still blocks.
            let lname = k["name"].as_str().unwrap_or("").to_lowercase();
            let curve_artifact = ["liquidity", "single holder", "holder concentration", "amount of holders", "top 10"].iter().any(|p| lname.contains(p));
            match k["level"].as_str().unwrap_or("") {
                "danger" if curve_artifact => r.warn_risks.push(label),
                "danger" => r.danger_risks.push(label),
                "warn" => r.warn_risks.push(label),
                _ => {}
            }
        }
    }
    r
}

/// Block on a rugged flag or any "danger" level risk. Warnings are kept as flags (ELEVATED).
pub fn rugcheck_rule(r: &RugReport, mint: &str) -> CheckResult {
    let name = "rugcheck";
    let evid = vec![ev("RugCheck report", format!("https://rugcheck.xyz/tokens/{mint}"))];
    if r.rugged {
        return CheckResult::block(name, "RUGCHECK_RUGGED", "RugCheck marks this token as already rugged.").with_evidence(evid);
    }
    if let Some(d) = r.danger_risks.first() {
        return CheckResult::block(name, "RUGCHECK_DANGER", format!("RugCheck danger flag: {d}")).with_evidence(evid);
    }
    if r.warn_risks.is_empty() {
        CheckResult::pass(name, "RugCheck reports no risks.").with_evidence(evid)
    } else {
        // Warnings do not block, but are shown and raise the verdict to ELEVATED.
        let mut c = CheckResult::pass(name, format!("RugCheck warnings: {}", r.warn_risks.join("; "))).with_evidence(evid);
        c.code = "RUGCHECK_WARN".into();
        c
    }
}

// ---------------------------------------------------------------- 5. metadata / community (SPEC 6, 11b #6)

pub fn metadata_present_rule(name_: &str, symbol: &str, uri: &str) -> CheckResult {
    if name_.trim().is_empty() || symbol.trim().is_empty() || uri.trim().is_empty() {
        CheckResult::block("metadata", "METADATA_MISSING", "Name, symbol or metadata link is missing.")
    } else {
        CheckResult::pass("metadata", "Name, symbol and metadata link present.")
    }
}

/// Result of trying to open the metadata JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaFetch {
    Ok { image: Option<String>, social_links: u32 },
    /// HTTP error, bad JSON or timeout.
    Failed(String),
    /// The network policy of this machine refuses the host (sandbox proxy). Not the token's fault.
    EgressBlocked,
}

/// Count social links in a pump.fun metadata JSON: twitter, telegram, website (non-empty).
pub fn parse_metadata_json(v: &Value) -> MetaFetch {
    let nz = |k: &str| v[k].as_str().map(|s| !s.trim().is_empty()).unwrap_or(false);
    let links = ["twitter", "telegram", "website"].iter().filter(|k| nz(k)).count() as u32;
    MetaFetch::Ok { image: v["image"].as_str().filter(|s| !s.is_empty()).map(String::from), social_links: links }
}

/// `relax_egress`: when the host machine cannot reach metadata hosts (sandbox only), treat EgressBlocked as flagged pass.
pub fn metadata_uri_rule(f: &MetaFetch, relax_egress: bool) -> CheckResult {
    let name = "metadata_uri";
    match f {
        MetaFetch::Ok { .. } => CheckResult::pass(name, "Metadata link opens within 2 s."),
        MetaFetch::Failed(e) => CheckResult::block(name, "METADATA_UNRESOLVED", format!("Metadata link did not open within 2 s ({e}).")),
        MetaFetch::EgressBlocked if relax_egress => CheckResult::not_available(name, "Metadata host unreachable from this machine's network policy; relaxed (sandbox only)."),
        MetaFetch::EgressBlocked => CheckResult::undecided(name, "Metadata host unreachable from this machine's network policy."),
    }
}

/// What RugCheck knows about the image.
#[derive(Debug, Clone, PartialEq)]
pub enum RugImage {
    Present,
    Absent,
    /// RugCheck has not read the metadata file yet (or no report).
    Unknown,
}

pub fn rug_image(r: &RugReport) -> RugImage {
    if r.image.is_some() {
        RugImage::Present
    } else if r.meta_known {
        RugImage::Absent
    } else {
        RugImage::Unknown
    }
}

pub fn community_rule(f: &MetaFetch, rug: &RugImage, min_social: u32, require_image: bool, relax_egress: bool) -> CheckResult {
    let name = "community";
    match f {
        MetaFetch::Ok { image, social_links } => {
            if require_image && image.is_none() && *rug != RugImage::Present {
                return CheckResult::block(name, "NO_IMAGE", "Token has no image.");
            }
            if *social_links < min_social {
                return CheckResult::block(name, "NO_SOCIALS", format!("Token has {social_links} social links (needs {min_social})."));
            }
            CheckResult::pass(name, format!("Image present, {social_links} social link(s)."))
        }
        _ => {
            // The metadata file could not be read. RugCheck may still tell us about the image; socials stay unknown.
            // RugCheck fills fileMeta from on-chain data even when it could not fetch the file, so an empty image
            // there is only trusted when we are not in the egress-relaxed sandbox mode.
            if require_image && *rug == RugImage::Absent && !(relax_egress && matches!(f, MetaFetch::EgressBlocked)) {
                return CheckResult::block(name, "NO_IMAGE", "Token has no image.");
            }
            if relax_egress && matches!(f, MetaFetch::EgressBlocked) {
                let img = if *rug == RugImage::Present { "Image present (via RugCheck)" } else { "Image not verifiable yet" };
                CheckResult::not_available(name, format!("{img}; social links unverifiable from this machine (relaxed, sandbox only)."))
            } else {
                CheckResult::undecided(name, "Could not read the metadata file to count social links.")
            }
        }
    }
}

// ---------------------------------------------------------------- 4. creator blacklist (SPEC 11b #4)

pub fn blacklist_rule(reason: Option<String>, creator: &str) -> CheckResult {
    match reason {
        Some(r) => CheckResult::block("creator_blacklist", "CREATOR_BLACKLISTED", format!("Creator is on the local blacklist: {r}."))
            .with_evidence(vec![ev("Creator on Solscan", solscan_account(creator))]),
        None => CheckResult::pass("creator_blacklist", "Creator not on the blacklist."),
    }
}

/// Serial-rugger rule: blacklist a creator whose token fell more than 70% from its peak within 5 minutes [est].
pub fn should_blacklist_creator(peak_mcap: f64, now_mcap: f64, age_secs: f64) -> bool {
    age_secs <= 300.0 && peak_mcap > 0.0 && (now_mcap / peak_mcap) < 0.30
}

// ---------------------------------------------------------------- 6. copycat (SPEC 11b #6)

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Allowed edit distance: SPEC says 2. Very short strings would match nearly anything, so they are tightened:
/// length >= 6 -> 2, length 4-5 -> 1, shorter -> exact match only.
fn allowed_distance(len: usize) -> usize {
    match len {
        0..=3 => 0,
        4..=5 => 1,
        _ => 2,
    }
}

fn near(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    if a.is_empty() || b.is_empty() {
        return false;
    }
    levenshtein(&a, &b) <= allowed_distance(a.chars().count().min(b.chars().count()))
}

pub fn copycat_rule(name_: &str, symbol: &str, trending: &[(String, String)]) -> CheckResult {
    for (n, s) in trending {
        if near(name_, n) || near(symbol, s) {
            return CheckResult::block("copycat", "COPYCAT", format!("Name or symbol is almost the same as '{n}' ({s}), seen trending in the last 24 h."));
        }
    }
    CheckResult::pass("copycat", "No look-alike among tokens seen trending in 24 h.")
}

// ---------------------------------------------------------------- bundles (SPEC 7)

/// One transaction touching the token: (slot, payer wallet).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotTx {
    pub slot: u64,
    pub payer: String,
    pub recv_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleHit {
    pub code: &'static str,
    pub detail: String,
    /// ACTIVE if the last matching tx was within 15 s of now, else HISTORICAL.
    pub tag: &'static str,
}

/// Bundle rules (SPEC 7): >= 20 transfers from <= 5 payers; >= 15 burst txs in one block;
/// >= 80% of supply held by <= 3 wallets (curve/pool excluded).
pub fn bundle_rules(txs: &[SlotTx], holders: &[Holder], excluded: &HashSet<String>, now_ms: i64) -> Option<BundleHit> {
    let tag = |last_ms: i64| if now_ms - last_ms <= 15_000 { "ACTIVE" } else { "HISTORICAL" };
    // burst: >= 15 txs in one slot
    let mut per_slot: HashMap<u64, Vec<&SlotTx>> = HashMap::new();
    for t in txs {
        per_slot.entry(t.slot).or_default().push(t);
    }
    if let Some((slot, v)) = per_slot.iter().filter(|(_, v)| v.len() >= 15).max_by_key(|(_, v)| v.len()) {
        let last = v.iter().map(|t| t.recv_ms).max().unwrap_or(0);
        return Some(BundleHit { code: "BUNDLE_BURST", detail: format!("{} transactions landed in one block (slot {slot}), a coordinated bundle.", v.len()), tag: tag(last) });
    }
    // >= 20 transfers from <= 5 payers
    if txs.len() >= 20 {
        let payers: HashSet<&str> = txs.iter().map(|t| t.payer.as_str()).collect();
        if payers.len() <= 5 {
            let last = txs.iter().map(|t| t.recv_ms).max().unwrap_or(0);
            return Some(BundleHit { code: "BUNDLE_PAYERS", detail: format!("{} transactions came from only {} wallets.", txs.len(), payers.len()), tag: tag(last) });
        }
    }
    // >= 80% of supply from <= 3 wallets
    let mut real: Vec<f64> = holders.iter().filter(|h| !excluded.contains(&h.owner)).map(|h| h.pct).collect();
    real.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let top3: f64 = real.iter().take(3).sum();
    if top3 >= 80.0 {
        return Some(BundleHit { code: "BUNDLE_SUPPLY", detail: format!("3 wallets hold {top3:.0}% of supply."), tag: "ACTIVE" });
    }
    None
}

pub fn bundle_check(hit: Option<BundleHit>, reject: bool, sigs: &[String]) -> CheckResult {
    match hit {
        Some(h) if reject => {
            let mut c = CheckResult::block("bundle", h.code, format!("{} Tag: {}.", h.detail, h.tag));
            c.evidence = sigs.iter().take(3).enumerate().map(|(i, s)| ev(&format!("Tx {}", i + 1), solscan_tx(s))).collect();
            c
        }
        Some(h) => CheckResult::pass("bundle", format!("Bundle detected but rejectBundle is off. {}", h.detail)),
        None => CheckResult::pass("bundle", "No bundle pattern found."),
    }
}

// ---------------------------------------------------------------- dev snipe (first block)

/// `sigs`: (signature, slot, failed) of transactions touching the bonding curve, `create_sig` is the create transaction.
/// Block if any other successful transaction landed in the same slot as the create.
pub fn dev_snipe_rule(sigs: &[(String, u64, bool)], create_sig: &str, reject: bool) -> CheckResult {
    let name = "dev_snipe";
    let Some(create_slot) = sigs.iter().find(|s| s.0 == create_sig).map(|s| s.1) else {
        return CheckResult::undecided(name, "Create transaction not found in the curve's signature list.");
    };
    let others: Vec<&(String, u64, bool)> = sigs.iter().filter(|s| s.1 == create_slot && s.0 != create_sig && !s.2).collect();
    if others.is_empty() {
        return CheckResult::pass(name, "No other buyer in the creation block.");
    }
    if !reject {
        return CheckResult::pass(name, format!("{} snipe(s) in first block but rejectDevSnipeFirstBlock is off.", others.len()));
    }
    let mut c = CheckResult::block(name, "DEV_SNIPE", format!("{} other transaction(s) landed in the same block as the launch (slot {create_slot}).", others.len()));
    c.evidence = others.iter().take(3).enumerate().map(|(i, s)| ev(&format!("Snipe tx {}", i + 1), solscan_tx(&s.0))).collect();
    c
}

// ---------------------------------------------------------------- 5. wash volume (SPEC 11b #5)

/// At least 15 distinct buyer wallets in the last 60 s, and the top 3 wallets under 40% of trades [est].
pub fn wash_volume_rule(trades: &[Trade], now_ms: i64) -> CheckResult {
    let name = "wash_volume";
    let recent: Vec<&Trade> = trades.iter().filter(|t| now_ms - t.recv_ms <= 60_000).collect();
    let buyers: HashSet<&str> = recent.iter().filter(|t| t.is_buy).map(|t| t.user.as_str()).collect();
    if buyers.len() < 15 {
        return CheckResult::block(name, "WASH_VOLUME", format!("Only {} distinct buyer wallets in the last 60 s (needs 15), volume looks thin or staged.", buyers.len()));
    }
    let mut per: HashMap<&str, usize> = HashMap::new();
    for t in &recent {
        *per.entry(t.user.as_str()).or_default() += 1;
    }
    let mut counts: Vec<usize> = per.values().copied().collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    let top3: usize = counts.iter().take(3).sum();
    let share = top3 as f64 / recent.len().max(1) as f64 * 100.0;
    if share >= 40.0 {
        return CheckResult::block(name, "WASH_VOLUME", format!("Top 3 wallets make {share:.0}% of trades (limit 40%)."));
    }
    CheckResult::pass(name, format!("{} distinct buyers in 60 s; top 3 wallets make {share:.0}% of trades.", buyers.len()))
}

// ---------------------------------------------------------------- 7. chase risk (SPEC 11b #7)

/// No entry if market cap already rose more than 3x from launch within 60 s [est].
pub fn chase_rule(launch_mcap: f64, now_mcap: f64, age_secs: f64) -> CheckResult {
    let name = "chase_risk";
    if launch_mcap > 0.0 && age_secs <= 60.0 && now_mcap / launch_mcap > 3.0 {
        CheckResult::block(name, "CHASE_RISK", format!("Market cap already rose {:.1}x in {age_secs:.0} s. Chasing a spike.", now_mcap / launch_mcap))
    } else {
        CheckResult::pass(name, "Price has not spiked more than 3x from launch.")
    }
}

// ---------------------------------------------------------------- verdict (SPEC 7 output)

/// Exit-liquidity risk verdict.
pub fn verdict_for(results: &[CheckResult]) -> &'static str {
    const HARD: [&str; 17] = [
        "MINT_AUTHORITY", "FREEZE_AUTHORITY", "TRANSFER_HOOK", "PERMANENT_DELEGATE", "TRANSFER_FEE", "CLOSE_AUTHORITY",
        "CREATOR_HOLDING", "SINGLE_HOLDER", "TOP_HOLDERS", "INSIDERS", "BUNDLE_BURST", "BUNDLE_PAYERS", "BUNDLE_SUPPLY",
        "DEV_SNIPE", "CREATOR_BLACKLISTED", "RUGCHECK_RUGGED", "RUGCHECK_DANGER",
    ];
    if results.iter().any(|r| r.outcome == Outcome::Block && HARD.contains(&r.code.as_str())) {
        return "HIGH";
    }
    // The honeypot stub is always "not available" in paper mode, so it is shown but does not raise the verdict.
    let flagged = results.iter().any(|r| (r.outcome != Outcome::Pass && r.name != "honeypot") || r.code == "RUGCHECK_WARN");
    if flagged { "ELEVATED" } else { "LOW_SIGNAL" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const M: &str = "MintMintMintMintMintMintMintMintMintMint111";

    fn mint_json(ext: Value, mint_auth: Value, freeze: Value) -> Value {
        json!({"data":{"program":"spl-token-2022","parsed":{"type":"mint","info":{
            "decimals":6,"supply":"1000000000000000","mintAuthority":mint_auth,"freezeAuthority":freeze,"extensions":ext}}}})
    }

    #[test]
    fn mint_clean_token2022_passes() {
        // Real shape captured from a live pump create_v2 token: metadataPointer + tokenMetadata only.
        let v = mint_json(json!([{"extension":"metadataPointer","state":{"authority":null,"metadataAddress":"x"}},{"extension":"tokenMetadata","state":{"name":"n"}}]), Value::Null, Value::Null);
        let m = parse_mint_account(&v).unwrap();
        let r = mint_rule(&m, true, true, M);
        assert_eq!(r.outcome, Outcome::Pass, "{}", r.detail);
    }

    #[test]
    fn mint_authority_blocks() {
        let v = mint_json(json!([]), json!("AuthAuthAuthAuthAuthAuth11111111111111111111"), Value::Null);
        let r = mint_rule(&parse_mint_account(&v).unwrap(), true, true, M);
        assert_eq!((r.outcome, r.code.as_str()), (Outcome::Block, "MINT_AUTHORITY"));
        assert!(!r.evidence.is_empty());
        // and is ignored when the config switch is off
        assert_eq!(mint_rule(&parse_mint_account(&v).unwrap(), false, true, M).outcome, Outcome::Pass);
    }

    #[test]
    fn freeze_authority_blocks() {
        let v = mint_json(json!([]), Value::Null, json!("FrzFrzFrzFrzFrzFrz1111111111111111111111"));
        assert_eq!(mint_rule(&parse_mint_account(&v).unwrap(), true, true, M).code, "FREEZE_AUTHORITY");
    }

    #[test]
    fn transfer_hook_blocks_only_with_program() {
        let with = mint_json(json!([{"extension":"transferHook","state":{"authority":null,"programId":"HookHookHookHookHook1111111111111111111111"}}]), Value::Null, Value::Null);
        assert_eq!(mint_rule(&parse_mint_account(&with).unwrap(), true, true, M).code, "TRANSFER_HOOK");
        let without = mint_json(json!([{"extension":"transferHook","state":{"authority":null,"programId":null}}]), Value::Null, Value::Null);
        assert_eq!(mint_rule(&parse_mint_account(&without).unwrap(), true, true, M).outcome, Outcome::Pass);
    }

    #[test]
    fn permanent_delegate_blocks() {
        let v = mint_json(json!([{"extension":"permanentDelegate","state":{"delegate":"DelDelDelDelDelDelDel111111111111111111111"}}]), Value::Null, Value::Null);
        assert_eq!(mint_rule(&parse_mint_account(&v).unwrap(), true, true, M).code, "PERMANENT_DELEGATE");
    }

    #[test]
    fn transfer_fee_blocks_when_positive() {
        let v = mint_json(json!([{"extension":"transferFeeConfig","state":{"olderTransferFee":{"transferFeeBasisPoints":0},"newerTransferFee":{"transferFeeBasisPoints":500}}}]), Value::Null, Value::Null);
        let r = mint_rule(&parse_mint_account(&v).unwrap(), true, true, M);
        assert_eq!(r.code, "TRANSFER_FEE");
        let zero = mint_json(json!([{"extension":"transferFeeConfig","state":{"olderTransferFee":{"transferFeeBasisPoints":0},"newerTransferFee":{"transferFeeBasisPoints":0}}}]), Value::Null, Value::Null);
        assert_eq!(mint_rule(&parse_mint_account(&zero).unwrap(), true, true, M).outcome, Outcome::Pass);
    }

    #[test]
    fn close_authority_blocks() {
        let v = mint_json(json!([{"extension":"mintCloseAuthority","state":{"closeAuthority":"ClsClsClsClsClsClsCls1111111111111111111111"}}]), Value::Null, Value::Null);
        assert_eq!(mint_rule(&parse_mint_account(&v).unwrap(), true, true, M).code, "CLOSE_AUTHORITY");
    }

    #[test]
    fn non_mint_account_is_error() {
        assert!(parse_mint_account(&json!({"data":{"parsed":{"type":"account"}}})).is_err());
    }

    #[test]
    fn honeypot_is_honestly_stubbed() {
        let h = honeypot_stub();
        assert_eq!(h.outcome, Outcome::NotAvailable);
        assert!(!h.blocks());
    }

    #[test]
    fn creator_holding_limit() {
        assert_eq!(creator_initial_pct(150_000_000.0), 15.0);
        assert_eq!(creator_holding_rule(15.0, 10.0, "c").code, "CREATOR_HOLDING");
        assert_eq!(creator_holding_rule(10.0, 10.0, "c").outcome, Outcome::Pass);
        assert_eq!(creator_holding_rule(0.0, 10.0, "c").outcome, Outcome::Pass);
    }

    fn hs(v: &[(&str, f64, bool)]) -> Vec<Holder> {
        v.iter().map(|(o, p, i)| Holder { owner: o.to_string(), pct: *p, insider: *i }).collect()
    }

    #[test]
    fn holder_curve_is_excluded() {
        let ex: HashSet<String> = ["curve".to_string()].into();
        // curve holds 90% but must not count
        let h = hs(&[("curve", 90.0, false), ("a", 3.0, false), ("b", 2.0, false)]);
        assert_eq!(holder_rule(&h, &ex, 5.0, 15.0, 20.0, M).outcome, Outcome::Pass);
    }

    #[test]
    fn holder_single_over_limit_blocks() {
        let ex: HashSet<String> = ["curve".to_string()].into();
        let h = hs(&[("curve", 70.0, false), ("whale", 6.5, false), ("b", 2.0, false)]);
        let r = holder_rule(&h, &ex, 5.0, 15.0, 20.0, M);
        assert_eq!(r.code, "SINGLE_HOLDER");
    }

    #[test]
    fn holder_top10_combined_blocks() {
        let ex = HashSet::new();
        let h: Vec<Holder> = (0..8).map(|i| Holder { owner: format!("w{i}"), pct: 2.5, insider: false }).collect(); // 20% combined, none over 5
        assert_eq!(holder_rule(&h, &ex, 5.0, 15.0, 20.0, M).code, "TOP_HOLDERS");
    }

    #[test]
    fn holder_insiders_block() {
        let ex = HashSet::new();
        let h = hs(&[("a", 4.0, true), ("b", 4.0, true), ("c", 4.0, true), ("d", 4.0, true), ("e", 4.5, true)]);
        // combined 20.5 > 15 hits first; use a looser combined limit to isolate the insider rule
        assert_eq!(holder_rule(&h, &ex, 5.0, 50.0, 20.0, M).code, "INSIDERS");
    }

    #[test]
    fn rugcheck_fixture_parse_and_rules() {
        // Trimmed real report shape (live capture) plus a danger risk.
        let clean = json!({"rugged":false,"creator":"cr","creatorBalance":0,"token":{"supply":1000000000000000u64},
            "fileMeta":{"image":"https://i/img"},"topHolders":[{"address":"ta","owner":"curve","pct":100.0,"insider":false}],
            "knownAccounts":{"curve":{"name":"Pump Fun","type":"AMM"}},"risks":[],"mintAuthority":null,"freezeAuthority":null});
        let r = parse_rugcheck(&clean);
        assert!(r.known_pools.contains("curve"));
        assert_eq!(r.holders.len(), 1);
        assert_eq!(r.image.as_deref(), Some("https://i/img"));
        assert_eq!(rugcheck_rule(&r, M).outcome, Outcome::Pass);

        let danger = json!({"rugged":false,"risks":[{"name":"Mutable metadata","level":"danger","description":"can change","score":1000}]});
        assert_eq!(rugcheck_rule(&parse_rugcheck(&danger), M).code, "RUGCHECK_DANGER");
        // bonding-curve artifacts reported as "danger" by RugCheck only warn
        let artifact = json!({"risks":[{"name":"Low Liquidity","level":"danger","description":"x"},{"name":"Single holder ownership","level":"danger","description":"curve"}]});
        let ra = rugcheck_rule(&parse_rugcheck(&artifact), M);
        assert_eq!(ra.outcome, Outcome::Pass);
        assert_eq!(ra.code, "RUGCHECK_WARN");
        let history = json!({"risks":[{"name":"Creator history of rugged tokens","level":"danger","description":"rugged before"}]});
        assert_eq!(rugcheck_rule(&parse_rugcheck(&history), M).code, "RUGCHECK_DANGER");
        let warn = json!({"risks":[{"name":"Low amount of holders","level":"warn","description":"few"}]});
        let rr = rugcheck_rule(&parse_rugcheck(&warn), M);
        assert_eq!(rr.outcome, Outcome::Pass);
        assert_eq!(verdict_for(&[rr]), "ELEVATED");
        assert_eq!(rugcheck_rule(&parse_rugcheck(&json!({"rugged":true})), M).code, "RUGCHECK_RUGGED");
    }

    #[test]
    fn metadata_missing_blocks() {
        assert_eq!(metadata_present_rule("A", "B", "").code, "METADATA_MISSING");
        assert_eq!(metadata_present_rule("", "B", "u").code, "METADATA_MISSING");
        assert_eq!(metadata_present_rule("A", "B", "u").outcome, Outcome::Pass);
    }

    #[test]
    fn metadata_uri_outcomes() {
        let ok = MetaFetch::Ok { image: Some("i".into()), social_links: 1 };
        assert_eq!(metadata_uri_rule(&ok, false).outcome, Outcome::Pass);
        assert_eq!(metadata_uri_rule(&MetaFetch::Failed("404".into()), false).code, "METADATA_UNRESOLVED");
        // egress blocked: undecided (blocks, it is required) unless the sandbox relax flag is on
        let u = metadata_uri_rule(&MetaFetch::EgressBlocked, false);
        assert!(u.blocks());
        assert!(!metadata_uri_rule(&MetaFetch::EgressBlocked, true).blocks());
    }

    #[test]
    fn metadata_json_and_community() {
        let v = json!({"name":"x","image":"https://i","twitter":"https://x.com/a","telegram":"","website":""});
        let f = parse_metadata_json(&v);
        assert_eq!(f, MetaFetch::Ok { image: Some("https://i".into()), social_links: 1 });
        assert_eq!(community_rule(&f, &RugImage::Unknown, 1, true, false).outcome, Outcome::Pass);
        let none = parse_metadata_json(&json!({"image":"https://i"}));
        assert_eq!(community_rule(&none, &RugImage::Unknown, 1, true, false).code, "NO_SOCIALS");
        let noimg = parse_metadata_json(&json!({"twitter":"t"}));
        assert_eq!(community_rule(&noimg, &RugImage::Unknown, 1, true, false).code, "NO_IMAGE");
        // RugCheck can supply the image when the metadata JSON has none
        assert_eq!(community_rule(&noimg, &RugImage::Present, 1, true, false).outcome, Outcome::Pass);
        // sandbox (relaxed, egress blocked): RugCheck's empty image is ambiguous -> flagged pass, not a block
        assert!(!community_rule(&MetaFetch::EgressBlocked, &RugImage::Absent, 1, true, true).blocks());
        // metadata file failed for a real reason and RugCheck says no image -> block
        assert_eq!(community_rule(&MetaFetch::Failed("404".into()), &RugImage::Absent, 1, true, false).code, "NO_IMAGE");
        assert!(!community_rule(&MetaFetch::EgressBlocked, &RugImage::Present, 1, true, true).blocks());
        assert!(!community_rule(&MetaFetch::EgressBlocked, &RugImage::Unknown, 1, true, true).blocks());
        // not relaxed (the user's PC): unreadable metadata is undecided and blocks
        assert!(community_rule(&MetaFetch::EgressBlocked, &RugImage::Present, 1, true, false).blocks());
    }

    #[test]
    fn blacklist_and_serial_rugger() {
        assert_eq!(blacklist_rule(Some("fell 80%".into()), "c").code, "CREATOR_BLACKLISTED");
        assert_eq!(blacklist_rule(None, "c").outcome, Outcome::Pass);
        assert!(should_blacklist_creator(100.0, 25.0, 200.0)); // fell 75% in 200 s
        assert!(!should_blacklist_creator(100.0, 40.0, 200.0)); // fell 60%
        assert!(!should_blacklist_creator(100.0, 10.0, 400.0)); // too slow to count
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("pepe", "pepe"), 0);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn copycat_blocks_near_names() {
        let trending = vec![("Moon Cat Coin".to_string(), "MCAT".to_string())];
        assert_eq!(copycat_rule("Moon Cat Coln", "XYZQ", &trending).code, "COPYCAT"); // distance 1 on name
        assert_eq!(copycat_rule("Totally Different", "MCAT", &trending).code, "COPYCAT"); // same symbol
        assert_eq!(copycat_rule("Totally Different", "ZZZZ", &trending).outcome, Outcome::Pass);
        // short symbols must match exactly, not within 2
        let t2 = vec![("Foo".to_string(), "AB".to_string())];
        assert_eq!(copycat_rule("Bar", "AC", &t2).outcome, Outcome::Pass);
        assert_eq!(copycat_rule("Bar", "ab", &t2).code, "COPYCAT");
    }

    fn txs(spec: &[(u64, &str)]) -> Vec<SlotTx> {
        spec.iter().map(|(s, p)| SlotTx { slot: *s, payer: p.to_string(), recv_ms: 1000 }).collect()
    }

    #[test]
    fn bundle_burst_in_one_block() {
        let t: Vec<SlotTx> = (0..16).map(|i| SlotTx { slot: 77, payer: format!("w{i}"), recv_ms: 1000 }).collect();
        let hit = bundle_rules(&t, &[], &HashSet::new(), 5000).unwrap();
        assert_eq!(hit.code, "BUNDLE_BURST");
        assert_eq!(hit.tag, "ACTIVE");
        let hit2 = bundle_rules(&t, &[], &HashSet::new(), 60_000).unwrap();
        assert_eq!(hit2.tag, "HISTORICAL");
        assert_eq!(bundle_check(Some(hit), true, &[]).code, "BUNDLE_BURST");
        assert_eq!(bundle_check(bundle_rules(&t[..14], &[], &HashSet::new(), 5000), true, &[]).outcome, Outcome::Pass);
    }

    #[test]
    fn bundle_few_payers_many_txs() {
        let spec: Vec<(u64, String)> = (0..20).map(|i| (100 + i as u64, format!("p{}", i % 4))).collect();
        let t: Vec<SlotTx> = spec.iter().map(|(s, p)| SlotTx { slot: *s, payer: p.clone(), recv_ms: 0 }).collect();
        assert_eq!(bundle_rules(&t, &[], &HashSet::new(), 100).unwrap().code, "BUNDLE_PAYERS");
        // 20 txs from 6 payers is not a bundle
        let t6: Vec<SlotTx> = (0..20).map(|i| SlotTx { slot: i as u64, payer: format!("p{}", i % 6), recv_ms: 0 }).collect();
        assert!(bundle_rules(&t6, &[], &HashSet::new(), 100).is_none());
        let _ = txs(&[]);
    }

    #[test]
    fn bundle_supply_concentration() {
        let h = hs(&[("curve", 15.0, false), ("a", 40.0, false), ("b", 30.0, false), ("c", 10.0, false)]);
        let ex: HashSet<String> = ["curve".to_string()].into();
        assert_eq!(bundle_rules(&[], &h, &ex, 0).unwrap().code, "BUNDLE_SUPPLY");
        let h2 = hs(&[("curve", 90.0, false), ("a", 5.0, false), ("b", 5.0, false)]);
        assert!(bundle_rules(&[], &h2, &ex, 0).is_none());
    }

    #[test]
    fn dev_snipe_in_first_block() {
        let sigs = vec![
            ("create".to_string(), 500, false),
            ("snipe1".to_string(), 500, false),
            ("late".to_string(), 502, false),
        ];
        let r = dev_snipe_rule(&sigs, "create", true);
        assert_eq!(r.code, "DEV_SNIPE");
        assert_eq!(r.evidence.len(), 1);
        // a failed tx in the same slot is not a snipe
        let sigs2 = vec![("create".to_string(), 500, false), ("failed".to_string(), 500, true)];
        assert_eq!(dev_snipe_rule(&sigs2, "create", true).outcome, Outcome::Pass);
        // create tx missing from the list: undecided (required, so it blocks)
        assert!(dev_snipe_rule(&[("x".to_string(), 1, false)], "create", true).blocks());
        // switch off
        assert_eq!(dev_snipe_rule(&sigs, "create", false).outcome, Outcome::Pass);
    }

    fn trade(user: &str, buy: bool, ms: i64) -> Trade {
        Trade { mint: "m".into(), user: user.into(), is_buy: buy, sol: 0.1, tokens: 1.0, slot: 1, signature: "s".into(), recv_ms: ms, v_sol: 30.0, v_tokens: 1e9, real_sol: 0.0 }
    }

    #[test]
    fn wash_volume_needs_15_buyers() {
        let few: Vec<Trade> = (0..10).map(|i| trade(&format!("w{i}"), true, 1000)).collect();
        assert_eq!(wash_volume_rule(&few, 2000).code, "WASH_VOLUME");
        let many: Vec<Trade> = (0..20).map(|i| trade(&format!("w{i}"), true, 1000)).collect();
        assert_eq!(wash_volume_rule(&many, 2000).outcome, Outcome::Pass);
        // old trades do not count
        assert_eq!(wash_volume_rule(&many, 100_000).code, "WASH_VOLUME");
    }

    #[test]
    fn wash_volume_top3_share() {
        let mut t: Vec<Trade> = (0..15).map(|i| trade(&format!("w{i}"), true, 1000)).collect();
        for _ in 0..30 {
            t.push(trade("w0", true, 1000));
            t.push(trade("w1", false, 1000));
        }
        assert_eq!(wash_volume_rule(&t, 2000).code, "WASH_VOLUME");
    }

    #[test]
    fn chase_over_3x_blocks() {
        assert_eq!(chase_rule(30.0, 100.0, 50.0).code, "CHASE_RISK");
        assert_eq!(chase_rule(30.0, 80.0, 50.0).outcome, Outcome::Pass);
        assert_eq!(chase_rule(30.0, 100.0, 120.0).outcome, Outcome::Pass); // beyond the 60 s window
    }

    #[test]
    fn required_flag_semantics() {
        let u = CheckResult::undecided("mint_account", "timeout");
        assert!(u.blocks());
        assert_eq!(u.code, "UNDECIDED_MINT_ACCOUNT");
        assert!(!u.clone().with_required(false).blocks());
    }

    #[test]
    fn verdicts() {
        assert_eq!(verdict_for(&[CheckResult::pass("a", "")]), "LOW_SIGNAL");
        assert_eq!(verdict_for(&[CheckResult::pass("a", ""), honeypot_stub()]), "LOW_SIGNAL");
        assert_eq!(verdict_for(&[CheckResult::pass("a", ""), CheckResult::not_available("metadata_uri", "x")]), "ELEVATED");
        assert_eq!(verdict_for(&[CheckResult::block("a", "MINT_AUTHORITY", "")]), "HIGH");
        assert_eq!(verdict_for(&[CheckResult::block("a", "COPYCAT", "")]), "ELEVATED");
    }
}
