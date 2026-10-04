//! Tauri commands: thin adapters over dm-core. Anything slow runs on a
//! blocking thread so the window never freezes.

use crate::state::AppState;
use dm_core::drives::Drive;
use dm_core::log::{Area, LogEntry};
use dm_core::Settings;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

// ---------- app ----------

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    elevated: bool,
    user: String,
    computer: String,
    data_dir: PathBuf,
    settings: Settings,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    let g = state.lock();
    AppInfo {
        version: app.package_info().version.to_string(),
        elevated: dm_core::sys::is_elevated(),
        user: std::env::var("USERNAME").unwrap_or_default(),
        computer: std::env::var("COMPUTERNAME").unwrap_or_default(),
        data_dir: g.data_dir.clone(),
        settings: g.settings.clone(),
    }
}

/// Start an elevated copy, then close this one. Errors (prompt cancelled)
/// leave this copy running.
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> CmdResult<()> {
    dm_core::sys::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    settings.validate()?;
    let mut g = state.lock();
    settings
        .save(&g.data_dir)
        .map_err(|e| format!("could not save settings: {e}"))?;
    g.settings = settings;
    Ok(())
}

#[tauri::command]
pub fn open_data_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.lock().data_dir.clone();
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("could not open {}: {e}", dir.display()))
}

// ---------- drives ----------

#[tauri::command]
pub async fn list_drives() -> CmdResult<Vec<Drive>> {
    blocking(|| Ok(dm_core::drives::list())).await
}

// ---------- log ----------

#[tauri::command]
pub fn read_log(state: State<'_, AppState>) -> Vec<LogEntry> {
    dm_core::log::read(&state.lock().log_path())
}

/// Save the log as plain text (for a ticket) to a path the user picked in
/// the save dialog.
#[tauri::command]
pub fn export_log(state: State<'_, AppState>, path: PathBuf) -> CmdResult<usize> {
    let g = state.lock();
    let entries = dm_core::log::read(&g.log_path());
    std::fs::write(&path, dm_core::log::to_text(&entries))
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    g.record(&LogEntry::new(
        Area::App,
        "Export log",
        true,
        format!("{} entries to {}", entries.len(), path.display()),
    ));
    Ok(entries.len())
}
