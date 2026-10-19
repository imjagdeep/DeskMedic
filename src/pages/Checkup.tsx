import { useEffect, useState } from "react";
import { AdminNotice, PageHeader, UsageBar } from "../components/Common";
import { HealthCard, InsightCard, LogExtras } from "../components/Findings";
import { DiskIcon, PulseIcon } from "../components/Icons";
import { SendToIt } from "../components/SendToIt";
import type { Dest } from "../components/Sidebar";
import { api } from "../lib/api";
import { humanSize, percent } from "../lib/format";
import { runCheckup, useHealthJob, useInsightsJob } from "../lib/jobs";
import { toastError, useAppInfo } from "../lib/store";
import type { AssistantStatus, Drive, DriveKind, HealthAction, HealthCheck, Insight, PcSummary } from "../lib/types";

const KIND: Record<DriveKind, string> = {
  fixed: "Local disk",
  removable: "Removable",
  network: "Network drive",
  optical: "CD/DVD",
  ram_disk: "RAM disk",
  unknown: "Drive",
};

function days(d: number): string {
  if (d < 1) return "today";
  const n = Math.round(d);
  return `${n} day${n === 1 ? "" : "s"} ago`;
}

function timeOf(t: string | number): string {
  return new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** One list for both checks: what's wrong now and what went wrong lately. */
type Item = { key: string; problem: boolean; score: number } & ({ kind: "now"; c: HealthCheck } | { kind: "logs"; x: Insight });

// A problem right now outranks the same level found in the logs, except the
// worst log findings (blue screens, failing drives), which score higher.
const NOW_PROBLEM = 100;
const NOW_WARNING = 30;

/** Run once per session, the first time Checkup opens. */
let checkedThisSession = false;

/** Checkup: the home screen. How this PC is now, what Windows logged lately, and the fix for each. */
export function Checkup({ onAction, onNavigate }: { onAction: (a: HealthAction) => void; onNavigate: (d: Dest) => void }) {
  const info = useAppInfo();
  const health = useHealthJob();
  const logs = useInsightsJob();
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
  useEffect(() => {
    load();
    // Opening DeskMedic should answer "what's wrong?" straight away.
    if (!checkedThisSession && !health.report && !logs.report) {
      checkedThisSession = true;
      void runCheckup();
    }
  }, []);

  const running = health.running || logs.running;
  const items: Item[] = [
    ...(health.report?.checks ?? [])
      .filter((c) => c.status === "problem" || c.status === "warning")
      .map((c): Item => ({ key: `now:${c.id}`, kind: "now", c, problem: c.status === "problem", score: c.status === "problem" ? NOW_PROBLEM : NOW_WARNING })),
    ...(logs.report?.findings ?? []).map((x): Item => ({ key: `logs:${x.key}`, kind: "logs", x, problem: x.level === "problem", score: x.score })),
  ].sort((a, b) => Number(b.problem) - Number(a.problem) || b.score - a.score);
  const fine = (health.report?.checks ?? []).filter((c) => c.status === "good" || c.status === "unknown");

  // The freshest result: this session's, else the background assistant's last.
  const thisSession = health.report !== null || logs.report !== null;
  const problems = thisSession ? items.filter((i) => i.problem).length : (assistant?.last_problems ?? 0);
  const warnings = thisSession ? items.length - problems : (assistant?.last_warnings ?? 0);
  const checkedAt = health.at ?? logs.at ?? assistant?.last_check ?? null;
  const known = thisSession || assistant?.last_check != null;
  const tone = !known ? "unknown" : problems ? "problem" : warnings ? "warning" : "good";
  const s = info?.settings;
  const every = s ? (s.check_every_min >= 60 ? `${s.check_every_min / 60} h` : `${s.check_every_min} min`) : "";
  const count = problems + warnings;

  return (
    <div className="page">
      <PageHeader title={info?.computer || "This PC"} subtitle={pc?.model ? `${pc.maker ?? ""} ${pc.model}`.trim() : undefined}>
        <SendToIt />
        <button
          className="primary"
          disabled={running}
          onClick={() => {
            load();
            void runCheckup();
          }}
        >
          {running ? "Checking…" : "Check this PC"}
        </button>
      </PageHeader>
      <AdminNotice need="Fast scans, cleaning other users' files, disk tools and most fixes need administrator rights." />

      <section className={`card home-health checkup-summary ${tone}`}>
        <div className="home-health-head">
          <span className={`home-ring ${tone}`} aria-hidden="true">
            <PulseIcon size={22} />
          </span>
          <div>
            <h2>
              {running && !thisSession
                ? "Checking this PC…"
                : !known
                  ? "Not checked yet"
                  : count === 0
                    ? "All good. Nothing needs attention."
                    : `${count} thing${count === 1 ? "" : "s"} need${count === 1 ? "s" : ""} attention`}
            </h2>
            <div className="muted small">
              {running
                ? "Looking at this PC now and reading what Windows logged over the last 30 days."
                : checkedAt
                  ? `Checked at ${timeOf(checkedAt)}: this PC right now, and Windows' logs for the last 30 days. Read-only; nothing leaves this PC.`
                  : "Check this PC to see how it's doing."}
            </div>
          </div>
        </div>
        {assistant?.last_temp_bytes != null && s?.temp_reminder && (
          <div className={assistant.last_temp_bytes >= s.temp_reminder_gb * 1e9 ? "home-temp over" : "home-temp"}>
            <span>
              Temporary files: <strong>{humanSize(assistant.last_temp_bytes)}</strong>
              <span className="muted"> · reminder at {s.temp_reminder_gb} GB</span>
            </span>
            <button className="link" onClick={() => onNavigate("cleanup")}>
              Clean up
            </button>
          </div>
        )}
        <div className="home-assistant small">
          <span className={s?.assistant ? "dot on" : "dot paused"} />
          <span>
            {s?.assistant
              ? `Medic assistant is watching: checks every ${every} and pops up only when something new goes wrong.`
              : "Medic assistant is off."}
          </span>
          <button className="link" onClick={() => onNavigate("settings")}>
            Change
          </button>
        </div>
      </section>

      {running && !thisSession && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>Checking this PC and reading Windows' event logs…</span>
        </div>
      )}

      {items.length > 0 && (
        <>
          <h3>Needs attention</h3>
          <div className="insight-list">
            {items.map((i) =>
              i.kind === "now" ? (
                <HealthCard key={i.key} c={i.c} onAction={onAction} />
              ) : (
                <InsightCard key={i.key} x={i.x} onAction={onAction} onNavigate={onNavigate} />
              ),
            )}
          </div>
        </>
      )}

      {thisSession && (
        <>
          {fine.length > 0 && (
            <details className="insight-more">
              <summary>{fine.length} things checked and fine right now</summary>
              <div className="checkup-fine">
                {fine.map((c) => (
                  <div key={c.id} className="checkup-fine-row">
                    <span className={`health-dot ${c.status}`} aria-hidden="true" />
                    <strong>{c.title}</strong>
                    <span className="muted small">{c.summary}</span>
                  </div>
                ))}
              </div>
            </details>
          )}
          {logs.report && <LogExtras report={logs.report} onAction={onAction} onNavigate={onNavigate} />}
        </>
      )}

      <h3>This PC</h3>
      <section className="card home-pc">
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

      <h3>Drives</h3>
      {drives === null && <div className="empty">Reading drives…</div>}
      {drives?.length === 0 && <div className="empty">No drives found.</div>}
      <div className="home-drives">
        {drives?.map((d) => {
          const used = d.total_bytes - d.free_bytes;
          const low = d.total_bytes > 0 && d.free_bytes / d.total_bytes < 0.1;
          return (
            <button className="card drive drive-link" key={d.root} onClick={() => onNavigate("diskmap")} title="See what fills this drive">
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
            </button>
          );
        })}
      </div>
    </div>
  );
}
