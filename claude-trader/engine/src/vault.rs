//! Obsidian vault connector: the bot keeps a human-readable memory as plain markdown files.
//! Local files only. No Obsidian plugin, no API. Everything written here is built from rows that are really in
//! SQLite (closed trades, Scam Shield decisions, the learning journal, reviews, changes, notes). Nothing is invented.
//!
//! Layout inside `<vault>/Claude Trader/`: Trades/, Coins/, Days/, Lessons/, Situations/, Index.md.
//! Safety: files are written to a temp name and renamed, nothing is ever deleted, a note this app did not write
//! (no `generated-by: claude-trader` line) is never overwritten, and every file name is sanitised.

use crate::curve::FeeBreakdown;
use crate::db::{Db, DecisionRow};
use crate::learning::{self, EntryFeatures, JournalRow};
use crate::position::Position;
use crate::report::{self, TradeLine};
use crate::timefmt::{date_local, iso_local};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FOLDER: &str = "Claude Trader";
pub const MARKER: &str = "generated-by: claude-trader";
const SETTINGS_KEY: &str = "vault.settings";
const ERROR_KEY: &str = "vault.last_error";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultSettings {
    /// The vault folder chosen in Settings. Empty means disconnected: nothing is written.
    pub path: String,
    pub trades: bool,
    pub coins: bool,
    pub days: bool,
    pub lessons: bool,
    pub situations: bool,
}

impl Default for VaultSettings {
    fn default() -> Self {
        VaultSettings { path: String::new(), trades: true, coins: true, days: true, lessons: true, situations: true }
    }
}

impl VaultSettings {
    pub fn load(db: &Db) -> VaultSettings {
        db.kv_get(SETTINGS_KEY).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
    }
    pub fn save(&self, db: &Db) {
        db.kv_set(SETTINGS_KEY, &serde_json::to_string(self).unwrap_or_default());
    }
    pub fn connected(&self) -> bool {
        !self.path.trim().is_empty()
    }
}

/// Everything the Settings card shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub settings: VaultSettings,
    pub folder_exists: bool,
    pub looks_like_obsidian: bool,
    pub notes_folder: String,
    pub last_error: String,
}

pub fn status(db: &Db) -> VaultStatus {
    let s = VaultSettings::load(db);
    let root = PathBuf::from(&s.path);
    let ok = s.connected() && root.is_dir();
    VaultStatus {
        folder_exists: ok,
        looks_like_obsidian: ok && root.join(".obsidian").is_dir(),
        notes_folder: if s.connected() { root.join(FOLDER).to_string_lossy().to_string() } else { String::new() },
        last_error: db.kv_get(ERROR_KEY).unwrap_or_default(),
        settings: s,
    }
}

pub fn connect(db: &Db, path: &str) -> Result<VaultStatus, String> {
    let p = Path::new(path.trim());
    if !p.is_dir() {
        return Err("That folder does not exist.".into());
    }
    if p.parent().is_none() {
        return Err("Pick your vault folder, not a whole drive.".into());
    }
    let mut s = VaultSettings::load(db);
    s.path = p.to_string_lossy().to_string();
    s.save(db);
    db.kv_del(ERROR_KEY);
    Ok(status(db))
}

/// Stops all writing. The notes already in the vault stay (they are yours).
pub fn disconnect(db: &Db) -> VaultStatus {
    let mut s = VaultSettings::load(db);
    s.path.clear();
    s.save(db);
    db.kv_del(ERROR_KEY);
    status(db)
}

pub fn set_toggles(db: &Db, t: &VaultSettings) -> VaultStatus {
    let mut s = VaultSettings::load(db);
    (s.trades, s.coins, s.days, s.lessons, s.situations) = (t.trades, t.coins, t.days, t.lessons, t.situations);
    s.save(db);
    status(db)
}

// ------------------------------------------------------------------ file names and atomic writes

