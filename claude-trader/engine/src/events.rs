use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AgentId {
    ClaudeBot,
    ScamShield,
    TokenPicker,
    Executor,
    HoldTimer,
    ThesisFinder,
    MemecoinMonitor,
    XMonitor,
    TopTraderMonitor,
    TopGainers,
}

impl AgentId {
    pub const AGENTS: [AgentId; 9] = [
        AgentId::ScamShield,
        AgentId::TokenPicker,
        AgentId::Executor,
        AgentId::HoldTimer,
        AgentId::ThesisFinder,
        AgentId::MemecoinMonitor,
        AgentId::XMonitor,
        AgentId::TopTraderMonitor,
        AgentId::TopGainers,
    ];
}

/// green = working, amber = waiting, grey = idle, red = blocked
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatusColor {
    Green,
    Amber,
    Grey,
    Red,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub agent: AgentId,
    pub color: StatusColor,
    pub line: String,
}

/// Everything the engine tells the UI. Sent over the Tauri event "engine-event".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EngineEvent {
    Status(AgentStatus),
    /// A message moved between two nodes: the line pulses green.
    #[serde(rename_all = "camelCase")]
    Pulse { from: AgentId, to: AgentId, text: String },
    #[serde(rename_all = "camelCase")]
    Activity { ts: i64, agent: AgentId, text: String, mint: Option<String> },
    #[serde(rename_all = "camelCase")]
    Decision { ts: i64, agent: AgentId, text: String },
    /// A new token was seen on the live feed (Watchlist row is created).
    #[serde(rename_all = "camelCase")]
    Token { ts: i64, mint: String, symbol: String, name: String, creator: String, mcap_sol: f64 },
    /// Outcome of one Scam Shield stage for one token. Every pass and every block is one of these.
    #[serde(rename_all = "camelCase")]
    Verdict {
        ts: i64,
        mint: String,
        symbol: String,
        stage: String,
        /// "pass" or "block"
        outcome: String,
        /// Machine code such as MINT_AUTHORITY (empty for a plain pass).
        reason: String,
        /// Plain-language explanation.
        detail: String,
        /// LOW_SIGNAL, ELEVATED or HIGH (exit-liquidity risk).
        verdict: String,
        evidence: Vec<Evidence>,
    },
    /// Token Picker observation result.
    #[serde(rename_all = "camelCase")]
    Score {
        ts: i64,
        mint: String,
        symbol: String,
        score: f64,
        min_score: f64,
        picked: bool,
        buyers: u32,
        buys: u32,
        sells: u32,
        ratio: f64,
        price_change_pct: f64,
        curve_sol: f64,
        mcap_sol: f64,
        gates: Vec<String>,
    },
    /// Today's Scam Shield totals (from the decisions table).
    #[serde(rename_all = "camelCase")]
    ShieldSummary { ts: i64, checked: u32, blocked: u32, passed: u32, by_reason: Vec<ReasonCount> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReasonCount {
    pub reason: String,
    pub count: u32,
}
