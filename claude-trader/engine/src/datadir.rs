//! The one data folder. Everything the app writes lives under a single root: the SQLite database, the paper-trade
//! logs and daily summaries, the raw feed recordings, the encrypted-secret fallback file, the stable Chrome
//! extension folder, logs, caches and the WebView cache. Every path in the engine and the app comes from
//! [`DataLayout`], so moving the app to another drive is moving one folder.
//!
//! Only a tiny pointer file stays in the default config folder: it says where the root is. The OS-managed
//! secrets (Windows Credential Manager) cannot be moved and are not files of ours.
//!
//! Moving ([`move_data`]) copies, verifies, then switches the pointer last. Nothing in the old folder is deleted
//! by it; [`delete_old`] is a separate step that refuses unless the new copy checks out.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const POINTER_FILE: &str = "data-location.json";
pub const DB_FILE: &str = "claude-trader.db";

/// Where every file of the app lives, relative to one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLayout {
    root: PathBuf,
}

impl DataLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        DataLayout { root: root.into() }
    }
    /// The layout next to a database file (used by the command line tool and tests).
    pub fn beside_db(db_path: &str) -> Self {
        let p = Path::new(db_path).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        DataLayout { root: p }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn db(&self) -> PathBuf {
        self.root.join(DB_FILE)
    }
    pub fn paper_logs(&self) -> PathBuf {
        self.root.join("paper-logs")
    }
    pub fn recordings(&self) -> PathBuf {
        self.root.join("recordings")
    }
    /// The folder for the encrypted-secret fallback files (`*.enc`).
    pub fn secrets(&self) -> PathBuf {
        self.root.clone()
    }
    /// The stable Chrome extension folder the person loads in Chrome.
    pub fn extension(&self) -> PathBuf {
        self.root.join("chrome-extension")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    /// The WebView2 (browser engine) cache and local storage of the app window.
    pub fn webview(&self) -> PathBuf {
        self.root.join("webview")
    }
    /// Every place the app writes, by name. A test checks each one is under the root.
    pub fn all(&self) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("Database", self.db()),
            ("Paper trade logs", self.paper_logs()),
            ("Raw feed recordings", self.recordings()),
            ("Encrypted key file", self.secrets().join("helius.key.enc")),
            ("Chrome extension", self.extension()),
            ("Logs", self.logs()),
            ("Caches", self.cache()),
            ("Window cache", self.webview()),
        ]
    }
    /// Names directly under the root that [`delete_old`] may remove (never anything else).
    fn known_items() -> &'static [&'static str] {
        &["claude-trader.db", "claude-trader.db-wal", "claude-trader.db-shm", "paper-logs", "recordings", "chrome-extension", "logs", "cache", "webview", "webview-boot"]
    }
}

/// Names under the root that a move does not copy: the window cache is rebuilt, SQLite side files are folded into
/// the database copy, and the pointer belongs to the config folder.
fn skipped_in_copy(name: &str) -> bool {
    name == POINTER_FILE || name == "webview" || name == "webview-boot" || name == "claude-trader.db-wal" || name == "claude-trader.db-shm" || name.ends_with(".ctnew")
}

// ---------------------------------------------------------------- the pointer file

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pointer {
    pub data_root: String,
    /// The folder the data was moved away from, until the person deletes or keeps it.
    pub previous_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerState {
    Missing,
    Ok(Pointer),
    Corrupt,
}

pub fn pointer_path(config_dir: &Path) -> PathBuf {
    config_dir.join(POINTER_FILE)
}

pub fn read_pointer(config_dir: &Path) -> PointerState {
    let p = pointer_path(config_dir);
    let Ok(text) = std::fs::read_to_string(&p) else { return PointerState::Missing };
    match serde_json::from_str::<Pointer>(&text) {
        Ok(ptr) if !ptr.data_root.trim().is_empty() => PointerState::Ok(ptr),
        _ => PointerState::Corrupt,
    }
}

/// Atomic: write a temp file, then rename over the pointer.
pub fn write_pointer(config_dir: &Path, ptr: &Pointer) -> Result<(), String> {
    std::fs::create_dir_all(config_dir).map_err(|e| io_msg(config_dir, &e))?;
    let p = pointer_path(config_dir);
    let tmp = config_dir.join(format!("{POINTER_FILE}.ctnew"));
    let text = serde_json::to_string_pretty(ptr).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| io_msg(&tmp, &e))?;
    if let Err(e) = std::fs::rename(&tmp, &p) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io_msg(&p, &e));
    }
    Ok(())
}

// ---------------------------------------------------------------- resolving the root

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub layout: DataLayout,
    /// The default root (the app data folder). Equals the layout root unless the pointer says otherwise.
    pub default_root: PathBuf,
    pub from_pointer: bool,
    /// True on the very first launch: no pointer and no data yet, so the app asks where to keep its data.
    pub first_run: bool,
    /// A problem to show (the chosen folder is missing, the pointer is unreadable). The pointer is left alone.
    pub warning: Option<String>,
    pub previous_root: Option<PathBuf>,
}

