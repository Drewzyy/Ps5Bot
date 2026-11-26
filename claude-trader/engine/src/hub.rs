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
    events: VecDeque<EngineEvent>,
}

pub struct Hub {
    tx: broadcast::Sender<EngineEvent>,
    hist: Mutex<History>,
    logger: Mutex<Option<Arc<dyn Fn(&str) + Send + Sync>>>,
}

impl Hub {
    pub fn new() -> Arc<Hub> {
        let (tx, _) = broadcast::channel(4096);
        Arc::new(Hub { tx, hist: Mutex::new(History::default()), logger: Mutex::new(None) })
    }

    pub fn set_logger(&self, f: Arc<dyn Fn(&str) + Send + Sync>) {
        *self.logger.lock().unwrap() = Some(f);
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
                    let Ok(mut ws) = tokio_tungstenite::accept_async(sock).await else { return };
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
                            m = ws.next() => { if matches!(m, None | Some(Err(_)) | Some(Ok(Message::Close(_)))) { return; } }
                        }
                    }
                });
            }
        });
        Ok(local)
    }
}
