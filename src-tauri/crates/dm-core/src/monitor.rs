//! The Medic assistant's memory: which health problems were already shown in
//! a popup, so a background check only speaks up when something new goes
//! wrong. Stored as `monitor.json` in the data folder.
//!
//! Rules: a check that turns into a problem pops up; a few warnings that need
//! the user (space, virus protection, updates, crashes, drive health) pop up
//! when they appear; the same problem repeats at most once a day; anything
//! that recovers is forgotten, so it pops up again if it comes back.

use crate::cleanup::Estimate;
use crate::health::{Check, Report, Status};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const FILE_NAME: &str = "monitor.json";

/// Warnings worth interrupting someone for. The rest (restart overdue,
/// memory in use right now, startup apps) wait for the Health check page.
const ALERT_WARNINGS: &[&str] = &["disk-space", "antivirus", "updates", "crashes", "disk-health"];

const REPEAT_AFTER_HOURS: i64 = 24;

/// Key in `shown` for the temporary files reminder.
pub const TEMP_ID: &str = "temp-files";
/// Still over the limit: remind again after this long.
const TEMP_REPEAT_DAYS: i64 = 3;
/// Cleanup categories that are not "temporary files" for the reminder: the
/// Recycle Bin holds things you chose to delete, and the component store
/// can't be measured in advance.
const NOT_TEMP: &[&str] = &["recycle-bin", "component-store"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    /// check id -> when it was last shown in a popup, and at what level.
    #[serde(default)]
    pub shown: HashMap<String, Shown>,
    /// The last background result, for the tray and the home page.
    #[serde(default)]
    pub last_check: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_problems: usize,
    #[serde(default)]
    pub last_warnings: usize,
    /// Cleanable temporary files at the last background check.
    #[serde(default)]
    pub last_temp_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub at: DateTime<Utc>,
    pub problem: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub body: String,
}

fn worth_alert(c: &Check) -> Option<bool> {
    match c.status {
        Status::Problem => Some(true),
        Status::Warning if ALERT_WARNINGS.contains(&c.id) => Some(false),
        _ => None,
    }
}

/// What to pop up for this report, updating `seen`.
pub fn alerts(seen: &mut Seen, report: &Report, now: DateTime<Utc>) -> Vec<Alert> {
    let mut out = Vec::new();
    for c in &report.checks {
        let Some(problem) = worth_alert(c) else {
            // Fine now (or not alert-worthy): forget it.
            seen.shown.remove(c.id);
            continue;
        };
        let due = match seen.shown.get(c.id) {
            None => true,
            // Got worse since the last popup.
            Some(s) if problem && !s.problem => true,
            // Still a problem a day later: remind once a day.
            Some(s) => problem && now - s.at >= Duration::hours(REPEAT_AFTER_HOURS),
        };
        if due {
            seen.shown.insert(c.id.to_string(), Shown { at: now, problem });
            let mut body = c.summary.clone();
            if let Some(a) = &c.advice {
                body.push(' ');
                body.push_str(a);
            }
            out.push(Alert {
                id: c.id.to_string(),
                title: if problem {
                    format!("{}: needs attention", c.title)
                } else {
                    c.title.to_string()
                },
                body,
            });
        }
    }
    seen.last_check = Some(now);
    seen.last_problems = report.problems;
    seen.last_warnings = report.warnings;
    out
}

/// Bytes of temporary files Cleanup could clear now (sizes it can't know
/// without admin count as zero).
pub fn temp_bytes(estimates: &[Estimate]) -> u64 {
    estimates
        .iter()
        .filter(|e| !NOT_TEMP.contains(&e.id))
        .filter_map(|e| e.bytes)
        .sum()
}

/// The reminder for temporary files over `limit_gb`, updating `seen`:
/// once when it passes the limit, then every few days while it stays over.
pub fn temp_alert(seen: &mut Seen, bytes: u64, limit_gb: u32, now: DateTime<Utc>) -> Option<Alert> {
    seen.last_temp_bytes = Some(bytes);
    let limit = u64::from(limit_gb) * 1_000_000_000;
    if bytes < limit {
        seen.shown.remove(TEMP_ID);
        return None;
    }
    let due = seen
        .shown
        .get(TEMP_ID)
        .is_none_or(|s| now - s.at >= Duration::days(TEMP_REPEAT_DAYS));
    if !due {
        return None;
    }
    seen.shown.insert(
        TEMP_ID.to_string(),
        Shown {
            at: now,
            problem: false,
        },
    );
    Some(Alert {
        id: TEMP_ID.to_string(),
        title: format!("{:.1} GB of temporary files", bytes as f64 / 1e9),
        body: format!(
            "That's past your {limit_gb} GB reminder. Open DeskMedic and go to Cleanup to clear them safely; your documents are never touched."
        ),
    })
}

/// One popup or a summary when several things went wrong at once.
pub fn summarize(alerts: &[Alert]) -> Option<(String, String)> {
    match alerts {
        [] => None,
        [one] => Some((one.title.clone(), one.body.clone())),
        many => Some((
            format!("{} things need attention", many.len()),
            many.iter()
                .map(|a| a.title.trim_end_matches(": needs attention").to_string())
                .collect::<Vec<_>>()
                .join(", ")
                + ". Open DeskMedic for details.",
        )),
    }
}

