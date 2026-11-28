//! X Monitor data model: the link a coin carries, the jobs the extension runs, and the structured results it sends back.
//! Everything that arrives from the extension is cleaned here (lengths, counts) before anything else reads it.

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------ the X link of a coin

/// What a coin's X link points to.
#[derive(Debug, Clone, PartialEq)]
pub enum XTarget {
    /// A post. The handle is unknown for `x.com/i/web/status/ID` links.
    Status { handle: Option<String>, id: String },
    Profile { handle: String },
    Community { id: String },
}

const RESERVED: &[&str] = &[
    "home", "i", "compose", "intent", "search", "explore", "settings", "notifications", "messages", "login", "logout", "signup", "share", "hashtag", "tos", "privacy", "about", "account", "download", "jobs", "lists", "bookmarks", "premium", "verified", "communities", "support", "help", "status", "statuses", "web", "followers", "following", "i/flow", "cashtag",
];

pub fn valid_handle(h: &str) -> bool {
    (1..=15).contains(&h.len()) && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !RESERVED.contains(&h.to_ascii_lowercase().as_str())
}

fn digits(s: &str) -> bool {
    (5..=25).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())
}

/// Read a coin's X link. twitter.com, x.com, www., mobile., http, query strings, fragments and trailing slashes are all
/// treated the same. Returns None for anything that is not a profile, a post or a community.
pub fn parse_x_link(raw: &str) -> Option<XTarget> {
    let t = raw.trim();
    if t.is_empty() || t.len() > 300 {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"))?;
    let rest_orig = &t[t.len() - rest.len()..];
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim_start_matches("www.").trim_start_matches("mobile.");
    if host != "x.com" && host != "twitter.com" {
        return None;
    }
    let path_orig = rest_orig.split_once('/').map(|x| x.1).unwrap_or("");
    let path_orig = path_orig.split(['?', '#']).next().unwrap_or("");
    let path = path.split(['?', '#']).next().unwrap_or("");
    let seg: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let seg_o: Vec<&str> = path_orig.split('/').filter(|s| !s.is_empty()).collect();
    match seg.as_slice() {
        ["i", "communities", id, ..] if digits(id) => Some(XTarget::Community { id: (*id).to_string() }),
        ["i", "web", "status", id, ..] | ["i", "status", id, ..] if digits(id) => Some(XTarget::Status { handle: None, id: (*id).to_string() }),
        [h, "status" | "statuses", id, ..] if digits(id) && valid_handle(h) => Some(XTarget::Status { handle: Some(seg_o[0].to_string()), id: (*id).to_string() }),
        [h] | [h, "with_replies" | "media" | "highlights" | "photo" | "followers" | "following", ..] if valid_handle(h) => Some(XTarget::Profile { handle: seg_o[0].to_string() }),
        _ => None,
    }
}

impl XTarget {
    pub fn kind(&self) -> &'static str {
        match self {
            XTarget::Status { .. } => "status",
            XTarget::Profile { .. } => "profile",
            XTarget::Community { .. } => "community",
        }
    }

    /// The exact post, profile or community page. Shown as a link in the app.
    pub fn url(&self) -> String {
        match self {
            XTarget::Status { handle: Some(h), id } => format!("https://x.com/{h}/status/{id}"),
            XTarget::Status { handle: None, id } => format!("https://x.com/i/web/status/{id}"),
            XTarget::Profile { handle } => format!("https://x.com/{handle}"),
            XTarget::Community { id } => format!("https://x.com/i/communities/{id}"),
        }
    }

    pub fn handle(&self) -> Option<&str> {
        match self {
            XTarget::Status { handle, .. } => handle.as_deref(),
            XTarget::Profile { handle } => Some(handle),
            XTarget::Community { .. } => None,
        }
    }

    pub fn tweet_id(&self) -> &str {
        match self {
            XTarget::Status { id, .. } => id,
            _ => "",
        }
    }

    pub fn community_id(&self) -> &str {
        match self {
            XTarget::Community { id } => id,
            _ => "",
        }
    }

    /// The result that must exist before a score can be made: `kind:key`.
    pub fn primary_key(&self) -> String {
        match self {
            XTarget::Status { id, .. } => format!("tweet:{id}"),
            XTarget::Profile { handle } => format!("profile:{}", handle.to_ascii_lowercase()),
            XTarget::Community { id } => format!("community:{id}"),
        }
    }
}

// ------------------------------------------------------------------ jobs

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum XJobState {
    Queued,
    Running,
    Done,
    Failed,
    Stale,
}

impl XJobState {
    pub fn as_str(self) -> &'static str {
        match self {
            XJobState::Queued => "queued",
            XJobState::Running => "running",
            XJobState::Done => "done",
            XJobState::Failed => "failed",
            XJobState::Stale => "stale",
        }
    }
}

