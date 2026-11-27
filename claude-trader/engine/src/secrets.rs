//! Helius API key storage and connection test.
//!
//! The key lives in the operating system's secure storage (Windows Credential Manager, macOS Keychain, Linux
//! kernel keyring). Only if that fails does it go to an encrypted file in the app's data folder. It is never
//! written to config.json, logs, git or the database, and the UI only ever receives a masked hint such as
//! "ends in ...ab12". Nothing in this file signs or sends a transaction.

use crate::net::{http_client, mask};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SERVICE: &str = "claude-trader";
pub const ACCOUNT: &str = "helius-api-key";

/// One place a secret can be kept.
pub trait SecretBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, value: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

// ----------------------------------------------------------------------------- OS keyring

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
pub struct KeyringBackend {
    label: &'static str,
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl KeyringBackend {
    pub fn new() -> Self {
        let label = if cfg!(windows) {
            "Windows Credential Manager"
        } else if cfg!(target_os = "macos") {
            "macOS Keychain"
        } else {
            "the system keyring"
        };
        KeyringBackend { label }
    }
    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keyring: {e}"))
    }
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl SecretBackend for KeyringBackend {
    fn name(&self) -> &'static str {
        self.label
    }
    fn get(&self) -> Result<Option<String>, String> {
        match self.entry()?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("keyring read: {e}")),
        }
    }
    fn set(&self, value: &str) -> Result<(), String> {
        self.entry()?.set_password(value).map_err(|e| format!("keyring write: {e}"))
    }
    fn delete(&self) -> Result<(), String> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("keyring delete: {e}")),
        }
    }
}

// ----------------------------------------------------------------------------- encrypted file fallback

/// Encrypted local file. ChaCha20-Poly1305 with a key derived from a random per-file salt plus this user and
/// computer name. That keeps the key out of plain text and out of backups that are copied to another machine,
/// but it is weaker than the OS vault (anyone running as this user on this computer can decrypt it), which is
/// why it is only a fallback.
pub struct FileBackend {
    path: PathBuf,
}

impl FileBackend {
    pub fn new(dir: PathBuf) -> Self {
        FileBackend { path: dir.join("helius.key.enc") }
    }
    fn derive(salt: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
        let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(b"claude-trader/helius/v1");
        h.update(salt);
        h.update(user.as_bytes());
        h.update([0]);
        h.update(host.as_bytes());
        h.finalize().into()
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl SecretBackend for FileBackend {
    fn name(&self) -> &'static str {
        "an encrypted file on this computer"
    }
    fn get(&self) -> Result<Option<String>, String> {
        use base64::Engine;
        use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
        let txt = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("key file read: {e}")),
        };
        let raw = base64::engine::general_purpose::STANDARD.decode(txt.trim()).map_err(|_| "key file is damaged".to_string())?;
        if raw.len() < 32 + 12 + 16 {
            return Err("key file is damaged".into());
        }
        let (salt, rest) = raw.split_at(32);
        let (nonce, ct) = rest.split_at(12);
        let cipher = ChaCha20Poly1305::new((&Self::derive(salt)).into());
        let pt = cipher.decrypt(Nonce::from_slice(nonce), ct).map_err(|_| "key file could not be decrypted on this computer".to_string())?;
        String::from_utf8(pt).map(Some).map_err(|_| "key file is damaged".into())
    }
    fn set(&self, value: &str) -> Result<(), String> {
        use base64::Engine;
        use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
        let mut salt = [0u8; 32];
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut salt).map_err(|e| format!("random: {e}"))?;
        getrandom::getrandom(&mut nonce).map_err(|e| format!("random: {e}"))?;
        let cipher = ChaCha20Poly1305::new((&Self::derive(&salt)).into());
        let ct = cipher.encrypt(Nonce::from_slice(&nonce), value.as_bytes()).map_err(|_| "encrypt failed".to_string())?;
        let mut out = salt.to_vec();
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("key file folder: {e}"))?;
        }
        std::fs::write(&self.path, base64::engine::general_purpose::STANDARD.encode(out)).map_err(|e| format!("key file write: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("key file delete: {e}")),
        }
    }
}

