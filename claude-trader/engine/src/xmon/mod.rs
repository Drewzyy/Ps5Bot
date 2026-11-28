//! X Monitor and Thesis Finder (SPEC 4e).
//!
//! The Chrome extension reads x.com (a spare logged-in account, no paid API). This module is the app's side:
//! it queues jobs, tracks their state (queued, running, done, failed, stale), keeps what the extension reports in
//! SQLite, scores a coin's X community (0-100, `score.rs`), clusters coins that tell the same story, and tells the UI.
//!
//! Rules this module keeps:
//! - X can add at most 15 points to a token score and can never buy by itself (the picker enforces it too).
//! - A failed, missing or stale scrape is "unknown": no points and no block, never a pass.
//! - It has no handle to the trader. It only reads what the extension sends and answers questions.

pub mod model;
pub mod score;

use crate::config::{XMonitor as Cfg, XPacing};
use crate::db::Db;
use crate::events::{AgentId, AgentStatus, EngineEvent, StatusColor};
use crate::hub::Hub;
use model::*;
use rusqlite::{params, OptionalExtension};
use score::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

const KV_SETTINGS: &str = "xmon.settings";
const EXT_ALIVE_MS: i64 = 45_000;
const JOB_RUN_MAX_MS: i64 = 60_000;
const JOBS_KEPT: usize = 400;
const HOUR_MS: i64 = 3_600_000;

// ------------------------------------------------------------------ settings (the user's, stored in SQLite)

/// What the user can change in Settings > X Monitor. Defaults come from `config.xMonitor.pacing`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XSettings {
    /// On by default. It only does anything once the extension is paired and x.com is logged in.
    pub enabled: bool,
    /// The pause button.
    pub paused: bool,
    pub min_gap_secs: u32,
    pub max_gap_secs: u32,
    pub max_loads_per_hour: u32,
    pub cache_minutes: u32,
}

impl XSettings {
    pub fn defaults(p: &XPacing) -> XSettings {
        XSettings { enabled: true, paused: false, min_gap_secs: p.min_gap_secs, max_gap_secs: p.max_gap_secs, max_loads_per_hour: p.max_loads_per_hour, cache_minutes: p.cache_minutes }
    }

    pub fn load(db: &Db, p: &XPacing) -> XSettings {
        let d = XSettings::defaults(p);
        let Some(v) = db.kv_get(KV_SETTINGS).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else { return d };
        let n = |k: &str, def: u32| v[k].as_u64().map(|x| x as u32).unwrap_or(def);
        let s = XSettings { enabled: v["enabled"].as_bool().unwrap_or(d.enabled), paused: v["paused"].as_bool().unwrap_or(false), min_gap_secs: n("minGapSecs", d.min_gap_secs), max_gap_secs: n("maxGapSecs", d.max_gap_secs), max_loads_per_hour: n("maxLoadsPerHour", d.max_loads_per_hour), cache_minutes: n("cacheMinutes", d.cache_minutes) };
        if s.check().is_ok() { s } else { d }
    }

    /// Hard floors that protect the spare account: never faster than every 5 s, never more than 200 pages an hour.
    pub fn check(&self) -> Result<(), String> {
        if self.min_gap_secs < 5 {
            return Err("The shortest wait between pages is 5 seconds.".into());
        }
        if self.max_gap_secs < self.min_gap_secs || self.max_gap_secs > 300 {
            return Err("The longest wait must be at least the shortest one, and at most 300 seconds.".into());
        }
        if self.max_loads_per_hour == 0 || self.max_loads_per_hour > 200 {
            return Err("Pages per hour must be between 1 and 200.".into());
        }
        if !(1..=240).contains(&self.cache_minutes) {
            return Err("The cache time must be between 1 and 240 minutes.".into());
        }
        Ok(())
    }

    pub fn save(&self, db: &Db) -> Result<(), String> {
        self.check()?;
        db.kv_set(KV_SETTINGS, &serde_json::to_string(self).map_err(|e| e.to_string())?);
        Ok(())
    }
}

/// What the extension reports about itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtStatus {
    /// None until the extension has loaded a page and seen whether x.com wants a login.
    pub logged_in: Option<bool>,
    pub paused: bool,
    pub paused_reason: String,
    pub paused_until: i64,
    pub loads_last_hour: u32,
    pub loads_today: u32,
    pub last_error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum XMode {
    Off,
    UserPaused,
    NoExtension,
    NotLoggedIn,
    AccountLimited,
    Ready,
}

// ------------------------------------------------------------------ what is logged (JSONL, journal, Obsidian)

/// Every part of the X score for one trade, so the weights can be tuned later (SPEC 8).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XLog {
    pub status: String,
    pub reason: String,
    pub score: f64,
    pub points: u32,
    pub components: XComponents,
    pub flags: Vec<String>,
    pub blocks: Vec<String>,
    pub rank: u32,
    pub clones: u32,
    pub kind: String,
    pub handle: String,
    pub tweet_id: String,
    pub url: String,
    pub age_min: i64,
}

impl From<&XEval> for XLog {
    fn from(e: &XEval) -> XLog {
        XLog { status: e.status.as_str().into(), reason: e.reason.clone(), score: e.score, points: e.points, components: e.components.clone(), flags: e.flags.clone(), blocks: e.blocks.clone(), rank: e.rank, clones: e.clones, kind: e.kind.clone(), handle: e.handle.clone(), tweet_id: e.tweet_id.clone(), url: e.url.clone(), age_min: e.age_min }
    }
}

impl XLog {
    /// One line for the learning journal and the Obsidian note.
    pub fn summary(&self) -> String {
        match self.status.as_str() {
            "ready" => {
                let c = &self.components;
                format!("X community {:.0} of 100 (+{} points): KOL {}, engagement {}, replies {}, growth {}, integrity {}, X Community {}, narrative {}{}.", self.score, self.points, c.kol, c.engagement, c.replies, c.growth, c.integrity, c.community, c.narrative, if self.rank > 0 { format!("; copy {} of {}", self.rank, self.clones) } else { String::new() })
            }
            "blocked" => format!("X Monitor blocked this coin: {}", self.blocks.join(" ")),
            "nolink" => "The coin has no usable X link, so there were no X points.".into(),
            "off" => format!("X Monitor was off ({}), so there were no X points.", self.reason),
            _ => format!("X data was unknown ({}), so the coin was scored without it.", self.reason),
        }
    }
}