/// One page the extension is asked to open. The address is built here from checked parts, never copied from the coin's metadata.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XJob {
    pub id: u64,
    /// profile | tweet | community | smart
    pub kind: String,
    /// lower-case handle, tweet id or community id
    pub key: String,
    pub url: String,
    pub tweet_id: String,
    /// The coin that asked first (for the activity line).
    pub mint: String,
    pub symbol: String,
    pub state: XJobState,
    pub queued_ms: i64,
    pub started_ms: i64,
    pub finished_ms: i64,
    pub error: String,
}

impl XJob {
    pub fn rkey(&self) -> String {
        format!("{}:{}", self.kind, self.key)
    }
}

// ------------------------------------------------------------------ results from the extension

fn cut(s: &mut String, max: usize) {
    if s.chars().count() > max {
        *s = s.chars().take(max).collect();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XUser {
    pub rest_id: String,
    pub screen_name: String,
    pub name: String,
    pub created_at_ms: i64,
    pub followers: u64,
    pub following: u64,
    pub statuses: u64,
    pub listed: u64,
    pub verified: bool,
    pub protected: bool,
    pub default_avatar: bool,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XTweet {
    pub id: String,
    pub text: String,
    pub created_at_ms: i64,
    pub views: u64,
    pub likes: u64,
    pub replies: u64,
    pub reposts: u64,
    pub quotes: u64,
    pub bookmarks: u64,
    pub is_retweet: bool,
    pub in_reply_to_id: String,
    pub community_id: String,
    pub author: Option<XUser>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XReply {
    pub id: String,
    pub text: String,
    pub likes: u64,
    pub author_rest_id: String,
    pub author_screen_name: String,
    pub author_created_at_ms: i64,
    pub author_followers: u64,
    pub default_avatar: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XCommunityInfo {
    pub id: String,
    pub name: String,
    pub members: Option<u64>,
    pub created_at_ms: i64,
    pub description: String,
    pub posts_seen: u32,
    pub latest_post_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XSmartFollower {
    pub handle: String,
    pub followers: u64,
    pub rest_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XSmart {
    pub count: u32,
    pub handles: Vec<XSmartFollower>,
    pub top_followers: u64,
    pub has_more: bool,
}

/// What the extension sends back for one job.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct XResult {
    /// The job this answers (not `id`: that field belongs to the bridge's own request numbering).
    pub job_id: u64,
    pub kind: String,
    pub key: String,
    pub ok: bool,
    /// ok | not_found | suspended | protected | limited | locked | login | timeout | drift | tab | bad_job
    pub state: String,
    pub error: String,
    /// graphql or dom
    pub source: String,
    pub drift: bool,
    pub cached: bool,
    pub user: Option<XUser>,
    pub posts: Vec<XTweet>,
    pub tweet: Option<XTweet>,
    pub replies: Vec<XReply>,
    pub community: Option<XCommunityInfo>,
    pub smart: Option<XSmart>,
}

impl XResult {
    /// Cut every text and list to a sane size. Anything the extension (or a page) sends is untrusted.
    pub fn clean(&mut self) {
        cut(&mut self.state, 20);
        cut(&mut self.error, 200);
        cut(&mut self.source, 10);
        cut(&mut self.kind, 12);
        cut(&mut self.key, 40);
        let user = |u: &mut XUser| {
            cut(&mut u.rest_id, 30);
            cut(&mut u.screen_name, 15);
            cut(&mut u.name, 80);
            cut(&mut u.description, 400);
        };
        let tweet = |t: &mut XTweet| {
            cut(&mut t.id, 30);
            cut(&mut t.text, 1500);
            cut(&mut t.in_reply_to_id, 30);
            cut(&mut t.community_id, 30);
            if let Some(a) = t.author.as_mut() {
                cut(&mut a.rest_id, 30);
                cut(&mut a.screen_name, 15);
                cut(&mut a.name, 80);
                cut(&mut a.description, 400);
            }
        };
        if let Some(u) = self.user.as_mut() {
            user(u);
        }
        self.posts.truncate(20);
        self.posts.iter_mut().for_each(tweet);
        if let Some(t) = self.tweet.as_mut() {
            tweet(t);
        }
        self.replies.truncate(60);
        for r in self.replies.iter_mut() {
            cut(&mut r.id, 30);
            cut(&mut r.text, 600);
            cut(&mut r.author_rest_id, 30);
            cut(&mut r.author_screen_name, 15);
        }
        if let Some(c) = self.community.as_mut() {
            cut(&mut c.id, 30);
            cut(&mut c.name, 100);
            cut(&mut c.description, 300);
        }
        if let Some(s) = self.smart.as_mut() {
            s.handles.truncate(40);
            s.count = s.count.min(10_000);
            for h in s.handles.iter_mut() {
                cut(&mut h.handle, 15);
                cut(&mut h.rest_id, 30);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(h: Option<&str>, id: &str) -> Option<XTarget> {
        Some(XTarget::Status { handle: h.map(String::from), id: id.into() })
    }

    #[test]
    fn reads_every_kind_of_x_link() {
        assert_eq!(parse_x_link("https://x.com/MoonBearSol"), Some(XTarget::Profile { handle: "MoonBearSol".into() }));
        assert_eq!(parse_x_link("https://twitter.com/moonbearsol/"), Some(XTarget::Profile { handle: "moonbearsol".into() }));
        assert_eq!(parse_x_link("http://www.x.com/moonbearsol?s=20&t=abc"), Some(XTarget::Profile { handle: "moonbearsol".into() }));
        assert_eq!(parse_x_link("https://mobile.twitter.com/moonbearsol#x"), Some(XTarget::Profile { handle: "moonbearsol".into() }));
        assert_eq!(parse_x_link("https://x.com/moonbearsol/with_replies"), Some(XTarget::Profile { handle: "moonbearsol".into() }));
        assert_eq!(parse_x_link("https://x.com/alphacalls/status/1861500000000000001"), st(Some("alphacalls"), "1861500000000000001"));
        assert_eq!(parse_x_link("https://twitter.com/alphacalls/status/1861500000000000001?s=46&t=zz"), st(Some("alphacalls"), "1861500000000000001"));
        assert_eq!(parse_x_link("https://x.com/alphacalls/status/1861500000000000001/photo/1"), st(Some("alphacalls"), "1861500000000000001"));
        assert_eq!(parse_x_link("https://x.com/i/web/status/1861500000000000001"), st(None, "1861500000000000001"));
        assert_eq!(parse_x_link("https://x.com/i/status/1861500000000000001"), st(None, "1861500000000000001"));
        assert_eq!(parse_x_link("https://x.com/i/communities/1861700000000000007"), Some(XTarget::Community { id: "1861700000000000007".into() }));
        assert_eq!(parse_x_link("https://x.com/i/communities/1861700000000000007/about"), Some(XTarget::Community { id: "1861700000000000007".into() }));
    }

    #[test]
    fn refuses_everything_else() {
        for bad in ["", "x.com/moonbear", "https://evil.com/moonbear", "https://x.com.evil.com/moonbear", "https://x.com/", "https://x.com/home", "https://x.com/search?q=moon", "https://x.com/hashtag/moon", "https://x.com/intent/tweet?text=hi", "https://x.com/i/flow/login", "https://x.com/moon bear", "https://x.com/waytoolonghandleover15", "https://x.com/moonbear/status/abc", "https://x.com/moonbear/status/12", "javascript:alert(1)", "https://x.com/<script>", "https://t.me/moonbear"] {
            assert_eq!(parse_x_link(bad), None, "{bad}");
        }
        assert_eq!(parse_x_link(&format!("https://x.com/{}", "a".repeat(400))), None);
    }

    #[test]
    fn canonical_urls_keys_and_parts() {
        let p = parse_x_link("https://twitter.com/MoonBearSol?x=1").unwrap();
        assert_eq!(p.url(), "https://x.com/MoonBearSol");
        assert_eq!(p.primary_key(), "profile:moonbearsol");
        assert_eq!(p.kind(), "profile");
        let s = parse_x_link("https://mobile.twitter.com/AlphaCalls/status/1861500000000000001?s=1").unwrap();
        assert_eq!(s.url(), "https://x.com/AlphaCalls/status/1861500000000000001");
        assert_eq!(s.primary_key(), "tweet:1861500000000000001");
        assert_eq!(s.tweet_id(), "1861500000000000001");
        let c = parse_x_link("https://x.com/i/communities/1861700000000000007").unwrap();
        assert_eq!(c.primary_key(), "community:1861700000000000007");
        assert_eq!(c.community_id(), "1861700000000000007");
        assert_eq!(c.handle(), None);
    }

    #[test]
    fn results_are_cleaned_and_unknown_fields_are_ignored() {
        let json = serde_json::json!({
            "jobId": 7, "kind": "tweet", "key": "1", "ok": true, "state": "ok", "extraThing": 1,
            "tweet": { "id": "1861500000000000001", "text": "x".repeat(5000), "views": 100, "author": { "screenName": "a".repeat(100), "followers": 5 } },
            "replies": (0..200).map(|i| serde_json::json!({"id": i.to_string(), "text": "y".repeat(3000)})).collect::<Vec<_>>(),
            "smart": { "count": 99999999, "handles": (0..100).map(|i| serde_json::json!({"handle": format!("h{i}")})).collect::<Vec<_>>() }
        });
        let mut r: XResult = serde_json::from_value(json).unwrap();
        r.clean();
        assert_eq!(r.tweet.as_ref().unwrap().text.chars().count(), 1500);
        assert_eq!(r.tweet.as_ref().unwrap().author.as_ref().unwrap().screen_name.chars().count(), 15);
        assert_eq!(r.replies.len(), 60);
        assert_eq!(r.replies[0].text.chars().count(), 600);
        assert_eq!(r.smart.as_ref().unwrap().handles.len(), 40);
        assert_eq!(r.smart.as_ref().unwrap().count, 10_000);
    }
}
