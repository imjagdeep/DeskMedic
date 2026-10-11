//! The Medic assistant: a background health check every so often (same
//! read-only check as the Health check page, no network), a popup when
//! something new looks wrong, and the tray status line kept current.

use crate::state::AppState;
use crate::tray;
use dm_core::health::Report;
use dm_core::monitor::{self, Seen};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// First background check this long after start, so signing in stays quick.
const FIRST_CHECK: Duration = Duration::from_secs(120);

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut last: Option<Instant> = None;
        loop {
            let (on, every) = {
                let s = app.state::<AppState>();
                let g = s.lock();
                (g.settings.assistant, g.settings.check_every_min)
            };
            let due = last.is_none_or(|t| t.elapsed() >= Duration::from_secs(u64::from(every) * 60));
            if on && due {
                last = Some(Instant::now());
                match dm_core::health::run() {
                    Ok(r) => after_check(&app, &r, true),
                    Err(e) => tracing::warn!("background health check failed: {e}"),
                }
            }
            // Wake up every minute so settings changes apply soon.
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

/// After any health check (background or from the page): update the tray
/// and the page; in the background, also pop up what's new.
pub fn after_check(app: &AppHandle, r: &Report, background: bool) {
    let now = chrono::Utc::now();
    let alerts = {
        let s = app.state::<AppState>();
        let g = s.lock();
        let mut seen = Seen::load(&g.data_dir);
        let alerts = monitor::alerts(&mut seen, r, now);
        if let Err(e) = seen.save(&g.data_dir) {
            tracing::warn!("could not save {}: {e}", monitor::FILE_NAME);
        }
        alerts
    };
    let at = chrono::Local::now().format("%H:%M").to_string();
    tray::refresh(app, r.problems, r.warnings, &at);
    let _ = app.emit("health-updated", r);
    // A check you ran yourself is already on screen: no popup for it.
    if background {
        if let Some((title, body)) = monitor::summarize(&alerts) {
            notify(app, &title, &body);
        }
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        tracing::warn!("notification failed: {e}");
    }
}
