//! The safety rules every cleanup delete goes through.
//!
//! - A cleanup root must be an absolute, specific folder (never a drive or a
//!   profile root), and may not be inside, or contain, a protected folder
//!   (Documents, Desktop, Downloads, Pictures, Music, Videos, OneDrive…, and
//!   the folder DeskMedic itself runs from).
//! - Every candidate path must sit lexically inside its root.
//! - Reparse points (junctions, symlinks) are never entered or deleted, so a
//!   link inside a temp folder can't lead the cleaner into user files.

use std::path::{Component, Path, PathBuf};

/// Profile sub-folders that are never touched, whatever a category says.
const PROTECTED_IN_PROFILE: &[&str] = &[
    "Documents",
    "Desktop",
    "Downloads",
    "Pictures",
    "Music",
    "Videos",
    "Favorites",
    "Contacts",
    "Saved Games",
];

#[derive(Debug, Clone, Default)]
pub struct Guard {
    protected: Vec<PathBuf>,
}

impl Guard {
    /// Protect the usual folders of each profile, any `OneDrive*` folder in
    /// it, and `extra` (e.g. the folder the exe runs from).
    pub fn new(profiles: &[PathBuf], extra: &[PathBuf]) -> Self {
        let mut protected = Vec::new();
        for p in profiles {
            for name in PROTECTED_IN_PROFILE {
                protected.push(p.join(name));
            }
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("onedrive") {
                        protected.push(e.path());
                    }
                }
            }
        }
        protected.extend(extra.iter().cloned());
        Guard { protected }
    }

    /// Why `root` can't be cleaned, or `Ok` if it can.
    pub fn check_root(&self, root: &Path) -> Result<(), String> {
        if !root.is_absolute() {
            return Err(format!("{} is not an absolute path", root.display()));
        }
        if root.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(format!("{} contains '..'", root.display()));
        }
        // Prefix + root + at least two folders: "C:\Windows\Temp", never "C:\" or "C:\Users".
        let normal = root
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        if normal < 2 {
            return Err(format!("{} is too broad to clean", root.display()));
        }
        for p in &self.protected {
            if is_within(root, p) || is_within(p, root) {
                return Err(format!(
                    "{} overlaps the protected folder {}",
                    root.display(),
                    p.display()
                ));
            }
        }
        Ok(())
    }

    pub fn is_protected(&self, path: &Path) -> bool {
        self.protected.iter().any(|p| is_within(path, p))
    }
}

/// `path` equals `base` or lies below it. Case-insensitive, by components,
/// so `C:\Temp2` is not inside `C:\Temp`.
pub fn is_within(path: &Path, base: &Path) -> bool {
    let mut p = path.components();
    for b in base.components() {
        match p.next() {
            Some(pc) if eq_component(pc, b) => {}
            _ => return false,
        }
    }
    true
}

fn eq_component(a: Component<'_>, b: Component<'_>) -> bool {
    a.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

/// `FILE_ATTRIBUTE_REPARSE_POINT` / `FILE_ATTRIBUTE_READONLY`.
#[cfg(windows)]
pub fn attributes(md: &std::fs::Metadata) -> (bool, bool) {
    use std::os::windows::fs::MetadataExt;
    let a = md.file_attributes();
    (a & 0x400 != 0, a & 0x1 != 0)
}

#[cfg(not(windows))]
pub fn attributes(md: &std::fs::Metadata) -> (bool, bool) {
    (md.file_type().is_symlink(), md.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> Guard {
        Guard::new(
            &[PathBuf::from(r"C:\Users\amy")],
            &[PathBuf::from(r"E:\Tools\DeskMedic")],
        )
    }

    #[test]
    fn normal_roots_pass() {
        let g = guard();
        assert!(g.check_root(Path::new(r"C:\Windows\Temp")).is_ok());
        assert!(g
            .check_root(Path::new(r"C:\Users\amy\AppData\Local\Temp"))
            .is_ok());
    }

    #[test]
    fn broad_or_relative_roots_fail() {
        let g = guard();
        assert!(g.check_root(Path::new(r"C:\")).is_err());
        assert!(g.check_root(Path::new(r"C:\Users")).is_err());
        assert!(g.check_root(Path::new(r"Temp\x")).is_err());
        assert!(g
            .check_root(Path::new(r"C:\Users\amy\AppData\..\Documents"))
            .is_err());
    }

    #[test]
    fn protected_overlap_fails_both_ways() {
        let g = guard();
        // Inside a protected folder.
        assert!(g
            .check_root(Path::new(r"C:\Users\amy\Documents\cache"))
            .is_err());
        // Contains a protected folder (the whole profile).
        assert!(g.check_root(Path::new(r"C:\Users\amy")).is_err());
        // Case doesn't matter.
        assert!(g.check_root(Path::new(r"c:\users\AMY\desktop")).is_err());
        // The exe's own folder.
        assert!(g.check_root(Path::new(r"E:\Tools\DeskMedic")).is_err());
    }

    #[test]
    fn within_is_by_component() {
        assert!(is_within(Path::new(r"C:\Temp\a"), Path::new(r"C:\Temp")));
        assert!(!is_within(Path::new(r"C:\Temp2\a"), Path::new(r"C:\Temp")));
        assert!(is_within(Path::new(r"C:\TEMP"), Path::new(r"c:\temp")));
    }

    #[test]
    fn onedrive_folders_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("bob");
        std::fs::create_dir_all(profile.join("OneDrive - Contoso")).unwrap();
        let g = Guard::new(&[profile.clone()], &[]);
        assert!(g.is_protected(&profile.join("OneDrive - Contoso").join("x.docx")));
        assert!(!g.is_protected(&profile.join("AppData")));
    }
}
