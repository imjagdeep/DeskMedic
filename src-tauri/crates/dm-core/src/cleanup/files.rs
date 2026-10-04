//! Walk a cleanup root and count (preview) or delete (run) matching files.
//! The same function does both, so the preview uses exactly the rules the
//! delete will use.

use super::guard::{attributes, is_within, Guard};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Only files not modified for this long.
    pub min_age: Duration,
    /// Simple file-name pattern with one `*`, e.g. "thumbcache_*.db".
    pub pattern: Option<&'static str>,
    /// Only look at files directly in the root (no sub-folders).
    pub top_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Tally {
    pub files: u64,
    pub bytes: u64,
    /// In use, read-only, links, or no access.
    pub skipped: u64,
    /// First few errors, for the log.
    pub errors: Vec<String>,
}

impl Tally {
    pub fn add(&mut self, other: Tally) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.skipped += other.skipped;
        for e in other.errors {
            if self.errors.len() < 20 {
                self.errors.push(e);
            }
        }
    }

    fn error(&mut self, msg: String) {
        self.skipped += 1;
        if self.errors.len() < 20 {
            self.errors.push(msg);
        }
    }
}

pub fn sweep(root: &Path, rule: Rule, guard: &Guard, delete: bool, cancel: &AtomicBool) -> Tally {
    let mut t = Tally::default();
    if let Err(e) = guard.check_root(root) {
        t.error(e);
        return t;
    }
    match fs::symlink_metadata(root) {
        Ok(md) if md.is_dir() && !attributes(&md).0 => {}
        Ok(_) => {
            t.error(format!("{} is not a plain folder", root.display()));
            return t;
        }
        Err(_) => return t, // nothing there: nothing to clean
    }
    let now = SystemTime::now();
    let mut stack = vec![root.to_path_buf()];
    let mut dirs: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = fs::read_dir(&dir) else {
            t.skipped += 1;
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            // Belt and braces: never act outside the root or on a protected path.
            if !is_within(&path, root) || guard.is_protected(&path) {
                t.error(format!("refused {}", path.display()));
                continue;
            }
            let Ok(md) = fs::symlink_metadata(&path) else {
                t.skipped += 1;
                continue;
            };
            let (reparse, readonly) = attributes(&md);
            if reparse {
                t.skipped += 1; // links are left alone, never followed
                continue;
            }
            if md.is_dir() {
                if !rule.top_only {
                    dirs.push(path.clone());
                    stack.push(path);
                }
                continue;
            }
            if let Some(p) = rule.pattern {
                if !matches(p, &entry.file_name().to_string_lossy()) {
                    continue;
                }
            }
            let old_enough = md
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .is_none_or(|age| age >= rule.min_age);
            if !old_enough {
                continue;
            }
            if readonly {
                t.skipped += 1;
                continue;
            }
            if delete {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        t.files += 1;
                        t.bytes += md.len();
                    }
                    // In use (sharing violation) or no access: expected, just count.
                    Err(e) if matches!(e.raw_os_error(), Some(5 | 32 | 33)) => t.skipped += 1,
                    Err(e) => t.error(format!("{}: {e}", path.display())),
                }
            } else {
                t.files += 1;
                t.bytes += md.len();
            }
        }
    }
    if delete {
        // Remove folders that are now empty, deepest first. `remove_dir`
        // only succeeds on empty folders, and never on the root itself.
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        for d in dirs {
            let _ = fs::remove_dir(&d);
        }
    }
    t
}

