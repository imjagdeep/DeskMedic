//! Turn a disk layout into a list of problems a technician can act on, each
//! in plain English with the likely cause and, where safe, a fix.

use super::actions::Action;
use super::model::{Layout, PartKind};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Problem,
    Warning,
    Info,
}

/// A fix the UI can offer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fix {
    /// A disk action (needs admin + typed confirmation).
    Action { action: Action, label: String },
    /// Open DeskMedic's Cleanup page.
    OpenCleanup,
    /// The guided "make room to extend" flow for a volume (milestone 5).
    ExtendGuide { letter: char },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub disk: Option<u32>,
    pub fix: Option<Fix>,
}

fn offline_reason(r: &str) -> &'static str {
    match r.to_ascii_lowercase().replace(' ', "").as_str() {
        "policy" => "Windows' storage policy kept it offline. This is normal for a newly added or moved disk.",
        "redundantpath" => "Windows sees the same disk twice (multipath) and turned one copy off.",
        "snapshot" => "It is a snapshot disk.",
        "collision" | "signaturecollision" => {
            "It has the same ID as another disk, usually because it was cloned. Bringing it online gives it a new ID."
        }
        "resourceexhaustion" => "Windows ran out of resources to mount it.",
        "criticalwritefailures" => {
            "Writes to it failed. Back it up before using it."
        }
        "dataintegrityscanrequired" => "It needs a data integrity scan.",
        _ => "Someone took it offline, or Windows did when it was added.",
    }
}