// ----------------------------------------------------------------------------- in-memory (tests, mock keyring)

/// In-memory backend. Used by the tests as the mock keyring; `fail` makes every call error like a broken vault.
#[derive(Default)]
pub struct MemoryBackend {
    value: std::sync::Mutex<Option<String>>,
    pub fail: std::sync::atomic::AtomicBool,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn raw(&self) -> Option<String> {
        self.value.lock().unwrap().clone()
    }
}

impl SecretBackend for MemoryBackend {
    fn name(&self) -> &'static str {
        "the system keyring"
    }
    fn get(&self) -> Result<Option<String>, String> {
        if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("vault unavailable".into());
        }
        Ok(self.value.lock().unwrap().clone())
    }
    fn set(&self, v: &str) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("vault unavailable".into());
        }
        *self.value.lock().unwrap() = Some(v.to_string());
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("vault unavailable".into());
        }
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}

// ----------------------------------------------------------------------------- the key store

/// What the UI may know about the saved key. Never the key itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub saved: bool,
    /// "saved, ends in ...ab12" or "No key saved".
    pub hint: String,
    /// Where it is kept, in plain words.
    pub storage: String,
    /// True when the HELIUS_API_KEY environment variable overrides the saved key.
    pub env_override: bool,
}

/// Last four characters, shown as "...ab12". Short keys show nothing but dots.
pub fn key_hint(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 12 {
        return "saved".into();
    }
    format!("saved, ends in ...{}", chars[chars.len() - 4..].iter().collect::<String>())
}

/// A key is only accepted if it looks like one: no spaces, no URL pieces, a sensible length.
pub fn validate_key(k: &str) -> Result<String, String> {
    let k = k.trim().to_string();
    if k.is_empty() {
        return Err("Paste your Helius API key first.".into());
    }
    if k.len() < 12 || k.len() > 128 {
        return Err("That does not look like a Helius key. It is usually about 36 characters.".into());
    }
    if k.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        return Err("Paste only the key itself, not the whole link. It has only letters, numbers and dashes.".into());
    }
    Ok(k)
}

pub fn helius_rpc_url(key: &str) -> String {
    format!("https://mainnet.helius-rpc.com/?api-key={key}")
}

#[derive(Clone)]
pub struct KeyStore {
    primary: Option<Arc<dyn SecretBackend>>,
    fallback: Arc<dyn SecretBackend>,
}

impl KeyStore {
    /// OS vault first, the encrypted file in `dir` as the fallback.
    pub fn system(dir: PathBuf) -> KeyStore {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        let primary: Option<Arc<dyn SecretBackend>> = Some(Arc::new(KeyringBackend::new()));
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        let primary: Option<Arc<dyn SecretBackend>> = None;
        KeyStore { primary, fallback: Arc::new(FileBackend::new(dir)) }
    }

    pub fn with_backends(primary: Option<Arc<dyn SecretBackend>>, fallback: Arc<dyn SecretBackend>) -> KeyStore {
        KeyStore { primary, fallback }
    }

    /// The key to use: the environment variable if set (override), else the saved one.
    pub fn effective_key(&self) -> Option<String> {
        if let Some(k) = std::env::var("HELIUS_API_KEY").ok().filter(|k| !k.is_empty()) {
            return Some(k);
        }
        self.load()
    }

    /// The saved key (secure storage first, then the file).
    pub fn load(&self) -> Option<String> {
        if let Some(p) = &self.primary {
            if let Ok(Some(k)) = p.get() {
                return Some(k);
            }
        }
        self.fallback.get().ok().flatten()
    }

    /// Save after checking the key. Returns where it ended up. The key is written back and read once to prove it stuck.
    pub fn save(&self, key: &str) -> Result<String, String> {
        let key = validate_key(key)?;
        if let Some(p) = &self.primary {
            if p.set(&key).is_ok() && matches!(p.get(), Ok(Some(ref v)) if *v == key) {
                let _ = self.fallback.delete(); // never keep a second copy around
                return Ok(p.name().to_string());
            }
        }
        self.fallback.set(&key).map_err(|e| mask(&e))?;
        match self.fallback.get() {
            Ok(Some(v)) if v == key => {
                if let Some(p) = &self.primary {
                    let _ = p.delete();
                }
                Ok(self.fallback.name().to_string())
            }
            _ => Err("The key could not be saved on this computer.".into()),
        }
    }

