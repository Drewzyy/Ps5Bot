//! Claude Trader engine. M2: live feed, Scam Shield, Token Picker. Paper only: no signing, no wallet code.

pub mod config;
pub mod curve;
pub mod replay;
pub mod trader;
pub mod report;
pub mod risk;
pub mod timefmt;
pub mod paper;
pub mod exits;
pub mod position;
pub mod demo;
pub mod db;
pub mod engine;
pub mod events;
pub mod hub;
pub mod monitor;
pub mod picker;
pub mod pump;
pub mod rpc;
pub mod shield;
pub mod net;
pub mod secrets;
pub mod update;
pub mod exec;
pub mod learning;

pub use config::Config;
pub use events::{AgentId, AgentStatus, EngineEvent, StatusColor};
