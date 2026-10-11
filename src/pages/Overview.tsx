import { useEffect, useState } from "react";
import { AdminNotice, PageHeader, UsageBar } from "../components/Common";
import { BroomIcon, DiskIcon, GlobeIcon, MapIcon, PulseIcon, WrenchIcon } from "../components/Icons";
import { SendToIt } from "../components/SendToIt";
import type { Page } from "../components/Sidebar";
import { api } from "../lib/api";
import { humanSize, percent } from "../lib/format";
import { runHealthCheck, useHealthJob } from "../lib/jobs";
import { toastError, useAppInfo } from "../lib/store";
import type { AssistantStatus, Drive, DriveKind, PcSummary } from "../lib/types";

const KIND: Record<DriveKind, string> = {
  fixed: "Local disk",
  removable: "Removable",
  network: "Network drive",
  optical: "CD/DVD",
  ram_disk: "RAM disk",
  unknown: "Drive",
};

type Tile = { page: Page; title: string; text: string; Icon: (p: { size?: number }) => React.ReactElement };
const TILES: Tile[] = [
  { page: "health", title: "Health check", text: "What usually sits behind a slow or odd PC", Icon: PulseIcon },
  { page: "network", title: "Network doctor", text: "Find where the internet breaks", Icon: GlobeIcon },
  { page: "cleanup", title: "Cleanup", text: "Clear temp files and caches safely", Icon: BroomIcon },
  { page: "diskmap", title: "Disk map", text: "See what fills a drive", Icon: MapIcon },
  { page: "remedies", title: "Remedies", text: "One-click fixes for common problems", Icon: WrenchIcon },
  { page: "disks", title: "Disks", text: "Missing drives, full C:, recovery partition", Icon: DiskIcon },
];

function days(d: number): string {
  if (d < 1) return "today";
  const n = Math.round(d);
  return `${n} day${n === 1 ? "" : "s"} ago`;
}

