import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAppMenus } from "./appMenu";
import logo from "./assets/logo.png";
import { Toasts } from "./components/Common";
import { UpdateBanner } from "./components/UpdateBanner";
import { PAGES, Sidebar, type Page } from "./components/Sidebar";
import { api } from "./lib/api";
import { runHealthCheck, startJobs } from "./lib/jobs";
import { refreshAppInfo, toastError, useAppInfo } from "./lib/store";
import { checkAndDownload, dailyCheckDue } from "./lib/updates";
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
import { ShellDialogs } from "./shell/Dialogs";
import { MenuBar } from "./shell/MenuBar";
import { applySavedZoom } from "./shell/shell";

export default function App() {
  const [page, setPageRaw] = useState<Page>("overview");
  const setPage = useCallback((p: Page) => setPageRaw(p), []);
  const menus = useAppMenus(setPage);
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
  const info = useAppInfo();
  const theme = info?.settings.theme ?? "system";
  const autoUpdate = !!info?.installed && !!info?.settings.auto_update;

  // Automatic updates (installed copy): soon after start, then daily while
  // DeskMedic keeps running in the tray. Downloads quietly, then the banner
  // (and a popup, for when only the tray icon is showing) offers a restart.
  useEffect(() => {
    if (!autoUpdate) return;
    const run = () => {
      if (!dailyCheckDue()) return;
      void checkAndDownload(false).then((v) => {
        if (v) void api.notifyUpdate(v).catch(() => undefined);
      });
    };
    const first = window.setTimeout(run, 30_000);
    const every = window.setInterval(run, 3 * 3600 * 1000);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, [autoUpdate]);

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

  useEffect(() => {
    void applySavedZoom().catch(() => undefined);
  }, []);

  // The window title names the page (and shows when running as administrator).
  const elevated = !!info?.elevated;
  useEffect(() => {
    const where = PAGES.find((p) => p.page === page)?.label ?? "";
    const name = elevated ? "DeskMedic (Administrator)" : "DeskMedic";
    void getCurrentWindow()
      .setTitle(page === "overview" ? name : `${where} — ${name}`)
      .catch(() => undefined);
  }, [page, elevated]);

  // "system" leaves the attribute off so the CSS media query decides.
  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="app-frame">
      <MenuBar appName="DeskMedic" menus={menus} onError={toastError} />
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
      </div>
      <ShellDialogs
        menus={menus}
        onError={toastError}
        about={{
          name: "DeskMedic",
          logo,
          version: info?.version ?? "",
          tagline: "A first-aid kit for Windows PCs: health check, disk map, safe cleanup, network doctor and one-click remedies.",
          links: [
            { label: "Website", open: () => void api.openLink("site").catch(toastError) },
            { label: "Source code", open: () => void api.openLink("repo").catch(toastError) },
            { label: "What's new", open: () => void api.openLink("releases").catch(toastError) },
            { label: "Report a problem", open: () => void api.openLink("issues").catch(toastError) },
          ],
        }}
      />
      <UpdateBanner app="DeskMedic" />
      <Toasts />
    </div>
  );
}
