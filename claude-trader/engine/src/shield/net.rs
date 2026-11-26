//! Network-backed Scam Shield inputs: RugCheck report and token metadata JSON.

use super::rules::{parse_metadata_json, MetaFetch};
use crate::net::{http_client, mask};
use crate::rpc::Limiter;
use serde_json::Value;
use std::time::Duration;

pub struct RugCheck {
    http: reqwest::Client,
    limiter: Limiter,
    base: String,
}

impl RugCheck {
    /// RugCheck allows 3 requests per second.
    pub fn new() -> Self {
        RugCheck { http: http_client(), limiter: Limiter::new(3.0), base: "https://api.rugcheck.xyz".into() }
    }

    /// Full report JSON. 404 means "not indexed yet" (new token): the caller may retry.
    pub async fn report(&self, mint: &str) -> Result<Option<Value>, String> {
        self.limiter.acquire().await;
        let url = format!("{}/v1/tokens/{mint}/report", self.base);
        let r = self.http.get(&url).timeout(Duration::from_secs(2)).send().await.map_err(|e| mask(&format!("rugcheck: {e}")))?;
        match r.status().as_u16() {
            200 => r.json::<Value>().await.map(Some).map_err(|e| format!("rugcheck body: {e}")),
            // 400/404: the token is not indexed yet (seen on coins about 1 s old). Caller retries.
            400 | 404 => Ok(None),
            429 => Err("rugcheck rate limited (429)".into()),
            s => Err(format!("rugcheck http {s}")),
        }
    }
}

impl Default for RugCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch the metadata JSON (name, image, twitter, telegram, website) within 2 s.
pub async fn fetch_metadata(http: &reqwest::Client, uri: &str) -> MetaFetch {
    if !(uri.starts_with("http://") || uri.starts_with("https://")) {
        return MetaFetch::Failed("not an http link".into());
    }
    match http.get(uri).timeout(Duration::from_secs(2)).send().await {
        Ok(r) if r.status().is_success() => match r.json::<Value>().await {
            Ok(v) => parse_metadata_json(&v),
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
