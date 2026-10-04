//! App state shared by commands. One mutex: operations are short and the app
//! has one user at a time.

use dm_core::Settings;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

pub struct AppState(Mutex<Inner>);

pub struct Inner {
    pub data_dir: PathBuf,
    pub settings: Settings,
}

impl AppState {
    pub fn new(data_dir: PathBuf, settings: Settings) -> Self {
        AppState(Mutex::new(Inner { data_dir, settings }))
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
