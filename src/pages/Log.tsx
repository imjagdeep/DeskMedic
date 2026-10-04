import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { dateTime } from "../lib/format";
import { toast, toastError } from "../lib/store";
import type { LogEntry } from "../lib/types";

export function Log() {
  const [entries, setEntries] = useState<LogEntry[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const load = () => {
    api.readLog().then(setEntries).catch((e) => {
      setEntries([]);
      toastError(e);
    });
  };
  useEffect(load, []);

  const exportText = async () => {
    try {
      const path = await save({
        title: "Save the log for a ticket",
        defaultPath: "DeskMedic-log.txt",
        filters: [{ name: "Text", extensions: ["txt"] }],
      });
      if (!path) return;
      const n = await api.exportLog(path);
      toast(`Saved ${n} entries`);
      load();
    } catch (e) {
      toastError(e);
    }
  };

  return (
    <div className="page">
      <PageHeader title="Log">
        <button onClick={exportText} disabled={!entries?.length}>Export for ticket…</button>
      </PageHeader>
      <p className="muted">Everything DeskMedic changed on this PC, who ran it and what happened.</p>
      {entries === null && <div className="empty">Loading…</div>}
      {entries?.length === 0 && (
        <div className="empty">Nothing yet. Cleanups, disk actions and fix-its will show up here.</div>
      )}
      {entries && entries.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>When</th>
              <th>Action</th>
              <th>Result</th>
              <th>By</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.id} onClick={() => setOpen(open === e.id ? null : e.id)}>
                <td className="nowrap">{dateTime(e.ts)}</td>
                <td>
                  <span className={e.ok ? "status ok" : "status fail"}>{e.ok ? "OK" : "Failed"}</span> {e.action}
                </td>
                <td>
                  {e.summary}
                  {open === e.id && e.details.length > 0 && <pre className="details">{e.details.join("\n")}</pre>}
                </td>
                <td className="nowrap muted">
                  {e.user}
                  {e.elevated ? " (admin)" : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