pub fn resolve(default_root: &Path, config_dir: &Path) -> Resolved {
    let default_has_data = default_root.join(DB_FILE).is_file();
    let base = |warning: Option<String>, first_run: bool| Resolved {
        layout: DataLayout::new(default_root),
        default_root: default_root.to_path_buf(),
        from_pointer: false,
        first_run,
        warning,
        previous_root: None,
    };
    match read_pointer(config_dir) {
        PointerState::Missing => base(None, !default_has_data),
        PointerState::Corrupt => base(Some("The data folder pointer could not be read, so the default folder is used. Pick your data folder again in Settings > Storage.".into()), false),
        PointerState::Ok(ptr) => {
            let root = PathBuf::from(&ptr.data_root);
            match check_writable(&root) {
                Ok(()) => Resolved {
                    layout: DataLayout::new(root),
                    default_root: default_root.to_path_buf(),
                    from_pointer: true,
                    first_run: false,
                    warning: None,
                    previous_root: ptr.previous_root.map(PathBuf::from),
                },
                Err(e) => {
                    let mut r = base(Some(format!("Your data folder {} is not available ({e}). The default folder is used until it is back. Reconnect the drive and restart the app to return to it.", root.display())), false);
                    r.previous_root = None;
                    r
                }
            }
        }
    }
}

/// Create the folder if needed and prove a file can be written there.
pub fn check_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| io_msg(dir, &e))?;
    let probe = dir.join(".ct-write-test");
    std::fs::write(&probe, b"x").map_err(|e| io_msg(dir, &e))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// The extension folder changed since the last start (the data folder moved): Chrome must load it again from the new place.
pub fn extension_moved(previous_folder: &str, now: &Path) -> bool {
    !previous_folder.trim().is_empty() && !same_path(Path::new(previous_folder), now)
}

// ---------------------------------------------------------------- messages

pub fn io_msg(path: &Path, e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        PermissionDenied => format!("Windows would not let the app write to {}. Pick a folder you own, for example D:\\ClaudeTraderData.", path.display()),
        NotFound => format!("The folder {} was not found. If it is on another drive, check that the drive is connected.", path.display()),
        _ => {
            let s = e.to_string();
            if s.contains("os error 112") || s.contains("os error 28") || s.to_lowercase().contains("no space") || s.to_lowercase().contains("not enough space") {
                format!("There is not enough free space for {}.", path.display())
            } else {
                format!("{}: {s}", path.display())
            }
        }
    }
}

// ---------------------------------------------------------------- path helpers

/// A comparable form of a path: absolute, symlinks resolved as far as the path exists, case folded on Windows.
fn norm(p: &Path) -> PathBuf {
    let mut existing = p.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = vec![];
    loop {
        if let Ok(c) = std::fs::canonicalize(&existing) {
            let mut out = c;
            for t in tail.iter().rev() {
                out.push(t);
            }
            return fold(out);
        }
        match (existing.file_name().map(|s| s.to_os_string()), existing.parent().map(|p| p.to_path_buf())) {
            (Some(n), Some(par)) if !par.as_os_str().is_empty() => {
                tail.push(n);
                existing = par;
            }
            _ => return fold(p.to_path_buf()),
        }
    }
}

fn fold(p: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) if cfg!(windows) => out.push(s.to_string_lossy().to_lowercase()),
            Component::Prefix(s) if cfg!(windows) => out.push(s.as_os_str().to_string_lossy().to_lowercase()),
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

fn is_inside(child: &Path, parent: &Path) -> bool {
    let (c, p) = (norm(child), norm(parent));
    c != p && c.starts_with(&p)
}

// ---------------------------------------------------------------- inspecting a destination

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DestKind {
    /// Empty or new: the data is copied there.
    Empty,
    /// Already holds Claude Trader data (a reinstall): the app switches to it, nothing is copied.
    ExistingData,
    /// Holds other files: refused.
    NotEmpty { count: usize },
}

pub fn inspect_destination(current_root: &Path, dest: &Path) -> Result<DestKind, String> {
    if dest.as_os_str().is_empty() {
        return Err("Choose a folder first.".into());
    }
    if !dest.is_absolute() {
        return Err("Choose a full folder path, for example D:\\ClaudeTraderData.".into());
    }
    if same_path(current_root, dest) {
        return Err("That is the folder the app already uses.".into());
    }
    if is_inside(dest, current_root) {
        return Err("Choose a folder outside the current data folder, not one inside it.".into());
    }
    if is_inside(current_root, dest) {
        return Err("Choose a folder that does not contain the current data folder.".into());
    }
    if dest.exists() && !dest.is_dir() {
        return Err("That path is a file, not a folder.".into());
    }
    if !dest.exists() {
        return Ok(DestKind::Empty);
    }
    let names: Vec<String> = std::fs::read_dir(dest).map_err(|e| io_msg(dest, &e))?.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    if names.is_empty() {
        return Ok(DestKind::Empty);
    }
    if names.iter().any(|n| n == DB_FILE) {
        return Ok(DestKind::ExistingData);
    }
    Ok(DestKind::NotEmpty { count: names.len() })
}

