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
