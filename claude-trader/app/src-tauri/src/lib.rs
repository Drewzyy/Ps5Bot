use engine::engine::{run as run_engine, EngineOpts};
use engine::{hub::Hub, Config};
use tauri::{Emitter, Manager};

const DEFAULT_CONFIG: &str = include_str!("../../../config.json");

/// Returns the strategy config as JSON. M1 is read-only: it ships with the app.
#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let c = Config::from_json(DEFAULT_CONFIG)?;
    serde_json::to_value(c).map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_config])
        .setup(|app| {
            let handle = app.handle().clone();
            // Live engine (M2): Memecoin Monitor -> Scam Shield -> Token Picker. Paper only.
            let db_path = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|d| std::fs::create_dir_all(&d).ok().map(|_| d.join("claude-trader.db")))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "claude-trader.db".to_string());
            let hub = Hub::new();
            let mut rx = hub.subscribe();
            let h2 = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(e) => {
                            let _ = h2.emit("engine-event", e);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            let cfg = Config::from_json(DEFAULT_CONFIG).expect("bundled config.json is valid");
            let opts = EngineOpts::new(cfg, &db_path);
            let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
            app.manage(stop_tx); // keep the sender alive for the life of the app
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_engine(opts, hub, stop_rx).await {
                    eprintln!("engine stopped: {e}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Trader");
}
