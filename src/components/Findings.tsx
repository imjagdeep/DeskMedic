// Cards for the Checkup list: a health check result (how the PC is right now) and a
// finding from Windows' event logs (what happened over the last 30 days).

import { useState } from "react";
import { api } from "../lib/api";
import { toastError } from "../lib/store";
import type { HealthAction, HealthCheck, Insight, InsightsReport, Remedy } from "../lib/types";
import { destLabel, type Dest } from "./Sidebar";

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

/** "2 hours ago", "yesterday", "3 weeks ago". */
export function ago(iso: string): string {
  const mins = Math.round((new Date(iso).getTime() - Date.now()) / 60000);
  if (mins > -60) return rtf.format(Math.min(mins, -1), "minute");
  const hours = Math.round(mins / 60);
  if (hours > -24) return rtf.format(hours, "hour");
  const days = Math.round(hours / 24);
  return days > -14 ? rtf.format(days, "day") : rtf.format(Math.round(days / 7), "week");
}

const times = (n: number) => (n === 1 ? "once" : `${n} times`);

const TAG = { problem: "Problem", warning: "Warning", minor: "Minor", good: "Good", unknown: "Unknown" } as const;

/** A health check result: what it found right now and the next step. */
export function HealthCard({ c, onAction }: { c: HealthCheck; onAction: (a: HealthAction) => void }) {
  const a = c.action;
  return (
    <div className={`card insight ${c.status}`}>
      <span className={`health-dot ${c.status}`} aria-hidden="true" />
      <div className="grow">
        <div className="health-head">
          <h2>{c.title}</h2>
          <span className={`health-tag ${c.status}`}>{TAG[c.status]}</span>
        </div>
        <p className="muted small insight-facts">Right now</p>
        <p className="small">{c.summary}</p>
        {c.advice && <p className="muted small">{c.advice}</p>}
        {a && (
          <div className="insight-actions">
            <button className={a.type === "remedy" ? "primary" : undefined} onClick={() => onAction(a)}>
              {a.type === "open_cleanup" ? "Clean up" : a.type === "open_network" ? "Network doctor" : "Fix it…"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

/** Guides from the remedies library, loaded once when a card asks for its steps. */
let guides: Promise<Remedy[]> | null = null;
const guide = (id: string) => {
  guides ??= api.remedyLibrary().then((l) => l.remedies);
  return guides.then((all) => all.find((r) => r.id === id) ?? null);
};

/** A finding from the event logs: what happened, how often, why it matters, and the fix. */
export function InsightCard({ x, onAction, onNavigate }: { x: Insight; onAction: (a: HealthAction) => void; onNavigate: (d: Dest) => void }) {
  const [steps, setSteps] = useState<Remedy | null>(null);
  const [open, setOpen] = useState(false);

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
      {/* Minor ones get a neutral grey dot and tag: not "good", just small. */}
      <span className={`health-dot ${x.level}`} aria-hidden="true" />
      <div className="grow">
        <div className="health-head">
          <h2>{x.title}</h2>
          <span className={`health-tag ${x.level}`}>{TAG[x.level]}</span>
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
                  {s.page && <button onClick={() => onNavigate(s.page as Dest)}>Go to {destLabel(s.page as Dest)}</button>}
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

/** Smaller log findings, unrecognised repeating errors, and what was left out. */
export function LogExtras({ report, onAction, onNavigate }: { report: InsightsReport; onAction: (a: HealthAction) => void; onNavigate: (d: Dest) => void }) {
  return (
    <>
      {report.minor.length > 0 && (
        <details className="insight-more">
          <summary>
            {report.minor.length} smaller thing{report.minor.length === 1 ? "" : "s"} in the logs (once, long ago, or not often enough to matter)
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
  );
}
