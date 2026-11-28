//! Keeps the Chrome extension files in ONE stable folder in the app data (for example
//! `%LOCALAPPDATA%\Claude Trader\chrome-extension`), so Chrome can load the extension from a path that never moves.
//!
//! Chrome does not update unpacked extensions. The app copies its bundled files into the stable folder at every start
//! (only when the bundled version differs), and when it sees a connected extension older than the files on disk it asks
//! the extension to reload itself. Pure file logic here, so it is tested on any computer.
//!
//! Safety rules: the folder is never deleted (Chrome may have it loaded), files are written to a temp name and renamed
//! (a half-written file is never seen), `manifest.json` is written last, and the manifest `key` (the pinned extension ID)
//! is checked after every copy.

use crate::db::Db;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const KV_KEY: &str = "ext.install";

/// "0.1.42" -> [0, 1, 42]. One to four numeric parts, like a Chrome extension version.
pub fn parse_version(v: &str) -> Option<Vec<u64>> {
    let parts: Vec<&str> = v.trim().split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    parts.iter().map(|p| if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) { None } else { p.parse().ok() }).collect()
}

/// Compare two versions, missing parts count as 0 (1.2 == 1.2.0). None when either is not a version.
pub fn cmp_versions(a: &str, b: &str) -> Option<Ordering> {
    let (mut x, mut y) = (parse_version(a)?, parse_version(b)?);
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    Some(x.cmp(&y))
}

/// Is `a` older than `b`? False when either is not a version (never reload on a guess).
pub fn is_older(a: &str, b: &str) -> bool {
    cmp_versions(a, b) == Some(Ordering::Less)
}

/// How the connected extension's version relates to the files on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Relation {
    /// Same version: nothing to do.
    Same,
    /// The extension is older: ask it to reload (it then reads the new files from the stable folder).
    Older,
    /// The extension is NEWER than the files, or its version is unreadable: it is loaded from somewhere else. The person must load it from the stable folder once.
    OtherFolder,
}

pub fn relation(ext: &str, disk: &str) -> Relation {
    match cmp_versions(ext, disk) {
        Some(Ordering::Equal) => Relation::Same,
        Some(Ordering::Less) => Relation::Older,
        _ => Relation::OtherFolder,
    }
}

/// `version` of `<dir>/manifest.json`.
pub fn manifest_version(dir: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).ok()?).ok()?;
    v["version"].as_str().map(|s| s.to_string())
}

fn manifest_key(dir: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).ok()?).ok()?;
    v["key"].as_str().map(|s| s.to_string())
}

/// Files that belong in the installed extension (no tests, no tooling).
fn wanted(rel: &Path) -> bool {
    let first = rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or("");
    if matches!(first, "tests" | "node_modules" | ".git") {
        return false;
    }
    !matches!(rel.to_str(), Some("package.json" | "package-lock.json" | "README.md"))
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(base, &p, out)?;
        } else if let Ok(rel) = p.strip_prefix(base) {
            if wanted(rel) {
                out.push(rel.to_path_buf());
            }
        }
    }
    Ok(())
}

/// Files to ship, relative paths, `manifest.json` last.
pub fn extension_files(bundled: &Path) -> Result<Vec<PathBuf>, String> {
    let mut v = vec![];
    walk(bundled, bundled, &mut v).map_err(|e| format!("could not read the bundled extension: {e}"))?;
    v.sort();
    v.sort_by_key(|p| p.as_os_str() == "manifest.json");
    Ok(v)
}

