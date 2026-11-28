//! Settings > Storage: where the data folder is, moving it, the first-run question, deleting the old copy.
//! The logic (paths, pointer, copy, verify, switch) is in `engine::datadir` and tested there.

use crate::{start_engine, AppPaths, EngineCtl};
use engine::datadir::{self, DataLayout};
use engine::hub::Hub;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageItem {
    name: String,
    path: String,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageView {
    root: String,
    default_root: String,
    is_default: bool,
    first_run: bool,
    warning: Option<String>,
    /// The folder the data was moved away from, while it still exists.
    previous_root: Option<String>,
    total_bytes: u64,
    free_bytes: Option<u64>,
    items: Vec<StorageItem>,
    pointer_file: String,
    extension_folder: String,
    /// The extension folder changed and Chrome has not been told yet.
    extension_notice: bool,
}

pub const EXT_NOTICE_KEY: &str = "storage.extension_notice";

fn root_of(paths: &AppPaths) -> PathBuf {
    paths.layout.root().to_path_buf()
}

#[tauri::command]
pub async fn get_storage(paths: tauri::State<'_, AppPaths>) -> Result<StorageView, String> {
    let (layout, default_root, config_dir, first_run, warning, db_path) = (paths.layout.clone(), paths.default_root.clone(), paths.config_dir.clone(), paths.first_run(), paths.warning.clone(), paths.db.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let root = layout.root().to_path_buf();
        let previous = match datadir::read_pointer(&config_dir) {
            datadir::PointerState::Ok(p) => p.previous_root.map(PathBuf::from).filter(|p| p.is_dir() && !datadir::same_path(p, &root)),
            _ => None,
        };
        let items = layout
            .all()
            .into_iter()
            .filter(|(n, _)| *n != "Logs" && *n != "Caches")
            .map(|(n, p)| {
                let bytes = if p.is_dir() { datadir::dir_size(&p) } else { std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) };
                StorageItem { name: n.to_string(), path: p.to_string_lossy().to_string(), bytes }
            })
            .collect();
        let notice = !first_run && engine::db::Db::open(&db_path).ok().and_then(|d| d.kv_get(EXT_NOTICE_KEY)).as_deref() == Some("1");
        StorageView {
            root: root.to_string_lossy().to_string(),
            default_root: default_root.to_string_lossy().to_string(),
            is_default: datadir::same_path(&root, &default_root),
            first_run,
            warning,
            previous_root: previous.map(|p| p.to_string_lossy().to_string()),
            total_bytes: datadir::dir_size(&root),
            free_bytes: datadir::free_bytes(&root),
            items,
            pointer_file: datadir::pointer_path(&config_dir).to_string_lossy().to_string(),
            extension_folder: layout.extension().to_string_lossy().to_string(),
            extension_notice: notice,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// What would happen if the data went to `dest` (shown in the dialog before the person confirms).
#[tauri::command]
pub async fn storage_check(dest: String, paths: tauri::State<'_, AppPaths>) -> Result<datadir::MoveCheck, String> {
    let root = root_of(&paths);
    tauri::async_runtime::spawn_blocking(move || datadir::check_move(&root, Path::new(&dest), &datadir::free_bytes)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn storage_show_folder(paths: tauri::State<AppPaths>) -> Result<(), String> {
    crate::startup::show_folder(paths.layout.root())
}

#[tauri::command]
pub fn storage_ack_extension(paths: tauri::State<AppPaths>) -> Result<(), String> {
    crate::open_db(&paths)?.kv_set(EXT_NOTICE_KEY, "0");
    Ok(())
}

/// Move the data folder. Order: close paper positions the normal way, stop the engine, copy, verify, switch the
/// pointer, restart. A failure before the switch leaves everything as it was and the engine is started again.
#[tauri::command]
pub async fn storage_move(dest: String, app: tauri::AppHandle, paths: tauri::State<'_, AppPaths>, hub: tauri::State<'_, Arc<Hub>>, ctl: tauri::State<'_, EngineCtl>) -> Result<datadir::MoveReport, String> {
    if paths.first_run() {
        return Err("Choose where to keep the data first.".into());
    }
    let root = root_of(&paths);
    let dest = PathBuf::from(dest);
    // 1. check everything that can be checked before anything is stopped
    let chk = {
        let (r, d) = (root.clone(), dest.clone());
        tauri::async_runtime::spawn_blocking(move || datadir::check_move(&r, &d, &datadir::free_bytes)).await.map_err(|e| e.to_string())?
    };
    if !chk.ok {
        return Err(chk.message);
    }
    // 2. pause trading: new entries stop and open paper positions are sold through the normal exit path
    hub.command("update-close");
    let mut open = hub.command("open-count").parse::<u32>().unwrap_or(0);
    for _ in 0..120 {
        if open == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        open = hub.command("open-count").parse().unwrap_or(0);
    }
    if open > 0 {
        hub.command("update-close-cancel");
        return Err(format!("{open} paper position{} could not be closed yet, so nothing was moved. Try again in a minute.", if open == 1 { "" } else { "s" }));
    }
    // 3. stop the engine so no file is being written
    hub.log("storage: engine stopped to move the data folder".to_string());
    ctl.stop_and_wait().await;
    // 4. copy, verify, switch
    let (r, d, c) = (root.clone(), dest.clone(), paths.config_dir.clone());
    let res = tauri::async_runtime::spawn_blocking(move || datadir::move_data(&r, &d, &c, &datadir::free_bytes)).await.map_err(|e| e.to_string())?;
    match res {
        Err(e) => {
            hub.log(format!("storage: move failed, nothing changed: {e}"));
            start_engine(&app)?;
            Err(e)
        }
        Ok(rep) => {
            hub.log(format!("storage: data folder moved to {} ({} files, {}); restarting", rep.new_root, rep.files, datadir::human_bytes(rep.bytes)));
            restart_soon(app);
            Ok(rep)
        }
    }
}

/// The first launch asked where to keep the data. `dest` None means the default folder.
#[tauri::command]
pub async fn storage_first_run(dest: Option<String>, app: tauri::AppHandle, paths: tauri::State<'_, AppPaths>) -> Result<(), String> {
    if !paths.first_run() {
        return Err("The data folder is already chosen. Use Settings > Storage to change it.".into());
    }
    match dest.filter(|d| !d.trim().is_empty()) {
        None => {
            datadir::check_writable(&paths.default_root)?;
            paths.set_first_run(false);
            crate::boot(&app);
            Ok(())
        }
        Some(d) => {
            let d = PathBuf::from(d);
            match datadir::inspect_destination(&paths.default_root, &d)? {
                datadir::DestKind::NotEmpty { count } => return Err(datadir::not_empty_message(&d, count)),
                datadir::DestKind::ExistingData => datadir::db_integrity(&d.join(datadir::DB_FILE)).map_err(|e| format!("Cannot use that folder: {e}."))?,
                datadir::DestKind::Empty => {}
            }
            datadir::check_writable(&d)?;
            datadir::write_pointer(&paths.config_dir, &datadir::Pointer { data_root: d.to_string_lossy().to_string(), previous_root: None })?;
            restart_soon(app);
            Ok(())
        }
    }
}

/// After a move: delete the old copy. Refused unless the data in the new folder passes its check.
#[tauri::command]
pub async fn storage_delete_old(paths: tauri::State<'_, AppPaths>) -> Result<datadir::DeleteReport, String> {
    let (cfg, cur) = (paths.config_dir.clone(), root_of(&paths));
    tauri::async_runtime::spawn_blocking(move || {
        let old = match datadir::read_pointer(&cfg) {
            datadir::PointerState::Ok(p) => p.previous_root.ok_or("There is no old copy to delete.")?,
            _ => return Err("There is no old copy to delete.".to_string()),
        };
        datadir::delete_old(Path::new(&old), &cur, &cfg)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn storage_keep_old(paths: tauri::State<AppPaths>) -> Result<(), String> {
    datadir::clear_previous(&paths.config_dir)
}

fn restart_soon(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        app.restart();
    });
}

/// The Tauri window cache folder for this start (the one under the data root, or a small boot folder on the very first run).
pub fn webview_dir(layout: &DataLayout, default_root: &Path, first_run: bool) -> PathBuf {
    if first_run {
        default_root.join("webview-boot")
    } else {
        layout.webview()
    }
}
