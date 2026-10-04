mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = dm_core::paths::data_dir().ok_or(
                "no writable folder for settings (ProgramData and LocalAppData both failed)",
            )?;
            let settings = dm_core::Settings::load(&data_dir);
            if let Err(e) = dm_core::log::compact(
                &data_dir.join(dm_core::log::FILE_NAME),
                settings.keep_log_days,
            ) {
                tracing::warn!("log compaction failed: {e}");
            }
            app.manage(AppState::new(data_dir, settings));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::restart_as_admin,
            commands::save_settings,
            commands::open_data_folder,
            commands::list_drives,
            commands::read_log,
            commands::export_log,
            commands::scan_start,
            commands::scan_cancel,
            commands::scan_summary,
            commands::scan_listing,
            commands::scan_top_files,
            commands::scan_types,
            commands::reveal_node,
            commands::user_profiles,
            commands::cleanup_preview,
            commands::cleanup_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeskMedic");
}
