<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import { onMount } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import Plus from '@lucide/svelte/icons/plus';
  import Store from '@lucide/svelte/icons/store';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t, kindText } from '../i18n.svelte';
  import type { LibraryMod } from '../types';

  let { close }: { close: () => void } = $props();

  const v = $derived(app.view!);
  const noBar = (id: string) => id.trim().replace(/^\\+/, '');
  const alreadyInside = $derived(new Set(v.rows.map((f) => noBar(f.id))));

  let mods = $state<LibraryMod[] | null>(null);
  let error = $state<string | null>(null);
  let search = $state('');
  let chosen = $state<Set<string>>(new Set());
  let set_enabled = $state(true);
  let anchor = $state<number | null>(null);
  let field: HTMLInputElement | undefined = $state();

  onMount(() => {
    field?.focus();
    api
      .library()
      .then((m) => (mods = m.filter((x) => x.id).sort((a, b) => a.name.localeCompare(b.name))))
      .catch((e) => (error = message(e)));
  });

  const outside = $derived((mods ?? []).filter((m) => !alreadyInside.has(noBar(m.id!))));
  const list = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q) return outside;
    return outside.filter((m) => m.name.toLowerCase().includes(q) || m.id!.toLowerCase().includes(q) || (m.author ?? '').toLowerCase().includes(q));
  });

  function mark(e: MouseEvent, i: number) {
    const m = list[i];
    const s = new Set(chosen);
    const put = !s.has(m.key);
    if (e.shiftKey && anchor !== null) {
      const [a, b] = anchor < i ? [anchor, i] : [i, anchor];
      for (const x of list.slice(a, b + 1)) {
        if (put) s.add(x.key);
        else s.delete(x.key);
      }
    } else if (put) s.add(m.key);
    else s.delete(m.key);
    anchor = i;
    chosen = s;
  }

  function allVisible() {
    const s = new Set(chosen);
    const all = list.every((m) => s.has(m.key));
    for (const m of list) {
      if (all) s.delete(m.key);
      else s.add(m.key);
    }
    chosen = s;
  }

  async function add() {
    const keys = [...chosen];
    const id = v.id;
    await app.run(() => api.add(id, keys, set_enabled), t(set_enabled ? 'add.done_on' : 'add.done_off', { n: keys.length }));
    close();
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
    }
  }
</script>

<div class="selector" role="dialog" aria-label={t('add.title')} tabindex="-1" onkeydown={key}>
  <div class="head">
    <strong>{t('add.title')}</strong>
    <span class="dim">{t('add.score')}</span>
  </div>
  <label class="search">
    <Search size={14} strokeWidth={1.75} />
    <input class="field" type="search" bind:this={field} bind:value={search} placeholder={t('lib.search')} />
  </label>
  <div class="list">
    {#if error}
      <p class="msg severity-blocks with-icon"><CircleX size={14} strokeWidth={2} class="err-icon" />{error}</p>
    {:else if !mods}
      {#each Array(6) as _, i (i)}<div class="row skeleton"><span></span></div>{/each}
    {:else if outside.length === 0}
      <p class="msg">{t('add.all_inside')}</p>
    {:else if list.length === 0}
      <p class="msg">{t('lib.without_results')}</p>
    {:else}
      {#each list as m, i (m.key)}
        <label class="row" class:chosen={chosen.has(m.key)}>
          <input type="checkbox" checked={chosen.has(m.key)} onclick={(e) => mark(e, i)} />
          <span class="name">{m.name}</span>
          {#if m.id !== m.name}<span class="id mono">{m.id}</span>{/if}
          <span class="tp" class:java={m.kind === 'java'}>{kindText(m.kind)}</span>
        </label>
      {/each}
    {/if}
  </div>
  <div class="footer">
    {#if list.length > 0}
      <button class="btn subtle" onclick={allVisible}>{list.every((m) => chosen.has(m.key)) ? t('add.none') : t('add.all', { n: list.length })}</button>
    {/if}
    <label class="checkbox"><input type="checkbox" bind:checked={set_enabled} />{t('add.set_enabled')}</label>
    <span class="gap"></span>
    <button class="btn subtle" onclick={() => app.openInWorkshop(null, false)}><Store size={14} strokeWidth={1.75} />{t('add.workshop')}</button>
    <button class="btn primary" disabled={chosen.size === 0 || app.taken} onclick={add}>
      <Plus size={14} strokeWidth={2} />{chosen.size ? t('add.button', { n: chosen.size }) : t('add.button_nothing')}
    </button>
  </div>
</div>

<style>
  .selector {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    width: min(560px, calc(100vw - 280px));
    max-height: min(540px, 70vh);
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    box-shadow: var(--shadow-menu);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 10px 12px 6px;
  }
  .head strong {
    font-weight: 600;
  }
  .dim {
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    padding: 0 12px 8px;
  }
  .search :global(svg) {
    position: absolute;
    left: 20px;
    top: 9px;
    color: var(--ink-3);
    pointer-events: none;
  }
  .search .field {
    width: 100%;
    padding-left: 28px;
  }
  .list {
    flex: 1;
    min-height: 120px;
    overflow: auto;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 12px;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  .row:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  .row.chosen {
    background: var(--accent-soft);
  }
  .row input,
  .checkbox input {
    accent-color: var(--accent);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .id {
    flex: none;
    max-width: 30%;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .tp {
    margin-left: auto;
    flex: none;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .tp.java {
    color: var(--accent);
    font-weight: 600;
  }
  .msg {
    margin: 0;
    padding: 18px 12px;
    color: var(--ink-2);
  }
  .skeleton span {
    width: 60%;
    height: 10px;
    border-radius: var(--radio);
    background: var(--surface-2);
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--t-s);
    white-space: nowrap;
  }
  .gap {
    flex: 1;
  }
</style>
