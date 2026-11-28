//! Raw feed recorder (SPEC 4f). Every decoded trade and every launch the engine sees is appended to a compressed daily
//! file in the app data folder, so a strategy can be replayed offline against exactly what the live feed showed.
//!
//! File: `recordings/feed-YYYY-MM-DD.jsonl.gz` (UTC day of the record). One JSON object per line:
//! - `{"k":"launch","ms":..,"mint","symbol","name","creator","curve","sig","vSol","vTok","initialBuy","mayhem","quote","uri"}`
//! - `{"k":"trade","ms":..,"mint","user","buy","sol","tok","slot","sig","vSol","vTok","realSol","mayhem","nonSol","holder"}`
//! - `{"k":"gap","ms":..,"from":..,"to":..,"why":".."}`  the feed was down or silent between `from` and `to`: trades in that
//!   span may be missing (backfilled ones are recorded as normal trades), so a replay must not trust coins that span it.
//!
//! Lines are buffered and appended as one gzip member per flush (concatenated gzip members are one valid gzip stream), so a crash
//! loses at most a few seconds. Retention: files older than `retentionDays` are deleted, then the oldest files go until the
//! folder is under `maxMegabytes`. Nothing here touches the network or any key.

use crate::config::RecorderCfg;
use crate::pump::{FeedMsg, Trade};
use flate2::{read::MultiGzDecoder, write::GzEncoder, Compression};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FLUSH_LINES: usize = 400;

pub struct Recorder {
    dir: PathBuf,
    cfg: Mutex<RecorderCfg>,
    buf: Mutex<Vec<(i64, String)>>,
    pub lines_written: std::sync::atomic::AtomicU64,
}

