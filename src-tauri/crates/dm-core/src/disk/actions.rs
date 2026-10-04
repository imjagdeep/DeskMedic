//! Changes DeskMedic can make to disks. Each one:
//! - is checked against a fresh read of the layout right before it runs,
//! - needs the user to type a confirmation word (the disk number or letter),
//! - runs a fixed PowerShell script whose only inputs are validated numbers
//!   or a drive letter, passed as environment variables.
//!
//! Nothing here formats, initializes or deletes a data partition.

use super::model::{Layout, PartKind};
use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Bring an offline disk online.
    DiskOnline { disk: u32 },
    /// Clear a disk's read-only flag.
    DiskWritable { disk: u32 },
    /// Clear a partition's read-only flag.
    PartitionWritable { disk: u32, partition: u32 },
    /// Give a partition a drive letter (or a different one).
    SetLetter {
        disk: u32,
        partition: u32,
        letter: char,
    },
    /// Read-only check of a volume for file-system errors.
    CheckVolume { letter: char },
    /// Repair: offline scan-and-fix for data drives; for the Windows drive,
    /// mark it so Windows checks it on the next restart.
    FixVolume { letter: char },
    /// Restart the Virtual Disk Service (Disk Management stuck "connecting").
    RestartVds,
    /// Set a disabled Virtual Disk Service back to Manual and start it.
    EnableVds,
}

impl Action {
    /// What the user must type to confirm.
    pub fn confirm_word(&self) -> String {
        match self {
            Action::DiskOnline { disk } | Action::DiskWritable { disk } => disk.to_string(),
            Action::PartitionWritable { disk, .. } => disk.to_string(),
            Action::SetLetter { letter, .. } => letter.to_string(),
            Action::CheckVolume { letter } | Action::FixVolume { letter } => letter.to_string(),
            Action::RestartVds | Action::EnableVds => "VDS".into(),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Action::DiskOnline { disk } => format!("Bring disk {disk} online"),
            Action::DiskWritable { disk } => format!("Make disk {disk} writable"),
            Action::PartitionWritable { disk, partition } => {
                format!("Make partition {partition} on disk {disk} writable")
            }
            Action::SetLetter {
                disk,
                partition,
                letter,
            } => format!("Give partition {partition} on disk {disk} the letter {letter}:"),
            Action::CheckVolume { letter } => format!("Check {letter}: for errors"),
            Action::FixVolume { letter } => format!("Repair {letter}:"),
            Action::RestartVds => "Restart the Virtual Disk Service".into(),
            Action::EnableVds => "Turn the Virtual Disk Service back on".into(),
        }
    }
}

fn letter_ok(c: char) -> Result<char, String> {
    let u = c.to_ascii_uppercase();
    if ('C'..='Z').contains(&u) {
        Ok(u)
    } else {
        Err(format!("{c} is not a usable drive letter"))
    }
}

/// Check `action` against the current layout. `windows_letter` is the drive
/// Windows runs from; `letters_in_use` includes network drives.
pub fn validate(
    action: &Action,
    layout: &Layout,
    windows_letter: char,
    letters_in_use: &[char],
) -> Result<(), String> {
    let disk = |n: u32| {
        layout
            .disks
            .iter()
            .find(|d| d.number == n)
            .ok_or_else(|| format!("There is no disk {n}."))
    };
    let partition = |d: u32, p: u32| {
        layout
            .partitions
            .iter()
            .find(|x| x.disk == d && x.number == p)
            .ok_or_else(|| format!("There is no partition {p} on disk {d}."))
    };
    let volume = |l: char| {
        layout
            .volumes
            .iter()
            .find(|v| v.letter.eq_ignore_ascii_case(&l.to_string()))
            .ok_or_else(|| format!("There is no volume {l}:."))
    };
    match action {
        Action::DiskOnline { disk: n } => {
            if !disk(*n)?.offline {
                return Err(format!("Disk {n} is already online."));
            }
        }
        Action::DiskWritable { disk: n } => {
            if !disk(*n)?.read_only {
                return Err(format!("Disk {n} is not read-only."));
            }
        }
        Action::PartitionWritable {
            disk: d,
            partition: p,
        } => {
            if !partition(*d, *p)?.read_only {
                return Err("That partition is not read-only.".into());
            }
        }
        Action::SetLetter {
            disk: d,
            partition: p,
            letter,
        } => {
            let l = letter_ok(*letter)?;
            let part = partition(*d, *p)?;
            if part.boot
                || part.system
                || part
                    .letter
                    .eq_ignore_ascii_case(&windows_letter.to_string())
            {
                return Err("The Windows and boot partitions keep their letters.".into());
            }
            if part.kind() != PartKind::Basic {
                return Err("Only normal data partitions get drive letters.".into());
            }
            if layout.volume_of(part).is_none_or(|v| v.fs.is_empty()) {
                return Err("That partition has no file system Windows can read.".into());
            }
            if letters_in_use.contains(&l) {
                return Err(format!("{l}: is already in use."));
            }
        }
        Action::CheckVolume { letter } | Action::FixVolume { letter } => {
            let l = letter_ok(*letter)?;
            let v = volume(l)?;
            if !matches!(
                v.fs.to_ascii_uppercase().as_str(),
                "NTFS" | "REFS" | "FAT32" | "EXFAT" | "FAT"
            ) {
                return Err(format!("{l}: has no file system to check."));
            }
        }
        Action::RestartVds => {}
        Action::EnableVds => {
            if !layout.vds_start.eq_ignore_ascii_case("Disabled") {
                return Err("The Virtual Disk Service is not disabled.".into());
            }
        }
    }
    Ok(())
}

