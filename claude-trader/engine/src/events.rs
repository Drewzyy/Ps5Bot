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
}
