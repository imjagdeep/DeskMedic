import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { FolderIcon, MapIcon } from "../components/Icons";
import { api } from "../lib/api";
import { dateTime, humanSize, percent } from "../lib/format";
import { startScan, useScanDone, useScanJob, type ScanJob } from "../lib/jobs";
import { toastError, useAppInfo } from "../lib/store";
import { squarify } from "../lib/treemap";
import type { BigFile, Drive, Listing, Profile, ScanSummary, TreeItem, TypeTotal } from "../lib/types";

type Tab = "folders" | "files" | "types" | "profiles";

/** "1 file" / "2 files". */
const files = (n: number) => `${n.toLocaleString()} ${n === 1 ? "file" : "files"}`;

export function DiskMap() {
  const info = useAppInfo();
  const job = useScanJob();
  const done = useScanDone();
  const [drives, setDrives] = useState<Drive[]>([]);
  const [root, setRoot] = useState("");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [tab, setTab] = useState<Tab>("folders");
  const [listing, setListing] = useState<Listing | null>(null);
  const scanning = job !== null;

  const openFolder = useCallback((id: number | null) => {
    api.scanListing(id, 200).then(setListing).catch(toastError);
  }, []);

  useEffect(() => {
    api
      .listDrives()
      .then((all) => {
        const local = all.filter((d) => d.kind === "fixed" || d.kind === "removable");
        setDrives(local);
        setRoot((r) => r || local.find((d) => d.is_system)?.root || local[0]?.root || "");
      })
      .catch(toastError);
  }, []);

  // The latest finished scan: when the page opens, and whenever a scan
  // finishes (even while you were on another page).
  useEffect(() => {
    api
      .scanSummary()
      .then((s) => {
        setSummary(s);
        if (s) {
          setRoot(s.root);
          openFolder(null);
        }
      })
      .catch(toastError);
    if (done.seq > 0) {
      setTab("folders");
      document.querySelector(".content")?.scrollTo(0, 0);
    }
  }, [done.seq, openFolder]);

  useEffect(() => {
    if (job) setRoot(job.root);
  }, [job]);

  const drive = drives.find((d) => d.root === root);

  return (
    <div className="page wide">
      <PageHeader title="Disk map">
        <select value={root} onChange={(e) => setRoot(e.target.value)} disabled={scanning} aria-label="Drive">
          {drives.map((d) => (
            <option key={d.root} value={d.root}>
              {d.root.replace(/\\$/, "")} {d.label && `(${d.label})`} · {humanSize(d.total_bytes - d.free_bytes)} used
            </option>
          ))}
        </select>
        {scanning ? (
          <button onClick={() => api.scanCancel().catch(toastError)}>Cancel</button>
        ) : (
          <button
            className="primary"
            onClick={() => drive && void startScan(drive.root, drive.total_bytes - drive.free_bytes)}
            disabled={!drive}
          >
            {summary ? "Scan again" : "Scan"}
          </button>
        )}
      </PageHeader>
      <AdminNotice need="Without it the scan goes folder by folder (slower) and skips folders you can't open." />

      {job && <ScanProgressCard job={job} />}

      {!scanning && !summary && (
        <div className="empty empty-big">
          <MapIcon size={34} />
          <div className="empty-title">See what fills a drive</div>
          <div>
            Pick a drive and press <strong>Scan</strong>.
            {info?.elevated
              ? " As administrator, NTFS drives are read straight from the file table: usually a few seconds."
              : " Restart as administrator for the fast scan."}
          </div>
        </div>
      )}

      {summary && !scanning && (
        <div className="fade-in">
          <p className="muted small">
            {summary.root} · {files(summary.files)} in {summary.folders.toLocaleString()} folders ·{" "}
            {humanSize(summary.bytes)} · {(summary.millis / 1000).toFixed(1)} s ·{" "}
            {summary.method === "mft" ? "fast NTFS scan" : "folder-by-folder scan"}
            {summary.skipped > 0 &&
              ` · ${summary.skipped.toLocaleString()} ${summary.method === "mft" ? "records" : "folders"} could not be read`}
          </p>
          <div className="segmented tabs" role="tablist">
            {(
              [
                ["folders", "Folders"],
                ["files", "Largest files"],
                ["types", "File types"],
                ["profiles", "User profiles"],
              ] as [Tab, string][]
            ).map(([t, label]) => (
              <button key={t} role="tab" aria-selected={tab === t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
                {label}
              </button>
            ))}
          </div>
          {tab === "folders" && listing && <Folders listing={listing} onOpen={openFolder} />}
          {tab === "files" && <LargestFiles />}
          {tab === "types" && <FileTypes total={summary.bytes} />}
          {tab === "profiles" && <Profiles scannedRoot={summary.root} />}
        </div>
      )}
    </div>
  );
}

function ScanProgressCard({ job }: { job: ScanJob }) {
  // Tick once a second for the elapsed time.
  const [, setNow] = useState(0);
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  const secs = Math.max(1, Math.round((Date.now() - job.started) / 1000));
  const pct = job.used > 0 ? Math.min(99, Math.floor((job.bytes / job.used) * 100)) : 0;
  return (
    <div className="card scan-card">
      <div className="scan-card-head">
        <div className="spinner" aria-hidden="true" />
        <strong className="grow">Scanning {job.root.replace(/\\$/, "")}…</strong>
        <span className="muted small tabular">
          {Math.floor(secs / 60)}:{String(secs % 60).padStart(2, "0")}
        </span>
      </div>
      <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
        <span style={{ width: `${pct}%` }} />
      </div>
      <div className="muted small tabular">
        {files(job.files)} · {humanSize(job.bytes)} of about {humanSize(job.used)} ·{" "}
        {Math.round(job.files / secs).toLocaleString()} files/s · you can use other pages meanwhile
      </div>
    </div>
  );
}

function Folders({ listing, onOpen }: { listing: Listing; onOpen: (id: number) => void }) {
  const parent = listing.trail.length > 1 ? listing.trail[listing.trail.length - 2] : null;
  return (
    <>
      <nav className="crumbs" aria-label="Folder path">
        <button className="crumb-up" disabled={!parent} onClick={() => parent && onOpen(parent.id)} title="Up one folder">
          ↑
        </button>
        {listing.trail.map((t, i) => (
          <span key={t.id}>
            {i > 0 && <span className="crumb-sep">›</span>}
            {i < listing.trail.length - 1 ? (
              <button className="link" onClick={() => onOpen(t.id)}>
                {t.name.replace(/\\$/, "")}
              </button>
            ) : (
              <strong>{t.name.replace(/\\$/, "")}</strong>
            )}
          </span>
        ))}
        <span className="muted small">
          {" "}
          · {humanSize(listing.size)} · {files(listing.files)}
        </span>
      </nav>
      <Treemap listing={listing} onOpen={onOpen} />
      <div className="card list stagger" key={listing.id}>
        {listing.children.map((c, i) => (
          <Row key={c.id} item={c} parentSize={listing.size} onOpen={onOpen} rank={i} />
        ))}
        {listing.rest_count > 0 && (
          <div className="list-row muted">
            <span>{listing.rest_count.toLocaleString()} smaller items</span>
            <span className="size-col">{humanSize(listing.rest_size)}</span>
          </div>
        )}
        {listing.children.length === 0 && <div className="muted small">This folder is empty.</div>}
      </div>
    </>
  );
}

function Row({
  item,
  parentSize,
  onOpen,
  rank,
}: {
  item: TreeItem;
  parentSize: number;
  onOpen: (id: number) => void;
  rank: number;
}) {
  const pct = percent(item.size, parentSize);
  const swatch = <span className="swatch" style={{ background: colorFor(rank, item.is_dir) }} />;
  return (
    <div className="list-row map-row" style={{ ["--i" as string]: Math.min(rank, 20) }}>
      {item.is_dir ? (
        <button className="link name-col" onClick={() => onOpen(item.id)} title="Open folder">
          {swatch}
          <FolderIcon size={15} /> {item.name}
        </button>
      ) : (
        <span className="name-col">
          {swatch}
          {item.name}
        </span>
      )}
      <div className="usage-bar small-bar" aria-hidden="true">
        <span style={{ width: `${Math.max(pct, item.size > 0 ? 1 : 0)}%` }} />
      </div>
      <span className="pct-col muted small">{pct}%</span>
      <span className="size-col">{humanSize(item.size)}</span>
      <span className="files-col muted small">{item.is_dir ? files(item.files) : ""}</span>
      <button className="link" onClick={() => api.revealNode(item.id).catch(toastError)}>
        Show
      </button>
    </div>
  );
}

/** One calm palette for folders, by size rank so a list swatch matches its tile. Files are grey. */
const PALETTE = ["#3b82f6", "#8b5cf6", "#0ea5e9", "#10b981", "#f59e0b", "#ec4899", "#6366f1", "#14b8a6", "#f97316", "#84cc16"];
function colorFor(rank: number, isDir: boolean): string {
  return isDir ? PALETTE[rank % PALETTE.length] : "#94a3b8";
}

function Treemap({ listing, onOpen }: { listing: Listing; onOpen: (id: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const height = 320;
  const tiles = useMemo(() => {
    const items = listing.children.filter((c) => c.size > 0).slice(0, 80);
    return squarify(
      items.map((c, rank) => ({ item: { ...c, rank }, size: c.size })),
      { x: 0, y: 0, w: width, h: height },
    );
  }, [listing, width]);

  return (
    <div className="treemap" ref={ref} style={{ height }}>
      <div className="treemap-inner" key={listing.id}>
        {tiles.map((t) => {
          const big = t.w > 84 && t.h > 40;
          const pct = percent(t.item.size, listing.size);
          return (
            <div
              key={t.item.id}
              className={t.item.is_dir ? "tile dir" : "tile file"}
              style={{ left: t.x, top: t.y, width: t.w, height: t.h, ["--c" as string]: colorFor(t.item.rank, t.item.is_dir) }}
              title={`${t.item.name} · ${humanSize(t.item.size)} · ${pct}%${t.item.is_dir ? " · click to open" : ""}`}
              onClick={() => t.item.is_dir && onOpen(t.item.id)}
            >
              {big && (
                <>
                  <span className="tile-name">{t.item.name}</span>
                  <span className="tile-size">
                    {humanSize(t.item.size)} · {pct}%
                  </span>
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function LargestFiles() {
  const [list, setList] = useState<BigFile[] | null>(null);
  useEffect(() => {
    api.scanTopFiles(50).then(setList).catch(toastError);
  }, []);
  if (!list) return <div className="empty">Loading…</div>;
  return (
    <table className="table fixed fade-in">
      <colgroup>
        <col />
        <col style={{ width: 100 }} />
        <col style={{ width: 64 }} />
      </colgroup>
      <thead>
        <tr>
          <th>File</th>
          <th className="num">Size</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {list.map((f) => (
          <tr key={f.id}>
            <td className="path-cell" title={f.path}>
              {f.path}
            </td>
            <td className="num">{humanSize(f.size)}</td>
            <td>
              <button className="link" onClick={() => api.revealNode(f.id).catch(toastError)}>
                Show
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function FileTypes({ total }: { total: number }) {
  const [types, setTypes] = useState<TypeTotal[] | null>(null);
  useEffect(() => {
    api.scanTypes(40).then(setTypes).catch(toastError);
  }, []);
  if (!types) return <div className="empty">Loading…</div>;
  return (
    <div className="card list stagger">
      {types.map((t, i) => {
        const pct = percent(t.size, total);
        return (
          <div className="list-row map-row" key={t.ext} style={{ ["--i" as string]: Math.min(i, 20) }}>
            <span className="name-col mono">{t.ext ? `.${t.ext}` : "(no extension)"}</span>
            <div className="usage-bar small-bar" aria-hidden="true">
              <span style={{ width: `${Math.max(pct, 1)}%` }} />
            </div>
            <span className="pct-col muted small">{pct}%</span>
            <span className="size-col">{humanSize(t.size)}</span>
            <span className="files-col muted small">{files(t.files)}</span>
          </div>
        );
      })}
    </div>
  );
}

function Profiles({ scannedRoot }: { scannedRoot: string }) {
  const [profiles, setProfiles] = useState<Profile[] | null>(null);
  useEffect(() => {
    api
      .userProfiles()
      .then(setProfiles)
      .catch((e) => {
        setProfiles([]);
        toastError(e);
      });
  }, []);
  if (!profiles) return <div className="empty">Reading profiles…</div>;
  const days = (iso: string) => (iso ? Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000) : null);
  return (
    <div className="fade-in">
      <p className="muted small">
        Oldest first. Old profiles on shared PCs often hold gigabytes. Sizes come from the scan of {scannedRoot}.
      </p>
      <table className="table">
        <thead>
          <tr>
            <th>Profile</th>
            <th>Last used</th>
            <th />
            <th className="num">Size</th>
          </tr>
        </thead>
        <tbody>
          {profiles.map((p) => {
            const d = days(p.last_used);
            return (
              <tr key={p.sid}>
                <td title={p.sid}>{p.path}</td>
                <td>
                  {p.last_used ? dateTime(p.last_used) : "unknown"}
                  {d !== null && d > 90 && <span className="badge warn">{d} days ago</span>}
                </td>
                <td>{p.loaded && <span className="badge">signed in</span>}</td>
                <td className="num">{p.size === null ? "—" : humanSize(p.size)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
