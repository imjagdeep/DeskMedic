//! Tray icon (next to the clock): status line, Health check, Network
//! doctor, Open, Quit. A left click opens the window.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// The status line, updated after each health check.
struct TrayStatus(MenuItem<Wry>);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Not checked yet", false, None::<&str>)?;
    let health = MenuItem::with_id(app, "health", "Run a health check", true, None::<&str>)?;
    let network = MenuItem::with_id(app, "network", "Network doctor", true, None::<&str>)?;
    let cleanup = MenuItem::with_id(app, "cleanup", "Clean up temporary files", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open DeskMedic", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit DeskMedic", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &health,
            &network,
            &cleanup,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("DeskMedic")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "health" => {
                show_main(app);
                let _ = app.emit("navigate", "health-run");
            }
            "network" => {
                show_main(app);
                let _ = app.emit("navigate", "network");
            }
            "cleanup" => {
                show_main(app);
                let _ = app.emit("navigate", "cleanup");
            }
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayStatus(status));
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Status line and tooltip after a health check.
pub fn refresh(app: &AppHandle, problems: usize, warnings: usize, at: &str) {
    let text = match (problems, warnings) {
        (0, 0) => "All good".to_string(),
        (0, w) => format!("{w} warning{}", if w == 1 { "" } else { "s" }),
        (p, 0) => format!("{p} problem{}", if p == 1 { "" } else { "s" }),
        (p, w) => format!(
            "{p} problem{}, {w} warning{}",
            if p == 1 { "" } else { "s" },
            if w == 1 { "" } else { "s" }
        ),
    };
    if let Some(s) = app.try_state::<TrayStatus>() {
        let _ = s.0.set_text(format!("{text} · checked {at}"));
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("DeskMedic: {text}")));
    }
}