    pub fn remove(&self) -> Result<(), String> {
        let a = self.primary.as_ref().map(|p| p.delete()).unwrap_or(Ok(()));
        let b = self.fallback.delete();
        a.and(b)
    }

    pub fn status(&self) -> KeyStatus {
        let env = std::env::var("HELIUS_API_KEY").ok().filter(|k| !k.is_empty());
        let mut storage = String::new();
        let mut saved_key = None;
        if let Some(p) = &self.primary {
            if let Ok(Some(k)) = p.get() {
                storage = p.name().to_string();
                saved_key = Some(k);
            }
        }
        if saved_key.is_none() {
            if let Ok(Some(k)) = self.fallback.get() {
                storage = self.fallback.name().to_string();
                saved_key = Some(k);
            }
        }
        match (&saved_key, &env) {
            (Some(k), e) => KeyStatus { saved: true, hint: key_hint(k), storage, env_override: e.is_some() },
            (None, Some(k)) => KeyStatus { saved: true, hint: format!("{} (from the HELIUS_API_KEY setting)", key_hint(k)), storage: "an environment variable".into(), env_override: true },
            (None, None) => KeyStatus { saved: false, hint: "No key saved".into(), storage: String::new(), env_override: false },
        }
    }
}

/// RPC URL for an optional key: Helius when there is one, the public endpoint otherwise.
pub fn rpc_url_for(key: Option<&str>) -> String {
    match key {
        Some(k) => helius_rpc_url(k),
        None => crate::rpc::PUBLIC_RPC.to_string(),
    }
}

// ----------------------------------------------------------------------------- connection test

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnTest {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub message: String,
}

/// Turn an HTTP status into a plain sentence. Pure, so it is tested without a network.
pub fn explain_status(status: u16) -> String {
    match status {
        401 | 403 => "Helius did not accept this key. Check that you copied all of it.".into(),
        429 => "Helius says too many requests. Wait a minute and try again.".into(),
        500..=599 => "Helius had a problem on its side. Try again in a minute.".into(),
        s => format!("Helius answered with an unexpected code ({s})."),
    }
}

/// One cheap call (`getSlot`) to Helius with this key. Never puts the key in the message.
pub async fn test_helius(key: &str) -> ConnTest {
    let key = match validate_key(key) {
        Ok(k) => k,
        Err(m) => return ConnTest { ok: false, latency_ms: None, message: m },
    };
    let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"getSlot","params":[{"commitment":"processed"}]});
    let t0 = Instant::now();
    let res = http_client().post(helius_rpc_url(&key)).timeout(Duration::from_secs(8)).json(&body).send().await;
    let ms = t0.elapsed().as_millis() as u64;
    match res {
        Err(e) => {
            let m = if e.is_timeout() { "Helius did not answer in 8 seconds. Check your internet connection.".to_string() } else { "Could not reach Helius. Check your internet connection.".to_string() };
            ConnTest { ok: false, latency_ms: None, message: m }
        }
        Ok(r) if !r.status().is_success() => ConnTest { ok: false, latency_ms: None, message: explain_status(r.status().as_u16()) },
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(v) if v["result"].is_u64() => ConnTest { ok: true, latency_ms: Some(ms), message: format!("OK. Helius answered in {ms} ms.") },
            Ok(v) if v["error"].is_object() => ConnTest { ok: false, latency_ms: None, message: "Helius did not accept this key. Check that you copied all of it.".into() },
            _ => ConnTest { ok: false, latency_ms: None, message: "Helius sent an answer the app could not read.".into() },
        },
    }
}

