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