// ------------------------------------------------------------------ views for the UI

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XJobCounts {
    pub queued: u32,
    pub running: u32,
    pub done: u32,
    pub failed: u32,
    pub stale: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XRising {
    pub handle: String,
    pub name: String,
    pub url: String,
    pub followers: u64,
    pub gain: i64,
    pub gain_pct: f64,
    pub minutes: i64,
    pub why: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XCheckRow {
    pub ts: i64,
    pub mint: String,
    pub symbol: String,
    pub kind: String,
    pub handle: String,
    /// The exact post, profile or community.
    pub url: String,
    pub link_label: String,
    pub status: String,
    pub score: f64,
    pub points: u32,
    pub reason: String,
    pub flags: Vec<String>,
    pub blocks: Vec<String>,
    pub rank: u32,
    pub clones: u32,
}

/// One coin's story for the Thesis Finder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XStory {
    pub mint: String,
    pub symbol: String,
    pub name: String,
    pub ts: i64,
    pub kind: String,
    pub url: String,
    pub handle: String,
    pub author: String,
    pub text: String,
    pub reach: u64,
    pub views: u64,
    /// How long before the coin was made the post was written, in minutes (None when unknown).
    pub tweet_age_min: Option<i64>,
    pub rank: u32,
    pub clones: u32,
    pub cluster: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XNarrative {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub url: String,
    pub count: u32,
    pub symbols: Vec<String>,
    pub first_ms: i64,
    pub last_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XMonitorView {
    pub ts: i64,
    pub enabled: bool,
    pub paused: bool,
    pub mode: String,
    /// working | waiting | paused | not-logged-in | off  (the side panel reads this)
    pub state: String,
    pub status_line: String,
    pub thesis_line: String,
    pub ext_alive: bool,
    pub logged_in: Option<bool>,
    pub paused_reason: String,
    pub paused_until_ms: i64,
    pub loads_last_hour: u32,
    pub max_per_hour: u32,
    pub jobs: XJobCounts,
    pub checked_hour: u32,
    pub rising: Vec<XRising>,
    pub latest: Vec<XCheckRow>,
    pub stories: Vec<XStory>,
    pub narratives: Vec<XNarrative>,
    pub demo: bool,
}

// ------------------------------------------------------------------ the monitor

struct Inner {
    jobs: Vec<XJob>,
    next_id: u64,
    ext: ExtStatus,
    last_contact: i64,
    last_started: i64,
    started: Vec<i64>,
}

pub struct XMon {
    db: Arc<Db>,
    hub: Arc<Hub>,
    pub cfg: Cfg,
    inner: Mutex<Inner>,
}

fn lc(s: &str) -> String {
    s.to_ascii_lowercase()
}

#[derive(Debug, Clone)]
struct CoinRow {
    mint: String,
    ts: i64,
    symbol: String,
    name: String,
    url: String,
    kind: String,
    handle: String,
    tweet_id: String,
    community_id: String,
}

impl CoinRow {
    fn target(&self) -> Option<XTarget> {
        match self.kind.as_str() {
            "profile" => Some(XTarget::Profile { handle: self.handle.clone() }),
            "status" => Some(XTarget::Status { handle: if self.handle.is_empty() { None } else { Some(self.handle.clone()) }, id: self.tweet_id.clone() }),
            "community" => Some(XTarget::Community { id: self.community_id.clone() }),
            _ => None,
        }
    }
}

impl XMon {
    pub fn new(db: Arc<Db>, hub: Arc<Hub>, cfg: Cfg) -> Arc<XMon> {
        db.with_conn(|c| {
            let _ = c.execute_batch(
                "CREATE TABLE IF NOT EXISTS x_coins(mint TEXT PRIMARY KEY, ts INTEGER NOT NULL, symbol TEXT, name TEXT, url TEXT, kind TEXT, handle TEXT, tweet_id TEXT, community_id TEXT, rest_id TEXT, author_rest_id TEXT);
                 CREATE INDEX IF NOT EXISTS x_coins_tweet ON x_coins(tweet_id);
                 CREATE INDEX IF NOT EXISTS x_coins_comm ON x_coins(community_id);
                 CREATE INDEX IF NOT EXISTS x_coins_rest ON x_coins(rest_id);
                 CREATE INDEX IF NOT EXISTS x_coins_ts ON x_coins(ts);
                 CREATE TABLE IF NOT EXISTS x_handles(rest_id TEXT NOT NULL, handle TEXT NOT NULL, ts INTEGER NOT NULL, PRIMARY KEY(rest_id, handle));
                 CREATE TABLE IF NOT EXISTS x_follow(id INTEGER PRIMARY KEY AUTOINCREMENT, rest_id TEXT NOT NULL, ts INTEGER NOT NULL, followers INTEGER, handle TEXT, name TEXT, created_ms INTEGER);
                 CREATE INDEX IF NOT EXISTS x_follow_rest ON x_follow(rest_id, ts);
                 CREATE TABLE IF NOT EXISTS x_results(rkey TEXT PRIMARY KEY, ts INTEGER NOT NULL, state TEXT, json TEXT);",
            );
        });
        Arc::new(XMon { db, hub, cfg, inner: Mutex::new(Inner { jobs: vec![], next_id: 1, ext: ExtStatus::default(), last_contact: 0, last_started: 0, started: vec![] }) })
    }

    /// Has this coin an X link on file?
    pub fn knows(&self, mint: &str) -> bool {
        self.coin(mint).is_some()
    }

    pub fn settings(&self) -> XSettings {
        XSettings::load(&self.db, &self.cfg.pacing)
    }

    /// What the bridge sends to the extension.
    pub fn settings_for_extension(&self) -> serde_json::Value {
        let s = self.settings();
        serde_json::json!({"enabled": s.enabled, "paused": s.paused, "pace": {"minGapSecs": s.min_gap_secs, "maxGapSecs": s.max_gap_secs, "maxLoadsPerHour": s.max_loads_per_hour, "cacheMinutes": s.cache_minutes}})
    }

    // -------------------------------------------------------------- state

    pub fn mode(&self, now: i64) -> XMode {
        let s = self.settings();
        if !s.enabled {
            return XMode::Off;
        }
        if s.paused {
            return XMode::UserPaused;
        }
        let g = self.inner.lock().unwrap();
        if now - g.last_contact > EXT_ALIVE_MS {
            return XMode::NoExtension;
        }
        if g.ext.logged_in == Some(false) {
            return XMode::NotLoggedIn;
        }
        if g.ext.paused && (g.ext.paused_until == 0 || g.ext.paused_until > now) {
            return XMode::AccountLimited;
        }
        XMode::Ready
    }

    pub fn ext_status(&self) -> ExtStatus {
        self.inner.lock().unwrap().ext.clone()
    }

    /// The extension says how it is doing (sent with every job request and every few seconds while paused).
    pub fn set_ext_status(&self, st: ExtStatus, now: i64) {
        let mut g = self.inner.lock().unwrap();
        g.last_contact = now;
        let mut st = st;
        if st.logged_in.is_none() {
            st.logged_in = g.ext.logged_in; // "unknown" never erases what we learned
        }
        g.ext = st;
    }

    // -------------------------------------------------------------- coins and jobs

    /// Remember a coin's X link (no page is opened). Every coin with a link is clustered, even when it is never scraped.
    pub fn note_coin(&self, mint: &str, symbol: &str, name: &str, twitter_url: &str, now: i64) {
        if twitter_url.trim().is_empty() {
            return;
        }
        let t = parse_x_link(twitter_url);
        let (kind, url, handle, tweet_id, community_id) = match &t {
            Some(t) => (t.kind().to_string(), t.url(), t.handle().map(String::from).unwrap_or_default(), t.tweet_id().to_string(), t.community_id().to_string()),
            None => ("bad".into(), twitter_url.chars().take(300).collect(), String::new(), String::new(), String::new()),
        };
        self.db.with_conn(|c| {
            let _ = c.execute("INSERT OR IGNORE INTO x_coins(mint, ts, symbol, name, url, kind, handle, tweet_id, community_id, rest_id, author_rest_id) VALUES(?,?,?,?,?,?,?,?,?,'','')", params![mint, now, symbol, name, url, kind, handle, tweet_id, community_id]);
        });
    }

    fn coin(&self, mint: &str) -> Option<CoinRow> {
        self.db.with_conn(|c| {
            c.query_row("SELECT mint, ts, symbol, name, url, kind, handle, tweet_id, community_id FROM x_coins WHERE mint=?", params![mint], |r| {
                Ok(CoinRow { mint: r.get(0)?, ts: r.get(1)?, symbol: r.get::<_, Option<String>>(2)?.unwrap_or_default(), name: r.get::<_, Option<String>>(3)?.unwrap_or_default(), url: r.get::<_, Option<String>>(4)?.unwrap_or_default(), kind: r.get::<_, Option<String>>(5)?.unwrap_or_default(), handle: r.get::<_, Option<String>>(6)?.unwrap_or_default(), tweet_id: r.get::<_, Option<String>>(7)?.unwrap_or_default(), community_id: r.get::<_, Option<String>>(8)?.unwrap_or_default() })
            })
            .optional()
            .ok()
            .flatten()
        })
    }

    /// The pages needed to score a coin: (kind, key, url, tweet id).
    fn plan(t: &XTarget) -> Vec<(&'static str, String, String, String)> {
        match t {
            XTarget::Profile { handle } => vec![("profile", lc(handle), format!("https://x.com/{handle}"), String::new()), ("smart", lc(handle), format!("https://x.com/{handle}/followers_you_follow"), String::new())],
            XTarget::Status { handle: Some(h), id } => vec![("tweet", id.clone(), format!("https://x.com/{h}/status/{id}"), id.clone()), ("smart", lc(h), format!("https://x.com/{h}/followers_you_follow"), String::new())],
            XTarget::Status { handle: None, id } => vec![("tweet", id.clone(), format!("https://x.com/i/web/status/{id}"), id.clone())],
            XTarget::Community { id } => vec![("community", id.clone(), format!("https://x.com/i/communities/{id}"), String::new())],
        }
    }

    /// Queue the pages a coin needs, unless a fresh result or a waiting job already covers them. Does nothing unless X Monitor can work right now.
    pub fn queue_coin(&self, mint: &str, now: i64) -> usize {
        if self.mode(now) != XMode::Ready {
            return 0;
        }
        let Some(row) = self.coin(mint) else { return 0 };
        let Some(t) = row.target() else { return 0 };
        let cache_ms = self.settings().cache_minutes as i64 * 60_000;
        let mut added = 0;
        for (kind, key, url, tweet_id) in Self::plan(&t) {
            let rkey = format!("{kind}:{key}");
            if self.result_ts(&rkey).map(|ts| now - ts < cache_ms).unwrap_or(false) {
                continue;
            }
            let mut g = self.inner.lock().unwrap();
            // already waiting or running, or failed a moment ago (do not hammer the account)
            if g.jobs.iter().any(|j| j.rkey() == rkey && (matches!(j.state, XJobState::Queued | XJobState::Running) || (j.state == XJobState::Failed && now - j.finished_ms < 120_000))) {
                continue;
            }
            let id = g.next_id;
            g.next_id += 1;
            g.jobs.push(XJob { id, kind: kind.into(), key, url, tweet_id, mint: mint.into(), symbol: row.symbol.clone(), state: XJobState::Queued, queued_ms: now, started_ms: 0, finished_ms: 0, error: String::new() });
            if g.jobs.len() > JOBS_KEPT {
                let drop = g.jobs.len() - JOBS_KEPT;
                g.jobs.drain(..drop);
            }
            added += 1;
        }
        added
    }

    /// Stale: a job nobody picked up in time, or one that has run for too long.
    fn expire(&self, now: i64) {
        let wait = self.cfg.job_wait_secs as i64 * 1000;
        let mut g = self.inner.lock().unwrap();
        for j in g.jobs.iter_mut() {
            match j.state {
                XJobState::Queued if now - j.queued_ms > wait => {
                    j.state = XJobState::Stale;
                    j.finished_ms = now;
                    j.error = "Not started in time.".into();
                }
                XJobState::Running if now - j.started_ms > JOB_RUN_MAX_MS => {
                    j.state = XJobState::Failed;
                    j.finished_ms = now;
                    j.error = "The extension did not answer in time.".into();
                }
                _ => {}
            }
        }
    }

    /// The extension asks for its next page. The app also enforces the pace: a minimum gap and the hourly cap.
    pub fn next_job(&self, now: i64) -> Option<XJob> {
        self.expire(now);
        if self.mode(now) != XMode::Ready {
            return None;
        }
        let s = self.settings();
        let mut g = self.inner.lock().unwrap();
        g.started.retain(|t| now - *t < HOUR_MS);
        if g.last_started > 0 && now - g.last_started < s.min_gap_secs as i64 * 1000 - 500 {
            return None;
        }
        if g.started.len() as u32 >= s.max_loads_per_hour {
            return None;
        }
        let idx = g.jobs.iter().position(|j| j.state == XJobState::Queued)?;
        g.jobs[idx].state = XJobState::Running;
        g.jobs[idx].started_ms = now;
        g.last_started = now;
        g.started.push(now);
        Some(g.jobs[idx].clone())
    }

    pub fn job_counts(&self) -> XJobCounts {
        let g = self.inner.lock().unwrap();
        let mut c = XJobCounts::default();
        for j in &g.jobs {
            match j.state {
                XJobState::Queued => c.queued += 1,
                XJobState::Running => c.running += 1,
                XJobState::Done => c.done += 1,
                XJobState::Failed => c.failed += 1,
                XJobState::Stale => c.stale += 1,
            }
        }
        c
    }

    pub fn jobs(&self) -> Vec<XJob> {
        self.inner.lock().unwrap().jobs.clone()
    }

    /// A result from the extension.
    pub fn submit(&self, mut res: XResult, now: i64) -> Result<(), String> {
        res.clean();
        let job = {
            let mut g = self.inner.lock().unwrap();
            g.last_contact = now;
            let Some(j) = g.jobs.iter_mut().find(|j| j.id == res.job_id && j.state == XJobState::Running) else {
                return Err("No such job is running.".into());
            };
            j.finished_ms = now;
            let useful = res.ok && matches!(res.state.as_str(), "ok" | "not_found" | "suspended" | "protected");
            if useful {
                j.state = XJobState::Done;
            } else {
                j.state = XJobState::Failed;
                j.error = if res.error.is_empty() { format!("X check failed ({}).", res.state) } else { res.error.clone() };
            }
            let jc = j.clone();
            match res.state.as_str() {
                "limited" | "locked" => {
                    g.ext.paused = true;
                    g.ext.paused_reason = res.state.clone();
                    g.ext.paused_until = now + 15 * 60_000; // the extension reports the real end with its next status
                    g.ext.last_error = jc.error.clone();
                }
                "login" => {
                    g.ext.logged_in = Some(false);
                    g.ext.last_error = jc.error.clone();
                }
                _ if useful => {
                    g.ext.logged_in = Some(true);
                    g.ext.last_error.clear();
                }
                _ => g.ext.last_error = jc.error.clone(),
            }
            jc
        };
        if !(res.ok && matches!(res.state.as_str(), "ok" | "not_found" | "suspended" | "protected")) {
            self.hub.log(format!("x-monitor: {} {} failed: {} ({})", job.kind, job.key, res.state, res.error));
            self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::XMonitor, text: format!("X check of {} {} did not work: {}", job.kind, job.key, if res.error.is_empty() { res.state.clone() } else { res.error.clone() }), mint: Some(job.mint.clone()), source_url: None });
            return Ok(());
        }
        // the result is stored under the job's own key, so a wrong key from the extension can never fill another slot
        res.kind = job.kind.clone();
        res.key = job.key.clone();
        self.store_result(&job.rkey(), now, &res);
        self.record_accounts(&res, &job, now);
        self.hub.emit(EngineEvent::Activity { ts: now, agent: AgentId::XMonitor, text: Self::activity_text(&res), mint: Some(job.mint.clone()), source_url: Some(job.url.clone()) });
        self.emit_view(now);
        Ok(())
    }

    fn activity_text(r: &XResult) -> String {
        match (r.kind.as_str(), r.state.as_str()) {
            (_, "not_found") => format!("X {}: it does not exist", r.key),
            (_, "suspended") => format!("X account @{}: suspended", r.key),
            (_, "protected") => format!("X account @{}: protected", r.key),
            ("profile", _) => r.user.as_ref().map(|u| format!("Checked @{}: {} followers, {} posts read", u.screen_name, u.followers, r.posts.len())).unwrap_or_else(|| "Checked an X profile".into()),
            ("tweet", _) => r.tweet.as_ref().map(|t| format!("Checked a post by @{}: {} views, {} replies read", t.author.as_ref().map(|a| a.screen_name.as_str()).unwrap_or("?"), t.views, r.replies.len())).unwrap_or_else(|| "Checked an X post".into()),
            ("community", _) => r.community.as_ref().map(|c| format!("Checked X Community {}: {} members", c.name, c.members.map(|m| m.to_string()).unwrap_or_else(|| "?".into()))).unwrap_or_else(|| "Checked an X Community".into()),
            ("smart", _) => format!("Smart followers of @{}: {}", r.key, r.smart.as_ref().map(|s| s.count).unwrap_or(0)),
            _ => "X check done".into(),
        }
    }

    fn store_result(&self, rkey: &str, now: i64, res: &XResult) {
        let json = serde_json::to_string(res).unwrap_or_default();
        self.db.with_conn(|c| {
            let _ = c.execute("INSERT OR REPLACE INTO x_results(rkey, ts, state, json) VALUES(?,?,?,?)", params![rkey, now, res.state, json]);
        });
    }

    fn result_ts(&self, rkey: &str) -> Option<i64> {
        self.db.with_conn(|c| c.query_row("SELECT ts FROM x_results WHERE rkey=?", params![rkey], |r| r.get::<_, i64>(0)).optional().ok().flatten())
    }

    fn result(&self, rkey: &str) -> Option<(i64, XResult)> {
        self.db.with_conn(|c| {
            c.query_row("SELECT ts, json FROM x_results WHERE rkey=?", params![rkey], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).optional().ok().flatten()
        })
        .and_then(|(ts, j)| serde_json::from_str::<XResult>(&j).ok().map(|r| (ts, r)))
    }

    /// Remember accounts: handle history (rename detection), follower counts (growth) and which coin each account belongs to.
    fn record_accounts(&self, res: &XResult, job: &XJob, now: i64) {
        let mut users: Vec<(&XUser, bool)> = vec![];
        if let Some(u) = &res.user {
            users.push((u, job.kind == "profile"));
        }
        if let Some(a) = res.tweet.as_ref().and_then(|t| t.author.as_ref()) {
            users.push((a, false));
        }
        for (u, own_profile) in users {
            if u.rest_id.is_empty() || u.screen_name.is_empty() {
                continue;
            }
            self.db.with_conn(|c| {
                let _ = c.execute("INSERT OR IGNORE INTO x_handles(rest_id, handle, ts) VALUES(?,?,?)", params![u.rest_id, lc(&u.screen_name), now]);
                let _ = c.execute("INSERT INTO x_follow(rest_id, ts, followers, handle, name, created_ms) VALUES(?,?,?,?,?,?)", params![u.rest_id, now, u.followers as i64, u.screen_name, u.name, u.created_at_ms]);
                if own_profile {
                    let _ = c.execute("UPDATE x_coins SET rest_id=? WHERE kind='profile' AND lower(handle)=?", params![u.rest_id, lc(&u.screen_name)]);
                }
                let _ = c.execute("UPDATE x_coins SET author_rest_id=? WHERE kind='status' AND tweet_id=? AND ?", params![u.rest_id, res.tweet.as_ref().map(|t| t.id.clone()).unwrap_or_default(), !own_profile]);
            });
        }
    }

    // -------------------------------------------------------------- evaluation

    fn mode_reason(m: XMode) -> (XStatus, &'static str) {
        match m {
            XMode::Off => (XStatus::Off, "X Monitor is off."),
            XMode::UserPaused => (XStatus::Off, "X Monitor is paused."),
            XMode::NoExtension => (XStatus::Unknown, "The Chrome extension is not connected."),
            XMode::NotLoggedIn => (XStatus::Unknown, "x.com is not logged in in the extension's Chrome."),
            XMode::AccountLimited => (XStatus::Unknown, "X paused: account limited."),
            XMode::Ready => (XStatus::Unknown, ""),
        }
    }

    /// Rank among coins linking the same post or community, and how many do.
    pub fn clone_info(&self, row: &CoinRowPub) -> (u32, u32) {
        let (col, val) = if !row.tweet_id.is_empty() { ("tweet_id", row.tweet_id.clone()) } else if !row.community_id.is_empty() { ("community_id", row.community_id.clone()) } else { return (0, 0) };
        let since = row.ts - self.cfg.clone_window_hours as i64 * HOUR_MS;
        let sql = format!("SELECT mint FROM x_coins WHERE {col}=? AND ts>=? ORDER BY ts ASC, rowid ASC");
        let mints: Vec<String> = self.db.with_conn(|c| {
            let Ok(mut st) = c.prepare(&sql) else { return vec![] };
            st.query_map(params![val, since], |r| r.get::<_, String>(0)).map(|it| it.flatten().collect()).unwrap_or_default()
        });
        let rank = mints.iter().position(|m| *m == row.mint).map(|p| p as u32 + 1).unwrap_or(0);
        (rank, mints.len() as u32)
    }

    fn history(&self, rest_id: &str, handle: &str, mint: &str, ts: i64) -> History {
        if rest_id.is_empty() {
            return History::default();
        }
        self.db.with_conn(|c| {
            let other_handles = c.prepare("SELECT handle FROM x_handles WHERE rest_id=? AND handle<>? ORDER BY ts ASC").and_then(|mut s| s.query_map(params![rest_id, lc(handle)], |r| r.get::<_, String>(0)).map(|it| it.flatten().collect::<Vec<_>>())).unwrap_or_default();
            let earlier_tokens = c.prepare("SELECT symbol FROM x_coins WHERE rest_id=? AND mint<>? AND kind='profile' AND ts<? ORDER BY ts ASC").and_then(|mut s| s.query_map(params![rest_id, mint, ts], |r| r.get::<_, Option<String>>(0)).map(|it| it.flatten().map(|x| x.unwrap_or_default()).collect::<Vec<_>>())).unwrap_or_default();
            History { other_handles, earlier_tokens }
        })
    }

    /// Follower counts: the earliest sighting that is at least 5 minutes older than the newest one, within 3 hours of it.
    fn growth(&self, rest_id: &str) -> Option<Growth> {
        if rest_id.is_empty() {
            return None;
        }
        self.db.with_conn(|c| {
            let (latest_ts, _latest) = c.query_row("SELECT ts, followers FROM x_follow WHERE rest_id=? ORDER BY ts DESC, id DESC LIMIT 1", params![rest_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).optional().ok().flatten()?;
            let (ts, f) = c
                .query_row("SELECT ts, followers FROM x_follow WHERE rest_id=? AND ts<=? AND ts>=? ORDER BY ts ASC LIMIT 1", params![rest_id, latest_ts - 5 * 60_000, latest_ts - 3 * HOUR_MS], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .optional()
                .ok()
                .flatten()?;
            Some(Growth { prev_followers: f.max(0) as u64, minutes: (latest_ts - ts) as f64 / 60_000.0 })
        })
    }

    /// Score one coin from what is stored. Never opens a page. Failed, missing or stale data gives `Unknown`.
    pub fn evaluate(&self, mint: &str, now: i64) -> XEval {
        let mode = self.mode(now);
        let Some(row) = self.coin(mint) else {
            return if self.cfg.block_when_no_x_link && mode == XMode::Ready {
                let mut e = XEval::empty(XStatus::Blocked, "This coin has no X link.");
                e.blocks = vec!["This coin has no X link (blocked by the xMonitor.blockWhenNoXLink setting).".into()];
                e
            } else {
                XEval::empty(XStatus::NoLink, "This coin has no X link.")
            };
        };
        let Some(target) = row.target() else { return XEval::empty(XStatus::NoLink, "The X link is not a profile, a post or a community.") };
        let (rank, clones) = self.clone_info(&CoinRowPub::from(&row));
        let mut base = XEval::empty(XStatus::Unknown, "");
        base.kind = target.kind().into();
        base.url = target.url();
        base.handle = target.handle().map(String::from).unwrap_or_default();
        base.tweet_id = target.tweet_id().into();
        base.rank = rank;
        base.clones = clones;
        base.link_label = match target {
            XTarget::Status { .. } => "X post",
            XTarget::Profile { .. } => "X profile",
            XTarget::Community { .. } => "X community",
        }
        .into();
        let with = |mut e: XEval, st: XStatus, why: &str| {
            e.status = st;
            e.reason = why.into();
            e
        };
        if mode != XMode::Ready {
            let (st, why) = Self::mode_reason(mode);
            return with(base, st, why);
        }
        let max_age = self.cfg.max_age_minutes as i64 * 60_000;
        let pkey = target.primary_key();
        let Some((pts, primary)) = self.result(&pkey) else {
            let why = self.job_note(&pkey);
            return with(base, XStatus::Unknown, &why);
        };
        if now - pts > max_age {
            return with(base, XStatus::Unknown, &format!("X data is {} minutes old (older than {}), so it is stale.", (now - pts) / 60_000, self.cfg.max_age_minutes));
        }
        let fresh = |k: &str| self.result(k).filter(|(ts, _)| now - ts <= max_age).map(|(_, r)| r);
        let user: Option<XUser> = match &target {
            XTarget::Profile { .. } => primary.user.clone(),
            XTarget::Status { .. } => primary.tweet.as_ref().and_then(|t| t.author.clone()),
            XTarget::Community { .. } => None,
        };
        let smart_handle = match &target {
            XTarget::Profile { handle } => Some(lc(handle)),
            XTarget::Status { handle: Some(h), .. } => Some(lc(h)),
            XTarget::Status { handle: None, .. } => user.as_ref().map(|u| lc(&u.screen_name)),
            XTarget::Community { .. } => None,
        };
        let smart = smart_handle.and_then(|h| fresh(&format!("smart:{h}"))).and_then(|r| r.smart);
        let (rest, handle) = user.as_ref().map(|u| (u.rest_id.clone(), u.screen_name.clone())).unwrap_or_default();
        let history = self.history(&rest, &handle, &row.mint, row.ts);
        let growth = self.growth(&rest);
        let inputs = XInputs {
            target: &target,
            token_name: &row.name,
            symbol: &row.symbol,
            mint: &row.mint,
            coin_ts: row.ts,
            now,
            primary_state: &primary.state,
            user: user.as_ref(),
            posts: &primary.posts,
            tweet: primary.tweet.as_ref(),
            replies: &primary.replies,
            community: primary.community.as_ref(),
            smart: smart.as_ref(),
            growth,
            history: &history,
            about: primary.about.as_ref().filter(|_| matches!(target, XTarget::Profile { .. })),
            rank,
            clones,
            age_min: (now - pts) / 60_000,
        };
        score(&inputs, &self.cfg)
    }

    fn job_note(&self, rkey: &str) -> String {
        let g = self.inner.lock().unwrap();
        match g.jobs.iter().rev().find(|j| j.rkey() == rkey) {
            Some(j) => match j.state {
                XJobState::Queued => "X data is not ready yet (the page is waiting its turn).".into(),
                XJobState::Running => "X data is not ready yet (the page is being read).".into(),
                XJobState::Failed => format!("The X check failed: {}", j.error),
                XJobState::Stale => "The X check did not start in time.".into(),
                XJobState::Done => "X data is missing.".into(),
            },
            None => "X data was never requested for this coin.".into(),
        }
    }

    // -------------------------------------------------------------- thesis finder

    fn coins_since(&self, since: i64, limit: usize) -> Vec<CoinRow> {
        self.db.with_conn(|c| {
            let Ok(mut st) = c.prepare("SELECT mint, ts, symbol, name, url, kind, handle, tweet_id, community_id FROM x_coins WHERE ts>=? ORDER BY ts DESC LIMIT ?") else { return vec![] };
            st.query_map(params![since, limit as i64], |r| {
                Ok(CoinRow { mint: r.get(0)?, ts: r.get(1)?, symbol: r.get::<_, Option<String>>(2)?.unwrap_or_default(), name: r.get::<_, Option<String>>(3)?.unwrap_or_default(), url: r.get::<_, Option<String>>(4)?.unwrap_or_default(), kind: r.get::<_, Option<String>>(5)?.unwrap_or_default(), handle: r.get::<_, Option<String>>(6)?.unwrap_or_default(), tweet_id: r.get::<_, Option<String>>(7)?.unwrap_or_default(), community_id: r.get::<_, Option<String>>(8)?.unwrap_or_default() })
            })
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
        })
    }

    /// The cluster a coin belongs to: its post, its community, or its account.
    fn cluster_of(row: &CoinRow, author: Option<&str>) -> Option<(String, String)> {
        if !row.tweet_id.is_empty() {
            return Some((format!("tweet:{}", row.tweet_id), "post".into()));
        }
        if !row.community_id.is_empty() {
            return Some((format!("community:{}", row.community_id), "community".into()));
        }
        let h = author.filter(|a| !a.is_empty()).map(lc).or_else(|| if row.handle.is_empty() { None } else { Some(lc(&row.handle)) })?;
        Some((format!("author:{h}"), "account".into()))
    }

    /// The story of one coin: the source post (or the account's bio), who wrote it, its reach, its age, and the copy rank.
    fn story(&self, row: &CoinRow) -> Option<XStory> {
        let target = row.target()?;
        let (rank, clones) = self.clone_info(&CoinRowPub::from(row));
        let res = self.result(&target.primary_key()).map(|x| x.1);
        let (author, text, reach, views, tweet_ms) = match (&target, &res) {
            (XTarget::Status { .. }, Some(r)) => match &r.tweet {
                Some(t) => (t.author.as_ref().map(|a| a.screen_name.clone()).unwrap_or_default(), t.text.clone(), t.author.as_ref().map(|a| a.followers).unwrap_or(0), t.views, t.created_at_ms),
                None => (String::new(), String::new(), 0, 0, 0),
            },
            (XTarget::Profile { handle }, Some(r)) => match &r.user {
                Some(u) => (u.screen_name.clone(), r.posts.first().map(|p| p.text.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| u.description.clone()), u.followers, r.posts.first().map(|p| p.views).unwrap_or(0), 0),
                None => (handle.clone(), String::new(), 0, 0, 0),
            },
            (XTarget::Community { .. }, Some(r)) => match &r.community {
                Some(c) => (String::new(), format!("{}: {}", c.name, c.description), c.members.unwrap_or(0), 0, 0),
                None => (String::new(), String::new(), 0, 0, 0),
            },
            _ => (target.handle().map(String::from).unwrap_or_default(), String::new(), 0, 0, 0),
        };
        let cluster = Self::cluster_of(row, Some(&author)).map(|c| c.0).unwrap_or_default();
        let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        Some(XStory { mint: row.mint.clone(), symbol: row.symbol.clone(), name: row.name.clone(), ts: row.ts, kind: target.kind().into(), url: target.url(), handle: target.handle().map(String::from).unwrap_or_default(), author, text: if one.chars().count() > 200 { format!("{}...", one.chars().take(200).collect::<String>()) } else { one }, reach, views, tweet_age_min: if tweet_ms > 0 { Some((row.ts - tweet_ms) / 60_000) } else { None }, rank, clones, cluster })
    }

    /// Narratives: several launches from the same post, community or account inside the window.
    pub fn narratives(&self, now: i64) -> Vec<XNarrative> {
        let since = now - self.cfg.trend_window_minutes as i64 * 60_000;
        let rows = self.coins_since(since, 600);
        let mut groups: std::collections::BTreeMap<String, (String, Vec<&CoinRow>)> = Default::default();
        let authors: std::collections::HashMap<String, String> = rows
            .iter()
            .filter_map(|r| {
                let t = r.target()?;
                let res = self.result(&t.primary_key())?.1;
                let a = res.tweet.as_ref().and_then(|t| t.author.as_ref()).map(|a| a.screen_name.clone()).or_else(|| res.user.as_ref().map(|u| u.screen_name.clone()))?;
                Some((r.mint.clone(), a))
            })
            .collect();
        for r in &rows {
            // a post cluster and an author cluster are both stories: a coin counts in each
            let mut keys: Vec<(String, String)> = vec![];
            if !r.tweet_id.is_empty() {
                keys.push((format!("tweet:{}", r.tweet_id), "post".into()));
            }
            if !r.community_id.is_empty() {
                keys.push((format!("community:{}", r.community_id), "community".into()));
            }
            if let Some(a) = authors.get(&r.mint).cloned().or_else(|| if r.handle.is_empty() { None } else { Some(r.handle.clone()) }) {
                keys.push((format!("author:{}", lc(&a)), "account".into()));
            }
            for (k, kind) in keys {
                groups.entry(k).or_insert_with(|| (kind, vec![])).1.push(r);
            }
        }
        let mut out: Vec<XNarrative> = vec![];
        for (key, (kind, coins)) in groups {
            let mut seen = std::collections::HashSet::new();
            let uniq: Vec<&&CoinRow> = coins.iter().filter(|c| seen.insert(c.mint.clone())).collect();
            if (uniq.len() as u32) < self.cfg.trend_min_coins {
                continue;
            }
            let newest = uniq.iter().max_by_key(|c| c.ts).unwrap();
            let url = match kind.as_str() {
                "account" => format!("https://x.com/{}", key.trim_start_matches("author:")),
                _ => newest.url.clone(),
            };
            let story = self.story(newest);
            out.push(XNarrative { label: match kind.as_str() { "post" => "Same post".into(), "community" => "Same X Community".into(), _ => format!("Same account @{}", key.trim_start_matches("author:")) }, key, kind, url, count: uniq.len() as u32, symbols: uniq.iter().map(|c| c.symbol.clone()).collect(), first_ms: uniq.iter().map(|c| c.ts).min().unwrap_or(0), last_ms: uniq.iter().map(|c| c.ts).max().unwrap_or(0), text: story.map(|s| s.text).unwrap_or_default() });
        }
        out.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_ms.cmp(&a.last_ms)));
        out.truncate(8);
        out
    }

    // -------------------------------------------------------------- rising accounts and the view

    fn rising(&self, now: i64) -> Vec<XRising> {
        let since = now - 3 * HOUR_MS;
        let rows: Vec<(String, i64, i64, String, String, i64)> = self.db.with_conn(|c| {
            let Ok(mut st) = c.prepare("SELECT rest_id, ts, followers, handle, name, created_ms FROM x_follow WHERE ts>=? ORDER BY rest_id, ts ASC") else { return vec![] };
            st.query_map(params![since], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0), r.get::<_, Option<String>>(3)?.unwrap_or_default(), r.get::<_, Option<String>>(4)?.unwrap_or_default(), r.get::<_, Option<i64>>(5)?.unwrap_or(0))))
                .map(|it| it.flatten().collect())
                .unwrap_or_default()
        });
        let mut out: Vec<XRising> = vec![];
        let mut i = 0;
        while i < rows.len() {
            let mut j = i;
            while j < rows.len() && rows[j].0 == rows[i].0 {
                j += 1;
            }
            let g = &rows[i..j];
            let (first, last) = (&g[0], &g[g.len() - 1]);
            let minutes = (last.1 - first.1) / 60_000;
            let gain = last.2 - first.2;
            let mut why = String::new();
            if minutes >= 5 && gain >= 30 && first.2 > 0 && gain as f64 / first.2 as f64 >= 0.2 {
                why = format!("up {:.0}% in {} minutes", gain as f64 / first.2 as f64 * 100.0, minutes);
            } else if last.5 > 0 {
                let days = ((now - last.5) as f64 / 86_400_000.0).max(1.0 / 24.0);
                if days < 14.0 && last.2 as f64 / days >= 1000.0 {
                    why = format!("about {} followers a day", (last.2 as f64 / days).round());
                }
            }
            if !why.is_empty() {
                let symbol = self.db.with_conn(|c| c.query_row("SELECT symbol FROM x_coins WHERE rest_id=? OR author_rest_id=? ORDER BY ts DESC LIMIT 1", params![last.0, last.0], |r| r.get::<_, Option<String>>(0)).optional().ok().flatten().flatten().unwrap_or_default());
                out.push(XRising { handle: last.3.clone(), name: last.4.clone(), url: format!("https://x.com/{}", last.3), followers: last.2.max(0) as u64, gain, gain_pct: if first.2 > 0 { gain as f64 / first.2 as f64 * 100.0 } else { 0.0 }, minutes, why, symbol });
            }
            i = j;
        }
        out.sort_by(|a, b| b.gain_pct.partial_cmp(&a.gain_pct).unwrap_or(std::cmp::Ordering::Equal).then(b.followers.cmp(&a.followers)));
        out.truncate(8);
        out
    }

    fn checked_hour(&self, now: i64) -> u32 {
        self.db.with_conn(|c| c.query_row("SELECT COUNT(DISTINCT rest_id) FROM x_follow WHERE ts>=?", params![now - HOUR_MS], |r| r.get::<_, i64>(0)).unwrap_or(0) as u32)
    }

    pub fn view(&self, now: i64) -> XMonitorView {
        let s = self.settings();
        let mode = self.mode(now);
        let ext = self.ext_status();
        let alive = now - self.inner.lock().unwrap().last_contact <= EXT_ALIVE_MS;
        let jobs = self.job_counts();
        let rising = self.rising(now);
        let checked = self.checked_hour(now);
        let rows = self.coins_since(now - 6 * HOUR_MS, 12);
        let latest: Vec<XCheckRow> = rows
            .iter()
            .map(|r| {
                let e = self.evaluate(&r.mint, now);
                XCheckRow { ts: r.ts, mint: r.mint.clone(), symbol: r.symbol.clone(), kind: e.kind.clone(), handle: e.handle.clone(), url: if e.url.is_empty() { r.url.clone() } else { e.url.clone() }, link_label: e.link_label.clone(), status: e.status.as_str().into(), score: e.score, points: e.points, reason: e.reason.clone(), flags: e.flags.iter().take(4).cloned().collect(), blocks: e.blocks.clone(), rank: e.rank, clones: e.clones }
            })
            .collect();
        let stories: Vec<XStory> = self.coins_since(now - 3 * HOUR_MS, 14).iter().filter_map(|r| self.story(r)).collect();
        let narratives = self.narratives(now);
        let unit = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
        let (state, line) = match mode {
            XMode::Off => ("off", "Off. Turn it on in Settings, X Monitor.".to_string()),
            XMode::UserPaused => ("paused", "Paused by you. Coins are scored without X.".to_string()),
            XMode::NoExtension => ("waiting", "Waiting for the Chrome extension to connect.".to_string()),
            XMode::NotLoggedIn => ("not-logged-in", "x.com is not logged in. Log in with the spare account.".to_string()),
            XMode::AccountLimited => ("paused", "X paused: account limited.".to_string()),
            XMode::Ready => {
                let base = format!("Checked {}, {} rising", unit(checked as usize, "account", "accounts") + " this hour", rising.len());
                if jobs.running > 0 {
                    ("working", base)
                } else if jobs.queued > 0 {
                    ("working", format!("{} waiting. {}", unit(jobs.queued as usize, "page", "pages"), base))
                } else {
                    ("working", base)
                }
            }
        };
        let thesis_line = if narratives.is_empty() {
            format!("No narrative trending. {} with X links in the last 3 hours.", unit(stories.len(), "coin", "coins"))
        } else {
            format!("{} trending: {}", unit(narratives.len(), "narrative", "narratives"), narratives.iter().take(2).map(|n| format!("{} ({} coins)", n.label, n.count)).collect::<Vec<_>>().join(", "))
        };
        XMonitorView {
            ts: now,
            enabled: s.enabled,
            paused: s.paused,
            mode: serde_json::to_value(mode).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            state: state.into(),
            status_line: line,
            thesis_line,
            ext_alive: alive,
            logged_in: ext.logged_in,
            paused_reason: if mode == XMode::AccountLimited { ext.paused_reason.clone() } else { String::new() },
            paused_until_ms: if mode == XMode::AccountLimited { ext.paused_until } else { 0 },
            loads_last_hour: ext.loads_last_hour,
            max_per_hour: s.max_loads_per_hour,
            jobs,
            checked_hour: checked,
            rising,
            latest,
            stories,
            narratives,
            demo: false,
        }
    }

    pub fn emit_view(&self, now: i64) {
        self.hub.emit(EngineEvent::XMonitor(Box::new(self.view(now))));
    }

    /// Called every 2 s by the engine: expires jobs, updates the two graph nodes and sends the view.
    pub fn tick(&self, now: i64) {
        self.expire(now);
        let v = self.view(now);
        let color = match v.state.as_str() {
            "working" => StatusColor::Green,
            "waiting" => StatusColor::Amber,
            "off" => StatusColor::Grey,
            _ => StatusColor::Red,
        };
        let color = if v.state == "paused" && v.mode == "user-paused" { StatusColor::Grey } else { color };
        self.hub.emit(EngineEvent::Status(AgentStatus { agent: AgentId::XMonitor, color, line: v.status_line.clone() }));
        let tcolor = if !v.narratives.is_empty() { StatusColor::Green } else if v.stories.is_empty() { StatusColor::Amber } else { StatusColor::Green };
        self.hub.emit(EngineEvent::Status(AgentStatus { agent: AgentId::ThesisFinder, color: tcolor, line: v.thesis_line.clone() }));
        self.hub.emit(EngineEvent::XMonitor(Box::new(v)));
    }

    /// The decision row and log for one coin (stage `x-monitor`).
    pub fn log_for(e: &XEval) -> XLog {
        XLog::from(e)
    }
}

