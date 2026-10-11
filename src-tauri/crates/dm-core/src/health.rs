//! Health check: one read-only look at the things that usually sit behind a
//! helpdesk call, rated good / warning / problem, each with a suggested next
//! step. Nothing here changes the PC and nothing goes online: the internet
//! result is the one Windows itself already worked out.
//!
//! A constant PowerShell script collects raw facts (each one on its own, so a
//! failing query only blanks that fact), and [`evaluate`] turns them into
//! checks. Keeping the rules in Rust lets them be tested with made-up facts.

use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const FACTS: &str = r#"
function T($b) { try { & $b } catch { $null } }
$now = Get-Date
$f = [ordered]@{}
$f.sys_drive = $env:SystemDrive
$d = T { Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$($env:SystemDrive)'" }
if ($d) { $f.sys_free = [double]$d.FreeSpace; $f.sys_total = [double]$d.Size }
$os = T { Get-CimInstance Win32_OperatingSystem }
if ($os) {
  $f.uptime_days = [math]::Round(($now - $os.LastBootUpTime).TotalDays, 1)
  $f.mem_total_kb = [double]$os.TotalVisibleMemorySize
  $f.mem_free_kb = [double]$os.FreePhysicalMemory
}
$r = @()
if (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') { $r += 'Windows components' }
if (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') { $r += 'Windows Update' }
$f.reboot_reasons = $r
$wu = T { (New-Object -ComObject Microsoft.Update.AutoUpdate).Results.LastInstallationSuccessDate }
if ($wu -and $wu.Year -gt 2000) { $f.update_days = [math]::Round(($now - $wu).TotalDays, 0) }
$av = T { ,@(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct) }
if ($null -ne $av) {
  $f.antivirus = @($av | ForEach-Object {
    $s = [int]$_.productState
    [ordered]@{ name = [string]$_.displayName; on = ((($s -shr 12) -band 0xF) -eq 1); current = ((($s -shr 4) -band 0xF) -eq 0) }
  })
}
$p = T { ,@(Get-NetConnectionProfile -ErrorAction SilentlyContinue) }
if ($null -ne $p) {
  $f.networks = $p.Count
  $f.internet = [bool]($p | Where-Object { "$($_.IPv4Connectivity)" -eq 'Internet' -or "$($_.IPv6Connectivity)" -eq 'Internet' })
}
$pd = T { ,@(Get-PhysicalDisk) }
if ($null -ne $pd) {
  $f.disks = @($pd | ForEach-Object { [ordered]@{ name = [string]$_.FriendlyName; health = [string]$_.HealthStatus } })
}
$since = $now.AddDays(-14)
$f.unexpected_shutdowns = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41; StartTime = $since } -ErrorAction SilentlyContinue).Count
$f.blue_screens = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001; StartTime = $since } -ErrorAction SilentlyContinue).Count
$ac = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000; StartTime = $now.AddDays(-7) } -ErrorAction SilentlyContinue)
$f.app_crashes = @($ac | ForEach-Object { [string]$_.Properties[0].Value } | Where-Object { $_ } | Group-Object | Sort-Object Count -Descending | Select-Object -First 3 | ForEach-Object { [ordered]@{ app = $_.Name; count = $_.Count } })
$bd = T { (Get-CimInstance -Namespace root/wmi -ClassName BatteryStaticData | Select-Object -First 1).DesignedCapacity }
$bf = T { (Get-CimInstance -Namespace root/wmi -ClassName BatteryFullChargedCapacity | Select-Object -First 1).FullChargedCapacity }
if ($bd -and $bf) { $f.battery_design = [double]$bd; $f.battery_full = [double]$bf }
$su = T { ,@(Get-CimInstance Win32_StartupCommand) }
if ($null -ne $su) { $f.startup_apps = $su.Count }
$f | ConvertTo-Json -Depth 4 -Compress
"#;

