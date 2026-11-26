//! Memecoin Monitor: ONE PumpPortal socket. subscribeNewToken + subscribeMigration.
//! (subscribeTokenTrade is not used: it needs an API key funded with 0.02 SOL, see rpc.rs for the free replacement.)
//! Reconnect backoff 1/2/4 .. 30 s. Buying is flagged off ("no buys") if disconnected for more than 5 s.

use crate::hub::Hub;
use crate::net::{connect_wss, mask};
use crate::pump::{parse_feed, FeedMsg, Parsed};
use crate::rpc::now_ms;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

pub const PUMPPORTAL_WS: &str = "wss://pumpportal.fun/api/data";

pub enum Incoming {
    Create(FeedMsg),
    Migrate(FeedMsg),
}

#[derive(Default)]
pub struct MonitorState {
    pub connected: AtomicBool,
    /// ms epoch when the socket was last lost (0 while connected).
    pub down_since_ms: AtomicI64,
    pub last_msg_ms: AtomicI64,
    pub created: AtomicU64,
    pub reconnects: AtomicU64,
}

impl MonitorState {
    /// True if the feed has been down for more than 5 s: no buys allowed.
    pub fn no_buys(&self) -> bool {
        !self.connected.load(Ordering::Relaxed) && {
            let d = self.down_since_ms.load(Ordering::Relaxed);
            d != 0 && now_ms() - d > 5000
        }
    }
}

/// Backoff sequence 1, 2, 4, 8, 16, 30, 30...
pub fn next_backoff(cur: u64) -> u64 {
    (cur * 2).min(30)
}

pub async fn run(url: String, tx: mpsc::UnboundedSender<Incoming>, st: Arc<MonitorState>, hub: Arc<Hub>, mut stop: watch::Receiver<bool>) {
    let mut backoff = 1u64;
    st.down_since_ms.store(now_ms(), Ordering::Relaxed);
    loop {
        if *stop.borrow() {
            return;
        }
        match connect_wss(&url).await {
            Ok(mut ws) => {
                let ok = ws.send(Message::Text(r#"{"method":"subscribeNewToken"}"#.into())).await.is_ok()
                    && ws.send(Message::Text(r#"{"method":"subscribeMigration"}"#.into())).await.is_ok();
                if !ok {
                    hub.log("feed: subscribe failed, reconnecting");
                } else {
                    st.connected.store(true, Ordering::Relaxed);
                    st.down_since_ms.store(0, Ordering::Relaxed);
                    st.last_msg_ms.store(now_ms(), Ordering::Relaxed);
                    hub.log(format!("feed: connected to {} (subscribeNewToken, subscribeMigration)", mask(&url)));
                    let started = Instant::now();
                    let mut silent = tokio::time::interval(Duration::from_secs(10));
                    let end: String = loop {
                        tokio::select! {
                            _ = stop.changed() => { if *stop.borrow() { return; } }
                            _ = silent.tick() => {
                                // pump.fun creates a coin every few seconds; 60 s of silence means a dead socket
                                if now_ms() - st.last_msg_ms.load(Ordering::Relaxed) > 60_000 { break "no messages for 60 s".to_string(); }
                            }
                            m = ws.next() => match m {
                                Some(Ok(Message::Text(t))) => {
                                    st.last_msg_ms.store(now_ms(), Ordering::Relaxed);
                                    if started.elapsed() > Duration::from_secs(5) { backoff = 1; }
                                    match parse_feed(&t) {
                                        Parsed::Create(m) => { st.created.fetch_add(1, Ordering::Relaxed); let _ = tx.send(Incoming::Create(m)); }
                                        Parsed::Migrate(m) => { let _ = tx.send(Incoming::Migrate(m)); }
                                        Parsed::Info(s) => hub.log(format!("feed: server says: {s}")),
                                        Parsed::Other => {}
                                    }
                                }
                                Some(Ok(Message::Ping(p))) => { let _ = ws.send(Message::Pong(p)).await; }
                                Some(Ok(Message::Close(_))) | None => break "closed by server".to_string(),
                                Some(Err(e)) => break mask(&e.to_string()),
                                _ => {}
                            }
                        }
                    };
                    st.connected.store(false, Ordering::Relaxed);
                    st.down_since_ms.store(now_ms(), Ordering::Relaxed);
                    st.reconnects.fetch_add(1, Ordering::Relaxed);
                    hub.log(format!("feed: DISCONNECTED ({end}); reconnecting in {backoff}s"));
                }
            }
            Err(e) => hub.log(format!("feed: connect error ({}); retry in {backoff}s", mask(&e))),
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
            _ = stop.changed() => { if *stop.borrow() { return; } }
        }
        backoff = next_backoff(backoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_sequence() {
        let mut b = 1;
        let mut seq = vec![b];
        for _ in 0..7 {
            b = next_backoff(b);
            seq.push(b);
        }
        assert_eq!(seq, vec![1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn no_buys_after_5s_down() {
        let s = MonitorState::default();
        s.connected.store(false, Ordering::Relaxed);
        s.down_since_ms.store(now_ms() - 6000, Ordering::Relaxed);
        assert!(s.no_buys());
        s.down_since_ms.store(now_ms() - 2000, Ordering::Relaxed);
        assert!(!s.no_buys());
        s.connected.store(true, Ordering::Relaxed);
        s.down_since_ms.store(0, Ordering::Relaxed);
        assert!(!s.no_buys());
    }
}