function timeOf(t: string | number): string {
  return new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function Overview({ onNavigate }: { onNavigate: (p: Page) => void }) {
  const info = useAppInfo();
  const health = useHealthJob();
  const [drives, setDrives] = useState<Drive[] | null>(null);
  const [pc, setPc] = useState<PcSummary | null>(null);
  const [assistant, setAssistant] = useState<AssistantStatus | null>(null);

  const load = () => {
    setDrives(null);
    api.listDrives().then(setDrives).catch((e) => {
      setDrives([]);
      toastError(e);
    });
    api.pcSummary().then(setPc).catch(toastError);
    api.assistantStatus().then(setAssistant).catch(toastError);
  };
  useEffect(load, []);

  // The freshest result: one from this session, else the assistant's last.
  const report = health.report;
  const problems = report ? report.problems : (assistant?.last_problems ?? 0);
  const warnings = report ? report.warnings : (assistant?.last_warnings ?? 0);
  const checkedAt = health.at ?? assistant?.last_check ?? null;
  const known = report !== null || assistant?.last_check != null;
  const tone = !known ? "unknown" : problems ? "problem" : warnings ? "warning" : "good";
  const s = info?.settings;
  const every = s ? (s.check_every_min >= 60 ? `${s.check_every_min / 60} h` : `${s.check_every_min} min`) : "";

  return (
    <div className="page">
      <PageHeader title={info?.computer || "This PC"} subtitle={pc?.model ? `${pc.maker ?? ""} ${pc.model}`.trim() : undefined}>
        <SendToIt />
        <button onClick={load}>Refresh</button>
      </PageHeader>
      <AdminNotice need="Fast scans, cleaning other users' files, disk tools and most remedies need administrator rights." />

      <div className="home-top">
        <section className={`card home-health ${tone}`}>
          <div className="home-health-head">
            <span className={`home-ring ${tone}`} aria-hidden="true">
              <PulseIcon size={22} />
            </span>
            <div>
              <h2>
                {!known
                  ? "Not checked yet"
                  : tone === "good"
                    ? "All good"
                    : [problems && `${problems} problem${problems === 1 ? "" : "s"}`, warnings && `${warnings} warning${warnings === 1 ? "" : "s"}`]
                        .filter(Boolean)
                        .join(", ")}
              </h2>
              <div className="muted small">
                {checkedAt ? `Checked at ${timeOf(checkedAt)}` : "Run a health check to see how this PC is doing."}
              </div>
            </div>
          </div>
          <div className="home-health-actions">
            <button className="primary" disabled={health.running} onClick={() => void runHealthCheck().then(() => onNavigate("health"))}>
              {health.running ? "Checking…" : "Run health check"}
            </button>
            {known && <button onClick={() => onNavigate("health")}>Details</button>}
          </div>
          <div className="home-assistant small">
            <span className={s?.assistant ? "dot on" : "dot paused"} />
            <span>
              {s?.assistant
                ? `Medic assistant is watching: checks every ${every} and pops up if something looks wrong.`
                : "Medic assistant is off."}
            </span>
            <button className="link" onClick={() => onNavigate("settings")}>
              Change
            </button>
          </div>
        </section>

        <section className="card home-pc">
          <h2>This PC</h2>
          {!pc && <div className="muted small">Reading…</div>}
          {pc && (
            <dl className="facts">
              {pc.windows && (
                <>
                  <dt>Windows</dt>
                  <dd>
                    {pc.windows} {pc.version}
                    {pc.build && <span className="muted"> · build {pc.build}</span>}
                  </dd>
                </>
              )}
              {pc.cpu && (
                <>
                  <dt>Processor</dt>
                  <dd title={pc.cpu}>
                    {pc.cpu}
                    {pc.cores ? <span className="muted"> · {pc.cores} cores</span> : null}
                  </dd>
                </>
              )}
              {pc.memory_gb != null && (
                <>
                  <dt>Memory</dt>
                  <dd>{pc.memory_gb} GB</dd>
                </>
              )}
              {pc.uptime_days != null && (
                <>
                  <dt>Last restart</dt>
                  <dd className={pc.uptime_days >= 14 ? "warn-text" : undefined}>{days(pc.uptime_days)}</dd>
                </>
              )}
              {pc.network && (
                <>
                  <dt>Network</dt>
                  <dd>
                    {pc.network}
                    {pc.ip && <span className="muted"> · {pc.ip}</span>}
                  </dd>
                </>
              )}
              <dt>Signed in</dt>
              <dd>
                {info?.user}
                {pc.domain && <span className="muted"> · {pc.domain}</span>}
                {info?.elevated && <span className="badge">admin</span>}
              </dd>
            </dl>
          )}
        </section>
      </div>

      <h3>Tools</h3>
      <div className="home-tiles">
        {TILES.map(({ page, title, text, Icon }) => (
          <button key={page} className="home-tile" onClick={() => onNavigate(page)}>
            <span className="home-tile-icon">
              <Icon size={20} />
            </span>
            <span className="home-tile-text">
              <strong>{title}</strong>
              <span className="muted small">{text}</span>
            </span>
          </button>
        ))}
      </div>

      <h3>Drives</h3>
      {drives === null && <div className="empty">Reading drives…</div>}
      {drives?.length === 0 && <div className="empty">No drives found.</div>}
      <div className="home-drives">
        {drives?.map((d) => {
          const used = d.total_bytes - d.free_bytes;
          const low = d.total_bytes > 0 && d.free_bytes / d.total_bytes < 0.1;
          return (
            <div className="card drive" key={d.root}>
              <div className="drive-icon">
                <DiskIcon size={22} />
              </div>
              <div className="grow">
                <div className="card-head">
                  <h2>
                    {d.label || KIND[d.kind]} ({d.root.replace(/\\$/, "")})
                    {d.is_system && <span className="badge">Windows</span>}
                  </h2>
                  <span className="muted small">
                    {KIND[d.kind]}
                    {d.file_system && ` · ${d.file_system}`}
                  </span>
                </div>
                <UsageBar used={used} total={d.total_bytes} />
                <div className="drive-numbers small">
                  <span className={low ? "warn-text" : undefined}>
                    {humanSize(d.free_bytes)} free of {humanSize(d.total_bytes)}
                  </span>
                  <span className="muted">{percent(used, d.total_bytes)}% used</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
