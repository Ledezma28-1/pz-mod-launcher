<script lang="ts">
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import Pin from '@lucide/svelte/icons/pin';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import Search from '@lucide/svelte/icons/search';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import ListChecks from '@lucide/svelte/icons/list-checks';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import Trash from '@lucide/svelte/icons/trash';
  import AddMods from './AddMods.svelte';
  import { api, inTauri } from '../api';
  import { app } from '../state.svelte';
  import { t, kindText, findingCause, findingText } from '../i18n.svelte';
  import type { Edge, Finding, Row, Removed } from '../types';

  const ROW = 30;
  const MARGIN = 40;

  const v = $derived(app.view!);
  const bare = (id: string) => id.replace(/^\\/, '');
  const worst = $derived.by(() => {
    const m = new Map<string, Finding>();
    for (const h of v.findings) if (h.severity !== 'Detail') for (const id of h.mods) if (!m.has(bare(id))) m.set(bare(id), h);
    return m;
  });
  let search = $state('');
  let container: HTMLDivElement | undefined = $state();

  const visible = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q) return v.rows;
    return v.rows.filter((f) => f.name.toLowerCase().includes(q) || f.id.toLowerCase().includes(q));
  });
  const filtering = $derived(search.trim() !== '');
  const position = $derived(new Map(v.rows.map((f, i) => [f.id, i])));

  type Thread = { lane: number; y1: number; y2: number; broken: boolean; dependent: boolean };
  const threads = $derived.by<Thread[]>(() => {
    const s = app.selection;
    if (!s || filtering) return [];
    const i = position.get(s);
    if (i === undefined) return [];
    const own = v.edges.filter((a) => (a.after === s || a.before === s) && a.rule !== 'UserPinned');
    const sorted = own
      .map((a) => ({ a, j: position.get(a.after === s ? a.before : a.after) }))
      .filter((x): x is { a: Edge; j: number } => x.j !== undefined)
      .sort((x, y) => Math.abs(x.j - i) - Math.abs(y.j - i));
    return sorted.map(({ a, j }, k) => ({
      lane: MARGIN - 10 - Math.min(k, 5) * 5,
      y1: i * ROW + ROW / 2,
      y2: j * ROW + ROW / 2,
      broken: a.broken,
      dependent: a.before === s,
    }));
  });

  let up = $state(0);
  let viewHeight = $state(900);
  let pending = false;
  function onScroll() {
    if (pending || !container) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      if (container) up = container.scrollTop;
    });
  }
  const SEGMENT = 600;
  const from = $derived(Math.floor((up - SEGMENT / 2) / SEGMENT) * SEGMENT);
  const to = $derived(Math.ceil((up + viewHeight + SEGMENT / 2) / SEGMENT) * SEGMENT);
  type Stroke = { d: string; offset: number; broken: boolean; dependent: boolean };
  const strokes = $derived.by(() => {
    const lines: Stroke[] = [];
    const ends = new Map<string, Stroke>();
    const points = new Map<string, { y: number; broken: boolean; dependent: boolean }>();
    const segments = new Map<string, { lane: number; broken: boolean; dependent: boolean; t: [number, number][] }>();
    for (const h of threads) {
      const r = Math.min(4, Math.abs(h.y2 - h.y1) / 2);
      const dir = h.y2 > h.y1 ? 1 : -1;
      const key = `${h.lane}|${h.broken}|${h.dependent}`;
      const g = segments.get(key) ?? { lane: h.lane, broken: h.broken, dependent: h.dependent, t: [] };
      g.t.push([Math.min(h.y1 + dir * r, h.y2 - dir * r), Math.max(h.y1 + dir * r, h.y2 - dir * r)]);
      segments.set(key, g);
      if (h.y1 >= from && h.y1 <= to)
        ends.set(`a|${key}|${dir}`, { d: `M ${MARGIN} ${h.y1} H ${h.lane + r} Q ${h.lane} ${h.y1} ${h.lane} ${h.y1 + dir * r}`, offset: 0, broken: h.broken, dependent: h.dependent });
      if (h.y2 >= from && h.y2 <= to) {
        ends.set(`b|${key}|${h.y2}`, { d: `M ${h.lane} ${h.y2 - dir * r} Q ${h.lane} ${h.y2} ${h.lane + r} ${h.y2} H ${MARGIN - 2}`, offset: 0, broken: h.broken, dependent: h.dependent });
        points.set(`${h.y2}|${h.broken}|${h.dependent}`, { y: h.y2, broken: h.broken, dependent: h.dependent });
      }
    }
    for (const g of segments.values()) {
      const t = g.t.map(([a, b]) => [Math.max(a, from), Math.min(b, to)] as [number, number]).filter(([a, b]) => b > a);
      t.sort((x, y) => x[0] - y[0]);
      const joined: [number, number][] = [];
      for (const [a, b] of t) {
        const u = joined[joined.length - 1];
        if (u && a <= u[1]) u[1] = Math.max(u[1], b);
        else joined.push([a, b]);
      }
      for (const [a, b] of joined) lines.push({ d: `M ${g.lane} ${a} V ${b}`, offset: a % 6, broken: g.broken, dependent: g.dependent });
    }
    return { lines: [...lines, ...ends.values()], points: [...points.values()] };
  });

  const unmet = $derived(new Set(v.edges.filter((a) => a.broken).flatMap((a) => [a.before, a.after])));

  let marked = $state<Set<string>>(new Set());
  let anchor: string | null = null;
  const visibleMarked = $derived(visible.filter((f) => marked.has(f.id)));

  function select(f: Row, e?: MouseEvent) {
    if (e && (e.ctrlKey || e.metaKey)) {
      const s = new Set(marked);
      if (s.size === 0 && app.selection && app.selection !== f.id) s.add(app.selection);
      if (s.has(f.id)) s.delete(f.id);
      else s.add(f.id);
      marked = s;
    } else if (e?.shiftKey && (anchor ?? app.selection)) {
      const from = visible.findIndex((x) => x.id === (anchor ?? app.selection));
      const to = visible.findIndex((x) => x.id === f.id);
      const [a, b] = from < to ? [from, to] : [to, from];
      marked = new Set(visible.slice(Math.max(0, a), b + 1).map((x) => x.id));
      app.selection = f.id;
      return;
    } else {
      marked = new Set();
    }
    app.selection = f.id;
    anchor = f.id;
  }

  async function remove(ids: string[]) {
    if (ids.length === 0) return;
    const id = v.id;
    const name = v.rows.find((f) => f.id === ids[0])?.name ?? ids[0];
    let removed: Removed[] = [];
    app.state = null;
    await app.run(async () => {
      removed = await api.removeMany(id, ids);
    });
    if ((app.state as { kind: string } | null)?.kind === 'error') return;
    marked = new Set();
    if (app.selection && ids.includes(app.selection)) app.selection = null;
    app.notify(ids.length === 1 ? t('table.removed', { n: name }) : t('table.removed_many', { n: ids.length }), 'ok', () => api.putBack(id, removed));
  }

  let selector = $state(false);
  let boxSelector: HTMLDivElement | undefined = $state();

  async function toggle(f: Row) {
    await app.runView(() => api.setEnabled(v.id, f.id, !f.enabled));
  }

  async function moveTo(from: number, to: number) {
    if (from === to) return;
    await app.run(() => api.moveTo(v.id, from, to));
  }

  const query = !inTauri && typeof location !== 'undefined' ? new URLSearchParams(location.search) : null;
  let manual = $state(readManual());
  function readManual() {
    if (query?.get('manual') === '1') return true;
    try {
      return localStorage.getItem('orden-manual') === '1';
    } catch {
      return false;
    }
  }
  function toggleManual() {
    manual = !manual;
    try {
      localStorage.setItem('orden-manual', manual ? '1' : '0');
    } catch {
    }
  }

  async function jumpTo(f: Row, n: number, field?: HTMLInputElement) {
    const to = Math.max(0, Math.min(v.rows.length - 1, Math.round(n) - 1));
    if (!Number.isFinite(n) || to === f.index) {
      if (field) field.value = String(f.index + 1);
      return;
    }
    app.selection = f.id;
    await moveTo(f.index, to);
    app.notify(t('table.moved', { n: f.name, p: to + 1 }));
    document.getElementById(`row-${to}`)?.scrollIntoView({ block: 'nearest' });
  }

  function keySlot(e: KeyboardEvent, f: Row) {
    const field = e.currentTarget as HTMLInputElement;
    if (e.key === 'Enter') {
      e.preventDefault();
      field.blur();
    } else if (e.key === 'Escape') {
      field.value = String(f.index + 1);
      field.blur();
    }
  }

  type ModCategory = 'blocks' | 'warning' | 'java' | 'lua' | 'map' | 'vehicle';
  const categories: ModCategory[] = ['blocks', 'warning', 'java', 'lua', 'map', 'vehicle'];
  function inCategory(f: Row, c: ModCategory): boolean {
    if (c === 'blocks') return f.severity === 'blocks' || !f.found;
    if (c === 'warning') return f.severity === 'warning';
    return (f.kind as string) === c;
  }
  let menu = $state(query?.get('menu') === '1');
  if (query?.get('add') === '1') selector = true;
  if (query?.get('mark')) marked = new Set(app.view!.rows.slice(2, 2 + Number(query.get('mark'))).map((f) => f.id));
  let boxMenu: HTMLDivElement | undefined = $state();

  async function apply(rows: Row[], enabled: boolean) {
    menu = false;
    const ids = rows.filter((f) => f.enabled !== enabled).map((f) => f.id);
    if (ids.length === 0) {
      app.notify(t('select.nothing'));
      return;
    }
    app.state = null;
    const id = v.id;
    await app.run(() => api.setEnabledMany(id, ids, enabled));
    const failure = (app.state as { kind: string } | null)?.kind === 'error';
    if (!failure) {
      app.notify(t(enabled ? 'select.done_on' : 'select.done_off', { n: ids.length }), 'ok', () => api.setEnabledMany(id, ids, !enabled));
    }
  }

  function outsideOfMenu(e: MouseEvent) {
    if (menu && boxMenu && !boxMenu.contains(e.target as Node)) menu = false;
    if (selector && boxSelector && !boxSelector.contains(e.target as Node)) selector = false;
  }

  function keyboard(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement && e.target.type !== 'checkbox') return;
    if (filtering && (e.key === 'ArrowUp' || e.key === 'ArrowDown') && e.altKey) return;
    const list = visible;
    if (list.length === 0) return;
    const current = list.findIndex((f) => f.id === app.selection);
    const go = (k: number) => {
      const f = list[Math.max(0, Math.min(list.length - 1, k))];
      app.selection = f.id;
      document.getElementById(`row-${f.index}`)?.scrollIntoView({ block: 'nearest' });
    };
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const step = e.key === 'ArrowDown' ? 1 : -1;
      if (e.altKey && current >= 0) {
        if (app.taken) return;
        const f = list[current];
        const target = f.index + step;
        if (target >= 0 && target < v.rows.length) moveTo(f.index, target);
        return;
      }
      go(current < 0 ? 0 : current + step);
    } else if (e.key === 'Home') {
      e.preventDefault();
      go(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      go(list.length - 1);
    } else if (e.key === ' ' && current >= 0) {
      e.preventDefault();
      toggle(list[current]);
    } else if (e.key === 'Delete') {
      e.preventDefault();
      remove(marked.size ? [...marked] : current >= 0 ? [list[current].id] : []);
    } else if (e.key === 'Escape' && marked.size) {
      marked = new Set();
    }
  }

  let drag = $state<{ from: number; to: number } | null>(null);
  let body: HTMLDivElement | undefined = $state();

  function startDrag(e: PointerEvent, f: Row) {
    if (filtering || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { from: f.index, to: f.index };
    app.selection = f.id;
  }
  function moveDrag(e: PointerEvent) {
    if (!drag || !body) return;
    const box = body.getBoundingClientRect();
    const k = Math.floor((e.clientY - box.top) / ROW);
    drag.to = Math.max(0, Math.min(v.rows.length - 1, k));
    if (container) {
      const c = container.getBoundingClientRect();
      if (e.clientY < c.top + 40) container.scrollTop -= 12;
      else if (e.clientY > c.bottom - 40) container.scrollTop += 12;
    }
  }
  async function unpin() {
    if (!drag) return;
    const { from, to } = drag;
    drag = null;
    await moveTo(from, to);
  }

  const markY = $derived(
    drag ? (drag.to > drag.from ? (drag.to + 1) * ROW : drag.to * ROW) : 0,
  );
</script>

<svelte:window onclick={outsideOfMenu} />

<div class="bar">
  <div class="sel" bind:this={boxSelector}>
    <button class="btn" aria-haspopup="dialog" aria-expanded={selector} onclick={() => (selector = !selector)}>
      <Plus size={14} strokeWidth={2} />{t('add.open')}
    </button>
    {#if selector}<AddMods close={() => (selector = false)} />{/if}
  </div>
  <label class="search">
    <Search size={14} strokeWidth={1.75} />
    <input class="field" type="search" bind:value={search} placeholder={t('table.search')} />
  </label>
  <button class="btn" class:enabled-manual={manual} aria-pressed={manual} title={t('table.manual_help')} onclick={toggleManual}>
    <ArrowUpDown size={14} strokeWidth={1.75} />{t('table.manual')}
  </button>
  <div class="sel" bind:this={boxMenu}>
    <button class="btn" aria-haspopup="menu" aria-expanded={menu} onclick={() => (menu = !menu)}>
      <ListChecks size={14} strokeWidth={1.75} />{t('select.button')}<ChevronDown size={13} strokeWidth={2} />
    </button>
    {#if menu}
      <div
        class="menu"
        role="menu"
        tabindex="-1"
        onkeydown={(e) => {
          if (e.key === 'Escape') menu = false;
        }}
      >
        <div class="menu-row">
          <button class="option" role="menuitem" disabled={app.taken} onclick={() => apply(visible, true)}>{t('select.all_on')}</button>
          <button class="option" role="menuitem" disabled={app.taken} onclick={() => apply(visible, false)}>{t('select.all_off')}</button>
        </div>
        <hr />
        {#each categories as c (c)}
          {@const rows = visible.filter((f) => inCategory(f, c))}
          <div class="menu-cat">
            <span class="cat">{t(`select.cat.${c}`)} <span class="num dim">{rows.length}</span></span>
            <button class="option" role="menuitem" disabled={rows.length === 0 || app.taken} onclick={() => apply(rows, true)}>{t('select.set_enabled')}</button>
            <button class="option" role="menuitem" disabled={rows.length === 0 || app.taken} onclick={() => apply(rows, false)}>{t('select.disable')}</button>
          </div>
        {/each}
        {#if filtering}<p class="note">{t('select.filter', { n: visible.length })}</p>{/if}
        <p class="note">{t('select.several')}</p>
      </div>
    {/if}
  </div>
</div>

{#if marked.size > 0}
  <div class="batch" role="toolbar" aria-label={t('batch.n', { n: marked.size })}>
    <strong class="num">{t('batch.n', { n: marked.size })}</strong>
    <button class="btn" disabled={app.taken} onclick={() => apply(visibleMarked, true)}>{t('select.set_enabled')}</button>
    <button class="btn" disabled={app.taken} onclick={() => apply(visibleMarked, false)}>{t('select.disable')}</button>
    <button class="btn remove-batch" disabled={app.taken} onclick={() => remove([...marked])}><Trash size={14} strokeWidth={1.75} />{t('batch.remove')}</button>
    <span class="gap"></span>
    <button class="btn subtle icon" aria-label={t('batch.clear')} title={t('batch.clear')} onclick={() => (marked = new Set())}><X size={14} strokeWidth={2} /></button>
  </div>
{/if}

<div
  class="table"
  class:manual
  role="grid"
  aria-rowcount={v.rows.length}
  aria-label={t('profile.tab.order')}
  aria-activedescendant={app.row ? `row-${app.row.index}` : undefined}
  tabindex="0"
  onkeydown={keyboard}
  onscroll={onScroll}
  bind:clientHeight={viewHeight}
  bind:this={container}
>
  <div class="header" role="row">
    <span class="c-margin" role="columnheader"><span class="hidden">{t('table.move')}</span></span>
    <span class="c-handle"></span>
    {#if manual}<span class="c-arrows"></span>{/if}
    <span class="c-enabled" role="columnheader"><span class="hidden">{t('detail.enabled')}</span></span>
    <span class="c-n" role="columnheader">{t('table.n')}</span>
    <span class="c-name" role="columnheader">{t('table.name')}</span>
    <span class="c-kind" role="columnheader">{t('table.kind')}</span>
    <span class="c-version" role="columnheader">{t('table.version')}</span>
    <span class="c-source" role="columnheader">{t('table.source')}</span>
    <span class="c-state" role="columnheader">{t('table.state')}</span>
    <span class="c-remove"></span>
  </div>

  {#if visible.length === 0}
    <p class="without-results">{t('table.without_results', { q: search })}</p>
  {:else}
    <div class="body" bind:this={body} style:height="{visible.length * ROW}px">
      <svg class="threads" width={MARGIN} height={visible.length * ROW} aria-hidden="true">
        {#each strokes.lines as h, k (k)}
          <path d={h.d} style:stroke-dashoffset={h.offset} class:broken={h.broken} class:dependent={h.dependent} />
        {/each}
        {#each strokes.points as p, k (k)}
          <circle cx={MARGIN - 2} cy={p.y} r="2.5" class:broken={p.broken} class:dependent={p.dependent} />
        {/each}
      </svg>
      {#if drag && drag.to !== drag.from}
        <div class="mark-unpin" style:top="{markY - 1}px"></div>
      {/if}

      {#each visible as f (f.id)}
        <div
          id="row-{f.index}"
          class="row"
          role="row"
          tabindex="-1"
          aria-selected={app.selection === f.id}
          aria-rowindex={f.index + 1}
          class:selected={app.selection === f.id}
          class:marked={marked.has(f.id)}
          class:inactive={!f.enabled}
          class:dragged={drag?.from === f.index}
          onclick={(e) => select(f, e)}
          onkeydown={() => {}}
        >
          <span class="c-margin">
            {#if unmet.has(f.id)}<span class="notch" title={t('sev.blocks')}></span>{/if}
          </span>
          <span
            class="c-handle"
            title={t('table.move')}
            role="presentation"
            class:disabled={filtering}
            onpointerdown={(e) => startDrag(e, f)}
            onpointermove={moveDrag}
            onpointerup={unpin}
            onpointercancel={() => (drag = null)}
          >
            <GripVertical size={14} strokeWidth={1.5} />
          </span>
          {#if manual}
            <span class="c-arrows">
              <button
                class="arrow"
                tabindex="-1"
                aria-label={t('table.up', { n: f.name })}
                title={t('table.up', { n: f.name })}
                disabled={f.index === 0 || app.taken}
                onclick={(e) => {
                  e.stopPropagation();
                  app.selection = f.id;
                  moveTo(f.index, f.index - 1);
                }}><ChevronUp size={14} strokeWidth={2} /></button
              >
              <button
                class="arrow"
                tabindex="-1"
                aria-label={t('table.down', { n: f.name })}
                title={t('table.down', { n: f.name })}
                disabled={f.index === v.rows.length - 1 || app.taken}
                onclick={(e) => {
                  e.stopPropagation();
                  app.selection = f.id;
                  moveTo(f.index, f.index + 1);
                }}><ChevronDown size={14} strokeWidth={2} /></button
              >
            </span>
          {/if}
          <span class="c-enabled" role="gridcell">
            <input
              type="checkbox"
              checked={f.enabled}
              aria-label={t('table.set_enabled', { n: f.name })}
              tabindex="-1"
              onclick={(e) => {
                e.stopPropagation();
                toggle(f);
              }}
            />
          </span>
          {#if manual}
            <span class="c-n" role="gridcell">
              <input
                class="slot num"
                inputmode="numeric"
                value={f.index + 1}
                aria-label={t('table.slot', { n: f.name })}
                title={t('table.slot', { n: f.name })}
                disabled={app.taken}
                onclick={(e) => e.stopPropagation()}
                onfocus={(e) => e.currentTarget.select()}
                onkeydown={(e) => keySlot(e, f)}
                onchange={(e) => jumpTo(f, Number(e.currentTarget.value.trim()), e.currentTarget)}
              />
            </span>
          {:else}
            <span class="c-n num" role="gridcell">{f.index + 1}</span>
          {/if}
          <span class="c-name" role="gridcell">
            <span class="name">{f.name}</span>
            {#if f.name !== f.id}<span class="id mono">{f.id}</span>{/if}
            {#if f.pinned}<span class="pinned" title={t('table.pinned')}><Pin size={12} strokeWidth={2} /></span>{/if}
          </span>
          <span class="c-kind" role="gridcell"><span class="tag" class:java={f.kind === 'java'}>{kindText(f.kind)}</span></span>
          <span class="c-version mono" role="gridcell">{f.version_mod ?? ''}</span>
          <span class="c-source" role="gridcell">{t(`source.${f.source}`)}</span>
          <span class="c-state" role="gridcell">
            {#if !f.found}
              <span class="severity-blocks est"><CircleX size={14} strokeWidth={2} />{t('table.not_found')}</span>
            {:else if f.severity}
              {@const h = worst.get(bare(f.id))}
              {@const Icon = f.severity === 'blocks' ? CircleX : f.severity === 'warning' ? TriangleAlert : Info}
              <span class="severity-{f.severity} est" title={h ? findingText(h) : undefined}>
                <Icon size={14} strokeWidth={2} />
                <span class="sr-only">{t(`sev.${f.severity}`)}: </span>
                <span class="cause">{h ? findingCause(h, bare(f.id)) : t(`sev.${f.severity}`)}</span>
              </span>
            {/if}
          </span>
          <span class="c-remove" role="gridcell">
            <button
              class="remove"
              tabindex="-1"
              aria-label={t('table.remove', { n: f.name })}
              title={t('table.remove', { n: f.name })}
              disabled={app.taken}
              onclick={(e) => {
                e.stopPropagation();
                remove(marked.has(f.id) ? [...marked] : [f.id]);
              }}><X size={14} strokeWidth={2} /></button
            >
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--ground);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 260px;
    min-width: 120px;
  }
  .search :global(svg) {
    position: absolute;
    left: 8px;
    color: var(--ink-3);
    pointer-events: none;
  }
  .search .field {
    width: 100%;
    padding-left: 28px;
  }
  .table {
    container-type: inline-size;
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
    outline-offset: -2px;
  }
  .header,
  .row {
    display: grid;
    grid-template-columns: 40px 18px 28px 44px minmax(220px, 1fr) 72px 84px 88px 164px 32px;
    align-items: center;
  }
  .manual .header,
  .manual .row {
    grid-template-columns: 40px 18px 46px 28px 56px minmax(220px, 1fr) 72px 84px 88px 164px 32px;
  }
  @container (max-width: 780px) {
    .header,
    .row {
      grid-template-columns: 40px 18px 28px 40px minmax(150px, 1fr) 56px 64px 0 132px 32px;
    }
    .manual .header,
    .manual .row {
      grid-template-columns: 40px 18px 46px 28px 52px minmax(130px, 1fr) 56px 60px 0 132px 32px;
    }
    .c-source {
      visibility: hidden;
      overflow: hidden;
    }
    .id {
      display: none;
    }
  }
  @container (max-width: 560px) {
    .header,
    .row {
      grid-template-columns: 36px 18px 28px 36px minmax(120px, 1fr) 52px 0 0 124px 28px;
    }
    .manual .header,
    .manual .row {
      grid-template-columns: 36px 18px 42px 28px 48px minmax(110px, 1fr) 52px 0 0 124px 28px;
    }
    .c-version {
      visibility: hidden;
      overflow: hidden;
    }
  }
  .header {
    position: sticky;
    top: 0;
    z-index: 2;
    height: 28px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .body {
    position: relative;
  }
  .row {
    position: relative;
    height: 30px;
    content-visibility: auto;
    contain-intrinsic-size: auto 30px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    cursor: default;
    transition: background var(--dur) var(--ease);
  }
  .row:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  .row.selected,
  .row.marked {
    background: var(--accent-soft);
  }
  .row.marked.selected {
    background: var(--accent-soft-2);
  }
  .c-remove {
    display: grid;
    place-items: center;
  }
  .remove {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radio);
    background: none;
    color: var(--ink-3);
    cursor: pointer;
    opacity: 0;
  }
  .row:hover .remove,
  .row.selected .remove,
  .row.marked .remove {
    opacity: 1;
  }
  .remove:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: var(--surface-2);
    color: var(--danger);
  }
  .batch {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 4px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--accent-soft);
    font-size: var(--t-s);
  }
  .gap {
    flex: 1;
  }
  .batch strong {
    margin-right: 4px;
    font-weight: 600;
  }
  .remove-batch:hover:not(:disabled) {
    color: var(--danger);
  }
  .icon {
    padding: 0 6px;
  }
  .row.inactive .name,
  .row.inactive .c-n,
  .row.inactive .c-version,
  .row.inactive .c-source,
  .row.inactive .tag {
    color: var(--ink-3);
  }
  .row.dragged {
    background: var(--accent-soft-2);
  }
  .c-n {
    color: var(--ink-3);
    text-align: right;
    padding-right: 10px;
    font-size: var(--t-s);
  }
  .c-name {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    padding-right: 12px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .id {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .pinned {
    align-self: center;
    display: inline-flex;
    color: var(--ink-3);
  }
  .c-version,
  .c-source {
    color: var(--ink-2);
    font-size: var(--t-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    font-size: var(--t-xs);
    color: var(--ink-2);
  }
  .tag.java {
    color: var(--accent);
    font-weight: 600;
  }
  .c-state {
    min-width: 0;
  }
  .est {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    font-size: var(--t-s);
    font-weight: 500;
  }
  .est :global(svg) {
    flex: none;
  }
  .cause {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-enabled {
    display: grid;
    place-items: center;
  }
  .c-enabled input {
    width: 14px;
    height: 14px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .c-handle {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--ink-3);
    cursor: grab;
    opacity: 0;
    touch-action: none;
  }
  .row:hover .c-handle,
  .row.selected .c-handle {
    opacity: 1;
  }
  .c-handle.disabled {
    cursor: default;
    opacity: 0 !important;
  }
  .c-margin {
    position: relative;
    height: 100%;
  }
  .notch {
    position: absolute;
    right: 3px;
    top: 9px;
    width: 3px;
    height: 12px;
    border-radius: var(--radio);
    background: var(--danger);
  }
  .threads {
    position: absolute;
    left: 0;
    top: 0;
    z-index: 1;
    pointer-events: none;
    overflow: visible;
  }
  .threads path {
    fill: none;
    stroke: var(--thread);
    stroke-width: 1.5;
    stroke-linecap: round;
  }
  .threads path.dependent {
    stroke: var(--thread-dependent);
  }
  .threads path.broken {
    stroke: var(--danger);
    stroke-dasharray: 3 3;
  }
  .threads circle {
    fill: var(--thread);
  }
  .threads circle.dependent {
    fill: var(--thread-dependent);
  }
  .threads circle.broken {
    fill: var(--danger);
  }
  .mark-unpin {
    position: absolute;
    left: 40px;
    right: 0;
    height: 2px;
    background: var(--accent);
    z-index: 3;
  }
  .enabled-manual {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .sel {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 10;
    min-width: 300px;
    padding: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    box-shadow: var(--shadow-menu);
    font-size: var(--t-s);
  }
  .menu hr {
    margin: 6px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
  .menu-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .menu-cat {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 4px;
    min-height: 28px;
  }
  .cat {
    padding-left: 6px;
  }
  .dim {
    color: var(--ink-3);
  }
  .option {
    height: 26px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radio);
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option:hover:not(:disabled),
  .option:focus-visible {
    background: var(--surface-2);
  }
  .option:disabled {
    color: var(--ink-3);
    cursor: default;
  }
  .note {
    margin: 6px 6px 2px;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .c-arrows {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .arrow {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radio);
    background: none;
    color: var(--ink-2);
    cursor: pointer;
  }
  .arrow:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: var(--surface-2);
    color: var(--ink);
  }
  .arrow:disabled {
    color: color-mix(in srgb, var(--ink-3) 45%, transparent);
    cursor: default;
  }
  .slot {
    width: 42px;
    height: 22px;
    margin-left: 4px;
    padding: 0 5px;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: var(--t-s);
    text-align: right;
  }
  .slot:hover {
    border-color: var(--line-strong);
  }
  .slot:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
    border-color: transparent;
  }
  .without-results {
    margin: 0;
    padding: 24px 16px;
    color: var(--ink-2);
  }
</style>
