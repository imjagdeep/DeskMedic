import { PageHeader } from "../components/Common";
import { SendToIt } from "../components/SendToIt";
import { runHealthCheck, useHealthJob } from "../lib/jobs";
import type { HealthAction, HealthCheck, HealthStatus } from "../lib/types";

const LABEL: Record<HealthStatus, string> = {
  good: "Good",
  warning: "Warning",
  problem: "Problem",
  unknown: "Unknown",
};

// Problems first, then warnings; the rest keep their order.
const RANK: Record<HealthStatus, number> = { problem: 0, warning: 1, good: 2, unknown: 3 };

export function Health({ onAction }: { onAction: (a: HealthAction) => void }) {
  // Kept outside the page so leaving it mid-check loses nothing.
  const { running, report, at } = useHealthJob();
  const checks = report ? [...report.checks].sort((a, b) => RANK[a.status] - RANK[b.status]) : [];

  return (
    <div className="page">
      <PageHeader title="Health check">
        <SendToIt />
        <button className="primary" disabled={running} onClick={() => void runHealthCheck()}>
          {running ? "Checking…" : report ? "Check again" : "Run health check"}
        </button>
      </PageHeader>
      <p className="muted">
        A quick, read-only look at what usually sits behind a slow or misbehaving PC: space, restarts, updates, virus
        protection, internet, drives, crashes, memory and battery. Nothing is changed, and nothing goes online. Send to IT
        saves this with system details and the log as one zip for a ticket.
      </p>

      {!report && !running && (
        <div className="empty">
          Run the health check to see what needs attention. It takes a few seconds.
        </div>
      )}
      {running && !report && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>Checking this PC…</span>
        </div>
      )}

      {report && (
        <>
          <div className={`health-summary ${report.problems ? "problem" : report.warnings ? "warning" : "good"}`}>
            <strong>
              {report.problems === 0 && report.warnings === 0
                ? "All good. Nothing needs attention."
                : [
                    report.problems && `${report.problems} problem${report.problems === 1 ? "" : "s"}`,
                    report.warnings && `${report.warnings} warning${report.warnings === 1 ? "" : "s"}`,
                  ]
                    .filter(Boolean)
                    .join(" and ") + " to look at."}
            </strong>
            {at && <span className="muted small">Checked at {new Date(at).toLocaleTimeString()}</span>}
          </div>
          <div className="health-list">
            {checks.map((c) => (
              <Row key={c.id} check={c} onAction={onAction} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function Row({ check: c, onAction }: { check: HealthCheck; onAction: (a: HealthAction) => void }) {
  const action = c.action;
  return (
    <div className={`card health-row ${c.status}`}>
      <span className={`health-dot ${c.status}`} aria-hidden="true" />
      <div className="grow">
        <div className="health-head">
          <h2>{c.title}</h2>
          <span className={`health-tag ${c.status}`}>{LABEL[c.status]}</span>
        </div>
        <p className="small">{c.summary}</p>
        {c.advice && <p className="muted small">{c.advice}</p>}
      </div>
      {action && (
        <button onClick={() => onAction(action)}>
          {action.type === "open_cleanup" ? "Open Cleanup" : action.type === "open_network" ? "Network doctor" : "Fix it"}
        </button>
      )}
    </div>
  );
}
