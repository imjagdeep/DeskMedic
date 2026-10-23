import { useEffect, useMemo, useRef, useState } from "react";
import { AdminNotice, PageHeader } from "../components/Common";
import { AppsIcon, CategoryIcon, GlobeIcon, LockIcon, PrinterIcon, ShieldIcon, WindowIcon } from "../components/Icons";
import { destLabel, type Dest } from "../components/Sidebar";
import { api } from "../lib/api";
import { cancelFix, clearFinishedFixes, latestRun, queueFix, showFixDetails, useFixDetails, useFixRuns, type FixRun } from "../lib/jobs";
import { index, search, words } from "../lib/remedySearch";
import { toast, toastError, useAppInfo } from "../lib/store";
import type { FixRecipe, Remedy, RemedyLibrary } from "../lib/types";

type Group = { title: string; Icon: (p: { size?: number }) => React.ReactElement; ids: string[] };

// One-click remedy ids by topic; anything new and unlisted lands in "More".
const GROUPS: Group[] = [
  { title: "Internet and network", Icon: GlobeIcon, ids: ["flush-dns", "reset-network"] },
  { title: "Windows", Icon: WindowIcon, ids: ["restart-explorer", "rebuild-icon-cache", "rebuild-search", "time-sync", "gpupdate"] },
  { title: "Apps", Icon: AppsIcon, ids: ["reset-teams", "reset-onedrive", "reset-store"] },
  { title: "Printing and sound", Icon: PrinterIcon, ids: ["print-queue", "restart-audio"] },
  { title: "System repair", Icon: ShieldIcon, ids: ["repair-windows-update", "sfc", "dism-restore"] },
];

/** All = the guided library (categories, search, step-by-step guides); One-click = the fixes DeskMedic runs itself. */
type Show = "all" | "oneclick";

/** Where a fix asked for from Checkup waits while DeskMedic restarts as administrator. */
export const PENDING_FIX = "deskmedic-pending-fix";

