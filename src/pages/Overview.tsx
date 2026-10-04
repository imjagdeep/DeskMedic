import { useEffect, useState } from "react";
import { AdminNotice, PageHeader, UsageBar } from "../components/Common";
import { DiskIcon } from "../components/Icons";
import { api } from "../lib/api";
import { humanSize, percent } from "../lib/format";
import { toastError, useAppInfo } from "../lib/store";
import type { Drive, DriveKind } from "../lib/types";

const KIND: Record<DriveKind, string> = {
  fixed: "Local disk",
  removable: "Removable",
  network: "Network drive",
  optical: "CD/DVD",
  ram_disk: "RAM disk",
  unknown: "Drive",
};

export function Overview() {
  const info = useAppInfo();
  const [drives, setDrives] = useState<Drive[] | null>(null);

  const load = () => {
    setDrives(null);
    api.listDrives().then(setDrives).catch((e) => {
      setDrives([]);
      toastError(e);
    });
  };
  useEffect(load, []);

  return (
    <div className="page">
      <PageHeader title={info?.computer || "This PC"}>
        <button onClick={load}>Refresh</button>
      </PageHeader>
      <p className="muted">
        Signed in as {info?.user || "…"}
        {info && (info.elevated ? " · running as administrator" : " · standard user")}
      </p>
      <AdminNotice need="Fast scans, cleaning other users' files, disk tools and fix-its need administrator rights." />

      <h3>Drives</h3>
      {drives === null && <div className="empty">Reading drives…</div>}
      {drives?.length === 0 && <div className="empty">No drives found.</div>}
      {drives?.map((d) => {
        const used = d.total_bytes - d.free_bytes;
        return (
          <div className="card drive" key={d.root}>
            <div className="drive-icon"><DiskIcon size={22} /></div>
            <div className="grow">
              <div className="card-head">
                <h2>
                  {d.label || KIND[d.kind]} ({d.root.replace(/\\$/, "")})
                  {d.is_system && <span className="badge">Windows</span>}
                </h2>
                <span className="muted small">
                  {KIND[d.kind]}{d.file_system && ` · ${d.file_system}`}
                </span>
              </div>
              <UsageBar used={used} total={d.total_bytes} />
              <div className="drive-numbers small">
                <span>{humanSize(d.free_bytes)} free of {humanSize(d.total_bytes)}</span>
                <span className="muted">{percent(used, d.total_bytes)}% used</span>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
