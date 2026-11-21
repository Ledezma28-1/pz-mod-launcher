<script lang="ts">
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Copy from '@lucide/svelte/icons/copy';
  import Lock from '@lucide/svelte/icons/lock';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import X from '@lucide/svelte/icons/x';
  import CircleMinus from '@lucide/svelte/icons/circle-minus';
  import { api, inTauri, message } from '../api';
  import { app } from '../state.svelte';
  import { i18n, t } from '../i18n.svelte';
  import type { Cell, CellInfo, MapInfo, MapsView } from '../types';
  import { cellsBox, compatibleBatch, findTarget, modSlots } from '../map-view';
  import MapCanvas from './MapCanvas.svelte';
  import MapDetail from './MapDetail.svelte';

  const BASE = 'Muldraugh, KY';

  let data = $state<MapsView | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<string | null>(null);
  let focus = $state<Cell | null>(null);
  let filter = $state<Cell | null>(null);
  let previewKeys = $state<Set<string>>(new Set());
  let onlyCompatible = $state(false);
  let layers = $state({ game: true, spawns: false, towns: true, ...stored('mapas-capas', {}) });
  let fillPct = $state<number>(stored('mapas-relleno', 100));
  let busy = $state(false);
  let asked = 0;
  let hoverKey = $state<string | null>(null);
  let pointedCell = $state<Cell | null>(null);
  let query = $state('');
  let canvasRef = $state<{ goTo: (squares: [number, number, number, number]) => void }>();

  function stored<T>(key: string, fallback: T): T {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : (JSON.parse(v) as T);
    } catch {
      return fallback;
    }
  }
  $effect(() => {
    const v = JSON.stringify(layers);
    const f = String(fillPct);
    try {
      localStorage.setItem('mapas-capas', v);
      localStorage.setItem('mapas-relleno', f);
    } catch {
    }
  });

  async function load() {
    const id = app.profileId;
    if (!id) return;
    const n = ++asked;
    const stale = () => n !== asked || app.profileId !== id;
    try {
      const v = await api.mapsView(id, i18n.language);
      if (stale()) return;
      data = v;
      error = null;
    } catch (e) {
      if (!stale()) error = message(e);
    }
  }

  $effect(() => {
    void app.view;
    void i18n.language;
    load();
  });

  const has = (m: MapInfo, c: Cell | null) => !c || m.cells.some(([x, y]) => x === c[0] && y === c[1]);
  const active = $derived((data?.maps ?? []).filter((m) => m.active));
  const candidates = $derived((data?.maps ?? []).filter((m) => !m.active));
  const shownActive = $derived(active.filter((m) => has(m, filter)));
  const shownCandidates = $derived(candidates.filter((m) => has(m, filter) && (!onlyCompatible || m.compatible)));
  const previews = $derived(candidates.filter((m) => previewKeys.has(m.key)));
  const clashing = $derived(new Set((data?.cells ?? []).filter((c) => c.state === 'Shared').flatMap((c) => c.maps)));
  const map = $derived(data?.maps.find((m) => m.key === selected) ?? null);
  const byKey = $derived(new Map((data?.maps ?? []).map((m) => [m.key, m])));
  const cellOf = (c: Cell | null) => (c && data ? (data.cells.find((i) => i.cell[0] === c[0] && i.cell[1] === c[1]) ?? null) : null);
  const slots = $derived(modSlots(data?.maps ?? []));
  const swatch = (m: MapInfo | undefined) => {
    const s = m ? slots.get(m.mod_key) : undefined;
    return s === undefined ? 'var(--ink-3)' : `var(--map-${s + 1})`;
  };
  const pointedKeys = $derived(new Set(cellOf(pointedCell)?.maps ?? []));

  function stateText(c: CellInfo) {
    if (c.state === 'Shared') return c.squares === null ? t('maps.cell_unknown') : t('maps.cell_shared', { n: c.squares });
    if (c.state === 'Touching') return t('maps.cell_touching');
    return '';
  }

  const targets = $derived.by(() => {
    if (!data) return [];
    const out: { name: string; box: [number, number, number, number]; key?: string }[] = [];
    for (const town of data.towns) {
      if (!town.spawns.length) continue;
      const xs = town.spawns.map((s) => s[0]);
      const ys = town.spawns.map((s) => s[1]);
      out.push({ name: town.name.replace(/, KY$/, ''), box: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)] });
    }
    for (const m of data.maps) if (m.cells.length) out.push({ name: m.title, box: cellsBox(m.cells), key: m.key });
    return out;
  });

  function go(e?: Event) {
    e?.preventDefault();
    if (!query.trim()) return;
    const found = findTarget(targets, query);
    if (!found) {
      app.notify(t('maps.goto_none', { q: query.trim() }), 'error');
      return;
    }
    canvasRef?.goTo(found.box);
    if (found.key) selected = found.key;
  }
  const pickedCell = $derived(cellOf(filter));
  const mapLine = $derived('Map=' + (data?.server_line ?? []).join(';'));

  const announce = $derived.by(() => {
    if (!focus || !data) return '';
    const c = cellOf(focus);
    const [x, y] = focus;
    if (!c) return t('maps.cell_base', { x, y });
    const titles = c.maps.map((k) => byKey.get(k)?.title ?? k);
    const extra = [stateText(c), c.urban ? t('maps.cell_urban') : ''].filter(Boolean);
    return t('maps.cell_maps', { x, y, m: [titles.join(' > '), ...extra].join('; ') });
  });

  const cells = (m: MapInfo) => (m.cells.length === 0 ? t('maps.spawn_only') : m.cells.length === 1 ? t('maps.cells_one') : t('maps.cells', { n: m.cells.length }));

  const folders = () => [...new Set(active.map((m) => m.folder))];

  function current() {
    const order = data?.saved_order ? folders() : [];
    const out = [...new Set(active.filter((m) => !m.server).map((m) => m.folder))];
    return { order, out };
  }

  async function save(order: string[], out: string[], text: string) {
    const id = app.profileId;
    if (!id) return;
    const before = current();
    busy = true;
    try {
      await api.saveMaps(id, order, out);
      app.notify(text, 'ok', async () => {
        await api.saveMaps(id, before.order, before.out);
        await app.loadView();
      });
      await app.loadView();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      busy = false;
    }
  }

  function move(i: number, step: number) {
    const { out } = current();
    const order = folders();
    const j = i + step;
    if (j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    void save(order, out, t('maps.order_saved'));
  }

  function toggleServer(m: MapInfo) {
    const { order, out } = current();
    const next = m.server ? [...out, m.folder] : out.filter((f) => f !== m.folder);
    void save(order, next, t(m.server ? 'maps.server_out' : 'maps.server_in', { m: m.title }));
  }

  async function enable(maps: MapInfo[]) {
    const id = app.profileId;
    if (!id || maps.length === 0) return;
    const rows = app.view?.rows ?? [];
    const keys = [...new Set(maps.flatMap((m) => [m.mod_key, ...m.require_keys]))];
    const ids = [...new Set(maps.flatMap((m) => [m.mod_id, ...m.require_ids]).filter((x): x is string => !!x))];
    const wereDisabled = rows.filter((r) => ids.includes(r.id) && !r.enabled).map((r) => r.id);
    const added = ids.filter((i) => !rows.some((r) => r.id === i));
    busy = true;
    try {
      await api.add(id, keys, true);
      previewKeys = new Set([...previewKeys].filter((k) => !maps.some((m) => m.key === k)));
      app.notify(maps.length === 1 ? t('maps.enabled_one', { m: maps[0].title }) : t('maps.enabled_many', { n: maps.length }), 'ok', async () => {
        if (added.length) await api.removeMany(id, added);
        for (const r of wereDisabled) await api.setEnabled(id, r, false);
        await app.loadView();
      });
      await app.loadView();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      busy = false;
    }
  }

  const batch = $derived(compatibleBatch(candidates));

  const siblings = (m: MapInfo) => active.filter((a) => a.mod_key === m.mod_key).length;
  const disableTitle = (m: MapInfo) => (siblings(m) > 1 ? t('maps.disable_many', { mod: m.mod_name, n: siblings(m) }) : t('maps.disable_title', { m: m.title }));

  async function disable(m: MapInfo) {
    const id = app.profileId;
    const mod = m.mod_id;
    if (!id || !mod) return;
    busy = true;
    try {
      await api.setEnabled(id, mod, false);
      if (selected && active.some((a) => a.key === selected && a.mod_key === m.mod_key)) selected = null;
      app.notify(t('maps.disabled_one', { m: siblings(m) > 1 ? m.mod_name : m.title }), 'ok', async () => {
        await api.setEnabled(id, mod, true);
        await app.loadView();
      });
      await app.loadView();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      busy = false;
    }
  }

  function togglePreview(m: MapInfo) {
    const next = new Set(previewKeys);
    if (next.has(m.key)) next.delete(m.key);
    else next.add(m.key);
    previewKeys = next;
  }

  function pick(c: Cell) {
    const info = cellOf(c);
    filter = info ? c : null;
    selected = info?.maps[0] ?? null;
  }

  async function copyLine() {
    try {
      await navigator.clipboard.writeText(mapLine);
      app.notify(t('export.copied', { q: 'Map=' }));
    } catch {
      app.notify(t('export.not_copied'), 'error');
    }
  }
</script>

{#snippet tip(c: Cell)}
  {@const info = cellOf(c)}
  <span class="tip-head">{t('maps.filter_cell', { x: c[0], y: c[1] })}</span>
  {#if !info}
    <span class="tip-dim">{t('maps.tip_base')}</span>
  {:else}
    <ol class="tip-maps">
      {#each info.maps as k (k)}
        <li><span class="dot" style:background={swatch(byKey.get(k))}></span>{byKey.get(k)?.title ?? k}</li>
      {/each}
    </ol>
    {#if stateText(info)}<span class:severity-warning={info.state === 'Shared'}>{stateText(info)}</span>{/if}
    {#if info.urban}<span class="tip-dim">{t('maps.cell_urban')}</span>{/if}
  {/if}
{/snippet}

<div class="wrap">
  {#if error}
    <p class="severity-blocks with-icon state"><CircleX size={14} strokeWidth={2} />{error}</p>
  {:else if !data}
    <p class="dim state">{t('maps.loading')}</p>
  {:else}
    <div class="tab" class:with-side={!!map || !!pickedCell}>
      <div class="lists">
        {#if filter}
          <div class="filter">
            <span>{t('maps.filter_cell', { x: filter[0], y: filter[1] })}</span>
            <button class="icon" aria-label={t('maps.filter_clear')} title={t('maps.filter_clear')} onclick={() => (filter = null)}><X size={14} strokeWidth={2} /></button>
          </div>
        {/if}
        <section>
          <div class="head-section">
            <h2>{t('maps.profile')}</h2>
            {#if data.saved_order}
              <button class="btn subtle" disabled={busy} onclick={() => save([], current().out, t('maps.order_auto_done'))}>{t('maps.order_auto')}</button>
            {/if}
          </div>
          <p class="text">{t('maps.profile_text')}</p>
          {#if active.length === 0}
            <p class="dim">{t('maps.none')}</p>
          {:else}
            <div class="list" role="table" aria-label={t('maps.profile')}>
              <div class="row header" role="row">
                <span class="c-check" role="columnheader" title={t('maps.server_hint')}>{t('maps.col_server')}</span>
                <span class="c-name" role="columnheader">{t('maps.col_map')}</span>
                <span class="c-end" role="columnheader">{t('maps.col_order')}</span>
              </div>
              {#each shownActive as m (m.key)}
                {@const i = folders().indexOf(m.folder)}
                <!-- svelte-ignore a11y_interactive_supports_focus -->
                <div class="row" class:sel={selected === m.key} class:hot={pointedKeys.has(m.key)} role="row" onmouseenter={() => (hoverKey = m.key)} onmouseleave={() => (hoverKey = null)} onfocusin={() => (hoverKey = m.key)} onfocusout={() => (hoverKey = null)}>
                  <span class="c-check" role="cell">
                    <input type="checkbox" checked={m.server} disabled={busy} title={t('maps.server_hint')} aria-label={`${t('maps.server')}: ${m.title}`} onchange={() => toggleServer(m)} />
                  </span>
                  <span class="c-name" role="cell"><button class="name" onclick={() => (selected = m.key)}>
                    <span class="title"><span class="dot" style:background={swatch(m)}></span>{m.title}</span>
                    <span class="meta">
                      <span class="num">{cells(m)}</span>
                      {#if clashing.has(m.key)}<span class="severity-warning flag"><TriangleAlert size={12} strokeWidth={2} />{t('maps.incompatible')}</span>{/if}
                    </span>
                  </button></span>
                  <span class="c-end arrows" role="cell">
                    <button class="arrow" aria-label={disableTitle(m)} title={disableTitle(m)} disabled={busy || !m.mod_id} onclick={() => disable(m)}><CircleMinus size={14} strokeWidth={2} /></button>
                    <button class="arrow" aria-label={t('table.up', { n: m.title })} disabled={busy || i <= 0} onclick={() => move(i, -1)}><ChevronUp size={14} strokeWidth={2} /></button>
                    <button class="arrow" aria-label={t('table.down', { n: m.title })} disabled={busy || i < 0 || i >= folders().length - 1} onclick={() => move(i, 1)}><ChevronDown size={14} strokeWidth={2} /></button>
                  </span>
                </div>
              {/each}
              {#if !filter}
                <div class="row base" role="row">
                  <span class="c-check" role="cell"><input type="checkbox" checked disabled aria-label={`${t('maps.server')}: ${BASE}`} /></span>
                  <span class="c-name name" role="cell"><span class="title">{BASE}</span><span class="meta">{t('maps.base')}</span></span>
                  <span class="c-end fixed" role="cell" title={t('export.base_map_fixed')}><Lock size={13} strokeWidth={1.75} /></span>
                </div>
              {/if}
            </div>
            <p class="hint">{t('maps.server_hint')}</p>
          {/if}
          <div class="line-head">
            <span class="tag">{t('maps.map_line')}</span>
            <button class="btn subtle" onclick={copyLine}><Copy size={14} strokeWidth={1.75} />{t('export.copy')}</button>
          </div>
          <pre class="mono code">{mapLine}</pre>
          <button class="link" onclick={() => (app.tab = 'export')}>{t('maps.to_export')}</button>
        </section>

        <section>
          <h2>{t('maps.candidates')}</h2>
          {#if candidates.length === 0}
            <p class="dim">{t('maps.candidates_none')}</p>
          {:else}
            <div class="actions">
              <label class="checkbox"><input type="checkbox" bind:checked={onlyCompatible} />{t('maps.only_compatible')}</label>
              <button class="btn" disabled={busy || batch.length === 0} onclick={() => enable(batch)}>{t('maps.add_compatible')}</button>
            </div>
            {#if shownCandidates.length === 0}
              <p class="dim">{t(filter ? 'maps.filter_none' : 'maps.candidates_none_compatible')}</p>
            {:else}
            <div class="list" role="table" aria-label={t('maps.candidates')}>
              <div class="row header" role="row">
                <span class="c-check" role="columnheader">{t('maps.col_show')}</span>
                <span class="c-name" role="columnheader">{t('maps.col_map')}</span>
                <span class="c-end" role="columnheader"><span class="sr">{t('maps.enable')}</span></span>
              </div>
              {#each shownCandidates as m (m.key)}
                <!-- svelte-ignore a11y_interactive_supports_focus -->
                <div class="row" class:sel={selected === m.key} role="row" onmouseenter={() => (hoverKey = m.key)} onmouseleave={() => (hoverKey = null)} onfocusin={() => (hoverKey = m.key)} onfocusout={() => (hoverKey = null)}>
                  <span class="c-check" role="cell">
                    <input type="checkbox" checked={previewKeys.has(m.key)} aria-label={t('maps.preview', { m: m.title })} title={t('maps.preview', { m: m.title })} onchange={() => togglePreview(m)} />
                  </span>
                  <span class="c-name" role="cell"><button class="name" title={m.mod_name} onclick={() => (selected = m.key)}>
                    <span class="title">{m.title}</span>
                    <span class="meta">
                      <span class="num">{cells(m)}</span>
                      {#if m.compatible}
                        <span>{t('maps.compatible')}</span>
                      {:else}
                        <span class="severity-warning flag"><TriangleAlert size={12} strokeWidth={2} />{t('maps.incompatible')}</span>
                      {/if}
                    </span>
                  </button></span>
                  <span class="c-end" role="cell">
                    <button class="btn" disabled={busy} aria-label={`${t('maps.enable')}: ${m.title}`} onclick={() => enable([m])}>{t('maps.enable')}</button>
                  </span>
                </div>
              {/each}
            </div>
            {/if}
          {/if}
        </section>
      </div>

      <div class="center">
        <div class="bar">
          <form class="goto" onsubmit={go}>
            <input type="search" list="maps-goto" placeholder={t('maps.goto')} aria-label={t('maps.goto_label')} bind:value={query} onchange={() => go()} />
            <datalist id="maps-goto">
              {#each targets as tg (tg.key ?? 'town:' + tg.name)}<option value={tg.name}></option>{/each}
            </datalist>
          </form>
          <label class="checkbox"><input type="checkbox" bind:checked={layers.game} />{t('maps.layer_game')}</label>
          <label class="checkbox"><input type="checkbox" bind:checked={layers.towns} />{t('maps.layer_towns')}</label>
          <label class="checkbox"><input type="checkbox" bind:checked={layers.spawns} />{t('maps.layer_spawns')}</label>
          <label class="fill" title={t('maps.fill_hint')}>
            {t('maps.fill')}
            <input type="range" min="0" max="100" step="10" bind:value={fillPct} aria-valuetext={fillPct === 0 ? t('maps.fill_none') : `${fillPct} %`} />
          </label>
          <ul class="legend" aria-label={t('maps.legend')}>
            <li><span class="sw map"></span>{t('maps.legend_map')}</li>
            <li><span class="sw shared"></span>{t('maps.legend_shared')}</li>
            {#if layers.towns}<li><span class="sw town"></span>{t('maps.layer_towns')}</li>{/if}
            <li><span class="sw preview"></span>{t('maps.legend_preview')}</li>
          </ul>
        </div>
        {#if !inTauri}<p class="demo">{t('maps.demo')}</p>{/if}
        <MapCanvas bind:this={canvasRef} {data} {previews} {selected} hover={hoverKey} {slots} bind:focus {layers} fill={fillPct / 100} onpick={pick} onclear={() => (filter = null)} onhover={(c) => (pointedCell = c)} {tip} />
        <p class="sr" aria-live="polite">{announce}</p>
      </div>

      {#if map || pickedCell}
        <div class="side">
          <MapDetail {data} {map} cell={pickedCell} {busy} onselect={(k) => (selected = k)} onenable={(m) => enable([m])} ondisable={disable} />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    display: flex;
    flex: 1;
    min-height: 0;
    container-type: inline-size;
    background: var(--ground);
  }
  .state {
    margin: auto;
    padding: 16px;
  }
  .with-icon {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .tab {
    display: grid;
    flex: 1;
    min-height: 0;
    grid-template-columns: 340px minmax(0, 1fr);
    grid-template-areas: 'lists center';
  }
  .tab.with-side {
    grid-template-columns: 340px minmax(0, 1fr) 340px;
    grid-template-areas: 'lists center side';
  }
  .lists {
    grid-area: lists;
    min-height: 0;
    overflow: auto;
    padding: 12px 16px 16px;
    border-right: 1px solid var(--line);
    background: var(--surface);
  }
  .center {
    grid-area: center;
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
  }
  .side {
    grid-area: side;
    min-height: 0;
    overflow: auto;
    border-left: 1px solid var(--line);
    background: var(--surface);
  }
  @container (max-width: 1000px) {
    .tab,
    .tab.with-side {
      grid-template-columns: 300px minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) auto;
      grid-template-areas: 'lists center' 'side center';
    }
    .side {
      max-height: 45cqh;
      border-left: 0;
      border-right: 1px solid var(--line);
      border-top: 1px solid var(--line);
    }
  }
  @container (min-width: 1001px) and (max-width: 1200px) {
    .tab.with-side {
      grid-template-columns: 320px minmax(0, 1fr) 300px;
    }
  }
  section + section {
    margin-top: 20px;
  }
  .head-section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: var(--t-l);
    font-weight: 600;
  }
  .text {
    margin: 0 0 8px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .hint {
    margin: 6px 0 0;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .dim {
    color: var(--ink-3);
  }
  .filter {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
    padding: 2px 2px 2px 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--accent-soft);
    font-size: var(--t-s);
    font-weight: 600;
  }
  .list {
    border: 1px solid var(--line);
    border-radius: var(--radio);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) auto;
    align-items: center;
    min-height: 40px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    font-size: var(--t-m);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.header {
    min-height: 28px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.hot {
    box-shadow: inset 3px 0 0 var(--ink);
  }
  .dot {
    display: inline-block;
    width: 9px;
    height: 9px;
    margin-right: 6px;
    flex: none;
    border-radius: 2px;
    vertical-align: 0;
  }
  .tip-head {
    display: block;
    font-weight: 600;
  }
  .tip-dim {
    display: block;
    color: var(--ink-3);
  }
  .tip-maps {
    margin: 2px 0;
    padding: 0;
    list-style: none;
  }
  .tip-maps li {
    display: flex;
    align-items: center;
  }
  .tip-maps + span {
    display: block;
  }
  .goto input {
    width: 150px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--ground);
    color: var(--ink);
    font: inherit;
  }
  .goto input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 0;
  }
  .fill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .fill input {
    width: 80px;
    accent-color: var(--accent);
  }
  .row.base {
    background: var(--surface-2);
  }
  .c-check {
    display: flex;
    justify-content: center;
  }
  .c-end {
    display: flex;
    justify-content: flex-end;
    padding-right: 6px;
  }
  .header .c-end {
    padding-right: 10px;
  }
  .c-name {
    min-width: 0;
  }
  .name {
    display: flex;
    width: 100%;
    min-width: 0;
    flex-direction: column;
    gap: 1px;
    padding: 4px 8px 4px 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  button.name {
    cursor: pointer;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 8px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .flag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-weight: 600;
  }
  .arrows,
  .fixed {
    gap: 2px;
    color: var(--ink-3);
  }
  .fixed {
    padding-right: 12px;
  }
  .arrow,
  .icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: var(--radio);
    background: none;
    color: var(--ink-2);
    cursor: pointer;
  }
  .arrow:hover:not(:disabled),
  .icon:hover {
    background: var(--surface-2);
    color: var(--ink);
  }
  .arrow:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .name:focus-visible,
  .arrow:focus-visible,
  .icon:focus-visible,
  .link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .line-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 16px;
  }
  .tag {
    font-size: var(--t-s);
    font-weight: 600;
  }
  .code {
    margin: 4px 0 6px;
    padding: 6px 8px;
    max-height: 96px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--ground);
    font-size: var(--t-xs);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: var(--t-s);
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 4px 0 8px;
    font-size: var(--t-s);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    min-height: 36px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--t-s);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 0 0 0 auto;
    padding: 0;
    list-style: none;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .sw {
    width: 12px;
    height: 12px;
    flex: none;
  }
  .sw.map {
    background: linear-gradient(90deg, var(--map-1) 0 33%, var(--map-2) 33% 66%, var(--map-3) 66%);
  }
  .sw.shared {
    border: 1.5px solid var(--warn);
    background: color-mix(in srgb, var(--warn) 55%, var(--ground));
  }
  .sw.town {
    border: 1px solid color-mix(in srgb, var(--ink) 50%, transparent);
    background: color-mix(in srgb, var(--ink) 10%, var(--ground));
  }
  .sw.preview {
    border: 1.5px dashed var(--accent);
  }
  .demo {
    margin: 0;
    padding: 4px 12px;
    border-bottom: 1px solid var(--line);
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
