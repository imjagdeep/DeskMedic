import { useState } from "react";
import { PageHeader } from "../components/Common";
import { runNetworkDoctor, useNetworkJob } from "../lib/jobs";
import type { HealthAction, HealthStatus } from "../lib/types";

const LABEL: Record<HealthStatus, string> = {
  good: "OK",
  warning: "Warning",
  problem: "Problem",
  unknown: "Not tested",
};

const ACTION_LABEL: Record<string, string> = {
  "reset-network": "Reset network…",
  "flush-dns": "Flush DNS…",
  "time-sync": "Sync the clock…",
};

export function Network({ onAction }: { onAction: (a: HealthAction) => void }) {
  // Kept outside the page so leaving it mid-test loses nothing.
  const { running, report, at } = useNetworkJob();
  const [asking, setAsking] = useState(false);
  const action = report?.action ?? null;

  const start = () => {
    setAsking(false);
    void runNetworkDoctor();
  };

  return (
    <div className="page">
      <PageHeader title="Network doctor">
        <button className="primary" disabled={running} onClick={() => setAsking(true)}>
          {running ? "Testing…" : report ? "Test again" : "Test the network"}
        </button>
      </PageHeader>
      <p className="muted">
        Tests each link from this PC to the internet in order and says where it breaks: the connection, Wi-Fi signal,
        router, DNS, the internet itself, and whether the clock is right.
      </p>

      {!report && !running && <div className="empty">Press Test the network. It takes a few seconds.</div>}
      {running && !report && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>Testing the network…</span>
        </div>
      )}

      {report && (
        <>
          <div className={`health-summary ${report.status}`}>
            <div>
              <strong>{report.verdict}</strong>
              {at && <div className="muted small">Tested at {new Date(at).toLocaleTimeString()}</div>}
            </div>
            {action && (
              <button className="primary" onClick={() => onAction(action)}>
                {action.type === "remedy" ? (ACTION_LABEL[action.id] ?? "Fix it") : "Open"}
              </button>
            )}
          </div>
          <ol className="net-chain">
            {report.links.map((l) => (
              <li key={l.id} className={`card net-link ${l.status}`}>
                <span className={`health-dot ${l.status}`} aria-hidden="true" />
                <div className="grow">
                  <div className="health-head">
                    <h2>{l.title}</h2>
                    <span className={`health-tag ${l.status}`}>{LABEL[l.status]}</span>
                  </div>
                  <p className="small">{l.summary}</p>
                </div>
              </li>
            ))}
          </ol>
        </>
      )}

      {asking && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="net-h">
          <div className="modal">
            <h2 id="net-h">Test the network?</h2>
            <p>This goes online, which DeskMedic otherwise doesn't do. It will:</p>
            <ul className="steps">
              <li>ping your router three times,</li>
              <li>look up one name with your DNS server,</li>
              <li>
                load Microsoft's connection-test page (<code>www.msftconnecttest.com</code>), the same check Windows does
                itself, and read its clock from the reply.
              </li>
            </ul>
            <p className="muted small">Nothing about this PC is sent. Nothing is changed.</p>
            <div className="modal-actions">
              <button onClick={() => setAsking(false)}>Cancel</button>
              <button className="primary" autoFocus onClick={start}>
                Test now
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
