import { useEffect, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { AppsIcon, GlobeIcon, LockIcon, PrinterIcon, ShieldIcon, WindowIcon } from "../components/Icons";
import { api } from "../lib/api";
import { clearFixResult, runFix, useFixJob } from "../lib/jobs";
import { toastError, useAppInfo } from "../lib/store";
import type { FixRecipe } from "../lib/types";

type Group = { title: string; Icon: (p: { size?: number }) => React.ReactElement; ids: string[] };

// Remedy ids by topic; anything new and unlisted lands in "More".
const GROUPS: Group[] = [
  { title: "Internet and network", Icon: GlobeIcon, ids: ["flush-dns", "reset-network"] },
  { title: "Windows", Icon: WindowIcon, ids: ["restart-explorer", "rebuild-icon-cache", "rebuild-search", "time-sync", "gpupdate"] },
  { title: "Apps", Icon: AppsIcon, ids: ["reset-teams", "reset-onedrive", "reset-store"] },
  { title: "Printing and sound", Icon: PrinterIcon, ids: ["print-queue", "restart-audio"] },
  { title: "System repair", Icon: ShieldIcon, ids: ["repair-windows-update", "sfc", "dism-restore"] },
];

/** `ask`: a remedy to open straight away (from the Health check). */
export function Remedies({ ask, onAsked }: { ask?: string | null; onAsked?: () => void }) {
  const info = useAppInfo();
  const [recipes, setRecipes] = useState<FixRecipe[]>([]);
  const [asking, setAsking] = useState<FixRecipe | null>(null);
  // Restore point first, for remedies that offer one. On by default.
  const [restorePoint, setRestorePoint] = useState(true);
  // Kept outside the page so leaving it mid-fix loses nothing.
  const { running, result } = useFixJob();

  useEffect(() => {
    api.fixList().then(setRecipes).catch(toastError);
  }, []);

  useEffect(() => {
    if (!ask || recipes.length === 0) return;
    const r = recipes.find((x) => x.id === ask);
    if (r && !(r.admin && !info?.elevated)) {
      setRestorePoint(true);
      setAsking(r);
    }
    onAsked?.();
  }, [ask, recipes, info, onAsked]);

  const run = (r: FixRecipe) => {
    setAsking(null);
    void runFix(r.id, r.restore_point && restorePoint);
  };

  return (
    <div className="page wide">
      <PageHeader title="Remedies" />
      <p className="muted">One-click remedies for common problems. Each shows exactly what it runs first, and every run goes in the Log.</p>
      <AdminNotice need="Many remedies change system settings and need administrator rights." />
      {[...GROUPS, { title: "More", Icon: AppsIcon, ids: recipes.map((r) => r.id).filter((id) => !GROUPS.some((g) => g.ids.includes(id))) }]
        .map((g) => ({ ...g, items: g.ids.map((id) => recipes.find((r) => r.id === id)).filter((r): r is FixRecipe => !!r) }))
        .filter((g) => g.items.length > 0)
        .map(({ title, Icon, items }) => (
          <section key={title} className="fix-group">
            <h3 className="fix-group-title">
              <span className="fix-group-icon">
                <Icon size={15} />
              </span>
              {title}
            </h3>
            <div className="fix-grid">
              {items.map((r) => {
                const blocked = r.admin && !info?.elevated;
                return (
                  <div className={running === r.id ? "card fix-card running" : "card fix-card"} key={r.id}>
                    <h2>{r.name}</h2>
                    <p className="small">{r.when}</p>
                    <p className="muted small fix-does">{r.does}</p>
                    <div className="fix-foot">
                      <span className="muted small fix-meta">
                        {r.takes}
                        {r.restart && <span className="badge warn">restart</span>}
                        {r.restore_point && <span className="badge">restore point</span>}
                      </span>
                      <button
                        className={blocked ? "with-icon" : "primary"}
                        disabled={blocked || running !== null}
                        title={blocked ? "Restart DeskMedic as administrator to run this" : undefined}
                        onClick={() => {
                          setRestorePoint(true);
                          setAsking(r);
                        }}
                      >
                        {blocked && <LockIcon size={13} />}
                        {running === r.id ? "Running…" : blocked ? "Needs admin" : "Run…"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ))}

      {running && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>
            Running {recipes.find((r) => r.id === running)?.name}… ({recipes.find((r) => r.id === running)?.takes})
          </span>
        </div>
      )}

      {asking && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-h">
          <div className="modal">
            <h2 id="fix-h">{asking.name}</h2>
            <p>{asking.does}</p>
            <ol className="steps">
              {asking.steps.map((s) => (
                <li key={s}>{s}</li>
              ))}
            </ol>
            <p className="muted small">
              Takes {asking.takes}.{asking.restart && " Restart the PC afterwards for it to take effect."}
            </p>
            {asking.restore_point && (
              <label className="check-row">
                <input type="checkbox" checked={restorePoint} onChange={(e) => setRestorePoint(e.target.checked)} />
                <span>
                  Create a restore point first (recommended)
                  <span className="muted small block">
                    A way back through System Restore if something goes wrong. Adds up to a minute.
                  </span>
                </span>
              </label>
            )}
            <div className="modal-actions">
              <button onClick={() => setAsking(null)}>Cancel</button>
              <button className="primary" autoFocus onClick={() => run(asking)}>
                Run it
              </button>
            </div>
          </div>
        </div>
      )}

      {result && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-result-h">
          <div className="modal">
            <h2 id="fix-result-h">
              <span className={result.ok ? "status ok" : "status fail"}>{result.ok ? "Done" : "Failed"}</span> {result.name}
            </h2>
            {result.steps.map((s) => (
              <div key={s.label} className="fix-step">
                <strong>{s.label}</strong>
                {s.output && <pre className="details">{s.output}</pre>}
              </div>
            ))}
            {result.restart && <p className="notice-text">Restart the PC to finish.</p>}
            <div className="modal-actions">
              <button className="primary" onClick={clearFixResult}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