const DISK_ONLINE: &str = "Set-Disk -Number ([int]$env:DM_ARG_DISK) -IsOffline $false";
const DISK_WRITABLE: &str = "Set-Disk -Number ([int]$env:DM_ARG_DISK) -IsReadOnly $false";
const PART_WRITABLE: &str = "Set-Partition -DiskNumber ([int]$env:DM_ARG_DISK) -PartitionNumber ([int]$env:DM_ARG_PART) -IsReadOnly $false";
const SET_LETTER: &str = "Set-Partition -DiskNumber ([int]$env:DM_ARG_DISK) -PartitionNumber ([int]$env:DM_ARG_PART) -NewDriveLetter ([char]$env:DM_ARG_LETTER)";
const CHECK_VOLUME: &str = "[string](Repair-Volume -DriveLetter ([char]$env:DM_ARG_LETTER) -Scan)";
const FIX_VOLUME: &str =
    "[string](Repair-Volume -DriveLetter ([char]$env:DM_ARG_LETTER) -OfflineScanAndFix)";
const RESTART_VDS: &str = r#"
$s = Get-Service -Name vds
if ($s.Status -eq 'Running') { Restart-Service -Name vds -Force } else { Start-Service -Name vds }
[string](Get-Service -Name vds).Status
"#;
const ENABLE_VDS: &str = r#"
Set-Service -Name vds -StartupType Manual
Start-Service -Name vds
[string](Get-Service -Name vds).Status
"#;