/** `ask`: a one-click remedy to open straight away (from a Checkup card). */
export function Remedies({
  ask,
  onAsked,
  onNavigate,
}: {
  ask?: string | null;
  onAsked?: () => void;
  onNavigate?: (d: Dest) => void;
}) {
  const info = useAppInfo();
  const [recipes, setRecipes] = useState<FixRecipe[]>([]);
  const [lib, setLib] = useState<RemedyLibrary | null>(null);
  const [show, setShow] = useState<Show>("all");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const [open, setOpen] = useState<Remedy | null>(null);
  const top = useRef<HTMLDivElement>(null);
  const idx = useMemo(() => index(lib?.remedies ?? []), [lib]);
  const found = useMemo(() => search(idx, query), [idx, query]);
  const [asking, setAsking] = useState<FixRecipe | null>(null);
  // A fix from Checkup that needs administrator rights DeskMedic doesn't have yet.
  const [needsAdmin, setNeedsAdmin] = useState<FixRecipe | null>(null);
  // The fix a Checkup card pointed at: its card is scrolled to and highlighted.
  const [focus, setFocus] = useState<string | null>(null);
  // Restore point first, for remedies that offer one. On by default.
  const [restorePoint, setRestorePoint] = useState(true);
  // Kept outside the page so leaving it mid-fix loses nothing.
  const runs = useFixRuns();
  const running = runs.find((r) => r.state === "running")?.id ?? null;

  useEffect(() => {
    api.fixList().then(setRecipes).catch(toastError);
    api.remedyLibrary().then(setLib).catch(toastError);
  }, []);

  const openRemedy = (r: Remedy | null) => {
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
  const hasFix = (r: Remedy) => r.steps.some((s) => s.fix);
  const inCategory = lib && category ? lib.remedies.filter((r) => r.category === category) : [];
  const cat = lib?.categories.find((c) => c.id === category);
  const q = query.trim();

  // One-click fixes matching the search: by their own words, or used by a matching guide
  // ("printer offline" finds Clear print queue through the Printer offline guide).
  const fixHits = useMemo(() => {
    if (!q) return recipes;
    const want = words(q);
    const viaGuides = new Set(found.flatMap((r) => r.steps.map((s) => s.fix).filter((f): f is string => !!f)));
    return recipes.filter((r) => {
      const text = ` ${words(`${r.name} ${r.when} ${r.does}`).join(" ")} `;
      return viaGuides.has(r.id) || (want.length > 0 && want.every((w) => text.includes(` ${w}`)));
    });
  }, [q, recipes, found]);

  // From a Checkup card: show that fix in the one-click list, highlighted, and ask to run it
  // (or explain that it needs administrator rights first, or that it's already on its way).
  useEffect(() => {
    if (!ask || recipes.length === 0 || !info) return;
    const r = recipes.find((x) => x.id === ask);
    onAsked?.();
    if (!r) return;
    setShow("oneclick");
    setQuery("");
    setCategory(null);
    setOpen(null);
    setFocus(r.id);
    const last = latestRun(runs, r.id);
    if (last && (last.state === "running" || last.state === "queued")) {
      toast(last.state === "running" ? `${r.name} is already running.` : `${r.name} is already waiting its turn.`);
    } else if (r.admin && !info.elevated) {
      setNeedsAdmin(r);
    } else {
      setRestorePoint(true);
      setAsking(r);
    }
  }, [ask, recipes, info, onAsked, runs]);

  // Bring the highlighted card into view once it's on screen.
  useEffect(() => {
    if (!focus) return;
    const t = setTimeout(() => document.getElementById(`fix-${focus}`)?.scrollIntoView({ block: "center", behavior: "smooth" }), 80);
    const off = setTimeout(() => setFocus(null), 6000);
    return () => {
      clearTimeout(t);
      clearTimeout(off);
    };
  }, [focus, show]);

  const run = (r: FixRecipe) => {
    setAsking(null);
    // Queued fixes say so on their card and in Fix activity; no pop-up for each.
    if (queueFix(r.id, r.name, r.restore_point && restorePoint) === "already") toast(`${r.name} is already running or waiting.`);
  };
  const restartForFix = (r: FixRecipe) => {
    try {
      localStorage.setItem(PENDING_FIX, JSON.stringify({ id: r.id, at: Date.now() }));
    } catch {
      // Not remembered: after the restart, open Fixes by hand.
    }
    api.restartAsAdmin().catch((e) => {
      try {
        localStorage.removeItem(PENDING_FIX);
      } catch {
        // nothing to undo
      }
      toastError(e);
    });
  };
  const switchTo = (s: Show) => {
    setShow(s);
    setCategory(null);
    setOpen(null);
  };

  return (
    <div className="page wide" ref={top}>
      <PageHeader title="Fixes" />
      <div className="remedy-search">
        <input
          type="search"
          value={query}
          placeholder={
            show === "all"
              ? `Describe the problem: "printer offline", "no sound in Teams", "Wi-Fi drops"…`
              : `Find a one-click fix: "printer", "Teams", "search"…`
          }
          aria-label="Search remedies"
          autoFocus
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(null);
          }}
        />
        <div className="remedy-filter segmented" role="group" aria-label="Show">
          <button className={show === "all" ? "active" : ""} aria-pressed={show === "all"} onClick={() => switchTo("all")}>
            All <span className="seg-count">{lib?.remedies.length ?? ""}</span>
          </button>
          <button className={show === "oneclick" ? "active" : ""} aria-pressed={show === "oneclick"} onClick={() => switchTo("oneclick")}>
            One-click <span className="seg-count">{recipes.length || ""}</span>
          </button>
        </div>
      </div>

      <FixActivity runs={runs} />

      {open ? (
        <RemedyView
          r={open}
          recipes={recipes}
          elevated={!!info?.elevated}
          runs={runs}
          related={open.related.map(byId).filter((x): x is Remedy => !!x)}
          onBack={() => openRemedy(null)}
          onShow={openRemedy}
          onFix={askFix}
          onPage={(p) => onNavigate?.(p as Dest)}
        />
      ) : show === "oneclick" ? (
        <section>
          <p className="muted remedy-intro">DeskMedic runs these for you. Each one shows exactly what it does first, and every run goes in the Log.</p>
          <AdminNotice need="Many one-click remedies change system settings and need administrator rights." />
          {q && fixHits.length === 0 && (
            <p className="muted">
              No one-click fix for "{q}". <button className="link" onClick={() => switchTo("all")}>Search all remedies</button>
            </p>
          )}
          {[...GROUPS, { title: "More", Icon: AppsIcon, ids: recipes.map((r) => r.id).filter((id) => !GROUPS.some((g) => g.ids.includes(id))) }]
            .map((g) => ({ ...g, items: g.ids.map((id) => fixHits.find((r) => r.id === id)).filter((r): r is FixRecipe => !!r) }))
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
                  {items.map((r) => (
                    <FixCard key={r.id} r={r} elevated={!!info?.elevated} last={latestRun(runs, r.id)} focused={focus === r.id} onRun={() => askFix(r.id)} />
                  ))}
                </div>
              </section>
            ))}
        </section>
      ) : q ? (
        <section>
          <h3 className="fix-group-title">{found.length ? `Best matches for "${q}"` : `Nothing found for "${q}"`}</h3>
          {found.length === 0 && <p className="muted">Try fewer or different words (what you see, like "offline" or "no sound"), or pick a category below.</p>}
          <RemedyList items={found} hasFix={hasFix} onShow={openRemedy} />
          {found.length === 0 && lib && <CategoryGrid lib={lib} onPick={(c) => { setQuery(""); setCategory(c); }} />}
        </section>
      ) : category && cat ? (
        <section>
          <button className="link" onClick={() => setCategory(null)}>‹ All categories</button>
          <h2 className="remedy-cat-title">
            <CategoryIcon name={cat.icon} size={20} /> {cat.title}
          </h2>
          <p className="muted">{cat.blurb}</p>
          <RemedyList items={inCategory} hasFix={hasFix} onShow={openRemedy} />
        </section>
      ) : (
        lib && <CategoryGrid lib={lib} onPick={setCategory} />
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
            {running && running !== asking.id && (
              <p className="muted small">
                {recipes.find((r) => r.id === running)?.name ?? "Another fix"} is running now; this one waits its turn and starts by itself.
              </p>
            )}
            <div className="modal-actions">
              <button onClick={() => setAsking(null)}>Cancel</button>
              <button className="primary" autoFocus onClick={() => run(asking)}>
                {running && running !== asking.id ? "Add to queue" : "Run it"}
              </button>
            </div>
          </div>
        </div>
      )}

      {needsAdmin && (
        <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-admin-h">
          <div className="modal">
            <h2 id="fix-admin-h">
              <LockIcon size={15} /> {needsAdmin.name}
            </h2>
            <p>{needsAdmin.does}</p>
            <p className="notice-text">
              This fix changes system settings, so it needs administrator rights. DeskMedic restarts as administrator (Windows asks you first) and
              comes straight back to this fix.
            </p>
            <div className="modal-actions">
              <button onClick={() => setNeedsAdmin(null)}>Not now</button>
              <button className="primary" autoFocus onClick={() => restartForFix(needsAdmin)}>
                Restart as administrator
              </button>
            </div>
          </div>
        </div>
      )}

      <FixDetails runs={runs} onRunAgain={askFix} />
      <FixBar runs={runs} />
    </div>
  );
}

