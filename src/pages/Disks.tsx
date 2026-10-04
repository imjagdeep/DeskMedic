import { useCallback, useEffect, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { AlertIcon, CheckCircleIcon } from "../components/Icons";
import { ExtendSheet, ResizeSheet } from "../components/ResizeSheets";
import { api } from "../lib/api";
import { humanSize } from "../lib/format";
import { toast, toastError, useAppInfo } from "../lib/store";
import type { DiskAction, DiskReport, DiskView, Finding, PartKind, Segment } from "../lib/types";

const KIND: Record<PartKind, string> = {
  efi: "EFI system",
  reserved: "Microsoft reserved",
  basic: "Data",
  recovery: "Recovery",
  linux: "Linux",
  extended: "Extended",
  other: "Other",
};

/** Word the user types to confirm; mirrors `Action::confirm_word` in Rust. */
function confirmWord(a: DiskAction): string {
  switch (a.kind) {
    case "disk_online":
    case "disk_writable":
    case "partition_writable":
      return String(a.disk);
    case "set_letter":
    case "check_volume":
    case "fix_volume":
      return a.letter;
    case "restart_vds":
    case "enable_vds":
      return "VDS";
  }
}

type Pending = { action: DiskAction; title: string; explain: string };
type ResizeTarget = { disk: number; partition: number; letter: string; current: number };

export function Disks({ onOpenCleanup }: { onOpenCleanup: () => void }) {
  const info = useAppInfo();
  const [report, setReport] = useState<DiskReport | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [extendLetter, setExtendLetter] = useState<string | null>(null);
  const [resizing, setResizing] = useState<ResizeTarget | null>(null);
  // Stable callbacks: the sheets load data in effects that depend on them.
  const closeExtend = useCallback(() => setExtendLetter(null), []);
  const closeResize = useCallback(() => setResizing(null), []);

  const load = useCallback(() => {
    setReport(null);
    api.diskReport().then(setReport).catch(toastError);
  }, []);
  useEffect(load, [load]);

  const ask = (action: DiskAction, title: string, explain: string) => setPending({ action, title, explain });

  return (
    <div className="page wide">
      <PageHeader title="Disks">
        <button
          onClick={() =>
            ask(
              { kind: "restart_vds" },
              "Restart the Virtual Disk Service",
              "Fixes Disk Management stuck on \"Connecting to Virtual Disk Service\". Close Disk Management first.",
            )
          }
          disabled={!info?.elevated}
          title="For Disk Management that hangs or won't connect"
        >
          Disk Management stuck?
        </button>
        <button onClick={load} disabled={report === null}>Refresh</button>
      </PageHeader>
      <AdminNotice need="You can look without it, but every fix needs administrator rights." />

      {report === null && (
        <div className="card scan-progress">
          <div className="spinner" aria-hidden="true" />
          <span>Reading disks…</span>
        </div>
      )}

      {report && (
        <>
          <Findings
            findings={report.findings}
            canFix={!!info?.elevated}
            onFix={(f) => {
              if (!f.fix) return;
              if (f.fix.type === "open_cleanup") onOpenCleanup();
              else if (f.fix.type === "extend_guide") setExtendLetter(f.fix.letter);
              else if (f.fix.type === "action") ask(f.fix.action, f.fix.label, f.title);
            }}
          />
          {report.disks.map((d) => (
            <DiskCard
              key={d.disk.number}
              view={d}
              report={report}
              canFix={!!info?.elevated}
              ask={ask}
              onResize={setResizing}
            />
          ))}
          <p className="muted small">
            Virtual Disk Service: {report.vds_status.toLowerCase()}, starts {report.vds_start.toLowerCase()}. DeskMedic
            never formats, initializes or deletes data partitions.
          </p>
        </>
      )}

      {extendLetter && (
        <ExtendSheet
          letter={extendLetter}
          onClose={closeExtend}
          onDone={() => {
            setExtendLetter(null);
            load();
          }}
        />
      )}
      {resizing && (
        <ResizeSheet
          {...resizing}
          onClose={closeResize}
          onDone={(msg) => {
            setResizing(null);
            toast(msg);
            load();
          }}
        />
      )}

      {pending && (
        <ConfirmSheet
          pending={pending}
          onClose={() => setPending(null)}
          onDone={(msg) => {
            setPending(null);
            toast(msg);
            load();
          }}
        />
      )}
    </div>
  );
}

function Findings({
  findings,
  canFix,
  onFix,
}: {
  findings: Finding[];
  canFix: boolean;
  onFix: (f: Finding) => void;
}) {
  if (findings.length === 0) {
    return (
      <div className="card finding ok">
        <CheckCircleIcon size={20} />
        <div>
          <strong>No disk problems found.</strong>
          <div className="muted small">Disks are online, writable and healthy.</div>
        </div>
      </div>
    );
  }
  return (
    <>
      {findings.map((f, i) => (
        <div className={`card finding ${f.severity}`} key={i}>
          {f.severity === "info" ? <CheckCircleIcon size={20} /> : <AlertIcon size={20} />}
          <div className="grow">
            <strong>{f.title}</strong>
            <div className="muted small">{f.detail}</div>
          </div>
          {f.fix?.type === "open_cleanup" && <button onClick={() => onFix(f)}>Open Cleanup</button>}
          {f.fix?.type === "extend_guide" && (
            <button className="primary" disabled={!canFix} onClick={() => onFix(f)}>
              Extend…
            </button>
          )}
          {f.fix?.type === "action" && (
            <button className="primary" disabled={!canFix} onClick={() => onFix(f)}>
              {f.fix.label}
            </button>
          )}
        </div>
      ))}
    </>
  );
}

function DiskCard({
  view,
  report,
  canFix,
  ask,
  onResize,
}: {
  view: DiskView;
  report: DiskReport;
  canFix: boolean;
  ask: (a: DiskAction, title: string, explain: string) => void;
  onResize: (t: ResizeTarget) => void;
}) {
  const d = view.disk;
  const health = view.physical?.health || d.health;
  return (
    <div className="card disk-card">
      <div className="card-head">
        <h2>
          Disk {d.number} · {d.name}
        </h2>
        <span className="muted small">
          {humanSize(d.size)} · {d.style} · {d.bus}
          {view.physical?.media && view.physical.media !== "Unspecified" && ` · ${view.physical.media}`}
          {" · "}
          <span className={health === "Healthy" ? "status ok" : "status fail"}>{health || "unknown"}</span>
          {d.offline && <span className="status fail">offline</span>}
          {d.read_only && <span className="status fail">read-only</span>}
        </span>
      </div>
      <div className="disk-bar" aria-hidden="true">
        {view.segments.map((s, i) => (
          <div
            key={i}
            className={`seg ${s.type === "free" ? "free" : s.kind}`}
            style={{ flexGrow: Math.max(segSize(s) / d.size, 0.04) }}
            title={segLabel(s)}
          >
            <span>{segShort(s)}</span>
          </div>
        ))}
      </div>
      <div className="seg-list">
        {view.segments.map((s, i) => (
          <SegmentRow key={i} s={s} report={report} canFix={canFix} ask={ask} onResize={onResize} />
        ))}
      </div>
    </div>
  );
}

function segSize(s: Segment): number {
  return s.type === "free" ? s.size : s.partition.size;
}

function segShort(s: Segment): string {
  if (s.type === "free") return humanSize(s.size);
  if (s.volume?.letter) return `${s.volume.letter}: ${humanSize(s.partition.size)}`;
  return humanSize(s.partition.size);
}

function segLabel(s: Segment): string {
  if (s.type === "free") return `Unallocated · ${humanSize(s.size)}`;
  return `${KIND[s.kind]} · ${humanSize(s.partition.size)}`;
}

function SegmentRow({
  s,
  report,
  canFix,
  ask,
  onResize,
}: {
  s: Segment;
  report: DiskReport;
  canFix: boolean;
  ask: (a: DiskAction, title: string, explain: string) => void;
  onResize: (t: ResizeTarget) => void;
}) {
  if (s.type === "free") {
    return (
      <div className="list-row">
        <span>
          <span className="seg-dot free" /> Unallocated
        </span>
        <span className="size-col">{humanSize(s.size)}</span>
        <span className="seg-actions" />
      </div>
    );
  }
  const p = s.partition;
  const v = s.volume;
  const letter = v?.letter || "";
  const isWindows = letter.toUpperCase() === report.windows_letter.toUpperCase();
  const free = "DEFGHIJKLMNOPQRSTUVWXYZ".split("").filter((c) => !report.letters_in_use.includes(c));
  return (
    <div className="list-row">
      <span>
        <span className={`seg-dot ${s.kind}`} /> {letter ? `${letter}:` : `Partition ${p.number}`}
        {v?.label && ` "${v.label}"`} <span className="muted">· {KIND[s.kind]}</span>
        {v?.fs && <span className="muted"> · {v.fs}</span>}
        {isWindows && <span className="badge">Windows</span>}
      </span>
      <span className="size-col">
        {v && v.size > 0 ? `${humanSize(v.free)} free of ${humanSize(v.size)}` : humanSize(p.size)}
      </span>
      <span className="seg-actions">
        {letter && v?.fs && (
          <>
            <button
              className="link"
              disabled={!canFix}
              onClick={() =>
                ask(
                  { kind: "check_volume", letter },
                  `Check ${letter}: for errors`,
                  "A read-only scan. Nothing is changed; any errors found are listed for repair.",
                )
              }
            >
              Check
            </button>
            <button
              className="link"
              disabled={!canFix}
              onClick={() =>
                ask(
                  { kind: "fix_volume", letter },
                  `Repair ${letter}:`,
                  isWindows
                    ? "Windows can't repair the drive it runs from while running. It will check and repair it on the next restart (this can take a while)."
                    : `${letter}: goes offline for a moment while it is repaired. Close any files open on it first.`,
                )
              }
            >
              Repair
            </button>
            {s.kind === "basic" && (
              <button
                className="link"
                disabled={!canFix}
                onClick={() => onResize({ disk: p.disk, partition: p.number, letter, current: p.size })}
              >
                Resize
              </button>
            )}
          </>
        )}
        {s.kind === "basic" && v?.fs && !isWindows && !p.boot && !p.system && free.length > 0 && (
          <select
            className="small-select"
            disabled={!canFix}
            value=""
            aria-label="Change drive letter"
            onChange={(e) =>
              e.target.value &&
              ask(
                { kind: "set_letter", disk: p.disk, partition: p.number, letter: e.target.value },
                `Give this partition the letter ${e.target.value}:`,
                letter
                  ? `Programs and shortcuts that use ${letter}: will need updating.`
                  : "It will show up in Explorer.",
              )
            }
          >
            <option value="">{letter ? "Change letter…" : "Add letter…"}</option>
            {free.map((c) => (
              <option key={c} value={c}>
                {c}:
              </option>
            ))}
          </select>
        )}
      </span>
    </div>
  );
}

function ConfirmSheet({
  pending,
  onClose,
  onDone,
}: {
  pending: Pending;
  onClose: () => void;
  onDone: (message: string) => void;
}) {
  const word = confirmWord(pending.action);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const ok = typed.trim().toUpperCase() === word.toUpperCase();
  const run = async () => {
    setBusy(true);
    try {
      onDone(await api.diskAction(pending.action, typed));
    } catch (e) {
      toastError(e);
      setBusy(false);
    }
  };
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="disk-confirm-h">
      <div className="modal">
        <h2 id="disk-confirm-h">{pending.title}</h2>
        <p>{pending.explain}</p>
        <label className="field">
          <span>
            Type <strong className="mono">{word}</strong> to confirm
          </span>
          <input
            autoFocus
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && ok && !busy && run()}
            disabled={busy}
          />
        </label>
        <div className="modal-actions">
          <button onClick={onClose} disabled={busy}>Cancel</button>
          <button className="primary" disabled={!ok || busy} onClick={run}>
            {busy ? "Working…" : "Go ahead"}
          </button>
        </div>
      </div>
    </div>
  );
}
