//! Paper trade log (one JSONL line per closed trade, SPEC 8 shape) and the daily summary.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// EXACTLY the SPEC 8 shape: same keys, same order (`feeBreakdown` is the last key). Do not add or reorder fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TradeLine {
    pub ts: String,
    pub mint: String,
    pub symbol: String,
    pub side: String,
    pub size_sol: f64,
    pub entry_price: f64,
    pub entry_mcap_usd: i64,
    pub exit_price: f64,
    pub exit_reason: String,
    pub pnl_sol: f64,
    pub pnl_pct: f64,
    pub hold_seconds: i64,
    pub fees_sol: f64,
    pub mode: String,
    /// Every fee component of the trade, SOL (SPEC 8). Their sum is `fees_sol`.
    pub fee_breakdown: crate::curve::FeeBreakdown,
}

fn r9(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

impl TradeLine {
    pub fn from_position(p: &crate::position::Position) -> TradeLine {
        let exit_ms = p.exit_ms.unwrap_or(p.entry_ms);
        TradeLine {
            ts: crate::timefmt::iso_local(exit_ms),
            mint: p.mint.clone(),
            symbol: p.symbol.clone(),
            side: "buy".into(),
            size_sol: r9(p.size_sol),
            entry_price: p.entry_price,
            entry_mcap_usd: p.entry_mcap_usd.round() as i64,
            exit_price: p.avg_exit_price(),
            exit_reason: p.exit_reason.clone(),
            pnl_sol: r9(p.pnl_sol),
            pnl_pct: (p.pnl_pct * 100.0).round() / 100.0,
            hold_seconds: ((exit_ms - p.entry_ms) as f64 / 1000.0).round() as i64,
            fees_sol: r9(p.fees.total()),
            mode: "paper".into(),
            fee_breakdown: p.fees,
        }
    }
}

/// Append the line to `<dir>/<YYYY-MM-DD>.jsonl` (momentum) or `<dir>/<style>/<YYYY-MM-DD>.jsonl` (variants).
pub fn append_line(dir: &Path, lane: &str, exit_ms: i64, line: &TradeLine) -> Result<std::path::PathBuf, String> {
    let d = if lane == "momentum" { dir.to_path_buf() } else { dir.join(lane) };
    std::fs::create_dir_all(&d).map_err(|e| format!("create {}: {e}", d.display()))?;
    let path = d.join(format!("{}.jsonl", crate::timefmt::date_local(exit_ms)));
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| format!("open {}: {e}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(line).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn read_lines(path: &Path) -> Vec<TradeLine> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub date: String,
    pub trades: u32,
    pub wins: u32,
    pub losses: u32,
    pub win_rate_pct: f64,
    pub avg_win_sol: f64,
    pub avg_loss_sol: f64,
    /// Gross wins / gross losses. `null` when there are no losing trades.
    pub profit_factor: Option<f64>,
    pub max_drawdown_sol: f64,
    pub total_pnl_sol: f64,
    pub trades_per_day: f64,
    pub best_sol: f64,
    pub worst_sol: f64,
    pub fees_sol: f64,
}

/// Win = pnlSol above 0. Max drawdown = largest fall of cumulative P/L below its running peak (peak starts at 0),
/// trades in file order. `days` is the number of distinct days covered (1 for a daily file).
pub fn summarize(date: &str, lines: &[TradeLine], days: f64) -> Summary {
    let n = lines.len() as u32;
    let wins: Vec<f64> = lines.iter().map(|l| l.pnl_sol).filter(|p| *p > 0.0).collect();
    let losses: Vec<f64> = lines.iter().map(|l| l.pnl_sol).filter(|p| *p <= 0.0).collect();
    let gw: f64 = wins.iter().sum();
    let gl: f64 = -losses.iter().sum::<f64>();
    let (mut cum, mut peak, mut dd) = (0.0f64, 0.0f64, 0.0f64);
    for l in lines {
        cum += l.pnl_sol;
        peak = peak.max(cum);
        dd = dd.max(peak - cum);
    }
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    Summary {
        date: date.into(),
        trades: n,
        wins: wins.len() as u32,
        losses: losses.len() as u32,
        win_rate_pct: if n > 0 { wins.len() as f64 / n as f64 * 100.0 } else { 0.0 },
        avg_win_sol: mean(&wins),
        avg_loss_sol: mean(&losses),
        profit_factor: if gl > 0.0 { Some(gw / gl) } else { None },
        max_drawdown_sol: dd,
        total_pnl_sol: lines.iter().map(|l| l.pnl_sol).sum(),
        trades_per_day: if days > 0.0 { n as f64 / days } else { 0.0 },
        best_sol: lines.iter().map(|l| l.pnl_sol).fold(0.0, f64::max),
        worst_sol: lines.iter().map(|l| l.pnl_sol).fold(0.0, f64::min),
        fees_sol: lines.iter().map(|l| l.fees_sol).sum(),
    }
}

pub fn summary_path(dir: &Path, lane: &str, date: &str) -> std::path::PathBuf {
    if lane == "momentum" { dir.join(format!("{date}-summary.json")) } else { dir.join(lane).join(format!("{date}-summary.json")) }
}

/// Recompute the summary of `date` from its JSONL file, save it as JSON and return it.
pub fn write_daily_summary(dir: &Path, lane: &str, date: &str) -> Result<Summary, String> {
    let jl = if lane == "momentum" { dir.join(format!("{date}.jsonl")) } else { dir.join(lane).join(format!("{date}.jsonl")) };
    let s = summarize(date, &read_lines(&jl), 1.0);
    let p = summary_path(dir, lane, date);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(s)
}

pub fn print_summary(lane: &str, s: &Summary) -> String {
    format!(
        "DAILY SUMMARY [{lane}] {}: trades={} winRate={:.1}% avgWin={:+.6} SOL avgLoss={:+.6} SOL profitFactor={} maxDrawdown={:.6} SOL totalPnL={:+.6} SOL fees={:.6} SOL tradesPerDay={:.1}",
        s.date,
        s.trades,
        s.win_rate_pct,
        s.avg_win_sol,
        s.avg_loss_sol,
        s.profit_factor.map(|p| format!("{p:.3}")).unwrap_or_else(|| "n/a (no losses)".into()),
        s.max_drawdown_sol,
        s.total_pnl_sol,
        s.fees_sol,
        s.trades_per_day
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example line from SPEC section 8, byte for byte.
    const SPEC_LINE: &str = r#"{"ts":"2026-11-25T15:02:11-08:00","mint":"7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgHR","symbol":"MOON","side":"buy","sizeSol":0.2,"entryPrice":0.00042,"entryMcapUsd":12400,"exitPrice":0.00081,"exitReason":"take-profit-1","pnlSol":0.176,"pnlPct":88.0,"holdSeconds":112,"feesSol":0.00551,"mode":"paper","feeBreakdown":{"pumpFun":0.0019,"creator":0.0006,"pumpPortal":0.002,"tx":0.00001,"priority":0.001,"jito":0.0}}"#;

    #[test]
    fn jsonl_shape_matches_the_spec_example_exactly() {
        let l: TradeLine = serde_json::from_str(SPEC_LINE).unwrap();
        assert_eq!(serde_json::to_string(&l).unwrap(), SPEC_LINE, "same keys, same order, same number formats");
    }

    #[test]
    fn a_real_position_serialises_with_exactly_the_spec_keys_in_order() {
        use crate::position::*;
        let mut p = Position::new_buying("momentum", "MINT", "TST", "C", "CV", 0.1, 1_800_000_000_000, 150.0);
        p.fill_buy(1.0e-7, 1_000_000.0, 7000.4, &Default::default(), 0.0).unwrap();
        p.begin_sell().unwrap();
        let st = StageSell { ts: 1_800_000_112_000, reason: "take-profit-1".into(), tokens: 1_000_000.0, price: 1.2e-7, gross_sol: 0.12, net_sol: 0.1176, fees_sol: 0.0024 };
        p.end_sell(Some((st, Default::default())), &Default::default(), 1_800_000_112_000, "take-profit-1").unwrap();
        let line = TradeLine::from_position(&p);
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&line).unwrap()).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        let spec: serde_json::Value = serde_json::from_str(SPEC_LINE).unwrap();
        let spec_keys: Vec<&str> = spec.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(keys, spec_keys);
        assert_eq!(line.hold_seconds, 112);
        assert_eq!(line.entry_mcap_usd, 7000);
        assert!((line.pnl_sol - 0.0176).abs() < 1e-9);
        assert_eq!(line.mode, "paper");
    }

    fn l(pnl: f64, fees: f64) -> TradeLine {
        let mut x: TradeLine = serde_json::from_str(SPEC_LINE).unwrap();
        x.pnl_sol = pnl;
        x.fees_sol = fees;
        x
    }

    #[test]
    fn summary_math() {
        // P/L in order: +0.10, -0.04, +0.02, -0.06, -0.03, +0.05
        let lines: Vec<TradeLine> = [0.10, -0.04, 0.02, -0.06, -0.03, 0.05].iter().map(|p| l(*p, 0.004)).collect();
        let s = summarize("2026-11-26", &lines, 1.0);
        assert_eq!((s.trades, s.wins, s.losses), (6, 3, 3));
        assert!((s.win_rate_pct - 50.0).abs() < 1e-9);
        assert!((s.avg_win_sol - 0.17 / 3.0).abs() < 1e-12);
        assert!((s.avg_loss_sol - (-0.13 / 3.0)).abs() < 1e-12);
        assert!((s.profit_factor.unwrap() - 0.17 / 0.13).abs() < 1e-12);
        // cumulative: .10 .06 .08 .02 -.01 .04 ; peak .10 ; trough -.01 -> drawdown .11
        assert!((s.max_drawdown_sol - 0.11).abs() < 1e-12);
        assert!((s.total_pnl_sol - 0.04).abs() < 1e-12);
        assert!((s.fees_sol - 0.024).abs() < 1e-12);
        assert_eq!(s.trades_per_day, 6.0);
        assert!((s.best_sol - 0.10).abs() < 1e-12 && (s.worst_sol + 0.06).abs() < 1e-12);
        // no losses -> profit factor is null, empty day is all zeros
        assert_eq!(summarize("d", &[l(0.1, 0.0)], 1.0).profit_factor, None);
        assert_eq!(summarize("d", &[], 1.0).trades, 0);
    }

    #[test]
    fn append_and_daily_summary_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ct-report-{}", crate::rpc::now_ms()));
        let ms = 1_800_000_000_000;
        append_line(&dir, "momentum", ms, &l(0.05, 0.004)).unwrap();
        append_line(&dir, "momentum", ms, &l(-0.02, 0.004)).unwrap();
        append_line(&dir, "dip-40pct", ms, &l(-0.09, 0.004)).unwrap();
        let date = crate::timefmt::date_local(ms);
        let s = write_daily_summary(&dir, "momentum", &date).unwrap();
        assert_eq!(s.trades, 2);
        assert!(dir.join(format!("{date}-summary.json")).exists());
        assert_eq!(write_daily_summary(&dir, "dip-40pct", &date).unwrap().trades, 1, "variant is tracked in its own file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
