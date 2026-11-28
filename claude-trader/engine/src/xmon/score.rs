//! The X community score (0-100) and the hard blocks. Pure functions: no database, no clock, no network.
//! Weights come from `config.xMonitor` and are STARTING GUESSES from research (SPEC 4e), not measured values.
//! An unknown part gives 0 points and a note. A failed or stale scrape is "unknown", never a pass.

use super::model::*;
use crate::config::XMonitor as Cfg;
use serde::{Deserialize, Serialize};

const DAY_MS: i64 = 86_400_000;
/// A username change this recent blocks a profile link. Starting guesses (SPEC 4e): no data supports them yet.
const ABOUT_RECENT_DAYS: i64 = 90;
/// Or this many changes whenever they happened.
const ABOUT_MAX_CHANGES: u32 = 3;

/// Hard ceiling on what X can add to the token score. `config.xMonitor.maxPoints` can lower it, never raise it.
pub const X_MAX_POINTS: f64 = 15.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum XStatus {
    /// A real score exists.
    Ready,
    /// No usable data (not ready, failed, stale, extension off). Counts as nothing, not as a pass.
    Unknown,
    /// A hard block applies. The coin must not be bought.
    Blocked,
    /// The coin has no X link.
    NoLink,
    /// X Monitor is off.
    Off,
}

impl XStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            XStatus::Ready => "ready",
            XStatus::Unknown => "unknown",
            XStatus::Blocked => "blocked",
            XStatus::NoLink => "nolink",
            XStatus::Off => "off",
        }
    }
}

/// Points each part earned (not percentages). They add up to the raw total before it is scaled to 0-100.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XComponents {
    pub kol: f64,
    pub engagement: f64,
    pub replies: f64,
    pub growth: f64,
    pub integrity: f64,
    pub community: f64,
    pub narrative: f64,
}

/// The full answer for one coin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XEval {
    pub status: XStatus,
    /// Plain words: why unknown / off, or the first block.
    pub reason: String,
    /// 0-100 (0 unless ready).
    pub score: f64,
    /// Points added to the token score: round(score * pointsFactor), at most 15. 0 unless ready.
    pub points: u32,
    pub components: XComponents,
    pub flags: Vec<String>,
    pub blocks: Vec<String>,
    /// Place among coins linking the same tweet or community (0 = not applicable) and how many link it.
    pub rank: u32,
    pub clones: u32,
    pub kind: String,
    pub handle: String,
    pub rest_id: String,
    pub tweet_id: String,
    /// The exact post, profile or community this coin links to.
    pub url: String,
    pub link_label: String,
    /// How old the data behind the score is, in minutes (0 when unknown).
    pub age_min: i64,
}

impl XEval {
    pub fn empty(status: XStatus, reason: &str) -> XEval {
        XEval { status, reason: reason.into(), score: 0.0, points: 0, components: XComponents::default(), flags: vec![], blocks: vec![], rank: 0, clones: 0, kind: String::new(), handle: String::new(), rest_id: String::new(), tweet_id: String::new(), url: String::new(), link_label: String::new(), age_min: 0 }
    }
    pub fn is_blocked(&self) -> bool {
        self.status == XStatus::Blocked
    }
}

/// Follower history for the same account (earlier sightings of this account, from our own database).
#[derive(Debug, Clone, PartialEq)]
pub struct Growth {
    pub prev_followers: u64,
    pub minutes: f64,
}

/// What our own database remembers about this X account.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    /// Handles this account (same numeric id) was seen under before, other than the current one.
    pub other_handles: Vec<String>,
    /// Coins (symbols) that earlier linked this same account as their own profile.
    pub earlier_tokens: Vec<String>,
}

pub struct XInputs<'a> {
    pub target: &'a XTarget,
    pub token_name: &'a str,
    pub symbol: &'a str,
    pub mint: &'a str,
    pub coin_ts: i64,
    pub now: i64,
    /// State of the primary result: ok, not_found, suspended or protected.
    pub primary_state: &'a str,
    /// The profile (profile link) or the post's author (post link).
    pub user: Option<&'a XUser>,
    pub posts: &'a [XTweet],
    pub tweet: Option<&'a XTweet>,
    pub replies: &'a [XReply],
    pub community: Option<&'a XCommunityInfo>,
    pub smart: Option<&'a XSmart>,
    pub growth: Option<Growth>,
    pub history: &'a History,
    /// X's own About data for the profile (username changes, country, affiliate), when it was captured.
    pub about: Option<&'a XAbout>,
    pub rank: u32,
    pub clones: u32,
    /// Age of the primary data in minutes.
    pub age_min: i64,
}

// ------------------------------------------------------------------ text helpers

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Lower-case words without links and @mentions.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| !w.starts_with('@') && !w.to_ascii_lowercase().starts_with("http"))
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '$').flat_map(|c| c.to_lowercase()).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect()
}

fn normalized(text: &str) -> String {
    words(text).join(" ")
}

const STOP: &[&str] = &["the", "coin", "token", "of", "and", "official", "sol", "solana", "pump", "fun", "inu", "for", "with"];

/// Does a post name the coin: its ticker, a real word of its name, or its address?
pub fn ties_to_token(text: &str, name: &str, symbol: &str, mint: &str) -> bool {
    let low = text.to_lowercase();
    if mint.len() >= 8 && (low.contains(&mint[..8].to_lowercase()) || low.contains(&mint.to_lowercase())) {
        return true;
    }
    let ws = words(text);
    let sym = alnum(symbol);
    if sym.len() >= 2 && ws.iter().any(|w| w.trim_start_matches('$') == sym) {
        return true;
    }
    name.split(|c: char| !c.is_alphanumeric()).map(|w| w.to_lowercase()).filter(|w| w.len() >= 3 && !STOP.contains(&w.as_str())).any(|w| ws.iter().any(|x| x.trim_start_matches('$') == w))
}

const GENERIC: &[&str] = &["gm", "gn", "lfg", "lfgg", "lfggg", "lfgo", "wagmi", "moon", "mooning", "bullish", "based", "nice", "wen", "send", "it", "100x", "1000x", "gem", "ape", "fire", "good", "great", "cool", "wow", "to", "the", "lets", "go", "early", "buy", "this", "so", "lol", "yes", "yep", "congrats", "gmgm", "lets", "let", "s"];

