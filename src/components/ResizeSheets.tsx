import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { humanSize } from "../lib/format";
import { toastError } from "../lib/store";
import type { ExtendPlan, ExtendStep } from "../lib/types";

function stepText(s: ExtendStep): string {
  switch (s.step) {
    case "disable_win_re":
      return "Turn off Windows Recovery for a moment (its image is kept safe on C:)";
    case "delete_recovery":
      return `Remove the old recovery partition (partition ${s.partition})`;
    case "extend":
      return `Extend the volume to ${humanSize(s.size)}`;
    case "create_recovery":
      return `Create a new ${humanSize(s.size)} recovery partition at the end of the disk`;
    case "enable_win_re":
      return "Turn Windows Recovery back on";
  }
}

/** The guided extend: shows the plan, runs it after a typed confirmation. */
export function ExtendSheet({ letter, onClose, onDone }: { letter: string; onClose: () => void; onDone: () => void }) {
  const [plan, setPlan] = useState<ExtendPlan | null>(null);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);

  useEffect(() => {
    api.extendPlan(letter).then(setPlan).catch((e) => {
      toastError(e);
      onClose();
    });
  }, [letter, onClose]);

  const run = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      setResult({ ok: true, lines: await api.extendRun(letter, plan.steps.length, typed) });
    } catch (e) {
      setResult({ ok: false, lines: [typeof e === "string" ? e : String(e)] });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="extend-h">
      <div className="modal">
        <h2 id="extend-h">Extend {letter}:</h2>
        {!plan && <p className="muted">Working out the steps…</p>}
        {plan?.blocked && (
          <>
            <p>{plan.blocked}</p>
            <div className="modal-actions">
              <button onClick={onClose}>Close</button>
            </div>
          </>
        )}
        {plan && !plan.blocked && !result && (
          <>
            <p>
              {humanSize(plan.current_size)} → <strong>{humanSize(plan.new_size)}</strong>
            </p>
            <ol className="steps">
              {plan.steps.map((s, i) => (
                <li key={i}>{stepText(s)}</li>
              ))}
            </ol>
            {plan.moves_recovery && (
              <p className="notice-text">
                The recovery partition is in the way, so it is moved to the end of the disk. This follows Microsoft's
                documented steps and takes a few minutes. Keep the PC on and DeskMedic open until it finishes. If it
                stops part-way, open this again to continue from that step.
              </p>
            )}
            <label className="field">
              <span>
                Type <strong className="mono">{letter}</strong> to confirm
              </span>
              <input autoFocus value={typed} onChange={(e) => setTyped(e.target.value)} disabled={busy} />
            </label>
            <div className="modal-actions">
              <button onClick={onClose} disabled={busy}>Cancel</button>
              <button
                className="primary"
                disabled={busy || typed.trim().toUpperCase() !== letter.toUpperCase()}
                onClick={run}
              >
                {busy ? "Working…" : "Extend"}
              </button>
            </div>
          </>
        )}
        {result && (
          <>
            <p className={result.ok ? "" : "error-text"}>{result.ok ? "Done." : "It stopped:"}</p>
            <ul className="steps">
              {result.lines.map((l, i) => (
                <li key={i}>{l}</li>
              ))}
            </ul>
            <div className="modal-actions">
              <button className="primary" onClick={onDone}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/** Grow or shrink a data partition within the sizes Windows allows. */
export function ResizeSheet({
  disk,
  partition,
  letter,
  current,
  onClose,
  onDone,
}: {
  disk: number;
  partition: number;
  letter: string;
  current: number;
  onClose: () => void;
  onDone: (msg: string) => void;
}) {
  const [range, setRange] = useState<[number, number] | null>(null);
  const [gb, setGb] = useState(current / 1e9);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.resizeInfo(disk, partition).then(setRange).catch((e) => {
      toastError(e);
      onClose();
    });
  }, [disk, partition, onClose]);

  const size = Math.round(gb * 1e9);
  const inRange = range !== null && size >= range[0] && size <= range[1];
  const run = async () => {
    setBusy(true);
    try {
      onDone(await api.resize(disk, partition, size, typed));
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="resize-h">
      <div className="modal">
        <h2 id="resize-h">Resize {letter}:</h2>
        {!range && <p className="muted">Asking Windows how far it can go (this can take a minute)…</p>}
        {range && (
          <>
            <p className="muted small">
              Now {humanSize(current)}. Windows allows {humanSize(range[0])} to {humanSize(range[1])}; shrinking stops at
              files it can't move.
            </p>
            <input
              type="range"
              className="slider"
              min={range[0] / 1e9}
              max={range[1] / 1e9}
              step={0.1}
              value={gb}
              onChange={(e) => setGb(Number(e.target.value))}
              disabled={busy}
              aria-label="New size in GB"
            />
            <div className="row">
              <input
                type="number"
                step={0.1}
                value={gb.toFixed(1)}
                onChange={(e) => setGb(Number(e.target.value))}
                disabled={busy}
                aria-label="New size in GB"
              />
              <span>GB</span>
              <span className="muted small">
                {size > current ? `grow by ${humanSize(size - current)}` : size < current ? `shrink by ${humanSize(current - size)}` : "no change"}
              </span>
            </div>
            <label className="field">
              <span>
                Type <strong className="mono">{letter}</strong> to confirm
              </span>
              <input value={typed} onChange={(e) => setTyped(e.target.value)} disabled={busy} />
            </label>
          </>
        )}
        <div className="modal-actions">
          <button onClick={onClose} disabled={busy}>Cancel</button>
          <button
            className="primary"
            disabled={!inRange || busy || size === current || typed.trim().toUpperCase() !== letter.toUpperCase()}
            onClick={run}
          >
            {busy ? "Working…" : "Resize"}
          </button>
        </div>
      </div>
    </div>
  );
}
