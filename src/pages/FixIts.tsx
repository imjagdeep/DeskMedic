import { useEffect, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { toastError, useAppInfo } from "../lib/store";
import type { FixRecipe, FixResult } from "../lib/types";

export function FixIts() {
  const info = useAppInfo();
  const [recipes, setRecipes] = useState<FixRecipe[]>([]);
  const [asking, setAsking] = useState<FixRecipe | null>(null);
  const [running, setRunning] = useState<string | null>(null);
  const [result, setResult] = useState<FixResult | null>(null);

  useEffect(() => {
    api.fixList().then(setRecipes).catch(toastError);
  }, []);

  const run = async (r: FixRecipe) => {
    setAsking(null);
    setRunning(r.id);
    try {
      setResult(await api.fixRun(r.id));
    } catch (e) {
      toastError(e);
    } finally {
      setRunning(null);
    }
  };

  return (
    <div className="page wide">
      <PageHeader title="Fix-its" />
      <p className="muted">One-click fixes for common calls. Each shows exactly what it runs first, and every run goes in the Log.</p>
      <AdminNotice need="Most fixes change system settings and need administrator rights." />
      <div className="fix-grid">
        {recipes.map((r) => {
          const blocked = r.admin && !info?.elevated;
          return (
            <div className="card fix-card" key={r.id}>
              <h2>{r.name}</h2>
              <p className="small">{r.when}</p>
              <p className="muted small fix-does">{r.does}</p>
              <div className="fix-foot">
                <span className="muted small">
                  {r.takes}
                  {r.admin && <span className="badge">admin</span>}
                  {r.restart && <span className="badge warn">restart</span>}
                </span>
                <button
                  className="primary"
                  disabled={blocked || running !== null}
                  title={blocked ? "Needs administrator" : undefined}
                  onClick={() => setAsking(r)}
                >
                  {running === r.id ? "Running…" : "Run"}
                </button>
              </div>
            </div>
          );
        })}
      </div>

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
              <button className="primary" onClick={() => setResult(null)}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
