import { useEffect, useState } from "react";
import { PageHeader } from "../components/Common";
import { SendToIt } from "../components/SendToIt";
import { PAGES, type Page } from "../components/Sidebar";
import { api } from "../lib/api";
import { runInsights, useInsightsJob } from "../lib/jobs";
import { toastError } from "../lib/store";
import type { HealthAction, Insight, Remedy } from "../lib/types";

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

/** "2 hours ago", "yesterday", "3 weeks ago". */
function ago(iso: string): string {
  const mins = Math.round((new Date(iso).getTime() - Date.now()) / 60000);
  if (mins > -60) return rtf.format(Math.min(mins, -1), "minute");
  const hours = Math.round(mins / 60);
  if (hours > -24) return rtf.format(hours, "hour");
  const days = Math.round(hours / 24);
  return days > -14 ? rtf.format(days, "day") : rtf.format(Math.round(days / 7), "week");
}

const times = (n: number) => (n === 1 ? "once" : `${n} times`);

/** Guides from the remedies library, loaded once when a card asks for its steps. */
let guides: Promise<Remedy[]> | null = null;
const guide = (id: string) => {
  guides ??= api.remedyLibrary().then((l) => l.remedies);
  return guides.then((all) => all.find((r) => r.id === id) ?? null);
};

export function Assistant({ onAction, onNavigate }: { onAction: (a: HealthAction) => void; onNavigate: (p: Page) => void }) {
  // Kept outside the page so leaving it mid-read loses nothing.
  const { running, report, at } = useInsightsJob();

  useEffect(() => {
    // Read once when the page first opens; "Check again" reads afresh.
    if (!report && !running) void runInsights();
  }, []);

  const tone = !report ? null : report.problems ? "problem" : report.warnings ? "warning" : "good";

  return (
    <div className="page">
      <PageHeader title="Assistant">
        <SendToIt />
        <button className="primary" disabled={running} onClick={() => void runInsights()}>
          {running ? "Reading…" : "Check again"}
        </button>
      </PageHeader>
      <p className="muted">
        Reads what Windows itself recorded over the last 30 days (crashes, drive and hardware errors, failed updates,
        apps and services that keep crashing) and shows only what affects this PC, worst first, with a way to fix it.
        Read-only: nothing is changed, and nothing leaves this PC.
      </p>

      {running && !report && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>Reading Windows' event logs…</span>
        </div>
      )}

      {report && tone && (
        <>
          <div className={`health-summary ${tone}`}>
            <strong>
              {report.problems === 0 && report.warnings === 0
                ? `Nothing serious in the last ${report.days} days.`
                : [
                    report.problems && `${report.problems} problem${report.problems === 1 ? "" : "s"}`,
                    report.warnings && `${report.warnings} warning${report.warnings === 1 ? "" : "s"}`,
                  ]
                    .filter(Boolean)
                    .join(" and ") + ` from the last ${report.days} days.`}
            </strong>
            {at && <span className="muted small">Read at {new Date(at).toLocaleTimeString()}</span>}
          </div>

          <div className="insight-list">
            {report.findings.map((x) => (
              <InsightCard key={x.key} x={x} onAction={onAction} onNavigate={onNavigate} />
            ))}
          </div>

          {report.minor.length > 0 && (
            <details className="insight-more">
              <summary>
                {report.minor.length} smaller thing{report.minor.length === 1 ? "" : "s"} (once, long ago, or not often enough to matter)
              </summary>
              <div className="insight-list">
                {report.minor.map((x) => (
                  <InsightCard key={x.key} x={x} onAction={onAction} onNavigate={onNavigate} />
                ))}
              </div>
            </details>
          )}

          {report.unknown.length > 0 && (
            <details className="insight-more">
              <summary>
                {report.unknown.length} repeating error{report.unknown.length === 1 ? "" : "s"} DeskMedic doesn't recognise
              </summary>
              <p className="muted small">
                Not necessarily a problem. If one of these lines up with something going wrong, Export report includes them for IT.
              </p>
              <div className="insight-unknown">
                {report.unknown.map((u) => (
                  <div key={`${u.provider}-${u.id}`} className="insight-unknown-row">
                    <strong>
                      {u.provider} <span className="muted">· event {u.id}</span>
                    </strong>
                    <span className="muted small">
                      {times(u.count)} · last {ago(u.last)} · {u.log} log
                    </span>
                    {u.message && <span className="small">{u.message}</span>}
                  </div>
                ))}
              </div>
            </details>
          )}

          <p className="muted small insight-foot">
            {report.hidden > 0 &&
              `${report.hidden.toLocaleString()} log entr${report.hidden === 1 ? "y was" : "ies were"} left out: known to be harmless, or rare and not recognised. `}
            {report.errors.length > 0 && `Couldn't read: ${report.errors.join("; ")}.`}
          </p>
        </>
      )}
    </div>
  );
}

function InsightCard({ x, onAction, onNavigate }: { x: Insight; onAction: (a: HealthAction) => void; onNavigate: (p: Page) => void }) {
  const [steps, setSteps] = useState<Remedy | null>(null);
  const [open, setOpen] = useState(false);
  const tag = x.level === "problem" ? "Problem" : x.level === "warning" ? "Warning" : "Minor";
  // Minor ones get a neutral grey dot and tag: not "good", just small.
  const status = x.level;

  const toggleGuide = () => {
    if (!open && !steps && x.remedy) {
      guide(x.remedy)
        .then((g) => setSteps(g))
        .catch(toastError);
    }
    setOpen(!open);
  };

  return (
    <div className={`card insight ${x.level}`}>
      <span className={`health-dot ${status}`} aria-hidden="true" />
      <div className="grow">
        <div className="health-head">
          <h2>{x.title}</h2>
          <span className={`health-tag ${status}`}>{tag}</span>
        </div>
        <p className="muted small insight-facts">
          {times(x.count)} in 30 days{x.this_week > 0 && ` · ${x.this_week} this week`} · last {ago(x.last)}
        </p>
        <p className="small">{x.what}</p>
        <p className="muted small">{x.impact}</p>
        {(x.fix || x.remedy) && (
          <div className="insight-actions">
            {x.fix && (
              <button className="primary" onClick={() => onAction({ type: "remedy", id: x.fix as string })}>
                Fix: {x.fix_name}…
              </button>
            )}
            {x.remedy && (
              <button onClick={toggleGuide} aria-expanded={open}>
                {open ? "Hide the steps" : "How to fix it"}
              </button>
            )}
          </div>
        )}
        {open && steps && (
          <div className="insight-guide">
            <strong className="small">{steps.title}</strong>
            <ol className="remedy-steps">
              {steps.steps.map((s, i) => (
                <li key={i}>
                  <span>{s.text}</span>
                  {s.fix && <button onClick={() => onAction({ type: "remedy", id: s.fix as string })}>Run this fix…</button>}
                  {s.open && <button onClick={() => api.remedyOpen(steps.id, i).catch(toastError)}>Open</button>}
                  {s.page && (
                    <button onClick={() => onNavigate(s.page as Page)}>
                      Go to {PAGES.find((p) => p.page === s.page)?.label ?? s.page}
                    </button>
                  )}
                </li>
              ))}
            </ol>
            {steps.escalate && <p className="muted small">Still stuck? {steps.escalate}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
