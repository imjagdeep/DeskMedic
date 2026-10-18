// Typed wrappers over the Rust commands. Errors arrive as plain strings.

import { invoke } from "@tauri-apps/api/core";
import type {
  RemedyLibrary,
  AppInfo,
  CleanupEstimate,
  CleanupReport,
  DiskAction,
  DiskReport,
  ExtendPlan,
  FixRecipe,
  FixResult,
  HealthReport,
  NetReport,
  PcSummary,
  AssistantStatus,
  BigFile,
  Drive,
  Listing,
  LogEntry,
  Profile,
  ScanSummary,
  Settings,
  TypeTotal,
} from "./types";

export type LabToolId = "deskzero" | "deskmedic" | "deskfolio";
export interface LabTool {
  id: LabToolId;
  installed: boolean;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  openDataFolder: () => invoke<void>("open_data_folder"),
  openLink: (id: "site" | "repo" | "issues" | "releases" | "developer" | LabToolId) => invoke<void>("open_link", { id }),
  /** The other DeskLab tools and whether each is installed here. */
  desklabTools: () => invoke<LabTool[]>("desklab_tools"),
  /** Start an installed DeskLab tool, optionally with a file for it to open. */
  openTool: (id: LabToolId, file?: string) => invoke<void>("open_tool", { id, file: file ?? null }),
  listDrives: () => invoke<Drive[]>("list_drives"),
  readLog: () => invoke<LogEntry[]>("read_log"),
  exportLog: (path: string) => invoke<number>("export_log", { path }),
  scanStart: (root: string) => invoke<void>("scan_start", { root }),
  scanCancel: () => invoke<void>("scan_cancel"),
  scanState: () =>
    invoke<{ root: string; used: number; files: number; bytes: number } | null>("scan_state"),
  scanSummary: () => invoke<ScanSummary | null>("scan_summary"),
  scanListing: (id: number | null, limit: number) => invoke<Listing>("scan_listing", { id, limit }),
  scanTopFiles: (n: number) => invoke<BigFile[]>("scan_top_files", { n }),
  scanTypes: (n: number) => invoke<TypeTotal[]>("scan_types", { n }),
  revealNode: (id: number) => invoke<void>("reveal_node", { id }),
  userProfiles: () => invoke<Profile[]>("user_profiles"),
  cleanupPreview: (allUsers: boolean) => invoke<CleanupEstimate[]>("cleanup_preview", { allUsers }),
  cleanupRun: (ids: string[], allUsers: boolean) => invoke<CleanupReport>("cleanup_run", { ids, allUsers }),
  diskReport: () => invoke<DiskReport>("disk_report"),
  diskAction: (action: DiskAction, confirm: string) => invoke<string>("disk_action", { action, confirm }),
  extendPlan: (letter: string) => invoke<ExtendPlan>("extend_plan", { letter }),
  extendRun: (letter: string, expectedSteps: number, confirm: string) =>
    invoke<string[]>("extend_run", { letter, expectedSteps, confirm }),
  resizeInfo: (disk: number, partition: number) => invoke<[number, number]>("resize_info", { disk, partition }),
  resize: (disk: number, partition: number, size: number, confirm: string) =>
    invoke<string>("resize", { disk, partition, size, confirm }),
  fixList: () => invoke<FixRecipe[]>("fix_list"),
  remedyLibrary: () => invoke<RemedyLibrary>("remedy_library"),
  remedyOpen: (id: string, step: number) => invoke<void>("remedy_open", { id, step }),
  fixRun: (id: string, restorePoint: boolean) => invoke<FixResult>("fix_run", { id, restorePoint }),
  healthRun: () => invoke<HealthReport>("health_run"),
  networkRun: () => invoke<NetReport>("network_run"),
  sendToIt: (path: string) => invoke<string[]>("send_to_it", { path }),
  pcSummary: () => invoke<PcSummary>("pc_summary"),
  assistantStatus: () => invoke<AssistantStatus>("assistant_status"),
  notifyUpdate: (version: string) => invoke<void>("notify_update", { version }),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
