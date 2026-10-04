//! Folder-by-folder scan, parallel over folders. Used without admin rights,
//! on non-NTFS drives, or when the NTFS read fails.
//!
//! Symlinks and junctions are recorded but never followed, so nothing is
//! counted twice and the walk can't loop.

use super::tree::RawEntry;
use super::{Progress, ScanError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub const ROOT_KEY: u64 = 0;

pub fn scan(root: &Path, progress: &Progress) -> Result<(Vec<RawEntry>, u64), ScanError> {
    // Fail early (and clearly) if the root itself can't be read.
    fs::read_dir(root)?;
    let out = Mutex::new(Vec::new());
    let next_key = AtomicU64::new(ROOT_KEY + 1);
    let skipped = AtomicU64::new(0);
    rayon::scope(|s| {
        visit(
            s,
            root.to_path_buf(),
            ROOT_KEY,
            &out,
            &next_key,
            &skipped,
            progress,
        )
    });
    if progress.cancelled() {
        return Err(ScanError::Cancelled);
    }
    let entries = out.into_inner().unwrap_or_else(|p| p.into_inner());
    Ok((entries, skipped.into_inner()))
}

fn visit<'s>(
    scope: &rayon::Scope<'s>,
    dir: PathBuf,
    dir_key: u64,
    out: &'s Mutex<Vec<RawEntry>>,
    next_key: &'s AtomicU64,
    skipped: &'s AtomicU64,
    progress: &'s Progress,
) {
    if progress.cancelled() {
        return;
    }
    let Ok(read) = fs::read_dir(&dir) else {
        skipped.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let mut batch = Vec::new();
    let mut subdirs = Vec::new();
    for entry in read.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let key = next_key.fetch_add(1, Ordering::Relaxed);
        let name = entry.file_name().to_string_lossy().into_owned();
        // `is_symlink` is true for junctions too on Windows.
        let is_dir = ft.is_dir() && !ft.is_symlink();
        let size = if ft.is_file() {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        if is_dir {
            subdirs.push((entry.path(), key));
        } else {
            progress.add(size);
        }
        batch.push(RawEntry {
            key,
            parent: dir_key,
            name,
            size,
            is_dir,
        });
    }
    out.lock().unwrap_or_else(|p| p.into_inner()).extend(batch);
    for (path, key) in subdirs {
        scope.spawn(move |s| visit(s, path, key, out, next_key, skipped, progress));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::tree::{ScanMethod, Tree};

    #[test]
    fn walks_and_sums() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/one.bin"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("a/b/two.bin"), vec![0u8; 234]).unwrap();
        fs::write(root.join("top.txt"), b"hi").unwrap();

        let p = Progress::default();
        let (entries, skipped) = scan(root, &p).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(p.snapshot().files, 3);
        let t = Tree::build(
            root.to_path_buf(),
            ROOT_KEY,
            entries,
            ScanMethod::Walk,
            0,
            0,
        );
        assert_eq!(t.summary.bytes, 1236);
        assert_eq!(t.summary.folders, 2);
        assert_eq!(t.size_of(&root.join("a")), Some(1234));
    }

    #[test]
    fn cancel_stops_the_scan() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("x"), b"x").unwrap();
        let p = Progress::default();
        p.cancel.store(true, Ordering::Relaxed);
        assert!(matches!(scan(dir.path(), &p), Err(ScanError::Cancelled)));
    }

    /// A junction pointing back up the tree must not be followed (it would
    /// double-count and loop). `mklink /J` needs no admin rights.
    #[cfg(windows)]
    #[test]
    fn junctions_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/data.bin"), vec![0u8; 500]).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("real").join("loop"))
            .arg(root)
            .output()
            .unwrap();
        assert!(status.status.success(), "mklink failed: {status:?}");

        let p = Progress::default();
        let (entries, _) = scan(root, &p).unwrap();
        let t = Tree::build(
            root.to_path_buf(),
            ROOT_KEY,
            entries,
            ScanMethod::Walk,
            0,
            0,
        );
        assert_eq!(t.summary.bytes, 500);
        assert_eq!(
            t.summary.files, 2,
            "the junction counts as one 0-byte entry"
        );
    }
}
