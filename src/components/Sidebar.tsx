import logo from "../assets/logo.png";
import { useFixRuns, useHealthJob, useInsightsJob } from "../lib/jobs";
import { useAppInfo } from "../lib/store";
import { SideNav, type NavGroup } from "../shell/SideNav";
import { DiskIcon, GlobeIcon, LogIcon, PulseIcon, SettingsIcon, WrenchIcon } from "./Icons";

export type Page = "checkup" | "network" | "storage" | "remedies" | "log" | "settings";

/** The three parts of Storage. */
export type StorageTab = "diskmap" | "cleanup" | "disks";

/**
 * Anywhere a link can point: a page, a Storage tab, or a name from before Checkup
 * and Storage existed (remedy steps, the tray and menus still use those).
 */
export type Dest = Page | StorageTab | "health" | "overview" | "assistant";

/** Where a link really goes: the page, and the Storage tab if it's one of those. */
export function resolve(d: Dest): { page: Page; tab?: StorageTab } {
  switch (d) {
    case "health":
    case "overview":
    case "assistant":
      return { page: "checkup" };
    case "diskmap":
    case "cleanup":
    case "disks":
      return { page: "storage", tab: d };
    default:
      return { page: d };
  }
}

export const STORAGE_TABS: { tab: StorageTab; label: string }[] = [
  { tab: "diskmap", label: "Space" },
  { tab: "cleanup", label: "Clean up" },
  { tab: "disks", label: "Drives" },
];

/** The pages in sidebar order (the Tools menu and Ctrl+1… use it too). */
export const PAGES: { page: Page; label: string; group: string }[] = [
  { page: "checkup", label: "Checkup", group: "This PC" },
  { page: "network", label: "Network", group: "This PC" },
  { page: "storage", label: "Storage", group: "This PC" },
  { page: "remedies", label: "Fixes", group: "This PC" },
  { page: "log", label: "Log", group: "App" },
  { page: "settings", label: "Settings", group: "App" },
];

/** A name for a link's destination: "Checkup", "Storage › Clean up". */
export function destLabel(d: Dest): string {
  const { page, tab } = resolve(d);
  const name = PAGES.find((p) => p.page === page)?.label ?? page;
  const sub = tab && STORAGE_TABS.find((t) => t.tab === tab)?.label;
  return sub ? `${name} › ${sub}` : name;
}

const ICONS: Record<Page, (p: { size?: number }) => React.ReactElement> = {
  checkup: PulseIcon,
  network: GlobeIcon,
  storage: DiskIcon,
  remedies: WrenchIcon,
  log: LogIcon,
  settings: SettingsIcon,
};

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const info = useAppInfo();
  // The last checkup (health check and event logs), as one dot: the worse of the two.
  const health = useHealthJob().report;
  const events = useInsightsJob().report;
  const problems = (health?.problems ?? 0) + (events?.problems ?? 0);
  const warnings = (health?.warnings ?? 0) + (events?.warnings ?? 0);
  const tone = !health && !events ? null : problems ? "problem" : warnings ? "warning" : "good";
  // Fixes: a blue dot while one runs or waits, red if the last one to finish failed.
  const runs = useFixRuns();
  const lastDone = [...runs].reverse().find((r) => r.state === "done" || r.state === "failed");
  const fixTone = runs.some((r) => r.state === "running" || r.state === "queued") ? "busy" : lastDone?.state === "failed" ? "problem" : null;
  const groups: NavGroup<Page>[] = [];
  for (const p of PAGES) {
    let g = groups.find((x) => x.title === p.group);
    if (!g) groups.push((g = { title: p.group, items: [] }));
    const badge =
      p.page === "checkup" && tone ? (
        <span className={`nav-dot ${tone}`} aria-label={`Checkup: ${tone}`} />
      ) : p.page === "remedies" && fixTone ? (
        <span className={`nav-dot ${fixTone}`} aria-label={fixTone === "busy" ? "A fix is running" : "A fix failed"} />
      ) : undefined;
    g.items.push({ page: p.page, label: p.label, Icon: ICONS[p.page], badge });
  }
  const who = info ? (info.elevated ? "Administrator" : "Standard user") : "…";
  return (
    <SideNav
      appName="DeskMedic"
      logo={logo}
      groups={groups}
      page={page}
      onNavigate={onNavigate}
      footTitle={info ? `${who} · ${info.computer}` : undefined}
      foot={
        <>
          <span className={info?.elevated ? "dot on" : "dot paused"} />
          {who}
          {info && <span className="muted ellipsis"> · {info.computer}</span>}
        </>
      }
    />
  );
}
