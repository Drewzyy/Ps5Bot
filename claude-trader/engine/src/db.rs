//! SQLite storage: decisions (every pass/block with stage + reason), creators (blacklist), tokens seen.

use crate::curve::FeeBreakdown;
use crate::events::ReasonCount;
use crate::position::{PosState, Position};
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
                mcap_sol REAL, trending INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS positions(
                id INTEGER PRIMARY KEY AUTOINCREMENT, lane TEXT NOT NULL, mint TEXT NOT NULL, symbol TEXT, creator TEXT, curve TEXT,
                state TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'trade', entry_ms INTEGER NOT NULL, size_sol REAL, entry_price REAL,
                entry_mcap_usd REAL, sol_usd REAL, tokens_initial REAL, tokens_remaining REAL, realized_sol REAL, gross_exit_sol REAL,
                tokens_sold REAL, fee_pump_fun REAL, fee_pump_portal REAL, fee_tx REAL, fee_priority REAL, fee_jito REAL, fee_creator REAL,
                buy_fixed_sol REAL, failed_sol REAL, peak_price REAL, tranches_done INTEGER, soft_since_ms INTEGER, taper_done INTEGER,
                last_price REAL, sell_fails INTEGER, exit_ms INTEGER, exit_reason TEXT, stages TEXT, pnl_sol REAL, pnl_pct REAL, slippage_pct REAL);
             CREATE INDEX IF NOT EXISTS positions_state ON positions(state);
             CREATE INDEX IF NOT EXISTS positions_mint ON positions(mint);
             CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT);",
        )
        .map_err(|e| format!("db schema: {e}"))?;
        // older databases were created before the creator fee column existed
        let _ = conn.execute("ALTER TABLE positions ADD COLUMN fee_creator REAL", []);
        Ok(Db { conn: Mutex::new(conn) })
    }

    /// Run `f` with the connection (used by the learning journal for its own tables).
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let c = self.conn.lock().unwrap();
        f(&c)
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

    /// Single-number query for the end-of-run counters.
    pub fn scalar(&self, sql: &str) -> i64 {
        self.conn.lock().unwrap().query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0)
    }

    pub fn kv_get(&self, k: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT v FROM kv WHERE k=?", params![k], |r| r.get::<_, String>(0)).ok()
    }

    pub fn kv_set(&self, k: &str, v: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute("INSERT INTO kv(k,v) VALUES(?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v", params![k, v]);
    }

    /// Kill switch state: the kv key holds the time it was switched on. Survives restarts.
    pub fn killed_at(&self) -> Option<i64> {
        self.kv_get("killed").and_then(|v| v.parse::<i64>().ok()).filter(|t| *t > 0)
    }

    pub fn kv_del(&self, k: &str) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute("DELETE FROM kv WHERE k=?", params![k]);
    }

    /// Insert a new position row (state BUYING is written BEFORE the paper buy) and return its id.
    pub fn insert_position(&self, p: &Position) -> i64 {
        {
            let c = self.conn.lock().unwrap();
            let _ = c.execute(
                "INSERT INTO positions(lane,mint,symbol,creator,curve,state,kind,entry_ms,size_sol,sol_usd) VALUES(?,?,?,?,?,?,?,?,?,?)",
                params![p.lane, p.mint, p.symbol, p.creator, p.curve, p.state.as_str(), p.kind, p.entry_ms, p.size_sol, p.sol_usd],
            );
        }
        let id = self.conn.lock().unwrap().last_insert_rowid();
        let mut q = p.clone();
        q.id = id;
        self.save_position(&q);
        id
    }

    pub fn save_position(&self, p: &Position) {
        let c = self.conn.lock().unwrap();
        let stages = serde_json::to_string(&p.stages).unwrap_or_else(|_| "[]".into());
        let _ = c.execute(
            "UPDATE positions SET lane=?,mint=?,symbol=?,creator=?,curve=?,state=?,kind=?,entry_ms=?,size_sol=?,entry_price=?,entry_mcap_usd=?,sol_usd=?,
             tokens_initial=?,tokens_remaining=?,realized_sol=?,gross_exit_sol=?,tokens_sold=?,fee_pump_fun=?,fee_pump_portal=?,fee_tx=?,fee_priority=?,fee_jito=?,
             buy_fixed_sol=?,failed_sol=?,peak_price=?,tranches_done=?,soft_since_ms=?,taper_done=?,last_price=?,sell_fails=?,exit_ms=?,exit_reason=?,stages=?,
             pnl_sol=?,pnl_pct=?,slippage_pct=?,fee_creator=? WHERE id=?",
            params![
                p.lane, p.mint, p.symbol, p.creator, p.curve, p.state.as_str(), p.kind, p.entry_ms, p.size_sol, p.entry_price, p.entry_mcap_usd, p.sol_usd,
                p.tokens_initial, p.tokens_remaining, p.realized_sol, p.gross_exit_sol, p.tokens_sold, p.fees.pump_fun, p.fees.pump_portal, p.fees.tx,
                p.fees.priority, p.fees.jito, p.buy_fixed_sol, p.failed_sol, p.peak_price, p.tranches_done, p.soft_since_ms, p.taper_done as i64,
                p.last_price, p.sell_fails, p.exit_ms, p.exit_reason, stages, p.pnl_sol, p.pnl_pct, p.slippage_pct, p.fees.creator, p.id
            ],
        );
    }

    fn query_positions(&self, where_sql: &str, args: &[&dyn rusqlite::ToSql]) -> Vec<Position> {
        let c = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT id,lane,mint,symbol,creator,curve,state,kind,entry_ms,size_sol,entry_price,entry_mcap_usd,sol_usd,tokens_initial,tokens_remaining,realized_sol,
             gross_exit_sol,tokens_sold,fee_pump_fun,fee_pump_portal,fee_tx,fee_priority,fee_jito,buy_fixed_sol,failed_sol,peak_price,tranches_done,soft_since_ms,
             taper_done,last_price,sell_fails,exit_ms,exit_reason,stages,pnl_sol,pnl_pct,slippage_pct,fee_creator FROM positions WHERE {where_sql}"
        );
        let Ok(mut st) = c.prepare(&sql) else { return vec![] };
        let rows = st.query_map(args, |r| {
            let f = |i: usize| r.get::<_, Option<f64>>(i).map(|v| v.unwrap_or(0.0));
            let stages: String = r.get::<_, Option<String>>(33)?.unwrap_or_default();
            Ok(Position {
                id: r.get(0)?,
                lane: r.get(1)?,
                mint: r.get(2)?,
                symbol: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                creator: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                curve: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                state: PosState::parse(&r.get::<_, String>(6)?),
                kind: r.get(7)?,
                entry_ms: r.get(8)?,
                size_sol: f(9)?,
                entry_price: f(10)?,
                entry_mcap_usd: f(11)?,
                sol_usd: f(12)?,
                tokens_initial: f(13)?,
                tokens_remaining: f(14)?,
                realized_sol: f(15)?,
                gross_exit_sol: f(16)?,
                tokens_sold: f(17)?,
                fees: FeeBreakdown { pump_fun: f(18)?, pump_portal: f(19)?, tx: f(20)?, priority: f(21)?, jito: f(22)?, creator: f(37)? },
                buy_fixed_sol: f(23)?,
                failed_sol: f(24)?,
                peak_price: f(25)?,
                tranches_done: r.get::<_, Option<u32>>(26)?.unwrap_or(0),
                soft_since_ms: r.get(27)?,
                taper_done: r.get::<_, Option<i64>>(28)?.unwrap_or(0) != 0,
                last_price: f(29)?,
                sell_fails: r.get::<_, Option<u32>>(30)?.unwrap_or(0),
                exit_ms: r.get(31)?,
                exit_reason: r.get::<_, Option<String>>(32)?.unwrap_or_default(),
                stages: serde_json::from_str(&stages).unwrap_or_default(),
                pnl_sol: f(34)?,
                pnl_pct: f(35)?,
                slippage_pct: f(36)?,
            })
        });
        rows.map(|it| it.flatten().collect()).unwrap_or_default()
    }

    /// Positions that must be managed after a restart: OPEN or SELLING (a SELLING row means the engine died
    /// mid-sell; paper sells are all-or-nothing in the DB, so it is resumed as OPEN by the caller).
    pub fn load_live_positions(&self) -> Vec<Position> {
        self.query_positions("state IN ('OPEN','SELLING') ORDER BY id", &[])
    }

    /// Rows left in BUYING by a crash before the fill was recorded.
    pub fn load_buying_positions(&self) -> Vec<Position> {
        self.query_positions("state='BUYING' ORDER BY id", &[])
    }

    /// Closed rows of a lane in order of exit (real trades only unless `with_failed`).
    pub fn load_closed(&self, lane: &str, with_failed: bool) -> Vec<Position> {
        let w = if with_failed { "lane=? AND state='CLOSED' ORDER BY exit_ms,id" } else { "lane=? AND state='CLOSED' AND kind='trade' ORDER BY exit_ms,id" };
        self.query_positions(w, &[&lane])
    }

    pub fn position_by_id(&self, id: i64) -> Option<Position> {
        self.query_positions("id=?", &[&id]).into_iter().next()
    }

    pub fn mint_has_position(&self, mint: &str) -> bool {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COUNT(*) FROM positions WHERE mint=?", params![mint], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    }

    /// Latest Scam Shield decision for a coin, newest first: (symbol, stage, outcome, reason, detail, verdict).
    pub fn latest_decision(&self, mint: &str) -> Option<(String, String, String, String, String, String)> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT COALESCE(symbol,''),stage,outcome,reason,COALESCE(detail,''),COALESCE(verdict,'') FROM decisions WHERE mint=? ORDER BY (outcome='block') DESC, id DESC LIMIT 1",
            params![mint],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .ok()
    }

    /// True while a position in this coin is open or being sold.
    pub fn mint_is_held(&self, mint: &str) -> bool {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COUNT(*) FROM positions WHERE mint=? AND state IN ('OPEN','SELLING','BUYING')", params![mint], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    }

    /// Number of position rows ever written (tests use it to prove a check opened nothing).
    pub fn position_count(&self) -> i64 {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COUNT(*) FROM positions", [], |r| r.get(0)).unwrap_or(0)
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
    fn position_survives_a_restart_with_every_field() {
        let path = std::env::temp_dir().join(format!("ct-pos-{}.db", crate::rpc::now_ms()));
        let ps = path.to_str().unwrap();
        let mut p = Position::new_buying("momentum", "MINT", "TST", "CR", "CV", 0.1, 5000, 150.0);
        let id;
        {
            let db = Db::open(ps).unwrap();
            id = db.insert_position(&p);
            assert_eq!(db.load_buying_positions().len(), 1, "written before the buy");
            p.id = id;
            p.fill_buy(1.5e-7, 600_000.0, 8000.0, &FeeBreakdown { pump_fun: 0.00095, creator: 0.0003, pump_portal: 0.0005, tx: 0.000005, priority: 0.0005, jito: 0.0 }, 1.5).unwrap();
            p.tranches_done = 1;
            p.soft_since_ms = Some(77);
            db.save_position(&p);
        }
        let db = Db::open(ps).unwrap();
        assert!(db.load_buying_positions().is_empty());
        let l = db.load_live_positions();
        assert_eq!(l.len(), 1);
        let q = &l[0];
        assert_eq!((q.id, q.state, q.entry_price, q.tokens_remaining, q.tranches_done, q.soft_since_ms), (id, PosState::Open, 1.5e-7, 600_000.0, 1, Some(77)));
        assert!((q.fees.total() - p.fees.total()).abs() < 1e-15 && q.buy_fixed_sol == p.buy_fixed_sol);
        db.kv_set("a", "1");
        assert_eq!(db.kv_get("a").as_deref(), Some("1"));
        let _ = std::fs::remove_file(path);
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
