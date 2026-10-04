//! Run a fixed PowerShell script and read its JSON output.
//!
//! Scripts are compile-time constants. Values the script needs are passed as
//! `DM_ARG_*` environment variables and read with `$env:DM_ARG_X`, so user
//! input is never pasted into script text. Each call has a timeout.

use crate::run::{self, RunError};
use serde::de::DeserializeOwned;
use std::time::Duration;

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
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue';          [Console]::OutputEncoding = [Text.Encoding]::UTF8;          try {{ {script} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    );
    let env: Vec<(String, String)> = args
        .iter()
        .map(|(k, v)| (format!("DM_ARG_{k}"), v.to_string()))
        .collect();
    let out = run::run(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &wrapped,
        ],
        &env,
        timeout,
    )
    .map_err(|e| match e {
        RunError::Start(_, io) => PsError::Start(io),
        RunError::Timeout(_, secs) => PsError::Timeout(secs),
    })?;
    if !out.success() {
        return Err(PsError::Script(if out.stderr.is_empty() {
            format!("PowerShell exited with code {:?}", out.code)
        } else {
            out.stderr
        }));
    }
    Ok(out.stdout)
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
