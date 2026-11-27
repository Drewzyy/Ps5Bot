//! Network-backed Scam Shield inputs: RugCheck report and token metadata JSON.

use super::rules::{parse_metadata_json, parse_social_links, MetaFetch, SocialLinks};
use crate::net::{http_client, mask};
use crate::rpc::Limiter;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Waits (after the first try) before each retry of a failed RugCheck request: 3 s, 8 s, 15 s (SPEC 11b #29).
pub const RUGCHECK_RETRY_SECS: [u64; 3] = [3, 8, 15];

/// How long a good report stays in the per-mint cache.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// One shared RugCheck client for the whole app: at most 2 requests in flight, a small rate limit, a cache per mint.
/// RugCheck is free and often busy (HTTP 502), so every failure is treated as "try again later", never as a verdict.
pub struct RugCheck {
    http: reqwest::Client,
    limiter: Limiter,
    inflight: tokio::sync::Semaphore,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
    base: String,
    delays: Vec<Duration>,
    /// Tests: scripted answers used instead of the network.
    script: Option<Mutex<VecDeque<Result<Option<Value>, String>>>>,
    /// Counters for the health line.
    pub requests: AtomicU64,
    pub failures: AtomicU64,
}

impl RugCheck {
    /// RugCheck allows 3 requests per second; we use 2 and never more than 2 at once.
    pub fn new() -> Self {
        RugCheck {
            http: http_client(),
            limiter: Limiter::new(2.0),
            inflight: tokio::sync::Semaphore::new(2),
            cache: Mutex::new(HashMap::new()),
            base: "https://api.rugcheck.xyz".into(),
            delays: RUGCHECK_RETRY_SECS.iter().map(|s| Duration::from_secs(*s)).collect(),
            script: None,
            requests: AtomicU64::new(0),
            failures: AtomicU64::new(0),
        }
    }

    /// Test client: answers come from `answers` in order (then "no report"), retry waits are `delays`.
    pub fn scripted(answers: Vec<Result<Option<Value>, String>>, delays: Vec<Duration>) -> Self {
        let mut r = RugCheck::new();
        r.script = Some(Mutex::new(answers.into()));
        r.delays = delays;
        r.limiter = Limiter::new(1000.0);
        r
    }

    /// Requests made so far (scripted or real), for tests and the health line.
    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    fn cached(&self, mint: &str) -> Option<Value> {
        let mut g = self.cache.lock().unwrap();
        match g.get(mint) {
            Some((at, v)) if at.elapsed() < CACHE_TTL => Some(v.clone()),
            Some(_) => {
                g.remove(mint);
                None
            }
            None => None,
        }
    }

    fn remember(&self, mint: &str, v: &Value) {
        let mut g = self.cache.lock().unwrap();
        if g.len() > 500 {
            g.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        }
        g.insert(mint.to_string(), (Instant::now(), v.clone()));
    }

    /// ONE attempt. Cache first; then wait for a slot (max 2 in flight) and the rate limit.
    /// 400/404 means "not indexed yet" (new token): Ok(None).
    pub async fn report(&self, mint: &str) -> Result<Option<Value>, String> {
        if let Some(v) = self.cached(mint) {
            return Ok(Some(v));
        }
        let _slot = self.inflight.acquire().await.map_err(|_| "rugcheck client closed".to_string())?;
        self.limiter.acquire().await;
        self.requests.fetch_add(1, Ordering::Relaxed);
        let out = self.fetch(mint).await;
        match &out {
            Ok(Some(v)) => self.remember(mint, v),
            _ => {
                self.failures.fetch_add(1, Ordering::Relaxed);
            }
        }
        out
    }

    async fn fetch(&self, mint: &str) -> Result<Option<Value>, String> {
        if let Some(s) = &self.script {
            return s.lock().unwrap().pop_front().unwrap_or(Ok(None));
        }
        let url = format!("{}/v1/tokens/{mint}/report", self.base);
        let r = self.http.get(&url).timeout(Duration::from_secs(4)).send().await.map_err(|e| {
            if e.is_timeout() { "rugcheck timed out".to_string() } else { mask(&format!("rugcheck: {e}")) }
        })?;
        match r.status().as_u16() {
            200 => r.json::<Value>().await.map(Some).map_err(|e| format!("rugcheck body: {e}")),
            400 | 404 => Ok(None),
            429 => Err("rugcheck rate limited (429)".into()),
            s => Err(format!("rugcheck http {s}")),
        }
    }

    /// Try now, then again after each wait in the retry schedule (3 s, 8 s, 15 s). Every failure kind is retried:
    /// HTTP 502 and other server errors, network errors, timeouts, 429 and "no report yet". `progress` is told
    /// the attempt number and the last error after every failed attempt. Returns the report, or why it gave up.
    pub async fn report_with_retry(&self, mint: &str, mut progress: impl FnMut(u32, &str)) -> Result<Value, String> {
        let mut last = String::new();
        for attempt in 0..=self.delays.len() {
            match self.report(mint).await {
                Ok(Some(v)) => return Ok(v),
                Ok(None) => last = "RugCheck has no report yet".to_string(),
                Err(e) => last = e,
            }
            progress(attempt as u32 + 1, &last);
            if let Some(d) = self.delays.get(attempt) {
                tokio::time::sleep(*d).await;
            }
        }
        Err(last)
    }
}

impl Default for RugCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch the metadata JSON (name, image, twitter, telegram, website) within 2 s.
pub async fn fetch_metadata(http: &reqwest::Client, uri: &str) -> MetaFetch {
    fetch_metadata_links(http, uri).await.0
}

/// Same fetch, plus the coin's website, X and Telegram links so the UI can show where to read about it.
pub async fn fetch_metadata_links(http: &reqwest::Client, uri: &str) -> (MetaFetch, SocialLinks) {
    let mut links = SocialLinks::default();
    let m = fetch_metadata_inner(http, uri, &mut links).await;
    (m, links)
}

async fn fetch_metadata_inner(http: &reqwest::Client, uri: &str, links: &mut SocialLinks) -> MetaFetch {
    if !(uri.starts_with("http://") || uri.starts_with("https://")) {
        return MetaFetch::Failed("not an http link".into());
    }
    match http.get(uri).timeout(Duration::from_secs(2)).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(v) => {
                *links = parse_social_links(&v);
                parse_metadata_json(&v)
            }
            Err(e) => MetaFetch::Failed(format!("not json: {e}")),
        },
        Ok(r) => MetaFetch::Failed(format!("http {}", r.status().as_u16())),
        Err(e) => {
            let s = format!("{e:?}").to_lowercase();
            // The sandbox egress proxy refuses unknown hosts with a 403 on CONNECT.
            if e.is_connect() && (s.contains("403") || s.contains("proxy") || s.contains("tunnel")) {
                MetaFetch::EgressBlocked
            } else if e.is_timeout() {
                MetaFetch::Failed("timeout".into())
            } else {
                MetaFetch::Failed(mask(&e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fast(answers: Vec<Result<Option<Value>, String>>) -> RugCheck {
        RugCheck::scripted(answers, vec![Duration::from_millis(5), Duration::from_millis(10), Duration::from_millis(15)])
    }

    #[test]
    fn retry_schedule_is_3_8_15_seconds() {
        assert_eq!(RUGCHECK_RETRY_SECS, [3, 8, 15]);
        let r = RugCheck::new();
        assert_eq!(r.delays, vec![Duration::from_secs(3), Duration::from_secs(8), Duration::from_secs(15)]);
        // at most 2 requests in flight
        assert_eq!(r.inflight.available_permits(), 2);
    }

    #[tokio::test]
    async fn a_502_a_network_error_and_no_report_yet_are_retried_until_a_report_arrives() {
        let rc = fast(vec![Err("rugcheck http 502".into()), Err("rugcheck: error sending request for url (https://api.rugcheck.xyz/...)".into()), Ok(None), Ok(Some(json!({"rugged": false})))]);
        let mut seen = vec![];
        let v = rc.report_with_retry("MINT", |n, e| seen.push((n, e.to_string()))).await.unwrap();
        assert_eq!(v["rugged"], false);
        assert_eq!(rc.request_count(), 4);
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0], (1, "rugcheck http 502".to_string()));
        assert_eq!(seen[2], (3, "RugCheck has no report yet".to_string()));
    }

    #[tokio::test]
    async fn gives_up_after_the_schedule_with_the_last_error() {
        let rc = fast(vec![Err("rugcheck http 502".into()), Err("rugcheck http 502".into()), Err("rugcheck timed out".into()), Err("rugcheck http 502".into()), Err("never used".into())]);
        let e = rc.report_with_retry("MINT", |_, _| {}).await.unwrap_err();
        assert_eq!(e, "rugcheck http 502");
        assert_eq!(rc.request_count(), 4, "one try plus three retries, no more");
    }

    #[tokio::test]
    async fn a_good_report_is_cached_per_mint() {
        let rc = fast(vec![Ok(Some(json!({"rugged": false, "n": 1})))]);
        let a = rc.report_with_retry("M1", |_, _| {}).await.unwrap();
        let b = rc.report_with_retry("M1", |_, _| {}).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(rc.request_count(), 1, "second call is served from the cache");
        // another mint is a separate request (no script left: "no report yet", then it gives up)
        assert!(rc.report_with_retry("M2", |_, _| {}).await.is_err());
        assert!(rc.request_count() > 1);
    }

    #[tokio::test]
    async fn never_more_than_two_requests_run_at_once() {
        let rc = std::sync::Arc::new(fast(vec![]));
        let held = (rc.inflight.acquire().await.unwrap(), rc.inflight.acquire().await.unwrap());
        let rc2 = rc.clone();
        let t = tokio::spawn(async move { rc2.report("M").await });
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(!t.is_finished(), "a third request waits for a free slot");
        drop(held);
        assert!(t.await.unwrap().is_ok());
    }
}
