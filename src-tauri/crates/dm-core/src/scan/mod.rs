//! Disk scanning: a fast NTFS table read when allowed, a parallel folder walk
//! otherwise. Both feed [`tree::Tree`].

pub mod mft;
pub mod tree;
pub mod walk;

use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
pub use tree::{ScanMethod, Tree};

/// Shared counters the UI polls while a scan runs.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressSnapshot {
    pub files: u64,
    pub bytes: u64,
}

impl Progress {
    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            files: self.files.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn add(&self, bytes: u64) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("scan cancelled")]
    Cancelled,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("NTFS read failed: {0}")]
    Ntfs(String),
}

/// Scan a drive root ("C:\"). Tries the NTFS table first when `allow_mft`
/// (admin + NTFS); falls back to walking if that fails for any reason other
/// than cancel.
pub fn scan_drive(root: &Path, allow_mft: bool, progress: &Progress) -> Result<Tree, ScanError> {
    let start = Instant::now();
    if allow_mft {
        match mft::scan(root, progress) {
            Ok((entries, skipped)) => {
                return Ok(Tree::build(
                    root.to_path_buf(),
                    mft::ROOT_RECORD,
                    entries,
                    ScanMethod::Mft,
                    start.elapsed().as_millis() as u64,
                    skipped,
                ))
            }
            Err(ScanError::Cancelled) => return Err(ScanError::Cancelled),
            Err(e) => {
                tracing::warn!(
                    "fast scan of {} failed, walking instead: {e}",
                    root.display()
                );
                progress.files.store(0, Ordering::Relaxed);
                progress.bytes.store(0, Ordering::Relaxed);
            }
        }
    }
    let (entries, skipped) = walk::scan(root, progress)?;
    Ok(Tree::build(
        root.to_path_buf(),
        walk::ROOT_KEY,
        entries,
        ScanMethod::Walk,
        start.elapsed().as_millis() as u64,
        skipped,
    ))
}
