//! Cleanup: a fixed catalog of places that hold only temporary data, a
//! preview of what each would free, and a run that deletes it.
//!
//! Every file delete goes through [`files::sweep`] and [`guard::Guard`].
//! The few things that aren't plain folders (Recycle Bin, Delivery
//! Optimization, the Windows component store) use the supported Windows
//! API or tool instead of deleting files by hand.

pub mod files;
pub mod guard;

use crate::{ps, run, sys};
use files::{Rule, Tally};
use guard::Guard;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const DAY: Duration = Duration::from_secs(24 * 3600);
const ANY_AGE: Duration = Duration::ZERO;

/// Everything a category needs to find its folders.
pub struct Ctx {
    /// Profiles to clean (the current user, or everyone when asked + admin).
    pub profiles: Vec<PathBuf>,
    pub windows: PathBuf,
    pub program_data: PathBuf,
    pub elevated: bool,
    pub running: HashSet<String>,
    pub guard: Guard,
}

impl Ctx {
    /// `all_users` only takes effect as administrator.
    pub fn detect(all_users: bool) -> Self {
        let elevated = sys::is_elevated();
        let me = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let users_dir = me.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        // Protect every profile on the PC, whichever ones get cleaned.
        let mut all = Vec::new();
        if let Some(dir) = &users_dir {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    let skip = ["public", "default", "default user", "all users"];
                    if e.path().join("AppData").join("Local").is_dir()
                        && !skip.contains(&name.as_str())
                    {
                        all.push(e.path());
                    }
                }
            }
        }
        if let Some(m) = &me {
            if !all.iter().any(|p| guard::is_within(p, m)) {
                all.push(m.clone());
            }
        }
        let profiles = if all_users && elevated {
            all.clone()
        } else {
            me.into_iter().collect()
        };
        let exe_dir: Vec<PathBuf> = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .into_iter()
            .collect();
        Ctx {
            guard: Guard::new(&all, &exe_dir),
            profiles,
            windows: std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            program_data: std::env::var_os("ProgramData")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")),
            elevated,
            running: crate::procs::running(),
        }
    }

    fn in_profiles(&self, rel: &[&str]) -> Vec<PathBuf> {
        self.profiles
            .iter()
            .map(|p| rel.iter().fold(p.clone(), |acc, part| acc.join(part)))
            .collect()
    }
}

pub enum Target {
    Folder(PathBuf, Rule),
    /// One exact file, e.g. `C:\Windows\MEMORY.DMP`.
    File(PathBuf),
}

enum Action {
    Files(fn(&Ctx) -> Vec<Target>),
    RecycleBin,
    DeliveryOptimization,
    ComponentStore,
}

pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Whole category needs admin rights.
    pub admin: bool,
    /// Ticked by default in the UI.
    pub recommended: bool,
    /// (exe name, display name): skipped while any of these run.
    closes: &'static [(&'static str, &'static str)],
    /// Windows services paused while files are removed.
    services: &'static [&'static str],
    action: Action,
}

fn folder(path: PathBuf, min_age: Duration) -> Target {
    Target::Folder(
        path,
        Rule {
            min_age,
            pattern: None,
            top_only: false,
        },
    )
}

/// Chromium keeps one folder per browser profile ("Default", "Profile 1"…).
fn chromium_caches(ctx: &Ctx, vendor: &[&str]) -> Vec<Target> {
    let mut out = Vec::new();
    for user_data in ctx.in_profiles(&[&["AppData", "Local"][..], vendor, &["User Data"]].concat())
    {
        for shared in ["ShaderCache", "GrShaderCache"] {
            out.push(folder(user_data.join(shared), ANY_AGE));
        }
        let Ok(rd) = std::fs::read_dir(&user_data) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "Default" || name.starts_with("Profile ") || name == "Guest Profile" {
                for cache in [
                    "Cache",
                    "Code Cache",
                    "GPUCache",
                    "DawnGraphiteCache",
                    "DawnWebGPUCache",
                ] {
                    out.push(folder(e.path().join(cache), ANY_AGE));
                }
            }
        }
    }
    out
}

