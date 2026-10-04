// Typed wrappers over the Rust commands. Errors arrive as plain strings.

import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Drive, LogEntry, Settings } from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  openDataFolder: () => invoke<void>("open_data_folder"),
  listDrives: () => invoke<Drive[]>("list_drives"),
  readLog: () => invoke<LogEntry[]>("read_log"),
  exportLog: (path: string) => invoke<number>("export_log", { path }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