impl Recorder {
    pub fn new(dir: PathBuf, cfg: RecorderCfg) -> Arc<Recorder> {
        Arc::new(Recorder { dir, cfg: Mutex::new(cfg), buf: Mutex::new(vec![]), lines_written: Default::default() })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn set_cfg(&self, cfg: RecorderCfg) {
        *self.cfg.lock().unwrap() = cfg;
    }

    pub fn cfg(&self) -> RecorderCfg {
        self.cfg.lock().unwrap().clone()
    }

    fn push(&self, ms: i64, v: Value) {
        if !self.cfg().enabled {
            return;
        }
        let n = {
            let mut b = self.buf.lock().unwrap();
            b.push((ms, v.to_string()));
            b.len()
        };
        if n >= FLUSH_LINES {
            self.flush();
        }
    }

    pub fn record_trade(&self, t: &Trade) {
        self.push(t.recv_ms, json!({"k":"trade","ms":t.recv_ms,"mint":t.mint,"user":t.user,"buy":t.is_buy,"sol":t.sol,"tok":t.tokens,"slot":t.slot,"sig":t.signature,"vSol":t.v_sol,"vTok":t.v_tokens,"realSol":t.real_sol,"mayhem":t.mayhem,"nonSol":t.non_sol_quote,"holder":t.holder_reward}));
    }

    pub fn record_launch(&self, ms: i64, m: &FeedMsg) {
        self.push(ms, json!({"k":"launch","ms":ms,"mint":m.mint,"symbol":m.symbol,"name":m.name,"creator":m.trader_public_key,"curve":m.bonding_curve_key,"sig":m.signature,"vSol":m.v_sol_in_bonding_curve,"vTok":m.v_tokens_in_bonding_curve,"initialBuy":m.initial_buy,"mayhem":m.is_mayhem_mode,"quote":m.quote_mint,"uri":m.uri}));
    }

    pub fn record_gap(&self, from_ms: i64, to_ms: i64, why: &str) {
        self.push(to_ms, json!({"k":"gap","ms":to_ms,"from":from_ms,"to":to_ms,"why":why}));
    }

    /// Append everything buffered. Returns the number of lines written. Errors are swallowed on purpose: a full disk must
    /// never stop trading (the lines are dropped and the next flush tries again).
    pub fn flush(&self) -> usize {
        let lines: Vec<(i64, String)> = std::mem::take(&mut *self.buf.lock().unwrap());
        if lines.is_empty() {
            return 0;
        }
        let mut by_day: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
        for (ms, l) in &lines {
            by_day.entry(crate::timefmt::date_utc(*ms)).or_default().push(l);
        }
        if std::fs::create_dir_all(&self.dir).is_err() {
            return 0;
        }
        let mut n = 0;
        for (day, ls) in by_day {
            let path = self.dir.join(format!("feed-{day}.jsonl.gz"));
            let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else { continue };
            let mut gz = GzEncoder::new(f, Compression::default());
            let mut ok = true;
            for l in &ls {
                ok &= gz.write_all(l.as_bytes()).is_ok() && gz.write_all(b"\n").is_ok();
            }
            if ok && gz.finish().is_ok() {
                n += ls.len();
            }
        }
        self.lines_written.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        n
    }

    /// Delete old files (`retentionDays`) and then the oldest ones until the folder fits `maxMegabytes`. Returns files deleted.
    pub fn prune(&self, now_ms: i64) -> usize {
        let cfg = self.cfg();
        let mut files = list_files(&self.dir);
        files.sort();
        let mut deleted = 0;
        let cutoff = crate::timefmt::date_utc(now_ms - cfg.retention_days as i64 * 86_400_000);
        files.retain(|(day, p, _)| {
            if day.as_str() < cutoff.as_str() {
                if std::fs::remove_file(p).is_ok() {
                    deleted += 1;
                }
                false
            } else {
                true
            }
        });
        let limit = cfg.max_megabytes as u64 * 1_048_576;
        let mut total: u64 = files.iter().map(|f| f.2).sum();
        for (_, p, size) in &files {
            if total <= limit {
                break;
            }
            if std::fs::remove_file(p).is_ok() {
                total -= size;
                deleted += 1;
            }
        }
        deleted
    }

    /// Flush every few seconds and prune once an hour. Stops when the process ends.
    pub fn spawn_flusher(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut n = 0u64;
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tick.tick().await;
                me.flush();
                n += 1;
                if n % 720 == 1 {
                    me.prune(crate::rpc::now_ms());
                }
            }
        });
    }

    /// (files, bytes, newest day) for the Settings line.
    pub fn disk_use(&self) -> (usize, u64, String) {
        let mut f = list_files(&self.dir);
        f.sort();
        (f.len(), f.iter().map(|x| x.2).sum(), f.last().map(|x| x.0.clone()).unwrap_or_default())
    }
}

/// (day, path, bytes) of every recording file in `dir`.
pub fn list_files(dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let day = name.strip_prefix("feed-")?.strip_suffix(".jsonl.gz")?.to_string();
            Some((day, e.path(), e.metadata().ok()?.len()))
        })
        .collect()
}

/// A recording read back for an offline replay.
#[derive(Debug, Default, Clone)]
pub struct Recording {
    /// (recorded at, launch message), oldest first.
    pub launches: Vec<(i64, FeedMsg)>,
    /// Trades per mint, oldest first (by slot then receive time), duplicates removed.
    pub trades: HashMap<String, Vec<Trade>>,
    /// (from, to, why) spans where the feed was down or silent.
    pub gaps: Vec<(i64, i64, String)>,
    pub files: usize,
    pub bad_lines: usize,
}