pub fn not_empty_message(dest: &Path, count: usize) -> String {
    format!("{} is not empty ({count} item{}). Pick an empty folder, or make a new one such as D:\\ClaudeTraderData. To use data from an earlier install, pick the folder that holds claude-trader.db.", dest.display(), if count == 1 { "" } else { "s" })
}

// ---------------------------------------------------------------- sizes and free space

/// Files (relative path, size) under `root` that a move would copy, skipping what [`skipped_in_copy`] names at the top level.
pub fn list_copy_files(root: &Path) -> Result<Vec<(PathBuf, u64)>, String> {
    fn walk(base: &Path, dir: &Path, top: bool, out: &mut Vec<(PathBuf, u64)>) -> Result<(), String> {
        for e in std::fs::read_dir(dir).map_err(|e| io_msg(dir, &e))? {
            let e = e.map_err(|e| io_msg(dir, &e))?;
            let name = e.file_name().to_string_lossy().to_string();
            if top && skipped_in_copy(&name) {
                continue;
            }
            let path = e.path();
            let ft = e.file_type().map_err(|e| io_msg(&path, &e))?;
            if ft.is_dir() {
                walk(base, &path, false, out)?;
            } else if ft.is_file() {
                let len = e.metadata().map_err(|e| io_msg(&path, &e))?.len();
                out.push((path.strip_prefix(base).unwrap_or(&path).to_path_buf(), len));
            }
        }
        Ok(())
    }
    let mut out = vec![];
    if root.is_dir() {
        walk(root, root, true, &mut out)?;
    }
    out.sort();
    Ok(out)
}

pub fn dir_size(root: &Path) -> u64 {
    fn walk(dir: &Path) -> u64 {
        let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
        rd.filter_map(|e| e.ok()).map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => walk(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        }).sum()
    }
    walk(root)
}

/// Free bytes on the drive that holds `path` (nearest existing folder). None when it cannot be read.
pub fn free_bytes(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        p = p.parent()?.to_path_buf();
    }
    free_bytes_of_existing(&p)
}

#[cfg(windows)]
fn free_bytes_of_existing(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: wide is NUL terminated and the three out pointers are valid for the call.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some(avail)
}

#[cfg(not(windows))]
fn free_bytes_of_existing(p: &Path) -> Option<u64> {
    let out = std::process::Command::new("df").arg("-Pk").arg(p).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = text.lines().nth(1)?;
    let kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(kb * 1024)
}

/// Space a copy needs: the files, a tenth extra for the database copy, and a margin.
pub fn needed_bytes(files_total: u64) -> u64 {
    files_total + files_total / 10 + 32 * 1024 * 1024
}

pub fn human_bytes(n: u64) -> String {
    let f = n as f64;
    if f >= 1e9 {
        format!("{:.1} GB", f / 1e9)
    } else if f >= 1e6 {
        format!("{:.1} MB", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0} KB", f / 1e3)
    } else {
        format!("{n} bytes")
    }
}

// ---------------------------------------------------------------- the check shown before a move

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveCheck {
    pub dest: String,
    #[serde(flatten)]
    pub kind: DestKind,
    pub ok: bool,
    pub message: String,
    pub need_bytes: u64,
    pub free_bytes: Option<u64>,
}

pub fn check_move(current_root: &Path, dest: &Path, free: &dyn Fn(&Path) -> Option<u64>) -> MoveCheck {
    let mk = |kind: DestKind, ok: bool, message: String, need: u64, free: Option<u64>| MoveCheck { dest: dest.to_string_lossy().to_string(), kind, ok, message, need_bytes: need, free_bytes: free };
    let kind = match inspect_destination(current_root, dest) {
        Ok(k) => k,
        Err(e) => return mk(DestKind::Empty, false, e, 0, None),
    };
    match &kind {
        DestKind::NotEmpty { count } => mk(kind.clone(), false, not_empty_message(dest, *count), 0, None),
        DestKind::ExistingData => mk(kind.clone(), true, format!("{} already holds Claude Trader data. The app will switch to it. Your current data stays where it is until you delete it.", dest.display()), 0, None),
        DestKind::Empty => {
            let files = match list_copy_files(current_root) {
                Ok(f) => f,
                Err(e) => return mk(kind.clone(), false, e, 0, None),
            };
            let total: u64 = files.iter().map(|f| f.1).sum();
            let need = needed_bytes(total);
            let fr = free(dest);
            if let Some(f) = fr {
                if f < need {
                    return mk(kind.clone(), false, format!("Not enough free space on that drive. The copy needs {} and only {} is free.", human_bytes(need), human_bytes(f)), need, fr);
                }
            }
            mk(kind.clone(), true, format!("{} of data will be copied to {}.", human_bytes(total), dest.display()), need, fr)
        }
    }
}

