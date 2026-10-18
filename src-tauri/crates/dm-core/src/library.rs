//! Remedies library: guided fixes for everyday IT problems, kept as data in
//! `remedies/*.json` (one file per category) and embedded at build time.
//!
//! A remedy is a symptom ("Printer says offline"), why it happens, and steps.
//! A step is plain text, optionally with one action:
//! - `fix`: run a one-click recipe from fixes.rs (still constant commands,
//!   same confirm + restore point rules),
//! - `open`: a Settings page (`ms-settings:…`, `x-apple.systempreferences:…`),
//!   a fixed Windows tool (`run:device-manager`) or a Mac app (`app:Disk
//!   Utility`); only names in remedies/targets.json exist,
//! - `page`: one of DeskMedic's own pages.
//!
//! The app opens a step by remedy id + step number, so the page can never
//! ask for an arbitrary program or link.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/remedy_files.rs"));
const TARGETS: &str = include_str!("../remedies/targets.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Windows
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub text: String,
    /// Only on these systems (default: wherever the remedy applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<Vec<Os>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remedy {
    pub id: String,
    pub os: Vec<Os>,
    pub title: String,
    /// Words people type when they have this problem (for search).
    pub symptoms: Vec<String>,
    pub why: String,
    pub steps: Vec<Step>,
    /// When it's time to call IT, and what to tell them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalate: Option<String>,
    #[serde(default)]
    pub related: Vec<String>,
    /// Filled in from the file it's in.
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryFile {
    id: String,
    title: String,
    icon: String,
    blurb: String,
    remedies: Vec<Remedy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub blurb: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Library {
    pub categories: Vec<Category>,
    pub remedies: Vec<Remedy>,
}

#[derive(Debug, Deserialize)]
struct Targets {
    pages: Vec<String>,
    run: BTreeMap<String, Vec<String>>,
    apps: Vec<String>,
}

struct Loaded {
    categories: Vec<Category>,
    remedies: Vec<Remedy>,
    targets: Targets,
}

fn targets() -> Result<Targets, String> {
    serde_json::from_str(TARGETS).map_err(|e| format!("targets.json: {e}"))
}

/// Parse every file and check it; the first problem found is the error.
fn load() -> Result<Loaded, String> {
    let targets = targets()?;
    let fix_ids: HashSet<&str> = crate::fixes::recipes().iter().map(|r| r.id).collect();
    let mut categories = Vec::new();
    let mut remedies: Vec<Remedy> = Vec::new();
    let mut ids = HashSet::new();
    let mut cat_ids = HashSet::new();
    for (name, text) in FILES {
        let file: CategoryFile =
            serde_json::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        // A category can span several files (NN-name.json, NN-name-2.json…):
        // later files must repeat the same title and only add remedies.
        let first = cat_ids.insert(file.id.clone());
        if !first {
            let known = categories.iter().find(|c: &&Category| c.id == file.id);
            if known.is_none_or(|c| c.title != file.title) {
                return Err(format!("{name}: category {} has a different title in another file", file.id));
            }
        }
        for mut r in file.remedies {
            let at = format!("{name} › {}", r.id);
            if !valid_id(&r.id) {
                return Err(format!("{at}: id must be lower-case words with dashes"));
            }
            if !ids.insert(r.id.clone()) {
                return Err(format!("{at}: id used twice"));
            }
            if r.os.is_empty() || r.title.trim().is_empty() || r.why.trim().is_empty() {
                return Err(format!("{at}: os, title and why are required"));
            }
            if r.symptoms.is_empty() || r.steps.is_empty() {
                return Err(format!("{at}: needs symptoms and steps"));
            }
            for (i, s) in r.steps.iter().enumerate() {
                check_step(s, &r.os, &targets, &fix_ids).map_err(|e| format!("{at} step {}: {e}", i + 1))?;
            }
            r.category = file.id.clone();
            remedies.push(r);
        }
        if first {
            categories.push(Category {
                count: 0,
                id: file.id,
                title: file.title,
                icon: file.icon,
                blurb: file.blurb,
            });
        }
    }
    for r in &remedies {
        for rel in &r.related {
            if !ids.contains(rel) || rel == &r.id {
                return Err(format!("{}: related {rel} doesn't exist", r.id));
            }
        }
    }
    Ok(Loaded {
        categories,
        remedies,
        targets,
    })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check_step(
    s: &Step,
    remedy_os: &[Os],
    targets: &Targets,
    fix_ids: &HashSet<&str>,
) -> Result<(), String> {
    if s.text.trim().is_empty() {
        return Err("empty text".into());
    }
    let actions = [s.fix.is_some(), s.open.is_some(), s.page.is_some()];
    if actions.iter().filter(|a| **a).count() > 1 {
        return Err("one action per step".into());
    }
    let os = s.os.as_deref().unwrap_or(remedy_os);
    if let Some(step_os) = &s.os {
        if step_os.iter().any(|o| !remedy_os.contains(o)) {
            return Err("step os outside the remedy's".into());
        }
    }
    if let Some(f) = &s.fix {
        if !fix_ids.contains(f.as_str()) {
            return Err(format!("unknown fix {f}"));
        }
        if os != [Os::Windows] {
            return Err("fixes are Windows-only: set the step's os".into());
        }
    }
    if let Some(p) = &s.page {
        if !targets.pages.contains(p) {
            return Err(format!("unknown page {p}"));
        }
    }
    if let Some(o) = &s.open {
        let target_os = open_os(o, targets).ok_or_else(|| format!("unknown open target {o}"))?;
        if os != [target_os] {
            return Err(format!("{o} only exists on {target_os:?}: set the step's os"));
        }
    }
    Ok(())
}

/// Which system an open target belongs to, if it's an allowed one.
fn open_os(target: &str, targets: &Targets) -> Option<Os> {
    if let Some(rest) = target.strip_prefix("ms-settings:") {
        return safe_uri_part(rest).then_some(Os::Windows);
    }
    if let Some(rest) = target.strip_prefix("x-apple.systempreferences:") {
        return (!rest.is_empty() && safe_uri_part(rest)).then_some(Os::Macos);
    }
    if let Some(name) = target.strip_prefix("run:") {
        return targets.run.contains_key(name).then_some(Os::Windows);
    }
    if let Some(name) = target.strip_prefix("app:") {
        return targets.apps.iter().any(|a| a == name).then_some(Os::Macos);
    }
    None
}

fn safe_uri_part(s: &str) -> bool {
    s.len() <= 120
        && s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._?=&_".contains(&b))
}

fn loaded() -> Result<&'static Loaded, String> {
    static CELL: OnceLock<Result<Loaded, String>> = OnceLock::new();
    CELL.get_or_init(load).as_ref().map_err(Clone::clone)
}

/// The library for one system: its remedies, steps for other systems left
/// out, categories with their counts (empty ones dropped).
pub fn library(os: Os) -> Result<Library, String> {
    let l = loaded()?;
    let here: HashSet<&str> = l
        .remedies
        .iter()
        .filter(|x| x.os.contains(&os))
        .map(|x| x.id.as_str())
        .collect();
    let remedies: Vec<Remedy> = l
        .remedies
        .iter()
        .filter(|r| r.os.contains(&os))
        .map(|r| {
            let mut r = r.clone();
            r.steps.retain(|s| s.os.as_ref().is_none_or(|o| o.contains(&os)));
            r.related.retain(|x| here.contains(x.as_str()));
            r
        })
        .collect();
    let categories = l
        .categories
        .iter()
        .map(|c| Category {
            count: remedies.iter().filter(|r| r.category == c.id).count(),
            ..c.clone()
        })
        .filter(|c| c.count > 0)
        .collect();
    Ok(Library {
        categories,
        remedies,
    })
}

/// What opening a step does, for the app to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    /// A Settings URI handed to the system (explorer.exe / open).
    Uri(String),
    /// A fixed program and arguments from targets.json.
    Program(String, Vec<String>),
    /// A Mac app by name (open -a).
    App(String),
}

/// The open action of `remedy`'s `step` (0-based, as the library for this
/// system lists them).
pub fn open_target(remedy: &str, step: usize, os: Os) -> Result<Open, String> {
    let l = loaded()?;
    let lib = library(os)?;
    let r = lib
        .remedies
        .iter()
        .find(|r| r.id == remedy)
        .ok_or("no such remedy")?;
    let target = r
        .steps
        .get(step)
        .and_then(|s| s.open.clone())
        .ok_or("that step doesn't open anything")?;
    if open_os(&target, &l.targets) != Some(os) {
        return Err("not available on this computer".into());
    }
    Ok(if let Some(name) = target.strip_prefix("run:") {
        let cmd = &l.targets.run[name];
        Open::Program(cmd[0].clone(), cmd[1..].to_vec())
    } else if let Some(name) = target.strip_prefix("app:") {
        Open::App(name.to_string())
    } else {
        Open::Uri(target)
    })
}

/// Carry out an open: Windows through the shell (handles Settings links and
/// tools that ask for elevation), macOS through `open`.
pub fn open(target: &Open) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (file, args) = match target {
            Open::Uri(u) => (u.clone(), String::new()),
            Open::Program(p, a) => (p.clone(), a.join(" ")),
            Open::App(_) => return Err("not available on Windows".into()),
        };
        shell_execute(&file, &args)
    }
    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new("open");
        match target {
            Open::Uri(u) => cmd.arg(u),
            Open::App(name) => cmd.args(["-a", name]),
            Open::Program(..) => return Err("not available on this computer".into()),
        };
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("could not open it: {e}"))
    }
}

