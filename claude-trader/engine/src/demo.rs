//! Mock event generator. Deterministic, no network. Used only for demos and the UI shell.

use crate::events::*;

pub struct DemoGen {
    state: u64,
    tick: u64,
}

impl DemoGen {
    pub fn new(seed: u64) -> Self {
        DemoGen { state: seed.max(1), tick: 0 }
    }

    fn next(&mut self) -> u64 {
        // xorshift64
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Initial status of every agent (all idle in M1).
    pub fn initial_statuses() -> Vec<EngineEvent> {
        AgentId::AGENTS
            .iter()
            .map(|a| {
                EngineEvent::Status(AgentStatus {
                    agent: *a,
                    color: StatusColor::Grey,
                    line: "Idle (demo)".into(),
                })
            })
            .collect()
    }

    /// Call about once per second. Occasionally returns a pulse event.
    pub fn tick(&mut self, _now_ms: i64) -> Vec<EngineEvent> {
        self.tick += 1;
        let mut out = Vec::new();
        if self.next() % 5 == 0 {
            let pairs = [
                (AgentId::MemecoinMonitor, AgentId::ScamShield, "New launch handed to Scam Shield"),
                (AgentId::ScamShield, AgentId::TokenPicker, "Clean token handed to Token Picker"),
                (AgentId::TokenPicker, AgentId::Executor, "Pick handed to Executor"),
            ];
            let (from, to, text) = pairs[(self.next() % pairs.len() as u64) as usize];
            out.push(EngineEvent::Pulse { from, to, text: text.into() });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_deterministic_and_emits_pulses() {
        let mut a = DemoGen::new(7);
        let mut b = DemoGen::new(7);
        let mut pulses = 0;
        for i in 0..100 {
            let ea = a.tick(i);
            assert_eq!(ea, b.tick(i));
            pulses += ea.iter().filter(|e| matches!(e, EngineEvent::Pulse { .. })).count();
        }
        assert!(pulses > 0);
        assert_eq!(DemoGen::initial_statuses().len(), 9);
    }

    #[test]
    fn events_serialize_with_type_tag() {
        let e = EngineEvent::Pulse { from: AgentId::ScamShield, to: AgentId::TokenPicker, text: "x".into() };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains("\"type\":\"pulse\"") && s.contains("scam-shield"));
    }
}
