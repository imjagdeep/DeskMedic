import logo from "../assets/logo.png";
import { useHealthJob, useInsightsJob } from "../lib/jobs";
import { useAppInfo } from "../lib/store";
import { SideNav, type NavGroup } from "../shell/SideNav";
import { BroomIcon, DiskIcon, GlobeIcon, LogIcon, MapIcon, OverviewIcon, PulseIcon, SettingsIcon, SparkleIcon, WrenchIcon } from "./Icons";

export type Page = "overview" | "assistant" | "health" | "network" | "diskmap" | "cleanup" | "disks" | "remedies" | "log" | "settings";

/** The pages in sidebar order (the Tools menu and Ctrl+1… use it too). */
export const PAGES: { page: Page; label: string; group: string }[] = [
  { page: "overview", label: "Overview", group: "This PC" },
  { page: "assistant", label: "Assistant", group: "This PC" },
  { page: "health", label: "Health check", group: "This PC" },
  { page: "network", label: "Network doctor", group: "This PC" },
  { page: "diskmap", label: "Disk map", group: "This PC" },
  { page: "cleanup", label: "Cleanup", group: "This PC" },
  { page: "disks", label: "Disks", group: "This PC" },
  { page: "remedies", label: "Remedies", group: "This PC" },
  { page: "log", label: "Log", group: "App" },
  { page: "settings", label: "Settings", group: "App" },
];

const ICONS: Record<Page, (p: { size?: number }) => React.ReactElement> = {
  overview: OverviewIcon,
  assistant: SparkleIcon,
  health: PulseIcon,
  network: GlobeIcon,
  diskmap: MapIcon,
  cleanup: BroomIcon,
  disks: DiskIcon,
  remedies: WrenchIcon,
  log: LogIcon,
  settings: SettingsIcon,
};

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const info = useAppInfo();
  const { report } = useHealthJob();
  // The last health result (this session or the assistant's), as a dot.
  const tone = !report ? null : report.problems ? "problem" : report.warnings ? "warning" : "good";
  // The Assistant's last read of the event logs, the same way.
  const events = useInsightsJob().report;
  const eventTone = !events ? null : events.problems ? "problem" : events.warnings ? "warning" : "good";
  const groups: NavGroup<Page>[] = [];
  for (const p of PAGES) {
    let g = groups.find((x) => x.title === p.group);
    if (!g) groups.push((g = { title: p.group, items: [] }));
    const dot = p.page === "health" ? tone : p.page === "assistant" ? eventTone : null;
    const badge = dot ? <span className={`nav-dot ${dot}`} aria-label={`${p.label}: ${dot}`} /> : undefined;
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