// ---------------------------------------------------------------- the move

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    pub old_root: String,
    pub new_root: String,
    pub files: usize,
    pub bytes: u64,
    /// "copied" or "existing" (switched to a folder that already held data).
    pub mode: String,
}

fn open_ro(path: &Path) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| format!("could not open the database {}: {e}", path.display()))
}

fn table_counts(c: &rusqlite::Connection) -> Result<Vec<(String, i64)>, String> {
    let names: Vec<String> = {
        let mut st = c.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(|e| e.to_string())?;
        let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let mut out = vec![];
    for n in names {
        let cnt: i64 = c.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", n.replace('"', "\"\"")), [], |r| r.get(0)).map_err(|e| e.to_string())?;
        out.push((n, cnt));
    }
    Ok(out)
}

/// SQLite's own integrity check on a database file. Ok(()) when it says "ok".
pub fn db_integrity(path: &Path) -> Result<(), String> {
    let c = open_ro(path)?;
    let r: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|e| format!("integrity check failed to run: {e}"))?;
    if r == "ok" {
        Ok(())
    } else {
        Err(format!("the database in {} did not pass SQLite's integrity check ({r})", path.display()))
    }
}

/// Copy the data root to `dest`, verify the copy, and only then switch the pointer. Never deletes from `src`.
/// On any failure before the switch, what this call created in `dest` is removed and the pointer is untouched.
pub fn move_data(src: &Path, dest: &Path, config_dir: &Path, free: &dyn Fn(&Path) -> Option<u64>) -> Result<MoveReport, String> {
    let chk = check_move(src, dest, free);
    if !chk.ok {
        return Err(chk.message);
    }
    let old = src.to_string_lossy().to_string();
    let new = dest.to_string_lossy().to_string();
    if chk.kind == DestKind::ExistingData {
        db_integrity(&dest.join(DB_FILE)).map_err(|e| format!("Cannot use that folder: {e}."))?;
        write_pointer(config_dir, &Pointer { data_root: new.clone(), previous_root: Some(old.clone()) })?;
        return Ok(MoveReport { old_root: old, new_root: new, files: 0, bytes: 0, mode: "existing".into() });
    }
    let dest_existed = dest.exists();
    let mut created_files: Vec<PathBuf> = vec![];
    let mut created_dirs: Vec<PathBuf> = vec![];
    let result = copy_and_verify(src, dest, &mut created_files, &mut created_dirs);
    let (files, bytes) = match result {
        Ok(x) => x,
        Err(e) => {
            cleanup(&created_files, &created_dirs, dest, dest_existed);
            return Err(format!("{e} Nothing was changed: the app keeps using {old}."));
        }
    };
    // the switch, last
    if let Err(e) = write_pointer(config_dir, &Pointer { data_root: new.clone(), previous_root: Some(old.clone()) }) {
        cleanup(&created_files, &created_dirs, dest, dest_existed);
        return Err(format!("{e} Nothing was changed: the app keeps using {old}."));
    }
    Ok(MoveReport { old_root: old, new_root: new, files, bytes, mode: "copied".into() })
}

fn cleanup(files: &[PathBuf], dirs: &[PathBuf], dest: &Path, dest_existed: bool) {
    for f in files.iter().rev() {
        let _ = std::fs::remove_file(f);
    }
    for d in dirs.iter().rev() {
        let _ = std::fs::remove_dir(d);
    }
    if !dest_existed {
        let _ = std::fs::remove_dir(dest);
    }
}

fn make_dir(d: &Path, created_dirs: &mut Vec<PathBuf>) -> Result<(), String> {
    if d.exists() {
        return Ok(());
    }
    if let Some(p) = d.parent() {
        make_dir(p, created_dirs)?;
    }
    std::fs::create_dir(d).map_err(|e| io_msg(d, &e))?;
    created_dirs.push(d.to_path_buf());
    Ok(())
}