pub fn diagnose(layout: &Layout, windows_letter: char, letters_in_use: &[char]) -> Vec<Finding> {
    let mut out = Vec::new();
    let free_letter = ('D'..='Z').find(|c| !letters_in_use.contains(c));

    if layout.vds_start.eq_ignore_ascii_case("Disabled") {
        out.push(Finding {
            severity: Severity::Problem,
            title: "The Virtual Disk Service is disabled".into(),
            detail: "Disk Management can't load without it (\"Unable to connect to Virtual Disk Service\").".into(),
            disk: None,
            fix: Some(Fix::Action {
                action: Action::EnableVds,
                label: "Turn it back on".into(),
            }),
        });
    }

    for d in &layout.disks {
        let n = Some(d.number);
        if d.offline {
            out.push(Finding {
                severity: Severity::Problem,
                title: format!("Disk {} ({}) is offline", d.number, d.name),
                detail: format!(
                    "Its drives don't show in Explorer. {}",
                    offline_reason(&d.offline_reason)
                ),
                disk: n,
                fix: Some(Fix::Action {
                    action: Action::DiskOnline { disk: d.number },
                    label: "Bring online".into(),
                }),
            });
        }
        if d.read_only && !d.offline {
            out.push(Finding {
                severity: Severity::Problem,
                title: format!("Disk {} is read-only", d.number),
                detail: "Nothing can be saved to it. Often left over from a SAN policy or a failed copy. A failing disk can also switch itself to read-only: check its health first.".into(),
                disk: n,
                fix: Some(Fix::Action {
                    action: Action::DiskWritable { disk: d.number },
                    label: "Make writable".into(),
                }),
            });
        }
        if d.style.eq_ignore_ascii_case("RAW") {
            out.push(Finding {
                severity: Severity::Warning,
                title: format!("Disk {} is not initialized", d.number),
                detail: "Windows can't use it until it is initialized and partitioned. DeskMedic doesn't do that (it can erase data): use Disk Management if the disk is new and empty.".into(),
                disk: n,
                fix: None,
            });
        }
        let phys_bad = layout
            .physical
            .iter()
            .find(|p| p.id == d.number.to_string())
            .is_some_and(|p| !p.health.is_empty() && !p.health.eq_ignore_ascii_case("Healthy"));
        if (!d.health.is_empty() && !d.health.eq_ignore_ascii_case("Healthy")) || phys_bad {
            out.push(Finding {
                severity: Severity::Problem,
                title: format!("Disk {} reports it is not healthy", d.number),
                detail: "The drive itself is warning of failure. Back up its data now and plan to replace it.".into(),
                disk: n,
                fix: None,
            });
        }

        let free = layout.free_space(d);
        let parts = layout.partitions_of(d.number);
        for p in &parts {
            if p.read_only && !d.read_only {
                out.push(Finding {
                    severity: Severity::Problem,
                    title: format!("Partition {} on disk {} is read-only", p.number, d.number),
                    detail: "Files on it can be opened but not saved.".into(),
                    disk: n,
                    fix: Some(Fix::Action {
                        action: Action::PartitionWritable {
                            disk: d.number,
                            partition: p.number,
                        },
                        label: "Make writable".into(),
                    }),
                });
            }
            let Some(v) = layout.volume_of(p) else {
                continue;
            };
            let named = if v.letter.is_empty() {
                format!("partition {} on disk {}", p.number, d.number)
            } else {
                format!("{}:", v.letter)
            };
            if p.kind() == PartKind::Basic && p.letter.is_empty() && !v.fs.is_empty() && !p.hidden {
                out.push(Finding {
                    severity: Severity::Warning,
                    title: format!("A {} volume on disk {} has no drive letter", v.fs, d.number),
                    detail: "It has data but doesn't show in Explorer. This often happens to USB drives after a letter clash with a network drive.".into(),
                    disk: n,
                    fix: free_letter.map(|letter| Fix::Action {
                        action: Action::SetLetter {
                            disk: d.number,
                            partition: p.number,
                            letter,
                        },
                        label: format!("Give it {letter}:"),
                    }),
                });
            }
            if !v.health.is_empty() && !v.health.eq_ignore_ascii_case("Healthy") {
                if let Some(l) = v.letter.chars().next() {
                    out.push(Finding {
                        severity: Severity::Problem,
                        title: format!("{named} reports file-system problems"),
                        detail: "Windows found errors on this volume. Check it, then repair if errors are confirmed.".into(),
                        disk: n,
                        fix: Some(Fix::Action {
                            action: Action::CheckVolume { letter: l },
                            label: "Check for errors".into(),
                        }),
                    });
                }
            }
            if v.size > 0 && !v.letter.is_empty() && p.kind() == PartKind::Basic {
                let pct_free = v.free as f64 * 100.0 / v.size as f64;
                if pct_free < 10.0 {
                    out.push(Finding {
                        severity: if pct_free < 5.0 {
                            Severity::Problem
                        } else {
                            Severity::Warning
                        },
                        title: format!("{named} is nearly full ({pct_free:.0}% free)"),
                        detail: "Windows slows down and updates fail when the drive is this full."
                            .into(),
                        disk: n,
                        fix: Some(Fix::OpenCleanup),
                    });
                }
            }

            // Free space and extending.
            if p.kind() == PartKind::Basic && !v.letter.is_empty() && !free.is_empty() {
                let l = v.letter.chars().next().unwrap_or(windows_letter);
                let right_after = free.iter().any(|(off, _)| *off == p.end());
                let later = free
                    .iter()
                    .filter(|(off, _)| *off > p.end())
                    .map(|(_, s)| s)
                    .sum::<u64>();
                if right_after {
                    out.push(Finding {
                        severity: Severity::Info,
                        title: format!("{named} can be extended"),
                        detail: "There is unallocated space right after it.".into(),
                        disk: n,
                        fix: Some(Fix::ExtendGuide { letter: l }),
                    });
                } else if later > 0 {
                    let blocker = parts
                        .iter()
                        .find(|q| q.offset >= p.end())
                        .map(|q| match q.kind() {
                            PartKind::Recovery => "the Windows recovery partition".to_string(),
                            k => format!("partition {} ({k:?})", q.number),
                        })
                        .unwrap_or_else(|| "another partition".into());
                    out.push(Finding {
                        severity: Severity::Warning,
                        title: format!("\"Extend Volume\" is greyed out for {named}"),
                        detail: format!(
                            "There are {:.1} GB unallocated on this disk, but {blocker} sits between {named} and that space. Windows can only extend into space directly after a volume.",
                            later as f64 / 1e9
                        ),
                        disk: n,
                        fix: Some(Fix::ExtendGuide { letter: l }),
                    });
                }
            }
        }
    }
    out.sort_by_key(|f| f.severity);
    out
}

