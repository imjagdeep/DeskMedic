import logo from "../assets/logo.png";
import { useAppInfo } from "../lib/store";
import { LogIcon, MapIcon, OverviewIcon, SettingsIcon } from "./Icons";

export type Page = "overview" | "diskmap" | "log" | "settings";

type Item = { page: Page; label: string; Icon: (p: { size?: number }) => React.ReactElement };

const GROUPS: { title: string; items: Item[] }[] = [
  {
    title: "This PC",
    items: [
      { page: "overview", label: "Overview", Icon: OverviewIcon },
      { page: "diskmap", label: "Disk map", Icon: MapIcon },
    ],
  },
  {
    title: "App",
    items: [
      { page: "log", label: "Log", Icon: LogIcon },
      { page: "settings", label: "Settings", Icon: SettingsIcon },
    ],
  },
];

export function Sidebar({ page, onNavigate }: { page: Page; onNavigate: (p: Page) => void }) {
  const info = useAppInfo();
  return (
    <nav className="sidebar">
      <div className="brand">
        <img src={logo} alt="" className="brand-logo" />
        <span>DeskMedic</span>
      </div>
      {GROUPS.map((g) => (
        <div className="nav-group" key={g.title}>
          <div className="nav-title">{g.title}</div>
          {g.items.map(({ page: p, label, Icon }) => (
            <button
              key={p}
              className={page === p ? "nav-item active" : "nav-item"}
              aria-current={page === p ? "page" : undefined}
              onClick={() => onNavigate(p)}
            >
              <Icon size={18} />
              <span className="nav-label">{label}</span>
            </button>
          ))}
        </div>
      ))}
      <div className="sidebar-foot">
        <span className={info?.elevated ? "dot on" : "dot paused"} />
        {info ? (info.elevated ? "Administrator" : "Standard user") : "…"}
        {info && <span className="muted ellipsis" title={info.computer}> · {info.computer}</span>}
      </div>
    </nav>
  );
}
