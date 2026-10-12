import { useEffect, useState } from "react";
import { PageHeader } from "../components/Common";
import { DeskLabTools } from "../components/DeskLab";
import { api } from "../lib/api";
import { refreshAppInfo, toastError, useAppInfo } from "../lib/store";
import { checkAndDownload, installNow, useUpdate } from "../lib/updates";
import type { Settings, Theme } from "../lib/types";

/** A GB box that saves when you leave it (or press Enter), not per keystroke. */
function GbInput({ value, disabled, onSave }: { value: number; disabled: boolean; onSave: (gb: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(text));
    if (Number.isFinite(n) && n >= 1 && n <= 500) {
      if (n !== value) onSave(n);
    } else {
      setText(String(value));
      toastError("Enter a size from 1 to 500 GB.");
    }
  };
  return (
    <span className="gb-input">
      <input
        type="number"
        min={1}
        max={500}
        step={1}
        value={text}
        disabled={disabled}
        aria-label="Reminder size in GB"
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
      <span className="muted">GB</span>
    </span>
  );
}

function UpdateLine() {
  const st = useUpdate();
  const info = useAppInfo();
  if (!info) return null;
  if (!info.installed) {
    return (
      <>
        <div className="muted small">Portable copy: get new versions from GitHub (the installed version updates itself).</div>
      </>
    );
  }
  return (
    <div className="muted small">
      {st.kind === "idle" && (info.settings.auto_update ? "Updates download automatically." : "Automatic updates are off.")}
      {st.kind === "checking" && "Checking GitHub for a newer version…"}
      {st.kind === "latest" && "You have the latest version."}
      {st.kind === "downloading" && `Downloading ${st.version}${st.pct != null ? ` (${st.pct}%)` : ""}…`}
      {st.kind === "ready" && `Version ${st.version} is downloaded and ready.`}
      {st.kind === "installing" && `Installing ${st.version}… DeskMedic restarts by itself.`}
      {st.kind === "error" && <span className="error-text">Couldn't update: {st.message}</span>}
    </div>
  );
}

function UpdateButton() {
  const st = useUpdate();
  const info = useAppInfo();
  if (!info) return null;
  if (!info.installed) {
    return <button onClick={() => api.openLink("releases").catch(toastError)}>Download page</button>;
  }
  if (st.kind === "ready") {
    return (
      <button className="primary" onClick={() => void installNow()}>
        Restart to update
      </button>
    );
  }
  const busy = st.kind === "checking" || st.kind === "downloading" || st.kind === "installing";
  return (
    <button disabled={busy} onClick={() => void checkAndDownload(true)}>
      Check for updates
    </button>
  );
}

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
        <h2>Medic assistant</h2>
        <p className="muted small">
          DeskMedic sits in the tray next to the clock, runs the read-only health check in the background and pops up
          when something new looks wrong: low disk space, virus protection off, updates stuck, crashes or a failing
          drive. It never changes anything by itself and never goes online.
        </p>
        <label className="check-row">
          <input type="checkbox" checked={s.assistant} onChange={(e) => update({ ...s, assistant: e.target.checked })} />
          <span>
            Watch this PC and tell me when something looks wrong
            <span className="muted small block">Closing the window keeps DeskMedic in the tray. Quit from the tray menu.</span>
          </span>
        </label>
        <label className="field">
          <span>Check every</span>
          <select
            value={s.check_every_min}
            disabled={!s.assistant}
            onChange={(e) => update({ ...s, check_every_min: Number(e.target.value) })}
          >
            {[
              [30, "30 minutes"],
              [60, "1 hour"],
              [120, "2 hours"],
              [240, "4 hours"],
              [720, "12 hours"],
            ].map(([m, label]) => (
              <option key={m} value={m}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <div className="check-row">
          <input
            type="checkbox"
            id="temp-reminder"
            checked={s.temp_reminder}
            disabled={!s.assistant}
            onChange={(e) => update({ ...s, temp_reminder: e.target.checked })}
          />
          <span>
            <label htmlFor="temp-reminder">Remind me when temporary files pass</label>{" "}
            <GbInput
              value={s.temp_reminder_gb}
              disabled={!s.assistant || !s.temp_reminder}
              onSave={(gb) => update({ ...s, temp_reminder_gb: gb })}
            />
            <span className="muted small block">
              Temp files, update leftovers, crash dumps and browser caches: what Cleanup can clear. The Recycle Bin
              doesn't count. Reminds you again every 3 days while they stay over.
            </span>
          </span>
        </div>
        <label className="check-row">
          <input
            type="checkbox"
            checked={s.start_with_windows}
            onChange={(e) => update({ ...s, start_with_windows: e.target.checked })}
          />
          <span>
            Start with Windows (in the tray)
            <span className="muted small block">
              On for the installed version. For the portable exe, only turn this on if it stays in the same folder.
            </span>
          </span>
        </label>
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
            <UpdateLine />
          </div>
          <UpdateButton />
        </div>
        {info.installed && (
          <label className="check-row">
            <input
              type="checkbox"
              checked={s.auto_update}
              onChange={(e) => update({ ...s, auto_update: e.target.checked })}
            />
            <span>
              Download updates automatically
              <span className="muted small block">
                Checks GitHub once a day, downloads new versions in the background (signature-checked) and asks you to
                restart. Nothing else is sent.
              </span>
            </span>
          </label>
        )}
        <div className="more-tools">
          <DeskLabTools own="deskmedic" />
          <div className="muted small made-by">
            Made by Jagdeep Sandhu ·{" "}
            <button className="link" onClick={() => api.openLink("developer").catch(toastError)}>
              github.com/imjagdeep
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