impl Recording {
    /// Add one JSON line (as written by the recorder).
    pub fn add_line(&mut self, line: &str) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            self.bad_lines += 1;
            return;
        };
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
        match v["k"].as_str() {
            Some("trade") => {
                let t = Trade {
                    mint: s("mint"),
                    user: s("user"),
                    is_buy: v["buy"].as_bool().unwrap_or(false),
                    sol: f("sol"),
                    tokens: f("tok"),
                    slot: v["slot"].as_u64().unwrap_or(0),
                    signature: s("sig"),
                    recv_ms: v["ms"].as_i64().unwrap_or(0),
                    v_sol: f("vSol"),
                    v_tokens: f("vTok"),
                    real_sol: f("realSol"),
                    mayhem: v["mayhem"].as_bool().unwrap_or(false),
                    non_sol_quote: v["nonSol"].as_bool().unwrap_or(false),
                    holder_reward: v["holder"].as_bool().unwrap_or(false),
                };
                if t.mint.is_empty() {
                    self.bad_lines += 1;
                } else {
                    self.trades.entry(t.mint.clone()).or_default().push(t);
                }
            }
            Some("launch") => {
                let m = FeedMsg {
                    mint: s("mint"),
                    symbol: s("symbol"),
                    name: s("name"),
                    trader_public_key: s("creator"),
                    bonding_curve_key: s("curve"),
                    signature: s("sig"),
                    v_sol_in_bonding_curve: f("vSol"),
                    v_tokens_in_bonding_curve: f("vTok"),
                    initial_buy: f("initialBuy"),
                    is_mayhem_mode: v["mayhem"].as_bool().unwrap_or(false),
                    quote_mint: s("quote"),
                    uri: s("uri"),
                    tx_type: "create".into(),
                    market_cap_sol: if f("vTok") > 0.0 { f("vSol") / f("vTok") * crate::pump::SUPPLY_TOKENS } else { 0.0 },
                    ..Default::default()
                };
                if m.mint.is_empty() {
                    self.bad_lines += 1;
                } else {
                    self.launches.push((v["ms"].as_i64().unwrap_or(0), m));
                }
            }
            Some("gap") => self.gaps.push((v["from"].as_i64().unwrap_or(0), v["to"].as_i64().unwrap_or(0), s("why"))),
            _ => self.bad_lines += 1,
        }
    }

    /// Sort, drop duplicate trades (backfill can repeat a live trade) and drop launches seen twice.
    pub fn finish(&mut self) {
        self.launches.sort_by_key(|l| l.0);
        let mut seen = std::collections::HashSet::new();
        self.launches.retain(|(_, m)| seen.insert(m.mint.clone()));
        for v in self.trades.values_mut() {
            let mut s = std::collections::HashSet::new();
            v.retain(|t| s.insert((t.signature.clone(), t.user.clone(), t.is_buy)));
            v.sort_by_key(|t| (t.slot, t.recv_ms));
        }
        self.gaps.sort();
    }
}

