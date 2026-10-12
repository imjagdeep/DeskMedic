// DeskMedic's menus: File, Edit, View, Tools, Help.

import { exit } from "@tauri-apps/plugin-process";
import { useEffect, useMemo, useState } from "react";
import { PAGES, type Page } from "./components/Sidebar";
import { api, type LabTool } from "./lib/api";
import { runHealthCheck } from "./lib/jobs";
import { refreshAppInfo, toastError, useAppInfo } from "./lib/store";
import type { Theme } from "./lib/types";
import { checkAndDownload } from "./lib/updates";
import { exportLogFile } from "./pages/Log";
import type { MenuDef } from "./shell/menu";
import {
  editCommand,
  isCompactSidebar,
  openShellDialog,
  pasteText,
  setZoom,
  textFieldFocused,
  toggleFullscreen,
  toggleSidebar,
  useCompactSidebar,
  zoomStep,
} from "./shell/shell";

const LAB_NAMES: Record<string, string> = { deskzero: "DeskZero", deskfolio: "DeskFolio" };

export function useAppMenus(go: (p: Page) => void): MenuDef[] {
  const info = useAppInfo();
  const compact = useCompactSidebar();
  const [lab, setLab] = useState<LabTool[]>([]);
  useEffect(() => {
    api.desklabTools().then(setLab).catch(toastError);
  }, []);

  return useMemo(() => {
    const settings = info?.settings;
    const setTheme = async (theme: Theme) => {
      if (!settings) return;
      await api.saveSettings({ ...settings, theme });
      await refreshAppInfo();
    };
    const themeItem = (t: Theme, label: string) => ({
      id: `theme-${t}`,
      label,
      checked: () => (settings?.theme ?? "system") === t,
      run: () => setTheme(t),
    });

    const file: MenuDef = {
      label: "File",
      items: [
        // Send to IT lives on the Overview page; this opens its explanation there.
        { id: "send-to-it", label: "Send to IT…", run: () => (go("overview"), setTimeout(() => window.dispatchEvent(new Event("send-to-it")), 60)) },
        { id: "export-log", label: "Export log for a ticket…", run: exportLogFile },
        { id: "data-folder", label: "Open data folder", run: () => api.openDataFolder() },
        "-",
        { id: "admin", label: "Restart as administrator", enabled: () => !!info && !info.elevated, run: () => api.restartAsAdmin() },
        { id: "settings", label: "Settings…", accel: "Mod+,", run: () => go("settings") },
        "-",
        // Closing the window keeps DeskMedic in the tray; this quits for real.
        { id: "exit", label: "Exit DeskMedic", run: () => exit(0) },
      ],
    };

    const edit: MenuDef = {
      label: "Edit",
      items: [
        { id: "undo", label: "Undo", accel: "Mod+Z", enabled: textFieldFocused, run: () => editCommand("undo") },
        { id: "redo", label: "Redo", accel: "Mod+Y", native: "redo", hidden: true, run: () => editCommand("redo") },
        "-",
        { id: "cut", label: "Cut", accel: "Mod+X", native: "cut", hidden: true, run: () => editCommand("cut") },
        { id: "copy", label: "Copy", accel: "Mod+C", native: "copy", hidden: true, run: () => editCommand("copy") },
        { id: "paste", label: "Paste", accel: "Mod+V", native: "paste", hidden: true, run: pasteText },
        { id: "select-all", label: "Select all", accel: "Mod+A", native: "selectAll", hidden: true, run: () => editCommand("selectAll") },
      ],
    };

    const view: MenuDef = {
      label: "View",
      items: [
        { id: "sidebar", label: "Compact sidebar", accel: "Mod+B", checked: () => isCompactSidebar(), run: toggleSidebar },
        "-",
        { id: "zoom-in", label: "Zoom in", accel: "Mod+=", run: () => zoomStep(1) },
        { id: "zoom-out", label: "Zoom out", accel: "Mod+-", run: () => zoomStep(-1) },
        { id: "zoom-reset", label: "Actual size", accel: "Mod+0", run: () => setZoom(1) },
        "-",
        { label: "Theme", items: [themeItem("system", "Match system"), themeItem("light", "Light"), themeItem("dark", "Dark")] },
        { id: "fullscreen", label: "Full screen", accel: "F11", run: toggleFullscreen },
      ],
    };

    const tools: MenuDef = {
      label: "Tools",
      items: [
        { id: "run-health", label: "Run a health check", accel: "Mod+R", run: () => (go("health"), void runHealthCheck()) },
        "-",
        ...PAGES.filter((p) => p.page !== "settings").map((p, i) => ({
          id: `go-${p.page}`,
          label: p.label,
          accel: i < 9 ? `Mod+${i + 1}` : undefined,
          run: () => go(p.page),
        })),
      ],
    };

    const help: MenuDef = {
      label: "Help",
      items: [
        { id: "palette", label: "Search commands…", accel: "Mod+K", run: () => openShellDialog("palette") },
        { id: "shortcuts", label: "Keyboard shortcuts", accel: "F1", run: () => openShellDialog("shortcuts") },
        "-",
        {
          id: "updates",
          label: "Check for updates…",
          // The portable exe doesn't update itself: show the download page.
          run: () => (info?.installed ? (go("settings"), void checkAndDownload(true)) : api.openLink("releases")),
        },
        { id: "whats-new", label: "What's new", run: () => api.openLink("releases") },
        { id: "report", label: "Report a problem…", run: () => api.openLink("issues") },
        {
          label: "More DeskLab tools",
          items: lab.map((t) => ({
            id: `lab-${t.id}`,
            label: t.installed ? `Open ${LAB_NAMES[t.id] ?? t.id}` : `Get ${LAB_NAMES[t.id] ?? t.id}`,
            run: () => (t.installed ? api.openTool(t.id) : api.openLink(t.id)),
          })),
        },
        "-",
        { id: "about", label: "About DeskMedic", run: () => openShellDialog("about") },
      ],
    };
    return [file, edit, view, tools, help];
  }, [info, go, lab, compact]);
}
