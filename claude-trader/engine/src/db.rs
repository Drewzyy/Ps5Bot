//! SQLite storage: decisions (every pass/block with stage + reason), creators (blacklist), tokens seen.

use crate::curve::FeeBreakdown;
use crate::events::ReasonCount;
use crate::position::{PosState, Position};
use rusqlite::{params, Connection};
use std::sync::Mutex;

/// Real counts for the dashboard's "Why no trades" card.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunnelCounts {
    pub seen: u32,
    pub blocked: u32,
    pub blocked_reasons: Vec<ReasonCount>,
    pub scored: u32,
    pub scored_no_trades: u32,
    pub best_score: f64,
    pub best_symbol: String,
    pub best_blocked_by: Vec<ReasonCount>,
    pub picked: u32,
    pub bought: u32,
    pub rug_fallback_passed: u32,
    pub rug_fallback_blocked: u32,
}

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
             CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY, v TEXT);
             CREATE TABLE IF NOT EXISTS scores(
                id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, mint TEXT NOT NULL, symbol TEXT, score REAL NOT NULL,
                picked INTEGER NOT NULL, gates TEXT, trades INTEGER NOT NULL DEFAULT 0, buyers INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX IF NOT EXISTS scores_ts ON scores(ts);",
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

    /// One scored coin (end of the 60 s watch). `gates` are the plain labels of every entry gate that failed.
    pub fn insert_score(&self, ts: i64, mint: &str, symbol: &str, score: f64, picked: bool, gates: &[String], trades: u32, buyers: u32) {
        let c = self.conn.lock().unwrap();
        let _ = c.execute(
            "INSERT INTO scores(ts,mint,symbol,score,picked,gates,trades,buyers) VALUES(?,?,?,?,?,?,?,?)",
            params![ts, mint, symbol, score, picked as i64, gates.join("|"), trades, buyers],
        );
    }

    /// Where coins dropped off since `since_ms`, from real rows only (SPEC 9, "Why no trades").
    pub fn funnel(&self, since_ms: i64, top_n: u32) -> FunnelCounts {
        let c = self.conn.lock().unwrap();
        let n = |sql: &str| -> u32 { c.query_row(sql, params![since_ms], |r| r.get::<_, i64>(0)).unwrap_or(0).max(0) as u32 };
        let shield = "stage IN ('cheap-prefilter','chain','rugcheck','rugcheck-fallback') AND outcome='block'";
        let seen = n("SELECT COUNT(*) FROM tokens_seen WHERE ts>=?");
        let blocked = n(&format!("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND {shield}"));
        let scored = n("SELECT COUNT(*) FROM scores WHERE ts>=?");
        let scored_no_trades = n("SELECT COUNT(*) FROM scores WHERE ts>=? AND trades=0");
        let picked = n("SELECT COUNT(*) FROM scores WHERE ts>=? AND picked=1");
        let bought = n("SELECT COUNT(*) FROM positions WHERE entry_ms>=? AND kind='trade'");
        let rug_fb_pass = n("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND stage='rugcheck-fallback' AND outcome='pass'");
        let rug_fb_block = n("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND stage='rugcheck-fallback' AND outcome='block'");
        let mut blocked_reasons = vec![];
        if let Ok(mut st) = c.prepare(&format!("SELECT reason, COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND {shield} GROUP BY reason ORDER BY COUNT(DISTINCT mint) DESC, reason LIMIT 8")) {
            if let Ok(rows) = st.query_map(params![since_ms], |r| Ok(ReasonCount { reason: r.get(0)?, count: r.get(1)? })) {
                blocked_reasons = rows.flatten().collect();
            }
        }
        let best: Option<(f64, String)> = c
            .query_row("SELECT score, COALESCE(symbol,'') FROM scores WHERE ts>=? ORDER BY score DESC, id DESC LIMIT 1", params![since_ms], |r| Ok((r.get(0)?, r.get(1)?)))
            .ok();
        // which gates stopped the highest-scoring coins that were not picked
        let mut gate_counts: std::collections::BTreeMap<String, u32> = Default::default();
        if let Ok(mut st) = c.prepare("SELECT COALESCE(gates,''), picked FROM scores WHERE ts>=? ORDER BY score DESC, id DESC LIMIT ?") {
            if let Ok(rows) = st.query_map(params![since_ms, top_n], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) {
                for (g, picked) in rows.flatten() {
                    if picked == 0 {
                        for label in g.split('|').filter(|x| !x.is_empty()) {
                            *gate_counts.entry(label.to_string()).or_default() += 1;
                        }
                    }
                }
            }
        }
        let mut best_blocked_by: Vec<ReasonCount> = gate_counts.into_iter().map(|(reason, count)| ReasonCount { reason, count }).collect();
        best_blocked_by.sort_by(|a, b| b.count.cmp(&a.count).then(a.reason.cmp(&b.reason)));
        FunnelCounts {
            seen,
            blocked,
            blocked_reasons,
            scored,
            scored_no_trades,
            best_score: best.as_ref().map(|b| b.0).unwrap_or(0.0),
            best_symbol: best.map(|b| b.1).unwrap_or_default(),
            best_blocked_by,
            picked,
            bought,
            rug_fallback_passed: rug_fb_pass,
            rug_fallback_blocked: rug_fb_block,
        }
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

    /// Every closed real trade, any lane, oldest exit first (the Obsidian vault backfill reads this).
    pub fn closed_trades_all(&self) -> Vec<Position> {
        self.query_positions("state='CLOSED' AND kind='trade' ORDER BY exit_ms,id", &[])
    }

    /// Scam Shield decisions of one coin, oldest first (observation rows left out).
    pub fn decisions_for_mint(&self, mint: &str) -> Vec<DecisionRow> {
        let c = self.conn.lock().unwrap();
        let Ok(mut st) = c.prepare("SELECT ts,mint,COALESCE(symbol,''),stage,outcome,reason,COALESCE(detail,''),COALESCE(verdict,'') FROM decisions WHERE mint=? AND stage!='observation' ORDER BY id") else { return vec![] };
        st.query_map(params![mint], |r| Ok(DecisionRow { ts: r.get(0)?, mint: r.get(1)?, symbol: r.get(2)?, stage: r.get(3)?, outcome: r.get(4)?, reason: r.get(5)?, detail: r.get(6)?, verdict: r.get(7)? }))
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    /// Real counts for one local day window `[from_ms, to_ms)`: (coins seen, coins blocked by Scam Shield, scored, picked, bought).
    pub fn day_counts(&self, from_ms: i64, to_ms: i64) -> (u32, u32, u32, u32, u32) {
        let c = self.conn.lock().unwrap();
        let n = |sql: &str| -> u32 { c.query_row(sql, params![from_ms, to_ms], |r| r.get::<_, i64>(0)).unwrap_or(0).max(0) as u32 };
        (
            n("SELECT COUNT(*) FROM tokens_seen WHERE ts>=? AND ts<?"),
            n("SELECT COUNT(DISTINCT mint) FROM decisions WHERE ts>=? AND ts<? AND stage IN ('cheap-prefilter','chain','rugcheck','rugcheck-fallback') AND outcome='block'"),
            n("SELECT COUNT(*) FROM scores WHERE ts>=? AND ts<?"),
            n("SELECT COUNT(*) FROM scores WHERE ts>=? AND ts<? AND picked=1"),
            n("SELECT COUNT(*) FROM positions WHERE entry_ms>=? AND entry_ms<? AND kind='trade'"),
        )
    }

    pub fn token_name(&self, mint: &str) -> Option<(String, String, String, i64)> {
        let c = self.conn.lock().unwrap();
        c.query_row("SELECT COALESCE(name,''),COALESCE(symbol,''),COALESCE(creator,''),ts FROM tokens_seen WHERE mint=?", params![mint], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).ok()
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
    fn funnel_counts_where_coins_dropped_off_from_real_rows() {
        let db = Db::open(":memory:").unwrap();
        let t = 1_000_000i64;
        for m in ["A", "B", "C", "D", "E", "F"] {
            db.upsert_token(m, t, m, m, "cr", 28.0);
        }
        let row = |mint: &str, stage: &str, outcome: &str, reason: &str| DecisionRow { ts: t, mint: mint.into(), symbol: mint.into(), stage: stage.into(), outcome: outcome.into(), reason: reason.into(), detail: String::new(), verdict: String::new() };
        db.insert_decision(&row("A", "cheap-prefilter", "block", "NO_IMAGE"));
        db.insert_decision(&row("B", "chain", "block", "DEV_SNIPE"));
        db.insert_decision(&row("C", "rugcheck", "block", "RUGCHECK_DANGER"));
        db.insert_decision(&row("D", "chain", "pass", ""));
        db.insert_decision(&row("E", "chain", "pass", ""));
        db.insert_decision(&row("E", "rugcheck-fallback", "pass", ""));
        db.insert_decision(&row("F", "chain", "pass", ""));
        db.insert_decision(&row("F", "pre-buy", "block", "WASH_VOLUME")); // a pre-buy block is after scoring, not a shield drop
        db.insert_score(t, "D", "D", 12.5, false, &["Not enough SOL in the curve".into(), "Too few buyers in the last 30 s".into()], 0, 0);
        db.insert_score(t, "E", "E", 91.0, false, &["Not enough SOL in the curve".into()], 14, 9);
        db.insert_score(t, "F", "F", 88.0, true, &[], 20, 11);
        let f = db.funnel(0, 10);
        assert_eq!((f.seen, f.blocked, f.scored, f.scored_no_trades, f.picked), (6, 3, 3, 1, 1));
        assert_eq!(f.best_score, 91.0);
        assert_eq!(f.best_symbol, "E");
        assert_eq!(f.rug_fallback_passed, 1);
        assert_eq!(f.blocked_reasons.len(), 3);
        // the gates behind the best coins that were NOT picked
        assert_eq!(f.best_blocked_by[0], ReasonCount { reason: "Not enough SOL in the curve".into(), count: 2 });
        assert_eq!(f.best_blocked_by[1].count, 1);
        // a later day starts from zero
        assert_eq!(db.funnel(t + 1, 10), FunnelCounts::default());
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