fn copy_and_verify(src: &Path, dest: &Path, created_files: &mut Vec<PathBuf>, created_dirs: &mut Vec<PathBuf>) -> Result<(usize, u64), String> {
    make_dir(dest, created_dirs)?;
    check_writable(dest)?;
    let files = list_copy_files(src)?;
    let src_db = src.join(DB_FILE);
    let mut bytes = 0u64;
    for (rel, _) in &files {
        if rel == Path::new(DB_FILE) {
            continue;
        }
        let to = dest.join(rel);
        if let Some(p) = to.parent() {
            make_dir(p, created_dirs)?;
        }
        std::fs::copy(src.join(rel), &to).map_err(|e| io_msg(&to, &e))?;
        created_files.push(to);
    }
    // the database goes through SQLite, so the copy is consistent even if a write-ahead file exists
    let dest_db = dest.join(DB_FILE);
    if src_db.is_file() {
        let c = open_ro(&src_db)?;
        let q = dest_db.to_string_lossy().replace('\'', "''");
        c.execute_batch(&format!("VACUUM INTO '{q}'")).map_err(|e| format!("could not copy the database to {}: {e}", dest_db.display()))?;
        created_files.push(dest_db.clone());
    }
    // verify: same file set and sizes, and the copied database is sound with the same tables and row counts
    let copied = list_copy_files(dest)?;
    let want: Vec<&(PathBuf, u64)> = files.iter().filter(|f| f.0 != Path::new(DB_FILE)).collect();
    let got: Vec<&(PathBuf, u64)> = copied.iter().filter(|f| f.0 != Path::new(DB_FILE)).collect();
    if want != got {
        return Err("The copy does not match the original (files or sizes differ).".to_string());
    }
    if src_db.is_file() {
        if !dest_db.is_file() {
            return Err("The database was not copied.".to_string());
        }
        db_integrity(&dest_db).map_err(|e| format!("The copy failed its check: {e}."))?;
        if table_counts(&open_ro(&src_db)?)? != table_counts(&open_ro(&dest_db)?)? {
            return Err("The copied database does not have the same rows as the original.".to_string());
        }
    }
    let total: u64 = files.iter().filter(|f| f.0 != Path::new(DB_FILE)).map(|f| f.1).sum::<u64>() + std::fs::metadata(&dest_db).map(|m| m.len()).unwrap_or(0);
    Ok((files.len(), total + bytes))
}

// ---------------------------------------------------------------- deleting the old copy

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteReport {
    pub removed: Vec<String>,
    pub kept: Vec<String>,
}

/// Remove the app's own items from the old folder. Refuses unless the current folder holds a database that passes
/// its integrity check. Only the names in `known_items` and our `*.enc` files go; anything else stays.
pub fn delete_old(old: &Path, current: &Path, config_dir: &Path) -> Result<DeleteReport, String> {
    if same_path(old, current) || is_inside(old, current) || is_inside(current, old) {
        return Err("The old folder and the current data folder overlap, so nothing was deleted.".into());
    }
    if !old.is_dir() {
        let _ = clear_previous(config_dir);
        return Ok(DeleteReport { removed: vec![], kept: vec![] });
    }
    let cur_db = current.join(DB_FILE);
    if !cur_db.is_file() {
        return Err("The current data folder has no database yet, so the old copy was kept.".into());
    }
    db_integrity(&cur_db).map_err(|e| format!("The old copy was kept because the current data failed its check: {e}."))?;
    let mut removed = vec![];
    let mut kept = vec![];
    for e in std::fs::read_dir(old).map_err(|e| io_msg(old, &e))?.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().to_string();
        let ours = DataLayout::known_items().contains(&name.as_str()) || name.ends_with(".enc");
        if !ours {
            if name != POINTER_FILE {
                kept.push(name);
            }
            continue;
        }
        let p = e.path();
        let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        match r {
            Ok(()) => removed.push(name),
            Err(err) => return Err(format!("{} (some of it may already be deleted)", io_msg(&p, &err))),
        }
    }
    // remove the folder itself only when nothing is left, and never the folder that holds the pointer
    if !same_path(old, config_dir) && std::fs::read_dir(old).map(|mut r| r.next().is_none()).unwrap_or(false) {
        let _ = std::fs::remove_dir(old);
    }
    clear_previous(config_dir)?;
    Ok(DeleteReport { removed, kept })
}

