//! Assistant: what Windows' own event logs say about this PC. Reads the
//! System, Application and Defender logs for the last 30 days (read-only,
//! no admin, nothing leaves the PC), keeps only events known to hurt the PC
//! (`insights/signatures.json`), ranks them by impact, and points each at a
//! remedy guide and, where there is one, a one-click fix.
//!
//! Most errors Windows logs are harmless, so recognised harmless ones are
//! hidden on purpose, and anything unrecognised is only listed when it
//! repeats. The rules live in Rust and are tested with made-up events.

use crate::fixes;
use crate::library::{self, Os};
use crate::monitor::{Alert, Seen, Shown};
use crate::ps;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// How far back the logs are read.
pub const DAYS: i64 = 30;

/// Recognised events at or above this score are problems, between the two
/// warnings, below it minor (listed, collapsed).
const PROBLEM_SCORE: f64 = 60.0;
const WARNING_SCORE: f64 = 20.0;
/// Unrecognised errors are listed only when they repeat this often.
const UNKNOWN_MIN: u32 = 5;
const UNKNOWN_SHOWN: usize = 8;

const SCRIPT: &str = r#"
$since = (Get-Date).AddDays(-[int]$env:DM_ARG_DAYS)
$queries = $env:DM_ARG_QUERIES | ConvertFrom-Json
$events = New-Object System.Collections.Generic.List[object]
$errors = New-Object System.Collections.Generic.List[string]
foreach ($q in $queries) {
  $werr = $null
  # Some errors (an unknown provider on this edition) throw outright; note them and go on.
  try {
    $found = @(Get-WinEvent -FilterHashtable @{ LogName = $q.log; ProviderName = @($q.providers); Id = @($q.ids); StartTime = $since } -MaxEvents 2000 -ErrorAction SilentlyContinue -ErrorVariable werr)
  } catch {
    $errors.Add("$($q.log) ($($q.providers -join ', ')): $($_.Exception.Message)")
    continue
  }
  foreach ($w in @($werr)) {
    if ("$($w.FullyQualifiedErrorId)" -notlike 'NoMatching*') { $errors.Add("$($q.log): $($w.Exception.Message)") }
  }
  foreach ($e in $found) {
    $a = ''
    if ($e.Properties.Count -gt 0) { $a = [string]$e.Properties[0].Value }
    $events.Add([ordered]@{ p = $e.ProviderName; id = $e.Id; t = $e.TimeCreated.ToUniversalTime().ToString('o'); a = $a })
  }
}
$other = New-Object System.Collections.Generic.List[object]
foreach ($log in 'System', 'Application') {
  try {
    $all = @(Get-WinEvent -FilterHashtable @{ LogName = $log; Level = 1, 2; StartTime = $since } -MaxEvents 3000 -ErrorAction SilentlyContinue)
  } catch {
    $errors.Add("$($log): $($_.Exception.Message)")
    continue
  }
  foreach ($g in ($all | Group-Object ProviderName, Id)) {
    $newest = $g.Group | Sort-Object TimeCreated -Descending | Select-Object -First 1
    $msg = ''
    # The text needs the provider's message file; without it the entry just has no text.
    try { $msg = ("$($newest.Message)" -replace '\s+', ' ').Trim() } catch { $msg = '' }
    if ($msg.Length -gt 200) { $msg = $msg.Substring(0, 200) }
    $other.Add([ordered]@{ log = $log; p = $newest.ProviderName; id = $newest.Id; n = $g.Count; last = $newest.TimeCreated.ToUniversalTime().ToString('o'); msg = $msg })
  }
}
[ordered]@{ events = $events; other = $other; errors = $errors } | ConvertTo-Json -Depth 4 -Compress
"#;

// ---------- the catalogue ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl Severity {
    fn weight(self) -> f64 {
        match self {
            Severity::Critical => 100.0,
            Severity::High => 40.0,
            Severity::Medium => 15.0,
            Severity::Low => 5.0,
        }
    }
}

