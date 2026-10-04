import { useEffect, useRef, useState } from "react";
import { Toasts } from "./components/Common";
import { Sidebar, type Page } from "./components/Sidebar";
import { startJobs } from "./lib/jobs";
import { refreshAppInfo, useAppInfo } from "./lib/store";
import { Cleanup } from "./pages/Cleanup";
import { DiskMap } from "./pages/DiskMap";
import { Disks } from "./pages/Disks";
import { FixIts } from "./pages/FixIts";
import { Log } from "./pages/Log";
import { Overview } from "./pages/Overview";
import { SettingsPage } from "./pages/Settings";

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  const content = useRef<HTMLElement>(null);

  // Each page opens at the top, not where the last one was scrolled to.
  useEffect(() => {
    content.current?.scrollTo(0, 0);
  }, [page]);
  const theme = useAppInfo()?.settings.theme ?? "system";

  useEffect(() => {
    void refreshAppInfo();
    startJobs();
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
        {page === "overview" && <Overview />}
        {page === "diskmap" && <DiskMap />}
        {page === "cleanup" && <Cleanup />}
        {page === "disks" && <Disks onOpenCleanup={() => setPage("cleanup")} />}
        {page === "fixits" && <FixIts />}
        {page === "log" && <Log />}
        {page === "settings" && <SettingsPage />}
      </main>
      <Toasts />
    </div>
  );
}
