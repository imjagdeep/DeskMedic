//! What Disk Management shows: disks, partitions, volumes and free space,
//! read through the Windows Storage cmdlets (works without admin).

use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Disk {
    pub number: u32,
    pub name: String,
    pub size: u64,
    /// "GPT", "MBR" or "RAW" (not initialized).
    pub style: String,
    pub offline: bool,
    #[serde(default)]
    pub offline_reason: String,
    pub read_only: bool,
    pub boot: bool,
    pub system: bool,
    #[serde(default)]
    pub bus: String,
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Partition {
    pub disk: u32,
    pub number: u32,
    pub offset: u64,
    pub size: u64,
    /// "" when none.
    #[serde(default)]
    pub letter: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub gpt_type: String,
    #[serde(default)]
    pub mbr_type: Option<u32>,
    pub system: bool,
    pub boot: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub offline: bool,
    /// Includes the volume's `\\?\Volume{…}\` path, which links it to a volume.
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub path: String,
    #[serde(default)]
    pub letter: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub fs: String,
    pub size: u64,
    pub free: u64,
    #[serde(default)]
    pub health: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalDisk {
    pub id: String,
    #[serde(default)]
    pub media: String,
    #[serde(default)]
    pub health: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub disks: Vec<Disk>,
    pub partitions: Vec<Partition>,
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub physical: Vec<PhysicalDisk>,
    /// Virtual Disk Service, which Disk Management talks to.
    pub vds_status: String,
    pub vds_start: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Efi,
    Reserved,
    Basic,
    Recovery,
    Linux,
    Extended,
    Other,
}

impl Partition {
    pub fn kind(&self) -> PartKind {
        let g = self.gpt_type.to_ascii_lowercase();
        let g = g.trim_matches(|c| c == '{' || c == '}');
        match g {
            "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" => return PartKind::Efi,
            "e3c9e316-0b5c-4db8-817d-f92df00215ae" => return PartKind::Reserved,
            "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7" => return PartKind::Basic,
            "de94bba4-06d1-4d40-a16a-bfd50179d6ac" => return PartKind::Recovery,
            "0fc63daf-8483-4772-8e79-3d69d8477de4" | "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f" => {
                return PartKind::Linux
            }
            "" => {}
            _ => return PartKind::Other,
        }
        match self.mbr_type {
            Some(0x07 | 0x0b | 0x0c | 0x0e | 0x06) => PartKind::Basic,
            Some(0x27) => PartKind::Recovery,
            Some(0x83 | 0x82) => PartKind::Linux,
            Some(0x05 | 0x0f) => PartKind::Extended,
            Some(0xef) => PartKind::Efi,
            _ => PartKind::Other,
        }
    }

    pub fn end(&self) -> u64 {
        self.offset + self.size
    }
}

/// One stretch of a disk, in order, as drawn in the disk bar.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Segment {
    Partition {
        partition: Box<Partition>,
        kind: PartKind,
        volume: Option<Volume>,
    },
    Free {
        offset: u64,
        size: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiskView {
    pub disk: Disk,
    pub segments: Vec<Segment>,
    pub physical: Option<PhysicalDisk>,
}

/// Gaps smaller than this are alignment and GPT bookkeeping, not real free space.
pub const MIN_FREE: u64 = 100 * 1024 * 1024;

impl Layout {
    pub fn volume_of(&self, p: &Partition) -> Option<&Volume> {
        self.volumes.iter().find(|v| {
            p.paths.iter().any(|a| a.eq_ignore_ascii_case(&v.path))
                || (!p.letter.is_empty() && v.letter.eq_ignore_ascii_case(&p.letter))
        })
    }

    pub fn partitions_of(&self, disk: u32) -> Vec<&Partition> {
        let mut parts: Vec<&Partition> =
            self.partitions.iter().filter(|p| p.disk == disk).collect();
        parts.sort_by_key(|p| p.offset);
        parts
    }

    /// Unallocated stretches of at least [`MIN_FREE`].
    pub fn free_space(&self, disk: &Disk) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        let mut cursor = 0u64;
        for p in self.partitions_of(disk.number) {
            // Logical partitions sit inside an extended one: skip the container.
            if p.kind() == PartKind::Extended {
                continue;
            }
            if p.offset > cursor && p.offset - cursor >= MIN_FREE {
                out.push((cursor, p.offset - cursor));
            }
            cursor = cursor.max(p.end());
        }
        if disk.size > cursor && disk.size - cursor >= MIN_FREE {
            out.push((cursor, disk.size - cursor));
        }
        out
    }

    pub fn views(&self) -> Vec<DiskView> {
        let mut disks = self.disks.clone();
        disks.sort_by_key(|d| d.number);
        disks
            .into_iter()
            .map(|disk| {
                let mut segments: Vec<(u64, Segment)> = self
                    .partitions_of(disk.number)
                    .into_iter()
                    .map(|p| {
                        (
                            p.offset,
                            Segment::Partition {
                                partition: Box::new(p.clone()),
                                kind: p.kind(),
                                volume: self.volume_of(p).cloned(),
                            },
                        )
                    })
                    .collect();
                for (offset, size) in self.free_space(&disk) {
                    segments.push((offset, Segment::Free { offset, size }));
                }
                segments.sort_by_key(|(o, _)| *o);
                DiskView {
                    physical: self
                        .physical
                        .iter()
                        .find(|p| p.id == disk.number.to_string())
                        .cloned(),
                    disk,
                    segments: segments.into_iter().map(|(_, s)| s).collect(),
                }
            })
            .collect()
    }
}

const READ_SCRIPT: &str = r#"
$disks = @(Get-Disk | ForEach-Object { [pscustomobject]@{
  number = [int]$_.Number; name = [string]$_.FriendlyName; size = [uint64]$_.Size
  style = [string]$_.PartitionStyle; offline = [bool]$_.IsOffline; offline_reason = [string]$_.OfflineReason
  read_only = [bool]$_.IsReadOnly; boot = [bool]$_.IsBoot; system = [bool]$_.IsSystem
  bus = [string]$_.BusType; health = [string]$_.HealthStatus; status = [string]$_.OperationalStatus } })
$parts = @(Get-Partition | ForEach-Object { [pscustomobject]@{
  disk = [int]$_.DiskNumber; number = [int]$_.PartitionNumber; offset = [uint64]$_.Offset; size = [uint64]$_.Size
  letter = ([string]$_.DriveLetter).Trim([char]0); type = [string]$_.Type; gpt_type = [string]$_.GptType
  mbr_type = $(if ($_.MbrType) { [int]$_.MbrType } else { $null })
  system = [bool]$_.IsSystem; boot = [bool]$_.IsBoot; hidden = [bool]$_.IsHidden
  read_only = [bool]$_.IsReadOnly; offline = [bool]$_.IsOffline
  paths = @($_.AccessPaths | ForEach-Object { [string]$_ }) } })
$vols = @(Get-Volume | Where-Object { $_.Path } | ForEach-Object { [pscustomobject]@{
  path = [string]$_.Path; letter = ([string]$_.DriveLetter).Trim([char]0); label = [string]$_.FileSystemLabel
  fs = [string]$_.FileSystem; size = [uint64]$_.Size; free = [uint64]$_.SizeRemaining; health = [string]$_.HealthStatus } })
$phys = @(try { Get-PhysicalDisk -ErrorAction Stop | ForEach-Object { [pscustomobject]@{
  id = [string]$_.DeviceId; media = [string]$_.MediaType; health = [string]$_.HealthStatus; status = [string]$_.OperationalStatus } } } catch { })
$vds = Get-Service -Name vds
[pscustomobject]@{ disks = $disks; partitions = $parts; volumes = $vols; physical = $phys
  vds_status = [string]$vds.Status; vds_start = [string]$vds.StartType } | ConvertTo-Json -Depth 6 -Compress
"#;

pub fn read() -> Result<Layout, ps::PsError> {
    ps::run_json(READ_SCRIPT, &[], Duration::from_secs(90))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const GB: u64 = 1_000_000_000;

    pub fn disk(number: u32, size: u64) -> Disk {
        Disk {
            number,
            name: format!("Disk {number}"),
            size,
            style: "GPT".into(),
            offline: false,
            offline_reason: String::new(),
            read_only: false,
            boot: number == 0,
            system: number == 0,
            bus: "NVMe".into(),
            health: "Healthy".into(),
            status: "Online".into(),
        }
    }

    pub fn part(
        disk: u32,
        number: u32,
        offset: u64,
        size: u64,
        gpt: &str,
        letter: &str,
    ) -> Partition {
        Partition {
            disk,
            number,
            offset,
            size,
            letter: letter.into(),
            r#type: String::new(),
            gpt_type: gpt.into(),
            mbr_type: None,
            system: false,
            boot: letter == "C",
            hidden: false,
            read_only: false,
            offline: false,
            paths: vec![format!("\\\\?\\Volume{{{disk}-{number}}}\\")],
        }
    }

    pub fn vol(disk: u32, number: u32, letter: &str, size: u64, free: u64) -> Volume {
        Volume {
            path: format!("\\\\?\\Volume{{{disk}-{number}}}\\"),
            letter: letter.into(),
            label: String::new(),
            fs: "NTFS".into(),
            size,
            free,
            health: "Healthy".into(),
        }
    }

    pub const EFI: &str = "{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}";
    pub const BASIC: &str = "{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}";
    pub const RECOVERY: &str = "{de94bba4-06d1-4d40-a16a-bfd50179d6ac}";

    /// EFI, C:, recovery, then 50 GB unallocated after the recovery
    /// partition: the classic "Extend Volume is greyed out" layout.
    pub fn grown_vm_disk() -> Layout {
        let mb = 1024 * 1024;
        Layout {
            disks: vec![disk(0, 150 * GB)],
            partitions: vec![
                part(0, 1, mb, 100 * mb, EFI, ""),
                part(0, 2, 101 * mb, 99 * GB, BASIC, "C"),
                part(0, 3, 101 * mb + 99 * GB, 700 * mb, RECOVERY, ""),
            ],
            volumes: vec![vol(0, 2, "C", 99 * GB, 4 * GB)],
            physical: vec![],
            vds_status: "Stopped".into(),
            vds_start: "Manual".into(),
        }
    }

    #[test]
    fn kinds_from_gpt_and_mbr() {
        let mut p = part(0, 1, 0, 1, RECOVERY, "");
        assert_eq!(p.kind(), PartKind::Recovery);
        p.gpt_type = "{0FC63DAF-8483-4772-8E79-3D69D8477DE4}".into();
        assert_eq!(p.kind(), PartKind::Linux);
        p.gpt_type.clear();
        p.mbr_type = Some(0x27);
        assert_eq!(p.kind(), PartKind::Recovery);
        p.mbr_type = Some(7);
        assert_eq!(p.kind(), PartKind::Basic);
    }

    #[test]
    fn free_space_ignores_small_gaps() {
        let l = grown_vm_disk();
        let free = l.free_space(&l.disks[0]);
        assert_eq!(free.len(), 1, "{free:?}");
        assert!(free[0].1 > 49 * GB);
        let v = &l.views()[0];
        assert_eq!(v.segments.len(), 4);
        assert!(matches!(v.segments[3], Segment::Free { .. }));
        match &v.segments[1] {
            Segment::Partition { volume, .. } => assert_eq!(volume.as_ref().unwrap().letter, "C"),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn parses_powershell_shape() {
        let json = r#"{"disks":[{"number":0,"name":"Samsung SSD","size":500107862016,"style":"GPT","offline":false,"offline_reason":"","read_only":false,"boot":true,"system":true,"bus":"NVMe","health":"Healthy","status":"Online"}],
          "partitions":[{"disk":0,"number":3,"offset":122683392,"size":388951441408,"letter":"C","type":"Basic","gpt_type":"{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}","mbr_type":null,"system":false,"boot":true,"hidden":false,"read_only":false,"offline":false,"paths":["C:\\","\\\\?\\Volume{52be}\\"]}],
          "volumes":[{"path":"\\\\?\\Volume{52be}\\","letter":"C","label":"","fs":"NTFS","size":388951437312,"free":18735271936,"health":"Healthy"}],
          "physical":[],"vds_status":"Stopped","vds_start":"Manual"}"#;
        let l: Layout = serde_json::from_str(json).unwrap();
        assert_eq!(l.volume_of(&l.partitions[0]).unwrap().letter, "C");
    }

    /// Reads this PC's real layout (no admin needed).
    #[cfg(windows)]
    #[test]
    fn reads_this_pc() {
        let l = read().unwrap();
        assert!(!l.disks.is_empty());
        let sys = l
            .partitions
            .iter()
            .find(|p| p.boot)
            .expect("boot partition");
        assert_eq!(sys.kind(), PartKind::Basic);
        assert!(l.volume_of(sys).is_some());
    }
}
