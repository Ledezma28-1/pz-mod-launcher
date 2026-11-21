
export const CELL = 256;

export type View = { x: number; y: number; scale: number };

type Box = [number, number, number, number];

export const MIN_SCALE = 1 / 512;
export const MAX_SCALE = 4;

export function fit(bounds: Box, w: number, h: number): View {
  const [x0, y0, x1, y1] = bounds;
  const ww = (x1 - x0 + 1) * CELL;
  const wh = (y1 - y0 + 1) * CELL;
  const scale = Math.min(w / ww, h / wh);
  return { x: x0 * CELL - (w / scale - ww) / 2, y: y0 * CELL - (h / scale - wh) / 2, scale };
}

export function levelFor(scale: number, levels: number): number {
  const l = Math.floor(-Math.log2(scale));
  return Math.max(0, Math.min(levels - 1, l));
}

export function visibleTiles(view: View, w: number, h: number, p: { bounds: Box; levels: number }, level: number) {
  const [bx0, by0, bx1, by1] = p.bounds;
  const span = CELL * 2 ** level;
  const cols = Math.ceil((bx1 - bx0) / span);
  const rows = Math.ceil((by1 - by0) / span);
  const first = (from: number, origin: number) => Math.max(0, Math.floor((from - origin) / span));
  const last = (to: number, origin: number, count: number) => Math.min(count - 1, Math.ceil((to - origin) / span) - 1);
  const out: { x: number; y: number; left: number; top: number; size: number }[] = [];
  for (let ty = first(view.y, by0); ty <= last(view.y + h / view.scale, by0, rows); ty++) {
    for (let tx = first(view.x, bx0); tx <= last(view.x + w / view.scale, bx0, cols); tx++) {
      out.push({ x: tx, y: ty, left: (bx0 + tx * span - view.x) * view.scale, top: (by0 + ty * span - view.y) * view.scale, size: span * view.scale });
    }
  }
  return out;
}

export function cellAt(view: View, px: number, py: number): [number, number] {
  return [Math.floor((view.x + px / view.scale) / CELL), Math.floor((view.y + py / view.scale) / CELL)];
}

export function cellRect(view: View, cell: [number, number]) {
  return { left: (cell[0] * CELL - view.x) * view.scale, top: (cell[1] * CELL - view.y) * view.scale, size: CELL * view.scale };
}

export function moveFocus(cell: [number, number], key: string, bounds: Box): [number, number] {
  const step: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  const d = step[key];
  if (!d) return cell;
  const [x0, y0, x1, y1] = bounds;
  return [Math.max(x0, Math.min(x1, cell[0] + d[0])), Math.max(y0, Math.min(y1, cell[1] + d[1]))];
}

export function zoomAt(view: View, factor: number, px: number, py: number): View {
  const scale = Math.max(MIN_SCALE, Math.min(MAX_SCALE, view.scale * factor));
  const wx = view.x + px / view.scale;
  const wy = view.y + py / view.scale;
  return { x: wx - px / scale, y: wy - py / scale, scale };
}

export class TileCache {
  private items = new Map<string, HTMLImageElement>();
  private limit: number;

  constructor(limit: number) {
    this.limit = limit;
  }

  get(url: string): HTMLImageElement | undefined {
    const img = this.items.get(url);
    if (img) {
      this.items.delete(url);
      this.items.set(url, img);
    }
    return img;
  }

  put(url: string, img: HTMLImageElement) {
    this.items.delete(url);
    this.items.set(url, img);
    while (this.items.size > this.limit) {
      const oldest = this.items.keys().next().value;
      if (oldest === undefined) break;
      this.items.delete(oldest);
    }
  }
}

export function compatibleBatch<T extends { mod_key: string; cells: [number, number][]; compatible: boolean }>(candidates: T[]): T[] {
  const byMod = new Map<string, T[]>();
  for (const c of candidates) byMod.set(c.mod_key, [...(byMod.get(c.mod_key) ?? []), c]);
  const taken = new Set<string>();
  const out: T[] = [];
  for (const maps of byMod.values()) {
    const cells = maps.flatMap((m) => m.cells.map(([x, y]) => `${x},${y}`));
    if (cells.length === 0 || maps.some((m) => !m.compatible) || cells.some((k) => taken.has(k))) continue;
    cells.forEach((k) => taken.add(k));
    out.push(...maps);
  }
  return out;
}

export const MAP_COLORS = 6;

function hash(s: string) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return h;
}

export function modSlots(maps: { mod_key: string; active: boolean }[]): Map<string, number> {
  const keys = [...new Set(maps.filter((m) => m.active).map((m) => m.mod_key))].sort();
  const out = new Map<string, number>();
  const taken = new Set<number>();
  for (const k of keys) {
    if (taken.size === MAP_COLORS) break;
    let s = hash(k) % MAP_COLORS;
    while (taken.has(s)) s = (s + 1) % MAP_COLORS;
    taken.add(s);
    out.set(k, s);
  }
  return out;
}

export function fitSquares(box: Box, w: number, h: number): View {
  const [x0, y0, x1, y1] = box;
  const pad = Math.max(x1 - x0, y1 - y0, CELL) * 0.25;
  const bw = x1 - x0 + 2 * pad;
  const bh = y1 - y0 + 2 * pad;
  const scale = Math.max(MIN_SCALE, Math.min(1, w / bw, h / bh));
  return { x: (x0 + x1) / 2 - w / 2 / scale, y: (y0 + y1) / 2 - h / 2 / scale, scale };
}

export function cellsBox(cells: [number, number][]): Box {
  const xs = cells.map((c) => c[0]);
  const ys = cells.map((c) => c[1]);
  return [Math.min(...xs) * CELL, Math.min(...ys) * CELL, (Math.max(...xs) + 1) * CELL, (Math.max(...ys) + 1) * CELL];
}

export function fold(s: string) {
  return s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .trim();
}

export function findTarget<T extends { name: string }>(targets: T[], q: string): T | undefined {
  const f = fold(q);
  if (!f) return undefined;
  return targets.find((t) => fold(t.name).startsWith(f)) ?? targets.find((t) => fold(t.name).includes(f));
}