/// "Keep it": forget the old folder without touching it.
pub fn clear_previous(config_dir: &Path) -> Result<(), String> {
    if let PointerState::Ok(mut p) = read_pointer(config_dir) {
        p.previous_root = None;
        write_pointer(config_dir, &p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ct-datadir-{tag}-{}-{}", std::process::id(), crate::rpc::now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_db(path: &Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch("CREATE TABLE kv (k TEXT, v TEXT); INSERT INTO kv VALUES ('a','1'),('b','2'); CREATE TABLE positions (id INTEGER); INSERT INTO positions VALUES (1);").unwrap();
    }

    /// A data root shaped like a real one.
    fn make_root(root: &Path) {
        std::fs::create_dir_all(root.join("paper-logs/dip-40pct")).unwrap();
        std::fs::create_dir_all(root.join("recordings")).unwrap();
        std::fs::create_dir_all(root.join("chrome-extension/lib")).unwrap();
        std::fs::create_dir_all(root.join("webview/Default")).unwrap();
        make_db(&root.join(DB_FILE));
        std::fs::write(root.join("paper-logs/2026-11-27.jsonl"), "{\"a\":1}\n").unwrap();
        std::fs::write(root.join("paper-logs/dip-40pct/2026-11-27.jsonl"), "{\"b\":2}\n").unwrap();
        std::fs::write(root.join("paper-logs/summary-2026-11-27.json"), "{}").unwrap();
        std::fs::write(root.join("recordings/feed-2026-11-27.jsonl.gz"), vec![7u8; 4096]).unwrap();
        std::fs::write(root.join("helius.key.enc"), "secret-bytes").unwrap();
        std::fs::write(root.join("chrome-extension/manifest.json"), "{\"version\":\"0.1.0\"}").unwrap();
        std::fs::write(root.join("chrome-extension/lib/core.js"), "core").unwrap();
        std::fs::write(root.join("webview/Default/cache"), "cache").unwrap();
    }

    fn plenty(_: &Path) -> Option<u64> {
        Some(u64::MAX / 2)
    }

    #[test]
    fn every_path_is_under_the_data_root() {
        let l = DataLayout::new("D:\\ClaudeTraderData");
        let root = l.root().to_path_buf();
        for (name, p) in l.all() {
            assert!(p.starts_with(&root), "{name} is not under the data root: {}", p.display());
        }
        assert_eq!(l.db(), root.join("claude-trader.db"));
        assert_eq!(l.extension(), root.join("chrome-extension"));
        assert_eq!(l.secrets(), root);
        let b = DataLayout::beside_db("/x/y/claude-trader.db");
        assert_eq!(b.paper_logs(), PathBuf::from("/x/y/paper-logs"));
        assert_eq!(b.recordings(), PathBuf::from("/x/y/recordings"));
        assert_eq!(DataLayout::beside_db("claude-trader.db").root(), Path::new("."));
    }

    #[test]
    fn engine_options_take_every_path_from_the_layout() {
        let cfg = crate::Config::from_json(include_str!("../../config.json")).unwrap();
        let opts = crate::engine::EngineOpts::new(cfg, "/data/root/claude-trader.db");
        let l = DataLayout::new("/data/root");
        assert_eq!(PathBuf::from(&opts.logs_dir), l.paper_logs());
        assert_eq!(opts.secrets_path(), l.secrets());
        assert_eq!(opts.recordings_dir(), Some(l.recordings()));
    }

    #[test]
    fn pointer_file_round_trips_and_handles_missing_and_corrupt() {
        let cfg = tmp("ptr");
        assert_eq!(read_pointer(&cfg), PointerState::Missing);
        let p = Pointer { data_root: "D:\\ClaudeTraderData".into(), previous_root: Some("C:\\old".into()) };
        write_pointer(&cfg, &p).unwrap();
        assert_eq!(read_pointer(&cfg), PointerState::Ok(p.clone()));
        assert!(!cfg.join("data-location.json.ctnew").exists(), "no temp file left");
        std::fs::write(pointer_path(&cfg), "not json").unwrap();
        assert_eq!(read_pointer(&cfg), PointerState::Corrupt);
        std::fs::write(pointer_path(&cfg), "{\"dataRoot\":\"  \"}").unwrap();
        assert_eq!(read_pointer(&cfg), PointerState::Corrupt);
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn resolve_default_first_run_pointer_and_missing_folder() {
        let base = tmp("res");
        let (def, cfg, d) = (base.join("default"), base.join("default"), base.join("D"));
        std::fs::create_dir_all(&def).unwrap();
        // nothing yet: first run, default root
        let r = resolve(&def, &cfg);
        assert!(r.first_run && !r.from_pointer);
        assert_eq!(r.layout.root(), def.as_path());
        // data in the default folder: not a first run
        make_db(&def.join(DB_FILE));
        assert!(!resolve(&def, &cfg).first_run);
        // a pointer wins
        write_pointer(&cfg, &Pointer { data_root: d.to_string_lossy().to_string(), previous_root: None }).unwrap();
        let r = resolve(&def, &cfg);
        assert!(r.from_pointer && !r.first_run);
        assert_eq!(r.layout.root(), d.as_path());
        assert!(d.is_dir(), "the chosen folder is created");
        // a folder that cannot be made (a file is in the way): default root, a warning, pointer untouched
        let blocked = base.join("blocked");
        std::fs::write(&blocked, "file").unwrap();
        write_pointer(&cfg, &Pointer { data_root: blocked.join("sub").to_string_lossy().to_string(), previous_root: None }).unwrap();
        let r = resolve(&def, &cfg);
        assert!(!r.from_pointer && r.warning.as_deref().unwrap().contains("not available"));
        assert_eq!(r.layout.root(), def.as_path());
        assert!(matches!(read_pointer(&cfg), PointerState::Ok(p) if p.data_root.ends_with("sub")), "pointer is left alone");
        // a corrupt pointer: warning, never a first run (it must not ask again over existing data)
        std::fs::write(pointer_path(&cfg), "{{").unwrap();
        let r = resolve(&def, &cfg);
        assert!(r.warning.is_some() && !r.first_run);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn destination_checks() {
        let base = tmp("dest");
        let src = base.join("src");
        make_root(&src);
        assert!(inspect_destination(&src, &src).unwrap_err().contains("already uses"));
        assert!(inspect_destination(&src, &src.join("inner")).unwrap_err().contains("outside"));
        assert!(inspect_destination(&src.join("inner"), &base).unwrap_err().contains("does not contain"));
        assert!(inspect_destination(&src, Path::new("relative/dir")).unwrap_err().contains("full folder path"));
        assert_eq!(inspect_destination(&src, &base.join("new")).unwrap(), DestKind::Empty);
        let empty = base.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(inspect_destination(&src, &empty).unwrap(), DestKind::Empty);
        let full = base.join("full");
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("a.txt"), "x").unwrap();
        std::fs::write(full.join("b.txt"), "x").unwrap();
        assert_eq!(inspect_destination(&src, &full).unwrap(), DestKind::NotEmpty { count: 2 });
        let older = base.join("older");
        make_root(&older);
        assert_eq!(inspect_destination(&src, &older).unwrap(), DestKind::ExistingData);
        let file = base.join("afile");
        std::fs::write(&file, "x").unwrap();
        assert!(inspect_destination(&src, &file).unwrap_err().contains("file, not a folder"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_non_empty_destination_is_refused_and_nothing_is_touched() {
        let base = tmp("nonempty");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("dest"));
        make_root(&src);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("holiday.jpg"), "x").unwrap();
        let e = move_data(&src, &dest, &cfg, &plenty).unwrap_err();
        assert!(e.contains("is not empty") && e.contains("1 item"), "{e}");
        assert_eq!(read_pointer(&cfg), PointerState::Missing, "pointer not written");
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1, "destination untouched");
        assert!(src.join(DB_FILE).is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn low_disk_space_is_reported_before_anything_is_copied() {
        let base = tmp("space");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("dest"));
        make_root(&src);
        let tiny = |_: &Path| Some(1024u64);
        let e = move_data(&src, &dest, &cfg, &tiny).unwrap_err();
        assert!(e.contains("Not enough free space"), "{e}");
        assert!(!dest.exists(), "nothing created");
        assert_eq!(read_pointer(&cfg), PointerState::Missing);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn move_copies_verifies_then_switches_and_never_deletes_the_source() {
        let base = tmp("move");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("D").join("ClaudeTraderData"));
        make_root(&src);
        let before = list_copy_files(&src).unwrap();
        let r = move_data(&src, &dest, &cfg, &plenty).unwrap();
        assert_eq!(r.mode, "copied");
        // copied: every file, same sizes (the database is a fresh, equal copy)
        for (rel, len) in &before {
            if rel == Path::new(DB_FILE) {
                continue;
            }
            assert_eq!(std::fs::metadata(dest.join(rel)).unwrap().len(), *len, "{}", rel.display());
        }
        db_integrity(&dest.join(DB_FILE)).unwrap();
        assert_eq!(table_counts(&open_ro(&dest.join(DB_FILE)).unwrap()).unwrap(), table_counts(&open_ro(&src.join(DB_FILE)).unwrap()).unwrap());
        // the window cache is not copied
        assert!(!dest.join("webview").exists());
        // switched: the pointer names the new root and remembers the old one
        match read_pointer(&cfg) {
            PointerState::Ok(p) => {
                assert_eq!(p.data_root, dest.to_string_lossy());
                assert_eq!(p.previous_root.as_deref(), Some(src.to_string_lossy().as_ref()));
            }
            other => panic!("{other:?}"),
        }
        // the source is untouched, all of it
        assert_eq!(list_copy_files(&src).unwrap(), before);
        assert!(src.join("webview/Default/cache").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failed_copy_leaves_the_pointer_alone_and_removes_the_partial_copy() {
        let base = tmp("fail");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("dest"));
        make_root(&src);
        // a database that is not a database: the copy fails after the plain files were copied
        std::fs::write(src.join(DB_FILE), "this is not sqlite at all, just some text to fill a header page....").unwrap();
        let e = move_data(&src, &dest, &cfg, &plenty).unwrap_err();
        assert!(e.contains("Nothing was changed"), "{e}");
        assert_eq!(read_pointer(&cfg), PointerState::Missing, "pointer not switched");
        assert!(!dest.exists(), "partial copy removed: {:?}", std::fs::read_dir(&dest).map(|r| r.count()));
        assert!(src.join("paper-logs/2026-11-27.jsonl").is_file(), "source untouched");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_pointer_that_cannot_be_written_undoes_the_copy() {
        let base = tmp("ptrfail");
        let (src, dest) = (base.join("src"), base.join("dest"));
        make_root(&src);
        // the "config folder" is a file, so the pointer cannot be written
        let cfg = base.join("cfgfile");
        std::fs::write(&cfg, "x").unwrap();
        let e = move_data(&src, &dest, &cfg, &plenty).unwrap_err();
        assert!(e.contains("Nothing was changed"), "{e}");
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn choosing_an_existing_data_folder_switches_without_copying() {
        let base = tmp("exist");
        let (src, cfg, other) = (base.join("src"), base.join("cfg"), base.join("older"));
        make_root(&src);
        make_root(&other);
        std::fs::write(other.join("paper-logs/marker.txt"), "keep").unwrap();
        let r = move_data(&src, &other, &cfg, &plenty).unwrap();
        assert_eq!(r.mode, "existing");
        assert!(matches!(read_pointer(&cfg), PointerState::Ok(p) if p.data_root == other.to_string_lossy()));
        assert!(other.join("paper-logs/marker.txt").is_file(), "existing data untouched");
        // a broken database in that folder is refused
        let bad = base.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join(DB_FILE), "garbage garbage garbage garbage garbage garbage garbage garbage").unwrap();
        let cfg2 = base.join("cfg2");
        assert!(move_data(&src, &bad, &cfg2, &plenty).unwrap_err().contains("Cannot use that folder"));
        assert_eq!(read_pointer(&cfg2), PointerState::Missing);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_old_only_after_a_verified_copy_and_only_our_items() {
        let base = tmp("del");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("dest"));
        make_root(&src);
        std::fs::write(src.join("my-notes.txt"), "mine").unwrap();
        move_data(&src, &dest, &cfg, &plenty).unwrap();
        // refuses when the new data is not sound (never delete before a verified copy)
        std::fs::write(dest.join(DB_FILE), "garbage garbage garbage garbage garbage garbage garbage garbage").unwrap();
        assert!(delete_old(&src, &dest, &cfg).unwrap_err().contains("kept"));
        assert!(src.join(DB_FILE).is_file() && src.join("paper-logs").is_dir());
        // refuses overlapping or identical folders
        assert!(delete_old(&dest, &dest, &cfg).is_err());
        assert!(delete_old(&base, &dest, &cfg).is_err());
        // with a sound copy it removes our items, leaves foreign files, and clears the pointer's memory
        std::fs::remove_file(dest.join(DB_FILE)).unwrap();
        std::fs::copy(src.join(DB_FILE), dest.join(DB_FILE)).unwrap();
        let rep = delete_old(&src, &dest, &cfg).unwrap();
        assert!(rep.removed.contains(&"claude-trader.db".to_string()) && rep.removed.contains(&"paper-logs".to_string()) && rep.removed.contains(&"helius.key.enc".to_string()));
        assert_eq!(rep.kept, vec!["my-notes.txt".to_string()]);
        assert!(src.join("my-notes.txt").is_file());
        assert!(!src.join("paper-logs").exists() && !src.join("chrome-extension").exists());
        assert!(matches!(read_pointer(&cfg), PointerState::Ok(p) if p.previous_root.is_none()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_old_never_removes_the_folder_that_holds_the_pointer() {
        let base = tmp("delptr");
        let (src, dest) = (base.join("src"), base.join("dest"));
        make_root(&src);
        // the default root is also the config folder, so the pointer lives in it
        move_data(&src, &dest, &src, &plenty).unwrap();
        assert!(src.join(POINTER_FILE).is_file());
        delete_old(&src, &dest, &src).unwrap();
        assert!(src.is_dir() && src.join(POINTER_FILE).is_file(), "pointer survives");
        assert!(matches!(read_pointer(&src), PointerState::Ok(p) if p.data_root == dest.to_string_lossy() && p.previous_root.is_none()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_wal_is_folded_in_so_a_database_with_unflushed_writes_copies_whole() {
        let base = tmp("wal");
        let (src, cfg, dest) = (base.join("src"), base.join("cfg"), base.join("dest"));
        std::fs::create_dir_all(&src).unwrap();
        let c = rusqlite::Connection::open(src.join(DB_FILE)).unwrap();
        c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1),(2),(3);").unwrap();
        // c stays open, so the rows may only be in the -wal file
        move_data(&src, &dest, &cfg, &plenty).unwrap();
        let n: i64 = open_ro(&dest.join(DB_FILE)).unwrap().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 3);
        drop(c);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn extension_folder_move_is_detected() {
        let l = DataLayout::new("/data/new");
        assert!(!extension_moved("", &l.extension()), "first start: nothing to reload");
        assert!(!extension_moved("/data/new/chrome-extension", &l.extension()));
        assert!(extension_moved("/data/old/chrome-extension", &l.extension()));
    }

    #[test]
    fn free_space_probe_answers_for_a_real_folder() {
        let d = tmp("free");
        assert!(free_bytes(&d.join("not/yet/made")).unwrap_or(1) > 0);
        assert_eq!(human_bytes(1_500_000), "1.5 MB");
        let _ = std::fs::remove_dir_all(&d);
    }
}
