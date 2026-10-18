mod assistant;
mod commands;
mod desklab;
mod state;
mod tray;

use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Started at sign-in: stay in the tray without opening the window.
const BACKGROUND_FLAG: &str = "--background";

/// The "still running in the tray" popup, once per run.
static TOLD_ABOUT_TRAY: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: opening DeskMedic again shows the running copy.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![BACKGROUND_FLAG]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        // Reopen where it was left (size, position, maximized), but never
        // show a window that starts hidden in the tray.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // As administrator, open the shared folder up for standard
            // users first, so this copy and the tray copy share one place.
            if dm_core::sys::is_elevated() && std::env::var_os("DESKMEDIC_DATA_DIR").is_none() {
                if let Some(dir) = dm_core::paths::shared_dir() {
                    if let Err(e) = dm_core::paths::share_with_users(&dir) {
                        tracing::warn!("could not share {}: {e}", dir.display());
                    }
                }
            }
            let data_dir = dm_core::paths::data_dir().ok_or(
                "no writable folder for settings (ProgramData and LocalAppData both failed)",
            )?;
            let mut settings = dm_core::Settings::load(&data_dir);
            // First run: an installed copy starts with Windows (so the
            // assistant can watch); the portable exe doesn't, since it may
            // live on a USB stick.
            if !settings.start_default_applied {
                settings.start_with_windows = dm_core::sys::is_installed();
                settings.start_default_applied = true;
                if let Err(e) = settings.save(&data_dir) {
                    tracing::warn!("could not save first-run settings: {e}");
                }
            }
            if let Err(e) = dm_core::log::compact(
                &data_dir.join(dm_core::log::FILE_NAME),
                settings.keep_log_days,
            ) {
                tracing::warn!("log compaction failed: {e}");
            }
            let start_with_windows = settings.start_with_windows;
            app.manage(AppState::new(data_dir, settings));
            if let Err(e) = commands::apply_autostart(app.handle(), start_with_windows) {
                tracing::warn!("{e}");
            }
            let background = std::env::args().any(|a| a == BACKGROUND_FLAG);

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
                .visible(!background)
                .build()?;
            tray::build(app.handle())?;
            assistant::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // With the assistant on, closing the window keeps DeskMedic in
            // the tray; Quit is in the tray menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if app.state::<AppState>().lock().settings.assistant {
                    api.prevent_close();
                    let _ = window.hide();
                    if !TOLD_ABOUT_TRAY.swap(true, Ordering::SeqCst) {
                        assistant::notify(
                            app,
                            "DeskMedic is still watching",
                            "It keeps checking this PC from the tray and tells you if something looks wrong. Right-click the tray icon to quit.",
                        );
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::restart_as_admin,
            commands::save_settings,
            commands::open_data_folder,
            commands::open_link,
            commands::desklab_tools,
            commands::open_tool,
            commands::list_drives,
            commands::read_log,
            commands::export_log,
            commands::scan_start,
            commands::scan_cancel,
            commands::scan_state,
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
            commands::remedy_library,
            commands::remedy_open,
            commands::fix_run,
            commands::health_run,
            commands::insights_run,
            commands::network_run,
            commands::send_to_it,
            commands::pc_summary,
            commands::assistant_status,
            commands::notify_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DeskMedic");
}
