import { useEffect, useMemo, useRef, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { AppsIcon, CategoryIcon, GlobeIcon, LockIcon, PrinterIcon, ShieldIcon, WindowIcon } from "../components/Icons";
import { PAGES, type Page } from "../components/Sidebar";
import { api } from "../lib/api";
import { clearFixResult, runFix, useFixJob } from "../lib/jobs";
import { index, search } from "../lib/remedySearch";
import { toastError, useAppInfo } from "../lib/store";
import type { FixRecipe, Remedy, RemedyLibrary } from "../lib/types";

type Group = { title: string; Icon: (p: { size?: number }) => React.ReactElement; ids: string[] };

// Remedy ids by topic; anything new and unlisted lands in "More".
const GROUPS: Group[] = [
  { title: "Internet and network", Icon: GlobeIcon, ids: ["flush-dns", "reset-network"] },
  { title: "Windows", Icon: WindowIcon, ids: ["restart-explorer", "rebuild-icon-cache", "rebuild-search", "time-sync", "gpupdate"] },
  { title: "Apps", Icon: AppsIcon, ids: ["reset-teams", "reset-onedrive", "reset-store"] },
  { title: "Printing and sound", Icon: PrinterIcon, ids: ["print-queue", "restart-audio"] },
  { title: "System repair", Icon: ShieldIcon, ids: ["repair-windows-update", "sfc", "dism-restore"] },
];

/** `ask`: a one-click remedy to open straight away (from the Health check). */
export function Remedies({
  ask,
  onAsked,
  onNavigate,
}: {
  ask?: string | null;
  onAsked?: () => void;
  onNavigate?: (p: Page) => void;
}) {
  const info = useAppInfo();
  const [recipes, setRecipes] = useState<FixRecipe[]>([]);
  const [lib, setLib] = useState<RemedyLibrary | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const [open, setOpen] = useState<Remedy | null>(null);
  const top = useRef<HTMLDivElement>(null);
  const idx = useMemo(() => index(lib?.remedies ?? []), [lib]);
  const found = useMemo(() => search(idx, query), [idx, query]);
  const [asking, setAsking] = useState<FixRecipe | null>(null);
  // Restore point first, for remedies that offer one. On by default.
  const [restorePoint, setRestorePoint] = useState(true);
  // Kept outside the page so leaving it mid-fix loses nothing.
  const { running, result } = useFixJob();

  useEffect(() => {
    api.fixList().then(setRecipes).catch(toastError);
    api.remedyLibrary().then(setLib).catch(toastError);
  }, []);

  const show = (r: Remedy | null) => {
    setOpen(r);
    top.current?.scrollIntoView({ block: "start" });
  };
  const askFix = (id: string) => {
    const r = recipes.find((x) => x.id === id);
    if (!r) return;
    setRestorePoint(true);
    setAsking(r);
  };
  const byId = (id: string) => lib?.remedies.find((r) => r.id === id);
  const inCategory = lib && category ? lib.remedies.filter((r) => r.category === category) : [];
  const cat = lib?.categories.find((c) => c.id === category);

  useEffect(() => {
    if (!ask || recipes.length === 0) return;
    const r = recipes.find((x) => x.id === ask);
    if (r && !(r.admin && !info?.elevated)) {
      setRestorePoint(true);
      setAsking(r);
    }
    onAsked?.();
  }, [ask, recipes, info, onAsked]);

  const run = (r: FixRecipe) => {
    setAsking(null);
    void runFix(r.id, r.restore_point && restorePoint);
  };

  return (
    <div className="page wide" ref={top}>
      <PageHeader title="Remedies" />
      <div className="remedy-search">
        <input
          type="search"
          value={query}
          placeholder={`Describe the problem: "printer offline", "no sound in Teams", "Wi-Fi drops"…`}
          aria-label="Search remedies"
          autoFocus
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(null);
          }}
        />
        {lib && (
          <span className="muted small">
            {lib.remedies.length} remedies in {lib.categories.length} categories
          </span>
        )}
      </div>

      {open ? (
        <RemedyView
          r={open}
          recipes={recipes}
          elevated={!!info?.elevated}
          running={running}
          related={open.related.map(byId).filter((x): x is Remedy => !!x)}
          onBack={() => show(null)}
          onShow={show}
          onFix={askFix}
          onPage={(p) => onNavigate?.(p as Page)}
        />
      ) : query.trim() ? (
        <section>
          <h3 className="fix-group-title">{found.length ? `Best matches for "${query.trim()}"` : `Nothing found for "${query.trim()}"`}</h3>
          {found.length === 0 && <p className="muted">Try fewer or different words (what you see, like "offline" or "no sound"), or pick a category below.</p>}
          <RemedyList items={found} onShow={show} />
          {found.length === 0 && lib && <CategoryGrid lib={lib} onPick={(c) => { setQuery(""); setCategory(c); }} />}
        </section>
      ) : category && cat ? (
        <section>
          <button className="link" onClick={() => setCategory(null)}>‹ All categories</button>
          <h2 className="remedy-cat-title">
            <CategoryIcon name={cat.icon} size={20} /> {cat.title}
          </h2>
          <p className="muted">{cat.blurb}</p>
          <RemedyList items={inCategory} onShow={show} />
        </section>
      ) : (
        <>
          {lib && <CategoryGrid lib={lib} onPick={setCategory} />}
          <h3 className="fix-group-title remedy-oneclick-title">One-click remedies</h3>
          <p className="muted">Each shows exactly what it runs first, and every run goes in the Log.</p>
          <AdminNotice need="Many one-click remedies change system settings and need administrator rights." />
        </>
      )}
      {!open && !query.trim() && !category && [...GROUPS, { title: "More", Icon: AppsIcon, ids: recipes.map((r) => r.id).filter((id) => !GROUPS.some((g) => g.ids.includes(id))) }]
        .map((g) => ({ ...g, items: g.ids.map((id) => recipes.find((r) => r.id === id)).filter((r): r is FixRecipe => !!r) }))
        .filter((g) => g.items.length > 0)
        .map(({ title, Icon, items }) => (
          <section key={title} className="fix-group">
            <h3 className="fix-group-title">
              <span className="fix-group-icon">
                <Icon size={15} />
              </span>
              {title}
            </h3>
            <div className="fix-grid">
              {items.map((r) => {
                const blocked = r.admin && !info?.elevated;
                return (
                  <div className={running === r.id ? "card fix-card running" : "card fix-card"} key={r.id}>
                    <h2>{r.name}</h2>
                    <p className="small">{r.when}</p>
                    <p className="muted small fix-does">{r.does}</p>
                    <div className="fix-foot">
                      <span className="muted small fix-meta">
                        {r.takes}
                        {r.restart && <span className="badge warn">restart</span>}
                        {r.restore_point && <span className="badge">restore point</span>}
                      </span>
                      <button
                        className={blocked ? "with-icon" : "primary"}
                        disabled={blocked || running !== null}
                        title={blocked ? "Restart DeskMedic as administrator to run this" : undefined}
                        onClick={() => {
                          setRestorePoint(true);
                          setAsking(r);
                        }}
                      >
                        {blocked && <LockIcon size={13} />}
                        {running === r.id ? "Running…" : blocked ? "Needs admin" : "Run…"}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ))}

      {running && (
        <div className="running-bar">
          <div className="spinner" aria-hidden="true" />
          <span>
            Running {recipes.find((r) => r.id === running)?.name}… ({recipes.find((r) => r.id === running)?.takes})
          </span>
        </div>
      )}

      {asking && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-h">
          <div className="modal">
            <h2 id="fix-h">{asking.name}</h2>
            <p>{asking.does}</p>
            <ol className="steps">
              {asking.steps.map((s) => (
                <li key={s}>{s}</li>
              ))}
            </ol>
            <p className="muted small">
              Takes {asking.takes}.{asking.restart && " Restart the PC afterwards for it to take effect."}
            </p>
            {asking.restore_point && (
              <label className="check-row">
                <input type="checkbox" checked={restorePoint} onChange={(e) => setRestorePoint(e.target.checked)} />
                <span>
                  Create a restore point first (recommended)
                  <span className="muted small block">
                    A way back through System Restore if something goes wrong. Adds up to a minute.
                  </span>
                </span>
              </label>
            )}
            <div className="modal-actions">
              <button onClick={() => setAsking(null)}>Cancel</button>
              <button className="primary" autoFocus onClick={() => run(asking)}>
                Run it
              </button>
            </div>
          </div>
        </div>
      )}

      {result && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-result-h">
          <div className="modal">
            <h2 id="fix-result-h">
              <span className={result.ok ? "status ok" : "status fail"}>{result.ok ? "Done" : "Failed"}</span> {result.name}
            </h2>
            {result.steps.map((s) => (
              <div key={s.label} className="fix-step">
                <strong>{s.label}</strong>
                {s.output && <pre className="details">{s.output}</pre>}
              </div>
            ))}
            {result.restart && <p className="notice-text">Restart the PC to finish.</p>}
            <div className="modal-actions">
              <button className="primary" onClick={clearFixResult}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function CategoryGrid({ lib, onPick }: { lib: RemedyLibrary; onPick: (id: string) => void }) {
  return (
    <div className="remedy-cats">
      {lib.categories.map((c) => (
        <button key={c.id} className="card remedy-cat" onClick={() => onPick(c.id)}>
          <span className="fix-group-icon">
            <CategoryIcon name={c.icon} size={16} />
          </span>
          <span className="remedy-cat-text">
            <strong>{c.title}</strong>
            <span className="muted small">{c.blurb}</span>
          </span>
          <span className="badge">{c.count}</span>
        </button>
      ))}
    </div>
  );
}

function RemedyList({ items, onShow }: { items: Remedy[]; onShow: (r: Remedy) => void }) {
  return (
    <div className="remedy-list">
      {items.map((r) => (
        <button key={r.id} className="remedy-row" onClick={() => onShow(r)}>
          <strong>{r.title}</strong>
          <span className="muted small">{r.why}</span>
          {r.steps.some((s) => s.fix) && <span className="badge ok">one-click fix</span>}
        </button>
      ))}
    </div>
  );
}

function RemedyView({
  r,
  recipes,
  elevated,
  running,
  related,
  onBack,
  onShow,
  onFix,
  onPage,
}: {
  r: Remedy;
  recipes: FixRecipe[];
  elevated: boolean;
  running: string | null;
  related: Remedy[];
  onBack: () => void;
  onShow: (r: Remedy) => void;
  onFix: (id: string) => void;
  onPage: (page: string) => void;
}) {
  const openStep = (i: number) => {
    api.remedyOpen(r.id, i).catch(toastError);
  };
  return (
    <article className="remedy-view">
      <button className="link" onClick={onBack}>‹ Back</button>
      <h2>{r.title}</h2>
      <p className="remedy-why">{r.why}</p>
      <h3 className="fix-group-title">Try these, in order</h3>
      <ol className="remedy-steps">
        {r.steps.map((s, i) => {
          const recipe = s.fix ? recipes.find((x) => x.id === s.fix) : undefined;
          const blocked = !!recipe?.admin && !elevated;
          return (
            <li key={i}>
              <span>{s.text}</span>
              {recipe && (
                <button
                  className={blocked ? "with-icon" : "primary"}
                  disabled={blocked || running !== null}
                  title={blocked ? "Restart DeskMedic as administrator to run this" : recipe.does}
                  onClick={() => onFix(recipe.id)}
                >
                  {blocked && <LockIcon size={13} />}
                  {running === recipe.id ? "Running…" : blocked ? "Needs admin" : `Run: ${recipe.name}…`}
                </button>
              )}
              {s.open && <button onClick={() => openStep(i)}>Open</button>}
              {s.page && <button onClick={() => onPage(s.page as string)}>Go to {PAGES.find((p) => p.page === s.page)?.label ?? s.page}</button>}
            </li>
          );
        })}
      </ol>
      {r.escalate && (
        <div className="card remedy-escalate">
          <strong>Still stuck?</strong>
          <p>{r.escalate}</p>
        </div>
      )}
      {related.length > 0 && (
        <>
          <h3 className="fix-group-title">Related</h3>
          <div className="remedy-list">
            {related.map((x) => (
              <button key={x.id} className="remedy-row" onClick={() => onShow(x)}>
                <strong>{x.title}</strong>
              </button>
            ))}
          </div>
        </>
      )}
    </article>
  );
}