/// One kind of event that hurts a PC, and what to do about it.
#[derive(Debug, Clone, Deserialize)]
pub struct Signature {
    pub id: String,
    /// May contain `{name}` (the app or service) when `group` is set.
    pub title: String,
    pub what: String,
    pub impact: String,
    pub log: String,
    pub providers: Vec<String>,
    pub ids: Vec<u32>,
    pub severity: Severity,
    /// One finding per app or service (the event's first value).
    #[serde(default)]
    pub group: bool,
    /// Fewer than this is shown as minor, however recent.
    #[serde(default = "one")]
    pub min_count: u32,
    /// Remedy guide in the library.
    pub remedy: Option<String>,
    /// One-click fix; when absent, the guide's first one (if any).
    pub fix: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct Harmless {
    pub providers: Vec<String>,
    pub ids: Vec<u32>,
    pub why: String,
}

#[derive(Debug, Deserialize)]
pub struct Catalogue {
    pub signatures: Vec<Signature>,
    pub harmless: Vec<Harmless>,
}

pub fn catalogue() -> Result<&'static Catalogue, String> {
    static CELL: OnceLock<Result<Catalogue, String>> = OnceLock::new();
    CELL.get_or_init(|| {
        serde_json::from_str(include_str!("../insights/signatures.json"))
            .map_err(|e| format!("signatures.json: {e}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn matches(providers: &[String], ids: &[u32], provider: &str, id: u32) -> bool {
    ids.contains(&id) && providers.iter().any(|p| p.eq_ignore_ascii_case(provider))
}

// ---------- facts from the logs ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    /// Provider.
    pub p: String,
    pub id: u32,
    pub t: DateTime<Utc>,
    /// The event's first value: the app or service name for grouped ones.
    #[serde(default)]
    pub a: String,
}

/// Errors not looked up by a signature, counted per provider and id.
#[derive(Debug, Clone, Deserialize)]
pub struct Other {
    pub log: String,
    pub p: String,
    pub id: u32,
    pub n: u32,
    pub last: DateTime<Utc>,
    #[serde(default)]
    pub msg: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Facts {
    #[serde(default)]
    pub events: Option<Vec<Event>>,
    #[serde(default)]
    pub other: Option<Vec<Other>>,
    #[serde(default)]
    pub errors: Option<Vec<String>>,
}

// ---------- the result ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Problem,
    Warning,
    Minor,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// Signature id, plus the app or service for grouped ones.
    pub key: String,
    pub signature: String,
    pub title: String,
    pub what: String,
    pub impact: String,
    pub level: Level,
    pub severity: Severity,
    pub count: u32,
    pub this_week: u32,
    pub first: DateTime<Utc>,
    pub last: DateTime<Utc>,
    pub score: f64,
    pub remedy: Option<String>,
    pub remedy_title: Option<String>,
    pub fix: Option<String>,
    pub fix_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unknown {
    pub log: String,
    pub provider: String,
    pub id: u32,
    pub count: u32,
    pub last: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub days: i64,
    /// Problems and warnings, worst first.
    pub findings: Vec<Finding>,
    /// Recognised but small (once long ago, or below its count).
    pub minor: Vec<Finding>,
    /// Not recognised, but repeating.
    pub unknown: Vec<Unknown>,
    /// Log entries left out: known harmless, or rare and unrecognised.
    pub hidden: u32,
    pub problems: usize,
    pub warnings: usize,
    /// Logs that couldn't be read (e.g. Defender's on Windows Server).
    pub errors: Vec<String>,
}

/// Read the logs and analyse them. Takes a few seconds.
pub fn run() -> Result<Report, String> {
    let cat = catalogue()?;
    let queries: Vec<serde_json::Value> = cat
        .signatures
        .iter()
        .map(|s| serde_json::json!({ "log": s.log, "providers": s.providers, "ids": s.ids }))
        .collect();
    let queries = serde_json::to_string(&queries).map_err(|e| e.to_string())?;
    let days = DAYS.to_string();
    let facts: Facts = ps::run_json(
        SCRIPT,
        &[("QUERIES", queries.as_str()), ("DAYS", days.as_str())],
        std::time::Duration::from_secs(120),
    )
    .map_err(|e| e.to_string())?;
    analyse(&facts, Utc::now())
}

/// Impact: how bad the kind of event is, how often, how recently, and
/// whether it's happening more lately.
fn score(
    sev: Severity,
    count: u32,
    this_week: u32,
    last: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    let age = now - last;
    let recency = if age <= Duration::days(2) {
        1.0
    } else if age <= Duration::days(7) {
        0.75
    } else {
        0.4
    };
    // A quarter of 30 days is about a week: more than half this week is a rise.
    let rising = if count >= 2 && this_week * 2 > count {
        1.25
    } else {
        1.0
    };
    sev.weight() * (1.0 + f64::from(count).ln()) * recency * rising
}

pub fn analyse(f: &Facts, now: DateTime<Utc>) -> Result<Report, String> {
    let cat = catalogue()?;
    let lib = library::library(Os::Windows)?;
    let recipes = fixes::recipes();
    let week_ago = now - Duration::days(7);

    // Group the recognised events.
    struct Acc<'a> {
        sig: &'a Signature,
        name: String,
        count: u32,
        this_week: u32,
        first: DateTime<Utc>,
        last: DateTime<Utc>,
    }
    let mut groups: HashMap<String, Acc> = HashMap::new();
    for e in f.events.iter().flatten() {
        let Some(sig) = cat
            .signatures
            .iter()
            .find(|s| matches(&s.providers, &s.ids, &e.p, e.id))
        else {
            continue;
        };
        let name = if sig.group {
            e.a.trim().to_string()
        } else {
            String::new()
        };
        let key = if name.is_empty() {
            sig.id.clone()
        } else {
            format!("{}:{}", sig.id, name.to_lowercase())
        };
        let acc = groups.entry(key).or_insert(Acc {
            sig,
            name,
            count: 0,
            this_week: 0,
            first: e.t,
            last: e.t,
        });
        acc.count += 1;
        if e.t >= week_ago {
            acc.this_week += 1;
        }
        acc.first = acc.first.min(e.t);
        acc.last = acc.last.max(e.t);
    }

    let mut findings = Vec::new();
    let mut minor = Vec::new();
    for (key, a) in groups {
        let s = a.sig;
        let sc = score(s.severity, a.count, a.this_week, a.last, now);
        let level = if a.count < s.min_count || sc < WARNING_SCORE {
            Level::Minor
        } else if sc >= PROBLEM_SCORE {
            Level::Problem
        } else {
            Level::Warning
        };
        let remedy = s
            .remedy
            .as_ref()
            .and_then(|id| lib.remedies.iter().find(|r| &r.id == id));
        let fix = s
            .fix
            .clone()
            .or_else(|| remedy.and_then(|r| r.steps.iter().find_map(|st| st.fix.clone())))
            .and_then(|id| recipes.iter().find(|r| r.id == id));
        let name = if a.name.is_empty() {
            "An app"
        } else {
            a.name.as_str()
        };
        let out = Finding {
            key,
            signature: s.id.clone(),
            title: s.title.replace("{name}", name),
            what: s.what.replace("{name}", name),
            impact: s.impact.clone(),
            level,
            severity: s.severity,
            count: a.count,
            this_week: a.this_week,
            first: a.first,
            last: a.last,
            score: (sc * 10.0).round() / 10.0,
            remedy: remedy.map(|r| r.id.clone()),
            remedy_title: remedy.map(|r| r.title.clone()),
            fix: fix.map(|r| r.id.to_string()),
            fix_name: fix.map(|r| r.name.to_string()),
        };
        if level == Level::Minor {
            minor.push(out);
        } else {
            findings.push(out);
        }
    }
    let by_score = |a: &Finding, b: &Finding| b.score.total_cmp(&a.score).then(b.last.cmp(&a.last));
    findings.sort_by(by_score);
    minor.sort_by(by_score);

    // Everything else: harmless, rare, or worth listing as unrecognised.
    let mut hidden = 0;
    let mut unknown: Vec<Unknown> = Vec::new();
    for o in f.other.iter().flatten() {
        if cat
            .signatures
            .iter()
            .any(|s| matches(&s.providers, &s.ids, &o.p, o.id))
        {
            continue; // already counted above
        }
        let harmless = cat
            .harmless
            .iter()
            .any(|h| matches(&h.providers, &h.ids, &o.p, o.id));
        if harmless || o.n < UNKNOWN_MIN {
            hidden += o.n;
        } else {
            unknown.push(Unknown {
                log: o.log.clone(),
                provider: o.p.clone(),
                id: o.id,
                count: o.n,
                last: o.last,
                message: o.msg.clone(),
            });
        }
    }
    unknown.sort_by(|a, b| b.count.cmp(&a.count));
    for u in unknown.drain(UNKNOWN_SHOWN.min(unknown.len())..) {
        hidden += u.count;
    }

    Ok(Report {
        days: DAYS,
        problems: findings
            .iter()
            .filter(|x| x.level == Level::Problem)
            .count(),
        warnings: findings
            .iter()
            .filter(|x| x.level == Level::Warning)
            .count(),
        findings,
        minor,
        unknown,
        hidden,
        errors: f.errors.clone().unwrap_or_default(),
    })
}

// ---------- popups ----------

/// Key prefix in the monitor's memory, so health checks leave these alone.
const SEEN_PREFIX: &str = "event:";

/// Popups for the background check: a problem the first time it shows up,
/// and again only if it happens again after that popup. Warnings and minor
/// findings wait for the Assistant page. Problems that are gone are forgotten.
pub fn alerts(seen: &mut Seen, report: &Report, now: DateTime<Utc>) -> Vec<Alert> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    for x in report.findings.iter().filter(|x| x.level == Level::Problem) {
        let key = format!("{SEEN_PREFIX}{}", x.key);
        let due = seen.shown.get(&key).is_none_or(|s| x.last > s.at);
        if due {
            seen.shown.insert(
                key.clone(),
                Shown {
                    at: now,
                    problem: true,
                },
            );
            let mut body = x.what.clone();
            body.push(' ');
            body.push_str(&x.impact);
            out.push(Alert {
                id: key.clone(),
                title: format!("{}: needs attention", x.title),
                body,
            });
        }
        current.push(key);
    }
    seen.shown
        .retain(|k, _| !k.starts_with(SEEN_PREFIX) || current.contains(k));
    out
}

/// Plain text for a ticket.
pub fn to_text(r: &Report) -> String {
    let mut s = format!(
        "Assistant (Windows event logs, last {} days): {} problem(s), {} warning(s), {} harmless or rare entries hidden\n\n",
        r.days, r.problems, r.warnings, r.hidden
    );
    for x in r.findings.iter().chain(&r.minor) {
        s.push_str(&format!(
            "[{:?}] {}: {} time(s), {} this week, last {}\n    {}\n",
            x.level,
            x.title,
            x.count,
            x.this_week,
            x.last.format("%Y-%m-%d %H:%M UTC"),
            x.what
        ));
        if let Some(t) = &x.remedy_title {
            s.push_str(&format!("    Guide: {t}\n"));
        }
    }
    if !r.unknown.is_empty() {
        s.push_str("\nNot recognised, repeating:\n");
        for u in &r.unknown {
            s.push_str(&format!(
                "  {} {} ({}x, {}): {}\n",
                u.provider, u.id, u.count, u.log, u.message
            ));
        }
    }
    for e in &r.errors {
        s.push_str(&format!("\nCould not read: {e}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn ev(p: &str, id: u32, days_ago: i64, a: &str) -> Event {
        Event {
            p: p.into(),
            id,
            t: now() - Duration::days(days_ago) - Duration::minutes(5),
            a: a.into(),
        }
    }

    fn facts(events: Vec<Event>) -> Facts {
        Facts {
            events: Some(events),
            ..Default::default()
        }
    }

    fn find<'a>(r: &'a Report, key: &str) -> &'a Finding {
        r.findings
            .iter()
            .chain(&r.minor)
            .find(|x| x.key == key)
            .unwrap_or_else(|| panic!("{key} not in {:?}", r.findings))
    }

    /// Every remedy and fix a signature points at must exist (and run on Windows).
    #[test]
    fn catalogue_points_at_real_remedies_and_fixes() {
        let cat = catalogue().unwrap();
        let lib = library::library(Os::Windows).unwrap();
        let mut seen = std::collections::HashSet::new();
        for s in &cat.signatures {
            assert!(seen.insert(&s.id), "duplicate signature {}", s.id);
            assert!(!s.providers.is_empty() && !s.ids.is_empty(), "{}", s.id);
            assert_eq!(
                s.group,
                s.title.contains("{name}"),
                "{}: group and {{name}} go together",
                s.id
            );
            let id = s
                .remedy
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no remedy", s.id));
            assert!(
                lib.remedies.iter().any(|r| &r.id == id),
                "{}: no Windows remedy {id}",
                s.id
            );
            if let Some(f) = &s.fix {
                assert!(
                    fixes::recipes().iter().any(|r| r.id == f.as_str()),
                    "{}: no fix {f}",
                    s.id
                );
            }
        }
        // A harmless entry must never hide a signature.
        for h in &cat.harmless {
            for s in &cat.signatures {
                for p in &h.providers {
                    for id in &h.ids {
                        assert!(
                            !matches(&s.providers, &s.ids, p, *id),
                            "{p} {id} is both harmless and {}",
                            s.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn no_events_no_findings() {
        let r = analyse(&Facts::default(), now()).unwrap();
        assert!(r.findings.is_empty() && r.minor.is_empty() && r.unknown.is_empty());
        assert_eq!((r.problems, r.warnings, r.hidden), (0, 0, 0));
    }

    #[test]
    fn recent_blue_screen_is_a_problem_with_a_fix() {
        let r = analyse(
            &facts(vec![ev(
                "Microsoft-Windows-WER-SystemErrorReporting",
                1001,
                1,
                "",
            )]),
            now(),
        )
        .unwrap();
        let x = find(&r, "blue-screen");
        assert_eq!(x.level, Level::Problem);
        assert_eq!(x.remedy.as_deref(), Some("blue-screen"));
        // The guide's first one-click fix.
        assert_eq!(x.fix.as_deref(), Some("sfc"));
        assert!(x.fix_name.is_some());
        assert_eq!(r.problems, 1);
    }

    #[test]
    fn old_single_blue_screen_is_only_a_warning() {
        let r = analyse(
            &facts(vec![ev(
                "Microsoft-Windows-WER-SystemErrorReporting",
                1001,
                25,
                "",
            )]),
            now(),
        )
        .unwrap();
        assert_eq!(find(&r, "blue-screen").level, Level::Warning);
    }

    #[test]
    fn shutdowns_one_warns_three_is_a_problem() {
        let one = analyse(
            &facts(vec![ev("Microsoft-Windows-Kernel-Power", 41, 1, "")]),
            now(),
        )
        .unwrap();
        assert_eq!(find(&one, "unexpected-shutdown").level, Level::Warning);
        let three = analyse(
            &facts(
                (1..=3)
                    .map(|d| ev("Microsoft-Windows-Kernel-Power", 41, d, ""))
                    .collect(),
            ),
            now(),
        )
        .unwrap();
        let x = find(&three, "unexpected-shutdown");
        assert_eq!(x.level, Level::Problem);
        assert_eq!((x.count, x.this_week), (3, 3));
    }

    #[test]
    fn app_crashes_are_grouped_per_app_and_need_three() {
        let mut e: Vec<Event> = (0..4)
            .map(|d| ev("Application Error", 1000, d, "OUTLOOK.EXE"))
            .collect();
        e.push(ev("Application Error", 1000, 2, "notepad.exe"));
        let r = analyse(&facts(e), now()).unwrap();
        let o = find(&r, "app-crash:outlook.exe");
        assert_eq!(o.title, "OUTLOOK.EXE keeps crashing");
        assert_eq!(o.count, 4);
        assert_ne!(o.level, Level::Minor);
        // Once is not a pattern.
        assert_eq!(find(&r, "app-crash:notepad.exe").level, Level::Minor);
    }

    #[test]
    fn group_policy_offers_gpupdate() {
        let r = analyse(
            &facts(vec![
                ev("Microsoft-Windows-GroupPolicy", 1129, 0, ""),
                ev("Microsoft-Windows-GroupPolicy", 1055, 1, ""),
            ]),
            now(),
        )
        .unwrap();
        assert_eq!(find(&r, "group-policy").fix.as_deref(), Some("gpupdate"));
    }

    #[test]
    fn dns_timeouts_need_many() {
        let few = analyse(
            &facts(
                (0..5)
                    .map(|d| ev("Microsoft-Windows-DNS-Client", 1014, d, ""))
                    .collect(),
            ),
            now(),
        )
        .unwrap();
        assert_eq!(find(&few, "dns-timeouts").level, Level::Minor);
        let many = analyse(
            &facts(
                (0..40)
                    .map(|i| ev("Microsoft-Windows-DNS-Client", 1014, i % 6, ""))
                    .collect(),
            ),
            now(),
        )
        .unwrap();
        assert_ne!(find(&many, "dns-timeouts").level, Level::Minor);
    }

    #[test]
    fn worst_first() {
        let mut e = vec![ev("Microsoft-Windows-DNS-Client", 1014, 1, ""); 30];
        e.push(ev("disk", 7, 1, ""));
        let r = analyse(&facts(e), now()).unwrap();
        assert_eq!(r.findings[0].signature, "disk-errors");
    }

    #[test]
    fn harmless_and_rare_unknowns_are_hidden() {
        let other = |p: &str, id: u32, n: u32| Other {
            log: "System".into(),
            p: p.into(),
            id,
            n,
            last: now(),
            msg: "Something".into(),
        };
        let f = Facts {
            other: Some(vec![
                other("Microsoft-Windows-DistributedCOM", 10016, 400),
                other("Some-Vendor-Service", 3, 2),
                other("Some-Vendor-Service", 9, 12),
                // Already looked up by a signature: not counted twice.
                other("Microsoft-Windows-Kernel-Power", 41, 3),
            ]),
            ..Default::default()
        };
        let r = analyse(&f, now()).unwrap();
        assert_eq!(r.hidden, 402);
        assert_eq!(r.unknown.len(), 1);
        assert_eq!(
            (r.unknown[0].provider.as_str(), r.unknown[0].count),
            ("Some-Vendor-Service", 12)
        );
    }

    #[test]
    fn popups_only_for_new_problems_and_repeats() {
        let bsod = |days_ago| {
            facts(vec![ev(
                "Microsoft-Windows-WER-SystemErrorReporting",
                1001,
                days_ago,
                "",
            )])
        };
        let mut seen = Seen::default();
        let r = analyse(&bsod(1), now()).unwrap();
        assert_eq!(alerts(&mut seen, &r, now()).len(), 1);
        // Same blue screen an hour later: no new popup.
        assert!(alerts(&mut seen, &r, now() + Duration::hours(1)).is_empty());
        // It happened again after the popup: tell them.
        let again = facts(vec![
            ev("Microsoft-Windows-WER-SystemErrorReporting", 1001, 1, ""),
            Event {
                p: "Microsoft-Windows-WER-SystemErrorReporting".into(),
                id: 1001,
                t: now() + Duration::hours(2),
                a: String::new(),
            },
        ]);
        let r2 = analyse(&again, now() + Duration::hours(3)).unwrap();
        assert_eq!(alerts(&mut seen, &r2, now() + Duration::hours(3)).len(), 1);
        // Warnings never pop up.
        let mut quiet = Seen::default();
        let w = analyse(
            &facts(vec![ev("Microsoft-Windows-Kernel-Power", 41, 1, "")]),
            now(),
        )
        .unwrap();
        assert!(alerts(&mut quiet, &w, now()).is_empty());
        // Gone: forgotten, other memory kept.
        seen.shown.insert(
            "disk-space".into(),
            Shown {
                at: now(),
                problem: true,
            },
        );
        alerts(
            &mut seen,
            &analyse(&Facts::default(), now()).unwrap(),
            now(),
        );
        assert!(seen.shown.keys().all(|k| !k.starts_with(SEEN_PREFIX)));
        assert!(seen.shown.contains_key("disk-space"));
    }

    #[test]
    fn script_output_parses() {
        let json = r#"{"events":[{"p":"Microsoft-Windows-Kernel-Power","id":41,"t":"2026-10-17T08:15:02.1234567Z","a":"0"}],"other":[{"log":"System","p":"Foo","id":7,"n":6,"last":"2026-10-18T01:00:00.0000000Z","msg":"Foo failed"}],"errors":[]}"#;
        let f: Facts = serde_json::from_str(json).unwrap();
        assert_eq!(f.events.unwrap().len(), 1);
        let empty: Facts =
            serde_json::from_str(r#"{"events":null,"other":null,"errors":null}"#).unwrap();
        assert!(empty.events.is_none());
    }

    /// The real logs on this PC: must parse.
    #[cfg(windows)]
    #[test]
    fn runs_on_this_pc() {
        let r = run().unwrap();
        assert_eq!(r.days, DAYS);
    }
}
