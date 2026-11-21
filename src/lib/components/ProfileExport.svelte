<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Copy from '@lucide/svelte/icons/copy';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Share2 from '@lucide/svelte/icons/share-2';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { Export } from '../types';

  const id = $derived(app.view!.id);

  let data = $state<Export | null>(null);
  let error = $state<string | null>(null);
  let onlyEnabled = $state(true);

  async function load() {
    try {
      data = await api.doExport(id);
      error = null;
    } catch (e) {
      error = message(e);
    }
  }

  $effect(() => {
    void app.view;
    load();
  });

  const active = $derived((data?.mods ?? []).filter((m) => m.enabled));
  const modsLine = $derived('Mods=' + active.map((m) => m.id.replace(/^\\/, '')).join(';'));
  const workshop = $derived([...new Set(active.map((m) => m.workshop_id).filter((x): x is number => x !== null))]);
  const workshopLine = $derived('WorkshopItems=' + workshop.join(';'));
  const mapsLine = $derived('Map=' + (data?.maps ?? []).filter((m) => m.server).map((m) => m.folder).join(';'));
  const notInstalled = $derived(active.filter((m) => !m.found).length);
  const rows = $derived((data?.mods ?? []).filter((m) => !onlyEnabled || m.enabled));

  let shared = $state<string | null>(null);
  async function share() {
    try {
      shared = await api.shareProfile(id);
      app.notify(t('share.saved', { r: shared }));
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }
  const folderOf = (r: string) => r.slice(0, Math.max(r.lastIndexOf('\\'), r.lastIndexOf('/')));

  async function copy(text: string, que: string) {
    try {
      await navigator.clipboard.writeText(text);
      app.notify(t('export.copied', { q: que }));
    } catch {
      app.notify(t('export.not_copied'), 'error');
    }
  }

  const table = () =>
    [[t('export.slot'), t('table.name'), t('export.mod_id'), t('export.workshop_id')].join('\t'), ...rows.map((m) => [m.slot, m.name, m.id, m.workshop_id ?? ''].join('\t'))].join('\n');

</script>

{#snippet line(tag: string, text: string, n: number)}
  <div class="line">
    <div class="head-line">
      <span class="tag">{tag} <span class="num dim">{n}</span></span>
      <button class="btn" onclick={() => copy(text, tag)}><Copy size={14} strokeWidth={1.75} />{t('export.copy')}</button>
    </div>
    <pre class="mono code">{text}</pre>
  </div>
{/snippet}

<div class="screen">
  {#if error}
    <p class="severity-blocks with-icon"><CircleX size={14} strokeWidth={2} class="err-icon" />{error}</p>
  {:else if !data}
    <p class="dim">{t('profile.loading')}</p>
  {:else}
    <section>
      <h2>{t('share.title')}</h2>
      <p class="text">{t('share.text')}</p>
      <div class="row-share">
        <button class="btn" onclick={share}><Share2 size={14} strokeWidth={1.75} />{t('share.button')}</button>
        {#if shared}
          <span class="mono path">{shared}</span>
          <button class="btn subtle" onclick={() => api.openFolder(folderOf(shared!)).catch((e) => app.notify(message(e), 'error'))}>
            <FolderOpen size={14} strokeWidth={1.75} />{t('detail.folder_open')}
          </button>
        {/if}
      </div>
    </section>

    <section>
      <h2>{t('export.server')}</h2>
      <p class="text">{t('export.server_text')}</p>
      {#if notInstalled > 0}
        <p class="severity-warning row-icon"><TriangleAlert size={14} strokeWidth={2} />{t('export.not_installed', { n: notInstalled })}</p>
      {/if}
      {@render line('Mods', modsLine, active.length)}
      {@render line('WorkshopItems', workshopLine, workshop.length)}
      {@render line('Map', mapsLine, data.maps.length)}
    </section>

    <section>
      <h2>{t('export.maps')}</h2>
      <p class="text">{t('export.maps_moved')}</p>
      <button class="btn" onclick={() => (app.tab = 'maps')}>{t('export.maps_open')}</button>
      {#if data.lost_maps.length}
        <p class="note severity-warning">{t('export.lost_maps', { m: data.lost_maps.join(', ') })}</p>
      {/if}
    </section>

    <section>
      <div class="head-section">
        <h2>{t('export.list')}</h2>
        <div class="actions">
          <label class="checkbox"><input type="checkbox" bind:checked={onlyEnabled} />{t('export.only_enabled')}</label>
          <button class="btn" onclick={() => copy(table(), t('export.list'))}><Copy size={14} strokeWidth={1.75} />{t('export.copy_table')}</button>
          <button class="btn" onclick={() => copy(rows.map((m) => m.id).join('\n'), 'Mod IDs')}>{t('export.copy_ids')}</button>
          <button class="btn" onclick={() => copy(rows.map((m) => m.workshop_id).filter((x) => x !== null).join('\n'), 'Workshop IDs')}>{t('export.copy_ws')}</button>
        </div>
      </div>
      <div class="table">
        <div class="row head"><span>{t('export.slot')}</span><span>{t('table.name')}</span><span>{t('export.mod_id')}</span><span>{t('export.workshop_id')}</span></div>
        {#each rows as m (m.slot)}
          <div class="row" class:inactive={!m.enabled}>
            <span class="num">{m.slot}</span>
            <span class="name">{m.name}</span>
            <span class="mono">{m.id}</span>
            <span class="mono">{m.workshop_id ?? (m.found ? t('export.local') : t('table.not_found'))}</span>
          </div>
        {/each}
      </div>
    </section>

    <section>
      <div class="head-section">
        <h2>default.txt</h2>
        <button class="btn" onclick={() => copy(data!.default_txt, 'default.txt')}><Copy size={14} strokeWidth={1.75} />{t('export.copy')}</button>
      </div>
      <p class="text">{t('export.default_text')}</p>
      <pre class="mono code height">{data.default_txt}</pre>
    </section>
  {/if}
</div>

<style>
  .screen {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px;
    background: var(--ground);
  }
  section {
    max-width: 980px;
    margin-bottom: 26px;
  }
  h2 {
    font-size: var(--t-l);
    font-weight: 600;
  }
  .head-section {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .text {
    margin: 4px 0 10px;
    color: var(--ink-2);
    font-size: var(--t-s);
    max-width: 75ch;
  }
  .note {
    margin: 2px 0 10px 22px;
    color: var(--ink-2);
    font-size: var(--t-xs);
    max-width: 75ch;
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--t-s);
  }
  .checkbox input {
    accent-color: var(--accent);
  }
  .row-icon {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 10px;
    font-size: var(--t-s);
  }
  .line {
    margin-top: 10px;
  }
  .head-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }
  .tag {
    font-weight: 600;
    font-size: var(--t-s);
  }
  .dim {
    color: var(--ink-3);
    font-weight: 400;
  }
  .code {
    margin: 0;
    padding: 8px 10px;
    max-height: 120px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
    font-size: var(--t-xs);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .code.height {
    max-height: 320px;
    word-break: normal;
  }
  .table {
    margin-top: 10px;
    max-height: 420px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
  }
  .table .row {
    display: grid;
    grid-template-columns: 52px minmax(0, 2fr) minmax(0, 1.4fr) 130px;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    font-size: var(--t-s);
  }
  .table .row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .table .head {
    position: sticky;
    top: 0;
    background: var(--surface-2);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .table .inactive {
    color: var(--ink-3);
  }
  .mono {
    font-family: var(--mono);
  }
  .row-share {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .path {
    color: var(--ink-2);
    font-size: var(--t-xs);
    overflow-wrap: anywhere;
  }
</style>
