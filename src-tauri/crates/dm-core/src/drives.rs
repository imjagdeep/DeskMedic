//! Drive letters with their size, free space and file system.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    RamDisk,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drive {
    /// "C:\"
    pub root: String,
    pub label: String,
    /// "NTFS", "FAT32", "exFAT", "ReFS"…
    pub file_system: String,
    pub kind: DriveKind,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// The drive Windows runs from.
    pub is_system: bool,
}

impl Drive {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }
}

/// `GetDriveTypeW` values.
fn kind_from(code: u32) -> DriveKind {
    match code {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        5 => DriveKind::Optical,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    }
}

/// "C:\" of the Windows folder, from `SystemRoot`.
pub fn system_root() -> String {
    std::env::var("SystemRoot")
        .ok()
        .and_then(|p| p.get(..2).map(|d| format!("{}\\", d.to_uppercase())))
        .unwrap_or_else(|| "C:\\".into())
}

/// Every drive that is ready. Empty card readers and disconnected network
/// drives are left out instead of failing the list.
#[cfg(windows)]
pub fn list() -> Vec<Drive> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };

    let mut buf = [0u16; 512];
    // SAFETY: buffer is valid for its whole length.
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    let sys = system_root();
    let mut out = Vec::new();
    for root in buf[..n.min(buf.len())]
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
    {
        let root_str = String::from_utf16_lossy(root);
        let wide: Vec<u16> = root.iter().copied().chain(Some(0)).collect();
        let p = PCWSTR(wide.as_ptr());
        // SAFETY: `wide` is NUL-terminated and outlives each call below.
        let code = unsafe { GetDriveTypeW(p) };
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(p, Some(&mut avail), Some(&mut total), Some(&mut free)) }
            .is_err()
        {
            continue; // not ready
        }
        let mut name = [0u16; 261];
        let mut fs = [0u16; 261];
        let (label, file_system) = if unsafe {
            GetVolumeInformationW(p, Some(&mut name), None, None, None, Some(&mut fs))
        }
        .is_ok()
        {
            (wide_str(&name), wide_str(&fs))
        } else {
            (String::new(), String::new())
        };
        out.push(Drive {
            is_system: root_str.eq_ignore_ascii_case(&sys),
            root: root_str,
            label,
            file_system,
            kind: kind_from(code),
            total_bytes: total,
            free_bytes: free,
        });
    }
    out
}

#[cfg(windows)]
fn wide_str(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[cfg(not(windows))]
pub fn list() -> Vec<Drive> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(kind_from(3), DriveKind::Fixed);
        assert_eq!(kind_from(4), DriveKind::Network);
        assert_eq!(kind_from(99), DriveKind::Unknown);
    }

    #[cfg(windows)]
    #[test]
    fn system_drive_is_listed() {
        let drives = list();
        let sys = drives.iter().find(|d| d.is_system).expect("system drive");
        assert!(sys.total_bytes > 0);
        assert!(sys.used_bytes() <= sys.total_bytes);
        assert_eq!(sys.kind, DriveKind::Fixed);
    }
}
