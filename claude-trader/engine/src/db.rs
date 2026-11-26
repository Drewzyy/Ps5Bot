//! SQLite storage: decisions (every pass/block with stage + reason), creators (blacklist), tokens seen.

use crate::events::ReasonCount;
use rusqlite::{params, Connection};
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct DecisionRow {
    pub ts: i64,
    pub mint: String,
    pub symbol: String,
    pub stage: String,
    pub outcome: String,
    pub reason: String,
    pub detail: String,
    pub verdict: String,
}

impl Db {
    pub fn open(path: &str) -> Result<Db, String> {
        let conn = if path == ":memory:" { Connection::open_in_memory() } else { Connection::open(path) }
            .map_err(|e| format!("db open: {e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS decisions(
                id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, mint TEXT NOT NULL, symbol TEXT,
                stage TEXT NOT NULL, outcome TEXT NOT NULL, reason TEXT NOT NULL, detail TEXT, verdict TEXT);
             CREATE INDEX IF NOT EXISTS decisions_ts ON decisions(ts);
             CREATE INDEX IF NOT EXISTS decisions_mint ON decisions(mint);
             CREATE TABLE IF NOT EXISTS creators(
                wallet TEXT PRIMARY KEY, blacklisted INTEGER NOT NULL DEFAULT 0, reason TEXT, ts INTEGER);
             CREATE TABLE IF NOT EXISTS tokens_seen(
                mint TEXT PRIMARY KEY, ts INTEGER NOT NULL, name TEXT, symbol TEXT, creator TEXT,
                mcap_sol REAL, trending INTEGER NOT NULL DEFAULT 0);",
        )
        .map_err(|e| format!("db schema: {e}"))?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    pub fn insert_decision(&self, d: &DecisionRow) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO decisions(ts,mint,symbol,stage,outcome,reason,detail,verdict) VALUES(?,?,?,?,?,?,?,?)",
            params![d.ts, d.mint, d.symbol, d.stage, d.outcome, d.reason, d.detail, d.verdict],
        );
    }

    pub fn upsert_token(&self, mint: &str, ts: i64, name: &str, symbol: &str, creator: &str, mcap_sol: f64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT OR IGNORE INTO tokens_seen(mint,ts,name,symbol,creator,mcap_sol) VALUES(?,?,?,?,?,?)",
            params![mint, ts, name, symbol, creator, mcap_sol],
        );
    }

    /// Mark a token as "seen trending" (it passed Scam Shield or its market cap cleared the feed minimum).
    pub fn mark_trending(&self, mint: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute("UPDATE tokens_seen SET trending=1 WHERE mint=?", params![mint]);
    }

    /// (name, symbol) of tokens seen trending since `since_ms`.
    pub fn trending_names(&self, since_ms: i64) -> Vec<(String, String)> {
        let c = self.conn.lock().unwrap();
        let mut st = match c.prepare("SELECT name, symbol FROM tokens_seen WHERE trending=1 AND ts>=?") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map(params![since_ms], |r| Ok((r.get::<_, String>(0).unwrap_or_default(), r.get::<_, String>(1).unwrap_or_default())))
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn blacklist_creator(&self, wallet: &str, reason: &str, ts: i64) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO creators(wallet,blacklisted,reason,ts) VALUES(?,1,?,?)
             ON CONFLICT(wallet) DO UPDATE SET blacklisted=1, reason=excluded.reason, ts=excluded.ts",
            params![wallet, reason, ts],
        );
    }

    pub fn blacklist_reason(&self, wallet: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT reason FROM creators WHERE wallet=? AND blacklisted=1", params![wallet], |r| r.get::<_, Option<String>>(0))
            .ok()
            .map(|r| r.unwrap_or_default())
    }

    /// (checked tokens, blocked tokens, passed tokens, blocks by reason) since `since_ms`.
    /// A token counts as blocked once if any of its decisions is a block, otherwise as passed.
    pub fn summary(&self, since_ms: i64) -> (u32, u32, u32, Vec<ReasonCount>) {
        let c = self.conn.lock().unwrap();
        let checked: u32 = c
            .query_row("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND stage!='observation'", params![since_ms], |r| r.get(0))
            .unwrap_or(0);
        let blocked: u32 = c
            .query_row("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND stage!='observation' AND outcome='block'", params![since_ms], |r| r.get(0))
            .unwrap_or(0);
        let mut by = vec![];
        if let Ok(mut st) = c.prepare(
            "SELECT reason, COUNT(*) FROM decisions WHERE ts>=? AND stage!='observation' AND outcome='block' GROUP BY reason ORDER BY COUNT(*) DESC",
        ) {
            if let Ok(rows) = st.query_map(params![since_ms], |r| Ok(ReasonCount { reason: r.get(0)?, count: r.get(1)? })) {
                by = rows.flatten().collect();
            }
        }
        (checked, blocked, checked.saturating_sub(blocked), by)
    }

    pub fn decision_count(&self) -> u32 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COUNT(*) FROM decisions", [], |r| r.get(0)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(mint: &str, outcome: &str, reason: &str) -> DecisionRow {
        DecisionRow { ts: 1000, mint: mint.into(), symbol: "S".into(), stage: "chain".into(), outcome: outcome.into(), reason: reason.into(), detail: "d".into(), verdict: "HIGH".into() }
    }

    #[test]
    fn decisions_and_summary() {
        let db = Db::open(":memory:").unwrap();
        db.insert_decision(&row("a", "block", "MINT_AUTHORITY"));
        db.insert_decision(&row("b", "pass", ""));
        db.insert_decision(&row("c", "block", "MINT_AUTHORITY"));
        db.insert_decision(&row("c", "block", "BUNDLE_BURST"));
        let (checked, blocked, passed, by) = db.summary(0);
        assert_eq!((checked, blocked, passed), (3, 2, 1));
        assert_eq!(by[0].reason, "MINT_AUTHORITY");
        assert_eq!(by[0].count, 2);
        assert_eq!(db.decision_count(), 4);
    }

    #[test]
    fn blacklist_roundtrip() {
        let db = Db::open(":memory:").unwrap();
        assert!(db.blacklist_reason("w1").is_none());
        db.blacklist_creator("w1", "fell 80% in 3 min", 5);
        assert_eq!(db.blacklist_reason("w1").unwrap(), "fell 80% in 3 min");
    }

    #[test]
    fn trending_only_marked() {
        let db = Db::open(":memory:").unwrap();
        db.upsert_token("m1", 100, "Moon Cat", "MCAT", "c", 30.0);
        db.upsert_token("m2", 100, "Other", "OTH", "c", 30.0);
        db.mark_trending("m1");
        let t = db.trending_names(0);
        assert_eq!(t, vec![("Moon Cat".to_string(), "MCAT".to_string())]);
    }
}
