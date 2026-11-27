//! "Check for updates": asks the public GitHub API for the newest successful Windows build of the repository
//! and compares it with the commit this app was built from. No token, no login, read only.

use crate::net::http_client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const REPO: &str = "Drewzyy/Ps5Bot";
pub const WORKFLOW_FILE: &str = "windows-build.yml";
pub const DEFAULT_BRANCH: &str = "claude/trader-build-prompt-v2-9pmbsi";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub version: String,
    pub sha: String,
    pub short_sha: String,
    pub branch: String,
    /// Unix seconds when this build was compiled.
    pub built_at: i64,
    pub built_at_text: String,
}

pub fn build_info() -> BuildInfo {
    let sha = env!("CT_GIT_SHA").to_string();
    let built_at: i64 = env!("CT_BUILD_UNIX").parse().unwrap_or(0);
    BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        short_sha: short(&sha),
        sha,
        branch: env!("CT_GIT_BRANCH").to_string(),
        built_at,
        built_at_text: if built_at > 0 { crate::timefmt::date_local(built_at * 1000) } else { "unknown".into() },
    }
}

pub fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub sha: String,
    pub short_sha: String,
    pub url: String,
    /// "build" (a workflow run) or "release".
    pub kind: String,
    pub title: String,
    pub created_at: String,
    /// Unix seconds, 0 if unknown.
    pub created_unix: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    UpToDate,
    UpdateAvailable,
    /// This build has no commit id (not built from git), or nothing newer could be found.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub verdict: Verdict,
    pub message: String,
    pub current: BuildInfo,
    pub latest: Option<Latest>,
    pub steps: Vec<String>,
}

pub fn install_steps() -> Vec<String> {
    vec![
        "Fallback only (automatic updating failed). Press Open download page. Sign in to GitHub if it asks.".into(),
        "Scroll to Artifacts at the bottom and download claude-trader-windows. Unzip it.".into(),
        "Close Claude Trader, then run the new setup file from the zip. Your settings and trade history stay.".into(),
    ]
}

/// Core comparison, pure so it is tested without a network.
/// `relation` is GitHub's compare status of current...latest: "ahead" means latest has commits current lacks.
pub fn decide(current_sha: &str, current_built_unix: i64, latest: &Latest, relation: Option<&str>) -> Verdict {
    let known = |s: &str| s.len() >= 7 && s != "unknown" && s.chars().all(|c| c.is_ascii_hexdigit());
    if !known(current_sha) || !known(&latest.sha) {
        return Verdict::Unknown;
    }
    let (a, b) = (current_sha.to_ascii_lowercase(), latest.sha.to_ascii_lowercase());
    if a.starts_with(&b[..7.min(b.len())]) && b.starts_with(&a[..7.min(a.len())]) {
        return Verdict::UpToDate;
    }
    match relation {
        Some("ahead") | Some("diverged") => Verdict::UpdateAvailable,
        Some("behind") | Some("identical") => Verdict::UpToDate,
        _ => {
            // no compare answer: fall back to time. A build created after ours is newer.
            if latest.created_unix > 0 && current_built_unix > 0 {
                if latest.created_unix > current_built_unix { Verdict::UpdateAvailable } else { Verdict::UpToDate }
            } else {
                Verdict::Unknown
            }
        }
    }
}

pub fn message_for(v: Verdict, latest: Option<&Latest>) -> String {
    match (v, latest) {
        (Verdict::UpdateAvailable, Some(l)) => format!("Update available. A newer build ({}) is ready from {}.", l.short_sha, l.created_at.get(..10).unwrap_or(&l.created_at)),
        (Verdict::UpToDate, _) => "You have the newest build.".into(),
        (Verdict::Unknown, Some(_)) => "This copy does not know which build it came from, so it cannot compare. The newest build is shown below.".into(),
        (Verdict::Unknown, None) => "No finished build was found on GitHub yet.".into(),
        (Verdict::UpdateAvailable, None) => "Update available.".into(),
    }
}

fn parse_unix(iso: &str) -> i64 {
    // 2026-11-27T01:02:03Z
    let b = iso.as_bytes();
    if b.len() < 19 {
        return 0;
    }
    let n = |r: std::ops::Range<usize>| iso.get(r).and_then(|s| s.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm), Some(ss)) = (n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19)) else { return 0 };
    // days from civil (Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + hh * 3600 + mm * 60 + ss
}