impl Seen {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE_NAME))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = dir.join("monitor.json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, dir.join(FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::report;

    fn check(id: &'static str, status: Status) -> Check {
        Check {
            id,
            title: "Free space",
            status,
            summary: "3.1 GB free on C: (1%).".into(),
            advice: Some("Clean up now.".into()),
            action: None,
        }
    }

    fn t(h: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap() + Duration::hours(h)
    }

    #[test]
    fn new_problem_pops_up_once_then_daily() {
        let mut seen = Seen::default();
        let r = report(vec![check("disk-space", Status::Problem)]);
        assert_eq!(alerts(&mut seen, &r, t(0)).len(), 1);
        assert!(alerts(&mut seen, &r, t(1)).is_empty());
        assert!(alerts(&mut seen, &r, t(23)).is_empty());
        assert_eq!(alerts(&mut seen, &r, t(24)).len(), 1);
    }

    #[test]
    fn warning_once_and_worse_pops_again() {
        let mut seen = Seen::default();
        let warn = report(vec![check("disk-space", Status::Warning)]);
        assert_eq!(alerts(&mut seen, &warn, t(0)).len(), 1);
        // Warnings don't repeat.
        assert!(alerts(&mut seen, &warn, t(48)).is_empty());
        let bad = report(vec![check("disk-space", Status::Problem)]);
        assert_eq!(alerts(&mut seen, &bad, t(49)).len(), 1);
    }

    #[test]
    fn quiet_warnings_never_pop_up() {
        let mut seen = Seen::default();
        let r = report(vec![check("restart", Status::Warning), check("memory", Status::Warning)]);
        assert!(alerts(&mut seen, &r, t(0)).is_empty());
        assert_eq!(seen.last_warnings, 2);
    }

    #[test]
    fn recovered_then_back_pops_up_again() {
        let mut seen = Seen::default();
        let bad = report(vec![check("antivirus", Status::Problem)]);
        let good = report(vec![check("antivirus", Status::Good)]);
        assert_eq!(alerts(&mut seen, &bad, t(0)).len(), 1);
        assert!(alerts(&mut seen, &good, t(1)).is_empty());
        assert_eq!(alerts(&mut seen, &bad, t(2)).len(), 1);
    }

    #[test]
    fn several_at_once_become_one_popup() {
        let a = |id: &str| Alert {
            id: id.into(),
            title: format!("{id}: needs attention"),
            body: String::new(),
        };
        let (title, body) = summarize(&[a("Free space"), a("Virus protection")]).unwrap();
        assert_eq!(title, "2 things need attention");
        assert!(body.starts_with("Free space, Virus protection."));
        assert!(summarize(&[]).is_none());
    }

    fn est(id: &'static str, bytes: Option<u64>) -> Estimate {
        Estimate {
            id,
            name: id,
            description: "",
            recommended: true,
            bytes,
            files: 0,
            blocked: None,
        }
    }

    #[test]
    fn temp_bytes_skips_recycle_bin_and_unknown() {
        let e = [
            est("user-temp", Some(4_000_000_000)),
            est("chrome-cache", Some(1_500_000_000)),
            est("recycle-bin", Some(9_000_000_000)),
            est("windows-temp", None),
        ];
        assert_eq!(temp_bytes(&e), 5_500_000_000);
    }

    #[test]
    fn temp_reminder_once_then_every_three_days() {
        let mut seen = Seen::default();
        assert!(temp_alert(&mut seen, 4_900_000_000, 5, t(0)).is_none());
        let a = temp_alert(&mut seen, 7_900_000_000, 5, t(1)).unwrap();
        assert_eq!(a.title, "7.9 GB of temporary files");
        assert!(a.body.contains("5 GB"));
        assert!(temp_alert(&mut seen, 8_000_000_000, 5, t(24)).is_none());
        assert!(temp_alert(&mut seen, 8_000_000_000, 5, t(73)).is_some());
        // Cleaned up: forgotten, so it pops up again next time it passes.
        assert!(temp_alert(&mut seen, 1_000_000_000, 5, t(74)).is_none());
        assert!(temp_alert(&mut seen, 6_000_000_000, 5, t(75)).is_some());
        assert_eq!(seen.last_temp_bytes, Some(6_000_000_000));
    }

    #[test]
    fn health_checks_dont_clear_the_temp_reminder() {
        let mut seen = Seen::default();
        temp_alert(&mut seen, 9_000_000_000, 5, t(0)).unwrap();
        alerts(&mut seen, &report(vec![check("disk-space", Status::Good)]), t(1));
        assert!(seen.shown.contains_key(TEMP_ID));
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Seen::default();
        alerts(
            &mut seen,
            &report(vec![check("disk-space", Status::Problem)]),
            t(0),
        );
        seen.save(dir.path()).unwrap();
        assert_eq!(Seen::load(dir.path()), seen);
        // Missing or broken file: start fresh.
        std::fs::write(dir.path().join(FILE_NAME), "{oops").unwrap();
        assert_eq!(Seen::load(dir.path()), Seen::default());
    }
}
