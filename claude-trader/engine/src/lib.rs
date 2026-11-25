//! Claude Trader engine (M1: types, events and a demo event generator).
//! No network, no signing, no wallet code lives here yet.

pub mod config;
pub mod demo;
pub mod events;

pub use config::Config;
pub use events::{AgentId, AgentStatus, EngineEvent, StatusColor};
