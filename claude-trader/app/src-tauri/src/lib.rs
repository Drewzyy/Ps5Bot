use engine::engine::{effective_config, run as run_engine, EngineOpts};
use engine::secrets::{ConnTest, KeyStatus, KeyStore};
use engine::update::{BuildInfo, UpdateCheck};
use engine::{hub::Hub, Config};
use std::sync::Arc;
use tauri::{Emitter, Manager};

const DEFAULT_CONFIG: &str = include_str!("../../../config.json");

/// Where this app keeps its database and (only if the OS vault fails) the encrypted key file.
struct AppPaths {
    db: String,
    dir: std::path::PathBuf,
}

fn store(p: &AppPaths) -> KeyStore {
    KeyStore::system(p.dir.clone())
}

/// Returns the strategy config as JSON: shipped values, the paper fill choice saved in Settings, and applied learning changes.
#[tauri::command]
fn get_config(paths: tauri::State<AppPaths>) -> Result<serde_json::Value, String> {
    let c = Config::from_json(DEFAULT_CONFIG)?;
    let c = match engine::db::Db::open(&paths.db) {
        Ok(db) => effective_config(&c, &Arc::new(db)),
        Err(_) => c,
    };
    serde_json::to_value(c).map_err(|e| e.to_string())
}

/// Kill switch: stop all trading and sell every open position. Same code path as live mode will use.
#[tauri::command]
fn kill_switch(hub: tauri::State<Arc<Hub>>) -> String {
    hub.command("kill")
}

/// Separate explicit step that lets trading start again after a kill.
#[tauri::command]
fn resume_trading(hub: tauri::State<Arc<Hub>>) -> String {
    hub.command("resume")
}

// ---------------------------------------------------------------- Helius key (the key never comes back to the page)

#[tauri::command]
fn helius_status(paths: tauri::State<AppPaths>) -> KeyStatus {
    store(&paths).status()
}

#[tauri::command]
fn save_helius_key(key: String, paths: tauri::State<AppPaths>, hub: tauri::State<Arc<Hub>>) -> Result<KeyStatus, String> {
    let s = store(&paths);
    let where_ = s.save(&key)?;
    hub.log(format!("settings: Helius key saved in {where_}"));
    hub.command("reload-rpc");
    Ok(s.status())
}

#[tauri::command]
fn remove_helius_key(paths: tauri::State<AppPaths>, hub: tauri::State<Arc<Hub>>) -> Result<KeyStatus, String> {
    let s = store(&paths);
    s.remove()?;
    hub.log("settings: Helius key removed".to_string());
    hub.command("reload-rpc");
    Ok(s.status())
}

/// Test the key typed in the box, or the saved one when `key` is empty.
#[tauri::command]
async fn test_helius_key(key: Option<String>, paths: tauri::State<'_, AppPaths>) -> Result<ConnTest, String> {
    let k = match key.filter(|k| !k.trim().is_empty()) {
        Some(k) => k,
        None => store(&paths).effective_key().ok_or("No key saved yet.")?,
    };
    Ok(engine::secrets::test_helius(&k).await)
}

// ---------------------------------------------------------------- updates and links

#[tauri::command]
fn build_info() -> BuildInfo {
    engine::update::build_info()
}

#[tauri::command]
async fn check_for_updates() -> Result<UpdateCheck, String> {
    engine::update::check_for_updates().await
}

/// Open a web page in the user's browser. http and https only.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !engine::update::is_safe_web_link(&url) {
        return Err("That is not a normal web link.".into());
    }
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(&url).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| format!("Could not open the browser: {e}"))
}

// ---------------------------------------------------------------- settings and learning

/// `{"preset":"harsh"}` or the five numbers. Takes effect on the next trade, no restart.
#[tauri::command]
fn set_paper_fill(values: String, hub: tauri::State<Arc<Hub>>) -> String {
    hub.command(&format!("set-paper-fill:{values}"))
}

/// `undo-change:ID` or `approve-change:ID`.
#[tauri::command]
fn learning_command(cmd: String, hub: tauri::State<Arc<Hub>>) -> String {
    hub.command(&cmd)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_config,
            kill_switch,
            resume_trading,
            helius_status,
            save_helius_key,
            remove_helius_key,
            test_helius_key,
            build_info,
            check_for_updates,
            open_link,
            set_paper_fill,
            learning_command
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Live engine: Memecoin Monitor -> Scam Shield -> Token Picker -> paper trader. Paper only.
            let dir = app.path().app_data_dir().ok().filter(|d| std::fs::create_dir_all(d).is_ok()).unwrap_or_else(|| std::path::PathBuf::from("."));
            let db_path = dir.join("claude-trader.db").to_string_lossy().to_string();
            app.manage(AppPaths { db: db_path.clone(), dir: dir.clone() });
            let hub = Hub::new();
            app.manage(hub.clone());
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
            let mut opts = EngineOpts::new(cfg, &db_path);
            opts.secrets_dir = Some(dir.to_string_lossy().to_string());
            // the Helius key saved in secure storage (the HELIUS_API_KEY / CT_RPC_URL environment values still override it)
            let opts = opts.with_saved_key();
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
