//! Administrator rights: check, and relaunch elevated.
//!
//! The exe is built `asInvoker`, so it starts for anyone; when the user isn't
//! elevated the app offers "Restart as administrator", which starts a new copy
//! through the UAC prompt and lets the old one close.

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();
    // SAFETY: plain Win32 calls with valid out-pointers; the token is closed.
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Start this exe again through the UAC prompt. `Err` when the user said no
/// or Windows refused; the caller keeps running in that case.
#[cfg(windows)]
pub fn relaunch_elevated() -> Result<(), String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let wide: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: both strings are NUL-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a value > 32 on success.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err("The administrator prompt was cancelled or refused.".into())
    }
}

#[cfg(not(windows))]
pub fn relaunch_elevated() -> Result<(), String> {
    Err("DeskMedic runs on Windows only.".into())
}

/// Size and item count of the current user's Recycle Bin on all drives.
#[cfg(windows)]
pub fn recycle_bin_size() -> Result<(u64, u64), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a null root means "all drives"; the struct has its size set.
    unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }
        .map_err(|e| format!("could not read the Recycle Bin: {e}"))?;
    Ok((info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

/// Empty the current user's Recycle Bin on all drives, silently.
#[cfg(windows)]
pub fn empty_recycle_bin() -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    // SAFETY: no window, null root = all drives.
    unsafe {
        SHEmptyRecycleBinW(
            None,
            PCWSTR::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }
    .map_err(|e| format!("could not empty the Recycle Bin: {e}"))
}

#[cfg(not(windows))]
pub fn recycle_bin_size() -> Result<(u64, u64), String> {
    Ok((0, 0))
}

#[cfg(not(windows))]
pub fn empty_recycle_bin() -> Result<(), String> {
    Err("DeskMedic runs on Windows only.".into())
}
