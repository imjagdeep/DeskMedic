// Typed wrappers over the Rust commands. Errors arrive as plain strings.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  CleanupEstimate,
  CleanupReport,
  BigFile,
  Drive,
  Listing,
  LogEntry,
  Profile,
  ScanSummary,
  Settings,
  TypeTotal,
} from "./types";

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  openDataFolder: () => invoke<void>("open_data_folder"),
  listDrives: () => invoke<Drive[]>("list_drives"),
  readLog: () => invoke<LogEntry[]>("read_log"),
  exportLog: (path: string) => invoke<number>("export_log", { path }),
  scanStart: (root: string) => invoke<void>("scan_start", { root }),
  scanCancel: () => invoke<void>("scan_cancel"),
  scanSummary: () => invoke<ScanSummary | null>("scan_summary"),
  scanListing: (id: number | null, limit: number) => invoke<Listing>("scan_listing", { id, limit }),
  scanTopFiles: (n: number) => invoke<BigFile[]>("scan_top_files", { n }),
  scanTypes: (n: number) => invoke<TypeTotal[]>("scan_types", { n }),
  revealNode: (id: number) => invoke<void>("reveal_node", { id }),
  userProfiles: () => invoke<Profile[]>("user_profiles"),
  cleanupPreview: (allUsers: boolean) => invoke<CleanupEstimate[]>("cleanup_preview", { allUsers }),
  cleanupRun: (ids: string[], allUsers: boolean) => invoke<CleanupReport>("cleanup_run", { ids, allUsers }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
