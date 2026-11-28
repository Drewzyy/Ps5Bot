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

/// The app version is the one in tauri.conf.json (CI sets it to 0.1.<run number> for every build).
#[tauri::command]
fn build_info(app: tauri::AppHandle) -> BuildInfo {
    let mut b = engine::update::build_info();
    b.version = app.package_info().version.to_string();
    b
}

/// Step before installing an update: stop new entries and sell open paper positions through the normal exit path
/// (NOT the kill switch: nothing persistent is set). Waits up to 2 minutes; returns how many are still open.
#[tauri::command]
async fn prepare_update(hub: tauri::State<'_, Arc<Hub>>) -> Result<u32, String> {
    hub.command("update-close");
    for _ in 0..120 {
        let n: u32 = hub.command("open-count").parse().unwrap_or(0);
        if n == 0 {
            return Ok(0);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Ok(hub.command("open-count").parse().unwrap_or(0))
}

/// The update was cancelled or failed: trading carries on.
#[tauri::command]
fn cancel_update(hub: tauri::State<Arc<Hub>>) -> String {
    hub.command("update-close-cancel")
}

// ---------------------------------------------------------------- Obsidian vault connector

// ---------------------------------------------------------------- outgoing connectors (Discord, generic webhook)
// Secrets go to the OS secure storage and never come back to the page: the page only gets masked hints.

fn conn_of(name: &str) -> Result<engine::notify::Conn, String> {
    engine::notify::Conn::parse(name).ok_or_else(|| "Unknown connector.".to_string())
}

fn is_paper() -> bool {
    Config::from_json(DEFAULT_CONFIG).map(|c| c.mode == engine::config::Mode::Paper).unwrap_or(true)
}

#[tauri::command]
fn notify_status(paths: tauri::State<AppPaths>, sec: tauri::State<Arc<engine::notify::ConnSecrets>>) -> Result<Vec<engine::notify::ConnStatus>, String> {
    let db = open_db(&paths)?;
    let now = engine::rpc::now_ms();
    Ok(engine::notify::Conn::ALL.iter().map(|c| engine::notify::status(&db, &sec, *c, now)).collect())
}

#[tauri::command]
fn notify_save(conn: String, fields: std::collections::BTreeMap<String, String>, paths: tauri::State<AppPaths>, sec: tauri::State<Arc<engine::notify::ConnSecrets>>, hub: tauri::State<Arc<Hub>>) -> Result<engine::notify::ConnStatus, String> {
    let c = conn_of(&conn)?;
    let db = open_db(&paths)?;
    engine::notify::save(&db, &sec, c, &fields)?;
    hub.log(format!("settings: {} connector saved", c.name()));
    Ok(engine::notify::status(&db, &sec, c, engine::rpc::now_ms()))
}

#[tauri::command]
fn notify_remove(conn: String, paths: tauri::State<AppPaths>, sec: tauri::State<Arc<engine::notify::ConnSecrets>>, hub: tauri::State<Arc<Hub>>) -> Result<engine::notify::ConnStatus, String> {
    let c = conn_of(&conn)?;
    let db = open_db(&paths)?;
    engine::notify::remove(&db, &sec, c);
    hub.log(format!("settings: {} connector removed", c.name()));
    Ok(engine::notify::status(&db, &sec, c, engine::rpc::now_ms()))
}

#[tauri::command]
fn notify_set_events(conn: String, events: engine::notify::Events, paths: tauri::State<AppPaths>, sec: tauri::State<Arc<engine::notify::ConnSecrets>>) -> Result<engine::notify::ConnStatus, String> {
    let c = conn_of(&conn)?;
    let db = open_db(&paths)?;
    engine::notify::set_events(&db, c, events);
    Ok(engine::notify::status(&db, &sec, c, engine::rpc::now_ms()))
}

#[tauri::command]
async fn notify_test(conn: String, paths: tauri::State<'_, AppPaths>, sec: tauri::State<'_, Arc<engine::notify::ConnSecrets>>) -> Result<String, String> {
    let c = conn_of(&conn)?;
    let db = open_db(&paths)?;
    engine::notify::send_test(&db, &sec, c, is_paper(), &engine::notify::HttpTransport, 1.0).await
}

#[tauri::command]
fn vault_status(paths: tauri::State<AppPaths>) -> Result<engine::vault::VaultStatus, String> {
    Ok(engine::vault::status(&open_db(&paths)?))
}

#[tauri::command]
fn vault_connect(path: String, paths: tauri::State<AppPaths>, hub: tauri::State<Arc<Hub>>) -> Result<engine::vault::VaultStatus, String> {
    let st = engine::vault::connect(&open_db(&paths)?, &path)?;
    hub.log("settings: Obsidian vault connected".to_string());
    Ok(st)
}

#[tauri::command]
fn vault_disconnect(paths: tauri::State<AppPaths>, hub: tauri::State<Arc<Hub>>) -> Result<engine::vault::VaultStatus, String> {
    hub.log("settings: Obsidian vault disconnected".to_string());
    Ok(engine::vault::disconnect(&open_db(&paths)?))
}

#[tauri::command]
fn vault_set_toggles(settings: engine::vault::VaultSettings, paths: tauri::State<AppPaths>) -> Result<engine::vault::VaultStatus, String> {
    Ok(engine::vault::set_toggles(&open_db(&paths)?, &settings))
}

#[tauri::command]
fn vault_test(paths: tauri::State<AppPaths>) -> Result<engine::vault::VaultTest, String> {
    Ok(engine::vault::test(&open_db(&paths)?, engine::rpc::now_ms()))
}

#[tauri::command]
async fn vault_backfill(paths: tauri::State<'_, AppPaths>) -> Result<engine::vault::BackfillResult, String> {
    let db_path = paths.db.clone();
    tauri::async_runtime::spawn_blocking(move || engine::vault::backfill(&engine::db::Db::open(&db_path)?)).await.map_err(|e| e.to_string())?
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

/// Raw feed recording: what is on disk and the settings (JSON text).
#[tauri::command]
fn recorder_status(hub: tauri::State<Arc<Hub>>) -> String {
    hub.command("recorder-status")
}

/// `{"enabled":true,"retentionDays":14,"maxMegabytes":2000}`. Takes effect at once.
#[tauri::command]
fn set_recorder(values: String, hub: tauri::State<Arc<Hub>>) -> String {
    hub.command(&format!("set-recorder:{values}"))
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

// ---------------------------------------------------------------- X Monitor settings (on/off, pause, pacing)

#[tauri::command]
fn get_xmon_settings(paths: tauri::State<AppPaths>) -> Result<engine::xmon::XSettings, String> {
    Ok(engine::xmon::XSettings::load(&open_db(&paths)?, &engine::config::XPacing::default()))
}

/// Saved in SQLite; the engine and the extension pick it up within a couple of seconds. Hard floors are checked here.
#[tauri::command]
fn set_xmon_settings(settings: engine::xmon::XSettings, paths: tauri::State<AppPaths>) -> Result<(), String> {
    settings.save(&open_db(&paths)?)
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
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
            prepare_update,
            cancel_update,
            notify_status,
            notify_save,
            notify_remove,
            notify_set_events,
            notify_test,
            vault_status,
            vault_connect,
            vault_disconnect,
            vault_set_toggles,
            vault_test,
            vault_backfill,
            open_link,
            set_paper_fill,
            recorder_status,
            set_recorder,
            learning_command,
            ext_pair_start,
            ext_unpair,
            ext_paired,
            get_startup,
            set_startup,
            set_autostart,
            open_chrome_now,
            open_extension_folder,
            get_xmon_settings,
            set_xmon_settings
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Live engine: Memecoin Monitor -> Scam Shield -> Token Picker -> paper trader. Paper only.
            let dir = app.path().app_data_dir().ok().filter(|d| std::fs::create_dir_all(d).is_ok()).unwrap_or_else(|| std::path::PathBuf::from("."));
            let db_path = dir.join("claude-trader.db").to_string_lossy().to_string();
            app.manage(AppPaths { db: db_path.clone(), dir: dir.clone() });
            app.manage(Arc::new(engine::notify::ConnSecrets::system(dir.clone())));
            {
                // "update installed": the version differs from the one at the last start (sent once the engine is up)
                let (db3, ver) = (db_path.clone(), handle.package_info().version.to_string());
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                    if let Ok(db) = engine::db::Db::open(&db3) {
                        engine::notify::on_app_start(&db, &ver, engine::rpc::now_ms());
                    }
                });
            }
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
        ;
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while building Claude Trader");
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            handle.state::<KeepAwake>().set(false); // let Windows sleep again
        }
    });
}
