import { Embedded, PageHeader } from "../components/Common";
import { STORAGE_TABS, type StorageTab } from "../components/Sidebar";
import { Cleanup } from "./Cleanup";
import { DiskMap } from "./DiskMap";
import { Disks } from "./Disks";

/** Storage: what fills a drive, clearing space safely, and the drives themselves, as tabs. */
export function Storage({ tab, onTab }: { tab: StorageTab; onTab: (t: StorageTab) => void }) {
  return (
    <div className="page wide">
      <PageHeader title="Storage" />
      <div className="segmented storage-tabs" role="tablist" aria-label="Storage">
        {STORAGE_TABS.map((t) => (
          <button
            key={t.tab}
            role="tab"
            aria-selected={tab === t.tab}
            className={tab === t.tab ? "active" : ""}
            onClick={() => onTab(t.tab)}
          >
            {t.label}
          </button>
        ))}
      </div>
      {/* The tab pages keep their own buttons; the tabs above stand in for their titles. */}
      <Embedded.Provider value={true}>
        <div className="storage-body">
          {tab === "diskmap" && <DiskMap />}
          {tab === "cleanup" && <Cleanup />}
          {tab === "disks" && <Disks onOpenCleanup={() => onTab("cleanup")} />}
        </div>
      </Embedded.Provider>
    </div>
  );
}
