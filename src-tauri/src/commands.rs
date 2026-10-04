//! Tauri commands: thin adapters over dm-core. Anything slow runs on a
//! blocking thread so the window never freezes.

use crate::state::AppState;
use dm_core::drives::{Drive, DriveKind};
use dm_core::log::{Area, LogEntry};
use dm_core::profiles::Profile;
use dm_core::scan::tree::{BigFile, Listing, Summary, TypeTotal};
use dm_core::scan::{Progress, ScanError, Tree};
use dm_core::Settings;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
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

// ---------- disk map (scan) ----------

/// Start scanning a drive in the background. Progress arrives as
/// `scan-progress` events, the end as `scan-done` or `scan-failed`.
#[tauri::command]
pub fn scan_start(app: AppHandle, state: State<'_, AppState>, root: String) -> CmdResult<()> {
    // Only drives Windows lists right now, and only local ones.
    let drive = dm_core::drives::list()
        .into_iter()
        .find(|d| d.root.eq_ignore_ascii_case(&root))
        .ok_or_else(|| format!("{root} is not a drive on this PC"))?;
    if !matches!(drive.kind, DriveKind::Fixed | DriveKind::Removable) {
        return Err("Only local drives can be scanned.".into());
    }
    let progress = Arc::new(Progress::default());
    {
        let mut g = state.lock();
        if g.scanning.is_some() {
            return Err("A scan is already running.".into());
        }
        g.scanning = Some(progress.clone());
    }
    let allow_mft = dm_core::sys::is_elevated() && drive.file_system.eq_ignore_ascii_case("NTFS");

    // Progress ticker.
    let ticker = progress.clone();
    let app_t = app.clone();
    std::thread::spawn(move || {
        while Arc::strong_count(&ticker) > 1 {
            let _ = app_t.emit("scan-progress", ticker.snapshot());
            std::thread::sleep(Duration::from_millis(300));
        }
    });

    std::thread::spawn(move || {
        let result =
            dm_core::scan::scan_drive(std::path::Path::new(&drive.root), allow_mft, &progress);
        let state = app.state::<AppState>();
        let mut g = state.lock();
        g.scanning = None;
        drop(progress); // lets the ticker stop
        match result {
            Ok(tree) => {
                let summary = tree.summary.clone();
                g.record(&LogEntry::new(
                    Area::Scan,
                    &format!("Scan {}", drive.root),
                    true,
                    format!(
                        "{} files, {:.1} GB in {:.1} s ({})",
                        summary.files,
                        summary.bytes as f64 / 1e9,
                        summary.millis as f64 / 1000.0,
                        match summary.method {
                            dm_core::scan::ScanMethod::Mft => "fast NTFS scan",
                            dm_core::scan::ScanMethod::Walk => "folder-by-folder scan",
                        }
                    ),
                ));
                g.tree = Some(Arc::new(tree));
                drop(g);
                let _ = app.emit("scan-done", summary);
            }
            Err(ScanError::Cancelled) => {
                drop(g);
                let _ = app.emit("scan-failed", "Scan cancelled.");
            }
            Err(e) => {
                drop(g);
                let _ = app.emit("scan-failed", format!("Scan failed: {e}"));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn scan_cancel(state: State<'_, AppState>) {
    if let Some(p) = &state.lock().scanning {
        p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn tree(state: &State<'_, AppState>) -> CmdResult<Arc<Tree>> {
    state
        .lock()
        .tree
        .clone()
        .ok_or_else(|| "No scan yet.".into())
}

#[tauri::command]
pub fn scan_summary(state: State<'_, AppState>) -> Option<Summary> {
    state.lock().tree.as_ref().map(|t| t.summary.clone())
}

#[tauri::command]
pub fn scan_listing(
    state: State<'_, AppState>,
    id: Option<u32>,
    limit: usize,
) -> CmdResult<Listing> {
    let t = tree(&state)?;
    t.listing(id.unwrap_or(t.root_id()), limit.clamp(1, 500))
        .ok_or_else(|| "That folder is not in the scan.".into())
}

#[tauri::command]
pub fn scan_top_files(state: State<'_, AppState>, n: usize) -> CmdResult<Vec<BigFile>> {
    Ok(tree(&state)?.top_files(n.min(100)))
}

#[tauri::command]
pub fn scan_types(state: State<'_, AppState>, n: usize) -> CmdResult<Vec<TypeTotal>> {
    Ok(tree(&state)?.types(n.min(200)))
}

/// Show a scanned file or folder in Explorer. Only ids from the scan are
/// accepted, so the path always comes from the disk itself.
#[tauri::command]
pub fn reveal_node(app: AppHandle, state: State<'_, AppState>, id: u32) -> CmdResult<()> {
    let path = tree(&state)?
        .path(id)
        .ok_or("That item is not in the scan.")?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| format!("could not open {}: {e}", path.display()))
}

#[tauri::command]
pub async fn user_profiles(state: State<'_, AppState>) -> CmdResult<Vec<Profile>> {
    let tree = state.lock().tree.clone();
    blocking(move || {
        let mut list = dm_core::profiles::list().map_err(|e| e.to_string())?;
        if let Some(t) = tree {
            for p in &mut list {
                p.size = t.size_of(std::path::Path::new(&p.path));
            }
        }
        Ok(list)
    })
    .await
}

// ---------- cleanup ----------

#[tauri::command]
pub async fn cleanup_preview(all_users: bool) -> CmdResult<Vec<dm_core::cleanup::Estimate>> {
    blocking(move || {
        let ctx = dm_core::cleanup::Ctx::detect(all_users);
        Ok(dm_core::cleanup::preview(
            &ctx,
            &std::sync::atomic::AtomicBool::new(false),
        ))
    })
    .await
}

#[derive(Serialize)]
pub struct CleanupReport {
    outcomes: Vec<dm_core::cleanup::Outcome>,
    free_before: u64,
    free_after: u64,
}

fn system_free() -> u64 {
    dm_core::drives::list()
        .into_iter()
        .find(|d| d.is_system)
        .map(|d| d.free_bytes)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn cleanup_run(
    app: AppHandle,
    ids: Vec<String>,
    all_users: bool,
) -> CmdResult<CleanupReport> {
    if ids.is_empty() {
        return Err("Nothing selected.".into());
    }
    let report = blocking(move || {
        let ctx = dm_core::cleanup::Ctx::detect(all_users);
        let free_before = system_free();
        let outcomes =
            dm_core::cleanup::run(&ids, &ctx, &std::sync::atomic::AtomicBool::new(false));
        Ok(CleanupReport {
            outcomes,
            free_before,
            free_after: system_free(),
        })
    })
    .await?;

    let gained = report.free_after.saturating_sub(report.free_before);
    let details: Vec<String> = report
        .outcomes
        .iter()
        .map(|o| {
            let size = o
                .freed
                .map(|b| format!("{:.1} MB", b as f64 / 1e6))
                .unwrap_or_else(|| "size n/a".into());
            let mut line = format!(
                "{} {}: {size}, {} files",
                if o.ok { "OK" } else { "--" },
                o.name,
                o.files
            );
            if !o.note.is_empty() {
                line.push_str(&format!(" ({})", o.note));
            }
            line
        })
        .collect();
    let all_ok = report.outcomes.iter().all(|o| o.ok);
    let state = app.state::<AppState>();
    state.lock().record(
        &LogEntry::new(
            Area::Cleanup,
            "Cleanup",
            all_ok,
            format!(
                "C: free {:.1} GB -> {:.1} GB (+{:.2} GB){}",
                report.free_before as f64 / 1e9,
                report.free_after as f64 / 1e9,
                gained as f64 / 1e9,
                if all_users { ", all users" } else { "" }
            ),
        )
        .with_details(details),
    );
    Ok(report)
}

// ---------- disks ----------

#[derive(Serialize)]
pub struct DiskReport {
    disks: Vec<dm_core::disk::DiskView>,
    findings: Vec<dm_core::disk::Finding>,
    letters_in_use: Vec<char>,
    windows_letter: char,
    vds_status: String,
    vds_start: String,
}

#[tauri::command]
pub async fn disk_report() -> CmdResult<DiskReport> {
    blocking(|| {
        let layout = dm_core::disk::read().map_err(|e| format!("Could not read the disks: {e}"))?;
        let win = dm_core::disk::windows_letter();
        let in_use = dm_core::disk::letters_in_use(&layout);
        Ok(DiskReport {
            findings: dm_core::disk::diagnose(&layout, win, &in_use),
            disks: layout.views(),
            letters_in_use: in_use,
            windows_letter: win,
            vds_status: layout.vds_status.clone(),
            vds_start: layout.vds_start.clone(),
        })
    })
    .await
}

/// Run one disk action. `confirm` must equal the action's confirm word
/// (typed by the user); the layout is read again and re-checked first.
#[tauri::command]
pub async fn disk_action(
    app: AppHandle,
    action: dm_core::disk::Action,
    confirm: String,
) -> CmdResult<String> {
    if !dm_core::sys::is_elevated() {
        return Err("Disk changes need administrator rights.".into());
    }
    if !confirm.trim().eq_ignore_ascii_case(&action.confirm_word()) {
        return Err(format!("Type {} to confirm.", action.confirm_word()));
    }
    let a = action.clone();
    let result = blocking(move || {
        let layout = dm_core::disk::read().map_err(|e| format!("Could not read the disks: {e}"))?;
        let win = dm_core::disk::windows_letter();
        dm_core::disk::actions::validate(
            &a,
            &layout,
            win,
            &dm_core::disk::letters_in_use(&layout),
        )?;
        dm_core::disk::actions::run(&a, win)
    })
    .await;
    let state = app.state::<AppState>();
    state.lock().record(&LogEntry::new(
        Area::Disk,
        &action.title(),
        result.is_ok(),
        match &result {
            Ok(m) => m.clone(),
            Err(e) => e.clone(),
        },
    ));
    result
}

// ---------- extend / shrink ----------

fn current_plan(letter: char) -> CmdResult<dm_core::disk::extend::Plan> {
    let layout = dm_core::disk::read().map_err(|e| format!("Could not read the disks: {e}"))?;
    let winre = if dm_core::sys::is_elevated() {
        Some(dm_core::disk::extend::winre()?)
    } else {
        None
    };
    Ok(dm_core::disk::extend::plan(
        &layout,
        winre.as_ref(),
        letter.to_ascii_uppercase(),
        dm_core::disk::windows_letter(),
    ))
}

#[tauri::command]
pub async fn extend_plan(letter: char) -> CmdResult<dm_core::disk::extend::Plan> {
    blocking(move || current_plan(letter)).await
}

/// Run the extend plan for `letter`. `expected_steps` is what the user saw;
/// if the plan changed since, nothing runs.
#[tauri::command]
pub async fn extend_run(
    app: AppHandle,
    letter: char,
    expected_steps: usize,
    confirm: String,
) -> CmdResult<Vec<String>> {
    if !dm_core::sys::is_elevated() {
        return Err("Extending needs administrator rights.".into());
    }
    if !confirm.trim().eq_ignore_ascii_case(&letter.to_string()) {
        return Err(format!("Type {letter} to confirm."));
    }
    let app2 = app.clone();
    blocking(move || {
        let plan = current_plan(letter)?;
        if plan.steps.len() != expected_steps {
            return Err("The disk changed since the plan was shown. Look at it again.".into());
        }
        let state = app2.state::<AppState>();
        dm_core::disk::extend::execute(&plan, |step, result| {
            state.lock().record(&LogEntry::new(
                Area::Disk,
                &format!("Extend {letter}: {}", step.describe()),
                result.is_ok(),
                match result {
                    Ok(m) => m.clone(),
                    Err(e) => e.clone(),
                },
            ));
        })
    })
    .await
}

#[tauri::command]
pub async fn resize_info(disk: u32, partition: u32) -> CmdResult<(u64, u64)> {
    if !dm_core::sys::is_elevated() {
        return Err("Resizing needs administrator rights.".into());
    }
    blocking(move || dm_core::disk::extend::supported_size(disk, partition)).await
}

/// Grow or shrink a data partition to `size` bytes, within what Windows allows.
#[tauri::command]
pub async fn resize(
    app: AppHandle,
    disk: u32,
    partition: u32,
    size: u64,
    confirm: String,
) -> CmdResult<String> {
    if !dm_core::sys::is_elevated() {
        return Err("Resizing needs administrator rights.".into());
    }
    let result = blocking(move || {
        let layout = dm_core::disk::read().map_err(|e| format!("Could not read the disks: {e}"))?;
        let p = layout
            .partitions
            .iter()
            .find(|p| p.disk == disk && p.number == partition)
            .ok_or("That partition is gone.")?;
        if p.kind() != dm_core::disk::model::PartKind::Basic || p.letter.is_empty() {
            return Err("Only data partitions with a drive letter can be resized.".into());
        }
        if !confirm.trim().eq_ignore_ascii_case(&p.letter) {
            return Err(format!("Type {} to confirm.", p.letter));
        }
        let (min, max) = dm_core::disk::extend::supported_size(disk, partition)?;
        let size = size / (1024 * 1024) * (1024 * 1024);
        if size < min || size > max {
            return Err(format!(
                "Pick a size between {:.1} and {:.1} GB.",
                min as f64 / 1e9,
                max as f64 / 1e9
            ));
        }
        if size == p.size {
            return Err("That is already its size.".into());
        }
        dm_core::disk::extend::run_step(&dm_core::disk::extend::Step::Extend {
            disk,
            partition,
            size,
        })
        .map(|_| format!("{}: is now {:.1} GB.", p.letter, size as f64 / 1e9))
    })
    .await;
    let state = app.state::<AppState>();
    state.lock().record(&LogEntry::new(
        Area::Disk,
        &format!("Resize partition {partition} on disk {disk}"),
        result.is_ok(),
        match &result {
            Ok(m) => m.clone(),
            Err(e) => e.clone(),
        },
    ));
    result
}