#[cfg(windows)]
fn shell_execute(file: &str, args: &str) -> Result<(), String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let file_w = wide(file);
    let args_w = wide(args);
    // SAFETY: both buffers are NUL-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file_w.as_ptr()),
            if args.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(args_w.as_ptr())
            },
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a value > 32 on success.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!("Windows couldn't open it (code {})", result.0 as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_loads_and_checks() {
        let l = load().unwrap_or_else(|e| panic!("{e}"));
        assert!(!l.remedies.is_empty());
        for os in [Os::Windows, Os::Macos] {
            let lib = library(os).unwrap();
            for r in &lib.remedies {
                assert!(!r.steps.is_empty(), "{} has no steps on {os:?}", r.id);
            }
        }
    }

    #[test]
    fn targets_are_checked() {
        let t = targets().unwrap();
        assert_eq!(open_os("ms-settings:printers", &t), Some(Os::Windows));
        assert_eq!(open_os("ms-settings:", &t), Some(Os::Windows));
        assert_eq!(open_os("ms-settings:a b", &t), None);
        assert_eq!(open_os("ms-settings:x\"&calc", &t), None);
        assert_eq!(
            open_os("x-apple.systempreferences:com.apple.Network-Settings.extension", &t),
            Some(Os::Macos)
        );
        assert_eq!(open_os("run:device-manager", &t), Some(Os::Windows));
        assert_eq!(open_os("run:cmd", &t), None);
        assert_eq!(open_os("app:Disk Utility", &t), Some(Os::Macos));
        assert_eq!(open_os("app:Terminal", &t), None);
        assert_eq!(open_os("https://example.com", &t), None);
        assert_eq!(open_os("file:///C:/Windows", &t), None);
    }

    #[test]
    fn ids_are_simple() {
        assert!(valid_id("printer-offline"));
        assert!(!valid_id("Printer Offline"));
        assert!(!valid_id(""));
    }

    #[test]
    fn open_goes_through_the_library() {
        let lib = library(Os::Windows).unwrap();
        let (r, i) = lib
            .remedies
            .iter()
            .find_map(|r| r.steps.iter().position(|s| s.open.is_some()).map(|i| (r, i)))
            .expect("a Windows remedy that opens something");
        assert!(open_target(&r.id, i, Os::Windows).is_ok());
        assert!(open_target(&r.id, 999, Os::Windows).is_err());
        assert!(open_target("no-such-remedy", 0, Os::Windows).is_err());
    }
}