pub fn catalog() -> Vec<Category> {
    vec![
        Category {
            id: "user-temp",
            name: "Temporary files",
            description: "Leftovers from installers and apps in the user's Temp folder, older than a day.",
            admin: false,
            recommended: true,
            closes: &[],
            services: &[],
            action: Action::Files(|c| {
                c.in_profiles(&["AppData", "Local", "Temp"])
                    .into_iter()
                    .map(|p| folder(p, DAY))
                    .collect()
            }),
        },
        Category {
            id: "windows-temp",
            name: "Windows temporary files",
            description: "Files in C:\\Windows\\Temp older than a day.",
            admin: true,
            recommended: true,
            closes: &[],
            services: &[],
            action: Action::Files(|c| vec![folder(c.windows.join("Temp"), DAY)]),
        },
        Category {
            id: "update-cache",
            name: "Windows Update downloads",
            description: "Update files Windows already downloaded. It fetches them again if it still needs them. Windows Update is paused while they're removed.",
            admin: true,
            recommended: true,
            closes: &[],
            services: &["wuauserv", "bits"],
            action: Action::Files(|c| {
                vec![folder(c.windows.join("SoftwareDistribution").join("Download"), ANY_AGE)]
            }),
        },
        Category {
            id: "delivery-optimization",
            name: "Delivery Optimization cache",
            description: "Update pieces Windows keeps to share with other PCs. Cleared with Windows' own command.",
            admin: true,
            recommended: true,
            closes: &[],
            services: &[],
            action: Action::DeliveryOptimization,
        },
        Category {
            id: "crash-dumps",
            name: "Crash dumps and error reports",
            description: "Memory dumps and Windows Error Reporting files. Only needed if someone is debugging a crash.",
            admin: false,
            recommended: true,
            closes: &[],
            services: &[],
            action: Action::Files(|c| {
                let mut t: Vec<Target> = c
                    .in_profiles(&["AppData", "Local", "CrashDumps"])
                    .into_iter()
                    .map(|p| folder(p, ANY_AGE))
                    .collect();
                for sub in ["ReportArchive", "ReportQueue"] {
                    t.extend(
                        c.in_profiles(&["AppData", "Local", "Microsoft", "Windows", "WER", sub])
                            .into_iter()
                            .map(|p| folder(p, ANY_AGE)),
                    );
                }
                if c.elevated {
                    let wer = c.program_data.join("Microsoft").join("Windows").join("WER");
                    t.push(folder(wer.join("ReportArchive"), ANY_AGE));
                    t.push(folder(wer.join("ReportQueue"), ANY_AGE));
                    t.push(folder(c.windows.join("Minidump"), ANY_AGE));
                    t.push(Target::Folder(
                        c.windows.join("LiveKernelReports"),
                        Rule {
                            min_age: ANY_AGE,
                            pattern: Some("*.dmp"),
                            top_only: false,
                        },
                    ));
                    t.push(Target::File(c.windows.join("MEMORY.DMP")));
                }
                t
            }),
        },
        Category {
            id: "thumbnails",
            name: "Thumbnail cache",
            description: "Picture previews Explorer rebuilds when needed. Files Explorer is using are skipped.",
            admin: false,
            recommended: false,
            closes: &[],
            services: &[],
            action: Action::Files(|c| {
                c.in_profiles(&["AppData", "Local", "Microsoft", "Windows", "Explorer"])
                    .into_iter()
                    .map(|p| {
                        Target::Folder(
                            p,
                            Rule {
                                min_age: ANY_AGE,
                                pattern: Some("thumbcache_*.db"),
                                top_only: true,
                            },
                        )
                    })
                    .collect()
            }),
        },
        Category {
            id: "recycle-bin",
            name: "Recycle Bin",
            description: "Deleted files waiting in the Recycle Bin of the signed-in user. Gone for good once emptied.",
            admin: false,
            recommended: false,
            closes: &[],
            services: &[],
            action: Action::RecycleBin,
        },
        Category {
            id: "chrome-cache",
            name: "Google Chrome cache",
            description: "Cached web pages and images. Logins, history and passwords are not touched.",
            admin: false,
            recommended: false,
            closes: &[("chrome.exe", "Google Chrome")],
            services: &[],
            action: Action::Files(|c| chromium_caches(c, &["Google", "Chrome"])),
        },
        Category {
            id: "edge-cache",
            name: "Microsoft Edge cache",
            description: "Cached web pages and images. Logins, history and passwords are not touched. Edge often keeps running in the background; close it from the tray first.",
            admin: false,
            recommended: false,
            closes: &[("msedge.exe", "Microsoft Edge")],
            services: &[],
            action: Action::Files(|c| chromium_caches(c, &["Microsoft", "Edge"])),
        },
        Category {
            id: "firefox-cache",
            name: "Firefox cache",
            description: "Cached web pages and images. Logins, history and passwords are not touched.",
            admin: false,
            recommended: false,
            closes: &[("firefox.exe", "Firefox")],
            services: &[],
            action: Action::Files(|c| {
                let mut out = Vec::new();
                for profiles in c.in_profiles(&["AppData", "Local", "Mozilla", "Firefox", "Profiles"]) {
                    if let Ok(rd) = std::fs::read_dir(&profiles) {
                        for e in rd.flatten() {
                            out.push(folder(e.path().join("cache2"), ANY_AGE));
                        }
                    }
                }
                out
            }),
        },
        Category {
            id: "teams-cache",
            name: "Microsoft Teams cache",
            description: "Microsoft's documented fix for Teams that won't load, sign in or show recent changes. Teams rebuilds it on the next start.",
            admin: false,
            recommended: false,
            closes: &[("ms-teams.exe", "Microsoft Teams"), ("teams.exe", "Microsoft Teams (classic)")],
            services: &[],
            action: Action::Files(|c| {
                let mut out: Vec<Target> = c
                    .in_profiles(&["AppData", "Local", "Packages", "MSTeams_8wekyb3d8bbwe", "LocalCache", "Microsoft", "MSTeams"])
                    .into_iter()
                    .map(|p| folder(p, ANY_AGE))
                    .collect();
                for classic in c.in_profiles(&["AppData", "Roaming", "Microsoft", "Teams"]) {
                    for sub in ["Cache", "Code Cache", "GPUCache", "blob_storage", "tmp"] {
                        out.push(folder(classic.join(sub), ANY_AGE));
                    }
                }
                out
            }),
        },
        Category {
            id: "outlook-temp",
            name: "Outlook temporary attachments",
            description: "Copies of attachments Outlook opened (its secure temp folder).",
            admin: false,
            recommended: false,
            closes: &[("outlook.exe", "Outlook")],
            services: &[],
            action: Action::Files(|c| {
                c.in_profiles(&["AppData", "Local", "Microsoft", "Windows", "INetCache", "Content.Outlook"])
                    .into_iter()
                    .map(|p| folder(p, ANY_AGE))
                    .collect()
            }),
        },
        Category {
            id: "component-store",
            name: "Windows component cleanup",
            description: "Removes old versions of Windows components replaced by updates (DISM). Takes several minutes; the space freed is known only afterwards.",
            admin: true,
            recommended: false,
            closes: &[],
            services: &[],
            action: Action::ComponentStore,
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct Estimate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub recommended: bool,
    /// `None` when the size can't be known in advance.
    pub bytes: Option<u64>,
    pub files: u64,
    /// Why it can't run right now.
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub id: &'static str,
    pub name: &'static str,
    pub ok: bool,
    /// Bytes removed; `None` when only free space tells (component store).
    pub freed: Option<u64>,
    pub files: u64,
    pub skipped: u64,
    pub note: String,
    pub errors: Vec<String>,
}

fn blocker(cat: &Category, ctx: &Ctx) -> Option<String> {
    if cat.admin && !ctx.elevated {
        return Some("Needs administrator".into());
    }
    cat.closes
        .iter()
        .find(|(exe, _)| ctx.running.contains(*exe))
        .map(|(_, name)| format!("Close {name} first"))
}

fn sweep_targets(targets: &[Target], ctx: &Ctx, delete: bool, cancel: &AtomicBool) -> Tally {
    let mut t = Tally::default();
    let mut seen = HashSet::new();
    for target in targets {
        match target {
            Target::Folder(root, rule) => {
                if seen.insert(root.to_string_lossy().to_lowercase()) {
                    t.add(files::sweep(root, *rule, &ctx.guard, delete, cancel));
                }
            }
            Target::File(path) => t.add(single_file(path, ctx, delete)),
        }
    }
    t
}

fn single_file(path: &Path, ctx: &Ctx, delete: bool) -> Tally {
    let mut t = Tally::default();
    let Ok(md) = std::fs::symlink_metadata(path) else {
        return t; // not there
    };
    let (reparse, readonly) = guard::attributes(&md);
    let deep_enough = path
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count()
        >= 2;
    if !md.is_file() || reparse || readonly || ctx.guard.is_protected(path) || !deep_enough {
        t.skipped += 1;
        return t;
    }
    if delete {
        if let Err(e) = std::fs::remove_file(path) {
            t.skipped += 1;
            t.errors.push(format!("{}: {e}", path.display()));
            return t;
        }
    }
    t.files = 1;
    t.bytes = md.len();
    t
}

/// What each category would free right now.
pub fn preview(ctx: &Ctx, cancel: &AtomicBool) -> Vec<Estimate> {
    catalog()
        .iter()
        .map(|cat| {
            let blocked = blocker(cat, ctx);
            let (bytes, files) = match &cat.action {
                // Without admin these folders can't be read, so any number
                // would be wrong; say "unknown" instead.
                _ if cat.admin && !ctx.elevated => (None, 0),
                Action::Files(targets) => {
                    let t = sweep_targets(&targets(ctx), ctx, false, cancel);
                    (Some(t.bytes), t.files)
                }
                Action::RecycleBin => match sys::recycle_bin_size() {
                    Ok((b, n)) => (Some(b), n),
                    Err(_) => (None, 0),
                },
                Action::DeliveryOptimization => (delivery_optimization_size(), 0),
                Action::ComponentStore => (None, 0),
            };
            Estimate {
                id: cat.id,
                name: cat.name,
                description: cat.description,
                recommended: cat.recommended,
                bytes,
                files,
                blocked,
            }
        })
        .collect()
}

/// Clean the categories in `ids`, in catalog order. Unknown ids are ignored.
pub fn run(ids: &[String], ctx: &Ctx, cancel: &AtomicBool) -> Vec<Outcome> {
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    catalog()
        .iter()
        .filter(|c| wanted.contains(c.id))
        .map(|cat| run_one(cat, ctx, cancel))
        .collect()
}

fn run_one(cat: &Category, ctx: &Ctx, cancel: &AtomicBool) -> Outcome {
    let mut out = Outcome {
        id: cat.id,
        name: cat.name,
        ok: false,
        freed: Some(0),
        files: 0,
        skipped: 0,
        note: String::new(),
        errors: Vec::new(),
    };
    if let Some(b) = blocker(cat, ctx) {
        out.note = format!("Skipped: {b}.");
        return out;
    }
    match &cat.action {
        Action::Files(targets) => {
            let stopped = match stop_services(cat.services) {
                Ok(s) => s,
                Err(e) => {
                    out.note = format!("Could not pause {}: {e}", cat.services.join(", "));
                    return out;
                }
            };
            let t = sweep_targets(&targets(ctx), ctx, true, cancel);
            if let Err(e) = start_services(&stopped) {
                out.errors
                    .push(format!("Could not restart {}: {e}", stopped.join(", ")));
            }
            out.ok = out.errors.is_empty();
            out.freed = Some(t.bytes);
            out.files = t.files;
            out.skipped = t.skipped;
            out.errors.extend(t.errors);
            if t.skipped > 0 {
                out.note = format!("{} files in use or locked were left.", t.skipped);
            }
        }
        Action::RecycleBin => {
            let before = sys::recycle_bin_size().unwrap_or((0, 0));
            if before.1 == 0 {
                out.ok = true;
                out.note = "Already empty.".into();
            } else {
                match sys::empty_recycle_bin() {
                    Ok(()) => {
                        out.ok = true;
                        out.freed = Some(before.0);
                        out.files = before.1;
                    }
                    Err(e) => out.note = e,
                }
            }
        }
        Action::DeliveryOptimization => {
            let before = delivery_optimization_size();
            match ps::run_text(
                "Delete-DeliveryOptimizationCache -Force",
                &[],
                Duration::from_secs(300),
            ) {
                Ok(_) => {
                    out.ok = true;
                    out.freed = before;
                }
                Err(e) => out.note = e.to_string(),
            }
        }
        Action::ComponentStore => {
            out.freed = None;
            match run::run(
                "dism.exe",
                &["/Online", "/Cleanup-Image", "/StartComponentCleanup"],
                &[],
                Duration::from_secs(3600),
            ) {
                Ok(o) if o.success() => {
                    out.ok = true;
                    out.note = "Done. The space freed shows in the drive's free space.".into();
                }
                Ok(o) => {
                    out.note = format!("DISM exited with code {:?}", o.code);
                    out.errors.push(last_lines(&o.stdout, 3));
                }
                Err(e) => out.note = e.to_string(),
            }
        }
    }
    out
}

fn delivery_optimization_size() -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Snap {
        #[serde(rename = "CacheSizeBytes")]
        cache: Option<u64>,
    }
    ps::run_json::<Snap>(
        "Get-DeliveryOptimizationPerfSnap -WarningAction SilentlyContinue | Select-Object CacheSizeBytes | ConvertTo-Json -Compress",
        &[],
        Duration::from_secs(60),
    )
    .ok()
    .and_then(|s| s.cache)
}

const STOP_SERVICES: &str = r#"
$was = @()
foreach ($n in ($env:DM_ARG_SERVICES -split ',')) {
  if (-not $n) { continue }
  $s = Get-Service -Name $n
  if ($s.Status -eq 'Running') { Stop-Service -Name $n -Force; $was += $n }
}
$was -join ','
"#;

const START_SERVICES: &str = r#"
foreach ($n in ($env:DM_ARG_SERVICES -split ',')) { if ($n) { Start-Service -Name $n } }
"#;

/// Stop the services that are running; returns the ones it stopped.
fn stop_services(names: &[&str]) -> Result<Vec<String>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let out = ps::run_text(
        STOP_SERVICES,
        &[("SERVICES", &names.join(","))],
        Duration::from_secs(120),
    )
    .map_err(|e| e.to_string())?;
    Ok(out
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

fn start_services(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }
    ps::run_text(
        START_SERVICES,
        &[("SERVICES", &names.join(","))],
        Duration::from_secs(120),
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_described() {
        let cats = catalog();
        let ids: HashSet<_> = cats.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), cats.len());
        assert!(cats.iter().all(|c| !c.description.is_empty()));
    }

    /// Every folder any category points at must pass the guard for a normal
    /// profile layout: no category may aim at a protected folder.
    #[test]
    fn every_target_passes_the_guard() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("Users").join("amy");
        for sub in [
            [
                "AppData",
                "Local",
                "Google",
                "Chrome",
                "User Data",
                "Default",
            ]
            .join("\\"),
            [
                "AppData",
                "Local",
                "Microsoft",
                "Edge",
                "User Data",
                "Profile 1",
            ]
            .join("\\"),
            [
                "AppData",
                "Local",
                "Mozilla",
                "Firefox",
                "Profiles",
                "x.default",
            ]
            .join("\\"),
            "Documents".into(),
        ] {
            std::fs::create_dir_all(profile.join(sub)).unwrap();
        }
        let windows = dir.path().join("Windows");
        let ctx = Ctx {
            profiles: vec![profile.clone()],
            windows,
            program_data: dir.path().join("ProgramData"),
            elevated: true,
            running: HashSet::new(),
            guard: Guard::new(&[profile], &[]),
        };
        for cat in catalog() {
            if let Action::Files(targets) = &cat.action {
                for t in targets(&ctx) {
                    if let Target::Folder(root, _) = t {
                        assert!(
                            ctx.guard.check_root(&root).is_ok(),
                            "{}: {}",
                            cat.id,
                            root.display()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn running_program_blocks_its_category() {
        let mut ctx = Ctx {
            profiles: vec![],
            windows: PathBuf::from(r"C:\Windows"),
            program_data: PathBuf::from(r"C:\ProgramData"),
            elevated: false,
            running: HashSet::from(["chrome.exe".to_string()]),
            guard: Guard::default(),
        };
        let cats = catalog();
        let chrome = cats.iter().find(|c| c.id == "chrome-cache").unwrap();
        assert_eq!(
            blocker(chrome, &ctx).as_deref(),
            Some("Close Google Chrome first")
        );
        let wu = cats.iter().find(|c| c.id == "update-cache").unwrap();
        assert_eq!(blocker(wu, &ctx).as_deref(), Some("Needs administrator"));
        ctx.elevated = true;
        assert_eq!(blocker(wu, &ctx), None);
    }

    #[test]
    fn single_file_target() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("MEMORY.DMP");
        std::fs::write(&f, vec![0u8; 64]).unwrap();
        let ctx = Ctx {
            profiles: vec![],
            windows: dir.path().into(),
            program_data: dir.path().into(),
            elevated: true,
            running: HashSet::new(),
            guard: Guard::default(),
        };
        let preview = single_file(&f, &ctx, false);
        assert_eq!((preview.files, preview.bytes), (1, 64));
        assert!(f.exists());
        let run = single_file(&f, &ctx, true);
        assert_eq!(run.files, 1);
        assert!(!f.exists());
    }
}
