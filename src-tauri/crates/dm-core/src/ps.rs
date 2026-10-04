//! Run a fixed PowerShell script and read its JSON output.
//!
//! Scripts are compile-time constants. Values the script needs are passed as
//! `DM_ARG_*` environment variables and read with `$env:DM_ARG_X`, so user
//! input is never pasted into script text. Each call has a timeout.

use serde::de::DeserializeOwned;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum PsError {
    #[error("could not start PowerShell: {0}")]
    Start(std::io::Error),
    #[error("PowerShell timed out after {0} s")]
    Timeout(u64),
    #[error("{0}")]
    Script(String),
    #[error("unexpected PowerShell output: {0}")]
    Parse(String),
}

/// Run `script` with `args` as `DM_ARG_<NAME>` env vars; return trimmed stdout.
pub fn run_text(script: &str, args: &[(&str, &str)], timeout: Duration) -> Result<String, PsError> {
    // Errors become a terminating error with a clean message on stderr.
    let wrapped = format!(
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; \
         [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         try {{ {script} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    );
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &wrapped,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    for (k, v) in args {
        cmd.env(format!("DM_ARG_{k}"), v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(PsError::Start)?;
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    // Read both pipes on threads so a full pipe can't stall the child.
    let out_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stdout.read_to_end(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stderr.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(PsError::Start)? {
            break st;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PsError::Timeout(timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let out = String::from_utf8_lossy(&out_t.join().unwrap_or_default())
        .trim()
        .to_string();
    let err = String::from_utf8_lossy(&err_t.join().unwrap_or_default())
        .trim()
        .to_string();
    if !status.success() {
        return Err(PsError::Script(if err.is_empty() {
            format!("PowerShell exited with {status}")
        } else {
            err
        }));
    }
    Ok(out)
}

/// Like [`run_text`], parsing stdout as JSON. Empty output parses as `null`.
pub fn run_json<T: DeserializeOwned>(
    script: &str,
    args: &[(&str, &str)],
    timeout: Duration,
) -> Result<T, PsError> {
    let out = run_text(script, args, timeout)?;
    let text = if out.is_empty() { "null" } else { &out };
    serde_json::from_str(text).map_err(|e| PsError::Parse(format!("{e}: {}", clip(text))))
}

fn clip(s: &str) -> &str {
    match s.char_indices().nth(200) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(60);

    #[test]
    fn json_round_trip() {
        let v: Vec<u32> = run_json("@(1,2,3) | ConvertTo-Json -Compress", &[], T).unwrap();
        assert_eq!(v, [1, 2, 3]);
    }

    /// Arguments arrive as data: quotes and `;` don't break out of the script.
    #[test]
    fn args_are_not_code() {
        let tricky = "x'; Write-Output 'pwned";
        let out = run_text("Write-Output $env:DM_ARG_NAME", &[("NAME", tricky)], T).unwrap();
        assert_eq!(out, tricky);
    }

    #[test]
    fn errors_come_back_as_messages() {
        let e = run_text("throw 'disk 9 not found'", &[], T).unwrap_err();
        assert!(e.to_string().contains("disk 9 not found"), "{e}");
    }

    #[test]
    fn timeout_kills() {
        let e = run_text("Start-Sleep -Seconds 30", &[], Duration::from_secs(2)).unwrap_err();
        assert!(matches!(e, PsError::Timeout(2)));
    }
}
