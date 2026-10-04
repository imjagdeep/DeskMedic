import { openUrl } from "@tauri-apps/plugin-opener";
import { PageHeader } from "../components/Common";
import { api } from "../lib/api";
import { refreshAppInfo, toastError, useAppInfo } from "../lib/store";
import type { Settings, Theme } from "../lib/types";

const RELEASES = "https://github.com/imjagdeep/deskmedic/releases/latest";

export function SettingsPage() {
  const info = useAppInfo();
  if (!info) return <div className="page" />;
  const s = info.settings;

  const update = async (next: Settings) => {
    try {
      await api.saveSettings(next);
      await refreshAppInfo();
    } catch (e) {
      toastError(e);
    }
  };

  return (
    <div className="page">
      <PageHeader title="Settings" />

      <div className="card">
        <h2>Appearance</h2>
        <div className="segmented" role="group" aria-label="Theme">
          {(["system", "light", "dark"] as Theme[]).map((t) => (
            <button key={t} className={s.theme === t ? "active" : ""} onClick={() => update({ ...s, theme: t })}>
              {t === "system" ? "System" : t === "light" ? "Light" : "Dark"}
            </button>
          ))}
        </div>
      </div>

      <div className="card">
        <h2>Log</h2>
        <label className="field">
          <span>Keep entries for</span>
          <select value={s.keep_log_days} onChange={(e) => update({ ...s, keep_log_days: Number(e.target.value) })}>
            {[30, 90, 180, 365, 730].map((d) => (
              <option key={d} value={d}>
                {d} days
              </option>
            ))}
          </select>
        </label>
        <div className="folder-field">
          <span className="folder-label">Data</span>
          <span className="folder-path" title={info.data_dir}>{info.data_dir}</span>
          <button onClick={() => api.openDataFolder().catch(toastError)}>Open</button>
        </div>
        <p className="muted small">
          Settings and the log live here, not next to the program, so DeskMedic can run from a USB stick or a
          read-only network share.
        </p>
      </div>

      <div className="card">
        <div className="about-row">
          <div>
            <div className="about-title">
              DeskMedic <span className="muted">{info.version}</span>
            </div>
            <div className="muted small">Open source, MIT licence. No account, no telemetry.</div>
          </div>
          <button onClick={() => openUrl(RELEASES).catch(toastError)}>Check for a newer version</button>
        </div>
      </div>
    </div>
  );
}