/// Newest successful run from the workflow-runs API answer.
pub fn latest_from_runs(v: &serde_json::Value) -> Option<Latest> {
    let r = v["workflow_runs"].as_array()?.iter().find(|r| r["conclusion"] == "success")?;
    let sha = r["head_sha"].as_str()?.to_string();
    let created = r["updated_at"].as_str().or_else(|| r["created_at"].as_str()).unwrap_or("").to_string();
    Some(Latest {
        short_sha: short(&sha),
        sha,
        url: r["html_url"].as_str()?.to_string(),
        kind: "build".into(),
        title: r["display_title"].as_str().or_else(|| r["head_commit"]["message"].as_str()).unwrap_or("Windows build").lines().next().unwrap_or("").to_string(),
        created_unix: parse_unix(&created),
        created_at: created,
    })
}

/// Latest release from the releases API answer (`target_commitish` is a branch name for most releases, so the
/// tag's commit is looked up separately; if it is not a sha the comparison falls back to time).
pub fn latest_from_release(v: &serde_json::Value) -> Option<Latest> {
    let url = v["html_url"].as_str()?.to_string();
    let created = v["published_at"].as_str().or_else(|| v["created_at"].as_str()).unwrap_or("").to_string();
    let sha = v["target_commitish"].as_str().filter(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit())).unwrap_or("").to_string();
    Some(Latest { short_sha: short(&sha), sha, url, kind: "release".into(), title: v["name"].as_str().or_else(|| v["tag_name"].as_str()).unwrap_or("Release").to_string(), created_unix: parse_unix(&created), created_at: created })
}

async fn get_json(path: &str) -> Result<serde_json::Value, (u16, String)> {
    let r = http_client()
        .get(format!("https://api.github.com{path}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|_| (0, "Could not reach GitHub. Check your internet connection.".to_string()))?;
    let st = r.status().as_u16();
    if st == 403 || st == 429 {
        return Err((st, "GitHub says too many checks from this network. Try again in a few minutes.".into()));
    }
    if st == 404 {
        return Err((st, "GitHub could not find the project page. It may be private.".into()));
    }
    if !(200..300).contains(&st) {
        return Err((st, format!("GitHub answered with an unexpected code ({st}).")));
    }
    r.json().await.map_err(|_| (st, "GitHub sent an answer the app could not read.".to_string()))
}

/// Full check: newest successful Windows build on the branch, else the latest release.
pub async fn check_for_updates() -> Result<UpdateCheck, String> {
    let cur = build_info();
    let branch = if cur.branch.is_empty() || cur.branch == "HEAD" { DEFAULT_BRANCH.to_string() } else { cur.branch.clone() };
    let enc = branch.replace('/', "%2F");
    let runs = get_json(&format!("/repos/{REPO}/actions/workflows/{WORKFLOW_FILE}/runs?branch={enc}&status=success&per_page=5")).await;
    let mut latest = match &runs {
        Ok(v) => latest_from_runs(v),
        Err((_, m)) => {
            if runs.as_ref().err().map(|e| e.0) != Some(404) {
                return Err(m.clone());
            }
            None
        }
    };
    if latest.is_none() {
        if let Ok(v) = get_json(&format!("/repos/{REPO}/releases/latest")).await {
            latest = latest_from_release(&v);
        }
    }
    let Some(l) = latest else {
        return Ok(UpdateCheck { verdict: Verdict::Unknown, message: message_for(Verdict::Unknown, None), current: cur, latest: None, steps: vec![] });
    };
    let relation = if l.sha.len() >= 7 && cur.sha.len() >= 7 && cur.sha != "unknown" && !cur.sha.starts_with(&l.short_sha) {
        get_json(&format!("/repos/{REPO}/compare/{}...{}", cur.sha, l.sha)).await.ok().and_then(|v| v["status"].as_str().map(String::from))
    } else {
        None
    };
    let verdict = decide(&cur.sha, cur.built_at, &l, relation.as_deref());
    let steps = if verdict == Verdict::UpdateAvailable { install_steps() } else { vec![] };
    Ok(UpdateCheck { verdict, message: message_for(verdict, Some(&l)), current: cur, latest: Some(l), steps })
}

/// Any web link the user clicks (a coin's website, X or Telegram): http or https only, nothing that could confuse a shell.
pub fn is_safe_web_link(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    (l.starts_with("https://") || l.starts_with("http://")) && url.len() <= 500 && !url.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '^' | '`' | '|' | '\\'))
}

