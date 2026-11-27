//! "Startup and browser" settings and the Chrome finder. Plain data and pure functions so they can be tested anywhere.
//! The app (Tauri side) does the actual launching; the Chrome extension reads the same settings over the local bridge.

use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEY: &str = "startup.settings";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Pages {
    /// pump.fun home page (where the coins appear)
    pub pump_fun_home: bool,
    /// The Dexscreener chart of each coin the bot holds, opened when it is bought
    pub dexscreener_held: bool,
}

impl Default for Pages {
    fn default() -> Self {
        Pages { pump_fun_home: true, dexscreener_held: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct StartupSettings {
    pub open_chrome: bool,
    pub pages: Pages,
    pub close_tab_on_close: bool,
    pub keep_awake: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        StartupSettings { open_chrome: true, pages: Pages::default(), close_tab_on_close: false, keep_awake: true }
    }
}

impl StartupSettings {
    pub fn load(db: &Db) -> StartupSettings {
        db.kv_get(KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self, db: &Db) {
        db.kv_set(KEY, &serde_json::to_string(self).unwrap_or_default());
    }
    /// Pages the app opens in Chrome at launch (the Dexscreener pages are opened by the extension when a coin is bought).
    pub fn launch_urls(&self) -> Vec<String> {
        let mut v = vec![];
        if self.pages.pump_fun_home {
            v.push("https://pump.fun/".to_string());
        }
        v
    }
}

/// Where chrome.exe normally lives on Windows. `env` looks up an environment variable.
pub fn chrome_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut out = vec![];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
        if let Some(base) = env(var).filter(|s| !s.is_empty()) {
            let mut p = PathBuf::from(base);
            p.push("Google");
            p.push("Chrome");
            p.push("Application");
            p.push("chrome.exe");
            out.push(p);
        }
    }
    // last-resort literal paths when the variables are missing
    for lit in [r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"] {
        let p = PathBuf::from(lit);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// First candidate that exists. `None` means "use the default browser instead".
pub fn pick_chrome(candidates: &[PathBuf], exists: &dyn Fn(&PathBuf) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|p| exists(p)).cloned()
}

/// Chrome on this computer (Windows paths; on Linux or macOS a few usual names, for development only).
pub fn find_chrome() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        pick_chrome(&chrome_candidates(&|k| std::env::var(k).ok()), &|p| p.is_file())
    }
    #[cfg(target_os = "macos")]
    {
        let p = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        p.is_file().then_some(p)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dirs = std::env::var("PATH").unwrap_or_default();
        for name in ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"] {
            for d in dirs.split(':') {
                let p = PathBuf::from(d).join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_open_chrome_and_pump_home_but_do_not_close_tabs() {
        let s = StartupSettings::default();
        assert!(s.open_chrome && s.pages.pump_fun_home && s.pages.dexscreener_held && s.keep_awake);
        assert!(!s.close_tab_on_close);
        assert_eq!(s.launch_urls(), vec!["https://pump.fun/".to_string()]);
    }

    #[test]
    fn settings_round_trip_through_sqlite_and_partial_json_fills_defaults() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(StartupSettings::load(&db), StartupSettings::default());
        let mut s = StartupSettings::default();
        s.close_tab_on_close = true;
        s.pages.pump_fun_home = false;
        s.save(&db);
        assert_eq!(StartupSettings::load(&db), s);
        assert!(StartupSettings::load(&db).launch_urls().is_empty());
        let p: StartupSettings = serde_json::from_str(r#"{"closeTabOnClose":true}"#).unwrap();
        assert!(p.open_chrome && p.close_tab_on_close);
    }

    #[test]
    fn chrome_is_found_in_the_standard_windows_places_in_order() {
        let env = |k: &str| match k {
            "ProgramFiles" => Some(r"C:\Program Files".to_string()),
            "ProgramFiles(x86)" => Some(r"C:\Program Files (x86)".to_string()),
            "LocalAppData" => Some(r"C:\Users\me\AppData\Local".to_string()),
            _ => None,
        };
        let c = chrome_candidates(&env);
        assert!(c[0].to_string_lossy().contains("Program Files") && c[0].to_string_lossy().ends_with("chrome.exe"));
        assert!(c.iter().any(|p| p.to_string_lossy().contains("AppData")));
        // only the per-user install exists
        let only_user = |p: &PathBuf| p.to_string_lossy().contains("AppData");
        assert!(pick_chrome(&c, &only_user).unwrap().to_string_lossy().contains("AppData"));
        // nothing installed: caller falls back to the default browser
        assert!(pick_chrome(&c, &|_| false).is_none());
        // no env variables at all still gives the literal paths
        assert!(chrome_candidates(&|_| None).len() >= 2);
    }
}