#[cfg(test)]
mod tests {
    use super::super::model::tests::*;
    use super::*;

    fn titles(f: &[Finding]) -> Vec<String> {
        f.iter().map(|x| x.title.clone()).collect()
    }

    #[test]
    fn healthy_pc_has_no_problems() {
        let mut l = grown_vm_disk();
        l.disks[0].size = 99 * GB + 900 * 1024 * 1024; // no free space
        l.volumes[0].free = 40 * GB;
        assert!(
            diagnose(&l, 'C', &['C']).is_empty(),
            "{:?}",
            titles(&diagnose(&l, 'C', &['C']))
        );
    }

    #[test]
    fn recovery_partition_in_the_way() {
        let f = diagnose(&grown_vm_disk(), 'C', &['C']);
        let extend = f
            .iter()
            .find(|x| x.title.contains("greyed out"))
            .expect("finding");
        assert!(
            extend.detail.contains("recovery partition"),
            "{}",
            extend.detail
        );
        assert_eq!(extend.fix, Some(Fix::ExtendGuide { letter: 'C' }));
        // 4 of 99 GB free: nearly full too.
        assert!(f.iter().any(|x| x.title.contains("nearly full")));
    }

    #[test]
    fn free_space_right_after_means_extendable() {
        let mut l = grown_vm_disk();
        // Drop the recovery partition: the gap now follows C: directly.
        l.partitions.pop();
        let f = diagnose(&l, 'C', &['C']);
        assert!(
            f.iter().any(|x| x.title == "C: can be extended"),
            "{:?}",
            titles(&f)
        );
        assert!(!f.iter().any(|x| x.title.contains("greyed out")));
    }

    #[test]
    fn offline_and_read_only_disks() {
        let mut l = grown_vm_disk();
        let mut d = disk(1, 500 * GB);
        d.offline = true;
        d.offline_reason = "Signature Collision".into();
        l.disks.push(d);
        let f = diagnose(&l, 'C', &['C']);
        let off = f.iter().find(|x| x.title.contains("offline")).unwrap();
        assert!(off.detail.contains("cloned"), "{}", off.detail);
        assert_eq!(
            off.fix,
            Some(Fix::Action {
                action: Action::DiskOnline { disk: 1 },
                label: "Bring online".into()
            })
        );
        assert_eq!(f[0].severity, Severity::Problem, "problems sort first");
    }

    #[test]
    fn disabled_vds_and_unhealthy_disk() {
        let mut l = grown_vm_disk();
        l.vds_start = "Disabled".into();
        l.disks[0].health = "Warning".into();
        let t = titles(&diagnose(&l, 'C', &['C']));
        assert!(t
            .iter()
            .any(|x| x.contains("Virtual Disk Service is disabled")));
        assert!(t.iter().any(|x| x.contains("not healthy")));
    }

    #[test]
    fn data_volume_without_letter() {
        let mut l = grown_vm_disk();
        l.disks.push(disk(1, 64 * GB));
        l.partitions.push(part(1, 1, 1 << 20, 60 * GB, BASIC, ""));
        l.volumes.push(vol(1, 1, "", 60 * GB, 50 * GB));
        let f = diagnose(&l, 'C', &['C', 'D']);
        let nl = f
            .iter()
            .find(|x| x.title.contains("has no drive letter"))
            .expect("finding");
        assert!(
            matches!(
                &nl.fix,
                Some(Fix::Action {
                    action: Action::SetLetter { letter: 'E', .. },
                    ..
                })
            ),
            "{:?}",
            nl.fix
        );
    }
}
