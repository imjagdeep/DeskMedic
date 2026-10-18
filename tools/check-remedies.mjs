// Checks the remedies library like library.rs does (Rust can't build on
// every PC): node tools/check-remedies.mjs [--stats]
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = new URL("../src-tauri/crates/dm-core/remedies/", import.meta.url).pathname.replace(/^\/([A-Z]:)/, "$1");
const targets = JSON.parse(readFileSync(join(dir, "targets.json"), "utf8"));
const fixes = readFileSync(join(dir, "../src/fixes.rs"), "utf8");
const fixIds = new Set([...fixes.matchAll(/^\s+id: "([a-z0-9-]+)",$/gm)].map((m) => m[1]));
const OS = new Set(["windows", "macos"]);
const errors = [];
const ids = new Map();
const cats = new Map();
const stats = {};

const safe = (s) => s.length <= 120 && /^[A-Za-z0-9\-._?=&_]*$/.test(s);
function openOs(t) {
  if (t.startsWith("ms-settings:")) return safe(t.slice(12)) ? "windows" : null;
  if (t.startsWith("x-apple.systempreferences:")) {
    const r = t.slice(26);
    return r && safe(r) ? "macos" : null;
  }
  if (t.startsWith("run:")) return t.slice(4) in targets.run ? "windows" : null;
  if (t.startsWith("app:")) return targets.apps.includes(t.slice(4)) ? "macos" : null;
  return null;
}
const same = (a, b) => a.length === b.length && a.every((x) => b.includes(x));

const files = readdirSync(dir).filter((f) => f.endsWith(".json") && f !== "targets.json").sort();
const all = [];
for (const f of files) {
  let file;
  try {
    file = JSON.parse(readFileSync(join(dir, f), "utf8"));
  } catch (e) {
    errors.push(`${f}: ${e.message}`);
    continue;
  }
  const keys = Object.keys(file).sort().join();
  if (keys !== "blurb,icon,id,remedies,title") errors.push(`${f}: top-level keys ${keys}`);
  // A category can span several files; later ones repeat the same title.
  if (cats.has(file.id)) {
    if (cats.get(file.id) !== file.title) errors.push(`${f}: category ${file.id} has a different title in another file`);
  } else {
    cats.set(file.id, file.title);
    stats[file.id] = { windows: 0, macos: 0, total: 0 };
  }
  for (const r of file.remedies ?? []) {
    const at = `${f} › ${r.id}`;
    const extra = Object.keys(r).filter((k) => !["id", "os", "title", "symptoms", "why", "steps", "escalate", "related"].includes(k));
    if (extra.length) errors.push(`${at}: unknown fields ${extra}`);
    if (!/^[a-z0-9-]{1,80}$/.test(r.id ?? "")) errors.push(`${at}: bad id`);
    if (ids.has(r.id)) errors.push(`${at}: id also in ${ids.get(r.id)}`);
    ids.set(r.id, f);
    if (!Array.isArray(r.os) || !r.os.length || r.os.some((o) => !OS.has(o))) errors.push(`${at}: bad os`);
    if (!r.title?.trim() || !r.why?.trim()) errors.push(`${at}: title and why required`);
    if (!r.symptoms?.length || !r.steps?.length) errors.push(`${at}: symptoms and steps required`);
    for (const o of r.os ?? []) {
      const n = (r.steps ?? []).filter((s) => !s.os || s.os.includes(o)).length;
      if (n === 0) errors.push(`${at}: no steps on ${o}`);
    }
    (r.steps ?? []).forEach((s, i) => {
      const sa = `${at} step ${i + 1}`;
      const sk = Object.keys(s).filter((k) => !["text", "os", "fix", "open", "page"].includes(k));
      if (sk.length) errors.push(`${sa}: unknown fields ${sk}`);
      if (!s.text?.trim()) errors.push(`${sa}: empty text`);
      if (["fix", "open", "page"].filter((k) => k in s).length > 1) errors.push(`${sa}: one action per step`);
      if (s.os && s.os.some((o) => !(r.os ?? []).includes(o))) errors.push(`${sa}: step os outside the remedy's`);
      const os = s.os ?? r.os ?? [];
      if (s.fix !== undefined) {
        if (!fixIds.has(s.fix)) errors.push(`${sa}: unknown fix ${s.fix}`);
        if (!same(os, ["windows"])) errors.push(`${sa}: fixes are Windows-only: set the step's os`);
      }
      if (s.page !== undefined && !targets.pages.includes(s.page)) errors.push(`${sa}: unknown page ${s.page}`);
      if (s.open !== undefined) {
        const t = openOs(s.open);
        if (!t) errors.push(`${sa}: unknown open target ${s.open}`);
        else if (!same(os, [t])) errors.push(`${sa}: ${s.open} only exists on ${t}: set the step's os`);
      }
    });
    for (const o of r.os ?? []) if (stats[file.id][o] !== undefined) stats[file.id][o]++;
    stats[file.id].total++;
    all.push(r);
  }
}
for (const r of all) for (const rel of r.related ?? []) if (!ids.has(rel) || rel === r.id) errors.push(`${r.id}: related ${rel} doesn't exist`);

// Near-duplicate titles within an OS are usually the same problem twice.
const seen = new Map();
for (const r of all)
  for (const o of r.os ?? []) {
    const k = `${o}:${r.title.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim()}`;
    if (seen.has(k)) errors.push(`${r.id}: same title as ${seen.get(k)} on ${o}`);
    seen.set(k, r.id);
  }

if (process.argv.includes("--stats")) {
  console.table(stats);
}
const win = all.filter((r) => r.os?.includes("windows")).length;
const mac = all.filter((r) => r.os?.includes("macos")).length;
console.log(`${all.length} remedies in ${cats.size} categories (Windows ${win}, Mac ${mac})`);
if (errors.length) {
  console.error(errors.slice(0, 50).join("\n"));
  console.error(`${errors.length} problem(s)`);
  process.exit(1);
}
console.log("all checks passed");
