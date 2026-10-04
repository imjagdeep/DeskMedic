//! User settings, stored as `settings.json` in the data folder.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system", "light" or "dark".
    pub theme: String,
    /// Log entries older than this are dropped on start.
    pub keep_log_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            keep_log_days: 180,
        }
    }
}

impl Settings {
    /// Missing or unreadable settings fall back to defaults (logged), so a
    /// broken file never stops the app from starting.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE_NAME);
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("ignoring unreadable {}: {e}", path.display());
                Self::default()
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!("cannot read {}: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            return Err(format!("unknown theme \"{}\"", self.theme));
        }
        if !(7..=3650).contains(&self.keep_log_days) {
            return Err("keep the log for 7 to 3650 days".into());
        }
        Ok(())
    }

    /// Write-then-rename so a crash never leaves half a file.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = dir.join("settings.json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, dir.join(FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        let s = Settings {
            theme: "dark".into(),
            keep_log_days: 30,
        };
        s.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), s);
    }

    #[test]
    fn broken_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{not json").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }

    #[test]
    fn validation() {
        assert!(Settings::default().validate().is_ok());
        let bad_theme = Settings {
            theme: "pink".into(),
            ..Settings::default()
        };
        assert!(bad_theme.validate().is_err());
        let bad_days = Settings {
            keep_log_days: 0,
            ..Settings::default()
        };
        assert!(bad_days.validate().is_err());
    }
}
