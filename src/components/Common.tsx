import { api } from "../lib/api";
import { toastError, useAppInfo, useToasts } from "../lib/store";
import { ShieldIcon } from "./Icons";

export function Toasts() {
  const toasts = useToasts();
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={t.error ? "toast error" : "toast"}>{t.text}</div>
      ))}
    </div>
  );
}

export function PageHeader({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <header className="page-header">
      <h1>{title}</h1>
      <div className="page-actions">{children}</div>
    </header>
  );
}

/** Shown on pages that need admin rights while the app runs without them. */
export function AdminNotice({ need }: { need: string }) {
  const info = useAppInfo();
  if (!info || info.elevated) return null;
  return (
    <div className="notice">
      <ShieldIcon size={20} />
      <div className="grow">
        <strong>Not running as administrator.</strong> {need}
      </div>
      <button className="primary" onClick={() => api.restartAsAdmin().catch(toastError)}>
        Restart as administrator
      </button>
    </div>
  );
}

/** Used/free bar; turns orange above 85 % and red above 95 %. */
export function UsageBar({ used, total }: { used: number; total: number }) {
  const pct = total > 0 ? Math.min(100, (used / total) * 100) : 0;
  const level = pct >= 95 ? "crit" : pct >= 85 ? "warn" : "";
  return (
    <div className={`usage-bar ${level}`} role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
      <span style={{ width: `${pct}%` }} />
    </div>
  );
}
