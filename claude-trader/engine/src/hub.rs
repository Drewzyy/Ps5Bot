//! Event hub: engine -> UI. Keeps a short history so a UI that connects late (or reloads) gets a snapshot,
//! and serves the events over a local WebSocket (127.0.0.1 only) for browser mode.

use crate::events::{AgentId, EngineEvent};
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

const HISTORY: usize = 1500;

#[derive(Default)]
struct History {
    statuses: HashMap<AgentId, EngineEvent>,
    summary: Option<EngineEvent>,
    positions: Option<EngineEvent>,
    stats: Option<EngineEvent>,
    kill: Option<EngineEvent>,
    learning: Option<EngineEvent>,
    skipped: Option<EngineEvent>,
    extension: Option<EngineEvent>,
    feed: Option<EngineEvent>,
    funnel: Option<EngineEvent>,
    links: HashMap<String, EngineEvent>,
    events: VecDeque<EngineEvent>,
}

pub struct Hub {
    tx: broadcast::Sender<EngineEvent>,
    hist: Mutex<History>,
    logger: Mutex<Option<Arc<dyn Fn(&str) + Send + Sync>>>,
    commands: Mutex<Option<Arc<dyn Fn(&str) -> String + Send + Sync>>>,
}

impl Hub {
    pub fn new() -> Arc<Hub> {
        let (tx, _) = broadcast::channel(4096);
        Arc::new(Hub { tx, hist: Mutex::new(History::default()), logger: Mutex::new(None), commands: Mutex::new(None) })
    }

    pub fn set_logger(&self, f: Arc<dyn Fn(&str) + Send + Sync>) {
        *self.logger.lock().unwrap() = Some(f);
    }

    /// Control commands from the UI ("kill", "resume", "kill-state"). Returns a one-line answer.
    pub fn set_command_handler(&self, f: Arc<dyn Fn(&str) -> String + Send + Sync>) {
        *self.commands.lock().unwrap() = Some(f);
    }

    pub fn command(&self, cmd: &str) -> String {
        match self.commands.lock().unwrap().as_ref() {
            Some(f) => f(cmd),
            None => "engine not ready".into(),
        }
    }

    /// Plain log line (reconnects, errors). Goes to the CLI's stdout; never contains keys (callers mask).
    pub fn log(&self, line: impl AsRef<str>) {
        if let Some(l) = self.logger.lock().unwrap().as_ref() {
            l(line.as_ref());
        }
    }

    pub fn emit(&self, e: EngineEvent) {
        {
            let mut h = self.hist.lock().unwrap();
            match &e {
                EngineEvent::Status(s) => {
                    h.statuses.insert(s.agent, e.clone());
                }
                EngineEvent::ShieldSummary { .. } => h.summary = Some(e.clone()),
                EngineEvent::Positions { .. } => h.positions = Some(e.clone()),
                EngineEvent::Stats { .. } => h.stats = Some(e.clone()),
                EngineEvent::Kill(_) => h.kill = Some(e.clone()),
                EngineEvent::Learning(_) => h.learning = Some(e.clone()),
                EngineEvent::Skipped { .. } => h.skipped = Some(e.clone()),
                EngineEvent::Extension { .. } => h.extension = Some(e.clone()),
                // sent every 2 s: only the latest is kept, so they never push real events out of the history
                EngineEvent::FeedHealth(_) => h.feed = Some(e.clone()),
                EngineEvent::Funnel(_) => h.funnel = Some(e.clone()),
                EngineEvent::Links { mint, .. } => {
                    if h.links.len() > 600 {
                        h.links.clear();
                    }
                    h.links.insert(mint.clone(), e.clone());
                }
                EngineEvent::Pulse { .. } => {}
                _ => {
                    h.events.push_back(e.clone());
                    if h.events.len() > HISTORY {
                        h.events.pop_front();
                    }
                }
            }
        }
        let _ = self.tx.send(e);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.tx.subscribe()
    }

    /// Statuses + summary + recent events, oldest first.
    pub fn snapshot(&self) -> Vec<EngineEvent> {
        let h = self.hist.lock().unwrap();
        let mut out: Vec<EngineEvent> = h.statuses.values().cloned().collect();
        out.extend(h.events.iter().cloned());
        if let Some(s) = &h.summary {
            out.push(s.clone());
        }
        out.extend(h.positions.iter().cloned());
        out.extend(h.stats.iter().cloned());
        out.extend(h.kill.iter().cloned());
        out.extend(h.learning.iter().cloned());
        out.extend(h.skipped.iter().cloned());
        out.extend(h.extension.iter().cloned());
        out.extend(h.feed.iter().cloned());
        out.extend(h.funnel.iter().cloned());
        out.extend(h.links.values().cloned());
        out
    }

    /// Serve events as JSON text frames on ws://addr. Each client first gets the snapshot.
    pub async fn serve(self: Arc<Self>, addr: &str) -> Result<std::net::SocketAddr, String> {
        let l = TcpListener::bind(addr).await.map_err(|e| format!("bind {addr}: {e}"))?;
        let local = l.local_addr().map_err(|e| e.to_string())?;
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = l.accept().await else { continue };
                let hub = self.clone();
                tokio::spawn(async move {
                    // only pages served from this machine may talk to the engine (the kill switch lives on this socket)
                    let check = |req: &tokio_tungstenite::tungstenite::handshake::server::Request, resp| {
                        let ok = match req.headers().get("origin").and_then(|v| v.to_str().ok()) {
                            None => true,
                            Some(o) => o.starts_with("http://127.0.0.1") || o.starts_with("http://localhost") || o.starts_with("tauri://") || o.starts_with("http://tauri.localhost") || o == "null",
                        };
                        if ok { Ok(resp) } else { Err(tokio_tungstenite::tungstenite::http::Response::builder().status(403).body(None).unwrap()) }
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(sock, check).await else { return };
                    let mut rx = hub.subscribe();
                    for e in hub.snapshot() {
                        if ws.send(Message::Text(serde_json::to_string(&e).unwrap_or_default())).await.is_err() {
                            return;
                        }
                    }
                    loop {
                        tokio::select! {
                            e = rx.recv() => match e {
                                Ok(e) => { if ws.send(Message::Text(serde_json::to_string(&e).unwrap_or_default())).await.is_err() { return; } }
                                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                                Err(_) => return,
                            },
                            m = ws.next() => match m {
                                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                                Some(Ok(Message::Text(t))) => {
                                    let cmd = serde_json::from_str::<serde_json::Value>(&t).ok().and_then(|v| v["cmd"].as_str().map(String::from)).unwrap_or_default();
                                    if !cmd.is_empty() {
                                        let ans = hub.command(&cmd);
                                        hub.log(format!("ui command {cmd}: {ans}"));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                });
            }
        });
        Ok(local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_health_and_funnel_keep_only_the_latest_and_never_flood_the_history() {
        let hub = Hub::new();
        for i in 0..2000 {
            hub.emit(EngineEvent::FeedHealth(crate::events::FeedHealthView { ts: i, ..Default::default() }));
            hub.emit(EngineEvent::Funnel(crate::events::FunnelView { ts: i, ..Default::default() }));
        }
        let snap = hub.snapshot();
        assert_eq!(snap.len(), 2, "one health and one funnel event, nothing else");
        assert!(snap.iter().any(|e| matches!(e, EngineEvent::FeedHealth(h) if h.ts == 1999)));
    }
}
