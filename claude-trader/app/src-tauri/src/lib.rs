use engine::engine::{effective_config, run as run_engine, EngineOpts};
use engine::secrets::{ConnTest, KeyStatus, KeyStore};
use engine::update::{BuildInfo, UpdateCheck};
use engine::{hub::Hub, Config};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

mod startup;
use startup::KeepAwake;

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

// ---------------------------------------------------------------- Chrome extension and startup

fn open_db(paths: &AppPaths) -> Result<engine::db::Db, String> {
    engine::db::Db::open(&paths.db)
}

/// New one-time pairing code, shown once in the app. Valid 10 minutes, works once.
#[tauri::command]
fn ext_pair_start(paths: tauri::State<AppPaths>) -> Result<String, String> {
    engine::bridge::new_pair_code(&open_db(&paths)?)
}

#[tauri::command]
fn ext_unpair(paths: tauri::State<AppPaths>) -> Result<(), String> {
    engine::bridge::unpair(&open_db(&paths)?);
    Ok(())
}

#[tauri::command]
fn ext_paired(paths: tauri::State<AppPaths>) -> bool {
    open_db(&paths).map(|d| engine::bridge::is_paired(&d)).unwrap_or(false)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupView {
    #[serde(flatten)]
    settings: engine::startup::StartupSettings,
    autostart: bool,
    chrome_found: bool,
    extension_id: &'static str,
    extension_folder: String,
}

fn extension_folder(app: &tauri::AppHandle) -> std::path::PathBuf {
    // installed app: bundled next to the program. Development: the folder in the source tree.
    if let Ok(r) = app.path().resource_dir() {
        let p = r.join("extension");
        if p.join("manifest.json").is_file() {
            return p;
        }
    }
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extension"))
}

#[tauri::command]
fn get_startup(app: tauri::AppHandle, paths: tauri::State<AppPaths>) -> Result<StartupView, String> {
    let settings = engine::startup::StartupSettings::load(&open_db(&paths)?);
    Ok(StartupView {
        settings,
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        chrome_found: engine::startup::find_chrome().is_some(),
        extension_id: engine::bridge::EXTENSION_ID,
        extension_folder: extension_folder(&app).to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn set_startup(settings: engine::startup::StartupSettings, paths: tauri::State<AppPaths>, awake: tauri::State<KeepAwake>) -> Result<(), String> {
    settings.save(&open_db(&paths)?);
    awake.set(settings.keep_awake);
    Ok(())
}

#[tauri::command]
fn set_autostart(on: bool, app: tauri::AppHandle) -> Result<bool, String> {
    let m = app.autolaunch();
    if on { m.enable() } else { m.disable() }.map_err(|e| format!("Could not change this: {e}"))?;
    m.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn open_chrome_now(paths: tauri::State<AppPaths>) -> Result<String, String> {
    Ok(startup::open_browser(&engine::startup::StartupSettings::load(&open_db(&paths)?)))
}

#[tauri::command]
fn open_extension_folder(app: tauri::AppHandle) -> Result<(), String> {
    startup::show_folder(&extension_folder(&app))
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![])))
        .manage(KeepAwake::default())
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
            learning_command,
            ext_pair_start,
            ext_unpair,
            ext_paired,
            get_startup,
            set_startup,
            set_autostart,
            open_chrome_now,
            open_extension_folder
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
            let mut opts = opts.with_saved_key();
            opts.bridge_addr = Some(format!("127.0.0.1:{}", engine::bridge::DEFAULT_PORT));
            // startup choices: keep the PC awake, and start Chrome (unless the extension is already connected, which means Chrome is open)
            let st = engine::startup::StartupSettings::load(&engine::db::Db::open(&db_path).expect("database opens"));
            handle.state::<KeepAwake>().set(st.keep_awake);
            if st.open_chrome {
                let hub3 = hub.clone();
                let db3 = db_path.clone();
                tauri::async_runtime::spawn(async move {
                    // give an already running Chrome 4 seconds to connect its extension; if it does, Chrome is open and nothing is started
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                    let up = hub3.snapshot().iter().any(|e| matches!(e, engine::EngineEvent::Extension { connected: true, .. }));
                    if !up {
                        if let Ok(db) = engine::db::Db::open(&db3) {
                            let msg = startup::open_browser(&engine::startup::StartupSettings::load(&db));
                            hub3.log(format!("startup: {msg}"));
                        }
                    }
                });
            }
            let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
            app.manage(stop_tx); // keep the sender alive for the life of the app
            tauri::async_runtime::spawn(async move {
                if let Err(e) = run_engine(opts, hub, stop_rx).await {
                    eprintln!("engine stopped: {e}");
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Claude Trader");
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            handle.state::<KeepAwake>().set(false); // let Windows sleep again
        }
    });
}