/// Read every recording file in `dir` whose day is within `[from_day, to_day]` (either may be empty = open).
pub fn read_dir(dir: &Path, from_day: &str, to_day: &str) -> Result<Recording, String> {
    let mut files = list_files(dir);
    files.sort();
    let mut rec = Recording::default();
    for (day, path, _) in files {
        if (!from_day.is_empty() && day.as_str() < from_day) || (!to_day.is_empty() && day.as_str() > to_day) {
            continue;
        }
        let f = std::fs::File::open(&path).map_err(|e| format!("could not open {}: {e}", path.display()))?;
        let mut text = String::new();
        // a file cut short by a crash still yields everything before the cut
        let _ = MultiGzDecoder::new(f).read_to_string(&mut text);
        for l in text.lines().filter(|l| !l.trim().is_empty()) {
            rec.add_line(l);
        }
        rec.files += 1;
    }
    rec.finish();
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ct-rec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn trade(ms: i64, sig: &str) -> Trade {
        Trade { mint: "MINT".into(), user: "U".into(), is_buy: true, sol: 1.5, tokens: 10.0, slot: (ms / 400) as u64, signature: sig.into(), recv_ms: ms, v_sol: 31.5, v_tokens: 1.0e9, real_sol: 1.5, holder_reward: true, ..Default::default() }
    }

    #[test]
    fn what_is_written_is_read_back_exactly_across_several_flushes() {
        let dir = tmp("roundtrip");
        let r = Recorder::new(dir.clone(), RecorderCfg::default());
        let day = 1_795_000_000_000i64; // a UTC day
        r.record_launch(day, &FeedMsg { mint: "MINT".into(), symbol: "AAA".into(), trader_public_key: "DEV".into(), bonding_curve_key: "CURVE".into(), v_sol_in_bonding_curve: 30.0, v_tokens_in_bonding_curve: 1.073e9, is_mayhem_mode: false, ..Default::default() });
        r.record_trade(&trade(day + 1000, "s1"));
        assert_eq!(r.flush(), 2);
        r.record_trade(&trade(day + 2000, "s2"));
        r.record_trade(&trade(day + 1000, "s1")); // a repeat (live feed and backfill both saw it)
        r.record_gap(day + 2500, day + 9000, "socket stalled");
        assert_eq!(r.flush(), 3);
        let rec = read_dir(&dir, "", "").unwrap();
        assert_eq!(rec.files, 1);
        assert_eq!(rec.launches.len(), 1);
        assert_eq!(rec.launches[0].1.symbol, "AAA");
        assert_eq!(rec.launches[0].1.trader_public_key, "DEV");
        let t = &rec.trades["MINT"];
        assert_eq!(t.len(), 2, "the repeated trade is dropped on read");
        assert_eq!(t[0], trade(day + 1000, "s1"));
        assert_eq!(rec.gaps, vec![(day + 2500, day + 9000, "socket stalled".to_string())]);
        assert_eq!(rec.bad_lines, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_land_in_the_file_of_their_utc_day_and_the_range_filter_works() {
        let dir = tmp("days");
        let r = Recorder::new(dir.clone(), RecorderCfg::default());
        let d1 = 1_795_000_000_000i64;
        r.record_trade(&trade(d1, "a"));
        r.record_trade(&trade(d1 + 86_400_000, "b"));
        r.flush();
        let mut names: Vec<String> = list_files(&dir).into_iter().map(|f| f.0).collect();
        names.sort();
        assert_eq!(names.len(), 2);
        assert_eq!(read_dir(&dir, &names[1], "").unwrap().trades["MINT"].len(), 1);
        assert_eq!(read_dir(&dir, "", &names[0]).unwrap().trades["MINT"].len(), 1);
        assert_eq!(read_dir(&dir, "", "").unwrap().trades["MINT"].len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_deletes_old_days_and_then_the_oldest_until_the_size_fits() {
        let dir = tmp("prune");
        std::fs::create_dir_all(&dir).unwrap();
        let now = 1_795_000_000_000i64;
        for back in [0i64, 1, 5, 30] {
            let day = crate::timefmt::date_utc(now - back * 86_400_000);
            std::fs::write(dir.join(format!("feed-{day}.jsonl.gz")), vec![0u8; 600_000]).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "not ours").unwrap();
        let r = Recorder::new(dir.clone(), RecorderCfg { enabled: true, retention_days: 14, max_megabytes: 2000 });
        assert_eq!(r.prune(now), 1, "only the 30 day old file is past retention");
        assert_eq!(list_files(&dir).len(), 3);
        r.set_cfg(RecorderCfg { enabled: true, retention_days: 14, max_megabytes: 1 }); // 1 MB holds only one 600 KB file
        assert_eq!(r.prune(now), 2, "the two oldest go until the folder fits");
        let left = list_files(&dir);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].0, crate::timefmt::date_utc(now));
        assert!(dir.join("notes.txt").exists(), "files that are not recordings are never touched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_disabled_recorder_writes_nothing_and_a_cut_file_is_still_read() {
        let dir = tmp("off");
        let r = Recorder::new(dir.clone(), RecorderCfg { enabled: false, retention_days: 14, max_megabytes: 10 });
        r.record_trade(&trade(1_795_000_000_000, "a"));
        assert_eq!(r.flush(), 0);
        assert!(!dir.exists());
        r.set_cfg(RecorderCfg::default());
        r.record_trade(&trade(1_795_000_000_000, "a"));
        r.record_trade(&trade(1_795_000_001_000, "b"));
        r.flush();
        let p = list_files(&dir)[0].1.clone();
        let mut bytes = std::fs::read(&p).unwrap();
        bytes.truncate(bytes.len() - 6); // a crash cut the tail
        std::fs::write(&p, bytes).unwrap();
        let rec = read_dir(&dir, "", "").unwrap();
        assert!(rec.trades.get("MINT").map(|v| v.len()).unwrap_or(0) <= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
