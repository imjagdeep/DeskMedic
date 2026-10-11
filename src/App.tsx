import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { Toasts } from "./components/Common";
import { Sidebar, type Page } from "./components/Sidebar";
import { runHealthCheck, startJobs } from "./lib/jobs";
import { refreshAppInfo, useAppInfo } from "./lib/store";
import { Cleanup } from "./pages/Cleanup";
import { DiskMap } from "./pages/DiskMap";
import { Health } from "./pages/Health";
import { Disks } from "./pages/Disks";
import { Log } from "./pages/Log";
import { Network } from "./pages/Network";
import { Overview } from "./pages/Overview";
import { Remedies } from "./pages/Remedies";
import { SettingsPage } from "./pages/Settings";
import type { HealthAction } from "./lib/types";

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  // A remedy the Health check asked to open on the Remedies page.
  const [askRemedy, setAskRemedy] = useState<string | null>(null);
  const clearAsk = useCallback(() => setAskRemedy(null), []);
  const onHealthAction = (a: HealthAction) => {
    if (a.type === "remedy") {
      setAskRemedy(a.id);
      setPage("remedies");
    } else setPage(a.type === "open_cleanup" ? "cleanup" : "network");
  };
  const content = useRef<HTMLElement>(null);

  // Each page opens at the top, not where the last one was scrolled to.
  useEffect(() => {
    content.current?.scrollTo(0, 0);
  }, [page]);
  const theme = useAppInfo()?.settings.theme ?? "system";

  useEffect(() => {
    void refreshAppInfo();
    startJobs();
    // Tray menu: "Run a health check", "Network doctor".
    const un = listen<string>("navigate", (e) => {
      if (e.payload === "health-run") {
        setPage("health");
        void runHealthCheck();
      } else if (e.payload === "network") setPage("network");
      else if (e.payload === "cleanup") setPage("cleanup");
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // "system" leaves the attribute off so the CSS media query decides.
  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="app">
      <Sidebar page={page} onNavigate={setPage} />
      <main className="content" ref={content}>
        {page === "overview" && <Overview onNavigate={setPage} />}
        {page === "health" && <Health onAction={onHealthAction} />}
        {page === "network" && <Network onAction={onHealthAction} />}
        {page === "diskmap" && <DiskMap />}
        {page === "cleanup" && <Cleanup />}
        {page === "disks" && <Disks onOpenCleanup={() => setPage("cleanup")} />}
        {page === "remedies" && <Remedies ask={askRemedy} onAsked={clearAsk} />}
        {page === "log" && <Log />}
        {page === "settings" && <SettingsPage />}
      </main>
      <Toasts />
    </div>
  );
}
