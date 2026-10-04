//! Extending a volume, including the common case where the Windows
//! recovery partition sits between C: and the free space (a VM disk that was
//! grown, a bigger drive after cloning). That case follows Microsoft's
//! documented WinRE procedure:
//!
//! 1. `reagentc /disable` (moves winre.wim to C:\Windows\System32\Recovery)
//! 2. delete the recovery partition
//! 3. extend C:, leaving room at the end
//! 4. create a new recovery partition there (diskpart: NTFS, recovery type,
//!    GPT attributes 0x8000000000000001 / MBR id 27)
//! 5. `reagentc /enable` (copies winre.wim into it)
//!
//! The plan is recomputed from the live disk state before every step, so a
//! run that stops half-way can be resumed. It refuses to touch a recovery
//! partition Windows isn't using (it may be the PC maker's factory image)
//! and any other kind of partition in the way.

use super::model::{Layout, PartKind, Partition};
use crate::{ps, run};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

const MIB: u64 = 1024 * 1024;
/// Size of the recreated recovery partition, at least.
const MIN_RECOVERY: u64 = 1024 * MIB;

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct WinRe {
    /// Disk and partition of the WinRE image when enabled.
    pub location: Option<(u32, u32)>,
    /// winre.wim parked in C:\Windows\System32\Recovery (WinRE disabled).
    pub wim_on_windows: bool,
}

