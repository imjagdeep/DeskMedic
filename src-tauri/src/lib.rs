mod commands;
mod state;

use state::AppState;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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

            // The window is built here rather than in tauri.conf.json so an
            // elevated copy gets its own WebView2 folder: elevated and normal
            // processes can't share one, and "Restart as administrator"
            // briefly runs both.
            let webview_dir =
                app.path()
                    .app_local_data_dir()?
                    .join(if dm_core::sys::is_elevated() {
                        "webview-admin"
                    } else {
                        "webview"
                    });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("DeskMedic")
                .inner_size(1120.0, 740.0)
                .min_inner_size(860.0, 560.0)
                .disable_drag_drop_handler()
                .data_directory(webview_dir)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::restart_as_admin,
            commands::save_settings,
            commands::open_data_folder,
            commands::open_link,
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
            commands::disk_report,
            commands::disk_action,
            commands::extend_plan,
            commands::extend_run,
            commands::resize_info,
            commands::resize,
            commands::fix_list,
            commands::fix_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeskMedic");
}
