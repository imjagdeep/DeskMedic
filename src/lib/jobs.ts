// Long-running jobs (scan, cleanup, remedy, health check, network doctor) live here instead of in the
// pages, so switching pages mid-job doesn't lose the progress or the result.

import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { api, errorText } from "./api";
import { createStore, toast, toastError } from "./store";
import type { CleanupReport, FixResult, HealthReport, InsightsReport, NetReport, ScanProgress, ScanSummary } from "./types";

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

const healthStore = createStore<{ running: boolean; report: HealthReport | null; at: number | null }>({
  running: false,
  report: null,
  at: null,
});

const insightsStore = createStore<{ running: boolean; report: InsightsReport | null; at: number | null }>({
  running: false,
  report: null,
  at: null,
});
const networkStore = createStore<{ running: boolean; report: NetReport | null; at: number | null }>({
  running: false,
  report: null,
  at: null,
});

export const useScanJob = () => useSyncExternalStore(scanStore.subscribe, scanStore.get);
export const useScanDone = () => useSyncExternalStore(scanDoneStore.subscribe, scanDoneStore.get);
export const useCleanupJob = () => useSyncExternalStore(cleanupStore.subscribe, cleanupStore.get);
export const useFixJob = () => useSyncExternalStore(fixStore.subscribe, fixStore.get);
export const useHealthJob = () => useSyncExternalStore(healthStore.subscribe, healthStore.get);
export const useNetworkJob = () => useSyncExternalStore(networkStore.subscribe, networkStore.get);
export const useInsightsJob = () => useSyncExternalStore(insightsStore.subscribe, insightsStore.get);

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
  // Background checks by the Medic assistant (and checks run from the tray).
  void listen<HealthReport>("health-updated", (e) => {
    healthStore.set({ running: healthStore.get().running, report: e.payload, at: Date.now() });
  });
  // The Medic assistant's background read of the event logs.
  void listen<InsightsReport>("insights-updated", (e) => {
    insightsStore.set({ running: insightsStore.get().running, report: e.payload, at: Date.now() });
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

// ---------- one-click fixes: a queue, one at a time (the backend refuses two at once) ----------

export type FixState = "queued" | "running" | "done" | "failed";

export interface FixRun {
  /** Unique per run, so the same fix can appear twice in the history. */
  seq: number;
  id: string;
  name: string;
  restorePoint: boolean;
  state: FixState;
  queuedAt: number;
  startedAt?: number;
  endedAt?: number;
  result?: FixResult;
  /** Why it couldn't run at all (the backend refused or crashed). */
  error?: string;
}

/** This session's runs, oldest first. */
const runsStore = createStore<FixRun[]>([]);
/** The run whose details are open, by seq. */
const detailsStore = createStore<number | null>(null);
export const useFixRuns = () => useSyncExternalStore(runsStore.subscribe, runsStore.get);
export const useFixDetails = () => useSyncExternalStore(detailsStore.subscribe, detailsStore.get);
export const showFixDetails = (seq: number | null) => detailsStore.set(seq);

let fixSeq = 0;
const patch = (seq: number, p: Partial<FixRun>) => runsStore.set(runsStore.get().map((r) => (r.seq === seq ? { ...r, ...p } : r)));

/** The latest run of a fix, for its card ("Running…", "Done 10:42", "Failed"). */
export function latestRun(runs: FixRun[], id: string): FixRun | undefined {
  for (let i = runs.length - 1; i >= 0; i--) if (runs[i].id === id) return runs[i];
  return undefined;
}

/**
 * Add a fix to the queue. Returns "running" if it starts now, "queued" if it waits
 * behind another, or "already" if that fix is already running or waiting.
 */
export function queueFix(id: string, name: string, restorePoint: boolean): "running" | "queued" | "already" {
  const runs = runsStore.get();
  if (runs.some((r) => r.id === id && (r.state === "queued" || r.state === "running"))) return "already";
  const busy = runs.some((r) => r.state === "running" || r.state === "queued");
  runsStore.set([...runs, { seq: ++fixSeq, id, name, restorePoint, state: "queued", queuedAt: Date.now() }]);
  void pumpFixes();
  return busy ? "queued" : "running";
}

/** Take a waiting fix out of the queue. */
export function cancelFix(seq: number) {
  runsStore.set(runsStore.get().filter((r) => !(r.seq === seq && r.state === "queued")));
}

/** Forget finished runs (their details stay in the Log). */
export function clearFinishedFixes() {
  runsStore.set(runsStore.get().filter((r) => r.state === "queued" || r.state === "running"));
  detailsStore.set(null);
}

let pumping = false;
async function pumpFixes() {
  if (pumping) return;
  pumping = true;
  try {
    for (;;) {
      const next = runsStore.get().find((r) => r.state === "queued");
      if (!next) break;
      patch(next.seq, { state: "running", startedAt: Date.now() });
      fixStore.set({ running: next.id, result: null });
      try {
        const result = await api.fixRun(next.id, next.restorePoint);
        patch(next.seq, { state: result.ok ? "done" : "failed", endedAt: Date.now(), result });
        toast(result.ok ? `${next.name}: done${result.restart ? ". Restart the PC to finish." : "."}` : `${next.name} failed. Open it in Fixes to see why.`, !result.ok);
      } catch (e) {
        patch(next.seq, { state: "failed", endedAt: Date.now(), error: errorText(e) });
        toast(`${next.name} couldn't run: ${errorText(e)}`, true);
      }
      fixStore.set({ running: null, result: null });
    }
  } finally {
    pumping = false;
  }
}

export async function runHealthCheck() {
  if (healthStore.get().running) return;
  healthStore.set({ ...healthStore.get(), running: true });
  try {
    healthStore.set({ running: false, report: await api.healthRun(), at: Date.now() });
  } catch (e) {
    healthStore.set({ ...healthStore.get(), running: false });
    toastError(e);
  }
}

export async function runNetworkDoctor() {
  if (networkStore.get().running) return;
  networkStore.set({ ...networkStore.get(), running: true });
  try {
    networkStore.set({ running: false, report: await api.networkRun(), at: Date.now() });
  } catch (e) {
    networkStore.set({ ...networkStore.get(), running: false });
    toastError(e);
  }
}

export async function runInsights() {
  if (insightsStore.get().running) return;
  insightsStore.set({ ...insightsStore.get(), running: true });
  try {
    insightsStore.set({ running: false, report: await api.insightsRun(), at: Date.now() });
  } catch (e) {
    insightsStore.set({ ...insightsStore.get(), running: false });
    toastError(e);
  }
}

/** Checkup: the health check (now) and the event logs (30 days), side by side. */
export async function runCheckup() {
  await Promise.all([runHealthCheck(), runInsights()]);
}