/// Temp file in the same folder, then rename over the target. If the rename is refused (a program has the old file open),
/// fall back to writing in place: the folder and path stay the same either way.
fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let tmp = dest.with_file_name(format!("{}.ctnew", dest.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    if std::fs::rename(&tmp, dest).is_err() {
        let r = std::fs::write(dest, bytes).map_err(|e| format!("{}: {e}", dest.display()));
        let _ = std::fs::remove_file(&tmp);
        r?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Version in the stable folder before this start, if any.
    pub from: Option<String>,
    /// Version in the stable folder now.
    pub to: String,
    /// Files written (relative names). Empty when nothing needed to change.
    pub copied: Vec<String>,
    /// True when the versions matched and every file was already there, so nothing was touched.
    pub skipped: bool,
}

/// Copy the bundled extension into the stable folder. Only when the bundled version differs (or a file is missing);
/// even then a file whose bytes are already the same is left alone.
pub fn sync(bundled: &Path, stable: &Path) -> Result<SyncReport, String> {
    let to = manifest_version(bundled).ok_or("the bundled extension has no readable manifest.json")?;
    let key = manifest_key(bundled).ok_or("the bundled manifest.json has no key (the extension ID would change)")?;
    let from = manifest_version(stable);
    let files = extension_files(bundled)?;
    let complete = files.iter().all(|f| stable.join(f).is_file());
    if from.as_deref() == Some(to.as_str()) && complete && manifest_key(stable).as_deref() == Some(key.as_str()) {
        return Ok(SyncReport { from, to, copied: vec![], skipped: true });
    }
    std::fs::create_dir_all(stable).map_err(|e| format!("{}: {e}", stable.display()))?;
    let mut copied = vec![];
    for rel in &files {
        let src = std::fs::read(bundled.join(rel)).map_err(|e| format!("{}: {e}", rel.display()))?;
        let dest = stable.join(rel);
        if std::fs::read(&dest).map(|old| old == src).unwrap_or(false) {
            continue;
        }
        write_atomic(&dest, &src)?;
        copied.push(rel.to_string_lossy().replace('\\', "/"));
    }
    if manifest_key(stable).as_deref() != Some(key.as_str()) {
        return Err("the copied manifest.json lost its key, so the extension ID would change".into());
    }
    Ok(SyncReport { from, to, copied, skipped: false })
}

/// What the app tells the bridge (through SQLite, so the app and engine-cli share it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallInfo {
    pub app_version: String,
    /// Version of the extension files in the stable folder (what a reload will give).
    pub disk_version: String,
    /// The stable folder the person loads in Chrome.
    pub folder: String,
}

impl InstallInfo {
    pub fn load(db: &Db) -> InstallInfo {
        db.kv_get(KV_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self, db: &Db) {
        db.kv_set(KV_KEY, &serde_json::to_string(self).unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ct-extinstall-{name}-{}-{}", std::process::id(), crate::rpc::now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn make(dir: &Path, version: &str, bg: &str) {
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("manifest.json"), format!(r#"{{"manifest_version":3,"version":"{version}","key":"PINNEDKEY"}}"#)).unwrap();
        std::fs::write(dir.join("background.js"), bg).unwrap();
        std::fs::write(dir.join("lib/core.js"), "core").unwrap();
        std::fs::write(dir.join("tests/a.test.js"), "t").unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_older("0.1.9", "0.1.10"));
        assert!(!is_older("0.1.10", "0.1.9"));
        assert!(!is_older("0.1.5", "0.1.5"));
        assert!(!is_older("0.1", "0.1.0"));
        assert!(is_older("0.1.99", "0.2.0"));
        assert!(is_older("1.2.3.4", "1.2.3.5"));
        // anything that is not a version is never "older": no reload on a guess
        assert!(!is_older("", "0.1.1"));
        assert!(!is_older("abc", "0.1.1"));
        assert!(!is_older("0.1.1", "0.1.x"));
        assert!(!is_older("0.1.2.3.4", "9.9.9"));
        assert!(parse_version("1..2").is_none());
        assert!(parse_version("-1.2").is_none());
        assert_eq!(relation("0.1.4", "0.1.5"), Relation::Older);
        assert_eq!(relation("0.1.5", "0.1.5"), Relation::Same);
        assert_eq!(relation("0.2.0", "0.1.5"), Relation::OtherFolder);
        assert_eq!(relation("weird", "0.1.5"), Relation::OtherFolder);
    }

    #[test]
    fn first_start_copies_everything_except_tests_and_tooling() {
        let (b, s) = (tmp("b1"), tmp("s1").join("chrome-extension"));
        make(&b, "0.1.5", "bg v5");
        let r = sync(&b, &s).unwrap();
        assert!(!r.skipped);
        assert_eq!(r.from, None);
        assert_eq!(r.to, "0.1.5");
        assert_eq!(r.copied.last().unwrap(), "manifest.json", "the manifest is written last");
        assert!(r.copied.contains(&"lib/core.js".to_string()));
        assert!(s.join("background.js").is_file() && s.join("lib/core.js").is_file());
        assert!(!s.join("tests").exists() && !s.join("package.json").exists());
        assert_eq!(manifest_key(&s).as_deref(), Some("PINNEDKEY"));
        assert!(std::fs::read_dir(&s).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".ctnew")), "no temp files left");
    }

    #[test]
    fn same_version_touches_nothing() {
        let (b, s) = (tmp("b2"), tmp("s2").join("ext"));
        make(&b, "0.1.5", "bg v5");
        sync(&b, &s).unwrap();
        let t0 = std::fs::metadata(s.join("background.js")).unwrap().modified().unwrap();
        // even a changed file is not copied while the version is the same: the version is the switch
        std::fs::write(b.join("background.js"), "bg changed without a bump").unwrap();
        let r = sync(&b, &s).unwrap();
        assert!(r.skipped && r.copied.is_empty());
        assert_eq!(std::fs::read_to_string(s.join("background.js")).unwrap(), "bg v5");
        assert_eq!(std::fs::metadata(s.join("background.js")).unwrap().modified().unwrap(), t0);
    }

    #[test]
    fn a_new_version_overwrites_in_place_and_copies_only_changed_files() {
        let (b, s) = (tmp("b3"), tmp("s3").join("ext"));
        make(&b, "0.1.5", "bg v5");
        sync(&b, &s).unwrap();
        make(&b, "0.1.6", "bg v6");
        let r = sync(&b, &s).unwrap();
        assert_eq!(r.from.as_deref(), Some("0.1.5"));
        assert_eq!(r.to, "0.1.6");
        assert_eq!(r.copied, vec!["background.js".to_string(), "manifest.json".to_string()], "lib/core.js had the same bytes, so it is left alone");
        assert_eq!(std::fs::read_to_string(s.join("background.js")).unwrap(), "bg v6");
        assert_eq!(manifest_version(&s).as_deref(), Some("0.1.6"));
        // the folder itself was never removed or recreated
        assert!(s.is_dir());
    }

    #[test]
    fn a_missing_file_is_repaired_and_the_pinned_key_must_survive() {
        let (b, s) = (tmp("b4"), tmp("s4").join("ext"));
        make(&b, "0.1.5", "bg");
        sync(&b, &s).unwrap();
        std::fs::remove_file(s.join("lib/core.js")).unwrap();
        let r = sync(&b, &s).unwrap();
        assert!(!r.skipped);
        assert_eq!(r.copied, vec!["lib/core.js".to_string()]);
        // a bundled manifest without a key is refused (the extension ID would change)
        std::fs::write(b.join("manifest.json"), r#"{"version":"0.1.9"}"#).unwrap();
        assert!(sync(&b, &s).unwrap_err().contains("no key"));
        // and a stable manifest with another key is rewritten
        make(&b, "0.1.5", "bg");
        std::fs::write(s.join("manifest.json"), r#"{"version":"0.1.5","key":"OTHER"}"#).unwrap();
        sync(&b, &s).unwrap();
        assert_eq!(manifest_key(&s).as_deref(), Some("PINNEDKEY"));
    }

    #[test]
    fn the_real_extension_folder_syncs_with_its_key() {
        let b = Path::new(env!("CARGO_MANIFEST_DIR")).join("../extension");
        let s = tmp("real").join("chrome-extension");
        let r = sync(&b, &s).unwrap();
        assert_eq!(Some(r.to.clone()), manifest_version(&b));
        assert!(s.join("background.js").is_file() && s.join("sidepanel.html").is_file() && s.join("lib/core.js").is_file());
        assert!(!s.join("tests").exists());
        assert_eq!(manifest_key(&s), manifest_key(&b));
    }

    #[test]
    fn install_info_round_trips_through_the_database() {
        let db = Db::open(":memory:").unwrap();
        assert_eq!(InstallInfo::load(&db), InstallInfo::default());
        let i = InstallInfo { app_version: "0.1.9".into(), disk_version: "0.1.9".into(), folder: "C:\\x".into() };
        i.save(&db);
        assert_eq!(InstallInfo::load(&db), i);
    }
}
