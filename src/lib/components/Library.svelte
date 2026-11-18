<script lang="ts">
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Confirm from './Confirm.svelte';
  import Search from '@lucide/svelte/icons/search';
  import Plus from '@lucide/svelte/icons/plus';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Download from '@lucide/svelte/icons/download';
  import PackageX from '@lucide/svelte/icons/package-x';
  import X from '@lucide/svelte/icons/x';
  import { api, inTauri, message } from '../api';
  import { untrack } from 'svelte';
  import { app } from '../state.svelte';
  import { t, kindText } from '../i18n.svelte';
  import { detailsCache } from '../steam.svelte';
  import type { LibraryMod, ModKind } from '../types';

  type SourceFilter = 'all' | 'workshop' | 'local';

  let mods = $state<LibraryMod[] | null>(null);
  let error = $state<string | null>(null);
  let search = $state('');
  let kind = $state<ModKind | 'all'>('all');
  let source = $state<SourceFilter>('all');
  let onlyAct = $state(false);
  let chosen = $state<Set<string>>(new Set());
  let anchor = $state<number | null>(null);
  let target = $state<string | null>(app.profileId);
  $effect(() => {
    if (!target || !app.profiles.some((p) => p.id === target)) target = app.profileId ?? app.profiles[0]?.id ?? null;
  });
  const targetSections = $derived(app.profiles.find((p) => p.id === target)?.sections ?? []);
  let targetSection = $state('');
  $effect(() => {
    const to = target;
    const sel = app.selection;
    const secs = targetSections;
    untrack(() => {
      const picked = to === app.profileId ? app.view?.rows.find((r) => r.id === sel) : undefined;
      targetSection = picked ? (picked.section ?? '') : (secs.at(-1) ?? '');
    });
  });

  $effect(() => {
    void app.versionMods;
    api
      .library()
      .then((m) => {
        mods = m.sort((a, b) => a.name.localeCompare(b.name));
        error = null;
        const n = !inTauri ? Number(new URLSearchParams(location.search).get('pick')) : 0;
        if (n) chosen = new Set(mods.filter((x) => x.id).slice(1, 1 + n).map((x) => x.key));
      })
      .catch((e) => (error = message(e)));
  });

  let waiting = $state<number[]>([]);
  let asked = $state(false);
  let downloadError = $state<string | null>(null);
  $effect(() => {
    void app.versionMods;
    api
      .workshopWaiting()
      .then((w) => (waiting = w))
      .catch(() => (waiting = []));
  });
  $effect(() => {
    if (waiting.length === 0) asked = false;
  });
  async function downloadNow() {
    downloadError = null;
    try {
      const refused = await api.workshopDownload(waiting);
      if (refused.length) downloadError = t('lib.waiting_refused', { ids: refused.join(', ') });
      else asked = true;
    } catch (e) {
      downloadError = message(e);
    }
  }

  const inProfile = $derived(new Set(app.view?.rows.filter((f) => f.enabled).map((f) => f.key) ?? []));
  const pendingSteam = $derived((mods ?? []).filter((m) => m.pending).length);

  const list = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return (mods ?? []).filter(
      (m) =>
        (kind === 'all' || m.kind === kind) &&
        (source === 'all' || m.source === source) &&
        (!onlyAct || m.pending) &&
        (!q || m.name.toLowerCase().includes(q) || (m.id ?? '').toLowerCase().includes(q) || (m.author ?? '').toLowerCase().includes(q)),
    );
  });

  function mark(e: MouseEvent, i: number) {
    const m = list[i];
    const s = new Set(chosen);
    const put = !s.has(m.key);
    const segment = e.shiftKey && anchor !== null ? list.slice(Math.min(anchor, i), Math.max(anchor, i) + 1) : [m];
    for (const x of segment.filter((x) => x.id)) {
      if (put) s.add(x.key);
      else s.delete(x.key);
    }
    anchor = i;
    chosen = s;
  }

  const eligible = $derived(list.filter((m) => m.id));
  const allVisible = $derived(eligible.length > 0 && eligible.every((m) => chosen.has(m.key)));
  function toggleVisible() {
    const s = new Set(chosen);
    for (const m of eligible) {
      if (allVisible) s.delete(m.key);
      else s.add(m.key);
    }
    chosen = s;
  }

  async function add(enabled: boolean) {
    const profile = target;
    if (!profile) return;
    const keys = [...chosen];
    const name = app.profiles.find((p) => p.id === profile)?.name ?? '';
    app.taken = true;
    try {
      await api.add(profile, keys, enabled, targetSections.length ? targetSection : null);
      app.notify(t(enabled ? 'lib.done_on' : 'lib.done_off', { n: keys.length, p: name }));
      chosen = new Set();
      if (profile === app.profileId) await app.loadView();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      app.taken = false;
    }
  }

  async function uninstall(m: LibraryMod) {
    try {
      if (m.workshop_id) await api.workshopUnsubscribe(m.workshop_id);
      else await api.modTrash(m.key);
      app.notify(m.workshop_id ? t('ws.unsubscribed_ok') : t('detail.trash_ok'));
      mods = (mods ?? []).filter((x) => x.key !== m.key);
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  const stop = (e: Event) => {
    e.preventDefault();
    e.stopPropagation();
  };
</script>

<section class="screen">
  <header class="head">
    <div>
      <h1>{t('lib.title')}</h1>
      <p class="text">{t('lib.text')}</p>
    </div>
  </header>

  {#if waiting.length > 0}
    <div class="news waiting" role="status">
      <p>
        <Download size={14} strokeWidth={1.75} />
        <span class="what">{asked ? t('lib.waiting_asked', { n: waiting.length }) : t('lib.waiting', { n: waiting.length })}</span>
        <button class="btn" disabled={asked || app.taken} onclick={downloadNow}>{t('lib.download_now')}</button>
      </p>
      <ul>
        {#each waiting as id (id)}
          {@const title = detailsCache.get(id)?.card.title}
          <li>
            <button class="link" onclick={() => app.openInWorkshop(id, false)}>{title ?? t('lib.waiting_item')}</button>
            <span class="mono id">{id}</span>
          </li>
        {/each}
      </ul>
      {#if downloadError}<p class="severity-blocks with-icon"><CircleX size={14} strokeWidth={2} class="err-icon" />{downloadError}</p>{/if}
    </div>
  {/if}

  {#if pendingSteam > 0}
    <div class="news" role="status">
      <p><Download size={14} strokeWidth={1.75} />{t('lib.ws_pending', { n: pendingSteam })}</p>
    </div>
  {/if}

  <div class="bar">
    <label class="search">
      <Search size={14} strokeWidth={1.75} />
      <input class="field" type="search" bind:value={search} placeholder={t('lib.search')} />
    </label>
    <select class="field" bind:value={kind} aria-label={t('table.kind')}>
      <option value="all">{t('lib.all')}</option>
      {#each ['java', 'lua', 'map', 'vehicle'] as tp (tp)}
        <option value={tp}>{kindText(tp as ModKind)}</option>
      {/each}
    </select>
    <select class="field" bind:value={source} aria-label={t('table.source')}>
      <option value="all">{t('lib.sources')}</option>
      <option value="workshop">{t('lib.f_workshop')}</option>
      <option value="local">{t('lib.f_local')}</option>
    </select>
    <label class="checkbox">
      <input type="checkbox" bind:checked={onlyAct} />{t('lib.with_update')}
    </label>
    {#if mods}<span class="count num">{list.length} / {mods.length}</span>{/if}
  </div>

  {#if app.profiles.length > 0}
    <div class="batch" class:enabled={chosen.size > 0} role="toolbar" aria-label={t('lib.batch')}>
      {#if chosen.size > 0}
        <strong class="num">{t('batch.n', { n: chosen.size })}</strong>
        <button class="btn subtle icon" aria-label={t('batch.clear')} title={t('batch.clear')} onclick={() => (chosen = new Set())}><X size={14} strokeWidth={2} /></button>
      {:else}
        <span class="hint">{t('lib.hint')}</span>
      {/if}
      <span class="gap"></span>
      <label class="target">
        {t('lib.target')}
        <select class="field" bind:value={target}>
          {#each app.profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </label>
      {#if targetSections.length > 0}
        <label class="target">
          <span class="hidden">{t('sections.move_to')}</span>
          <select class="field" bind:value={targetSection} aria-label={t('sections.move_to')}>
            <option value="">{t('sections.none')}</option>
            {#each targetSections as s (s)}<option value={s}>{s}</option>{/each}
          </select>
        </label>
      {/if}
      <button class="btn" disabled={chosen.size === 0 || !target || app.taken} onclick={() => add(false)}>{t('lib.without_enable')}</button>
      <button class="btn primary" disabled={chosen.size === 0 || !target || app.taken} onclick={() => add(true)}>
        <Plus size={15} strokeWidth={2} />{chosen.size ? t('lib.add_enable', { n: chosen.size }) : t('lib.add_nothing')}
      </button>
    </div>
  {/if}

  <div class="table">
    {#if error}
      <p class="msg severity-blocks with-icon"><CircleX size={14} strokeWidth={2} class="err-icon" />{error}</p>
    {:else if !mods}
      {#each Array(12) as _, i (i)}<div class="row skeleton"><span></span></div>{/each}
    {:else if mods.length === 0}
      <p class="msg">{t('lib.empty')}</p>
    {:else}
      <div class="header">
        <span class="all"
          ><input type="checkbox" checked={allVisible} indeterminate={!allVisible && eligible.some((m) => chosen.has(m.key))} aria-label={t('lib.all_visible')} title={t('lib.all_visible')} onchange={toggleVisible} /></span
        ><span>{t('table.name')}</span><span>{t('detail.author')}</span><span>{t('table.source')}</span><span>{t('table.kind')}</span><span>{t('table.version')}</span><span></span>
      </div>
      {#each list as m, i (m.key)}
        {@const inside = inProfile.has(m.key)}
        <label class="row" class:inside class:chosen={chosen.has(m.key)}>
          <input type="checkbox" disabled={!m.id} checked={chosen.has(m.key)} onclick={(e) => mark(e, i)} />
          <span class="name">
            <span>{m.name}</span>
            {#if m.id && m.id !== m.name}<span class="id mono">{m.id}</span>{/if}
            {#if inside}<span class="inside-txt">{t('lib.in_profile')}</span>{/if}
            {#if m.error}<span class="severity-warning err"><TriangleAlert size={12} strokeWidth={2} class="inline-icon" />{m.error}</span>{/if}
          </span>
          <span class="author">{m.author ?? ''}</span>
          <span class="origin">
            {#if m.pending}
              <span class="new"><Download size={13} strokeWidth={2} />{t('lib.pending')}</span>
            {:else}
              <span class:usual={m.source === 'workshop'}>{t(`source.${m.source}`)}</span>
            {/if}
          </span>
          <span class="tp" class:java={m.kind === 'java'}>{kindText(m.kind)}</span>
          <span class="mono show">{m.version_mod ?? ''}</span>
          <span class="end">
            {#if m.workshop_id}
              <button class="btn subtle icon" title={t('lib.steam_remove')} aria-label={t('lib.steam_remove')} onclick={(e) => { stop(e); api.openWorkshop(m.workshop_id!); }}>
                <ExternalLink size={14} strokeWidth={1.75} />
              </button>
            {/if}
            <Confirm
              text={m.workshop_id ? t('lib.unsubscribe_confirm', { n: m.name }) : t('detail.trash_confirm', { n: m.name })}
              yes={m.workshop_id ? t('detail.unsubscribe') : t('detail.trash')}
              onsi={() => uninstall(m)}
            >
              {#snippet button(request)}
                <button
                  class="btn subtle icon danger"
                  title={m.workshop_id ? t('detail.unsubscribe') : t('detail.trash')}
                  aria-label={m.workshop_id ? t('detail.unsubscribe') : t('detail.trash')}
                  onclick={request}
                >
                  <PackageX size={14} strokeWidth={1.75} />
                </button>
              {/snippet}
            </Confirm>
          </span>
        </label>
      {:else}
        <p class="msg">{t('lib.without_results')}</p>
      {/each}
    {/if}
  </div>
</section>

<style>
  .screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    padding: 14px 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  h1 {
    font-size: var(--t-xl);
  }
  .text {
    margin: 3px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .news {
    padding: 6px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--t-s);
  }
  .news p {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 28px;
    margin: 0;
  }
  .news :global(svg) {
    flex: none;
    color: var(--ink-2);
  }
  .waiting .what {
    flex: 1;
  }
  .waiting ul {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    margin: 0 0 6px;
    padding: 0 0 0 21px;
    list-style: none;
  }
  .waiting li {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .waiting .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .waiting .id {
    font-size: var(--t-xs);
    color: var(--ink-3);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
  }
  .search :global(svg) {
    position: absolute;
    left: 8px;
    color: var(--ink-3);
  }
  .search .field {
    width: 260px;
    padding-left: 28px;
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--t-s);
    white-space: nowrap;
  }
  .checkbox input {
    accent-color: var(--accent);
  }
  .count {
    margin-left: auto;
    color: var(--ink-3);
    font-size: var(--t-s);
  }
  .table {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
    container-type: inline-size;
  }
  .header,
  .row {
    display: grid;
    grid-template-columns: 36px minmax(220px, 1fr) 160px 150px 80px 84px 44px;
    align-items: center;
  }
  .header {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 28px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .row {
    height: 32px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    cursor: pointer;
  }
  .row:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  .row.chosen {
    background: var(--accent-soft);
  }
  .all {
    display: grid;
    place-items: center;
  }
  .all input {
    accent-color: var(--accent);
  }
  .batch {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 6px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--t-s);
  }
  .batch.enabled {
    background: var(--accent-soft);
  }
  .batch strong {
    font-weight: 600;
  }
  .hint {
    color: var(--ink-2);
  }
  .gap {
    flex: 1;
  }
  .target {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-2);
    white-space: nowrap;
  }
  .target .field {
    max-width: 220px;
  }
  .row input {
    justify-self: center;
    accent-color: var(--accent);
  }
  .name {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    padding-right: 12px;
    overflow: hidden;
    white-space: nowrap;
  }
  .name > span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .id {
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .inside-txt {
    flex: none;
    padding: 0 5px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
    line-height: 16px;
  }
  .usual {
    color: var(--ink-3);
  }
  .err {
    font-size: var(--t-xs);
  }
  .author,
  .show,
  .origin {
    color: var(--ink-2);
    font-size: var(--t-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .new {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ink);
    font-weight: 600;
  }
  .tp {
    font-size: var(--t-xs);
    color: var(--ink-2);
  }
  .tp.java {
    color: var(--accent);
    font-weight: 600;
  }
  .end {
    display: flex;
    justify-content: flex-end;
    padding-right: 10px;
  }
  .end .btn {
    height: 24px;
  }
  .icon {
    padding: 0 6px;
  }
  .msg {
    margin: 0;
    padding: 24px 16px;
    color: var(--ink-2);
  }
  .skeleton span {
    grid-column: 2 / 6;
    height: 10px;
    border-radius: var(--radio);
    background: var(--surface-2);
  }
  @container (max-width: 900px) {
    .header,
    .row {
      grid-template-columns: 36px minmax(200px, 1fr) 150px 80px 84px 44px;
    }
    .header > :nth-child(3),
    .row > .author {
      display: none;
    }
  }
  .end .danger {
    color: var(--ink-3);
  }
  .end .danger:hover,
  .end .danger:focus-visible {
    color: var(--danger);
  }
</style>
