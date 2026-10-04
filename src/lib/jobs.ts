// Long-running jobs (scan, cleanup, fix-it) live here instead of in the
// pages, so switching pages mid-job doesn't lose the progress or the result.

import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { api } from "./api";
import { createStore, toastError } from "./store";
import type { CleanupReport, FixResult, ScanProgress, ScanSummary } from "./types";

export interface ScanJob {
  root: string;
  /** Used space on the drive, for a percentage. */
  used: number;
  files: number;
  bytes: number;
  started: number;
}

const scanStore = createStore<ScanJob | null>(null);
/** Bumped every time a scan finishes, so the Disk map reloads its results. */
const scanDoneStore = createStore<{ summary: ScanSummary | null; seq: number }>({ summary: null, seq: 0 });
const cleanupStore = createStore<{ running: boolean; report: CleanupReport | null }>({ running: false, report: null });
const fixStore = createStore<{ running: string | null; result: FixResult | null }>({ running: null, result: null });

export const useScanJob = () => useSyncExternalStore(scanStore.subscribe, scanStore.get);
export const useScanDone = () => useSyncExternalStore(scanDoneStore.subscribe, scanDoneStore.get);
export const useCleanupJob = () => useSyncExternalStore(cleanupStore.subscribe, cleanupStore.get);
export const useFixJob = () => useSyncExternalStore(fixStore.subscribe, fixStore.get);

let started = false;
/** Follow backend job events for the whole session. Call once at start. */
export function startJobs() {
  if (started) return;
  started = true;
  void listen<ScanProgress>("scan-progress", (e) => {
    const cur = scanStore.get();
    if (cur) scanStore.set({ ...cur, ...e.payload });
  });
  void listen<ScanSummary>("scan-done", (e) => {
    scanStore.set(null);
    scanDoneStore.set({ summary: e.payload, seq: scanDoneStore.get().seq + 1 });
  });
  void listen<string>("scan-failed", (e) => {
    scanStore.set(null);
    toastError(e.payload);
  });
  // A scan started before the window reloaded.
  api
    .scanState()
    .then((s) => s && !scanStore.get() && scanStore.set({ ...s, started: Date.now() }))
    .catch(toastError);
}

export async function startScan(root: string, used: number) {
  scanStore.set({ root, used, files: 0, bytes: 0, started: Date.now() });
  try {
    await api.scanStart(root);
  } catch (e) {
    scanStore.set(null);
    toastError(e);
  }
}

export async function runCleanup(ids: string[], allUsers: boolean) {
  cleanupStore.set({ running: true, report: null });
  try {
    cleanupStore.set({ running: false, report: await api.cleanupRun(ids, allUsers) });
  } catch (e) {
    cleanupStore.set({ running: false, report: null });
    toastError(e);
  }
}

export const clearCleanupReport = () => cleanupStore.set({ running: false, report: null });

export async function runFix(id: string) {
  fixStore.set({ running: id, result: null });
  try {
    fixStore.set({ running: null, result: await api.fixRun(id) });
  } catch (e) {
    fixStore.set({ running: null, result: null });
    toastError(e);
  }
}

export const clearFixResult = () => fixStore.set({ running: fixStore.get().running, result: null });
