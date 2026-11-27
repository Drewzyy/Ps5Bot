//! Things the desktop app does for the Chrome side: start Chrome, keep the PC awake. Windows is the real target.

use engine::startup::{find_chrome, StartupSettings};
use std::sync::Mutex;

/// Start Chrome with the pages the user chose. Falls back to the default browser when Chrome is not found.
/// Returns a short plain sentence for the Settings screen.
pub fn open_browser(s: &StartupSettings) -> String {
    let urls = s.launch_urls();
    if let Some(chrome) = find_chrome() {
        let mut cmd = std::process::Command::new(&chrome);
        cmd.args(&urls);
        return match cmd.spawn() {
            Ok(_) => "Chrome is starting.".into(),
            Err(e) => format!("Could not start Chrome: {e}"),
        };
    }
    // no Chrome found: the default browser opens the first page (the extension only works in Chrome)
    let url = urls.first().cloned().unwrap_or_else(|| "https://pump.fun/".into());
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(&url).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    match r {
        Ok(_) => "Chrome was not found, so your default browser is opening. The extension needs Chrome.".into(),
        Err(e) => format!("Could not open a browser: {e}"),
    }
}

/// Open a folder in File Explorer (only folders the app itself names).
pub fn show_folder(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    r.map(|_| ()).map_err(|e| format!("Could not open the folder: {e}"))
}

#[cfg(windows)]
mod win {
    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }
    pub fn on() {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
    }
    pub fn off() {
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}
#[cfg(not(windows))]
mod win {
    pub fn on() {}
    pub fn off() {}
}

/// Keeps Windows from sleeping while the bot runs. SetThreadExecutionState belongs to one thread, so a small
/// thread holds it until told to stop (or until the app exits, which also releases it).
#[derive(Default)]
pub struct KeepAwake {
    stop: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl KeepAwake {
    pub fn set(&self, on: bool) {
        let mut g = self.stop.lock().unwrap();
        if on && g.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                win::on();
                let _ = rx.recv(); // returns when the sender is dropped or sent to
                win::off();
            });
            *g = Some(tx);
        } else if !on {
            if let Some(tx) = g.take() {
                let _ = tx.send(());
            }
        }
    }
    #[allow(dead_code)]
    pub fn is_on(&self) -> bool {
        self.stop.lock().unwrap().is_some()
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        self.set(false);
    }
}