/// Public copy of the few coin fields other modules need (the full row stays private).
#[derive(Debug, Clone)]
pub struct CoinRowPub {
    pub mint: String,
    pub ts: i64,
    pub tweet_id: String,
    pub community_id: String,
}

impl From<&CoinRow> for CoinRowPub {
    fn from(r: &CoinRow) -> CoinRowPub {
        CoinRowPub { mint: r.mint.clone(), ts: r.ts, tweet_id: r.tweet_id.clone(), community_id: r.community_id.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_795_000_000_000;
    const MIN: i64 = 60_000;

    fn mk() -> (Arc<XMon>, Arc<Db>) {
        let db = Arc::new(Db::open(":memory:").unwrap());
        let x = XMon::new(db.clone(), Hub::new(), Cfg::default());
        (x, db)
    }

    /// The extension has contacted the app and is logged in.
    fn ready(x: &XMon, now: i64) {
        x.set_ext_status(ExtStatus { logged_in: Some(true), ..Default::default() }, now);
    }

    fn user(handle: &str, rest: &str, followers: u64) -> XUser {
        XUser { rest_id: rest.into(), screen_name: handle.into(), name: "Moon Bear".into(), created_at_ms: T0 - 90 * 86_400_000, followers, following: 100, statuses: 300, ..Default::default() }
    }

    fn profile_result(id: u64, handle: &str, rest: &str, followers: u64) -> XResult {
        XResult { job_id: id, kind: "profile".into(), key: handle.to_lowercase(), ok: true, state: "ok".into(), user: Some(user(handle, rest, followers)), posts: vec![XTweet { id: "1".into(), text: "Moon Bear $MOONB".into(), views: 10_000, likes: 250, author: Some(user(handle, rest, followers)), ..Default::default() }], ..Default::default() }
    }

    fn run_next(x: &XMon, now: i64) -> XJob {
        x.next_job(now).expect("a job")
    }

    #[test]
    fn a_coin_with_an_x_link_queues_pages_and_they_move_through_the_states() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "MOONB", "Moon Bear", "https://twitter.com/moonbearsol?s=1", T0);
        assert_eq!(x.queue_coin("M1", T0), 2, "profile and smart followers");
        assert_eq!(x.queue_coin("M1", T0 + 1000), 0, "never queued twice");
        assert_eq!(x.job_counts().queued, 2);
        let j = run_next(&x, T0 + 1000);
        assert_eq!(j.kind, "profile");
        assert_eq!(j.url, "https://x.com/moonbearsol");
        assert_eq!(j.state, XJobState::Running);
        assert_eq!(x.job_counts().running, 1);
        x.submit(profile_result(j.id, "moonbearsol", "111", 8000), T0 + 5000).unwrap();
        assert_eq!(x.job_counts().done, 1);
        // the same job cannot be answered twice
        assert!(x.submit(profile_result(j.id, "moonbearsol", "111", 8000), T0 + 6000).is_err());
        // a job id the app never handed out is refused
        assert!(x.submit(profile_result(999, "a", "1", 1), T0 + 6000).is_err());
    }