/// Language-neutral parse of `reagentc /info`: only the
/// `harddiskN\partitionM` part of the location line is used.
pub fn parse_reagentc(out: &str) -> Option<(u32, u32)> {
    let lower = out.to_ascii_lowercase();
    let i = lower.find("\\harddisk")?;
    let rest = &lower[i + "\\harddisk".len()..];
    let disk: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let j = rest.find("\\partition")?;
    let part: String = rest[j + "\\partition".len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Some((disk.parse().ok()?, part.parse().ok()?))
}

fn parked_wim() -> PathBuf {
    let win = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    win.join("System32").join("Recovery").join("Winre.wim")
}

/// Needs admin (reagentc refuses otherwise).
pub fn winre() -> Result<WinRe, String> {
    let out = run::run("reagentc.exe", &["/info"], &[], Duration::from_secs(60))
        .map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(format!("reagentc /info failed: {}", out.stdout));
    }
    Ok(WinRe {
        location: parse_reagentc(&out.stdout),
        wim_on_windows: parked_wim().is_file(),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    DisableWinRe,
    DeleteRecovery {
        disk: u32,
        partition: u32,
    },
    Extend {
        disk: u32,
        partition: u32,
        size: u64,
    },
    CreateRecovery {
        disk: u32,
        offset: u64,
        size: u64,
        gpt: bool,
    },
    EnableWinRe,
}

impl Step {
    pub fn describe(&self) -> String {
        match self {
            Step::DisableWinRe => {
                "Turn off Windows Recovery for a moment (reagentc /disable)".into()
            }
            Step::DeleteRecovery { partition, .. } => {
                format!("Remove the recovery partition (partition {partition}); its contents are kept on C:")
            }
            Step::Extend { size, .. } => {
                format!("Extend the volume to {:.1} GB", *size as f64 / 1e9)
            }
            Step::CreateRecovery { size, .. } => format!(
                "Create a new {:.1} GB recovery partition at the end of the disk",
                *size as f64 / 1e9
            ),
            Step::EnableWinRe => "Turn Windows Recovery back on (reagentc /enable)".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Plan {
    pub letter: char,
    pub current_size: u64,
    pub new_size: u64,
    pub steps: Vec<Step>,
    /// Why it can't be done (steps is then empty).
    pub blocked: Option<String>,
    /// True when the recovery partition has to move.
    pub moves_recovery: bool,
}

fn blocked(letter: char, current: u64, why: String) -> Plan {
    Plan {
        letter,
        current_size: current,
        new_size: current,
        steps: vec![],
        blocked: Some(why),
        moves_recovery: false,
    }
}

fn floor_mib(x: u64) -> u64 {
    x / MIB * MIB
}

/// What extending `letter` takes, given the layout and WinRE state
/// (`winre` is `None` when it couldn't be read, e.g. without admin).
pub fn plan(layout: &Layout, winre: Option<&WinRe>, letter: char, windows_letter: char) -> Plan {
    let Some(p) = layout
        .partitions
        .iter()
        .find(|p| p.letter.eq_ignore_ascii_case(&letter.to_string()))
    else {
        return blocked(letter, 0, format!("There is no {letter}: partition."));
    };
    let Some(disk) = layout.disks.iter().find(|d| d.number == p.disk) else {
        return blocked(letter, p.size, "Its disk is missing.".into());
    };
    let gpt = disk.style.eq_ignore_ascii_case("GPT");
    let free = layout.free_space(disk);
    let parts = layout.partitions_of(disk.number);
    let next: Option<&&Partition> = parts.iter().find(|q| q.offset >= p.end());
    let is_windows = letter.eq_ignore_ascii_case(&windows_letter);
    let has_recovery = parts.iter().any(|q| q.kind() == PartKind::Recovery);

    // Free space directly after the volume.
    if let Some(&(_, gap)) = free.iter().find(|(off, _)| *off == p.end()) {
        // Resuming a recovery move: WinRE parked on C:, no recovery partition
        // left. Never extend into the room the new recovery partition needs.
        if let Some(w) = winre {
            if is_windows && w.location.is_none() && w.wim_on_windows && !has_recovery {
                let (new_size, mut steps) = if gap > MIN_RECOVERY * 2 {
                    let new_size = floor_mib(p.size + gap - MIN_RECOVERY);
                    (
                        new_size,
                        vec![Step::Extend {
                            disk: disk.number,
                            partition: p.number,
                            size: new_size,
                        }],
                    )
                } else {
                    // Already extended: only the reserve is left.
                    (p.size, vec![])
                };
                if p.offset + new_size + MIN_RECOVERY - MIB > p.end() + gap {
                    return blocked(
                        letter,
                        p.size,
                        "Windows Recovery is turned off and there is no room left for its partition. Shrink C: by 1 GB, then run this again.".into(),
                    );
                }
                steps.push(Step::CreateRecovery {
                    disk: disk.number,
                    offset: p.offset + new_size,
                    size: floor_mib(p.end() + gap - (p.offset + new_size)),
                    gpt,
                });
                steps.push(Step::EnableWinRe);
                return Plan {
                    letter,
                    current_size: p.size,
                    new_size,
                    steps,
                    blocked: None,
                    moves_recovery: true,
                };
            }
        }
        let new_size = floor_mib(p.size + gap);
        return Plan {
            letter,
            current_size: p.size,
            new_size,
            steps: vec![Step::Extend {
                disk: disk.number,
                partition: p.number,
                size: new_size,
            }],
            blocked: None,
            moves_recovery: false,
        };
    }

    let Some(r) = next else {
        return blocked(
            letter,
            p.size,
            "There is no unallocated space after it.".into(),
        );
    };
    let after_r = free.iter().find(|(off, _)| *off == r.end());
    let Some(&(_, gap)) = after_r else {
        return blocked(
            letter,
            p.size,
            "There is no unallocated space it could grow into.".into(),
        );
    };
    if r.kind() != PartKind::Recovery {
        return blocked(
            letter,
            p.size,
            format!(
                "Partition {} ({:?}) sits between {letter}: and the free space. DeskMedic won't move or delete it.",
                r.number,
                r.kind()
            ),
        );
    }
    if !is_windows {
        return blocked(
            letter,
            p.size,
            "A recovery partition is in the way. DeskMedic only moves the one belonging to this Windows.".into(),
        );
    }
    let Some(w) = winre else {
        return blocked(
            letter,
            p.size,
            "Run DeskMedic as administrator to check Windows Recovery first.".into(),
        );
    };
    if w.location != Some((disk.number, r.number)) {
        return blocked(
            letter,
            p.size,
            "That recovery partition isn't the one Windows uses (it may be the PC maker's factory restore). DeskMedic won't delete it.".into(),
        );
    }
    let reserve = floor_mib(r.size.max(MIN_RECOVERY) + MIB - 1).max(MIN_RECOVERY);
    let total = r.end() + gap - p.offset;
    if total <= reserve + p.size {
        return blocked(
            letter,
            p.size,
            "There isn't enough free space to be worth it.".into(),
        );
    }
    let new_size = floor_mib(total - reserve);
    let new_rec_offset = p.offset + new_size;
    Plan {
        letter,
        current_size: p.size,
        new_size,
        steps: vec![
            Step::DisableWinRe,
            Step::DeleteRecovery {
                disk: disk.number,
                partition: r.number,
            },
            Step::Extend {
                disk: disk.number,
                partition: p.number,
                size: new_size,
            },
            Step::CreateRecovery {
                disk: disk.number,
                offset: new_rec_offset,
                size: floor_mib(r.end() + gap - new_rec_offset),
                gpt,
            },
            Step::EnableWinRe,
        ],
        blocked: None,
        moves_recovery: true,
    }
}

const DELETE_PARTITION: &str = "Remove-Partition -DiskNumber ([int]$env:DM_ARG_DISK) -PartitionNumber ([int]$env:DM_ARG_PART) -Confirm:$false";
const RESIZE: &str = "Resize-Partition -DiskNumber ([int]$env:DM_ARG_DISK) -PartitionNumber ([int]$env:DM_ARG_PART) -Size ([uint64]$env:DM_ARG_SIZE)";

/// diskpart script for the new recovery partition. Only numbers go in.
pub fn recovery_script(disk: u32, offset: u64, size: u64, gpt: bool) -> String {
    let id = if gpt {
        "set id=de94bba4-06d1-4d40-a16a-bfd50179d6ac override\ngpt attributes=0x8000000000000001"
    } else {
        "set id=27 override"
    };
    format!(
        "select disk {disk}\ncreate partition primary size={} offset={}\nformat quick fs=ntfs label=\"Windows RE tools\"\nremove noerr\n{id}\nexit\n",
        size / MIB,
        offset / 1024
    )
}

/// Run one step. The caller re-plans before each one.
pub fn run_step(step: &Step) -> Result<String, String> {
    let t = Duration::from_secs(900);
    match step {
        Step::DisableWinRe => {
            let out = run::run("reagentc.exe", &["/disable"], &[], t).map_err(|e| e.to_string())?;
            if !out.success() || !parked_wim().is_file() {
                return Err(format!(
                    "reagentc /disable did not park winre.wim on C: ({})",
                    out.stdout
                ));
            }
            Ok("Windows Recovery is off; its image is safe on C:.".into())
        }
        Step::DeleteRecovery { disk, partition } => {
            // Last check: WinRE must be off and its image parked on C:.
            let w = winre()?;
            if w.location.is_some() || !w.wim_on_windows {
                return Err(
                    "Windows Recovery is still using that partition; not deleting it.".into(),
                );
            }
            ps::run_text(
                DELETE_PARTITION,
                &[
                    ("DISK", &disk.to_string()),
                    ("PART", &partition.to_string()),
                ],
                t,
            )
            .map_err(|e| e.to_string())?;
            Ok(format!("Removed partition {partition}."))
        }
        Step::Extend {
            disk,
            partition,
            size,
        } => {
            ps::run_text(
                RESIZE,
                &[
                    ("DISK", &disk.to_string()),
                    ("PART", &partition.to_string()),
                    ("SIZE", &size.to_string()),
                ],
                t,
            )
            .map_err(|e| e.to_string())?;
            Ok(format!("Extended to {:.1} GB.", *size as f64 / 1e9))
        }
        Step::CreateRecovery {
            disk,
            offset,
            size,
            gpt,
        } => {
            let script =
                std::env::temp_dir().join(format!("deskmedic-re-{}.txt", std::process::id()));
            std::fs::write(&script, recovery_script(*disk, *offset, *size, *gpt))
                .map_err(|e| format!("could not write the diskpart script: {e}"))?;
            let path = script.to_string_lossy().into_owned();
            let out = run::run("diskpart.exe", &["/s", &path], &[], t);
            let _ = std::fs::remove_file(&script);
            let out = out.map_err(|e| e.to_string())?;
            if !out.success() {
                return Err(format!("diskpart failed: {}", tail(&out.stdout)));
            }
            Ok("Created the new recovery partition.".into())
        }
        Step::EnableWinRe => {
            let out = run::run("reagentc.exe", &["/enable"], &[], t).map_err(|e| e.to_string())?;
            let w = winre()?;
            if !out.success() || w.location.is_none() {
                return Err(format!("reagentc /enable failed: {}", tail(&out.stdout)));
            }
            Ok("Windows Recovery is back on.".into())
        }
    }
}

/// Run `plan` step by step. Before each step the disk is read again and the
/// step's starting point is checked, so nothing runs against a layout that
/// changed underneath. Stops at the first failure; `report` gets every
/// step's result (for the log). A stopped run can be resumed: planning
/// again picks up where it left off.
pub fn execute(
    plan: &Plan,
    mut report: impl FnMut(&Step, &Result<String, String>),
) -> Result<Vec<String>, String> {
    if let Some(b) = &plan.blocked {
        return Err(b.clone());
    }
    let mut done = Vec::new();
    for step in &plan.steps {
        let check = precondition(step);
        let result = check.and_then(|()| run_step(step));
        report(step, &result);
        match result {
            Ok(msg) => done.push(msg),
            Err(e) => {
                return Err(format!(
                    "Stopped at \"{}\": {e}. Nothing after it ran. Open this again to continue from here.",
                    step.describe()
                ))
            }
        }
    }
    Ok(done)
}

fn precondition(step: &Step) -> Result<(), String> {
    let layout = super::model::read().map_err(|e| e.to_string())?;
    match step {
        Step::DisableWinRe | Step::EnableWinRe | Step::DeleteRecovery { .. } => Ok(()),
        Step::Extend {
            disk,
            partition,
            size,
        } => {
            let p = layout
                .partitions
                .iter()
                .find(|p| p.disk == *disk && p.number == *partition)
                .ok_or("the partition to extend is gone")?;
            if p.size >= *size {
                return Err("it is already that size or bigger".into());
            }
            let d = layout
                .disks
                .iter()
                .find(|d| d.number == *disk)
                .ok_or("disk gone")?;
            let room = layout
                .free_space(d)
                .iter()
                .find(|(off, _)| *off == p.end())
                .map(|(_, s)| *s)
                .unwrap_or(0);
            if p.size + room < *size {
                return Err("there is not enough free space right after it".into());
            }
            Ok(())
        }
        Step::CreateRecovery {
            disk, offset, size, ..
        } => {
            let d = layout
                .disks
                .iter()
                .find(|d| d.number == *disk)
                .ok_or("disk gone")?;
            let fits = layout
                .free_space(d)
                .iter()
                .any(|(off, s)| *off <= *offset && off + s >= offset + size);
            if fits {
                Ok(())
            } else {
                Err("the space for the recovery partition is not free".into())
            }
        }
    }
}

fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" / ")
}

/// Smallest and largest size Windows allows for a partition right now
/// (shrinking stops at files it can't move). Needs admin.
pub fn supported_size(disk: u32, partition: u32) -> Result<(u64, u64), String> {
    #[derive(serde::Deserialize)]
    struct S {
        #[serde(rename = "SizeMin")]
        min: u64,
        #[serde(rename = "SizeMax")]
        max: u64,
    }
    let s: S = ps::run_json(
        "Get-PartitionSupportedSize -DiskNumber ([int]$env:DM_ARG_DISK) -PartitionNumber ([int]$env:DM_ARG_PART) | Select-Object SizeMin, SizeMax | ConvertTo-Json -Compress",
        &[("DISK", &disk.to_string()), ("PART", &partition.to_string())],
        Duration::from_secs(300),
    )
    .map_err(|e| e.to_string())?;
    Ok((s.min, s.max))
}

#[cfg(test)]
mod tests {
    use super::super::model::tests::*;
    use super::*;

    #[test]
    fn reagentc_location() {
        let out = "Windows RE status:         Enabled\r\n    Windows RE location:       \\\\?\\GLOBALROOT\\device\\harddisk0\\partition4\\Recovery\\WindowsRE\r\n";
        assert_eq!(parse_reagentc(out), Some((0, 4)));
        // German output: same path, different words.
        let de = "Status von Windows RE:      Enabled\n Speicherort von Windows RE:  \\\\?\\GLOBALROOT\\device\\HardDisk1\\Partition12\\Recovery\\WindowsRE";
        assert_eq!(parse_reagentc(de), Some((1, 12)));
        assert_eq!(
            parse_reagentc("Windows RE status: Disabled\nWindows RE location:\n"),
            None
        );
    }

    fn winre_on(part: u32) -> WinRe {
        WinRe {
            location: Some((0, part)),
            wim_on_windows: false,
        }
    }

    #[test]
    fn recovery_in_the_way_full_plan() {
        let l = grown_vm_disk();
        let p = plan(&l, Some(&winre_on(3)), 'C', 'C');
        assert!(p.blocked.is_none(), "{:?}", p.blocked);
        assert!(p.moves_recovery);
        assert_eq!(p.steps.len(), 5);
        assert_eq!(p.steps[0], Step::DisableWinRe);
        assert_eq!(
            p.steps[1],
            Step::DeleteRecovery {
                disk: 0,
                partition: 3
            }
        );
        // C: grows by nearly all of recovery + free, minus the 1 GiB reserve.
        let Step::Extend { size, .. } = p.steps[2] else {
            panic!()
        };
        let Step::CreateRecovery {
            offset,
            size: rsize,
            gpt,
            ..
        } = p.steps[3]
        else {
            panic!()
        };
        assert!(gpt);
        assert!(size > 140 * GB && size < 150 * GB, "{size}");
        assert_eq!(
            offset,
            (1 << 20) * 101 + size,
            "new recovery starts right after C:"
        );
        assert!(rsize >= MIN_RECOVERY, "{rsize}");
        assert_eq!(size % MIB, 0);
    }

    #[test]
    fn refuses_a_recovery_partition_windows_does_not_use() {
        let l = grown_vm_disk();
        let p = plan(&l, Some(&winre_on(7)), 'C', 'C');
        assert!(p.blocked.unwrap().contains("factory"));
        let p = plan(&l, None, 'C', 'C');
        assert!(p.blocked.unwrap().contains("administrator"));
    }

    #[test]
    fn refuses_other_partitions_in_the_way() {
        let mut l = grown_vm_disk();
        l.partitions[2].gpt_type = "{0fc63daf-8483-4772-8e79-3d69d8477de4}".into(); // Linux
        let p = plan(&l, Some(&winre_on(3)), 'C', 'C');
        assert!(p.blocked.unwrap().contains("won't move or delete"));
    }

    #[test]
    fn simple_extend_when_space_follows() {
        let mut l = grown_vm_disk();
        l.partitions.pop(); // no recovery partition; gap follows C:
        let p = plan(&l, Some(&winre_on(9)), 'C', 'C');
        assert_eq!(p.steps.len(), 1);
        assert!(!p.moves_recovery);
        let Step::Extend { size, .. } = p.steps[0] else {
            panic!()
        };
        assert!(size > 149 * GB, "{size}");
    }

    #[test]
    fn resume_after_the_recovery_partition_was_removed() {
        let mut l = grown_vm_disk();
        l.partitions.pop();
        let parked = WinRe {
            location: None,
            wim_on_windows: true,
        };
        let p = plan(&l, Some(&parked), 'C', 'C');
        assert!(p.moves_recovery);
        assert_eq!(p.steps.len(), 3);
        assert!(matches!(p.steps[0], Step::Extend { .. }));
        assert_eq!(p.steps[2], Step::EnableWinRe);
    }

    /// Stopped after "extend": only the 1 GiB reserve is free after C:.
    /// The plan must create the recovery partition there, not grow C: into it.
    #[test]
    fn resume_after_extend_does_not_eat_the_reserve() {
        let mut l = grown_vm_disk();
        l.partitions.pop();
        l.disks[0].size = (1 << 20) * 101 + 99 * GB + MIN_RECOVERY;
        let parked = WinRe {
            location: None,
            wim_on_windows: true,
        };
        let p = plan(&l, Some(&parked), 'C', 'C');
        assert_eq!(p.steps.len(), 2, "{:?}", p.steps);
        assert!(matches!(p.steps[0], Step::CreateRecovery { .. }));
        assert_eq!(p.steps[1], Step::EnableWinRe);
    }

    #[test]
    fn diskpart_script_has_only_numbers_in_it() {
        let s = recovery_script(0, 160 * GB, 1024 * MIB, true);
        assert!(s.contains("select disk 0"));
        assert!(s.contains("size=1024"));
        assert!(s.contains("gpt attributes=0x8000000000000001"));
        assert!(recovery_script(1, GB, GB, false).contains("set id=27"));
    }
}
