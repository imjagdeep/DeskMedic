//! Remedies: one-click fixes for common helpdesk calls. Each recipe is data: fixed
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
    /// Offer a Windows restore point first: the remedy changes system
    /// settings or files in a way that is hard to undo by hand.
    pub restore_point: bool,
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

// Restarting the endpoint builder takes Windows Audio down with it.
const RESTART_AUDIO: &str = r#"
Restart-Service -Name AudioEndpointBuilder -Force
Start-Service -Name Audiosrv
"Windows Audio restarted."
"#;

const RESET_STORE: &str = r#"
$exe = Join-Path $env:SystemRoot 'System32\wsreset.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'wsreset.exe is not on this PC (no Microsoft Store).' }
Start-Process -FilePath $exe -Wait
"Store cache cleared; the Microsoft Store opens when it's done."
"#;

const ICON_CACHE: &str = r#"
$dir = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Explorer'
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
$files = @(Get-ChildItem -LiteralPath $dir -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'iconcache_*.db' -or $_.Name -like 'thumbcache_*.db' } | ForEach-Object { $_.FullName })
$old = Join-Path $env:LOCALAPPDATA 'IconCache.db'
if (Test-Path -LiteralPath $old) { $files += $old }
$files | ForEach-Object { Remove-Item -LiteralPath $_ -Force -ErrorAction SilentlyContinue }
$left = @($files | Where-Object { Test-Path -LiteralPath $_ }).Count
for ($i = 0; $i -lt 10; $i++) { Start-Sleep -Milliseconds 500; if (Get-Process -Name explorer -ErrorAction SilentlyContinue) { break } }
if (-not (Get-Process -Name explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
$msg = "Deleted $($files.Count - $left) of $($files.Count) cache file(s); Windows rebuilds them as you browse."
if ($left -gt 0) { $msg += " $left were in use and go at the next sign-in." }
$msg
"#;

// Same as Indexing Options > Advanced > Rebuild.
const REBUILD_SEARCH: &str = r#"
Stop-Service -Name WSearch -Force
Set-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Search' -Name SetupCompletedSuccessfully -Value 0 -Type DWord
Start-Service -Name WSearch
"Search index reset; Windows rebuilds it in the background. Results may be incomplete for a while."
"#;

const RESET_ONEDRIVE: &str = r#"
$me = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if ($me.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'OneDrive must not run as administrator. Open DeskMedic normally (not as administrator) and run this again.' }
$exe = @(
  (Join-Path $env:LOCALAPPDATA 'Microsoft\OneDrive\OneDrive.exe'),
  (Join-Path $env:ProgramFiles 'Microsoft OneDrive\OneDrive.exe'),
  (Join-Path ${env:ProgramFiles(x86)} 'Microsoft OneDrive\OneDrive.exe')
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $exe) { throw 'OneDrive is not installed for this user.' }
Start-Process -FilePath $exe -ArgumentList '/reset' -Wait
for ($i = 0; $i -lt 20; $i++) { Start-Sleep -Milliseconds 500; if (Get-Process -Name OneDrive -ErrorAction SilentlyContinue) { break } }
if (-not (Get-Process -Name OneDrive -ErrorAction SilentlyContinue)) { Start-Process -FilePath $exe }
"OneDrive reset and started again. It re-checks every file, so syncing can take a while; your files stay where they are."
"#;

// Windows allows one restore point per 24 hours by default and quietly skips
// extra ones, so a recent existing point counts as the way back.
const RESTORE_POINT: &str = r#"
$before = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count
Checkpoint-Computer -Description $env:DM_ARG_DESC -RestorePointType MODIFY_SETTINGS -WarningAction SilentlyContinue
$all = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue)
if ($all.Count -gt $before) {
  "Restore point created."
} else {
  $last = $all | Sort-Object SequenceNumber | Select-Object -Last 1
  $t = if ($last) { [Management.ManagementDateTimeConverter]::ToDateTime($last.CreationTime) } else { $null }
  if ($t -and ((Get-Date) - $t).TotalHours -lt 24) {
    "Windows already made a restore point at $($t.ToString('g')) (it allows one a day); that one is the way back."
  } else {
    throw 'No restore point was made. System Protection may be off for the Windows drive (Control Panel > Recovery > Configure System Restore).'
  }
}
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
            restore_point: false,
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
            restore_point: true,
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
            restore_point: false,
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
            restore_point: false,
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
            restore_point: false,
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
            restore_point: false,
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
            restore_point: true,
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
            restore_point: true,
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
            restore_point: true,
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
            restore_point: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Resync with the time server",
                cmd: Cmd::Script(RESYNC_TIME),
                timeout_secs: 120,
            }],
        },
        Recipe {
            id: "restart-audio",
            name: "Fix no sound",
            when: "No sound, a red X on the speaker icon, or \"No audio output device is installed\".",
            does: "Restarts the Windows Audio services. Sound cuts out for a second.",
            admin: true,
            restart: false,
            restore_point: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Restart Windows Audio",
                cmd: Cmd::Script(RESTART_AUDIO),
                timeout_secs: 120,
            }],
        },
        Recipe {
            id: "reset-store",
            name: "Fix the Microsoft Store",
            when: "The Store won't open, downloads are stuck, or apps won't install or update.",
            does: "Runs wsreset (Microsoft's Store reset): clears the Store cache, then opens the Store. Installed apps are not touched.",
            admin: false,
            restart: false,
            restore_point: false,
            takes: "up to a minute",
            steps: &[Step {
                label: "Reset the Store cache",
                cmd: Cmd::Script(RESET_STORE),
                timeout_secs: 300,
            }],
        },
        Recipe {
            id: "rebuild-icon-cache",
            name: "Fix blank icons and thumbnails",
            when: "Icons are blank or wrong, or pictures show the wrong preview.",
            does: "Closes Explorer, deletes this user's icon and thumbnail caches, and starts Explorer again. Open Explorer windows close.",
            admin: false,
            restart: false,
            restore_point: false,
            takes: "a few seconds",
            steps: &[Step {
                label: "Rebuild the icon and thumbnail caches",
                cmd: Cmd::Script(ICON_CACHE),
                timeout_secs: 120,
            }],
        },
        Recipe {
            id: "rebuild-search",
            name: "Fix Windows search",
            when: "Start menu or Outlook search finds nothing, or misses files you know are there.",
            does: "Resets the search index the same way as Indexing Options › Rebuild. Windows re-indexes in the background, which can take an hour or more.",
            admin: true,
            restart: false,
            restore_point: true,
            takes: "a few seconds (then re-indexing)",
            steps: &[Step {
                label: "Reset the search index",
                cmd: Cmd::Script(REBUILD_SEARCH),
                timeout_secs: 180,
            }],
        },
        Recipe {
            id: "reset-onedrive",
            name: "Fix OneDrive sync",
            when: "OneDrive is stuck on \"Processing changes\", shows sync errors, or stopped syncing.",
            does: "OneDrive /reset (Microsoft's documented fix), then starts OneDrive again. Nothing is deleted; OneDrive re-checks your files. Run it without administrator rights.",
            admin: false,
            restart: false,
            restore_point: false,
            takes: "under a minute (then re-syncing)",
            steps: &[Step {
                label: "Reset OneDrive",
                cmd: Cmd::Script(RESET_ONEDRIVE),
                timeout_secs: 180,
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
    pub restore_point: bool,
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
            restore_point: r.restore_point,
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

/// Run a recipe by id. Stops at the first failed step. With
/// `restore_point` (only for recipes that offer it) a Windows restore point
/// is made first, and nothing runs if that fails.
pub fn run_fix(id: &str, elevated: bool, restore_point: bool) -> Result<FixResult, String> {
    let r = recipes()
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("unknown remedy {id}"))?;
    if r.admin && !elevated {
        return Err(format!("{} needs administrator rights.", r.name));
    }
    let mut steps = Vec::new();
    if restore_point && r.restore_point {
        let res = ps::run_text(
            RESTORE_POINT,
            &[("DESC", &format!("DeskMedic: before {}", r.name))],
            Duration::from_secs(600),
        )
        .map_err(|e| e.to_string());
        let failed = res.is_err();
        steps.push(StepResult {
            label: "Create a restore point",
            ok: !failed,
            output: res.unwrap_or_else(|e| {
                format!("{e}
Nothing was changed. Run it again without a restore point if that is acceptable.")
            }),
        });
        if failed {
            return Ok(FixResult {
                id: r.id,
                name: r.name,
                ok: false,
                restart: false,
                steps,
            });
        }
    }
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

    /// Only admin recipes can make a restore point (Windows needs admin).
    #[test]
    fn restore_points_only_on_admin_recipes() {
        assert!(recipes().iter().filter(|r| r.restore_point).all(|r| r.admin));
        assert!(recipes().iter().any(|r| r.restore_point));
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
        let e = run_fix("sfc", false, true).unwrap_err();
        assert!(e.contains("administrator"));
        assert!(run_fix("nope", true, false).is_err());
    }

    /// Safe to run for real: only clears the DNS cache.
    #[cfg(windows)]
    #[test]
    fn flush_dns_runs() {
        let r = run_fix("flush-dns", false, true).unwrap();
        assert!(r.ok, "{:?}", r.steps);
    }
}
