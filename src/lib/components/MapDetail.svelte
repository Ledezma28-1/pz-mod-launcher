<script lang="ts">
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { inTauri } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { CellInfo, MapInfo, MapsView } from '../types';

  let {
    data,
    map,
    cell,
    busy,
    onselect,
    onenable,
    ondisable,
  }: {
    data: MapsView;
    map: MapInfo | null;
    cell: CellInfo | null;
    busy: boolean;
    onselect: (key: string) => void;
    onenable: (m: MapInfo) => void;
    ondisable: (m: MapInfo) => void;
  } = $props();

  const byKey = $derived(new Map(data.maps.map((m) => [m.key, m])));
  const clashes = $derived.by(() => {
    if (!map) return [];
    const out = new Set<string>();
    for (const c of data.cells) if (c.state === 'Shared' && c.maps.includes(map.key)) for (const k of c.maps) if (k !== map.key) out.add(k);
    return [...out].map((k) => byKey.get(k)?.title ?? k);
  });

  function stateText(c: CellInfo) {
    if (c.state === 'Shared') return c.squares === null ? t('maps.cell_unknown') : t('maps.cell_shared', { n: c.squares });
    if (c.state === 'Touching') return t('maps.cell_touching');
    return '';
  }
</script>

<aside class="detail">
  {#if map}
    <h3>{map.title}</h3>
    <p class="dim">{map.mod_name}</p>
    {#if map.thumb && inTauri}
      <img class="thumb" src={`http://pzmap.localhost/${map.thumb}`} alt="" />
    {/if}
    <dl>
      <dt>{t('maps.detail_folder')}</dt>
      <dd class="mono">{map.folder}</dd>
      <dt>{t('maps.detail_cells')}</dt>
      <dd class="num">{map.cells.length}</dd>
      {#if map.spawns.length}
        <dt>{t('maps.detail_spawns')}</dt>
        <dd class="num">{map.spawns.length}</dd>
      {/if}
      {#if map.requires.length}
        <dt>{t('maps.detail_requires')}</dt>
        <dd class="mono">{map.requires.join(', ')}</dd>
      {/if}
      {#if clashes.length}
        <dt>{t('maps.detail_clashes')}</dt>
        <dd class="severity-warning row-icon"><TriangleAlert size={13} strokeWidth={2} />{clashes.join(', ')}</dd>
      {/if}
    </dl>
    <div class="actions">
      {#if map.active}
        <button class="btn" disabled={busy || !map.mod_id} onclick={() => ondisable(map)}>{t('maps.disable')}</button>
      {:else}
        <button class="btn" disabled={busy} onclick={() => onenable(map)}>{t('maps.enable')}</button>
      {/if}
      {#if map.workshop_id}
        <button class="btn subtle" onclick={() => app.openInWorkshop(map.workshop_id, false)}><ExternalLink size={14} strokeWidth={1.75} />{t('maps.detail_workshop')}</button>
      {/if}
    </div>
  {/if}
  {#if cell}
    <section class="cell" class:sep={!!map}>
      <h4>{t('maps.filter_cell', { x: cell.cell[0], y: cell.cell[1] })}</h4>
      {#if cell.urban}<p class="dim">{t('maps.cell_urban')}</p>{/if}
      <p class="dim">{t('maps.detail_cell_maps')}</p>
      <ol>
        {#each cell.maps as k (k)}
          <li><button class="link" onclick={() => onselect(k)}>{byKey.get(k)?.title ?? k}</button></li>
        {/each}
      </ol>
      {#if stateText(cell)}<p class:severity-warning={cell.state === 'Shared'}>{stateText(cell)}</p>{/if}
    </section>
  {/if}
</aside>

<style>
  .detail {
    padding: 12px 16px 16px;
    font-size: var(--t-s);
  }
  h3 {
    margin: 0 0 2px;
    font-size: var(--t-l);
  }
  h4 {
    margin: 0 0 4px;
    font-size: var(--t-m);
  }
  .dim {
    margin: 0 0 8px;
    color: var(--ink-3);
  }
  .thumb {
    display: block;
    max-width: 100%;
    margin: 4px 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--radio);
  }
  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 10px;
    margin: 0 0 12px;
  }
  dt {
    color: var(--ink-3);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .row-icon {
    display: flex;
    gap: 4px;
    align-items: flex-start;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .cell.sep {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  ol {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