fn is_generic(text: &str) -> bool {
    let w = words(text);
    !w.is_empty() && w.len() <= 4 && w.iter().all(|x| GENERIC.contains(&x.as_str()))
}

fn emoji_only(text: &str) -> bool {
    let kept: String = text.split_whitespace().filter(|w| !w.starts_with('@') && !w.to_ascii_lowercase().starts_with("http")).collect();
    !kept.chars().any(|c| c.is_alphanumeric())
}

fn digit_suffix_handle(h: &str) -> bool {
    let tail = h.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    tail >= 6 || h.chars().filter(|c| c.is_ascii_digit()).count() >= 8
}

/// Share of the sampled replies that look like bots, and the sample size.
pub fn bot_reply_stats(replies: &[XReply], now: i64) -> (u32, u32) {
    let mut counts: std::collections::HashMap<String, u32> = Default::default();
    for r in replies {
        let n = normalized(&r.text);
        if !n.is_empty() {
            *counts.entry(n).or_default() += 1;
        }
    }
    let mut bots = 0u32;
    for r in replies {
        let n = normalized(&r.text);
        let content = emoji_only(&r.text) || is_generic(&r.text) || (!n.is_empty() && counts.get(&n).copied().unwrap_or(0) >= 2);
        let acct = [r.author_created_at_ms > 0 && now - r.author_created_at_ms < 7 * DAY_MS, r.default_avatar, digit_suffix_handle(&r.author_screen_name)].iter().filter(|b| **b).count();
        if content || acct >= 2 || (acct >= 1 && n.len() < 25) {
            bots += 1;
        }
    }
    (bots, replies.len() as u32)
}

/// Largest share of accounts (with a known creation date) made inside one 30-day window. Bought followers are made in bulk.
pub fn created_cluster_pct(created: &[i64]) -> f64 {
    let v: Vec<i64> = created.iter().copied().filter(|t| *t > 0).collect();
    if v.is_empty() {
        return 0.0;
    }
    let best = v.iter().map(|s| v.iter().filter(|t| **t >= *s && **t < *s + 30 * DAY_MS).count()).max().unwrap_or(0);
    best as f64 / v.len() as f64 * 100.0
}

fn shares_substring(a: &str, b: &str, n: usize) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len() < n || b.len() < n {
        return false;
    }
    let bs: String = b.iter().collect();
    a.windows(n).any(|w| bs.contains(&w.iter().collect::<String>()))
}

/// Does the handle fit the account's display name or the coin?
pub fn handle_fits(handle: &str, name: &str, token_name: &str, symbol: &str) -> bool {
    let h = alnum(handle);
    if h.len() < 3 {
        return true;
    }
    [alnum(name), alnum(token_name), alnum(symbol)].iter().any(|o| o.len() >= 3 && (o.contains(&h) || h.contains(o.as_str()) || shares_substring(&h, o, 4)))
}

// ------------------------------------------------------------------ the parts

fn lerp(x: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    if x1 <= x0 {
        return y1;
    }
    y0 + (y1 - y0) * ((x - x0) / (x1 - x0)).clamp(0.0, 1.0)
}

/// Smart (KOL) followers: 0 = 0, 1 = 8/30, 3 = 18/30, 5 = 25/30, 10 or more (or one top-tier KOL) = full.
pub fn kol_frac(count: u32, top_followers: u64, top_tier: u64) -> f64 {
    let c = count as f64;
    let f = if c <= 0.0 {
        0.0
    } else if c < 1.0 {
        0.0
    } else if c < 3.0 {
        lerp(c, 1.0, 3.0, 8.0 / 30.0, 18.0 / 30.0)
    } else if c < 5.0 {
        lerp(c, 3.0, 5.0, 18.0 / 30.0, 25.0 / 30.0)
    } else {
        lerp(c, 5.0, 10.0, 25.0 / 30.0, 1.0)
    };
    if count > 0 && top_followers >= top_tier && top_tier > 0 { 1.0 } else { f }
}

fn interactions(t: &XTweet) -> f64 {
    (t.likes + t.replies + t.reposts + t.quotes) as f64
}

/// Engagement as a percentage: interactions per view when views are known, else per follower.
pub fn engagement_rate_pct(tweet: Option<&XTweet>, posts: &[XTweet], followers: u64) -> Option<f64> {
    if let Some(t) = tweet {
        if t.views > 0 {
            return Some(interactions(t) / t.views as f64 * 100.0);
        }
        return if followers > 0 { Some(interactions(t) / followers as f64 * 100.0) } else { None };
    }
    let own: Vec<&XTweet> = posts.iter().filter(|p| !p.is_retweet).take(10).collect();
    if own.is_empty() {
        return None;
    }
    let views: u64 = own.iter().map(|p| p.views).sum();
    let inter: f64 = own.iter().map(|p| interactions(p)).sum();
    if views > 0 {
        Some(inter / views as f64 * 100.0)
    } else if followers > 0 {
        Some(inter / (own.len() as f64 * followers as f64) * 100.0)
    } else {
        None
    }
}

/// 0..1. Under 0.1% = 0, 0.3% = 0.25, 1% = 0.7, up to 5% = 0.85, above 5% = full only when the repliers look varied.
pub fn engagement_frac(rate_pct: f64, followers: u64, diverse: bool) -> f64 {
    let r = rate_pct;
    if r < 0.1 {
        0.0
    } else if r < 0.3 {
        lerp(r, 0.1, 0.3, 0.0, 0.25)
    } else if r < 1.0 {
        lerp(r, 0.3, 1.0, 0.25, 0.7)
    } else if r <= 5.0 {
        lerp(r, 1.0, 5.0, 0.7, 0.85)
    } else if r > 10.0 && followers < 500 {
        0.3 // a huge rate on a tiny account is suspicious
    } else if diverse {
        1.0
    } else {
        0.85
    }
}

fn age_days(created_ms: i64, now: i64) -> Option<f64> {
    if created_ms > 0 { Some((now - created_ms).max(0) as f64 / DAY_MS as f64) } else { None }
}

/// Followers per day since the account was made. Only meaningful for young accounts.
pub fn followers_per_day(u: &XUser, now: i64) -> Option<f64> {
    age_days(u.created_at_ms, now).map(|d| u.followers as f64 / d.max(1.0 / 24.0))
}

