import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAppMenus } from "./appMenu";
import logo from "./assets/logo.png";
import { Toasts } from "./components/Common";
import { UpdateBanner } from "./components/UpdateBanner";
import { PAGES, resolve, Sidebar, type Dest, type Page, type StorageTab } from "./components/Sidebar";
import { api } from "./lib/api";
import { runCheckup, startJobs } from "./lib/jobs";
import { refreshAppInfo, toastError, useAppInfo } from "./lib/store";
import { checkAndDownload, dailyCheckDue } from "./lib/updates";
import { Checkup } from "./pages/Checkup";
import { Log } from "./pages/Log";
import { Network } from "./pages/Network";
import { PENDING_FIX, Remedies } from "./pages/Remedies";
import { SettingsPage } from "./pages/Settings";
import { Storage } from "./pages/Storage";
import type { HealthAction } from "./lib/types";
import { ShellDialogs } from "./shell/Dialogs";
import { TopBar } from "./shell/TopBar";
import { applySavedZoom } from "./shell/shell";

export default function App() {
  const [page, setPageRaw] = useState<Page>("checkup");
  const [storageTab, setStorageTab] = useState<StorageTab>("diskmap");
  // Every link goes through here, so older names ("health", "cleanup") still land right.
  const go = useCallback((d: Dest) => {
    const { page, tab } = resolve(d);
    if (tab) setStorageTab(tab);
    setPageRaw(page);
  }, []);
  const menus = useAppMenus(go);
  // A one-click fix a Checkup card asked to open on the Fixes page.
  const [askRemedy, setAskRemedy] = useState<string | null>(null);
  const clearAsk = useCallback(() => setAskRemedy(null), []);
  const onHealthAction = (a: HealthAction) => {
    if (a.type === "remedy") {
      setAskRemedy(a.id);
      go("remedies");
    } else go(a.type === "open_cleanup" ? "cleanup" : "network");
  };
  const content = useRef<HTMLElement>(null);
  const info = useAppInfo();

  // A fix asked for from Checkup that needed administrator rights: DeskMedic restarted
  // elevated, so open that fix now (only within a few minutes of asking).
  useEffect(() => {
    if (!info?.elevated) return;
    let pending: { id?: unknown; at?: unknown } | null = null;
    try {
      pending = JSON.parse(localStorage.getItem(PENDING_FIX) ?? "null");
      localStorage.removeItem(PENDING_FIX);
    } catch {
      pending = null;
    }
    if (pending && typeof pending.id === "string" && typeof pending.at === "number" && Date.now() - pending.at < 10 * 60_000) {
      setAskRemedy(pending.id);
      go("remedies");
    }
  }, [info?.elevated, go]);

  // Each page opens at the top, not where the last one was scrolled to.
  useEffect(() => {
    content.current?.scrollTo(0, 0);
  }, [page]);
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
    // Tray menu: "Check this PC", "Network doctor", "Clean up temporary files".
    const un = listen<string>("navigate", (e) => {
      if (e.payload === "health-run") {
        go("checkup");
        void runCheckup();
      } else if (e.payload === "network") go("network");
      else if (e.payload === "cleanup") go("cleanup");
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
      .setTitle(page === "checkup" ? name : `${where} — ${name}`)
      .catch(() => undefined);
  }, [page, elevated]);

  // "system" leaves the attribute off so the CSS media query decides.
  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="app-frame">
      <TopBar appName="DeskMedic" logo={logo} title={PAGES.find((p) => p.page === page)?.label} menus={menus} onError={toastError} />
      <div className="app">
        <Sidebar page={page} onNavigate={go} />
        <main className="content" ref={content}>
          {page === "checkup" && <Checkup onAction={onHealthAction} onNavigate={go} />}
          {page === "network" && <Network onAction={onHealthAction} />}
          {page === "storage" && <Storage tab={storageTab} onTab={setStorageTab} />}
          {page === "remedies" && <Remedies ask={askRemedy} onAsked={clearAsk} onNavigate={go} />}
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
          tagline: "A first-aid kit for Windows PCs: a checkup that reads Windows' own logs, network doctor, storage and safe cleanup, and one-click fixes.",
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