/// Tests that touch the HELIUS_API_KEY variable hold this lock (environment variables are process wide).
#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0a1b2c3d-4e5f-6789-abcd-ef01234bab12";

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ct-secrets-{tag}-{}-{}", std::process::id(), crate::rpc::now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn store(mem: &Arc<MemoryBackend>, tag: &str) -> (KeyStore, PathBuf) {
        let d = dir(tag);
        (KeyStore::with_backends(Some(mem.clone()), Arc::new(FileBackend::new(d.clone()))), d)
    }

    #[test]
    fn save_goes_to_the_keyring_and_the_ui_only_sees_a_masked_hint() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("HELIUS_API_KEY");
        let mem = Arc::new(MemoryBackend::new());
        let (s, d) = store(&mem, "a");
        assert!(!s.status().saved);
        let where_ = s.save(&format!("  {KEY} \n")).unwrap();
        assert_eq!(where_, "the system keyring");
        assert_eq!(mem.raw().as_deref(), Some(KEY));
        let st = s.status();
        assert!(st.saved);
        assert_eq!(st.hint, "saved, ends in ...ab12");
        let json = serde_json::to_string(&st).unwrap();
        assert!(!json.contains(KEY) && !format!("{st:?}").contains(KEY), "status must never carry the key");
        assert_eq!(s.load().as_deref(), Some(KEY));
        assert!(!d.exists(), "nothing written to disk when the keyring works");
    }

    #[test]
    fn a_broken_keyring_falls_back_to_an_encrypted_file() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("HELIUS_API_KEY");
        let mem = Arc::new(MemoryBackend::new());
        mem.fail.store(true, std::sync::atomic::Ordering::Relaxed);
        let (s, d) = store(&mem, "b");
        let where_ = s.save(KEY).unwrap();
        assert_eq!(where_, "an encrypted file on this computer");
        let raw = std::fs::read_to_string(d.join("helius.key.enc")).unwrap();
        assert!(!raw.contains(KEY), "the file is encrypted");
        assert!(!raw.contains("ab12"));
        assert_eq!(s.load().as_deref(), Some(KEY));
        assert_eq!(s.status().storage, "an encrypted file on this computer");
        s.remove().ok();
        assert!(s.load().is_none());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn remove_clears_every_copy() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("HELIUS_API_KEY");
        let mem = Arc::new(MemoryBackend::new());
        let (s, _d) = store(&mem, "c");
        s.save(KEY).unwrap();
        s.remove().unwrap();
        assert!(mem.raw().is_none());
        assert!(!s.status().saved);
        assert_eq!(s.status().hint, "No key saved");
    }

    #[test]
    fn bad_keys_are_refused_with_a_plain_sentence() {
        for bad in ["", "short", "https://mainnet.helius-rpc.com/?api-key=0a1b2c3d-4e5f-6789-abcd-ef01234bab12", "has space in the middle of it ok"] {
            let e = validate_key(bad).unwrap_err();
            assert!(!e.contains('\u{2014}') && e.ends_with('.'), "{e}");
        }
        assert_eq!(validate_key(&format!(" {KEY} ")).unwrap(), KEY);
    }

    #[test]
    fn env_var_overrides_the_saved_key_and_is_reported() {
        let _g = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mem = Arc::new(MemoryBackend::new());
        let (s, _d) = store(&mem, "d");
        s.save(KEY).unwrap();
        std::env::set_var("HELIUS_API_KEY", "zzzzzzzzzzzzzzzzzzzz9999");
        assert_eq!(s.effective_key().as_deref(), Some("zzzzzzzzzzzzzzzzzzzz9999"));
        let st = s.status();
        assert!(st.env_override);
        std::env::remove_var("HELIUS_API_KEY");
        assert_eq!(s.effective_key().as_deref(), Some(KEY));
    }

    #[test]
    fn urls_built_from_the_key_are_masked_in_logs() {
        let u = helius_rpc_url(KEY);
        assert!(!mask(&u).contains(KEY));
        assert_eq!(rpc_url_for(None), crate::rpc::PUBLIC_RPC);
    }

    #[test]
    fn status_codes_become_plain_sentences() {
        assert!(explain_status(401).contains("did not accept this key"));
        assert!(explain_status(429).contains("too many requests"));
        for c in [401, 429, 503, 418] {
            assert!(!explain_status(c).contains('\u{2014}'));
        }
    }
}