/// Narrative reach of an account: 1k followers 0.1, 10k 0.3, 50k 0.6, 200k 0.85, 1M 1.0.
fn reach_frac(f: u64) -> f64 {
    let f = f as f64;
    if f < 1_000.0 {
        lerp(f, 0.0, 1_000.0, 0.0, 0.1)
    } else if f < 10_000.0 {
        lerp(f, 1_000.0, 10_000.0, 0.1, 0.3)
    } else if f < 50_000.0 {
        lerp(f, 10_000.0, 50_000.0, 0.3, 0.6)
    } else if f < 200_000.0 {
        lerp(f, 50_000.0, 200_000.0, 0.6, 0.85)
    } else {
        lerp(f, 200_000.0, 1_000_000.0, 0.85, 1.0)
    }
}

fn clone_mult(rank: u32) -> f64 {
    match rank {
        0 | 1 => 1.0,
        2 | 3 => 0.8,
        4 | 5 => 0.4,
        _ => 0.0,
    }
}

fn excerpt(s: &str, n: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= n { one } else { format!("{}...", one.chars().take(n).collect::<String>()) }
}

// ------------------------------------------------------------------ the score

/// Score one coin from data that is present and fresh. See `XInputs`.
pub fn score(i: &XInputs, cfg: &Cfg) -> XEval {
    let mut ev = XEval::empty(XStatus::Ready, "");
    ev.kind = i.target.kind().into();
    ev.url = i.target.url();
    ev.link_label = match i.target {
        XTarget::Status { .. } => "X post".into(),
        XTarget::Profile { .. } => "X profile".into(),
        XTarget::Community { .. } => "X community".into(),
    };
    ev.handle = i.user.map(|u| u.screen_name.clone()).or_else(|| i.target.handle().map(String::from)).unwrap_or_default();
    ev.rest_id = i.user.map(|u| u.rest_id.clone()).unwrap_or_default();
    ev.tweet_id = i.target.tweet_id().into();
    ev.rank = i.rank;
    ev.clones = i.clones;
    ev.age_min = i.age_min;
    let mut blocks: Vec<String> = vec![];
    let mut flags: Vec<String> = vec![];

    // 1. missing, suspended or protected link
    match i.primary_state {
        "not_found" => blocks.push(format!("The linked X {} does not exist.", if matches!(i.target, XTarget::Status { .. }) { "post" } else { "account" })),
        "suspended" => blocks.push("The linked X account is suspended.".into()),
        "protected" => blocks.push("The linked X account is protected (private).".into()),
        _ => {}
    }
    if !blocks.is_empty() {
        ev.status = XStatus::Blocked;
        ev.reason = blocks[0].clone();
        ev.blocks = blocks;
        return ev;
    }

    let w = &cfg.weights;
    let wsum = w.kol + w.engagement + w.replies + w.growth + w.integrity + w.community + w.narrative;
    let mut c = XComponents::default();
    let followers = i.user.map(|u| u.followers).unwrap_or(0);

    // smart followers
    let smart_text = |s: &XSmart| {
        let names: Vec<String> = s.handles.iter().take(3).map(|h| format!("@{}", h.handle)).collect();
        format!("{} smart follower(s){}", s.count, if names.is_empty() { String::new() } else { format!(" ({})", names.join(", ")) })
    };
    match i.smart {
        Some(s) => {
            c.kol = w.kol * kol_frac(s.count, s.top_followers, cfg.top_tier_followers);
            if s.count == 0 {
                flags.push("None of the accounts the spare X account follows follow this one (is it following enough KOLs?).".into());
            } else {
                flags.push(smart_text(s));
            }
        }
        None => flags.push("No smart-follower data yet.".into()),
    }

    // replies: bot share and authenticity
    let (bots, sample) = bot_reply_stats(i.replies, i.now);
    let bot_pct = if sample > 0 { bots as f64 / sample as f64 * 100.0 } else { 0.0 };
    let enough = sample >= cfg.min_reply_sample;
    if enough {
        let authentic = 1.0 - bot_pct / 100.0;
        c.replies = w.replies * lerp(authentic, 0.3, 0.7, 0.0, 1.0);
        if bot_pct >= cfg.block_bot_reply_pct {
            blocks.push(format!("{bot_pct:.0}% of the sampled replies look like bots ({bots} of {sample})."));
        } else if bot_pct >= 15.0 {
            flags.push(format!("{bot_pct:.0}% of the sampled replies look like bots."));
        }
    } else if matches!(i.target, XTarget::Status { .. }) {
        flags.push(format!("Only {sample} replies sampled, too few to judge them."));
    } else {
        flags.push("Replies were not sampled (the coin links a profile, not a post).".into());
    }

    // engagement
    let rate = engagement_rate_pct(i.tweet, i.posts, followers);
    match rate {
        Some(r) => {
            let diverse = enough && bot_pct < 15.0;
            c.engagement = w.engagement * engagement_frac(r, followers, diverse);
            if r < 0.3 {
                flags.push(format!("Low engagement ({r:.2}% of views)."));
            }
        }
        None => flags.push("No engagement data.".into()),
    }

    // follower growth
    let growth_note;
    let mut jump_blocked = false;
    match (&i.growth, i.user) {
        (Some(g), Some(u)) if g.prev_followers > 0 && g.minutes >= 5.0 => {
            let ratio = u.followers as f64 / g.prev_followers as f64;
            let low_eng = rate.map(|r| r < 0.3).unwrap_or(true);
            if ratio >= 3.0 && g.minutes <= 60.0 && low_eng {
                jump_blocked = true;
                blocks.push(format!("Followers jumped {ratio:.1}x in {:.0} minutes with almost no engagement (bought followers).", g.minutes));
                growth_note = 0.0;
            } else if ratio >= 3.0 && low_eng {
                flags.push(format!("Followers grew {ratio:.1}x with flat engagement."));
                growth_note = 0.0;
            } else if ratio > 1.02 {
                growth_note = if rate.map(|r| r >= 0.3).unwrap_or(false) { 1.0 } else { 0.4 };
                flags.push(format!("Followers up {:.0}% in {:.0} minutes.", (ratio - 1.0) * 100.0, g.minutes));
            } else if ratio >= 0.98 {
                growth_note = 0.5;
            } else {
                growth_note = 0.2;
                flags.push("Followers are dropping.".into());
            }
        }
        (_, Some(u)) => match followers_per_day(u, i.now) {
            // first sighting: only the account's age is known
            Some(fpd) if age_days(u.created_at_ms, i.now).map(|d| d < 14.0).unwrap_or(false) && fpd >= 1000.0 => {
                growth_note = if rate.map(|r| r >= 0.3).unwrap_or(false) { 0.6 } else { 0.2 };
                flags.push(format!("Fast-rising account: about {} followers a day.", fpd.round()));
            }
            Some(_) => growth_note = 0.5,
            None => {
                growth_note = 0.0;
                flags.push("Follower history unknown (first look).".into());
            }
        },
        _ => growth_note = 0.0,
    }
    c.growth = w.growth * growth_note;

    // integrity of the account and handle
    if let Some(u) = i.user {
        let mut frac = 0.0;
        match age_days(u.created_at_ms, i.now) {
            Some(d) if d >= 30.0 => frac += 0.4,
            Some(d) if d >= 7.0 => frac += 0.2,
            Some(d) if d < 1.0 && alnum(&u.screen_name) == alnum(i.token_name) => flags.push("Brand-new account named exactly like the coin.".into()),
            _ => {}
        }
        let mut rename = false;
        if let Some(h) = i.target.handle() {
            if !h.eq_ignore_ascii_case(&u.screen_name) && !u.screen_name.is_empty() {
                rename = true;
                if matches!(i.target, XTarget::Profile { .. }) {
                    blocks.push(format!("The link says @{h} but the account is @{}: it was renamed.", u.screen_name));
                } else {
                    flags.push(format!("The link names @{h} but the post is by @{}.", u.screen_name));
                }
            }
        }
        if matches!(i.target, XTarget::Profile { .. }) {
            if let Some(old) = i.history.other_handles.first() {
                rename = true;
                blocks.push(format!("This X account used to be @{old} (same account id). It was renamed."));
            }
            if let Some(t) = i.history.earlier_tokens.first() {
                rename = true;
                blocks.push(format!("This X account was already linked to an earlier coin ({t}). Reused account."));
            }
        }
        // X's own count of username changes (the About page data), when the page showed it
        if matches!(i.target, XTarget::Profile { .. }) {
            if let Some(a) = i.about.filter(|a| a.changes_known) {
                let recent = a.last_changed_ms > 0 && i.now - a.last_changed_ms <= ABOUT_RECENT_DAYS * DAY_MS;
                if a.username_changes > 0 && (recent || a.username_changes >= ABOUT_MAX_CHANGES) {
                    rename = true;
                    let when = if a.last_changed_ms > 0 { format!(", last {} days ago", ((i.now - a.last_changed_ms) / DAY_MS).max(0)) } else { String::new() };
                    blocks.push(format!("X says this account changed its username {} time(s){when}. A renamed account is a common clone or rug sign.", a.username_changes));
                } else if a.username_changes > 0 {
                    flags.push(format!("The username was changed {} time(s), last {} days ago.", a.username_changes, if a.last_changed_ms > 0 { ((i.now - a.last_changed_ms) / DAY_MS).max(0).to_string() } else { "an unknown number of".into() }));
                }
                if !a.country.is_empty() {
                    flags.push(format!("X shows the account as based in {}{}.", a.country, if a.location_accurate == Some(false) { " (X marks this location as possibly inaccurate)" } else { "" }));
                } else if a.location_accurate == Some(false) {
                    flags.push("X marks the account's location as possibly inaccurate.".into());
                }
                if !a.affiliate.is_empty() {
                    flags.push(format!("The account is affiliated with @{}.", a.affiliate));
                }
            }
        }
        if !rename {
            frac += 0.4;
        }
        let mention = match i.tweet {
            Some(t) => ties_to_token(&t.text, i.token_name, i.symbol, i.mint),
            None => ties_to_token(&u.description, i.token_name, i.symbol, i.mint) || i.posts.iter().take(10).any(|p| ties_to_token(&p.text, i.token_name, i.symbol, i.mint)),
        };
        if mention {
            frac += 0.2;
        } else if !matches!(i.target, XTarget::Status { .. }) {
            flags.push("The account's bio and posts do not mention the coin.".into());
        }
        if !handle_fits(&u.screen_name, &u.name, i.token_name, i.symbol) {
            flags.push(format!("The handle @{} does not match its name \"{}\" or the coin.", u.screen_name, excerpt(&u.name, 30)));
        }
        c.integrity = w.integrity * frac;
    } else {
        flags.push("No account data.".into());
    }

    // X Community
    match i.community {
        Some(cm) => match cm.members {
            Some(m) if m >= 100 && cm.posts_seen >= 3 => {
                c.community = w.community * (0.6 + 0.4 * lerp(m as f64, 100.0, 1000.0, 0.0, 1.0));
            }
            Some(m) if m >= 100 => flags.push(format!("The X Community has {m} members but almost no posts: it looks empty or fake.")),
            Some(m) => c.community = w.community * 0.2 * (m as f64 / 100.0),
            None => flags.push("No member count for the X Community.".into()),
        },
        None => {}
    }

    // narrative strength: reach and age of the source post's author, discounted by copy rank
    let mult = clone_mult(i.rank);
    c.narrative = w.narrative
        * match (i.tweet, i.user, i.community) {
            (Some(t), Some(u), _) => {
                let mut f = reach_frac(u.followers);
                if t.views > 0 && u.followers > 0 && t.views as f64 / u.followers as f64 >= 0.5 {
                    f = (f + 0.1).min(1.0);
                }
                if age_days(u.created_at_ms, i.now).map(|d| d < 30.0).unwrap_or(true) {
                    f *= 0.5;
                }
                f * mult
            }
            (None, Some(u), _) => reach_frac(u.followers) * 0.5 * mult,
            (None, None, Some(cm)) => cm.members.map(|m| lerp(m as f64, 100.0, 10_000.0, 0.2, 1.0) * 0.5).unwrap_or(0.0) * mult,
            _ => 0.0,
        };

    // blocks that need the whole picture
    if let (XTarget::Status { .. }, Some(t), Some(u)) = (i.target, i.tweet, i.user) {
        if u.followers >= cfg.big_account_followers && !ties_to_token(&t.text, i.token_name, i.symbol, i.mint) {
            blocks.push(format!("This links a post by a big account (@{}, {} followers) that never mentions the coin's name, ticker or address.", u.screen_name, u.followers));
        }
    }
    if !jump_blocked && sample >= cfg.min_reply_sample {
        let created: Vec<i64> = i.replies.iter().map(|r| r.author_created_at_ms).collect();
        let known = created.iter().filter(|t| **t > 0).count();
        let low_eng = rate.map(|r| r < 0.3).unwrap_or(false);
        if known as u32 >= cfg.min_reply_sample && created_cluster_pct(&created) > 60.0 && low_eng {
            blocks.push("More than 60% of the sampled accounts were made within one 30-day window and engagement is under 0.3% (bought followers).".into());
        }
    }
    if i.rank > cfg.max_clone_rank {
        blocks.push(format!("Copy #{} of {} coins linking the same {}. Only the first {} count.", i.rank, i.clones, if i.target.community_id().is_empty() { "post" } else { "community" }, cfg.max_clone_rank));
    } else if i.clones > cfg.max_clones {
        blocks.push(format!("{} coins link the same {}. That is a copy farm.", i.clones, if i.target.community_id().is_empty() { "post" } else { "community" }));
    }
    if i.rank > 1 && i.rank <= cfg.max_clone_rank {
        flags.push(format!("Copy #{} of {} coins linking the same {}.", i.rank, i.clones, if i.target.community_id().is_empty() { "post" } else { "community" }));
    }

    // total
    let raw = c.kol + c.engagement + c.replies + c.growth + c.integrity + c.community + c.narrative;
    let s = if wsum > 0.0 { (raw / wsum * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    ev.score = (s * 10.0).round() / 10.0;
    ev.points = points_for(s, cfg);
    ev.components = XComponents { kol: r1(c.kol), engagement: r1(c.engagement), replies: r1(c.replies), growth: r1(c.growth), integrity: r1(c.integrity), community: r1(c.community), narrative: r1(c.narrative) };
    ev.flags = flags;
    if !blocks.is_empty() {
        ev.status = XStatus::Blocked;
        ev.reason = blocks[0].clone();
        ev.points = 0;
        ev.blocks = blocks;
    }
    ev
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// round(community * pointsFactor), never more than 15 and never more than `maxPoints`.
pub fn points_for(score: f64, cfg: &Cfg) -> u32 {
    let cap = cfg.max_points.min(X_MAX_POINTS).max(0.0);
    (score.clamp(0.0, 100.0) * cfg.points_factor.clamp(0.0, 0.15)).round().min(cap) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_000_000_000;
    const MIN: i64 = 60_000;

    fn cfg() -> Cfg {
        Cfg::default()
    }

    fn user(handle: &str, followers: u64, age_days: i64) -> XUser {
        XUser { rest_id: "111".into(), screen_name: handle.into(), name: "Moon Bear".into(), created_at_ms: NOW - age_days * DAY_MS, followers, following: 100, statuses: 300, ..Default::default() }
    }

    fn post(text: &str, views: u64, inter: u64) -> XTweet {
        XTweet { id: "1".into(), text: text.into(), views, likes: inter, ..Default::default() }
    }

    fn reply(text: &str, handle: &str, age_days: i64, default_avatar: bool) -> XReply {
        XReply { id: "r".into(), text: text.into(), author_screen_name: handle.into(), author_created_at_ms: NOW - age_days * DAY_MS, default_avatar, ..Default::default() }
    }

    fn good_replies() -> Vec<XReply> {
        (0..10).map(|i| reply(&format!("Looked at holder {i} and the curve, the dev has not sold yet so I am watching"), &format!("trader{i}"), 400 + i, false)).collect()
    }

    fn smart(n: u32, top: u64) -> XSmart {
        XSmart { count: n, handles: (0..n).map(|i| XSmartFollower { handle: format!("kol{i}"), followers: top, rest_id: i.to_string() }).collect(), top_followers: top, has_more: false }
    }

    struct Fx {
        target: XTarget,
        user: Option<XUser>,
        posts: Vec<XTweet>,
        tweet: Option<XTweet>,
        replies: Vec<XReply>,
        community: Option<XCommunityInfo>,
        smart: Option<XSmart>,
        growth: Option<Growth>,
        history: History,
        about: Option<XAbout>,
        rank: u32,
        clones: u32,
        state: &'static str,
    }

    impl Fx {
        fn profile() -> Fx {
            Fx { target: XTarget::Profile { handle: "moonbearsol".into() }, user: Some(user("moonbearsol", 8000, 90)), posts: vec![post("Moon Bear $MOONB is live", 10_000, 250)], tweet: None, replies: vec![], community: None, smart: Some(smart(6, 50_000)), growth: None, history: History::default(), about: None, rank: 0, clones: 0, state: "ok" }
        }
        fn status() -> Fx {
            let mut u = user("alphacalls", 182_000, 800);
            u.name = "Alpha Calls".into();
            Fx { target: XTarget::Status { handle: Some("alphacalls".into()), id: "1861500000000000001".into() }, user: Some(u.clone()), posts: vec![], tweet: Some(XTweet { id: "1861500000000000001".into(), text: "New one to watch: $MOONB, the bear that went to the moon".into(), views: 215_000, likes: 3100, replies: 410, reposts: 640, quotes: 55, author: Some(u), ..Default::default() }), replies: good_replies(), community: None, smart: Some(smart(6, 50_000)), growth: None, history: History::default(), about: None, rank: 1, clones: 1, state: "ok" }
        }
        fn eval(&self) -> XEval {
            self.eval_with(&cfg())
        }
        fn eval_with(&self, c: &Cfg) -> XEval {
            score(
                &XInputs { target: &self.target, token_name: "Moon Bear", symbol: "MOONB", mint: "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU", coin_ts: NOW - MIN, now: NOW, primary_state: self.state, user: self.user.as_ref(), posts: &self.posts, tweet: self.tweet.as_ref(), replies: &self.replies, community: self.community.as_ref(), smart: self.smart.as_ref(), growth: self.growth.clone(), history: &self.history, about: self.about.as_ref(), rank: self.rank, clones: self.clones, age_min: 1 },
                c,
            )
        }
    }

    #[test]
    fn the_starting_weights_are_the_researched_ones_and_add_to_100() {
        let w = cfg().weights;
        assert_eq!((w.kol, w.engagement, w.replies, w.growth, w.integrity, w.community, w.narrative), (30.0, 20.0, 10.0, 10.0, 10.0, 10.0, 10.0));
        assert_eq!(w.kol + w.engagement + w.replies + w.growth + w.integrity + w.community + w.narrative, 100.0);
    }

    #[test]
    fn a_strong_post_scores_high_and_each_part_stays_under_its_weight() {
        let e = Fx::status().eval();
        assert_eq!(e.status, XStatus::Ready, "{:?}", e.blocks);
        let c = &e.components;
        assert!(c.kol <= 30.0 && c.engagement <= 20.0 && c.replies <= 10.0 && c.growth <= 10.0 && c.integrity <= 10.0 && c.community <= 10.0 && c.narrative <= 10.0);
        assert!(e.score >= 60.0, "score {}", e.score);
        assert!(e.score <= 100.0);
        assert!(c.kol > 20.0, "6 smart followers is 83% of 30");
        assert!(c.replies > 9.0, "all replies are real");
        assert!(c.narrative > 6.0, "182k author, rank 1");
        assert_eq!(e.points, (e.score * 0.15).round() as u32);
        assert_eq!(e.link_label, "X post");
        assert_eq!(e.url, "https://x.com/alphacalls/status/1861500000000000001");
    }

    #[test]
    fn smart_followers_curve() {
        let t = 200_000;
        assert_eq!(kol_frac(0, 0, t), 0.0);
        assert!((kol_frac(1, 1000, t) - 8.0 / 30.0).abs() < 1e-9);
        assert!((kol_frac(3, 1000, t) - 18.0 / 30.0).abs() < 1e-9);
        assert!((kol_frac(5, 1000, t) - 25.0 / 30.0).abs() < 1e-9);
        assert_eq!(kol_frac(10, 1000, t), 1.0);
        assert_eq!(kol_frac(40, 1000, t), 1.0);
        assert_eq!(kol_frac(1, 300_000, t), 1.0, "one top-tier KOL is enough");
    }

    #[test]
    fn engagement_curve_follows_the_research_bands() {
        assert_eq!(engagement_frac(0.05, 5000, false), 0.0);
        assert!((engagement_frac(0.3, 5000, false) - 0.25).abs() < 1e-9);
        assert!((engagement_frac(1.0, 5000, false) - 0.7).abs() < 1e-9);
        assert!((engagement_frac(5.0, 5000, false) - 0.85).abs() < 1e-9);
        assert_eq!(engagement_frac(7.0, 5000, true), 1.0);
        assert_eq!(engagement_frac(7.0, 5000, false), 0.85);
        assert_eq!(engagement_frac(15.0, 200, true), 0.3, "a huge rate on a tiny account is suspicious");
        let t = post("x", 10_000, 150);
        assert!((engagement_rate_pct(Some(&t), &[], 5000).unwrap() - 1.5).abs() < 1e-9);
        let no_views = post("x", 0, 50);
        assert!((engagement_rate_pct(Some(&no_views), &[], 5000).unwrap() - 1.0).abs() < 1e-9, "falls back to followers");
        assert_eq!(engagement_rate_pct(None, &[], 5000), None);
    }

    #[test]
    fn reply_bot_detection_on_the_fixture_style_replies() {
        let rs = vec![
            reply("The chart plus the thread is why I looked. Holder spread is better than the last three launches.", "real_trader_ana", 500, false),
            reply("LFG 🚀🚀🚀", "bot_user48271905", 5, true),
            reply("LFG 🚀🚀🚀", "bot_user48271906", 5, true),
            reply("gm", "gm_fren_33", 300, false),
            reply("gm", "gm_fren_34", 300, false),
            reply("to the moon", "moonboi20260101", 7, false),
            reply("Not touching it. The tweet never mentions the contract and I cannot find the team.", "sol_sage", 400, false),
            reply("🔥🔥🔥", "bullish_bro", 6, false),
            reply("Bought a small bag at 9 SOL on the curve, will sell into the first push.", "dex_dan", 300, false),
            reply("Curve is at 11 SOL and the dev wallet has not sold. Watching the next 5 minutes.", "cryptokev_77", 300, false),
        ];
        let (bots, n) = bot_reply_stats(&rs, NOW);
        assert_eq!((bots, n), (6, 10));
        assert_eq!(bot_reply_stats(&good_replies(), NOW), (0, 10));
        assert!(is_generic("GM!") && is_generic("lets go") && is_generic("to the moon") && !is_generic("the dev wallet has not sold"));
        assert!(emoji_only("🚀🚀 @someone") && !emoji_only("lfg 🚀"));
        assert!(digit_suffix_handle("user48271905") && !digit_suffix_handle("real_trader_ana") && !digit_suffix_handle("kev77"));
    }

    #[test]
    fn thirty_percent_bot_replies_is_a_hard_block_and_29_is_not() {
        let mut f = Fx::status();
        // 10 replies, 3 bots = 30% = block
        for i in 0..3 {
            f.replies[i] = reply("gm", &format!("gm{i}"), 400, false);
        }
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("look like bots")), "{:?}", e.blocks);
        assert_eq!(e.points, 0, "a block means no points");
        // 2 of 10 = 20%: a flag, no block
        let mut g = Fx::status();
        for i in 0..2 {
            g.replies[i] = reply("gm", &format!("gm{i}"), 400, false);
        }
        let e = g.eval();
        assert_eq!(e.status, XStatus::Ready, "{:?}", e.blocks);
        assert!(e.flags.iter().any(|b| b.contains("look like bots")));
        // too few replies to judge: never a block
        let mut h = Fx::status();
        h.replies = (0..5).map(|i| reply("gm", &format!("gm{i}"), 400, false)).collect();
        assert_eq!(h.eval().status, XStatus::Ready);
    }

    #[test]
    fn missing_suspended_and_protected_links_block() {
        for (state, word) in [("not_found", "does not exist"), ("suspended", "suspended"), ("protected", "protected")] {
            let mut f = Fx::profile();
            f.state = state;
            let e = f.eval();
            assert_eq!(e.status, XStatus::Blocked, "{state}");
            assert!(e.blocks[0].contains(word), "{:?}", e.blocks);
            assert_eq!(e.points, 0);
            assert_eq!(e.score, 0.0);
        }
    }

    #[test]
    fn a_renamed_or_reused_account_blocks() {
        // the same account id was seen under another handle
        let mut f = Fx::profile();
        f.history.other_handles = vec!["oldbear".into()];
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("used to be @oldbear")));
        // the same account id was linked to an earlier coin
        let mut g = Fx::profile();
        g.history.earlier_tokens = vec!["OLDCOIN".into()];
        let e = g.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("OLDCOIN")));
        // the link names one handle but the account is another one
        let mut h = Fx::profile();
        h.user.as_mut().unwrap().screen_name = "newbear".into();
        let e = h.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("renamed")));
        // a clean account is fine
        assert_eq!(Fx::profile().eval().status, XStatus::Ready);
        // for a post link, the KOL author being used by many coins is not a reuse block
        let mut s = Fx::status();
        s.history.earlier_tokens = vec!["A".into(), "B".into()];
        s.history.other_handles = vec!["oldname".into()];
        assert_eq!(s.eval().status, XStatus::Ready);
    }

    fn about(changes: u32, days_ago: i64) -> XAbout {
        XAbout { username_changes: changes, changes_known: true, last_changed_ms: if changes > 0 { NOW - days_ago * DAY_MS } else { 0 }, country: "Nigeria".into(), location_accurate: Some(false), affiliate: String::new(), source: String::new() }
    }

    #[test]
    fn x_own_username_change_count_blocks_a_recent_or_serial_rename_and_only_flags_an_old_one() {
        // renamed 20 days ago: blocked, with X's own numbers in the reason
        let mut f = Fx::profile();
        f.about = Some(about(1, 20));
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("changed its username 1 time(s), last 20 days ago")), "{:?}", e.blocks);
        // renamed three times, the last one long ago: still a serial renamer
        let mut g = Fx::profile();
        g.about = Some(about(3, 400));
        assert_eq!(g.eval().status, XStatus::Blocked);
        // one old rename: a flag, not a block, and no integrity penalty for it
        let mut h = Fx::profile();
        h.about = Some(about(1, 400));
        let e = h.eval();
        assert_eq!(e.status, XStatus::Ready, "{:?}", e.blocks);
        assert!(e.flags.iter().any(|x| x.contains("changed 1 time(s), last 400 days ago")), "{:?}", e.flags);
        assert_eq!(e.components.integrity, Fx::profile().eval().components.integrity);
        // no changes at all
        let mut n = Fx::profile();
        n.about = Some(about(0, 0));
        assert_eq!(n.eval().status, XStatus::Ready);
        // an About answer without the changes field claims nothing
        let mut u = Fx::profile();
        u.about = Some(XAbout { changes_known: false, username_changes: 9, ..Default::default() });
        let e = u.eval();
        assert_eq!(e.status, XStatus::Ready);
        assert!(!e.flags.iter().any(|x| x.contains("username")));
        // never captured: nothing changes
        assert_eq!(Fx::profile().eval(), Fx::profile().eval());
    }

    #[test]
    fn country_and_affiliate_from_the_about_data_are_shown_as_flags_not_blocks() {
        let mut f = Fx::profile();
        f.about = Some(XAbout { country: "Nigeria".into(), location_accurate: Some(false), affiliate: "pumpfun".into(), changes_known: true, ..Default::default() });
        let e = f.eval();
        assert_eq!(e.status, XStatus::Ready);
        assert!(e.flags.iter().any(|x| x.contains("based in Nigeria") && x.contains("possibly inaccurate")), "{:?}", e.flags);
        assert!(e.flags.iter().any(|x| x.contains("affiliated with @pumpfun")));
        // a post link does not use the author's About data (it belongs to a profile job)
        let mut s = Fx::status();
        s.about = Some(about(2, 5));
        assert_eq!(s.eval().status, XStatus::Ready);
    }

    #[test]
    fn a_big_account_post_that_never_mentions_the_coin_blocks() {
        let mut f = Fx::status();
        f.tweet.as_mut().unwrap().text = "The market is moving. Stay patient, everyone.".into();
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("never mentions the coin")), "{:?}", e.blocks);
        // naming the ticker, a word of the name, or the address is enough
        for text in ["watch $MOONB closely", "the moon bear is back", "ca 7xKXtg2C is live", "MOONB"] {
            let mut g = Fx::status();
            g.tweet.as_mut().unwrap().text = text.into();
            assert_eq!(g.eval().status, XStatus::Ready, "{text}");
        }
        // a small account does not trigger this block
        let mut s = Fx::status();
        s.tweet.as_mut().unwrap().text = "unrelated words".into();
        s.user.as_mut().unwrap().followers = 4000;
        assert_eq!(s.eval().status, XStatus::Ready);
        assert!(ties_to_token("hello $MOONB", "Other", "MOONB", "x"));
        assert!(!ties_to_token("hello world", "Moon Bear", "MOONB", "7xKXtg2CW87d97"));
        assert!(!ties_to_token("the coin of the token", "The Coin", "AB", "x"), "stop words alone are not a tie");
    }

    #[test]
    fn a_follower_jump_with_no_engagement_is_the_bought_follower_block() {
        let mut f = Fx::profile();
        f.posts = vec![post("x", 50_000, 20)]; // 0.04%
        f.user.as_mut().unwrap().followers = 12_000;
        f.growth = Some(Growth { prev_followers: 3000, minutes: 40.0 });
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("bought followers")), "{:?}", e.blocks);
        // the same jump with healthy engagement is only fast growth
        let mut g = Fx::profile();
        g.posts = vec![post("x", 10_000, 300)];
        g.user.as_mut().unwrap().followers = 12_000;
        g.growth = Some(Growth { prev_followers: 3000, minutes: 40.0 });
        assert_eq!(g.eval().status, XStatus::Ready);
        // slow growth is fine
        let mut h = Fx::profile();
        h.growth = Some(Growth { prev_followers: 7800, minutes: 40.0 });
        h.user.as_mut().unwrap().followers = 8000;
        assert_eq!(h.eval().status, XStatus::Ready);
    }

    #[test]
    fn many_new_accounts_made_in_one_month_with_flat_engagement_is_the_sample_block() {
        let mut f = Fx::status();
        f.tweet.as_mut().unwrap().views = 400_000;
        f.tweet.as_mut().unwrap().likes = 50;
        f.tweet.as_mut().unwrap().replies = 5;
        f.tweet.as_mut().unwrap().reposts = 5;
        f.tweet.as_mut().unwrap().quotes = 0;
        // 10 varied replies, 8 from accounts made within 30 days of each other long ago (so not "new")
        for i in 0..8 {
            f.replies[i].author_created_at_ms = NOW - 300 * DAY_MS - i as i64 * 2 * DAY_MS;
        }
        for i in 8..10 {
            f.replies[i].author_created_at_ms = NOW - 900 * DAY_MS - i as i64 * 100 * DAY_MS;
        }
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked, "{:?}", e.flags);
        assert!(e.blocks.iter().any(|b| b.contains("one 30-day window")));
        assert!(created_cluster_pct(&[]) == 0.0);
        assert!((created_cluster_pct(&[1, NOW, NOW + DAY_MS]) - 66.7).abs() < 0.1);
    }

    #[test]
    fn copy_rank_and_clone_count_block() {
        let mut f = Fx::status();
        f.rank = 5;
        f.clones = 5;
        let e = f.eval();
        assert_eq!(e.status, XStatus::Ready, "5th is the last allowed");
        assert!(e.flags.iter().any(|x| x.contains("Copy #5")));
        f.rank = 6;
        f.clones = 6;
        let e = f.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks[0].contains("Copy #6"));
        let mut g = Fx::status();
        g.rank = 1;
        g.clones = 11;
        let e = g.eval();
        assert_eq!(e.status, XStatus::Blocked, "11 clones is a copy farm even for the first coin");
        assert!(e.blocks[0].contains("copy farm"));
        let mut h = Fx::status();
        h.rank = 1;
        h.clones = 10;
        assert_eq!(h.eval().status, XStatus::Ready);
        // community links count too
        let mut c = Fx::profile();
        c.target = XTarget::Community { id: "1861700000000000007".into() };
        c.user = None;
        c.smart = None;
        c.community = Some(XCommunityInfo { id: "1861700000000000007".into(), members: Some(412), posts_seen: 5, ..Default::default() });
        c.rank = 7;
        c.clones = 7;
        let e = c.eval();
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks[0].contains("community"));
    }

    #[test]
    fn narrative_shrinks_with_copy_rank() {
        let r = |rank: u32| {
            let mut f = Fx::status();
            f.rank = rank;
            f.clones = rank;
            f.eval().components.narrative
        };
        assert!(r(1) > r(2) && r(2) == r(3) && r(3) > r(4) && r(4) == r(5));
    }

    #[test]
    fn community_presence_needs_members_and_posts() {
        let mk = |members: Option<u64>, posts: u32| {
            let mut f = Fx::profile();
            f.target = XTarget::Community { id: "1861700000000000007".into() };
            f.user = None;
            f.posts = vec![];
            f.smart = None;
            f.community = Some(XCommunityInfo { members, posts_seen: posts, ..Default::default() });
            f.eval()
        };
        assert!(mk(Some(412), 5).components.community >= 6.0);
        assert!(mk(Some(5000), 5).components.community > mk(Some(412), 5).components.community);
        let empty = mk(Some(900), 0);
        assert_eq!(empty.components.community, 0.0);
        assert!(empty.flags.iter().any(|f| f.contains("almost no posts")));
        assert!(mk(Some(30), 4).components.community < 2.0);
    }

    #[test]
    fn unknown_parts_score_zero_never_a_pass() {
        let mut f = Fx::profile();
        f.smart = None;
        f.posts = vec![];
        f.user = Some(user("moonbearsol", 8000, 3));
        let e = f.eval();
        assert_eq!(e.components.kol, 0.0);
        assert_eq!(e.components.engagement, 0.0);
        assert!(e.flags.iter().any(|x| x.contains("No smart-follower data")));
        assert!(e.flags.iter().any(|x| x.contains("No engagement data")));
    }

    #[test]
    fn points_are_round_of_fifteen_percent_and_never_above_15() {
        let c = cfg();
        assert_eq!(points_for(100.0, &c), 15);
        assert_eq!(points_for(72.0, &c), 11);
        assert_eq!(points_for(70.0, &c), 11, "10.5 rounds up");
        assert_eq!(points_for(0.0, &c), 0);
        assert_eq!(points_for(500.0, &c), 15, "a score above 100 is clamped");
        assert_eq!(points_for(-5.0, &c), 0);
        // a config can lower the cap, never raise it
        let mut hot = cfg();
        hot.max_points = 40.0;
        hot.points_factor = 1.0;
        assert_eq!(points_for(100.0, &hot), 15);
        let mut low = cfg();
        low.max_points = 6.0;
        assert_eq!(points_for(100.0, &low), 6);
    }

    #[test]
    fn changing_a_weight_changes_the_score_and_the_total_is_rescaled_to_100() {
        let base = Fx::status().eval();
        let mut c = cfg();
        c.weights.kol = 0.0;
        let no_kol = Fx::status().eval_with(&c);
        assert!(no_kol.score < 100.0 && no_kol.components.kol == 0.0);
        let mut all = cfg();
        all.weights = crate::config::XWeights { kol: 60.0, engagement: 40.0, replies: 20.0, growth: 20.0, integrity: 20.0, community: 20.0, narrative: 20.0 };
        let doubled = Fx::status().eval_with(&all);
        assert!((doubled.score - base.score).abs() < 0.3, "doubling every weight changes nothing: {} vs {}", doubled.score, base.score);
    }

    #[test]
    fn growth_rises_with_history_and_flags_young_fast_accounts() {
        let mut f = Fx::profile();
        f.growth = Some(Growth { prev_followers: 6000, minutes: 30.0 });
        let e = f.eval();
        assert!(e.components.growth >= 9.0, "rising with healthy engagement: {}", e.components.growth);
        assert!(e.flags.iter().any(|x| x.contains("Followers up")));
        let mut y = Fx::profile();
        y.user = Some(user("moonbearsol", 9000, 2));
        let e = y.eval();
        assert!(e.flags.iter().any(|x| x.contains("Fast-rising")), "{:?}", e.flags);
    }

    #[test]
    fn the_handle_is_checked_against_the_name_and_the_coin() {
        assert!(handle_fits("moonbearsol", "Moon Bear", "Moon Bear", "MOONB"));
        assert!(handle_fits("realmoonbear", "someone", "Moon Bear", "MOONB"));
        assert!(!handle_fits("xk39dj2", "Elon Musk", "Moon Bear", "MOONB"));
        let mut f = Fx::profile();
        f.user.as_mut().unwrap().screen_name = "xk39dj2".into();
        f.target = XTarget::Profile { handle: "xk39dj2".into() };
        f.user.as_mut().unwrap().name = "Elon Musk".into();
        assert!(f.eval().flags.iter().any(|x| x.contains("does not match")));
    }
}
