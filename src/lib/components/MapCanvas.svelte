<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import Minus from '@lucide/svelte/icons/minus';
  import Maximize from '@lucide/svelte/icons/maximize';
  import { onMount, type Snippet } from 'svelte';
  import { inTauri } from '../api';
  import { t } from '../i18n.svelte';
  import { MAP_COLORS, TileCache, cellAt, cellRect, fit, fitSquares, levelFor, moveFocus, visibleTiles, zoomAt, CELL, type View } from '../map-view';
  import type { Cell, MapInfo, MapsView, PyramidInfo } from '../types';

  let {
    data,
    previews,
    selected,
    hover,
    slots,
    focus = $bindable(null),
    layers,
    fill,
    onpick,
    onclear,
    onhover,
    tip,
  }: {
    data: MapsView;
    previews: MapInfo[];
    selected: string | null;
    hover: string | null;
    slots: Map<string, number>;
    focus: Cell | null;
    layers: { game: boolean; spawns: boolean; towns: boolean };
    fill: number;
    onpick: (cell: Cell) => void;
    onclear: () => void;
    onhover: (cell: Cell | null) => void;
    tip: Snippet<[Cell]>;
  } = $props();

  const TILE_BASE = 'http://pzmap.localhost/';
  const tiles = new TileCache(200);
  const pending = new Set<string>();

  let box: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let w = $state(0);
  let h = $state(0);
  let view = $state<View>({ x: 0, y: 0, scale: 0.04 });
  let fitted = false;
  let frame = 0;
  let pointed = $state<{ cell: Cell; x: number; y: number } | null>(null);

  const byKey = $derived(new Map(data.maps.map((m) => [m.key, m])));

  export function goTo(squares: [number, number, number, number]) {
    if (w > 0 && h > 0) view = fitSquares(squares, w, h);
  }

  function color(name: string) {
    return getComputedStyle(canvas).getPropertyValue(name).trim() || '#888';
  }

  function tile(source: string, level: number, x: number, y: number) {
    const url = `${TILE_BASE}${source}/${level}/${x}x${y}.png`;
    const img = tiles.get(url);
    if (img || pending.has(url)) return img;
    pending.add(url);
    const i = new Image();
    i.onload = () => {
      pending.delete(url);
      tiles.put(url, i);
      redraw();
    };
    i.onerror = () => pending.delete(url);
    i.src = url;
    return undefined;
  }

  function drawPyramid(ctx: CanvasRenderingContext2D, p: PyramidInfo) {
    const coarse = p.levels - 1;
    const level = levelFor(view.scale, p.levels);
    for (const l of level === coarse ? [coarse] : [coarse, level]) {
      for (const tl of visibleTiles(view, w, h, p, l)) {
        const img = tile(p.source, l, tl.x, tl.y);
        if (img) ctx.drawImage(img, tl.left, tl.top, (img.naturalWidth / 256) * tl.size, (img.naturalHeight / 256) * tl.size);
      }
    }
  }

  function outline(ctx: CanvasRenderingContext2D, cells: Cell[]) {
    const set = new Set(cells.map(([x, y]) => `${x},${y}`));
    for (const [x, y] of cells) {
      const r = cellRect(view, [x, y]);
      if (r.left + r.size < 0 || r.top + r.size < 0 || r.left > w || r.top > h) continue;
      const [l, tp, rr, b] = [r.left, r.top, r.left + r.size, r.top + r.size];
      const edge = (x0: number, y0: number, x1: number, y1: number) => {
        ctx.moveTo(x0, y0);
        ctx.lineTo(x1, y1);
      };
      if (!set.has(`${x},${y - 1}`)) edge(l, tp, rr, tp);
      if (!set.has(`${x},${y + 1}`)) edge(l, b, rr, b);
      if (!set.has(`${x - 1},${y}`)) edge(l, tp, l, b);
      if (!set.has(`${x + 1},${y}`)) edge(rr, tp, rr, b);
    }
  }

  function townPath(ctx: CanvasRenderingContext2D) {
    const sx = (x: number) => (x - view.x) * view.scale;
    const sy = (y: number) => (y - view.y) * view.scale;
    ctx.beginPath();
    for (const z of data.town_zones) {
      const l = sx(z.x);
      const tp = sy(z.y);
      const zw = z.w * view.scale;
      const zh = z.h * view.scale;
      if (l + zw < 0 || tp + zh < 0 || l > w || tp > h) continue;
      if (!z.points) {
        ctx.rect(l, tp, zw, zh);
        continue;
      }
      const p = z.points;
      const n = p.length >> 1;
      let area = 0;
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        area += p[2 * i] * p[2 * j + 1] - p[2 * j] * p[2 * i + 1];
      }
      const at = (k: number) => (area >= 0 ? k : n - 1 - k);
      ctx.moveTo(sx(p[2 * at(0)]), sy(p[2 * at(0) + 1]));
      for (let k = 1; k < n; k++) ctx.lineTo(sx(p[2 * at(k)]), sy(p[2 * at(k) + 1]));
      ctx.closePath();
    }
  }

  function draw() {
    frame = 0;
    if (!canvas || w === 0 || h === 0) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(w * dpr)) canvas.width = Math.round(w * dpr);
    if (canvas.height !== Math.round(h * dpr)) canvas.height = Math.round(h * dpr);
    const ctx = canvas.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = color('--ground');
    ctx.fillRect(0, 0, w, h);
    ctx.imageSmoothingEnabled = view.scale < 1;

    const [wx0, wy0, wx1, wy1] = data.world;
    const wl = (wx0 * CELL - view.x) * view.scale;
    const wt = (wy0 * CELL - view.y) * view.scale;
    const ww = (wx1 - wx0 + 1) * CELL * view.scale;
    const wh = (wy1 - wy0 + 1) * CELL * view.scale;
    const show = layers.game && inTauri;
    if (!(show && data.vanilla)) {
      ctx.fillStyle = color('--surface');
      ctx.fillRect(wl, wt, ww, wh);
    }
    if (show && data.vanilla) drawPyramid(ctx, data.vanilla);
    if (show) for (const m of [...data.maps.filter((x) => x.active), ...previews]) if (m.pyramid) drawPyramid(ctx, m.pyramid);

    const accent = color('--accent');
    const warn = color('--warn');
    const ink = color('--ink');
    const ink2 = color('--ink-2');
    const neutral = color('--ink-3');
    const halo = color('--surface');
    const line = color('--line-strong');
    const palette = Array.from({ length: MAP_COLORS }, (_, i) => color(`--map-${i + 1}`));
    const modColor = (m: MapInfo | undefined) => {
      const s = m ? slots.get(m.mod_key) : undefined;
      return s === undefined ? neutral : palette[s];
    };
    const size = CELL * view.scale;
    ctx.strokeStyle = line;
    ctx.lineWidth = 1;
    ctx.strokeRect(Math.round(wl) + 0.5, Math.round(wt) + 0.5, Math.round(ww), Math.round(wh));

    if (layers.towns && data.town_zones.length) {
      townPath(ctx);
      ctx.fillStyle = ink;
      ctx.globalAlpha = 0.1;
      ctx.fill('nonzero');
      ctx.globalAlpha = 0.5;
      ctx.strokeStyle = ink;
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    if (size >= 10) {
      ctx.strokeStyle = line;
      ctx.globalAlpha = 0.25;
      ctx.lineWidth = 1;
      const [x0, y0, x1, y1] = data.bounds;
      ctx.beginPath();
      for (let x = x0; x <= x1 + 1; x++) {
        const px = Math.round((x * CELL - view.x) * view.scale) + 0.5;
        if (px < 0 || px > w) continue;
        ctx.moveTo(px, Math.max(0, (y0 * CELL - view.y) * view.scale));
        ctx.lineTo(px, Math.min(h, ((y1 + 1) * CELL - view.y) * view.scale));
      }
      for (let y = y0; y <= y1 + 1; y++) {
        const py = Math.round((y * CELL - view.y) * view.scale) + 0.5;
        if (py < 0 || py > h) continue;
        ctx.moveTo(Math.max(0, (x0 * CELL - view.x) * view.scale), py);
        ctx.lineTo(Math.min(w, ((x1 + 1) * CELL - view.x) * view.scale), py);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    const visible = (r: { left: number; top: number; size: number }) => r.left + r.size >= 0 && r.top + r.size >= 0 && r.left <= w && r.top <= h;
    const squares = size >= 48;
    for (const c of data.cells) {
      const r = cellRect(view, c.cell);
      if (!visible(r)) continue;
      const shared = c.state === 'Shared';
      const exact = shared && squares && !!c.clash?.length;
      ctx.fillStyle = shared ? warn : modColor(byKey.get(c.maps[0]));
      ctx.globalAlpha = (exact ? 0.2 : shared ? 0.6 : 0.45) * fill;
      ctx.fillRect(r.left, r.top, r.size, r.size);
      if (exact) {
        ctx.globalAlpha = 0.9;
        const s = view.scale;
        for (const [y, x, n] of c.clash!) ctx.fillRect(r.left + x * s, r.top + y * s, Math.max(1, n * s), Math.max(1, s));
      }
      ctx.globalAlpha = 1;
      if (shared && size >= 6) {
        ctx.strokeStyle = warn;
        ctx.lineWidth = 1.5;
        ctx.strokeRect(r.left + 0.75, r.top + 0.75, r.size - 1.5, r.size - 1.5);
      } else if ((c.state === 'Touching' || c.state === 'SameMod') && size >= 6) {
        ctx.strokeStyle = ink;
        ctx.globalAlpha = 0.6;
        ctx.lineWidth = 1;
        ctx.strokeRect(r.left + 0.5, r.top + 0.5, r.size - 1, r.size - 1);
        ctx.globalAlpha = 1;
      }
    }

    ctx.lineWidth = size >= 6 ? 2 : 1.5;
    for (const m of data.maps) {
      if (!m.active || m.cells.length === 0) continue;
      ctx.strokeStyle = modColor(m);
      ctx.beginPath();
      outline(ctx, m.cells);
      ctx.stroke();
    }

    ctx.setLineDash([4, 3]);
    ctx.lineWidth = 1.5;
    for (const m of previews) {
      ctx.strokeStyle = m.compatible ? accent : warn;
      for (const cell of m.cells) {
        const r = cellRect(view, cell);
        if (visible(r)) ctx.strokeRect(r.left + 1, r.top + 1, Math.max(1, r.size - 2), Math.max(1, r.size - 2));
      }
    }
    ctx.setLineDash([]);

    const sel = selected ? byKey.get(selected) : undefined;
    if (sel) {
      ctx.strokeStyle = ctx.fillStyle = ink;
      ctx.lineWidth = 2;
      ctx.globalAlpha = size >= 12 ? 1 : 0.4;
      for (const cell of sel.cells) {
        const r = cellRect(view, cell);
        if (!visible(r)) continue;
        if (size >= 12) ctx.strokeRect(r.left + 1, r.top + 1, r.size - 2, r.size - 2);
        else ctx.fillRect(r.left, r.top, r.size, r.size);
      }
      ctx.globalAlpha = 1;
    }

    const pointedMap = hover ? byKey.get(hover) : undefined;
    if (pointedMap && pointedMap.cells.length) {
      ctx.beginPath();
      outline(ctx, pointedMap.cells);
      ctx.strokeStyle = halo;
      ctx.lineWidth = 5;
      ctx.stroke();
      ctx.strokeStyle = ink;
      ctx.lineWidth = 2.5;
      ctx.stroke();
    }

    if (layers.spawns) {
      const dot = (x: number, y: number, fillStyle: string, d: number) => {
        const px = (x - view.x) * view.scale;
        const py = (y - view.y) * view.scale;
        if (px < -d || py < -d || px > w + d || py > h + d) return;
        ctx.fillStyle = halo;
        ctx.fillRect(px - d / 2 - 1, py - d / 2 - 1, d + 2, d + 2);
        ctx.fillStyle = fillStyle;
        ctx.fillRect(px - d / 2, py - d / 2, d, d);
      };
      for (const town of data.towns) for (const [x, y] of town.spawns) dot(x, y, ink, 3);
      for (const m of data.maps) if (m.active) for (const [x, y] of m.spawns) dot(x, y, modColor(m), 5);
    }

    const font = getComputedStyle(canvas).fontFamily;
    ctx.textBaseline = 'middle';
    const taken: [number, number, number, number][] = [];
    const label = (name: string, x: number, y: number, text: string, mark: string | null, dy = 0) => {
      const px = (x - view.x) * view.scale;
      const py = (y - view.y) * view.scale + dy;
      if (px < 0 || py < 0 || px > w || py > h) return;
      const markW = mark ? 12 : 0;
      const tw = ctx.measureText(name).width + markW;
      const left = px - tw / 2;
      const area: [number, number, number, number] = [left - 3, py - 9, left + tw + 3, py + 9];
      if (taken.some((b) => b[0] < area[2] && area[0] < b[2] && b[1] < area[3] && area[1] < b[3])) return;
      taken.push(area);
      if (mark) {
        ctx.fillStyle = halo;
        ctx.fillRect(left - 1, py - 5, 10, 10);
        ctx.fillStyle = mark;
        ctx.fillRect(left, py - 4, 8, 8);
      }
      ctx.textAlign = 'left';
      ctx.lineWidth = 3.5;
      ctx.strokeStyle = halo;
      ctx.strokeText(name, left + markW, py);
      ctx.fillStyle = text;
      ctx.fillText(name, left + markW, py);
    };
    ctx.font = `600 ${size >= 6 ? 12 : 11}px ${font}`;
    const named = [...data.maps.filter((m) => m.active), ...previews].filter((m) => m.cells.length > 0);
    named.sort((a, b) => Number(b.key === selected) - Number(a.key === selected));
    for (const m of named) {
      const ys = m.cells.map((c) => c[1]);
      const top = Math.min(...ys);
      const bottom = Math.max(...ys) + 1;
      const x = (m.cells.reduce((s, c) => s + c[0], 0) / m.cells.length + 0.5) * CELL;
      const mark = m.active ? modColor(m) : m.compatible ? accent : warn;
      if ((bottom - top) * size < 24) label(m.title, x, bottom * CELL, ink, mark, 10);
      else label(m.title, x, (m.cells.reduce((s, c) => s + c[1], 0) / m.cells.length + 0.5) * CELL, ink, mark);
    }
    ctx.font = `500 ${size >= 6 ? 12 : 11}px ${font}`;
    for (const town of data.towns) {
      if (!town.spawns.length) continue;
      const x = town.spawns.reduce((s, p) => s + p[0], 0) / town.spawns.length;
      const y = town.spawns.reduce((s, p) => s + p[1], 0) / town.spawns.length;
      label(town.name.replace(/, KY$/, ''), x, y, ink2, null);
    }

    if (focus) {
      const r = cellRect(view, focus);
      ctx.strokeStyle = accent;
      ctx.lineWidth = 2;
      ctx.strokeRect(r.left - 1, r.top - 1, r.size + 2, r.size + 2);
    }
  }

  function redraw() {
    if (!frame) frame = requestAnimationFrame(draw);
  }

  $effect(() => {
    void [data, previews, selected, hover, slots, focus, layers.game, layers.spawns, layers.towns, fill, view, w, h];
    redraw();
  });

  $effect(() => {
    if (!fitted && w > 0 && h > 0) {
      view = fit(data.bounds, w, h);
      fitted = true;
    }
  });

  let shown: string | null = null;
  $effect(() => {
    const m = selected ? byKey.get(selected) : undefined;
    if (!m || selected === shown || m.cells.length === 0 || w === 0) return;
    shown = selected;
    const onScreen = m.cells.some((c) => {
      const r = cellRect(view, c);
      return r.left + r.size > 0 && r.top + r.size > 0 && r.left < w && r.top < h;
    });
    if (onScreen) return;
    const xs = m.cells.map((c) => c[0]);
    const ys = m.cells.map((c) => c[1]);
    const cx = ((Math.min(...xs) + Math.max(...xs) + 1) / 2) * CELL;
    const cy = ((Math.min(...ys) + Math.max(...ys) + 1) / 2) * CELL;
    view = { ...view, x: cx - w / 2 / view.scale, y: cy - h / 2 / view.scale };
  });

  onMount(() => {
    const ro = new ResizeObserver(([e]) => {
      w = e.contentRect.width;
      h = e.contentRect.height;
    });
    ro.observe(box);
    return () => {
      ro.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  });

  let drag: { x: number; y: number; vx: number; vy: number; moved: boolean } | null = null;

  function point(cell: Cell | null, x = 0, y = 0) {
    const same = pointed && cell && pointed.cell[0] === cell[0] && pointed.cell[1] === cell[1];
    pointed = cell ? { cell, x, y } : null;
    if (!same) onhover(cell);
  }

  function down(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
  }
  function move(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    if (!drag) {
      point(cellAt(view, px, py), px, py);
      return;
    }
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) {
      drag.moved = true;
      point(null);
    }
    if (drag.moved) view = { ...view, x: drag.vx - dx / view.scale, y: drag.vy - dy / view.scale };
  }
  function up(e: PointerEvent) {
    if (drag && !drag.moved) {
      const r = canvas.getBoundingClientRect();
      const c = cellAt(view, e.clientX - r.left, e.clientY - r.top);
      focus = c;
      onpick(c);
    }
    drag = null;
  }
  function wheel(e: WheelEvent) {
    e.preventDefault();
    const r = canvas.getBoundingClientRect();
    view = zoomAt(view, e.deltaY < 0 ? 1.25 : 0.8, e.clientX - r.left, e.clientY - r.top);
    point(cellAt(view, e.clientX - r.left, e.clientY - r.top), e.clientX - r.left, e.clientY - r.top);
  }

  function reveal(c: Cell) {
    const r = cellRect(view, c);
    if (r.left < 0 || r.top < 0 || r.left + r.size > w || r.top + r.size > h) {
      view = { ...view, x: (c[0] + 0.5) * CELL - w / 2 / view.scale, y: (c[1] + 0.5) * CELL - h / 2 / view.scale };
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key.startsWith('Arrow')) {
      e.preventDefault();
      const start: Cell = focus ?? cellAt(view, w / 2, h / 2);
      focus = focus ? moveFocus(start, e.key, data.bounds) : start;
      reveal(focus);
    } else if (e.key === 'Enter' && focus) {
      e.preventDefault();
      onpick(focus);
    } else if (e.key === '+' || e.key === '=') {
      view = zoomAt(view, 1.25, w / 2, h / 2);
    } else if (e.key === '-') {
      view = zoomAt(view, 0.8, w / 2, h / 2);
    } else if (e.key === '0') {
      view = fit(data.bounds, w, h);
    } else if (e.key === 'Escape') {
      onclear();
    }
  }

  const flipX = $derived(!!pointed && pointed.x > w - 280);
  const flipY = $derived(!!pointed && pointed.y > h - 140);
</script>

<div class="map" bind:this={box}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="surface" role="application" aria-label={t('maps.canvas')} tabindex="0" onkeydown={key}>
    <canvas bind:this={canvas} aria-hidden="true" onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => point(null)} onwheel={wheel}></canvas>
  </div>
  {#if pointed}
    <div class="tip" class:fx={flipX} class:fy={flipY} style:left="{pointed.x}px" style:top="{pointed.y}px" aria-hidden="true">
      {@render tip(pointed.cell)}
    </div>
  {/if}
  <div class="zoom">
    <button class="icon" aria-label={t('maps.zoom_in')} title={t('maps.zoom_in')} onclick={() => (view = zoomAt(view, 1.25, w / 2, h / 2))}><Plus size={14} strokeWidth={2} /></button>
    <button class="icon" aria-label={t('maps.zoom_out')} title={t('maps.zoom_out')} onclick={() => (view = zoomAt(view, 0.8, w / 2, h / 2))}><Minus size={14} strokeWidth={2} /></button>
    <button class="icon" aria-label={t('maps.fit')} title={t('maps.fit')} onclick={() => (view = fit(data.bounds, w, h))}><Maximize size={14} strokeWidth={2} /></button>
  </div>
</div>

<style>
  .map {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--ground);
  }
  .surface {
    position: absolute;
    inset: 0;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: grab;
    touch-action: none;
    font-family: var(--font);
  }
  canvas:active {
    cursor: grabbing;
  }
  .surface:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .tip {
    position: absolute;
    z-index: 1;
    max-width: 260px;
    padding: 6px 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    color: var(--ink);
    font-size: var(--t-xs);
    pointer-events: none;
    transform: translate(14px, 14px);
  }
  .tip.fx {
    transform: translate(calc(-100% - 14px), 14px);
  }
  .tip.fy {
    transform: translate(14px, calc(-100% - 14px));
  }
  .tip.fx.fy {
    transform: translate(calc(-100% - 14px), calc(-100% - 14px));
  }
  .zoom {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    background: none;
    color: var(--ink);
    cursor: pointer;
  }
  .icon + .icon {
    border-top: 1px solid var(--line);
  }
  .icon:hover {
    background: var(--accent-soft);
  }
  .icon:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
