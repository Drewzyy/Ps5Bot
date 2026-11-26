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
    /// Open positions plus the few most recently closed ones (for the chart), refreshed about once a second.
    #[serde(rename_all = "camelCase")]
    Positions { ts: i64, positions: Vec<PositionView> },
    /// Paper account, performance, fees and equity curve per entry style (first lane is the main one).
    #[serde(rename_all = "camelCase")]
    Stats { ts: i64, balance_sol: f64, bankroll_sol: f64, sol_usd: f64, replay: bool, lanes: Vec<LaneStats> },
    /// Kill switch state, refreshed about once a second.
    #[serde(rename_all = "camelCase")]
    Kill(KillState),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KillState {
    pub ts: i64,
    /// True while the kill switch is on (persisted in SQLite, survives a restart).
    pub killed: bool,
    pub killed_at_ms: i64,
    /// Positions still being sold, and positions already closed by the kill switch.
    pub closing: u32,
    pub closed: u32,
    pub items: Vec<KillItem>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KillItem {
    pub symbol: String,
    pub lane: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExitMark {
    pub ts: i64,
    pub price: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PositionView {
    pub id: i64,
    pub lane: String,
    pub mint: String,
    pub symbol: String,
    /// OPEN, SELLING or CLOSED
    pub state: String,
    pub entry_ts: i64,
    pub entry_price: f64,
    pub current_price: f64,
    pub size_sol: f64,
    pub tokens_remaining: f64,
    /// Net P/L in SOL after every fee, marked at what the remaining tokens would really fetch (never mid price).
    pub pnl_sol: f64,
    pub pnl_pct: f64,
    /// Price move against the entry price (what the stops and targets read).
    pub price_pct: f64,
    pub hold_seconds: f64,
    pub next_action: String,
    pub soft_stop: f64,
    pub hard_stop: f64,
    pub take_profits: Vec<f64>,
    pub tranches_done: u32,
    pub exits: Vec<ExitMark>,
    pub exit_reason: String,
    /// Recent curve trades as [ms, price] for the candles.
    pub ticks: Vec<[f64; 2]>,
    /// True when the market data is a scripted replay, not the live feed.
    pub replay: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LaneStats {
    pub lane: String,
    pub trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub win_rate_pct: f64,
    pub avg_win_sol: f64,
    pub avg_loss_sol: f64,
    pub profit_factor: Option<f64>,
    pub max_drawdown_sol: f64,
    pub total_pnl_sol: f64,
    pub best_sol: f64,
    pub worst_sol: f64,
    pub trades_per_day: f64,
    /// [exit ms, equity SOL] after each closed trade.
    pub equity: Vec<[f64; 2]>,
    pub fees_today: crate::curve::FeeBreakdown,
    pub fees_all: crate::curve::FeeBreakdown,
    /// Token-account rent out of the wallet for open positions (returned when each account closes).
    pub rent_locked_sol: f64,
    pub fee_per_trade: f64,
    pub pnl_today: f64,
    pub trades_today: u32,
    pub buys_today: u32,
    pub sells_today: u32,
    pub failed_buys: u32,
    pub open: u32,
    pub halted: Option<String>,
    pub paused: bool,
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
