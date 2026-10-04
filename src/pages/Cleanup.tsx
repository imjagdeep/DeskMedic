import { useCallback, useEffect, useMemo, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { humanSize } from "../lib/format";
import { toastError, useAppInfo } from "../lib/store";
import type { CleanupEstimate, CleanupReport } from "../lib/types";

export function Cleanup() {
  const info = useAppInfo();
  const [allUsers, setAllUsers] = useState(false);
  const [items, setItems] = useState<CleanupEstimate[] | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [running, setRunning] = useState(false);
  const [report, setReport] = useState<CleanupReport | null>(null);

  const measure = useCallback((everyone: boolean) => {
    setItems(null);
    setReport(null);
    api
      .cleanupPreview(everyone)
      .then((list) => {
        setItems(list);
        // Recommended, runnable, and with something to free (or unknown size).
        setPicked(
          new Set(list.filter((e) => e.recommended && !e.blocked && (e.bytes === null || e.bytes > 0)).map((e) => e.id)),
        );
      })
      .catch((e) => {
        setItems([]);
        toastError(e);
      });
  }, []);

  useEffect(() => measure(false), [measure]);

  const selected = useMemo(() => (items ?? []).filter((e) => picked.has(e.id)), [items, picked]);
  const total = selected.reduce((s, e) => s + (e.bytes ?? 0), 0);

  const toggle = (id: string) =>
    setPicked((p) => {
      const n = new Set(p);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const run = async () => {
    setConfirming(false);
    setRunning(true);
    try {
      setReport(await api.cleanupRun([...picked], allUsers));
    } catch (e) {
      toastError(e);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="page">
      <PageHeader title="Cleanup">
        <button onClick={() => measure(allUsers)} disabled={items === null || running}>Measure again</button>
      </PageHeader>
      <p className="muted">
        Only places that hold temporary data. Documents, Desktop, Downloads, pictures and OneDrive are never touched.
      </p>
      <AdminNotice need="Windows temp files, update caches, system crash dumps and component cleanup need administrator rights." />

      {info?.elevated && (
        <label className="check card-row">
          <input
            type="checkbox"
            checked={allUsers}
            disabled={items === null || running}
            onChange={(e) => {
              setAllUsers(e.target.checked);
              measure(e.target.checked);
            }}
          />
          <span>
            Include every user profile on this PC
            <span className="muted small">Otherwise only {info.user}'s files are cleaned.</span>
          </span>
        </label>
      )}

      {report ? (
        <Result report={report} onDone={() => measure(allUsers)} />
      ) : items === null ? (
        <div className="card scan-progress">
          <div className="spinner" aria-hidden="true" />
          <span>Measuring what can be cleaned…</span>
        </div>
      ) : (
        <>
          <div className="card list">
            {items.map((e) => (
              <label key={e.id} className={e.blocked ? "clean-row blocked" : "clean-row"}>
                <input
                  type="checkbox"
                  checked={picked.has(e.id)}
                  disabled={!!e.blocked || running}
                  onChange={() => toggle(e.id)}
                />
                <span className="grow">
                  <span className="clean-name">{e.name}</span>
                  {e.blocked && <span className="badge warn">{e.blocked}</span>}
                  <span className="muted small clean-desc">{e.description}</span>
                </span>
                <span className="clean-size">
                  {e.bytes === null ? <span className="muted">—</span> : humanSize(e.bytes)}
                  {e.files > 0 && (
                    <span className="muted small">
                      {e.files.toLocaleString()} {e.files === 1 ? "file" : "files"}
                    </span>
                  )}
                </span>
              </label>
            ))}
          </div>
          <div className="row clean-foot">
            <span className="grow">
              {selected.length} selected · about <strong>{humanSize(total)}</strong>
              {selected.some((e) => e.bytes === null) && " plus Windows clean-up"}
            </span>
            <button className="primary big" disabled={!selected.length || running} onClick={() => setConfirming(true)}>
              {running ? "Cleaning…" : "Clean up…"}
            </button>
          </div>
        </>
      )}

      {running && (
        <div className="card scan-progress">
          <div className="spinner" aria-hidden="true" />
          <span>Cleaning. Windows component cleanup can take several minutes…</span>
        </div>
      )}

      {confirming && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="confirm-h">
          <div className="modal">
            <h2 id="confirm-h">Delete these files?</h2>
            <div className="preview-list">
              {selected.map((e) => (
                <div className="preview-row" key={e.id}>
                  <div className="preview-src">{e.name}</div>
                  <div className="preview-dst">{e.bytes === null ? "size known afterwards" : humanSize(e.bytes)}</div>
                </div>
              ))}
            </div>
            <p className="muted small">
              Deleted files don't go to the Recycle Bin. Files in use are skipped. Everything is written to the Log.
            </p>
            <div className="modal-actions">
              <button onClick={() => setConfirming(false)}>Cancel</button>
              <button className="primary" onClick={run} autoFocus>
                Clean up {humanSize(total)}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Result({ report, onDone }: { report: CleanupReport; onDone: () => void }) {
  const gained = Math.max(0, report.free_after - report.free_before);
  return (
    <>
      <div className="card result-head">
        <div className="stat-num">+{humanSize(gained)}</div>
        <div className="muted">
          Free space on the Windows drive: {humanSize(report.free_before)} → {humanSize(report.free_after)}
        </div>
      </div>
      <div className="card list">
        {report.outcomes.map((o) => (
          <div className="list-row" key={o.id}>
            <span>
              <span className={o.ok ? "status ok" : "status fail"}>{o.ok ? "Done" : "Not done"}</span> {o.name}
              {o.note && <span className="muted small"> · {o.note}</span>}
            </span>
            <span className="size-col">{o.freed === null ? "—" : humanSize(o.freed)}</span>
          </div>
        ))}
      </div>
      <div className="row">
        <button onClick={onDone}>Back to the list</button>
      </div>
    </>
  );
}
