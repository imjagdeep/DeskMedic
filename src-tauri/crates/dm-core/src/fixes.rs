//! One-click fixes for common helpdesk calls. Each recipe is data: fixed
//! programs with fixed argument lists, or a constant PowerShell script. No
//! user input is ever part of a command.

use crate::{cleanup, ps, run};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum Cmd {
    /// A program with fixed arguments. Exit codes in `ok` count as success.
    Program {
        program: &'static str,
        args: &'static [&'static str],
        ok: &'static [i32],
    },
    /// A constant PowerShell script.
    Script(&'static str),
    /// Clean one cleanup category for the current user (reuses its guard).
    Cleanup(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub label: &'static str,
    pub cmd: Cmd,
    pub timeout_secs: u64,
}

pub struct Recipe {
    pub id: &'static str,
    pub name: &'static str,
    /// When to use it, in a helpdesk tech's words.
    pub when: &'static str,
    /// What it does, shown before running.
    pub does: &'static str,
    pub admin: bool,
    pub restart: bool,
    /// Rough duration shown to the user.
    pub takes: &'static str,
    pub steps: &'static [Step],
}

const OK0: &[i32] = &[0];

const PRINT_QUEUE: &str = r#"
Stop-Service -Name Spooler -Force
$dir = Join-Path $env:SystemRoot 'System32\spool\PRINTERS'
$n = @(Get-ChildItem -LiteralPath $dir -File -Force -ErrorAction SilentlyContinue).Count
Get-ChildItem -LiteralPath $dir -File -Force -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
Start-Service -Name Spooler
"Removed $n stuck job file(s); the print spooler is running again."
"#;

const RESTART_EXPLORER: &str = r#"
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
for ($i = 0; $i -lt 10; $i++) { Start-Sleep -Milliseconds 500; if (Get-Process -Name explorer -ErrorAction SilentlyContinue) { break } }
if (-not (Get-Process -Name explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
"Explorer restarted."
"#;

const CLOSE_TEAMS: &str = r#"
$p = @(Get-Process -Name ms-teams, Teams -ErrorAction SilentlyContinue)
$p | Stop-Process -Force
Start-Sleep -Seconds 2
"Closed $($p.Count) Teams process(es)."
"#;

const REPAIR_WU: &str = r#"
$svcs = 'wuauserv', 'bits', 'cryptsvc', 'msiserver'
foreach ($s in $svcs) { Stop-Service -Name $s -Force -ErrorAction SilentlyContinue }
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$sd = Join-Path $env:SystemRoot 'SoftwareDistribution'
$cr = Join-Path $env:SystemRoot 'System32\catroot2'
if (Test-Path -LiteralPath $sd) { Rename-Item -LiteralPath $sd -NewName "SoftwareDistribution.old-$stamp" }
if (Test-Path -LiteralPath $cr) { Rename-Item -LiteralPath $cr -NewName "catroot2.old-$stamp" }
foreach ($s in $svcs) { Start-Service -Name $s -ErrorAction SilentlyContinue }
"Windows Update components reset (old folders kept as *.old-$stamp)."
"#;

const RESYNC_TIME: &str = r#"
$s = Get-Service -Name w32time
if ($s.StartType -eq 'Disabled') { Set-Service -Name w32time -StartupType Manual }
if ($s.Status -ne 'Running') { Start-Service -Name w32time; Start-Sleep -Seconds 2 }
$out = & w32tm.exe /resync /force 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) { throw "w32tm: $($out.Trim())" }
"Clock synced with the time server."
"#;

pub fn recipes() -> &'static [Recipe] {
    &[
        Recipe {
            id: "flush-dns",
            name: "Flush DNS cache",
            when: "A website or server moved and this PC still goes to the old address.",
            does: "ipconfig /flushdns",
            admin: false,
            restart: false,
            takes: "a second",
            steps: &[Step {
                label: "Flush the DNS resolver cache",
                cmd: Cmd::Program {
                    program: "ipconfig.exe",
                    args: &["/flushdns"],
                    ok: OK0,
                },
                timeout_secs: 60,
            }],
        },
        Recipe {
            id: "reset-network",
            name: "Reset network stack",
            when: "Connected but nothing loads, or network errors after VPN or malware cleanup.",
            does: "netsh winsock reset, netsh int ip reset. Network adapters keep their settings; a restart is needed.",
            admin: true,
            restart: true,
            takes: "a few seconds",
            steps: &[
                Step {
                    label: "Reset Winsock",
                    cmd: Cmd::Program {
                        program: "netsh.exe",
                        args: &["winsock", "reset"],
                        ok: OK0,
                    },
                    timeout_secs: 120,
                },
                Step {
                    label: "Reset TCP/IP",
                    cmd: Cmd::Program {
                        program: "netsh.exe",
                        args: &["int", "ip", "reset"],
                        ok: OK0,
                    },
                    timeout_secs: 120,
                },
            ],
        },
        Recipe {
            id: "print-queue",
            name: "Clear stuck print queue",
            when: "Print jobs stuck at \"Deleting\" or nothing prints.",
            does: "Stops the print spooler, deletes the queued job files, starts it again. Pending print jobs are lost.",
            admin: true,
            restart: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Restart the spooler with an empty queue",
                cmd: Cmd::Script(PRINT_QUEUE),
                timeout_secs: 120,
            }],
        },
        Recipe {
            id: "restart-explorer",
            name: "Restart Explorer",
            when: "Taskbar, Start menu or desktop frozen or missing icons.",
            does: "Ends explorer.exe and starts it again. Open Explorer windows close.",
            admin: false,
            restart: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Restart explorer.exe",
                cmd: Cmd::Script(RESTART_EXPLORER),
                timeout_secs: 60,
            }],
        },
        Recipe {
            id: "gpupdate",
            name: "Refresh Group Policy",
            when: "A new policy, drive mapping or printer hasn't reached this PC.",
            does: "gpupdate /force",
            admin: true,
            restart: false,
            takes: "up to a few minutes",
            steps: &[Step {
                label: "Apply computer and user policy",
                cmd: Cmd::Program {
                    program: "gpupdate.exe",
                    args: &["/force"],
                    ok: OK0,
                },
                timeout_secs: 600,
            }],
        },
        Recipe {
            id: "reset-teams",
            name: "Reset Teams cache",
            when: "Teams won't load, sign in, or shows old messages and status.",
            does: "Closes Teams, then clears its cache (Microsoft's documented fix). Teams rebuilds it on the next start; the user may need to sign in again.",
            admin: false,
            restart: false,
            takes: "a few seconds",
            steps: &[
                Step {
                    label: "Close Teams",
                    cmd: Cmd::Script(CLOSE_TEAMS),
                    timeout_secs: 60,
                },
                Step {
                    label: "Clear the Teams cache",
                    cmd: Cmd::Cleanup("teams-cache"),
                    timeout_secs: 300,
                },
            ],
        },
        Recipe {
            id: "repair-windows-update",
            name: "Repair Windows Update",
            when: "Updates fail again and again, or are stuck downloading.",
            does: "Stops the update services, renames SoftwareDistribution and catroot2 (kept as .old), starts the services. Microsoft's standard reset.",
            admin: true,
            restart: true,
            takes: "under a minute",
            steps: &[Step {
                label: "Reset Windows Update components",
                cmd: Cmd::Script(REPAIR_WU),
                timeout_secs: 300,
            }],
        },
        Recipe {
            id: "sfc",
            name: "Check system files (SFC)",
            when: "Crashes, missing Windows features, or errors about corrupt files.",
            does: "sfc /scannow: checks protected Windows files and repairs them from the local cache.",
            admin: true,
            restart: false,
            takes: "10 to 20 minutes",
            steps: &[Step {
                label: "sfc /scannow",
                cmd: Cmd::Program {
                    program: "sfc.exe",
                    args: &["/scannow"],
                    ok: OK0,
                },
                timeout_secs: 3600,
            }],
        },
        Recipe {
            id: "dism-restore",
            name: "Repair the Windows image (DISM)",
            when: "SFC says it couldn't fix some files, or updates fail with corruption errors.",
            does: "DISM /Online /Cleanup-Image /RestoreHealth: repairs the component store using Windows Update. Needs internet.",
            admin: true,
            restart: false,
            takes: "10 to 30 minutes",
            steps: &[Step {
                label: "DISM /RestoreHealth",
                cmd: Cmd::Program {
                    program: "dism.exe",
                    args: &["/Online", "/Cleanup-Image", "/RestoreHealth"],
                    ok: &[0, 3010],
                },
                timeout_secs: 5400,
            }],
        },
        Recipe {
            id: "time-sync",
            name: "Sync the clock",
            when: "Wrong time, certificate errors, or sign-in failures from clock drift.",
            does: "Starts the Windows Time service if needed, then w32tm /resync.",
            admin: true,
            restart: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Resync with the time server",
                cmd: Cmd::Script(RESYNC_TIME),
                timeout_secs: 120,
            }],
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub when: &'static str,
    pub does: &'static str,
    pub admin: bool,
    pub restart: bool,
    pub takes: &'static str,
    pub steps: Vec<&'static str>,
}

