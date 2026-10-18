//! The Medic assistant: a background health check and a read of the event
//! logs every so often (the same read-only checks as the Health check and
//! Assistant pages, no network), a popup when something new looks wrong,
//! and the tray status line kept current.

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
            let (on, every, temp_limit) = {
                let s = app.state::<AppState>();
                let g = s.lock();
                (
                    g.settings.assistant,
                    g.settings.check_every_min,
                    g.settings
                        .temp_reminder
                        .then_some(g.settings.temp_reminder_gb),
                )
            };
            let due =
                last.is_none_or(|t| t.elapsed() >= Duration::from_secs(u64::from(every) * 60));
            if on && due {
                last = Some(Instant::now());
                match dm_core::health::run() {
                    Ok(r) => after_check(&app, &r, true),
                    Err(e) => tracing::warn!("background health check failed: {e}"),
                }
                match dm_core::insights::run() {
                    Ok(r) => after_insights(&app, &r),
                    Err(e) => tracing::warn!("background event log check failed: {e}"),
                }
                if let Some(limit) = temp_limit {
                    check_temp(&app, limit);
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

/// After a background read of the event logs: pop up only problems that
/// are new or happened again since their last popup.
fn after_insights(app: &AppHandle, r: &dm_core::insights::Report) {
    let alerts = {
        let s = app.state::<AppState>();
        let g = s.lock();
        let mut seen = Seen::load(&g.data_dir);
        let alerts = dm_core::insights::alerts(&mut seen, r, chrono::Utc::now());
        if let Err(e) = seen.save(&g.data_dir) {
            tracing::warn!("could not save {}: {e}", monitor::FILE_NAME);
        }
        alerts
    };
    let _ = app.emit("insights-updated", r);
    if let Some((title, body)) = monitor::summarize(&alerts) {
        notify(app, &title, &body);
    }
}

/// Measure what Cleanup could clear (this user, no admin) and remind
/// when it passes the limit. Same measurement as the Cleanup page.
fn check_temp(app: &AppHandle, limit_gb: u32) {
    let ctx = dm_core::cleanup::Ctx::detect(false);
    let estimates = dm_core::cleanup::preview(&ctx, &std::sync::atomic::AtomicBool::new(false));
    let bytes = monitor::temp_bytes(&estimates);
    let alert = {
        let s = app.state::<AppState>();
        let g = s.lock();
        let mut seen = Seen::load(&g.data_dir);
        let alert = monitor::temp_alert(&mut seen, bytes, limit_gb, chrono::Utc::now());
        if let Err(e) = seen.save(&g.data_dir) {
            tracing::warn!("could not save {}: {e}", monitor::FILE_NAME);
        }
        alert
    };
    let _ = app.emit("temp-measured", bytes);
    if let Some(a) = alert {
        notify(app, &a.title, &a.body);
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("notification failed: {e}");
    }
}