/// The browser may only be sent to this project's own GitHub pages.
pub fn is_allowed_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{REPO}/");
    url.starts_with(&prefix) && !url.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>' || c == '^' || c == '&' || c == '`' || c == '|' || c == '\\')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn latest(sha: &str, unix: i64) -> Latest {
        Latest { sha: sha.into(), short_sha: short(sha), url: "https://github.com/Drewzyy/Ps5Bot/actions/runs/1".into(), kind: "build".into(), title: "t".into(), created_at: "2026-11-27T01:00:00Z".into(), created_unix: unix }
    }
    const A: &str = "aaaaaaa1111111111111111111111111111111aa";
    const B: &str = "bbbbbbb2222222222222222222222222222222bb";

    #[test]
    fn same_commit_is_up_to_date_even_if_one_side_is_short() {
        assert_eq!(decide(A, 100, &latest(A, 200), None), Verdict::UpToDate);
        assert_eq!(decide(&A[..7], 100, &latest(A, 200), None), Verdict::UpToDate);
    }

    #[test]
    fn github_compare_status_decides_when_commits_differ() {
        assert_eq!(decide(A, 100, &latest(B, 50), Some("ahead")), Verdict::UpdateAvailable);
        assert_eq!(decide(A, 100, &latest(B, 999), Some("behind")), Verdict::UpToDate);
        assert_eq!(decide(A, 100, &latest(B, 999), Some("identical")), Verdict::UpToDate);
    }

    #[test]
    fn without_a_compare_answer_the_newer_time_wins() {
        assert_eq!(decide(A, 100, &latest(B, 200), None), Verdict::UpdateAvailable);
        assert_eq!(decide(A, 300, &latest(B, 200), None), Verdict::UpToDate);
        assert_eq!(decide(A, 0, &latest(B, 200), None), Verdict::Unknown);
    }

    #[test]
    fn a_build_with_no_commit_id_cannot_compare() {
        assert_eq!(decide("unknown", 100, &latest(B, 200), None), Verdict::Unknown);
        assert_eq!(decide(A, 100, &latest("", 200), Some("ahead")), Verdict::Unknown);
    }

    #[test]
    fn newest_successful_run_is_picked_from_the_api_answer() {
        let v = json!({"workflow_runs":[
            {"conclusion":"failure","head_sha":A,"html_url":"https://github.com/Drewzyy/Ps5Bot/actions/runs/3","updated_at":"2026-11-27T03:00:00Z"},
            {"conclusion":"success","head_sha":B,"html_url":"https://github.com/Drewzyy/Ps5Bot/actions/runs/2","display_title":"Add updates\nmore","updated_at":"2026-11-27T02:00:00Z"}]});
        let l = latest_from_runs(&v).unwrap();
        assert_eq!(l.sha, B);
        assert_eq!(l.title, "Add updates");
        assert_eq!(l.created_unix, 1_795_744_800); // 2026-11-27T02:00:00Z
        assert!(latest_from_runs(&json!({"workflow_runs":[]})).is_none());
        let rel = latest_from_release(&json!({"html_url":"https://github.com/Drewzyy/Ps5Bot/releases/tag/v1","tag_name":"v1","published_at":"2026-11-27T02:00:00Z","target_commitish":"main"})).unwrap();
        assert_eq!((rel.kind.as_str(), rel.sha.as_str()), ("release", ""));
    }

    #[test]
    fn messages_and_steps_are_plain_and_free_of_em_dashes() {
        let l = latest(B, 200);
        for v in [Verdict::UpToDate, Verdict::UpdateAvailable, Verdict::Unknown] {
            assert!(!message_for(v, Some(&l)).contains('\u{2014}'));
        }
        assert_eq!(install_steps().len(), 3);
        assert!(install_steps().iter().all(|s| !s.contains('\u{2014}')));
        assert!(message_for(Verdict::UpdateAvailable, Some(&l)).starts_with("Update available"));
    }

    #[test]
    fn only_this_projects_github_pages_may_be_opened() {
        assert!(is_allowed_url("https://github.com/Drewzyy/Ps5Bot/actions/runs/1"));
        assert!(!is_allowed_url("https://github.com/Evil/Ps5Bot/actions"));
        assert!(!is_allowed_url("https://github.com/Drewzyy/Ps5Bot/x y"));
        assert!(!is_allowed_url("file:///c:/windows/system32/calc.exe"));
        assert!(!is_allowed_url("https://github.com/Drewzyy/Ps5Bot/&calc"));
    }

    #[test]
    fn coin_links_must_be_plain_web_links() {
        assert!(is_safe_web_link("https://x.com/mooncat"));
        assert!(is_safe_web_link("http://moon.cat/a?b=1&c=2"));
        for bad in ["javascript:alert(1)", "file:///c:/x", "https://a.com/ b", "https://a.com/\"x", "ftp://a.com", "calc.exe", "https://a.com/|calc"] {
            assert!(!is_safe_web_link(bad), "{bad}");
        }
    }

    #[test]
    fn build_info_has_a_commit_and_time() {
        let b = build_info();
        assert!(!b.sha.is_empty() && !b.version.is_empty());
        assert!(b.built_at > 1_700_000_000);
    }
}
