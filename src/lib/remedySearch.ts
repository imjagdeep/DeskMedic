// Search the remedies library by what people type ("printer offline",
// "no sound in teams"). Small and fast enough for thousands of entries.
import type { Remedy } from "./types";

const STOP = new Set(["a", "an", "the", "my", "is", "it", "to", "on", "in", "of", "and", "or", "i", "me", "not", "won't", "wont", "doesnt", "doesn't", "cant", "can't", "how", "do", "does", "with", "for", "keeps", "when"]);

export function words(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/['’]/g, "")
    .replace(/[^a-z0-9%.+#-]+/g, " ")
    .split(" ")
    .map((w) => w.replace(/^[.-]+|[.-]+$/g, ""))
    .filter((w) => w.length > 0 && !STOP.has(w));
}

interface Indexed {
  r: Remedy;
  title: string;
  symptoms: string;
  rest: string;
}

export function index(remedies: Remedy[]): Indexed[] {
  return remedies.map((r) => ({
    r,
    title: ` ${words(r.title).join(" ")} `,
    symptoms: ` ${r.symptoms.map((s) => words(s).join(" ")).join(" | ")} `,
    rest: ` ${words(`${r.why} ${r.steps.map((s) => s.text).join(" ")}`).join(" ")} `,
  }));
}

/** Score one word against a field: whole word beats a prefix beats inside. */
function hit(field: string, w: string): number {
  if (field.includes(` ${w} `)) return 1;
  if (field.includes(` ${w}`)) return 0.7;
  if (w.length >= 4 && field.includes(w)) return 0.4;
  return 0;
}

/** Best matches first; every query word must appear somewhere. */
export function search(idx: Indexed[], query: string, limit = 40): Remedy[] {
  const q = words(query);
  if (q.length === 0) return [];
  const phrase = ` ${q.join(" ")} `;
  const scored: { r: Remedy; s: number }[] = [];
  for (const e of idx) {
    let total = 0;
    let all = true;
    for (const w of q) {
      const s = Math.max(hit(e.title, w) * 3, hit(e.symptoms, w) * 2.5, hit(e.rest, w) * 0.6);
      if (s === 0) {
        all = false;
        break;
      }
      total += s;
    }
    if (!all) continue;
    if (q.length > 1 && (e.symptoms.includes(phrase) || e.title.includes(phrase))) total += 4;
    scored.push({ r: e.r, s: total });
  }
  return scored.sort((a, b) => b.s - a.s).slice(0, limit).map((x) => x.r);
}