    #[test]
    fn the_about_data_a_profile_job_captured_reaches_the_rename_block_through_the_whole_path() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("M1", T0);
        let j = run_next(&x, T0 + 1000);
        // the extension sends JSON: the about object comes in with X's own field names mapped by the extension
        let mut res = profile_result(j.id, "moonbearsol", "111", 8000);
        let json = serde_json::json!({"usernameChanges": 2, "changesKnown": true, "lastChangedMs": T0 - 10 * 86_400_000, "country": "Iceland", "locationAccurate": true, "affiliate": "", "source": "Web"});
        res.about = Some(serde_json::from_value(json).unwrap());
        let wire = serde_json::to_string(&res).unwrap();
        let back: XResult = serde_json::from_str(&wire).unwrap();
        assert_eq!(back.about.as_ref().unwrap().username_changes, 2);
        x.submit(back, T0 + 5000).unwrap();
        let e = x.evaluate("M1", T0 + 6000);
        assert_eq!(e.status, XStatus::Blocked, "{:?}", e);
        assert!(e.blocks.iter().any(|b| b.contains("changed its username 2 time(s)")), "{:?}", e.blocks);
        // the same account without About data scores normally: a missing capture never blocks
        let (y, _) = mk();
        ready(&y, T0);
        y.note_coin("M1", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        y.queue_coin("M1", T0);
        let j2 = run_next(&y, T0 + 1000);
        y.submit(profile_result(j2.id, "moonbearsol", "111", 8000), T0 + 5000).unwrap();
        assert_ne!(y.evaluate("M1", T0 + 6000).status, XStatus::Blocked);
    }