/// Case-insensitive match with at most one `*`.
fn matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_ascii_lowercase(), name.to_ascii_lowercase());
    match p.split_once('*') {
        None => p == n,
        Some((pre, post)) => {
            n.len() >= pre.len() + post.len() && n.starts_with(pre) && n.ends_with(post)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANY: Rule = Rule {
        min_age: Duration::ZERO,
        pattern: None,
        top_only: false,
    };

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// root = <tmp>/profile/AppData/Local/Temp, so the guard accepts it.
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("profile");
        let root = profile.join("AppData").join("Local").join("Temp");
        fs::create_dir_all(root.join("sub").join("deeper")).unwrap();
        fs::write(root.join("a.tmp"), vec![0u8; 100]).unwrap();
        fs::write(root.join("sub").join("b.tmp"), vec![0u8; 50]).unwrap();
        fs::write(root.join("sub").join("deeper").join("c.log"), vec![0u8; 7]).unwrap();
        (dir, profile, root)
    }

    #[test]
    fn preview_counts_and_run_deletes_the_same() {
        let (_d, profile, root) = setup();
        let g = Guard::new(&[profile], &[]);
        let preview = sweep(&root, ANY, &g, false, &no_cancel());
        assert_eq!((preview.files, preview.bytes), (3, 157));
        assert!(root.join("a.tmp").exists(), "preview must not delete");

        let run = sweep(&root, ANY, &g, true, &no_cancel());
        assert_eq!((run.files, run.bytes), (3, 157));
        assert!(root.exists(), "the root itself stays");
        assert!(!root.join("sub").exists(), "emptied sub-folders go");
    }

    #[test]
    fn age_and_pattern_filters() {
        let (_d, profile, root) = setup();
        let g = Guard::new(&[profile], &[]);
        let young = Rule {
            min_age: Duration::from_secs(3600),
            ..ANY
        };
        assert_eq!(sweep(&root, young, &g, false, &no_cancel()).files, 0);
        let logs = Rule {
            pattern: Some("*.LOG"),
            ..ANY
        };
        assert_eq!(sweep(&root, logs, &g, false, &no_cancel()).files, 1);
        let top = Rule {
            top_only: true,
            ..ANY
        };
        assert_eq!(sweep(&root, top, &g, false, &no_cancel()).files, 1);
    }

    #[test]
    fn read_only_files_are_skipped() {
        let (_d, profile, root) = setup();
        let p = root.join("a.tmp");
        let mut perm = fs::metadata(&p).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&p, perm).unwrap();
        let g = Guard::new(&[profile], &[]);
        let run = sweep(&root, ANY, &g, true, &no_cancel());
        assert!(p.exists());
        assert_eq!(run.files, 2);
        assert_eq!(run.skipped, 1);
    }

    /// The key safety test: a junction in the temp folder that points at the
    /// user's Documents must not be followed, and Documents must survive.
    #[cfg(windows)]
    #[test]
    fn junction_to_documents_is_not_followed() {
        let (_d, profile, root) = setup();
        let docs = profile.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("thesis.docx"), b"precious").unwrap();
        let out = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("innocent"))
            .arg(&docs)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");

        let g = Guard::new(&[profile], &[]);
        let run = sweep(&root, ANY, &g, true, &no_cancel());
        assert!(
            docs.join("thesis.docx").exists(),
            "Documents must be untouched"
        );
        assert!(
            root.join("innocent").exists(),
            "the link itself is left alone"
        );
        assert_eq!(run.files, 3);
    }

    #[test]
    fn a_protected_root_is_refused() {
        let (_d, profile, _root) = setup();
        let docs = profile.join("Documents").join("Cache");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("keep.txt"), b"x").unwrap();
        let g = Guard::new(&[profile], &[]);
        let run = sweep(&docs, ANY, &g, true, &no_cancel());
        assert_eq!(run.files, 0);
        assert!(docs.join("keep.txt").exists());
        assert!(!run.errors.is_empty());
    }

    #[test]
    fn pattern_matching() {
        assert!(matches("thumbcache_*.db", "thumbcache_1280.db"));
        assert!(!matches("thumbcache_*.db", "iconcache_32.db"));
        assert!(matches("MEMORY.DMP", "memory.dmp"));
        assert!(!matches("a*a", "a"));
    }
}
