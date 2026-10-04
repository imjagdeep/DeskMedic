//! Names of running programs, so cleanup can skip a browser's cache while
//! the browser is open instead of half-deleting it.

use std::collections::HashSet;

/// Lower-case exe names of every running process ("chrome.exe").
#[cfg(windows)]
pub fn running() -> HashSet<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut out = HashSet::new();
    // SAFETY: snapshot handle is closed; the entry struct has its size set.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };
        let mut e = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let end = e
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(e.szExeFile.len());
            out.insert(String::from_utf16_lossy(&e.szExeFile[..end]).to_lowercase());
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
pub fn running() -> HashSet<String> {
    HashSet::new()
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn sees_itself() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(super::running().contains(&name));
    }
}
