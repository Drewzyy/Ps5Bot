use engine::{demo::DemoGen, Config};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

const DEFAULT_CONFIG: &str = include_str!("../../../config.json");

/// Returns the strategy config as JSON. M1 is read-only: it ships with the app.
#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let c = Config::from_json(DEFAULT_CONFIG)?;
    serde_json::to_value(c).map_err(|e| e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_config])
        .setup(|app| {
            let handle = app.handle().clone();
            // Demo event loop. Emits "engine-event" about once a second (a pulse every few seconds).
            std::thread::spawn(move || {
                let mut gen = DemoGen::new(0x5eed);
                for e in DemoGen::initial_statuses() {
                    let _ = handle.emit("engine-event", e);
                }
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                    for e in gen.tick(now_ms()) {
                        let _ = handle.emit("engine-event", e);
                    }
                }
            });
            let _ = app.get_webview_window("main");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Trader");
}