/// Raw facts from the script. Every field is optional: a query that failed
/// (no permission, missing on this edition) leaves it empty.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Facts {
    pub sys_drive: Option<String>,
    pub sys_free: Option<f64>,
    pub sys_total: Option<f64>,
    pub uptime_days: Option<f64>,
    pub mem_total_kb: Option<f64>,
    pub mem_free_kb: Option<f64>,
    #[serde(default)]
    pub reboot_reasons: Vec<String>,
    pub update_days: Option<f64>,
    pub antivirus: Option<Vec<Antivirus>>,
    pub networks: Option<u32>,
    pub internet: Option<bool>,
    pub disks: Option<Vec<PhysicalDisk>>,
    #[serde(default)]
    pub unexpected_shutdowns: u32,
    #[serde(default)]
    pub blue_screens: u32,
    #[serde(default)]
    pub app_crashes: Vec<AppCrash>,
    pub battery_design: Option<f64>,
    pub battery_full: Option<f64>,
    pub startup_apps: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Antivirus {
    pub name: String,
    pub on: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PhysicalDisk {
    pub name: String,
    pub health: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppCrash {
    pub app: String,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Good,
    Warning,
    Problem,
    /// Windows wouldn't tell us (permission, edition, query failed).
    Unknown,
}

/// What the "fix it" button next to a check does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Open the confirm dialog of a remedy (see `fixes`).
    Remedy {
        id: &'static str,
    },
    OpenCleanup,
    /// Run the network doctor for a closer look.
    OpenNetwork,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    pub status: Status,
    /// One line: what we found.
    pub summary: String,
    /// What to do about it, when there's something to do.
    pub advice: Option<String>,
    pub action: Option<Action>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub checks: Vec<Check>,
    pub problems: usize,
    pub warnings: usize,
}

/// Collect facts and rate them. Takes a few seconds.
pub fn run() -> Result<Report, String> {
    let facts: Facts =
        ps::run_json(FACTS, &[], Duration::from_secs(120)).map_err(|e| e.to_string())?;
    Ok(report(evaluate(&facts)))
}

pub fn report(checks: Vec<Check>) -> Report {
    let count = |s| checks.iter().filter(|c| c.status == s).count();
    Report {
        problems: count(Status::Problem),
        warnings: count(Status::Warning),
        checks,
    }
}

const GB: f64 = 1e9;

fn check(id: &'static str, title: &'static str, status: Status, summary: String) -> Check {
    Check {
        id,
        title,
        status,
        summary,
        advice: None,
        action: None,
    }
}

impl Check {
    fn advice(mut self, a: &str) -> Self {
        self.advice = Some(a.into());
        self
    }
    fn action(mut self, a: Action) -> Self {
        self.action = Some(a);
        self
    }
}

fn unknown(id: &'static str, title: &'static str) -> Check {
    check(
        id,
        title,
        Status::Unknown,
        "Windows didn't report this.".into(),
    )
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Turn facts into checks, in the order they're shown (most common causes of
/// a call first).
pub fn evaluate(f: &Facts) -> Vec<Check> {
    let mut out = Vec::new();

    // Free space on the Windows drive.
    out.push(match (f.sys_free, f.sys_total) {
        (Some(free), Some(total)) if total > 0.0 => {
            let pct = free / total * 100.0;
            let drive = f.sys_drive.as_deref().unwrap_or("C:");
            let summary = format!("{:.1} GB free on {drive} ({pct:.0}%).", free / GB);
            // Absolute amounts matter most; the percentage only catches
            // small drives.
            if free < 5.0 * GB || pct < 2.0 {
                check("disk-space", "Free space", Status::Problem, summary)
                    .advice("Windows needs room for updates and temporary files. Clean up now.")
                    .action(Action::OpenCleanup)
            } else if free < 15.0 * GB || pct < 5.0 {
                check("disk-space", "Free space", Status::Warning, summary)
                    .advice("Running low. A cleanup usually wins back several GB.")
                    .action(Action::OpenCleanup)
            } else {
                check("disk-space", "Free space", Status::Good, summary)
            }
        }
        _ => unknown("disk-space", "Free space"),
    });

    // Restart: pending, or simply too long ago.
    out.push(if !f.reboot_reasons.is_empty() {
        check(
            "restart",
            "Restart",
            Status::Warning,
            format!("{} is waiting for a restart.", f.reboot_reasons.join(" and ")),
        )
        .advice("Save your work and restart the PC to finish installing.")
    } else {
        match f.uptime_days {
            Some(d) if d >= 14.0 => check(
                "restart",
                "Restart",
                Status::Warning,
                format!("Not restarted in {d:.0} days."),
            )
            .advice("A restart clears many slow-PC and odd-behaviour problems. Use Restart, not Shut down."),
            Some(d) => check(
                "restart",
                "Restart",
                Status::Good,
                if d < 1.0 {
                    "Restarted today.".into()
                } else {
                    format!("Restarted {d:.0} day(s) ago.")
                },
            ),
            None => unknown("restart", "Restart"),
        }
    });

    // Windows Update.
    out.push(match f.update_days {
        Some(d) if d > 60.0 => check(
            "updates",
            "Windows Update",
            Status::Problem,
            format!("Last update installed {d:.0} days ago."),
        )
        .advice("Open Settings › Windows Update and check for updates. If they keep failing, repair Windows Update.")
        .action(Action::Remedy {
            id: "repair-windows-update",
        }),
        Some(d) if d > 35.0 => check(
            "updates",
            "Windows Update",
            Status::Warning,
            format!("Last update installed {d:.0} days ago."),
        )
        .advice("Open Settings › Windows Update and check for updates.")
        .action(Action::Remedy {
            id: "repair-windows-update",
        }),
        Some(d) => check(
            "updates",
            "Windows Update",
            Status::Good,
            format!("Last update installed {d:.0} day(s) ago."),
        ),
        None => unknown("updates", "Windows Update"),
    });

    // Antivirus.
    out.push(match &f.antivirus {
        Some(list) if !list.is_empty() => {
            let on: Vec<&Antivirus> = list.iter().filter(|a| a.on).collect();
            match on.iter().find(|a| a.current).or(on.first()) {
                None => check(
                    "antivirus",
                    "Virus protection",
                    Status::Problem,
                    "Virus protection is turned off.".into(),
                )
                .advice(
                    "Open Windows Security › Virus & threat protection and turn it on, or ask IT.",
                ),
                Some(a) if !a.current => check(
                    "antivirus",
                    "Virus protection",
                    Status::Warning,
                    format!("{} is on, but its definitions are out of date.", a.name),
                )
                .advice("Open it and update now, or ask IT."),
                Some(a) => check(
                    "antivirus",
                    "Virus protection",
                    Status::Good,
                    format!("{} is on and up to date.", a.name),
                ),
            }
        }
        Some(_) => check(
            "antivirus",
            "Virus protection",
            Status::Problem,
            "No virus protection registered with Windows.".into(),
        )
        .advice("Ask IT before using this PC for anything sensitive."),
        // Windows Server has no Security Center.
        None => unknown("antivirus", "Virus protection"),
    });

    // Internet, as Windows' own connection test sees it.
    out.push(match (f.internet, f.networks) {
        (Some(true), _) => check(
            "internet",
            "Internet",
            Status::Good,
            "Windows says this PC is online.".into(),
        ),
        (Some(false), Some(0)) | (None, Some(0)) => check(
            "internet",
            "Internet",
            Status::Problem,
            "Not connected to any network.".into(),
        )
        .advice("Check the Wi-Fi is on or the cable is plugged in."),
        (Some(false), _) => check(
            "internet",
            "Internet",
            Status::Problem,
            "Connected to a network, but Windows can't reach the internet.".into(),
        )
        .advice("Run the network doctor to see where it breaks.")
        .action(Action::OpenNetwork),
        _ => unknown("internet", "Internet").action(Action::OpenNetwork),
    });

    // Physical disk health.
    out.push(match &f.disks {
        Some(disks) if !disks.is_empty() => {
            let bad: Vec<&PhysicalDisk> = disks
                .iter()
                .filter(|d| !d.health.eq_ignore_ascii_case("healthy"))
                .collect();
            if bad.is_empty() {
                check(
                    "disk-health",
                    "Drive health",
                    Status::Good,
                    plural(
                        disks.len() as u32,
                        "drive reports healthy.",
                        "drives report healthy.",
                    ),
                )
            } else {
                let names: Vec<String> = bad
                    .iter()
                    .map(|d| format!("{} ({})", d.name, d.health))
                    .collect();
                check(
                    "disk-health",
                    "Drive health",
                    Status::Problem,
                    format!("Not healthy: {}.", names.join(", ")),
                )
                .advice("Back up important files now and have the drive checked. Disks shows more.")
            }
        }
        _ => unknown("disk-health", "Drive health"),
    });

    // Blue screens and power-loss restarts.
    let crashes = f.blue_screens + f.unexpected_shutdowns;
    let mut parts = Vec::new();
    if f.blue_screens > 0 {
        parts.push(plural(f.blue_screens, "blue screen", "blue screens"));
    }
    if f.unexpected_shutdowns > 0 {
        parts.push(plural(
            f.unexpected_shutdowns,
            "unexpected shutdown",
            "unexpected shutdowns",
        ));
    }
    out.push(if crashes == 0 {
        check(
            "crashes",
            "Crashes",
            Status::Good,
            "No blue screens or unexpected shutdowns in 14 days.".into(),
        )
    } else {
        let c = check(
            "crashes",
            "Crashes",
            if crashes >= 3 {
                Status::Problem
            } else {
                Status::Warning
            },
            format!("{} in 14 days.", parts.join(" and ")),
        );
        if f.blue_screens > 0 {
            c.advice("Check the system files; if it keeps happening, update drivers or ask IT.")
                .action(Action::Remedy { id: "sfc" })
        } else {
            c.advice("Usually a power cut, a held power button, or a laptop battery running flat. If none of those, ask IT.")
        }
    });

    // Apps that keep crashing.
    out.push(match f.app_crashes.first() {
        Some(top) if top.count >= 3 => {
            let list: Vec<String> = f
                .app_crashes
                .iter()
                .filter(|a| a.count >= 2)
                .map(|a| format!("{} ({}×)", a.app, a.count))
                .collect();
            let c = check(
                "app-crashes",
                "App crashes",
                Status::Warning,
                format!("Crashed this week: {}.", list.join(", ")),
            );
            if top.app.to_ascii_lowercase().contains("teams") {
                c.advice("Reset the Teams cache.")
                    .action(Action::Remedy { id: "reset-teams" })
            } else {
                c.advice("Update or repair that app (Settings › Apps), or reinstall it.")
            }
        }
        _ => check(
            "app-crashes",
            "App crashes",
            Status::Good,
            "No app crashed repeatedly this week.".into(),
        ),
    });

    // Memory in use right now.
    out.push(match (f.mem_total_kb, f.mem_free_kb) {
        (Some(t), Some(free)) if t > 0.0 => {
            let used = (1.0 - free / t) * 100.0;
            let summary = format!("{used:.0}% of {:.0} GB in use right now.", t * 1024.0 / GB);
            if used >= 90.0 {
                check("memory", "Memory", Status::Warning, summary)
                    .advice("Close apps and browser tabs you aren't using, or restart.")
            } else {
                check("memory", "Memory", Status::Good, summary)
            }
        }
        _ => unknown("memory", "Memory"),
    });

    // Apps that start with Windows.
    if let Some(n) = f.startup_apps {
        out.push(if n > 20 {
            check(
                "startup",
                "Startup apps",
                Status::Warning,
                format!("{n} apps start with Windows."),
            )
            .advice("Turn off the ones you don't need: Task Manager › Startup apps.")
        } else {
            check(
                "startup",
                "Startup apps",
                Status::Good,
                plural(n, "app starts with Windows.", "apps start with Windows."),
            )
        });
    }

    // Battery wear, laptops only.
    if let (Some(design), Some(full)) = (f.battery_design, f.battery_full) {
        if design > 0.0 {
            let pct = (full / design * 100.0).min(100.0);
            let summary = format!("Holds {pct:.0}% of its original charge.");
            out.push(if pct < 50.0 {
                check("battery", "Battery", Status::Problem, summary)
                    .advice("The battery is worn out; plan a replacement.")
            } else if pct < 70.0 {
                check("battery", "Battery", Status::Warning, summary)
                    .advice("The battery is wearing; expect shorter runtimes.")
            } else {
                check("battery", "Battery", Status::Good, summary)
            });
        }
    }

    out
}

/// Plain text for a ticket.
pub fn to_text(r: &Report) -> String {
    let mut s = format!(
        "Health check: {} problem(s), {} warning(s)\n\n",
        r.problems, r.warnings
    );
    for c in &r.checks {
        s.push_str(&format!("[{:?}] {}: {}\n", c.status, c.title, c.summary));
        if let Some(a) = &c.advice {
            s.push_str(&format!("    {a}\n"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Facts {
        Facts {
            sys_drive: Some("C:".into()),
            sys_free: Some(120.0 * GB),
            sys_total: Some(500.0 * GB),
            uptime_days: Some(2.0),
            mem_total_kb: Some(16.0 * 1024.0 * 1024.0),
            mem_free_kb: Some(8.0 * 1024.0 * 1024.0),
            update_days: Some(5.0),
            antivirus: Some(vec![Antivirus {
                name: "Microsoft Defender Antivirus".into(),
                on: true,
                current: true,
            }]),
            networks: Some(1),
            internet: Some(true),
            disks: Some(vec![PhysicalDisk {
                name: "NVMe SSD".into(),
                health: "Healthy".into(),
            }]),
            startup_apps: Some(8),
            ..Default::default()
        }
    }

    fn find<'a>(checks: &'a [Check], id: &str) -> &'a Check {
        checks.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn healthy_pc_is_all_good() {
        let r = report(evaluate(&healthy()));
        assert_eq!((r.problems, r.warnings), (0, 0), "{:?}", r.checks);
        assert!(r.checks.iter().all(|c| c.status == Status::Good));
        // No battery facts on a desktop: no battery check.
        assert!(r.checks.iter().all(|c| c.id != "battery"));
    }

    #[test]
    fn empty_facts_are_unknown_not_bad() {
        let r = report(evaluate(&Facts::default()));
        assert_eq!(r.problems, 0, "{:?}", r.checks);
        assert_eq!(find(&r.checks, "disk-space").status, Status::Unknown);
        assert_eq!(find(&r.checks, "updates").status, Status::Unknown);
    }

    #[test]
    fn low_disk_points_at_cleanup() {
        let mut f = healthy();
        f.sys_free = Some(3.0 * GB);
        let c = evaluate(&f);
        let d = find(&c, "disk-space");
        assert_eq!(d.status, Status::Problem);
        assert_eq!(d.action, Some(Action::OpenCleanup));
        f.sys_free = Some(12.0 * GB);
        assert_eq!(find(&evaluate(&f), "disk-space").status, Status::Warning);
    }

    #[test]
    fn pending_restart_wins_over_uptime() {
        let mut f = healthy();
        f.reboot_reasons = vec!["Windows Update".into()];
        let c = evaluate(&f);
        assert_eq!(find(&c, "restart").status, Status::Warning);
        assert!(find(&c, "restart").summary.contains("Windows Update"));
        f.reboot_reasons.clear();
        f.uptime_days = Some(30.0);
        assert!(find(&evaluate(&f), "restart").summary.contains("30 days"));
    }

    #[test]
    fn stale_updates_offer_repair() {
        let mut f = healthy();
        f.update_days = Some(90.0);
        let c = evaluate(&f);
        let u = find(&c, "updates");
        assert_eq!(u.status, Status::Problem);
        assert_eq!(
            u.action,
            Some(Action::Remedy {
                id: "repair-windows-update"
            })
        );
    }

    #[test]
    fn antivirus_states() {
        let mut f = healthy();
        f.antivirus = Some(vec![Antivirus {
            name: "X".into(),
            on: false,
            current: true,
        }]);
        assert_eq!(find(&evaluate(&f), "antivirus").status, Status::Problem);
        f.antivirus = Some(vec![Antivirus {
            name: "X".into(),
            on: true,
            current: false,
        }]);
        assert_eq!(find(&evaluate(&f), "antivirus").status, Status::Warning);
        // A second, current product counts.
        f.antivirus.as_mut().unwrap().push(Antivirus {
            name: "Y".into(),
            on: true,
            current: true,
        });
        assert_eq!(find(&evaluate(&f), "antivirus").status, Status::Good);
    }

    #[test]
    fn offline_offers_network_doctor_only_when_connected() {
        let mut f = healthy();
        f.internet = Some(false);
        let c = evaluate(&f);
        assert_eq!(find(&c, "internet").action, Some(Action::OpenNetwork));
        f.networks = Some(0);
        let c = evaluate(&f);
        assert_eq!(find(&c, "internet").status, Status::Problem);
        assert_eq!(find(&c, "internet").action, None);
    }

    #[test]
    fn crashes_and_failing_disk() {
        let mut f = healthy();
        f.blue_screens = 1;
        f.unexpected_shutdowns = 2;
        let c = evaluate(&f);
        let x = find(&c, "crashes");
        assert_eq!(x.status, Status::Problem);
        assert!(x
            .summary
            .contains("1 blue screen and 2 unexpected shutdowns"));
        assert_eq!(x.action, Some(Action::Remedy { id: "sfc" }));
        f.disks.as_mut().unwrap()[0].health = "Warning".into();
        assert_eq!(find(&evaluate(&f), "disk-health").status, Status::Problem);
    }

    #[test]
    fn teams_crashes_offer_teams_reset() {
        let mut f = healthy();
        f.app_crashes = vec![AppCrash {
            app: "ms-teams.exe".into(),
            count: 4,
        }];
        let c = evaluate(&f);
        assert_eq!(
            find(&c, "app-crashes").action,
            Some(Action::Remedy { id: "reset-teams" })
        );
    }

    #[test]
    fn battery_wear() {
        let mut f = healthy();
        f.battery_design = Some(50_000.0);
        f.battery_full = Some(20_000.0);
        assert_eq!(find(&evaluate(&f), "battery").status, Status::Problem);
    }

    /// Every remedy a check points at must exist.
    #[test]
    fn actions_point_at_real_remedies() {
        let ids: Vec<&str> = crate::fixes::recipes().iter().map(|r| r.id).collect();
        for id in [
            "repair-windows-update",
            "reset-network",
            "sfc",
            "reset-teams",
        ] {
            assert!(ids.contains(&id), "{id}");
        }
    }

    /// The real script on this PC: must parse and yield the core checks.
    #[cfg(windows)]
    #[test]
    fn runs_on_this_pc() {
        let r = run().unwrap();
        assert!(r.checks.len() >= 9, "{:?}", r.checks);
        assert_ne!(find(&r.checks, "disk-space").status, Status::Unknown);
    }
}
