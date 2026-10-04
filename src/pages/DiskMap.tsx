import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { FolderIcon } from "../components/Icons";
import { api } from "../lib/api";
import { dateTime, humanSize, percent } from "../lib/format";
import { toastError, useAppInfo } from "../lib/store";
import { squarify } from "../lib/treemap";
import type {
  BigFile,
  Drive,
  Listing,
  Profile,
  ScanProgress,
  ScanSummary,
  TreeItem,
  TypeTotal,
} from "../lib/types";

type Tab = "folders" | "files" | "types" | "profiles";

export function DiskMap() {
  const info = useAppInfo();
  const [drives, setDrives] = useState<Drive[]>([]);
  const [root, setRoot] = useState("");
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [tab, setTab] = useState<Tab>("folders");
  const [listing, setListing] = useState<Listing | null>(null);

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
    // A scan from earlier in this session is still in memory.
    api
      .scanSummary()
      .then((s) => {
        if (s) {
          setSummary(s);
          setRoot(s.root);
          openFolder(null);
        }
      })
      .catch(toastError);

    const subs = [
      listen<ScanProgress>("scan-progress", (e) => setProgress(e.payload)),
      listen<ScanSummary>("scan-done", (e) => {
        setScanning(false);
        setProgress(null);
        setSummary(e.payload);
        setTab("folders");
        openFolder(null);
      }),
      listen<string>("scan-failed", (e) => {
        setScanning(false);
        setProgress(null);
        toastError(e.payload);
      }),
    ];
    return () => subs.forEach((s) => void s.then((f) => f()));
  }, [openFolder]);

  const start = async () => {
    try {
      setProgress({ files: 0, bytes: 0 });
      setScanning(true);
      await api.scanStart(root);
    } catch (e) {
      setScanning(false);
      setProgress(null);
      toastError(e);
    }
  };

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
          <button className="primary" onClick={start} disabled={!root}>
            {summary ? "Scan again" : "Scan"}
          </button>
        )}
      </PageHeader>
      <AdminNotice need="Without it the scan goes folder by folder (slower) and skips folders you can't open." />

      {scanning && progress && (
        <div className="card scan-progress">
          <div className="spinner" aria-hidden="true" />
          <div>
            <strong>Scanning {root}…</strong>
            <div className="muted small">
              {progress.files.toLocaleString()} files · {humanSize(progress.bytes)}
            </div>
          </div>
        </div>
      )}

      {!scanning && !summary && (
        <div className="empty">
          Pick a drive and press Scan to see which folders and files take the space.
          {info?.elevated && " As administrator, NTFS drives are read straight from the file table: usually seconds."}
        </div>
      )}

      {summary && !scanning && (
        <>
          <p className="muted small">
            {summary.root} · {summary.files.toLocaleString()} files in {summary.folders.toLocaleString()} folders ·{" "}
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
        </>
      )}
    </div>
  );
}

function Folders({ listing, onOpen }: { listing: Listing; onOpen: (id: number) => void }) {
  return (
    <>
      <nav className="crumbs" aria-label="Folder path">
        {listing.trail.map((t, i) => (
          <span key={t.id}>
            {i > 0 && <span className="crumb-sep">›</span>}
            {i < listing.trail.length - 1 ? (
              <button className="link" onClick={() => onOpen(t.id)}>{t.name.replace(/\\$/, "")}</button>
            ) : (
              <strong>{t.name.replace(/\\$/, "")}</strong>
            )}
          </span>
        ))}
        <span className="muted small"> · {humanSize(listing.size)}</span>
      </nav>
      <Treemap listing={listing} onOpen={onOpen} />
      <div className="card list">
        {listing.children.map((c) => (
          <Row key={c.id} item={c} parentSize={listing.size} onOpen={onOpen} />
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

function Row({ item, parentSize, onOpen }: { item: TreeItem; parentSize: number; onOpen: (id: number) => void }) {
  const pct = percent(item.size, parentSize);
  return (
    <div className="list-row map-row">
      {item.is_dir ? (
        <button className="link name-col" onClick={() => onOpen(item.id)} title="Open folder">
          <FolderIcon size={15} /> {item.name}
        </button>
      ) : (
        <span className="name-col">{item.name}</span>
      )}
      <div className="usage-bar small-bar" aria-hidden="true">
        <span style={{ width: `${Math.max(pct, item.size > 0 ? 1 : 0)}%` }} />
      </div>
      <span className="pct-col muted small">{pct}%</span>
      <span className="size-col">{humanSize(item.size)}</span>
      <span className="files-col muted small">{item.is_dir ? `${item.files.toLocaleString()} files` : ""}</span>
      <button className="link" onClick={() => api.revealNode(item.id).catch(toastError)}>Show</button>
    </div>
  );
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
  const height = 300;
  const tiles = useMemo(() => {
    const items = listing.children.filter((c) => c.size > 0).slice(0, 80);
    return squarify(
      items.map((c) => ({ item: c, size: c.size })),
      { x: 0, y: 0, w: width, h: height },
    );
  }, [listing, width]);

  return (
    <div className="treemap" ref={ref} style={{ height }}>
      {tiles.map((t, i) => {
        const big = t.w > 70 && t.h > 34;
        return (
          <div
            key={t.item.id}
            className={t.item.is_dir ? "tile dir" : "tile file"}
            style={{ left: t.x, top: t.y, width: t.w, height: t.h, ["--hue" as string]: (i * 37) % 360 }}
            title={`${t.item.name} · ${humanSize(t.item.size)}`}
            onClick={() => t.item.is_dir && onOpen(t.item.id)}
          >
            {big && (
              <>
                <span className="tile-name">{t.item.name}</span>
                <span className="tile-size">{humanSize(t.item.size)}</span>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}

function LargestFiles() {
  const [files, setFiles] = useState<BigFile[] | null>(null);
  useEffect(() => {
    api.scanTopFiles(50).then(setFiles).catch(toastError);
  }, []);
  if (!files) return <div className="empty">Loading…</div>;
  return (
    <table className="table fixed">
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
        {files.map((f) => (
          <tr key={f.id}>
            <td className="path-cell" title={f.path}>{f.path}</td>
            <td className="num">{humanSize(f.size)}</td>
            <td>
              <button className="link" onClick={() => api.revealNode(f.id).catch(toastError)}>Show</button>
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
    <div className="card list">
      {types.map((t) => {
        const pct = percent(t.size, total);
        return (
          <div className="list-row map-row" key={t.ext}>
            <span className="name-col mono">{t.ext ? `.${t.ext}` : "(no extension)"}</span>
            <div className="usage-bar small-bar" aria-hidden="true">
              <span style={{ width: `${Math.max(pct, 1)}%` }} />
            </div>
            <span className="pct-col muted small">{pct}%</span>
            <span className="size-col">{humanSize(t.size)}</span>
            <span className="files-col muted small">{t.files.toLocaleString()} files</span>
          </div>
        );
      })}
    </div>
  );
}

function Profiles({ scannedRoot }: { scannedRoot: string }) {
  const [profiles, setProfiles] = useState<Profile[] | null>(null);
  useEffect(() => {
    api.userProfiles().then(setProfiles).catch((e) => {
      setProfiles([]);
      toastError(e);
    });
  }, []);
  if (!profiles) return <div className="empty">Reading profiles…</div>;
  const days = (iso: string) => (iso ? Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000) : null);
  return (
    <>
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
    </>
  );
}
