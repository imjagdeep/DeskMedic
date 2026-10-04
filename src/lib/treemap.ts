// Squarified treemap layout (Bruls, Huizing & van Wijk): rectangles as close
// to square as possible, biggest first. Pure function, no library.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Placed<T> extends Rect {
  item: T;
}

/** Lay out `items` (sorted biggest first, sizes > 0) inside `box`. */
export function squarify<T>(items: { item: T; size: number }[], box: Rect): Placed<T>[] {
  const total = items.reduce((s, i) => s + i.size, 0);
  if (total <= 0 || box.w <= 0 || box.h <= 0) return [];
  const scale = (box.w * box.h) / total;
  const areas = items.map((i) => ({ item: i.item, area: i.size * scale }));
  const out: Placed<T>[] = [];
  let rect = { ...box };
  let row: typeof areas = [];

  const worst = (r: typeof areas, side: number) => {
    const sum = r.reduce((s, a) => s + a.area, 0);
    let max = 0;
    let min = Infinity;
    for (const a of r) {
      max = Math.max(max, a.area);
      min = Math.min(min, a.area);
    }
    const s2 = side * side;
    const sum2 = sum * sum;
    return Math.max((s2 * max) / sum2, sum2 / (s2 * min));
  };

  const place = (r: typeof areas) => {
    const sum = r.reduce((s, a) => s + a.area, 0);
    if (rect.w >= rect.h) {
      // Column on the left.
      const w = sum / rect.h;
      let y = rect.y;
      for (const a of r) {
        const h = a.area / w;
        out.push({ item: a.item, x: rect.x, y, w, h });
        y += h;
      }
      rect = { x: rect.x + w, y: rect.y, w: rect.w - w, h: rect.h };
    } else {
      // Row on top.
      const h = sum / rect.w;
      let x = rect.x;
      for (const a of r) {
        const w = a.area / h;
        out.push({ item: a.item, x, y: rect.y, w, h });
        x += w;
      }
      rect = { x: rect.x, y: rect.y + h, w: rect.w, h: rect.h - h };
    }
  };

  for (const a of areas) {
    const side = Math.min(rect.w, rect.h);
    if (row.length === 0 || worst([...row, a], side) <= worst(row, side)) {
      row.push(a);
    } else {
      place(row);
      row = [a];
    }
  }
  if (row.length) place(row);
  return out;
}
