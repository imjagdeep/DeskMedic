//! Audit log: append-only JSON lines, one action per line.
//!
//! Every change DeskMedic makes (cleanup, disk action, fix-it) is written
//! here with who ran it and what happened, so a technician can paste it into
//! a ticket. Same crash-safety as DeskZero's history: one flushed line per
//! entry, a torn last line is ignored, compaction writes then renames.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const FILE_NAME: &str = "log.jsonl";
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    App,
    Scan,
    Cleanup,
    Disk,
    Fix,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub ts: DateTime<Utc>,
    pub area: Area,
    /// Short name of what was done, e.g. "Empty Recycle Bin".
    pub action: String,
    pub ok: bool,
    /// One-line result, e.g. "Freed 1.2 GB, 3 files in use skipped".
    pub summary: String,
    #[serde(default)]
    pub details: Vec<String>,
    pub user: String,
    pub computer: String,
    pub elevated: bool,
}

impl LogEntry {
    pub fn new(area: Area, action: &str, ok: bool, summary: String) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self {
            id: format!(
                "{}_{}_{}",
                Utc::now().timestamp_millis(),
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ),
            ts: Utc::now(),
            area,
            action: action.into(),
            ok,
            summary,
            details: Vec::new(),
            user: env_or("USERNAME"),
            computer: env_or("COMPUTERNAME"),
            elevated: crate::sys::is_elevated(),
        }
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

fn env_or(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "?".into())
}

/// Append an entry and flush.
pub fn append(path: &Path, entry: &LogEntry) -> io::Result<()> {
    let line =
        serde_json::to_string(entry).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    f.flush()
}

/// All entries, newest first. A torn or foreign line is skipped.
pub fn read(path: &Path) -> Vec<LogEntry> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!("cannot open log {}: {e}", path.display());
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        match line {
            Ok(text) if text.trim().is_empty() => continue,
            Ok(text) => match serde_json::from_str::<LogEntry>(&text) {
                Ok(e) => out.push(e),
                Err(e) => tracing::warn!("ignoring unreadable log line: {e}"),
            },
            Err(e) => {
                tracing::warn!("log read interrupted: {e}");
                break;
            }
        }
    }
    out.reverse();
    out
}

/// Drop entries older than `keep_days`; also compacts a file over 10 MB.
pub fn compact(path: &Path, keep_days: u32) -> io::Result<()> {
    let Ok(md) = fs::metadata(path) else {
        return Ok(());
    };
    let entries = read(path);
    let cutoff = Utc::now() - chrono::Duration::days(keep_days as i64);
    let has_old = entries.last().is_some_and(|e| e.ts < cutoff);
    if md.len() <= MAX_LOG_BYTES && !has_old {
        return Ok(());
    }
    // Over the size cap: keep the newest half even if they're young.
    let mut keep: Vec<&LogEntry> = entries.iter().filter(|e| e.ts >= cutoff).collect();
    if md.len() > MAX_LOG_BYTES {
        keep.truncate(keep.len() / 2);
    }
    let tmp = path.with_extension("compact.tmp");
    {
        let mut f = File::create(&tmp)?;
        for e in keep.iter().rev() {
            let line = serde_json::to_string(e)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            writeln!(f, "{line}")?;
        }
        f.flush()?;
    }
    fs::rename(&tmp, path)
}

/// Plain-text version for pasting into a ticket.
pub fn to_text(entries: &[LogEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "{}  {}  {}  {}  [{}@{}{}]\n",
            e.ts.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M"),
            if e.ok { "OK  " } else { "FAIL" },
            e.action,
            e.summary,
            e.user,
            e.computer,
            if e.elevated { ", admin" } else { "" },
        ));
        for d in &e.details {
            out.push_str(&format!("      {d}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(action: &str, age_days: i64) -> LogEntry {
        let mut e = LogEntry::new(Area::App, action, true, "done".into());
        e.ts = Utc::now() - chrono::Duration::days(age_days);
        e
    }

    #[test]
    fn append_then_read_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(FILE_NAME);
        append(&p, &entry("one", 2)).unwrap();
        append(&p, &entry("two", 0)).unwrap();
        let got = read(&p);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].action, "two");
    }

    #[test]
    fn torn_last_line_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(FILE_NAME);
        append(&p, &entry("good", 0)).unwrap();
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{\"id\":\"half").unwrap();
        drop(f);
        let got = read(&p);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].action, "good");
    }

    #[test]
    fn compact_drops_old_and_keeps_order() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(FILE_NAME);
        append(&p, &entry("ancient", 400)).unwrap();
        append(&p, &entry("older", 3)).unwrap();
        append(&p, &entry("newest", 0)).unwrap();
        compact(&p, 180).unwrap();
        let got: Vec<String> = read(&p).into_iter().map(|e| e.action).collect();
        assert_eq!(got, ["newest", "older"]);
    }

    #[test]
    fn text_export_has_one_line_per_entry_plus_details() {
        let e = entry("Empty Recycle Bin", 0).with_details(vec!["C: 1.2 GB".into()]);
        let text = to_text(&[e]);
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("OK  "));
        assert!(text.contains("Empty Recycle Bin"));
    }
}