pub fn list() -> Vec<RecipeInfo> {
    recipes()
        .iter()
        .map(|r| RecipeInfo {
            id: r.id,
            name: r.name,
            when: r.when,
            does: r.does,
            admin: r.admin,
            restart: r.restart,
            takes: r.takes,
            steps: r.steps.iter().map(|s| s.label).collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub label: &'static str,
    pub ok: bool,
    /// Last few lines of output, or the error.
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixResult {
    pub id: &'static str,
    pub name: &'static str,
    pub ok: bool,
    pub restart: bool,
    pub steps: Vec<StepResult>,
}

/// Run a recipe by id. Stops at the first failed step.
pub fn run_fix(id: &str, elevated: bool) -> Result<FixResult, String> {
    let r = recipes()
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("unknown fix {id}"))?;
    if r.admin && !elevated {
        return Err(format!("{} needs administrator rights.", r.name));
    }
    let mut steps = Vec::new();
    let mut ok = true;
    for s in r.steps {
        let res = run_step(s);
        let failed = res.is_err();
        steps.push(StepResult {
            label: s.label,
            ok: !failed,
            output: res.unwrap_or_else(|e| e),
        });
        if failed {
            ok = false;
            break;
        }
    }
    Ok(FixResult {
        id: r.id,
        name: r.name,
        ok,
        restart: r.restart && ok,
        steps,
    })
}

fn run_step(s: &Step) -> Result<String, String> {
    let t = Duration::from_secs(s.timeout_secs);
    match s.cmd {
        Cmd::Program { program, args, ok } => {
            let out = run::run(program, args, &[], t).map_err(|e| e.to_string())?;
            let text = last_lines(
                if out.stdout.is_empty() {
                    &out.stderr
                } else {
                    &out.stdout
                },
                6,
            );
            if out.code.is_some_and(|c| ok.contains(&c)) {
                Ok(text)
            } else {
                Err(format!("exit code {:?}: {text}", out.code))
            }
        }
        Cmd::Script(script) => ps::run_text(script, &[], t).map_err(|e| e.to_string()),
        Cmd::Cleanup(category) => {
            let ctx = cleanup::Ctx::detect(false);
            let out = cleanup::run(
                &[category.to_string()],
                &ctx,
                &std::sync::atomic::AtomicBool::new(false),
            );
            let o = out.first().ok_or("cleanup category missing")?;
            let msg = format!(
                "Removed {} files ({:.1} MB). {}",
                o.files,
                o.freed.unwrap_or(0) as f64 / 1e6,
                o.note
            );
            if o.ok {
                Ok(msg.trim().to_string())
            } else {
                Err(msg)
            }
        }
    }
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_unique_and_every_recipe_has_steps() {
        let ids: HashSet<_> = recipes().iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), recipes().len());
        assert!(recipes().iter().all(|r| !r.steps.is_empty()));
    }

    /// Commands are fixed data: no program name or argument may look like a
    /// shell string (spaces, pipes, redirection, chaining).
    #[test]
    fn no_shell_strings_in_programs() {
        for r in recipes() {
            for s in r.steps {
                if let Cmd::Program { program, args, .. } = s.cmd {
                    assert!(program.ends_with(".exe"), "{}: {program}", r.id);
                    for a in args.iter().chain(std::iter::once(&program)) {
                        assert!(
                            !a.chars()
                                .any(|c| matches!(c, ' ' | '|' | '&' | '>' | '<' | ';')),
                            "{}: {a}",
                            r.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cleanup_steps_point_at_real_categories() {
        let cats: HashSet<_> = cleanup::catalog().iter().map(|c| c.id).collect();
        for r in recipes() {
            for s in r.steps {
                if let Cmd::Cleanup(id) = s.cmd {
                    assert!(cats.contains(id), "{}: {id}", r.id);
                }
            }
        }
    }

    #[test]
    fn admin_fix_refused_without_admin() {
        let e = run_fix("sfc", false).unwrap_err();
        assert!(e.contains("administrator"));
        assert!(run_fix("nope", true).is_err());
    }

    /// Safe to run for real: only clears the DNS cache.
    #[cfg(windows)]
    #[test]
    fn flush_dns_runs() {
        let r = run_fix("flush-dns", false).unwrap();
        assert!(r.ok, "{:?}", r.steps);
    }
}
