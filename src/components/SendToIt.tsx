import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useNetworkJob } from "../lib/jobs";
import { toast, toastError, useAppInfo } from "../lib/store";

/** "Export report" button: explains what goes in the zip, then saves it. */
export function SendToIt() {
  const info = useAppInfo();
  const network = useNetworkJob().report;
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  // File › Export report opens this explanation too.
  useEffect(() => {
    const show = () => setOpen(true);
    window.addEventListener("send-to-it", show);
    return () => window.removeEventListener("send-to-it", show);
  }, []);

  const make = async () => {
    try {
      const day = new Date().toISOString().slice(0, 10);
      const path = await save({
        title: "Save the report",
        defaultPath: `DeskMedic-${info?.computer || "PC"}-${day}.zip`,
        filters: [{ name: "Zip", extensions: ["zip"] }],
      });
      if (!path) return;
      setBusy(true);
      const files = await api.sendToIt(path);
      setOpen(false);
      toast(`Saved (${files.length} files). Attach the zip to your email or ticket.`);
    } catch (e) {
      toastError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button onClick={() => setOpen(true)}>Export report…</button>
      {open && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="it-h">
          <div className="modal">
            <h2 id="it-h">Export report</h2>
            <p>Saves one zip you can attach to an email or ticket. It holds plain-text files:</p>
            <ul className="steps">
              <li>
                <strong>System information</strong>: computer and user name, make, model and serial number, Windows
                version, drives, IP addresses.
              </li>
              <li>
                <strong>Health check</strong>, run fresh now.
              </li>
              {network && (
                <li>
                  <strong>Network doctor</strong> result from earlier.
                </li>
              )}
              <li>
                <strong>DeskMedic log</strong>: everything DeskMedic did on this PC.
              </li>
            </ul>
            <p className="muted small">No documents, passwords or browsing history. Nothing is sent anywhere; you decide who gets the file.</p>
            <div className="modal-actions">
              <button onClick={() => setOpen(false)} disabled={busy}>
                Cancel
              </button>
              <button className="primary" autoFocus disabled={busy} onClick={() => void make()}>
                {busy ? "Collecting…" : "Save zip…"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