/// Run a validated action; returns a one-line result for the user and log.
pub fn run(action: &Action, windows_letter: char) -> Result<String, String> {
    let t = Duration::from_secs(600);
    let err = |e: ps::PsError| e.to_string();
    match action {
        Action::DiskOnline { disk } => {
            ps::run_text(DISK_ONLINE, &[("DISK", &disk.to_string())], t).map_err(err)?;
            Ok(format!("Disk {disk} is online."))
        }
        Action::DiskWritable { disk } => {
            ps::run_text(DISK_WRITABLE, &[("DISK", &disk.to_string())], t).map_err(err)?;
            Ok(format!("Disk {disk} is writable."))
        }
        Action::PartitionWritable { disk, partition } => {
            ps::run_text(
                PART_WRITABLE,
                &[
                    ("DISK", &disk.to_string()),
                    ("PART", &partition.to_string()),
                ],
                t,
            )
            .map_err(err)?;
            Ok("The partition is writable.".into())
        }
        Action::SetLetter {
            disk,
            partition,
            letter,
        } => {
            ps::run_text(
                SET_LETTER,
                &[
                    ("DISK", &disk.to_string()),
                    ("PART", &partition.to_string()),
                    ("LETTER", &letter.to_ascii_uppercase().to_string()),
                ],
                t,
            )
            .map_err(err)?;
            Ok(format!(
                "The partition is now {}:.",
                letter.to_ascii_uppercase()
            ))
        }
        Action::CheckVolume { letter } => {
            let out =
                ps::run_text(CHECK_VOLUME, &[("LETTER", &letter.to_string())], t).map_err(err)?;
            Ok(match out.as_str() {
                "NoErrorsFound" => format!("{letter}: no errors found."),
                "ScanNeeded" | "SpotFixNeeded" | "FullRepairNeeded" => {
                    format!("{letter}: has errors ({out}). Use Repair.")
                }
                other => format!("{letter}: {other}"),
            })
        }
        Action::FixVolume { letter } => {
            if letter.eq_ignore_ascii_case(&windows_letter) {
                // Windows can't repair the drive it runs from while running:
                // set the dirty bit so autochk checks it on the next start.
                let drive = format!("{}:", letter.to_ascii_uppercase());
                let out = crate::run::run(
                    "fsutil.exe",
                    &["dirty", "set", &drive],
                    &[],
                    Duration::from_secs(60),
                )
                .map_err(|e| e.to_string())?;
                if !out.success() {
                    return Err(format!("fsutil failed: {}", out.stdout));
                }
                Ok(format!(
                    "{drive} will be checked and repaired on the next restart."
                ))
            } else {
                let out =
                    ps::run_text(FIX_VOLUME, &[("LETTER", &letter.to_string())], t).map_err(err)?;
                Ok(format!("{letter}: {out}"))
            }
        }
        Action::RestartVds => {
            let s = ps::run_text(RESTART_VDS, &[], Duration::from_secs(120)).map_err(err)?;
            Ok(format!(
                "Virtual Disk Service: {s}. Open Disk Management again."
            ))
        }
        Action::EnableVds => {
            let s = ps::run_text(ENABLE_VDS, &[], Duration::from_secs(120)).map_err(err)?;
            Ok(format!("Virtual Disk Service: {s}."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::tests::*;
    use super::*;

    fn layout_with_data_disk() -> Layout {
        let mut l = grown_vm_disk();
        let mut d1 = disk(1, 1000 * GB);
        d1.offline = true;
        d1.read_only = true;
        l.disks.push(d1);
        l.partitions.push(part(1, 1, 1 << 20, 900 * GB, BASIC, ""));
        l.volumes.push(vol(1, 1, "", 900 * GB, 800 * GB));
        l
    }

    #[test]
    fn disk_actions_need_the_matching_state() {
        let l = layout_with_data_disk();
        assert!(validate(&Action::DiskOnline { disk: 1 }, &l, 'C', &[]).is_ok());
        assert!(validate(&Action::DiskOnline { disk: 0 }, &l, 'C', &[]).is_err());
        assert!(validate(&Action::DiskWritable { disk: 1 }, &l, 'C', &[]).is_ok());
        assert!(validate(&Action::DiskOnline { disk: 7 }, &l, 'C', &[]).is_err());
    }

    #[test]
    fn letters() {
        let l = layout_with_data_disk();
        let give = |letter| Action::SetLetter {
            disk: 1,
            partition: 1,
            letter,
        };
        assert!(validate(&give('e'), &l, 'C', &['C']).is_ok());
        assert!(
            validate(&give('E'), &l, 'C', &['C', 'E']).is_err(),
            "in use"
        );
        assert!(
            validate(&give('A'), &l, 'C', &[]).is_err(),
            "A/B not allowed"
        );
        // The Windows partition keeps its letter.
        let c = Action::SetLetter {
            disk: 0,
            partition: 2,
            letter: 'D',
        };
        assert!(validate(&c, &l, 'C', &['C']).is_err());
        // EFI never gets a letter.
        let efi = Action::SetLetter {
            disk: 0,
            partition: 1,
            letter: 'S',
        };
        assert!(validate(&efi, &l, 'C', &['C']).is_err());
    }

    #[test]
    fn confirm_words() {
        assert_eq!(Action::DiskOnline { disk: 3 }.confirm_word(), "3");
        assert_eq!(Action::CheckVolume { letter: 'D' }.confirm_word(), "D");
        assert_eq!(Action::RestartVds.confirm_word(), "VDS");
    }

    #[test]
    fn vds_enable_only_when_disabled() {
        let mut l = grown_vm_disk();
        assert!(validate(&Action::EnableVds, &l, 'C', &[]).is_err());
        l.vds_start = "Disabled".into();
        assert!(validate(&Action::EnableVds, &l, 'C', &[]).is_ok());
    }

    #[test]
    fn actions_round_trip_as_json() {
        let a = Action::SetLetter {
            disk: 1,
            partition: 2,
            letter: 'F',
        };
        let j = serde_json::to_string(&a).unwrap();
        assert_eq!(
            j,
            r#"{"kind":"set_letter","disk":1,"partition":2,"letter":"F"}"#
        );
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), a);
    }
}
