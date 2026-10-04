//! Run an external program with a fixed argument list (never a shell
//! string), extra environment variables, no console window, and a timeout.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("could not start {0}: {1}")]
    Start(String, std::io::Error),
    #[error("{0} timed out after {1} s")]
    Timeout(String, u64),
}

pub fn run(
    program: &str,
    args: &[&str],
    env: &[(String, String)],
    timeout: Duration,
) -> Result<Output, RunError> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| RunError::Start(program.into(), e))?;
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    // Drain both pipes on threads so a full pipe can't stall the child.
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
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {}
            Err(e) => return Err(RunError::Start(program.into(), e)),
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout(program.into(), timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Output {
        code: status.code(),
        stdout: String::from_utf8_lossy(&out_t.join().unwrap_or_default())
            .trim()
            .to_string(),
        stderr: String::from_utf8_lossy(&err_t.join().unwrap_or_default())
            .trim()
            .to_string(),
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_exit_code() {
        let out = run(
            "cmd.exe",
            &["/C", "echo hello & exit 3"],
            &[],
            Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(out.stdout, "hello");
        assert_eq!(out.code, Some(3));
        assert!(!out.success());
    }

    #[test]
    fn missing_program_is_a_start_error() {
        let e = run("no-such-program-dm.exe", &[], &[], Duration::from_secs(5)).unwrap_err();
        assert!(matches!(e, RunError::Start(..)));
    }
}