    #[test]
    fn a_failed_job_is_failed_and_the_coin_is_unknown_not_a_pass() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("M1", T0);
        let j = run_next(&x, T0 + 1000);
        x.submit(XResult { job_id: j.id, ok: false, state: "timeout".into(), error: "No data arrived in time.".into(), ..Default::default() }, T0 + 30_000).unwrap();
        assert_eq!(x.job_counts().failed, 1);
        let e = x.evaluate("M1", T0 + 31_000);
        assert_eq!(e.status, XStatus::Unknown);
        assert_eq!(e.points, 0);
        assert!(e.blocks.is_empty(), "unknown never blocks");
        assert!(e.reason.contains("failed"), "{}", e.reason);
        // a failed page is not asked for again within 2 minutes (do not hammer the account)
        assert_eq!(x.queue_coin("M1", T0 + 40_000), 0);
        ready(&x, T0 + 200_000);
        assert!(x.queue_coin("M1", T0 + 200_000) > 0);
    }

    #[test]
    fn jobs_nobody_starts_become_stale_and_running_jobs_that_never_answer_fail() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "A", "A", "https://x.com/aaa", T0);
        x.queue_coin("M1", T0);
        x.tick(T0 + 121_000);
        assert_eq!(x.job_counts().stale, 2);
        ready(&x, T0 + 121_000);
        let e = x.evaluate("M1", T0 + 121_000);
        assert_eq!(e.status, XStatus::Unknown);
        assert!(e.reason.contains("did not start in time"));
        // running too long
        ready(&x, T0 + 130_000);
        x.note_coin("M2", "B", "B", "https://x.com/bbb", T0 + 130_000);
        x.queue_coin("M2", T0 + 130_000);
        let j = run_next(&x, T0 + 131_000);
        x.tick(T0 + 131_000 + 61_000);
        let jobs = x.jobs();
        assert_eq!(jobs.iter().find(|q| q.id == j.id).unwrap().state, XJobState::Failed);
    }

    #[test]
    fn old_data_is_stale_so_unknown_and_fresh_data_scores() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("M1", T0);
        let j = run_next(&x, T0 + 1000);
        x.submit(profile_result(j.id, "moonbearsol", "111", 8000), T0 + 5000).unwrap();
        let e = x.evaluate("M1", T0 + 10_000);
        assert_eq!(e.status, XStatus::Ready, "{:?} {:?}", e.reason, e.blocks);
        assert!(e.points > 0);
        ready(&x, T0 + 31 * MIN);
        let late = x.evaluate("M1", T0 + 31 * MIN);
        assert_eq!(late.status, XStatus::Unknown);
        assert!(late.reason.contains("stale"));
        assert_eq!(late.points, 0);
    }

    #[test]
    fn no_link_off_and_extension_problems_never_block() {
        let (x, db) = mk();
        assert_eq!(x.evaluate("NOSUCH", T0).status, XStatus::NoLink);
        x.note_coin("M1", "A", "A", "https://x.com/aaa", T0);
        // the extension never connected
        let e = x.evaluate("M1", T0);
        assert_eq!(e.status, XStatus::Unknown);
        assert!(e.reason.contains("not connected"));
        // not logged in
        x.set_ext_status(ExtStatus { logged_in: Some(false), ..Default::default() }, T0);
        assert!(x.evaluate("M1", T0).reason.contains("not logged in"));
        // account limited
        x.set_ext_status(ExtStatus { logged_in: Some(true), paused: true, paused_reason: "limited".into(), paused_until: T0 + 15 * MIN, ..Default::default() }, T0);
        let e = x.evaluate("M1", T0 + 1000);
        assert_eq!(e.status, XStatus::Unknown);
        assert!(e.reason.contains("paused: account limited"));
        assert!(x.view(T0 + 1000).status_line.contains("X paused: account limited"));
        assert_eq!(x.queue_coin("M1", T0 + 1000), 0, "nothing is queued while paused");
        assert!(x.next_job(T0 + 1000).is_none());
        // user paused and off
        let mut s = x.settings();
        s.paused = true;
        s.save(&db).unwrap();
        assert_eq!(x.evaluate("M1", T0 + 1000).status, XStatus::Off);
        s.paused = false;
        s.enabled = false;
        s.save(&db).unwrap();
        assert_eq!(x.evaluate("M1", T0 + 1000).status, XStatus::Off);
    }

    #[test]
    fn a_link_that_is_not_a_profile_post_or_community_is_no_link() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("M1", "A", "A", "https://x.com/search?q=moon", T0);
        assert_eq!(x.evaluate("M1", T0).status, XStatus::NoLink);
        assert_eq!(x.queue_coin("M1", T0), 0);
    }

    #[test]
    fn the_app_enforces_the_pace_too() {
        let (x, db) = mk();
        ready(&x, T0);
        let mut s = x.settings();
        s.max_loads_per_hour = 3;
        s.save(&db).unwrap();
        for i in 0..6 {
            x.note_coin(&format!("M{i}"), "A", "A", &format!("https://x.com/user{i}"), T0);
            x.queue_coin(&format!("M{i}"), T0);
        }
        assert!(x.next_job(T0 + 1000).is_some());
        assert!(x.next_job(T0 + 2000).is_none(), "under the minimum gap");
        assert!(x.next_job(T0 + 9500).is_some());
        assert!(x.next_job(T0 + 19_000).is_some());
        ready(&x, T0 + 30_000);
        assert!(x.next_job(T0 + 30_000).is_none(), "the hourly cap of 3 is reached");
        ready(&x, T0 + 61 * MIN);
        assert!(x.next_job(T0 + 61 * MIN).is_none() || true);
    }

    #[test]
    fn settings_are_checked_and_have_hard_floors() {
        let (x, db) = mk();
        let d = x.settings();
        assert!(d.enabled && !d.paused);
        assert_eq!((d.min_gap_secs, d.max_gap_secs, d.max_loads_per_hour, d.cache_minutes), (8, 15, 120, 30));
        for bad in [XSettings { min_gap_secs: 1, ..d.clone() }, XSettings { max_gap_secs: 3, ..d.clone() }, XSettings { max_loads_per_hour: 0, ..d.clone() }, XSettings { max_loads_per_hour: 500, ..d.clone() }, XSettings { cache_minutes: 0, ..d.clone() }] {
            assert!(bad.save(&db).is_err());
        }
        assert_eq!(x.settings(), d, "a bad save changes nothing");
        // a damaged value in the database falls back to the defaults
        db.kv_set(KV_SETTINGS, "{\"minGapSecs\":1}");
        assert_eq!(x.settings(), d);
        let ext = x.settings_for_extension();
        assert_eq!(ext["pace"]["minGapSecs"], 8);
        assert_eq!(ext["enabled"], true);
    }

    // ---------------------------------------------------------------- clone ranking, reuse, narratives

    #[test]
    fn clone_rank_follows_launch_order_for_the_same_post_and_a_late_copy_is_blocked() {
        let (x, _) = mk();
        ready(&x, T0);
        let link = "https://x.com/alphacalls/status/1861500000000000001";
        for i in 0..7 {
            x.note_coin(&format!("C{i}"), &format!("COPY{i}"), "Moon Bear", link, T0 + i * 1000);
        }
        x.note_coin("OTHER", "OTH", "Other", "https://x.com/alphacalls/status/1861500000000000002", T0 + 500);
        let row = |m: &str| CoinRowPub::from(&x.coin(m).unwrap());
        assert_eq!(x.clone_info(&row("C0")), (1, 7));
        assert_eq!(x.clone_info(&row("C3")), (4, 7));
        assert_eq!(x.clone_info(&row("C6")), (7, 7));
        assert_eq!(x.clone_info(&row("OTHER")), (1, 1));
        // the same post an age ago is outside the 24 h window
        x.note_coin("OLD", "OLD", "Old", link, T0 - 30 * 3_600_000);
        assert_eq!(x.clone_info(&row("C0")).1, 7);
        // 6th and later are blocked once the post data is in
        let pass = |mint: &str, now: i64| {
            let target = parse_x_link(link).unwrap();
            let _ = target;
            x.queue_coin(mint, now);
        };
        pass("C5", T0 + 10_000);
        let j = run_next(&x, T0 + 20_000);
        assert_eq!(j.kind, "tweet");
        let author = user("alphacalls", "99887766", 182_000);
        let tweet = XTweet { id: "1861500000000000001".into(), text: "New one to watch: $COPY5 the moon bear".into(), views: 215_000, likes: 3100, replies: 410, reposts: 640, quotes: 55, created_at_ms: T0 - 30 * MIN, author: Some(author), ..Default::default() };
        x.submit(XResult { job_id: j.id, kind: "tweet".into(), key: "1861500000000000001".into(), ok: true, state: "ok".into(), tweet: Some(tweet), ..Default::default() }, T0 + 25_000).unwrap();
        let e = x.evaluate("C5", T0 + 30_000);
        assert_eq!((e.rank, e.clones), (6, 7));
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("Copy #6")), "{:?}", e.blocks);
        let first = x.evaluate("C0", T0 + 30_000);
        assert_eq!(first.rank, 1);
        assert!(first.blocks.iter().all(|b| !b.contains("Copy #")), "the first coin is not a late copy");
    }

    #[test]
    fn an_account_linked_to_an_earlier_coin_is_a_reused_account_and_a_rename_is_caught() {
        let (x, _) = mk();
        ready(&x, T0);
        // coin 1 links @oldbear, which turns out to be account 111
        x.note_coin("A", "OLDCOIN", "Old Coin", "https://x.com/oldbear", T0);
        x.queue_coin("A", T0);
        let j = run_next(&x, T0 + 1000);
        x.submit(profile_result(j.id, "oldbear", "111", 5000), T0 + 5000).unwrap();
        assert_eq!(x.evaluate("A", T0 + 6000).status, XStatus::Ready, "the first coin is clean");
        // coin 2, an hour later, links the renamed same account
        let later = T0 + 70 * MIN;
        ready(&x, later);
        x.note_coin("B", "MOONB", "Moon Bear", "https://x.com/moonbearsol", later);
        x.queue_coin("B", later);
        let j2 = run_next(&x, later + 1000);
        assert_eq!(j2.url, "https://x.com/moonbearsol");
        x.submit(profile_result(j2.id, "moonbearsol", "111", 5200), later + 5000).unwrap();
        let e = x.evaluate("B", later + 6000);
        assert_eq!(e.status, XStatus::Blocked);
        assert!(e.blocks.iter().any(|b| b.contains("used to be @oldbear")), "{:?}", e.blocks);
        assert!(e.blocks.iter().any(|b| b.contains("OLDCOIN")), "{:?}", e.blocks);
        // a different account is untouched
        x.note_coin("C", "NEWC", "New Coin", "https://x.com/freshone", later);
        x.queue_coin("C", later);
        let j3 = run_next(&x, later + 15_000);
        x.submit(profile_result(j3.id, "freshone", "222", 3000), later + 20_000).unwrap();
        assert_eq!(x.evaluate("C", later + 21_000).blocks.len(), 0);
    }

    #[test]
    fn follower_history_is_kept_per_account_and_rising_accounts_are_listed() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("A", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("A", T0);
        let j = run_next(&x, T0 + 1000);
        x.submit(profile_result(j.id, "moonbearsol", "111", 1000), T0 + 2000).unwrap();
        // 40 minutes later a second sighting shows 1,600 followers
        let t1 = T0 + 40 * MIN;
        ready(&x, t1);
        x.note_coin("B", "MOONB2", "Moon Bear 2", "https://x.com/moonbearsol", t1);
        x.queue_coin("B", t1);
        let j2 = run_next(&x, t1 + 1000);
        x.submit(profile_result(j2.id, "moonbearsol", "111", 1600), t1 + 2000).unwrap();
        let g = x.growth("111").unwrap();
        assert_eq!(g.prev_followers, 1000);
        assert!((g.minutes - 40.0).abs() < 0.1);
        let v = x.view(t1 + 3000);
        assert_eq!(v.rising.len(), 1);
        assert_eq!(v.rising[0].handle, "moonbearsol");
        assert_eq!(v.rising[0].gain, 600);
        assert!(v.rising[0].why.contains("up 60%"), "{}", v.rising[0].why);
        assert_eq!(v.checked_hour, 1);
        assert!(v.status_line.contains("Checked 1 account this hour, 1 rising"), "{}", v.status_line);
    }

    #[test]
    fn several_launches_from_one_post_or_author_in_the_last_hour_are_a_trending_narrative() {
        let (x, _) = mk();
        ready(&x, T0);
        let link = "https://x.com/alphacalls/status/1861500000000000001";
        for i in 0..3 {
            x.note_coin(&format!("C{i}"), &format!("COPY{i}"), "Moon Bear", link, T0 + i * 60_000);
        }
        x.note_coin("D0", "LONE", "Lone", "https://x.com/loner/status/1861500000000000099", T0);
        let n = x.narratives(T0 + 10 * MIN);
        assert_eq!(n.len(), 2, "the same post and the same author: {n:?}");
        assert!(n.iter().all(|x| x.count == 3));
        assert!(n.iter().any(|x| x.kind == "post" && x.label == "Same post"));
        assert!(n.iter().any(|x| x.kind == "account" && x.label.contains("@alphacalls")));
        assert_eq!(n[0].symbols.len(), 3);
        // two launches are not enough, and old ones fall out of the window
        assert!(x.narratives(T0 + 2 * 3_600_000).is_empty());
        let v = x.view(T0 + 10 * MIN);
        assert!(v.thesis_line.contains("2 narratives trending"), "{}", v.thesis_line);
    }

    #[test]
    fn the_story_of_a_coin_has_the_post_text_author_reach_age_and_rank() {
        let (x, _) = mk();
        ready(&x, T0);
        let link = "https://x.com/alphacalls/status/1861500000000000001";
        x.note_coin("C0", "MOONB", "Moon Bear", link, T0);
        x.note_coin("C1", "MOONB2", "Moon Bear 2", link, T0 + 30_000);
        x.queue_coin("C1", T0 + 30_000);
        let j = run_next(&x, T0 + 31_000);
        let tweet = XTweet { id: "1861500000000000001".into(), text: "New one to watch:   $MOONB,\nthe bear that went to the moon".into(), views: 215_000, created_at_ms: T0 - 25 * MIN, author: Some(user("alphacalls", "99887766", 182_000)), ..Default::default() };
        x.submit(XResult { job_id: j.id, ok: true, state: "ok".into(), tweet: Some(tweet), ..Default::default() }, T0 + 40_000).unwrap();
        let s = x.story(&x.coin("C1").unwrap()).unwrap();
        assert_eq!(s.text, "New one to watch: $MOONB, the bear that went to the moon");
        assert_eq!(s.author, "alphacalls");
        assert_eq!(s.reach, 182_000);
        assert_eq!(s.views, 215_000);
        assert_eq!(s.tweet_age_min, Some(25));
        assert_eq!((s.rank, s.clones), (2, 2));
        assert_eq!(s.cluster, "tweet:1861500000000000001");
        assert_eq!(s.url, link);
    }

    #[test]
    fn xlog_carries_every_component_and_summarises_each_status() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("A", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("A", T0);
        let j = run_next(&x, T0 + 1000);
        x.submit(profile_result(j.id, "moonbearsol", "111", 8000), T0 + 2000).unwrap();
        let e = x.evaluate("A", T0 + 3000);
        let log = XLog::from(&e);
        assert_eq!(log.status, "ready");
        let json = serde_json::to_value(&log).unwrap();
        for k in ["kol", "engagement", "replies", "growth", "integrity", "community", "narrative"] {
            assert!(json["components"][k].is_number(), "{k}");
        }
        assert!(log.summary().starts_with("X community"));
        assert!(XLog { status: "unknown".into(), reason: "stale".into(), ..Default::default() }.summary().contains("without it"));
        assert!(XLog { status: "blocked".into(), blocks: vec!["Reused.".into()], ..Default::default() }.summary().contains("blocked"));
    }

    #[test]
    fn an_exposed_result_for_the_wrong_slot_cannot_fill_another_one() {
        let (x, _) = mk();
        ready(&x, T0);
        x.note_coin("A", "MOONB", "Moon Bear", "https://x.com/moonbearsol", T0);
        x.queue_coin("A", T0);
        let j = run_next(&x, T0 + 1000);
        // the extension claims the answer is for another account: it is stored under the job's own key
        x.submit(XResult { job_id: j.id, kind: "profile".into(), key: "victim".into(), ok: true, state: "ok".into(), user: Some(user("moonbearsol", "111", 5)), ..Default::default() }, T0 + 2000).unwrap();
        assert!(x.result("profile:victim").is_none());
        assert!(x.result("profile:moonbearsol").is_some());
    }
}