/// A safe file or folder name: no path separators, no characters Windows or macOS refuse, no control characters,
/// no trailing dots or spaces, no Windows device names, at most 90 characters, never empty.
pub fn sanitize_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']') { '-' } else { c })
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = out.chars().take(90).collect();
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    while out.starts_with('.') || out.starts_with(' ') {
        out.remove(0);
    }
    let up = out.to_ascii_uppercase();
    let stem = up.split('.').next().unwrap_or("");
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if out.is_empty() {
        return "untitled".into();
    }
    if reserved.contains(&stem) {
        out.insert(0, '_');
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrote {
    New,
    Updated,
    Unchanged,
    /// A file with that name exists that this app did not write. It is left alone.
    KeptYours,
}

/// Write `<vault>/Claude Trader/<rel>` where `rel` is `Folder/File.md` (each part sanitised again here).
pub fn write_note(vault: &Path, rel: &str, content: &str) -> Result<Wrote, String> {
    let mut dir = vault.join(FOLDER);
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let Some((file, folders)) = parts.split_last() else { return Err("empty note name".into()) };
    for f in folders {
        dir.push(sanitize_name(f));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let target = dir.join(sanitize_name(file));
    let existed = target.exists();
    if existed {
        let old = std::fs::read_to_string(&target).unwrap_or_default();
        if !old.contains(MARKER) {
            return Ok(Wrote::KeptYours);
        }
        if old == content {
            return Ok(Wrote::Unchanged);
        }
    }
    let tmp = dir.join(format!(".{}.ct-tmp", sanitize_name(file)));
    std::fs::write(&tmp, content).map_err(|e| format!("Could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp); // only our own temp file
        format!("Could not save {}: {e}", target.display())
    })?;
    Ok(if existed { Wrote::Updated } else { Wrote::New })
}

// ------------------------------------------------------------------ rendering helpers (pure)

fn yq(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn tag(s: &str) -> String {
    s.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' { c } else { '-' }).collect()
}

fn sol(x: f64) -> String {
    format!("{x:.6}")
}

fn price(x: f64) -> String {
    format!("{x:.4e}")
}

fn cell(s: &str) -> String {
    s.replace('|', "/").replace('\n', " ")
}

pub fn day_of(ms: i64) -> String {
    date_local(ms)
}

fn hms(secs: i64) -> String {
    if secs >= 3600 { format!("{}h {}m {}s", secs / 3600, secs % 3600 / 60, secs % 60) } else if secs >= 60 { format!("{}m {}s", secs / 60, secs % 60) } else { format!("{secs}s") }
}

fn solscan(mint: &str) -> String {
    crate::pump::solscan_token(mint)
}

pub fn trade_note_name(p: &Position) -> String {
    let day = day_of(p.exit_ms.unwrap_or(p.entry_ms));
    sanitize_name(&format!("{day} {} {}.md", p.symbol, p.id))
}

fn coin_link(mint: &str) -> String {
    format!("[[Coins/{}]]", sanitize_name(mint))
}

fn fee_rows(f: &FeeBreakdown) -> String {
    format!(
        "| pump.fun fee | {} |\n| creator fee | {} |\n| PumpPortal fee | {} |\n| network (base) fee | {} |\n| priority fee | {} |\n| Jito tip | {} |\n| **total** | **{}** |\n",
        sol(f.pump_fun), sol(f.creator), sol(f.pump_portal), sol(f.tx), sol(f.priority), sol(f.jito), sol(f.total())
    )
}

/// One closed trade. `features` and `journal` are None when the app has no row for them (then the section says so).
pub fn render_trade(p: &Position, features: Option<&EntryFeatures>, journal: Option<&JournalRow>, decisions: &[DecisionRow]) -> String {
    let exit_ms = p.exit_ms.unwrap_or(p.entry_ms);
    let day = day_of(exit_ms);
    let win = p.pnl_sol > 0.0;
    let result = if win { "win" } else { "loss" };
    let hold = ((exit_ms - p.entry_ms) as f64 / 1000.0).round() as i64;
    let line = TradeLine::from_position(p);
    let tags = [result.to_string(), format!("exit/{}", tag(&p.exit_reason)), format!("lane/{}", tag(&p.lane)), "trade".into()];
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("{MARKER}\ntype: trade\nid: {}\nsymbol: {}\nmint: {}\nlane: {}\nentry: {}\nexit: {}\nsize_sol: {}\npnl_sol: {}\npnl_pct: {}\nhold_seconds: {hold}\nexit_reason: {}\nresult: {result}\nmode: paper\n", p.id, yq(&p.symbol), yq(&p.mint), yq(&p.lane), yq(&iso_local(p.entry_ms)), yq(&iso_local(exit_ms)), sol(p.size_sol), sol(p.pnl_sol), line.pnl_pct, yq(&p.exit_reason)));
    s.push_str(&format!("tags: [{}]\n---\n\n", tags.join(", ")));
    s.push_str(&format!("# {} trade {} ({})\n\n", p.symbol, p.id, p.lane));
    s.push_str(&format!("{} {}  \nCoin: {} | Day: [[Days/{day}]]\n\n", tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "), "#paper", coin_link(&p.mint)));
    s.push_str("## Result\n\n| | |\n|---|---|\n");
    s.push_str(&format!("| Net P&L | {} SOL ({:+.2}%) |\n| Held | {} |\n| Exit reason | {} |\n| Opened | {} |\n| Closed | {} |\n", sol(p.pnl_sol), line.pnl_pct, hms(hold), cell(&p.exit_reason), iso_local(p.entry_ms), iso_local(exit_ms)));
    s.push_str(&format!("| Size | {} SOL |\n| Entry price | {} SOL per token |\n| Average exit price | {} SOL per token |\n| Highest price seen | {} SOL per token |\n| Entry market cap | ${} |\n| Buy slippage | {:.2}% |\n\n", sol(p.size_sol), price(p.entry_price), price(p.avg_exit_price()), price(p.peak_price), p.entry_mcap_usd.round(), p.slippage_pct));
    s.push_str("## Fees (SOL)\n\n| Line | SOL |\n|---|---|\n");
    s.push_str(&fee_rows(&p.fees));
    if p.failed_sol > 0.0 {
        s.push_str(&format!("\nFees of attempts that were paid but did not trade: {} SOL.\n", sol(p.failed_sol)));
    }
    s.push_str(&format!("\nGross sold: {} SOL. Net received after fees: {} SOL.\n\n", sol(p.gross_exit_sol), sol(p.realized_sol)));
    if !p.stages.is_empty() {
        s.push_str("## Sells\n\n| Time | Why | Tokens | Price | Gross SOL | Net SOL | Fees SOL |\n|---|---|---|---|---|---|---|\n");
        for st in &p.stages {
            s.push_str(&format!("| {} | {} | {:.2} | {} | {} | {} | {} |\n", iso_local(st.ts), cell(&st.reason), st.tokens, price(st.price), sol(st.gross_sol), sol(st.net_sol), sol(st.fees_sol)));
        }
        s.push('\n');
    }
    s.push_str("## Why it was bought\n\n");
    match features {
        Some(f) => {
            s.push_str(&format!("Picker score {:.1}. {} buyers, {} buys and {} sells in the watch window (buy/sell ratio {:.2}). Curve held {:.2} SOL. Coin age {:.0} s. Market cap {:.1} SOL.\n", f.score, f.buyers, f.buys, f.sells, f.ratio, f.curve_sol, f.age_seconds, f.mcap_sol));
            if f.source_url.starts_with("http") {
                s.push_str(&format!("\nSource: <{}>\n", f.source_url));
            }
            if let Some(x) = &f.x_community {
                s.push_str("\n## X community\n\n");
                s.push_str(&format!("{}\n\n", x.summary()));
                let c = &x.components;
                s.push_str("| Part | Points |\n|---|---|\n");
                for (n, v) in [("Smart (KOL) followers", c.kol), ("Engagement quality", c.engagement), ("Reply authenticity", c.replies), ("Follower growth", c.growth), ("Account and handle integrity", c.integrity), ("X Community presence", c.community), ("Narrative strength", c.narrative)] {
                    s.push_str(&format!("| {n} | {v} |\n"));
                }
                s.push_str(&format!("| Score (0-100) | {:.1} |\n| Points added to the token score | {} |\n| Status | {} |\n", x.score, x.points, cell(&x.status)));
                for fl in &x.flags {
                    s.push_str(&format!("\n- Flag: {}", cell(fl)));
                }
                for b in &x.blocks {
                    s.push_str(&format!("\n- Block: {}", cell(b)));
                }
                if x.url.starts_with("https://x.com/") {
                    s.push_str(&format!("\n\nSource: <{}>", x.url));
                }
                s.push('\n');
            }
        }
        None => s.push_str("The app did not save the entry numbers for this trade.\n"),
    }
    s.push_str("\n## Scam Shield\n\n");
    if decisions.is_empty() {
        s.push_str("No Scam Shield rows are stored for this coin.\n");
    } else {
        s.push_str("| Time | Stage | Outcome | Reason | Detail | Verdict |\n|---|---|---|---|---|---|\n");
        for d in decisions.iter().filter(|d| d.ts <= exit_ms) {
            s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", iso_local(d.ts), cell(&d.stage), cell(&d.outcome), cell(&d.reason), cell(&d.detail), cell(&d.verdict)));
        }
    }
    s.push_str(&format!("\nEvidence: [Token on Solscan]({})\n\n", solscan(&p.mint)));
    s.push_str("## What the learning journal noted\n\n");
    match journal {
        Some(j) => {
            s.push_str(&format!("{}\n\n", j.headline));
            for o in &j.observations {
                s.push_str(&format!("- {o}\n"));
            }
        }
        None => s.push_str("No journal entry for this trade (the shadow test lane is not journaled).\n"),
    }
    s.push_str(&format!("\n---\nLinks: {} | [[Days/{day}]] | [[Index]]\n", coin_link(&p.mint)));
    s
}

/// A coin the bot traded or noted. History is the real decision rows plus the trades.
pub fn render_coin(mint: &str, symbol: &str, name: &str, creator: &str, seen_ms: i64, decisions: &[DecisionRow], trades: &[Position], notes: &[(i64, String, String, String, String)]) -> String {
    let mut s = String::new();
    s.push_str(&format!("---\n{MARKER}\ntype: coin\nmint: {}\nsymbol: {}\n", yq(mint), yq(symbol)));
    if !name.is_empty() {
        s.push_str(&format!("name: {}\n", yq(name)));
    }
    if !creator.is_empty() {
        s.push_str(&format!("creator: {}\n", yq(creator)));
    }
    s.push_str("tags: [coin]\n---\n\n");
    s.push_str(&format!("# {} ({})\n\n", if symbol.is_empty() { mint } else { symbol }, mint));
    if seen_ms > 0 {
        s.push_str(&format!("First seen by the bot: {}\n\n", iso_local(seen_ms)));
    }
    s.push_str(&format!("[Token on Solscan]({})\n\n", solscan(mint)));
    s.push_str("## Verdicts history (Scam Shield)\n\n");
    if decisions.is_empty() {
        s.push_str("No Scam Shield rows are stored for this coin.\n\n");
    } else {
        s.push_str("| Time | Stage | Outcome | Reason | Detail | Verdict |\n|---|---|---|---|---|---|\n");
        for d in decisions {
            s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", iso_local(d.ts), cell(&d.stage), cell(&d.outcome), cell(&d.reason), cell(&d.detail), cell(&d.verdict)));
        }
        s.push('\n');
    }
    s.push_str("## Trades\n\n");
    if trades.is_empty() {
        s.push_str("The bot did not trade this coin.\n\n");
    }
    for p in trades {
        s.push_str(&format!("- [[Trades/{}]]: {} SOL ({}), {}\n", sanitize_name(&trade_note_name(p)).trim_end_matches(".md"), sol(p.pnl_sol), cell(&p.exit_reason), iso_local(p.exit_ms.unwrap_or(p.entry_ms))));
    }
    if !notes.is_empty() {
        s.push_str("\n## Learning notes\n\n");
        for (ts, kind, text, src, _) in notes {
            s.push_str(&format!("- {} ({}): {}{}\n", iso_local(*ts), kind, text, if src.starts_with("http") { format!(" <{src}>") } else { String::new() }));
        }
    }
    s.push_str("\n[[Index]]\n");
    s
}

pub struct DayData {
    pub date: String,
    pub trades: Vec<Position>,
    pub seen: u32,
    pub blocked: u32,
    pub scored: u32,
    pub picked: u32,
    pub bought: u32,
}

pub fn render_day(d: &DayData) -> String {
    let lines: Vec<TradeLine> = d.trades.iter().map(TradeLine::from_position).collect();
    let sm = report::summarize(&d.date, &lines, 1.0);
    let mut s = String::new();
    s.push_str(&format!("---\n{MARKER}\ntype: day\ndate: {}\ntrades: {}\nwins: {}\nlosses: {}\npnl_sol: {}\ntags: [day]\n---\n\n", d.date, sm.trades, sm.wins, sm.losses, sol(sm.total_pnl_sol)));
    s.push_str(&format!("# {}\n\n", d.date));
    s.push_str("## Summary\n\n| | |\n|---|---|\n");
    s.push_str(&format!("| Trades | {} ({} wins, {} losses, win rate {:.0}%) |\n| Net P&L | {} SOL |\n| Fees | {} SOL |\n| Best / worst trade | {} / {} SOL |\n| Max drawdown | {} SOL |\n", sm.trades, sm.wins, sm.losses, sm.win_rate_pct, sol(sm.total_pnl_sol), sol(sm.fees_sol), sol(sm.best_sol), sol(sm.worst_sol), sol(sm.max_drawdown_sol)));
    if let Some(pf) = sm.profit_factor {
        s.push_str(&format!("| Profit factor | {pf:.2} |\n"));
    }
    s.push_str("\n## Funnel (counts from the database)\n\n| Step | Coins |\n|---|---|\n");
    s.push_str(&format!("| Seen | {} |\n| Blocked by Scam Shield | {} |\n| Scored after the 60 s watch | {} |\n| Picked | {} |\n| Bought | {} |\n\n", d.seen, d.blocked, d.scored, d.picked, d.bought));
    s.push_str("## Trades\n\n");
    if d.trades.is_empty() {
        s.push_str("No trades closed this day.\n");
    }
    for p in &d.trades {
        s.push_str(&format!("- [[Trades/{}]]: {} {} SOL ({})\n", trade_note_name(p).trim_end_matches(".md"), p.symbol, sol(p.pnl_sol), cell(&p.exit_reason)));
    }
    s.push_str("\n[[Index]]\n");
    s
}

pub fn render_review(id: i64, ts: i64, trades: u32, r: &learning::ReviewResult) -> String {
    let mut s = format!("---\n{MARKER}\ntype: review\nid: {id}\nts: {}\ntrades_reviewed: {trades}\ntags: [lesson, review]\n---\n\n# Learning review {id} ({trades} trades)\n\n", yq(&iso_local(ts)));
    s.push_str("## Findings\n\n");
    if r.findings.is_empty() {
        s.push_str("No pattern was strong enough to report.\n");
    }
    for f in &r.findings {
        s.push_str(&format!("- **{} / {} ({})**: {}\n  - Evidence: {} trades here, win rate {:.0}%, average {} SOL. The rest: {} trades, win rate {:.0}%, average {} SOL.\n", cell(&f.feature), cell(&f.bucket), f.direction, f.text, f.n, f.win_rate_pct, sol(f.avg_pnl_sol), f.rest_n, f.rest_win_rate_pct, sol(f.rest_avg_pnl_sol)));
    }
    if !r.exit_lines.is_empty() {
        s.push_str("\n## Exits\n\n");
        for l in &r.exit_lines {
            s.push_str(&format!("- {l}\n"));
        }
    }
    s.push_str("\n## Proposal\n\n");
    match &r.proposal {
        Some(p) => {
            s.push_str(&format!("{} ({}): {} to {}. {}\n", p.label, p.key, learning::trim(p.before), learning::trim(p.after), p.why));
            for e in &p.evidence {
                s.push_str(&format!("- {e}\n"));
            }
        }
        None => s.push_str("No change was proposed.\n"),
    }
    if !r.notes.is_empty() {
        s.push_str("\n## Notes\n\n");
        for n in &r.notes {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s.push_str("\n[[Index]]\n");
    s
}

pub fn render_change(c: &learning::ChangeRow) -> String {
    let mut s = format!("---\n{MARKER}\ntype: change\nid: {}\nkey: {}\nstatus: {}\nbefore: {}\nafter: {}\ntags: [lesson, change, {}]\n---\n\n", c.id, yq(&c.key), yq(&c.status), learning::trim(c.before), learning::trim(c.after), tag(&c.status));
    s.push_str(&format!("# {} ({})\n\n#{}\n\n", c.label, c.status, tag(&c.status)));
    s.push_str(&format!("Proposed {}: {} to {}.\n\nWhy: {}\n\n", iso_local(c.ts), learning::trim(c.before), learning::trim(c.after), c.why));
    if !c.evidence.is_empty() {
        s.push_str("Evidence:\n\n");
        for e in &c.evidence {
            s.push_str(&format!("- {e}\n"));
        }
    }
    if !c.result.is_empty() {
        s.push_str(&format!("\nResult of the test: {}\n", c.result));
    }
    s.push_str("\n[[Index]]\n");
    s
}

pub fn render_efficiency(notes: &[(i64, String, String, String, String)]) -> String {
    let mut s = format!("---\n{MARKER}\ntype: efficiency\ntags: [lesson, efficiency]\n---\n\n# Efficiency notes\n\n");
    if notes.is_empty() {
        s.push_str("No notes recorded yet.\n");
    }
    for (ts, kind, text, src, mint) in notes.iter().rev() {
        s.push_str(&format!("- {} ({}): {}{}{}\n", iso_local(*ts), kind, text, if mint.is_empty() { String::new() } else { format!(" {}", coin_link(mint)) }, if src.starts_with("http") { format!(" <{src}>") } else { String::new() }));
    }
    s.push_str("\n[[Index]]\n");
    s
}

pub fn render_situation(ts: i64, kind: &str, title: &str, text: &str, mint: &str) -> String {
    let mut s = format!("---\n{MARKER}\ntype: situation\nkind: {}\nts: {}\ntags: [situation, {}]\n---\n\n# {}\n\n#situation #{}\n\n{}\n\nWhen: {}\n", yq(kind), yq(&iso_local(ts)), tag(kind), title, tag(kind), text, iso_local(ts));
    if !mint.is_empty() {
        s.push_str(&format!("\nCoin: {}\n", coin_link(mint)));
    }
    s.push_str(&format!("Day: [[Days/{}]] | [[Index]]\n", day_of(ts)));
    s
}

fn list_names(dir: &Path, newest: usize) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".md") && !n.starts_with('.')).collect())
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v.truncate(newest);
    v.into_iter().map(|n| n.trim_end_matches(".md").to_string()).collect()
}

pub fn render_index(vault: &Path, trade_count: usize, now_ms: i64) -> String {
    let root = vault.join(FOLDER);
    let mut s = format!("---\n{MARKER}\ntype: index\ntags: [index]\n---\n\n# Claude Trader memory\n\nWritten by the bot from its own database. Paper trading only. Updated {}.\n\n{} closed trades are in the database.\n\n", iso_local(now_ms), trade_count);
    for (title, folder, n) in [("Recent trades", "Trades", 15), ("Days", "Days", 14), ("Lessons", "Lessons", 15), ("Situations", "Situations", 15)] {
        s.push_str(&format!("## {title}\n\n"));
        let names = list_names(&root.join(folder), n);
        if names.is_empty() {
            s.push_str("Nothing yet.\n");
        }
        for nm in names {
            s.push_str(&format!("- [[{folder}/{nm}]]\n"));
        }
        s.push('\n');
    }
    s
}

// ------------------------------------------------------------------ writing from the database

fn root_if_on(db: &Db) -> Option<(PathBuf, VaultSettings)> {
    let s = VaultSettings::load(db);
    if !s.connected() {
        return None;
    }
    let root = PathBuf::from(&s.path);
    if !root.is_dir() {
        db.kv_set(ERROR_KEY, "The vault folder is not there any more. Choose it again in Settings > Connectors.");
        return None;
    }
    Some((root, s))
}

fn put(db: &Db, root: &Path, rel: &str, content: &str) -> Option<Wrote> {
    match write_note(root, rel, content) {
        Ok(w) => Some(w),
        Err(e) => {
            db.kv_set(ERROR_KEY, &e);
            None
        }
    }
}

fn write_trade(db: &Db, root: &Path, st: &VaultSettings, p: &Position) {
    if p.kind != "trade" || p.state != crate::position::PosState::Closed {
        return;
    }
    let decisions = db.decisions_for_mint(&p.mint);
    if st.trades {
        let feats = learning::features_for(db, p.id);
        let j = learning::journal_row_for(db, p.id);
        put(db, root, &format!("Trades/{}", trade_note_name(p)), &render_trade(p, feats.as_ref(), j.as_ref(), &decisions));
    }
    if st.coins {
        write_coin(db, root, &p.mint, &p.symbol, &decisions);
    }
    if st.days {
        write_day(db, root, &day_of(p.exit_ms.unwrap_or(p.entry_ms)));
    }
}

fn write_coin(db: &Db, root: &Path, mint: &str, symbol: &str, decisions: &[DecisionRow]) {
    let trades: Vec<Position> = db.closed_trades_all().into_iter().filter(|t| t.mint == mint).collect();
    let notes: Vec<_> = learning::notes_all(db).into_iter().filter(|n| n.4 == mint).collect();
    let (name, sym2, creator, seen) = db.token_name(mint).unwrap_or_default();
    let sym = if symbol.is_empty() { sym2 } else { symbol.to_string() };
    put(db, root, &format!("Coins/{}.md", sanitize_name(mint)), &render_coin(mint, &sym, &name, &creator, seen, decisions, &trades, &notes));
}

fn day_window(date: &str, any_ms_in_day: i64) -> (i64, i64) {
    let _ = date;
    let off = crate::timefmt::offset_ms();
    let local = any_ms_in_day + off;
    let start = local.div_euclid(86_400_000) * 86_400_000 - off;
    (start, start + 86_400_000)
}

fn write_day(db: &Db, root: &Path, date: &str) {
    let trades: Vec<Position> = db.closed_trades_all().into_iter().filter(|p| day_of(p.exit_ms.unwrap_or(p.entry_ms)) == date).collect();
    let Some(first) = trades.first() else { return };
    let (from, to) = day_window(date, first.exit_ms.unwrap_or(first.entry_ms));
    let (seen, blocked, scored, picked, bought) = db.day_counts(from, to);
    put(db, root, &format!("Days/{date}.md"), &render_day(&DayData { date: date.into(), trades, seen, blocked, scored, picked, bought }));
}

fn write_lessons(db: &Db, root: &Path) {
    for (id, ts, n, r) in learning::reviews_all(db) {
        put(db, root, &format!("Lessons/{} review {id} ({n} trades).md", day_of(ts)), &render_review(id, ts, n, &r));
    }
    for c in learning::changes_all(db) {
        put(db, root, &format!("Lessons/{} change {} {}.md", day_of(c.ts), c.id, c.key), &render_change(&c));
    }
    put(db, root, "Lessons/Efficiency notes.md", &render_efficiency(&learning::notes_all(db)));
}

fn write_index(db: &Db, root: &Path, now_ms: i64) {
    put(db, root, "Index.md", &render_index(root, db.closed_trades_all().len(), now_ms));
}

/// Called when a trade closes (after the learning journal row was written). Does nothing when disconnected.
pub fn on_trade_closed(db: &Db, p: &Position) {
    let Some((root, st)) = root_if_on(db) else { return };
    write_trade(db, &root, &st, p);
    if st.lessons {
        write_lessons(db, &root);
    }
    write_index(db, &root, p.exit_ms.unwrap_or(p.entry_ms));
}

/// After a learning change was approved or undone.
pub fn on_lessons_changed(db: &Db, now_ms: i64) {
    let Some((root, st)) = root_if_on(db) else { return };
    if st.lessons {
        write_lessons(db, &root);
        write_index(db, &root, now_ms);
    }
}

/// A notable event: kill switch, loss limit, feed warning, RugCheck outage, a missed coin. `key` makes the file name.
pub fn situation(db: &Db, ts: i64, kind: &str, title: &str, text: &str, mint: &str) {
    let Some((root, st)) = root_if_on(db) else { return };
    if !st.situations {
        return;
    }
    let stamp = iso_local(ts).replace('T', " ").chars().take(16).collect::<String>().replace(':', "-");
    let rel = format!("Situations/{stamp} {}.md", kind);
    put(db, &root, &rel, &render_situation(ts, kind, title, text, mint));
    if let Some(m) = Some(mint).filter(|m| !m.is_empty() && st.coins) {
        write_coin(db, &root, m, "", &db.decisions_for_mint(m));
    }
    write_index(db, &root, ts);
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackfillResult {
    pub trades: u32,
    pub days: u32,
    pub coins: u32,
    pub lessons: u32,
    pub situations: u32,
    pub kept_yours: u32,
}

/// Write notes for every past closed trade, day, coin, review, change and missed-coin note from SQLite.
pub fn backfill(db: &Db) -> Result<BackfillResult, String> {
    let (root, st) = root_if_on(db).ok_or("Choose your vault folder first.")?;
    let mut r = BackfillResult::default();
    let trades = db.closed_trades_all();
    let mut days = std::collections::BTreeSet::new();
    let mut coins = std::collections::BTreeSet::new();
    let count = |w: Option<Wrote>, kept: &mut u32| -> bool {
        if w == Some(Wrote::KeptYours) {
            *kept += 1;
        }
        matches!(w, Some(Wrote::New) | Some(Wrote::Updated) | Some(Wrote::Unchanged))
    };
    for p in &trades {
        let decisions = db.decisions_for_mint(&p.mint);
        if st.trades {
            let w = put(db, &root, &format!("Trades/{}", trade_note_name(p)), &render_trade(p, learning::features_for(db, p.id).as_ref(), learning::journal_row_for(db, p.id).as_ref(), &decisions));
            if count(w, &mut r.kept_yours) {
                r.trades += 1;
            }
        }
        days.insert(day_of(p.exit_ms.unwrap_or(p.entry_ms)));
        coins.insert((p.mint.clone(), p.symbol.clone()));
    }
    for (mint, _) in learning::notes_all(db).into_iter().filter(|n| !n.4.is_empty()).map(|n| (n.4, ())) {
        coins.insert((mint, String::new()));
    }
    if st.days {
        for d in &days {
            write_day(db, &root, d);
            r.days += 1;
        }
    }
    if st.coins {
        let mut seen = std::collections::BTreeSet::new();
        for (m, s) in &coins {
            if seen.insert(m.clone()) {
                write_coin(db, &root, m, s, &db.decisions_for_mint(m));
                r.coins += 1;
            }
        }
    }
    if st.lessons {
        write_lessons(db, &root);
        r.lessons = (learning::reviews_all(db).len() + learning::changes_all(db).len() + 1) as u32;
    }
    if st.situations {
        for (ts, kind, text, _src, mint) in learning::notes_all(db) {
            if kind == "missed" {
                situation_raw(db, &root, ts, "missed-coin", "Missed coin", &text, &mint);
                r.situations += 1;
            }
        }
    }
    write_index(db, &root, trades.last().and_then(|p| p.exit_ms).unwrap_or(0));
    Ok(r)
}

fn situation_raw(db: &Db, root: &Path, ts: i64, kind: &str, title: &str, text: &str, mint: &str) {
    let stamp = iso_local(ts).replace('T', " ").chars().take(16).collect::<String>().replace(':', "-");
    let sym = text.split_whitespace().nth(1).unwrap_or("");
    put(db, root, &format!("Situations/{stamp} {kind} {sym}.md"), &render_situation(ts, kind, title, text, mint));
}

/// A missed coin that scored 80 or more, with what the price did next (the text carries the real numbers).
pub fn missed_coin(db: &Db, ts: i64, score: f64, text: &str, mint: &str) {
    if score < learning::MISSED_MIN_SCORE {
        return;
    }
    let Some((root, st)) = root_if_on(db) else { return };
    if st.situations {
        situation_raw(db, &root, ts, "missed-coin", "Missed coin", text, mint);
    }
    if st.lessons {
        put(db, &root, "Lessons/Efficiency notes.md", &render_efficiency(&learning::notes_all(db)));
    }
    write_index(db, &root, ts);
}

// ------------------------------------------------------------------ the Test button

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultTest {
    pub ok: bool,
    pub message: String,
}

/// Writes `Claude Trader/Test note.md`, reads it back and compares.
pub fn test(db: &Db, now_ms: i64) -> VaultTest {
    let s = VaultSettings::load(db);
    if !s.connected() {
        return VaultTest { ok: false, message: "Choose your vault folder first.".into() };
    }
    let root = PathBuf::from(&s.path);
    if !root.is_dir() {
        return VaultTest { ok: false, message: "That folder is not there any more. Choose it again.".into() };
    }
    let body = format!("---\n{MARKER}\ntype: test\n---\n\n# Test note\n\nClaude Trader wrote this at {}. You can delete this note.\n\n[[Index]]\n", iso_local(now_ms));
    match write_note(&root, "Test note.md", &body) {
        Ok(Wrote::KeptYours) => VaultTest { ok: false, message: "A note called Test note.md that Claude Trader did not write is already there. It was left alone.".into() },
        Ok(_) => {
            let back = std::fs::read_to_string(root.join(FOLDER).join("Test note.md")).unwrap_or_default();
            if back == body {
                let hint = if root.join(".obsidian").is_dir() { "Open Obsidian and look for the Claude Trader folder." } else { "This folder has no .obsidian folder yet, so open it once in Obsidian (Open folder as vault) and look for the Claude Trader folder." };
                db.kv_del(ERROR_KEY);
                VaultTest { ok: true, message: format!("OK. Wrote and read back Claude Trader/Test note.md. {hint}") }
            } else {
                VaultTest { ok: false, message: "The note was written but reading it back gave different text.".into() }
            }
        }
        Err(e) => VaultTest { ok: false, message: e },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{PosState, StageSell};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ct-vault-{name}-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const T0: i64 = 1_795_000_000_000;

    fn closed(id: i64, symbol: &str, pnl: f64, reason: &str) -> Position {
        let mut p = Position::new_buying("momentum", "MintAAA111", symbol, "Creator1", "Curve1", 0.1, T0, 150.0);
        p.id = id;
        p.state = PosState::Closed;
        p.entry_price = 2.5e-7;
        p.peak_price = 3.1e-7;
        p.tokens_initial = 400_000.0;
        p.tokens_sold = 400_000.0;
        p.gross_exit_sol = 0.12;
        p.realized_sol = 0.118;
        p.fees.pump_fun = 0.001;
        p.fees.priority = 0.0002;
        p.exit_ms = Some(T0 + 95_000);
        p.exit_reason = reason.into();
        p.pnl_sol = pnl;
        p.pnl_pct = pnl / 0.1 * 100.0;
        p.stages = vec![StageSell { ts: T0 + 95_000, reason: reason.into(), tokens: 400_000.0, price: 3.0e-7, gross_sol: 0.12, net_sol: 0.118, fees_sol: 0.0012 }];
        p
    }

    fn decision(stage: &str, outcome: &str, reason: &str) -> DecisionRow {
        DecisionRow { ts: T0 - 60_000, mint: "MintAAA111".into(), symbol: "CAT".into(), stage: stage.into(), outcome: outcome.into(), reason: reason.into(), detail: "checked".into(), verdict: "ok".into() }
    }

    #[test]
    fn trade_note_is_built_from_the_data_only() {
        let p = closed(7, "CAT", 0.018, "trailing-stop");
        let f = EntryFeatures { score: 91.5, buyers: 12, buys: 30, sells: 5, ratio: 6.0, curve_sol: 9.25, age_seconds: 41.0, mcap_sol: 33.0, picked_ms: T0 - 1000, source_url: "https://x.com/cat".into(), ..Default::default() };
        let n = render_trade(&p, Some(&f), None, &[decision("chain", "pass", "")]);
        assert!(n.starts_with("---\ngenerated-by: claude-trader\n"));
        for must in ["symbol: \"CAT\"", "lane: \"momentum\"", "result: win", "#win", "#exit/trailing-stop", "[[Coins/MintAAA111]]", "[[Days/", "pump.fun fee | 0.001000", "priority fee | 0.000200", "Picker score 91.5", "12 buyers", "https://x.com/cat", "| chain | pass |", "solscan.io/token/MintAAA111", "No journal entry"] {
            assert!(n.contains(must), "missing {must:?} in\n{n}");
        }
        let loss = render_trade(&closed(8, "DOG", -0.02, "hard-stop"), None, None, &[]);
        assert!(loss.contains("#loss") && loss.contains("result: loss") && loss.contains("The app did not save the entry numbers") && loss.contains("No Scam Shield rows"));
        assert!(!loss.contains("Picker score"), "nothing invented when there is no data");
    }

    #[test]
    fn file_names_are_sanitised() {
        assert_eq!(sanitize_name("a/b\\c:d*e?f\"g<h>i|j"), "a-b-c-d-e-f-g-h-i-j");
        let evil = sanitize_name("../../etc/passwd");
        assert!(!evil.contains('/') && !evil.contains('\\') && !evil.starts_with('.'), "{evil}");
        assert_eq!(sanitize_name("CON"), "_CON");
        assert_eq!(sanitize_name("nul.md"), "_nul.md");
        assert_eq!(sanitize_name("  .. "), "untitled");
        assert_eq!(sanitize_name("name. "), "name");
        assert!(sanitize_name(&"x".repeat(300)).len() <= 90);
        assert_eq!(sanitize_name("2026-11-27 $WIF 12.md"), "2026-11-27 $WIF 12.md");
        // a hostile symbol cannot escape the folder
        let p = closed(9, "../../Evil/Sym", 0.01, "max-hold");
        let name = trade_note_name(&p);
        assert!(!name.contains('/') && !name.contains('\\'), "{name}");
        let dir = tmp("names");
        write_note(&dir, &format!("Trades/{name}"), &format!("{MARKER}\n")).unwrap();
        assert!(dir.join(FOLDER).join("Trades").join(&name).is_file());
        assert!(!dir.join("Evil").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_are_atomic_and_never_touch_notes_that_are_not_ours() {
        let dir = tmp("atomic");
        assert_eq!(write_note(&dir, "Days/a.md", &format!("{MARKER}\none")).unwrap(), Wrote::New);
        assert_eq!(write_note(&dir, "Days/a.md", &format!("{MARKER}\none")).unwrap(), Wrote::Unchanged);
        assert_eq!(write_note(&dir, "Days/a.md", &format!("{MARKER}\ntwo")).unwrap(), Wrote::Updated);
        let leftovers: Vec<_> = std::fs::read_dir(dir.join(FOLDER).join("Days")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(leftovers, vec!["a.md".to_string()], "no temp file left behind");
        // a note the user wrote (no marker) is kept as it is
        let mine = dir.join(FOLDER).join("Days").join("mine.md");
        std::fs::write(&mine, "my own words").unwrap();
        assert_eq!(write_note(&dir, "Days/mine.md", &format!("{MARKER}\nbot")).unwrap(), Wrote::KeptYours);
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "my own words");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn db_with_trade(dir: &Path) -> (Db, Position) {
        let db = Db::open(":memory:").unwrap();
        learning::init(&db);
        let mut p = closed(0, "CAT", 0.018, "trailing-stop");
        p.id = db.insert_position(&p);
        db.save_position(&p);
        db.insert_decision(&decision("chain", "pass", ""));
        let _ = dir;
        (db, p)
    }

    #[test]
    fn nothing_is_written_while_disconnected() {
        let dir = tmp("off");
        let (db, p) = db_with_trade(&dir);
        assert!(!VaultSettings::load(&db).connected());
        on_trade_closed(&db, &p);
        situation(&db, T0, "kill-switch", "Kill switch", "x", "");
        missed_coin(&db, T0, 95.0, "Missed X", "m");
        on_lessons_changed(&db, T0);
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "the vault folder stays empty");
        assert!(backfill(&db).is_err());
        assert_eq!(test(&db, T0).ok, false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn connect_test_toggles_and_disconnect() {
        let dir = tmp("conn");
        let db = Db::open(":memory:").unwrap();
        assert!(connect(&db, dir.join("missing").to_str().unwrap()).is_err());
        let st = connect(&db, dir.to_str().unwrap()).unwrap();
        assert!(st.folder_exists && !st.looks_like_obsidian);
        std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
        assert!(status(&db).looks_like_obsidian);
        let t = test(&db, T0);
        assert!(t.ok, "{}", t.message);
        assert!(std::fs::read_to_string(dir.join(FOLDER).join("Test note.md")).unwrap().contains("Claude Trader wrote this"));
        let mut s = VaultSettings::load(&db);
        s.trades = false;
        set_toggles(&db, &s);
        assert!(!VaultSettings::load(&db).trades && VaultSettings::load(&db).days);
        disconnect(&db);
        assert!(!VaultSettings::load(&db).connected());
        assert!(dir.join(FOLDER).join("Test note.md").is_file(), "disconnecting never deletes notes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn closing_a_trade_writes_trade_coin_day_and_index_and_toggles_work() {
        let dir = tmp("close");
        let (db, p) = db_with_trade(&dir);
        connect(&db, dir.to_str().unwrap()).unwrap();
        let mut s = VaultSettings::load(&db);
        s.coins = false;
        s.save(&db);
        on_trade_closed(&db, &p);
        let root = dir.join(FOLDER);
        let trade = std::fs::read_to_string(root.join("Trades").join(trade_note_name(&p))).unwrap();
        assert!(trade.contains("| chain | pass |") && trade.contains("#win"));
        assert!(!root.join("Coins").exists(), "coins toggle off");
        let day = std::fs::read_to_string(root.join("Days").join(format!("{}.md", day_of(T0 + 95_000)))).unwrap();
        assert!(day.contains("| Trades | 1 (1 wins, 0 losses") && day.contains("[[Trades/"), "{day}");
        let idx = std::fs::read_to_string(root.join("Index.md")).unwrap();
        assert!(idx.contains("[[Trades/") && idx.contains("[[Days/"), "{idx}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backfill_writes_past_trades_days_coins_lessons_and_missed_coins() {
        let dir = tmp("back");
        let (db, p) = db_with_trade(&dir);
        let mut p2 = closed(0, "DOG", -0.02, "hard-stop");
        p2.mint = "MintBBB222".into();
        p2.id = db.insert_position(&p2);
        db.save_position(&p2);
        db.with_conn(|c| {
            c.execute("INSERT INTO reviews(ts,trades,json) VALUES(?,?,?)", rusqlite::params![T0, 20, serde_json::to_string(&learning::ReviewResult { trades_reviewed: 20, ..Default::default() }).unwrap()]).unwrap();
        });
        db.with_conn(|c| {
            c.execute("INSERT INTO learning_notes(ts,kind,text,source_url,mint) VALUES(?,?,?,?,?)", rusqlite::params![T0, "missed", "Missed RUDOLPH (score 94.5): blocked by x.", "", "MintCCC333"]).unwrap();
        });
        connect(&db, dir.to_str().unwrap()).unwrap();
        let r = backfill(&db).unwrap();
        assert_eq!((r.trades, r.days, r.coins), (2, 1, 3), "{r:?}");
        let root = dir.join(FOLDER);
        assert!(root.join("Trades").join(trade_note_name(&p)).is_file());
        assert!(root.join("Coins/MintBBB222.md").is_file() && root.join("Coins/MintCCC333.md").is_file());
        assert!(std::fs::read_dir(root.join("Lessons")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("review 1 (20 trades)")));
        assert!(std::fs::read_dir(root.join("Situations")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("missed-coin RUDOLPH")));
        let again = backfill(&db).unwrap();
        assert_eq!(again.trades, 2, "running it twice is harmless");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missed_coins_under_80_are_not_written() {
        let dir = tmp("missed");
        let db = Db::open(":memory:").unwrap();
        learning::init(&db);
        connect(&db, dir.to_str().unwrap()).unwrap();
        missed_coin(&db, T0, 79.9, "Missed LOW (score 79.9): blocked.", "m1");
        assert!(!dir.join(FOLDER).join("Situations").exists());
        missed_coin(&db, T0, 90.0, "Missed HIGH (score 90.0): blocked. Over the next 5 minutes the price ended +12%.", "m2");
        let f: Vec<_> = std::fs::read_dir(dir.join(FOLDER).join("Situations")).unwrap().flatten().collect();
        assert_eq!(f.len(), 1);
        assert!(std::fs::read_to_string(f[0].path()).unwrap().contains("ended +12%"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
