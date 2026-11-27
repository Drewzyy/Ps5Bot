//! engine-cli: run the Claude Trader pipeline headless.
//!   engine-cli                       print the loaded config (M1 behaviour)
//!   engine-cli kill|resume|kill-state --db PATH   kill switch (stops buying, sells everything; resume is separate)
//!   engine-cli feed [--minutes N] [--ws-port P] [--db PATH] [--rpc URL] [--config PATH] [--relax-metadata-egress]
//! Logs to stdout, writes every decision to the SQLite `decisions` table, and (with --ws-port) serves the
//! events on ws://127.0.0.1:P for the browser dashboard.

use engine::engine::{run, EngineOpts};
use engine::events::EngineEvent;
use engine::hub::Hub;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn ts(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms.rem_euclid(1000))
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn line(e: &EngineEvent) -> Option<String> {
    Some(match e {
        EngineEvent::Token { ts: t, mint, symbol, name, mcap_sol, .. } => format!("{} NEW-TOKEN  mint={mint} symbol={symbol} name={name:?} mcapSol={mcap_sol:.2}", ts(*t)),
        EngineEvent::Verdict { ts: t, mint, symbol, stage, outcome, reason, detail, verdict, .. } => {
            if outcome == "block" {
                format!("{} BLOCK       {symbol} {mint} stage={stage} reason={reason} verdict={verdict} | {detail}", ts(*t))
            } else {
                format!("{} PASS        {symbol} {mint} stage={stage} verdict={verdict} | {detail}", ts(*t))
            }
        }
        EngineEvent::Score { ts: t, mint, symbol, score, picked, buyers, buys, sells, ratio, price_change_pct, curve_sol, gates, .. } => format!(
            "{} SCORE       {symbol} {mint} score={score:.1} picked={picked} buyers={buyers} buys={buys} sells={sells} ratio={ratio:.2} priceChg={price_change_pct:.1}% curveSol={curve_sol:.1} gates={gates:?}",
            ts(*t)
        ),
        EngineEvent::Pulse { from, to, text } => format!("{} HANDOFF     {from:?} -> {to:?}: {text}", ts(engine::rpc::now_ms())),
        _ => return None,
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let default_cfg = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../config.json"));
    // kill switch from the command line: writes the persisted state; a running engine on the same db picks it up within 1 s
    if matches!(args.get(1).map(|s| s.as_str()), Some("kill") | Some("resume") | Some("kill-state")) {
        let db_path = arg(&args, "--db").unwrap_or_else(|| "claude-trader.db".into());
        let db = engine::db::Db::open(&db_path).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        match args[1].as_str() {
            "kill" => {
                if db.killed_at().is_none() {
                    db.kv_set("killed", &engine::rpc::now_ms().to_string());
                }
                println!("kill switch ON (saved in {db_path}). A running engine stops buying and sells every open position; a restart stays stopped.");
            }
            "resume" => {
                db.kv_del("killed");
                println!("kill switch OFF: trading may resume.");
            }
            _ => println!("kill switch is {}", if db.killed_at().is_some() { "ON" } else { "off" }),
        }
        return;
    }
    if args.get(1).map(|s| s.as_str()) != Some("feed") {
        let path = args.get(1).map(PathBuf::from).unwrap_or(default_cfg);
        println!("engine ok");
        match engine::Config::load(&path) {
            Ok(c) => println!("{}", serde_json::to_string_pretty(&c).unwrap()),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let cfg_path = arg(&args, "--config").map(PathBuf::from).unwrap_or(default_cfg);
    let cfg = engine::Config::load(&cfg_path).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let minutes: Option<f64> = arg(&args, "--minutes").and_then(|m| m.parse().ok());
    let db_path = arg(&args, "--db").unwrap_or_else(|| "claude-trader.db".into());
    let mut opts = EngineOpts::new(cfg, &db_path);
    if let Some(r) = arg(&args, "--rpc") {
        opts.rpc_url = r;
        opts.rpc_pinned = true; // an explicit --rpc is never replaced by a saved key
    }
    opts.relax_egress = args.iter().any(|a| a == "--relax-metadata-egress");
    if let Some(l) = arg(&args, "--logs") {
        opts.logs_dir = l;
    }
    opts.clear_halt = args.iter().any(|a| a == "--clear-halt");
    let opts = opts.with_saved_key();
    let mut opts = opts;
    opts.replay = arg(&args, "--replay");
    opts.sol_usd = arg(&args, "--sol-usd").and_then(|v| v.parse().ok());
    if opts.replay.is_some() && opts.sol_usd.is_none() {
        // the $7000 market-cap gate and the 3x chase rule only fit together near $150 per SOL (see STATUS.md)
        opts.sol_usd = Some(150.0);
    }

    let hub = Hub::new();
    hub.set_logger(Arc::new(|l| println!("{} LOG          {l}", ts(engine::rpc::now_ms()))));
    if let Some(p) = arg(&args, "--ws-port") {
        match hub.clone().serve(&format!("127.0.0.1:{p}")).await {
            Ok(a) => println!("{} LOG          events served on ws://{a}", ts(engine::rpc::now_ms())),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
    }
    if opts.relax_egress {
        println!("{} LOG          WARNING: --relax-metadata-egress is on (sandbox only): unreachable metadata hosts are flagged, not blocked", ts(engine::rpc::now_ms()));
    }
    let mut rx = hub.subscribe();
    let printer = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(e) => {
                    if let Some(l) = line(&e) {
                        println!("{l}");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => println!("LOG          (log lagged, {n} lines dropped)"),
                Err(_) => return,
            }
        }
    });

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let h2 = hub.clone();
    let db_for_summary = db_path.clone();
    let logs_dir = opts.logs_dir.clone();
    let engine_task = tokio::spawn(run(opts, h2, stop_rx));
    match minutes {
        Some(m) => tokio::time::sleep(Duration::from_secs_f64(m * 60.0)).await,
        None => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    let _ = stop_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), engine_task).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    printer.abort();
    if let Ok(db) = engine::db::Db::open(&db_for_summary) {
        let (checked, blocked, passed, by) = db.summary(0);
        let scored = db.scalar("SELECT COUNT(DISTINCT mint) FROM decisions WHERE stage='observation'");
        let picked = db.scalar("SELECT COUNT(DISTINCT mint) FROM decisions WHERE stage='observation' AND outcome='pass'");
        let seen = db.scalar("SELECT COUNT(*) FROM tokens_seen");
        let entered = db.scalar("SELECT COUNT(*) FROM positions WHERE kind='trade'");
        let failed = db.scalar("SELECT COUNT(*) FROM positions WHERE kind='failed-buy'");
        let gated = db.scalar("SELECT COUNT(*) FROM decisions WHERE stage='risk-gate'");
        println!("{} SUMMARY      coinsSeen={seen} checked={checked} blocked={blocked} passed={passed} scored={scored} picked={picked} riskGateBlocks={gated} entered={entered} failedBuys={failed} decisions={}", ts(engine::rpc::now_ms()), db.decision_count());
        let date = engine::timefmt::date_local(engine::rpc::now_ms());
        for lane in ["momentum", "dip-40pct"] {
            if let Ok(s) = engine::report::write_daily_summary(std::path::Path::new(&logs_dir), lane, &date) {
                println!("{} {}", ts(engine::rpc::now_ms()), engine::report::print_summary(lane, &s));
            }
        }
        for r in by {
            println!("{} SUMMARY      block-reason {} x{}", ts(engine::rpc::now_ms()), r.reason, r.count);
        }
    }
}
