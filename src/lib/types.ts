// Mirrors of the Rust types sent over IPC.

export type Theme = "system" | "light" | "dark";

export interface Settings {
  theme: Theme;
  keep_log_days: number;
}

export interface AppInfo {
  version: string;
  elevated: boolean;
  user: string;
  computer: string;
  data_dir: string;
  settings: Settings;
}

export type DriveKind = "fixed" | "removable" | "network" | "optical" | "ram_disk" | "unknown";

export interface Drive {
  root: string;
  label: string;
  file_system: string;
  kind: DriveKind;
  total_bytes: number;
  free_bytes: number;
  is_system: boolean;
}

export type LogArea = "app" | "scan" | "cleanup" | "disk" | "fix";

export interface LogEntry {
  id: string;
  ts: string;
  area: LogArea;
  action: string;
  ok: boolean;
  summary: string;
  details: string[];
  user: string;
  computer: string;
  elevated: boolean;
}

export type ScanMethod = "mft" | "walk";

export interface ScanSummary {
  root: string;
  method: ScanMethod;
  files: number;
  folders: number;
  bytes: number;
  millis: number;
  skipped: number;
}

export interface ScanProgress {
  files: number;
  bytes: number;
}

export interface TreeItem {
  id: number;
  name: string;
  size: number;
  files: number;
  is_dir: boolean;
}

export interface Listing {
  id: number;
  path: string;
  trail: TreeItem[];
  size: number;
  files: number;
  children: TreeItem[];
  rest_count: number;
  rest_size: number;
}

export interface BigFile {
  id: number;
  path: string;
  size: number;
}

export interface TypeTotal {
  ext: string;
  size: number;
  files: number;
}

export interface Profile {
  path: string;
  sid: string;
  last_used: string;
  loaded: boolean;
  size: number | null;
}

export interface CleanupEstimate {
  id: string;
  name: string;
  description: string;
  recommended: boolean;
  bytes: number | null;
  files: number;
  blocked: string | null;
}

export interface CleanupOutcome {
  id: string;
  name: string;
  ok: boolean;
  freed: number | null;
  files: number;
  skipped: number;
  note: string;
  errors: string[];
}

export interface CleanupReport {
  outcomes: CleanupOutcome[];
  free_before: number;
  free_after: number;
}

export interface Disk {
  number: number;
  name: string;
  size: number;
  style: string;
  offline: boolean;
  offline_reason: string;
  read_only: boolean;
  boot: boolean;
  system: boolean;
  bus: string;
  health: string;
  status: string;
}

export interface Partition {
  disk: number;
  number: number;
  offset: number;
  size: number;
  letter: string;
  type: string;
  gpt_type: string;
  mbr_type: number | null;
  system: boolean;
  boot: boolean;
  hidden: boolean;
  read_only: boolean;
  offline: boolean;
  paths: string[];
}

export interface Volume {
  path: string;
  letter: string;
  label: string;
  fs: string;
  size: number;
  free: number;
  health: string;
}

export type PartKind = "efi" | "reserved" | "basic" | "recovery" | "linux" | "extended" | "other";

export type Segment =
  | { type: "partition"; partition: Partition; kind: PartKind; volume: Volume | null }
  | { type: "free"; offset: number; size: number };

export interface DiskView {
  disk: Disk;
  segments: Segment[];
  physical: { id: string; media: string; health: string; status: string } | null;
}

export type DiskAction =
  | { kind: "disk_online"; disk: number }
  | { kind: "disk_writable"; disk: number }
  | { kind: "partition_writable"; disk: number; partition: number }
  | { kind: "set_letter"; disk: number; partition: number; letter: string }
  | { kind: "check_volume"; letter: string }
  | { kind: "fix_volume"; letter: string }
  | { kind: "restart_vds" }
  | { kind: "enable_vds" };

export type Fix =
  | { type: "action"; action: DiskAction; label: string }
  | { type: "open_cleanup" }
  | { type: "extend_guide"; letter: string };

export interface Finding {
  severity: "problem" | "warning" | "info";
  title: string;
  detail: string;
  disk: number | null;
  fix: Fix | null;
}

export interface DiskReport {
  disks: DiskView[];
  findings: Finding[];
  letters_in_use: string[];
  windows_letter: string;
  vds_status: string;
  vds_start: string;
}

export type ExtendStep =
  | { step: "disable_win_re" }
  | { step: "delete_recovery"; disk: number; partition: number }
  | { step: "extend"; disk: number; partition: number; size: number }
  | { step: "create_recovery"; disk: number; offset: number; size: number; gpt: boolean }
  | { step: "enable_win_re" };

export interface ExtendPlan {
  letter: string;
  current_size: number;
  new_size: number;
  steps: ExtendStep[];
  blocked: string | null;
  moves_recovery: boolean;
}
