//! Where DeskMedic keeps its settings and log.
//!
//! The exe is portable (USB stick, read-only network share), so nothing is
//! written next to it. Default is `%ProgramData%\DeskMedic` so every
//! technician on the PC shares one log; if that isn't writable (standard user
//! after an admin created it) it falls back to `%LOCALAPPDATA%\DeskMedic`.
//! `DESKMEDIC_DATA_DIR` overrides both (tests, portable use).

use std::fs;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "DeskMedic";

pub fn data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DESKMEDIC_DATA_DIR") {
        return Some(PathBuf::from(p));
    }
    ["ProgramData", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(APP_DIR))
        .find(|dir| writable(dir))
}

/// Creates `dir` if needed and proves a file can be written in it.
fn writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".probe-{}", std::process::id()));
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            // The log may exist from an admin run and be read-only for us.
            let log = dir.join(crate::log::FILE_NAME);
            !log.exists() || fs::OpenOptions::new().append(true).open(&log).is_ok()
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_creates_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a").join("DeskMedic");
        assert!(writable(&target));
        assert!(target.is_dir());
        // The probe file is cleaned up.
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }
}