/** "Running · 0:42", "Waiting", "Done 10:42", "Failed 10:43": a run's state in words. */
function stateText(run: FixRun, now: number): string {
  const at = (t?: number) => (t ? new Date(t).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "");
  const secs = Math.max(0, Math.round((now - (run.startedAt ?? now)) / 1000));
  if (run.state === "running") return `Running · ${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
  if (run.state === "queued") return "Waiting its turn";
  if (run.state === "done") return `Done at ${at(run.endedAt)}${run.result?.restart ? " · restart the PC to finish" : ""}`;
  return `Failed at ${at(run.endedAt)}`;
}

/** Re-render every second while something runs (for the timer). */
function useTick(active: boolean): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!active) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [active]);
  return now;
}

const STATE_ICON: Record<FixRun["state"], string> = { queued: "…", running: "", done: "✓", failed: "!" };

/** This session's fixes: what's running, what waits, what finished or failed. */
function FixActivity({ runs }: { runs: FixRun[] }) {
  const active = runs.some((r) => r.state === "running" || r.state === "queued");
  const now = useTick(active);
  if (runs.length === 0) return null;
  const waiting = runs.filter((r) => r.state === "queued");
  const finished = runs.filter((r) => r.state === "done" || r.state === "failed");
  // Newest first: running, then the queue in order, then finished ones.
  const ordered = [...runs.filter((r) => r.state === "running"), ...waiting, ...[...finished].reverse()];
  return (
    <section className="card fix-activity" aria-label="Fix activity" aria-live="polite">
      <div className="fix-activity-head">
        <h3>Fix activity</h3>
        <span className="muted small">
          {active
            ? `${runs.some((r) => r.state === "running") ? "1 running" : ""}${waiting.length ? `${runs.some((r) => r.state === "running") ? " · " : ""}${waiting.length} waiting` : ""}`
            : "Nothing running"}
        </span>
        {finished.length > 0 && (
          <button className="link" onClick={clearFinishedFixes}>
            Clear finished
          </button>
        )}
      </div>
      <ul className="fix-activity-list">
        {ordered.map((run) => (
          <li key={run.seq} className={`fix-run ${run.state}`}>
            <span className={`fix-run-icon ${run.state}`} aria-hidden="true">
              {run.state === "running" ? <span className="spinner small" /> : STATE_ICON[run.state]}
            </span>
            <strong>{run.name}</strong>
            <span className="muted small">
              {stateText(run, now)}
              {run.state === "queued" && waiting.length > 1 && ` (#${waiting.indexOf(run) + 1} in line)`}
            </span>
            <span className="fix-run-actions">
              {run.state === "queued" && (
                <button className="link" onClick={() => cancelFix(run.seq)}>
                  Cancel
                </button>
              )}
              {(run.state === "done" || run.state === "failed") && (
                <button className="link" onClick={() => showFixDetails(run.seq)}>
                  {run.state === "failed" ? "Why it failed" : "Details"}
                </button>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Pinned to the bottom while fixes run, so the queue stays in sight anywhere on the page. */
function FixBar({ runs }: { runs: FixRun[] }) {
  const current = runs.find((r) => r.state === "running");
  const waiting = runs.filter((r) => r.state === "queued").length;
  const now = useTick(!!current);
  if (!current) return null;
  return (
    <div className="running-bar fix-bar">
      <div className="spinner" aria-hidden="true" />
      <span className="grow">
        Running <strong>{current.name}</strong> · {stateText(current, now).replace("Running · ", "")}
        {waiting > 0 && <span className="muted"> · {waiting} waiting</span>}
      </span>
      <button className="link" onClick={() => document.querySelector(".fix-activity")?.scrollIntoView({ behavior: "smooth", block: "start" })}>
        Show
      </button>
    </div>
  );
}

/** The steps and output of one finished run. */
function FixDetails({ runs, onRunAgain }: { runs: FixRun[]; onRunAgain: (id: string) => void }) {
  const seq = useFixDetails();
  const run = runs.find((r) => r.seq === seq);
  if (!run) return null;
  const close = () => showFixDetails(null);
  const ok = run.state === "done";
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="fix-result-h">
      <div className="modal">
        <h2 id="fix-result-h">
          <span className={ok ? "status ok" : "status fail"}>{ok ? "Done" : "Failed"}</span> {run.name}
        </h2>
        {run.error && <p className="error-text">{run.error}</p>}
        {run.result?.steps.map((s) => (
          <div key={s.label} className="fix-step">
            <strong>
              <span className={s.ok ? "step-ok" : "step-fail"} aria-label={s.ok ? "worked" : "failed"}>
                {s.ok ? "✓" : "✗"}
              </span>{" "}
              {s.label}
            </strong>
            {s.output && <pre className="details">{s.output}</pre>}
          </div>
        ))}
        {run.result?.restart && ok && <p className="notice-text">Restart the PC to finish.</p>}
        <div className="modal-actions">
          {!ok && (
            <button
              onClick={() => {
                close();
                onRunAgain(run.id);
              }}
            >
              Run again…
            </button>
          )}
          <button className="primary" onClick={close}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function FixCard({ r, elevated, last, focused, onRun }: { r: FixRecipe; elevated: boolean; last?: FixRun; focused: boolean; onRun: () => void }) {
  const blocked = r.admin && !elevated;
  const busy = last?.state === "running" || last?.state === "queued";
  return (
    <div id={`fix-${r.id}`} className={`card fix-card${last?.state === "running" ? " running" : ""}${focused ? " focused" : ""}`}>
      <h2>{r.name}</h2>
      <p className="small">{r.when}</p>
      <p className="muted small fix-does">{r.does}</p>
      {(r.restart || r.restore_point) && (
        <div className="fix-tags">
          {r.restart && <span className="badge warn">restart</span>}
          {r.restore_point && <span className="badge">restore point</span>}
        </div>
      )}
      {last && (
        <div className={`fix-last ${last.state}`}>
          {last.state === "running" && <span className="spinner small" aria-hidden="true" />}
          {last.state === "queued" ? "Waiting its turn" : last.state === "running" ? "Running now" : last.state === "done" ? "Done this session" : "Failed this session"}
          {(last.state === "done" || last.state === "failed") && (
            <button className="link" onClick={() => showFixDetails(last.seq)}>
              {last.state === "failed" ? "Why?" : "Details"}
            </button>
          )}
        </div>
      )}
      <div className="fix-foot">
        <span className="muted small">{r.takes}</span>
        <button
          className={blocked ? "with-icon" : "primary"}
          disabled={blocked || busy}
          title={blocked ? "Restart DeskMedic as administrator to run this" : undefined}
          onClick={onRun}
        >
          {blocked && <LockIcon size={13} />}
          {last?.state === "running" ? "Running…" : last?.state === "queued" ? "Queued" : blocked ? "Needs admin" : last?.state === "failed" ? "Try again…" : "Run…"}
        </button>
      </div>
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

function RemedyList({ items, hasFix, onShow }: { items: Remedy[]; hasFix: (r: Remedy) => boolean; onShow: (r: Remedy) => void }) {
  return (
    <div className="remedy-list">
      {items.map((r) => (
        <button key={r.id} className="remedy-row" onClick={() => onShow(r)}>
          <strong>{r.title}</strong>
          <span className="muted small">{r.why}</span>
          {hasFix(r) && <span className="badge ok">One-click</span>}
        </button>
      ))}
    </div>
  );
}

function RemedyView({
  r,
  recipes,
  elevated,
  runs,
  related,
  onBack,
  onShow,
  onFix,
  onPage,
}: {
  r: Remedy;
  recipes: FixRecipe[];
  elevated: boolean;
  runs: FixRun[];
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
          const last = recipe ? latestRun(runs, recipe.id) : undefined;
          const busy = last?.state === "running" || last?.state === "queued";
          return (
            <li key={i}>
              <span>{s.text}</span>
              {recipe && (
                <button
                  className={blocked ? "with-icon" : "primary"}
                  disabled={blocked || busy}
                  title={blocked ? "Restart DeskMedic as administrator to run this" : recipe.does}
                  onClick={() => onFix(recipe.id)}
                >
                  {blocked && <LockIcon size={13} />}
                  {last?.state === "running" ? "Running…" : last?.state === "queued" ? "Queued" : blocked ? "Needs admin" : `Run: ${recipe.name}…`}
                </button>
              )}
              {s.open && <button onClick={() => openStep(i)}>Open</button>}
              {s.page && <button onClick={() => onPage(s.page as string)}>Go to {destLabel(s.page as Dest)}</button>}
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
