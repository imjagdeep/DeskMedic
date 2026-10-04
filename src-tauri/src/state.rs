//! App state shared by commands. One mutex: operations are short and the app
//! has one user at a time.

use dm_core::scan::{Progress, Tree};
use dm_core::Settings;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct AppState(Mutex<Inner>);

/// A running scan, so the page can pick it up again after you switch pages.
pub struct Scanning {
    pub root: String,
    /// Used space on the drive, for a percentage while walking.
    pub used: u64,
    pub progress: Arc<Progress>,
}

pub struct Inner {
    pub data_dir: PathBuf,
    pub settings: Settings,
    /// Last finished scan (the UI browses it).
    pub tree: Option<Arc<Tree>>,
    /// The scan in progress, if any.
    pub scanning: Option<Scanning>,
    /// A fix-it is running (one at a time).
    pub fixing: bool,
}

impl AppState {
    pub fn new(data_dir: PathBuf, settings: Settings) -> Self {
        AppState(Mutex::new(Inner {
            data_dir,
            settings,
            tree: None,
            scanning: None,
            fixing: false,
        }))
    }

    /// A panicked holder must not brick the app: recover the data.
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Inner {
    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join(dm_core::log::FILE_NAME)
    }

    /// Write an audit entry. A log failure must not undo work already done,
    /// so it is reported in the app log instead of failing the command.
    pub fn record(&self, entry: &dm_core::log::LogEntry) {
        if let Err(e) = dm_core::log::append(&self.log_path(), entry) {
            tracing::error!("could not write the audit log: {e}");
        }
    }
}
