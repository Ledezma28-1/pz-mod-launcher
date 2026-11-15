<script lang="ts">
  import { untrack } from 'svelte';
  import Play from '@lucide/svelte/icons/play';
  import Plus from '@lucide/svelte/icons/plus';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FileUp from '@lucide/svelte/icons/file-up';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import Layers from '@lucide/svelte/icons/layers';
  import Users from '@lucide/svelte/icons/users';
  import StopButton from './StopButton.svelte';
  import { api } from '../api';
  import { app, readText } from '../state.svelte';
  import { i18n, t } from '../i18n.svelte';
  import { detailsCache, image, steam } from '../steam.svelte';
  import { still } from '../still';
  import { ago } from '../time';
  import { channelOf, updateToNotify } from '../zb-flow';
  import type { FeaturedItem, ProfileView } from '../types';

  let nMods = $state<number | null>(null);
  $effect(() => {
    void app.versionMods;
    api.library().then((l) => (nMods = l.length)).catch(() => {});
  });

  const firstTime = $derived(app.profiles.length === 0);
  const recent = $derived(
    [...app.profiles].sort((a, b) => (b.last_use ?? 0) - (a.last_use ?? 0) || a.name.localeCompare(b.name)),
  );
  const last = $derived(recent[0] ?? null);
  let picked = $state<string | null>(null);
  const sel = $derived(recent.find((p) => p.id === picked) ?? last);
  const busy = $derived(app.starting || !!(app.session && !app.session.finished));
  let tabsEl = $state<HTMLElement>();
  let overflows = $state(false);
  $effect(() => {
    const el = tabsEl;
    if (!el) return;
    const check = () => (overflows = el.scrollWidth > el.clientWidth + 1);
    const ro = new ResizeObserver(check);
    ro.observe(el);
    for (const c of el.children) ro.observe(c);
    check();
    return () => ro.disconnect();
  });

  let counts = $state<Record<string, ProfileView['counts']>>({});
  $effect(() => {
    const ids = recent.slice(0, 8).map((p) => p.id);
    void app.versionMods;
    let cancelled = false;
    (async () => {
      for (const id of ids) {
        if (cancelled) return;
        if (app.view?.id === id) {
          counts[id] = app.view.counts;
          continue;
        }
        try {
          counts[id] = await api.counts(id);
        } catch {
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  let creating = $state(false);
  let name = $state('');
  let do_import = $state(true);
  async function create(e: SubmitEvent) {
    e.preventDefault();
    const n = name.trim() || t(do_import ? 'empty.profiles.imported_name' : 'empty.profiles.name_empty');
    await app.createProfile(n, do_import);
    name = '';
    creating = false;
  }
  function openCreate() {
    creating = true;
    do_import = firstTime || (app.settings?.new_profile.import_game_mods ?? false);
    queueMicrotask(() => document.getElementById('home-name')?.focus());
  }

  let file: HTMLInputElement | undefined = $state();
  async function importFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    await app.importProfile(await f.text());
  }

  async function showConflicts(id: string) {
    await app.chooseProfile(id);
    app.tab = 'conflicts';
  }

  async function playWith(id: string) {
    await app.chooseProfile(id, false);
    await app.play();
  }

  $effect(() => {
    untrack(() => void steam.revalidateFeatured());
  });
  const featured = $derived(steam.featured);
  const updating = $derived(steam.updating);
  const errorDest = $derived(steam.errorDest);
  const update = () => steam.updateFeatured();
  const lead = $derived(featured?.mods.slice(0, 5) ?? []);
  const more = $derived(featured?.mods.slice(5) ?? []);
  const excerpt = (id: number) => detailsCache.get(id)?.card.description?.slice(0, 320) ?? '';

  const compact = $derived(new Intl.NumberFormat(i18n.language, { notation: 'compact', maximumFractionDigits: 1 }));
  const does = ago;
  const initial = (s: string) =>
    s
      .replace(/[[\](){}|_\-–—:]+/g, ' ')
      .trim()
      .split(/\s+/)
      .slice(0, 2)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .join('');

  function scroll(id: string, dir: number) {
    const el = document.getElementById(id);
    el?.scrollBy({ left: dir * el.clientWidth * 0.8, behavior: 'smooth' });
  }
</script>

{#snippet cover(d: FeaturedItem, rank: number | null, big = false)}
  <button class="cover" class:big onclick={() => app.openInWorkshop(d.id, d.collection)} title={rank ? t('home.rank', { n: rank }) : undefined}>
    <span class="img">
      {#if d.preview_view}
        <img class="pic" src={image(d.preview_view, 0, big ? 1024 : 512)} alt="" loading="lazy" referrerpolicy="no-referrer" use:still />
      {:else}
        <span class="no-img" aria-hidden="true">{initial(d.title)}</span>
      {/if}
      {#if d.collection}<span class="timestamp"><Layers size={12} strokeWidth={2} />{t('home.n_mods', { n: d.children })}</span>{/if}
    </span>
    <span class="title">{#if rank}<span class="rank num">{rank}</span>{/if}<span class="name">{d.title}</span></span>
    <span class="meta">
      <span class="author">{d.author ?? t('common.author_unknown')}</span>
      <span class="subs num" title={t('ws.n_subscribers', { n: d.subscribers })}><Users size={12} strokeWidth={1.75} />{compact.format(d.subscribers)}</span>
    </span>
    {#if big && excerpt(d.id)}<span class="excerpt">{excerpt(d.id)}</span>{/if}
  </button>
{/snippet}

{#snippet profileState(id: string, link = false)}
  {@const r = counts[id]}
  {#if !r}
    <span class="schema-line" aria-hidden="true"></span>
  {:else if r.blocks}
    {#if link}
      <button class="link severity-blocks est" onclick={() => showConflicts(id)}><CircleX size={13} strokeWidth={2} />{t('home.not_will_load', { n: r.blocks })}</button>
    {:else}
      <span class="severity-blocks est"><CircleX size={13} strokeWidth={2} />{t('home.n_blockers', { n: r.blocks })}</span>
    {/if}
  {:else if r.warning}
    <span class="severity-warning est"><TriangleAlert size={13} strokeWidth={2} />{t('home.n_warnings', { n: r.warning })}</span>
  {:else}
    <span class="ok-text est"><CircleCheck size={13} strokeWidth={2} />{t('home.without_conflicts')}</span>
  {/if}
{/snippet}

{#snippet shelfCtrl(id: string)}
  <button class="btn subtle icon" aria-label={t('home.older')} onclick={() => scroll(id, -1)}><ChevronLeft size={16} strokeWidth={1.75} /></button>
  <button class="btn subtle icon" aria-label={t('home.next_many')} onclick={() => scroll(id, 1)}><ChevronRight size={16} strokeWidth={1.75} /></button>
{/snippet}

<div class="start">
  <header class="desk">
    <div class="desk-head">
      <h1>{firstTime ? t('home.new_hello') : t('home.your_profiles')}</h1>
      {#if app.environment}
        <dl class="system" aria-label={t('home.system')}>
          <dt>{t('home.game')}</dt>
          <dd class="mono">{app.environment.game.version}</dd>
          <dt>ZombieBuddy</dt>
          <dd>
            {#if app.environment.zb.in_game}
              <span class="ok-text est"><CircleCheck size={13} strokeWidth={2} /><span class="mono">{app.environment.zb.in_game}</span></span>
              {@const zbNew = updateToNotify(app.environment.zb.in_game, channelOf(app.environment.zb.in_game, app.environment.zb.workshop, readText('zb-channel')), app.environment.zb.workshop, null)}
              {#if zbNew}<button class="link" onclick={() => (app.screen = 'zb')}>{t('home.zb_available', { v: zbNew })}</button>{/if}
            {:else}
              <button class="link" onclick={() => (app.screen = 'zb')}><Info size={13} strokeWidth={2} />{t('home.zb_no')}</button>
            {/if}
          </dd>
          {#if nMods !== null}
            <dt>{t('home.mods')}</dt>
            <dd class="num">{t('home.n_installed', { n: nMods })}</dd>
          {/if}
        </dl>
      {/if}
    </div>

    {#if firstTime && !creating}
      <div class="welcome">
        <p class="explains">{t('empty.profiles.text')}</p>
        <div class="actions">
          <button class="btn primary huge" onclick={openCreate}><Plus size={18} strokeWidth={2} />{t('home.create_first')}</button>
          <button class="btn big" onclick={() => file?.click()}><FileUp size={15} strokeWidth={1.75} />{t('share.import')}</button>
        </div>
      </div>
    {:else if !firstTime}
      <div class="strip">
      <div class="tabs" class:overflows role="list" bind:this={tabsEl}>
        {#each recent as p (p.id)}
          {@const on = sel?.id === p.id}
          <div class="tab" class:on role="listitem">
            <div class="tab-row">
            <button class="tab-pick" aria-pressed={on} onclick={() => (picked = p.id)} ondblclick={() => app.chooseProfile(p.id)}>
              <span class="p-name" title={p.name}>{p.name}</span>
              <span class="p-line">
                <span class="num">{t('home.enabled_of', { a: p.active, n: p.total })}</span>
                {#if on}<span class="p-use">{t('home.used', { c: does(p.last_use) })}</span>{:else}{@render profileState(p.id)}{/if}
              </span>
            </button>
            {#if on}
              <div class="tab-actions">
                {#if busy}
                  <span class="running" role="status">{app.starting ? t('home.starting') : t('home.playing', { p: app.profiles.find((x) => x.id === app.session?.profile)?.name ?? '' })}</span>
                  <StopButton />
                  <button
                    class="btn"
                    onclick={() => {
                      if (app.session) {
                        app.chooseProfile(app.session.profile);
                        app.tab = 'console';
                      }
                    }}>{t('home.show_console')}</button
                  >
                {:else}
                  <button class="btn primary big" onclick={() => playWith(p.id)} aria-label={t('home.play_with', { p: p.name })}>
                    <Play size={15} strokeWidth={2} fill="currentColor" /><span>{t('home.play')}</span>
                  </button>
                  <button class="btn big icon-only" onclick={() => app.chooseProfile(p.id)} aria-label={t('home.open_profile')} title={t('home.open_profile')}><FolderOpen size={15} strokeWidth={1.75} /></button>
                {/if}
              </div>
            {/if}
            </div>
            {#if on}<div class="tab-state">{@render profileState(p.id, true)}</div>{/if}
          </div>
        {/each}
        {#each app.unreadable as [folder, path, reason] (folder)}
          <div class="tab unreadable" role="listitem" title={reason}>
            <span class="severity-blocks est"><CircleX size={13} strokeWidth={2} />{t('home.unreadable', { c: folder })}</span>
            <span class="p-reason mono">{reason}</span>
            <button class="btn subtle" onclick={() => api.openFolder(path).catch((e) => app.notify(String(e), 'error'))}><FolderOpen size={14} strokeWidth={1.75} />{t('home.open_folder')}</button>
          </div>
        {/each}
      </div>
        {#if !creating}
          <div class="adds">
            <button class="btn subtle" onclick={openCreate}><Plus size={14} strokeWidth={2} />{t('nav.new')}</button>
            <button class="btn subtle" onclick={() => file?.click()}><FileUp size={14} strokeWidth={1.75} />{t('share.import')}</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if creating}
      <form class="create" onsubmit={create}>
        <input
          id="home-name"
          class="field"
          bind:value={name}
          placeholder={t(do_import ? 'empty.profiles.imported_name' : 'empty.profiles.name_empty')}
          aria-label={t('home.name_profile')}
          onkeydown={(e) => e.key === 'Escape' && (creating = false)}
        />
        <label class="checkbox"><input type="checkbox" bind:checked={do_import} />{t('home.import')}</label>
        <button class="btn primary big" type="submit"><Plus size={15} strokeWidth={2} />{t('home.create')}</button>
        <button class="btn big" type="button" onclick={() => (creating = false)}>{t('common.cancel')}</button>
      </form>
    {/if}
    <input bind:this={file} class="hidden" type="file" accept=".pzprofile,.json" tabindex="-1" aria-hidden="true" onchange={importFile} />
  </header>

  <div class="body">
    {#if app.bisect}
      {@const b = app.bisect}
      <p class="bisect-notice">
        {t('bisect.open_notice', { p: app.profiles.find((p) => p.id === b.origin)?.name ?? b.origin })}
        <button
          class="btn"
          onclick={async () => {
            await app.chooseProfile(b.profile);
            app.tab = 'console';
          }}>{t('bisect.continue')}</button
        >
      </p>
    {/if}

    <section class="block" aria-labelledby="featured-t">
      <div class="head-block">
        <h2 id="featured-t">{t('home.featured_week')}</h2>
        <div class="ctrl">
          {#if featured}
            <span class="read">
              {#if errorDest}<span class="severity-blocks" title={errorDest}><CircleX size={13} strokeWidth={2} />{t('home.not_updated')}</span>{' · '}{/if}
              {updating ? t('home.connecting') : t('home.parsed', { c: does(featured.parsed) })}
            </span>
            <button class="btn subtle icon" title={t('home.update_note')} aria-label={t('home.update')} onclick={update} disabled={updating}><RefreshCw size={14} strokeWidth={1.75} class={updating ? 'rotates' : ''} /></button>
            <span class="div" aria-hidden="true"></span>
          {/if}
          <button class="btn subtle" onclick={() => app.openInWorkshop(null, false)}>{t('home.show_workshop')}<ArrowRight size={14} strokeWidth={1.75} /></button>
        </div>
      </div>

      {#if !featured && (steam.reading || updating)}
        <div class="mosaic" aria-busy="true" aria-label={t('home.connecting')}>
          {#each [0, 1, 2, 3, 4] as i (i)}
            <span class="cover schema" class:big={i === 0} aria-hidden="true"><span class="img"></span><span class="title"><span></span></span><span class="meta"><span></span></span></span>
          {/each}
        </div>
      {:else if errorDest && !featured}
        <div class="warning-block">
          <CircleX size={18} strokeWidth={1.75} class="red" />
          <div>
            <p class="a-title">{t('home.no_steam')}</p>
            <p class="a-detail">{errorDest}</p>
          </div>
          <button class="btn" onclick={update}><RefreshCw size={14} strokeWidth={1.75} />{t('home.retry')}</button>
        </div>
      {:else if !featured}
        <div class="warning-block">
          <Layers size={22} strokeWidth={1.5} />
          <div>
            <p class="a-title">{t('home.invitation')}</p>
            <p class="a-detail">{t('home.invitation_note')}</p>
          </div>
          <button class="btn" onclick={update}>{t('home.show_featured')}</button>
        </div>
      {:else if lead.length}
        <div class="mosaic">
          {#each lead as d, i (d.id)}{@render cover(d, i + 1, i === 0)}{/each}
        </div>
      {:else}
        <p class="nothing">{t('home.shelf_empty')}</p>
      {/if}
    </section>

    {#if featured?.collections.length}
      <section class="block" aria-labelledby="collections-t">
        <div class="head-block">
          <h2 id="collections-t">{t('home.featured_collections')}</h2>
          <div class="ctrl">
            {@render shelfCtrl('featured-collections')}
            <span class="div" aria-hidden="true"></span>
            <button class="btn subtle" onclick={() => app.openInWorkshop(null, true)}>{t('home.show_collections')}<ArrowRight size={14} strokeWidth={1.75} /></button>
          </div>
        </div>
        <div class="hint" id="featured-collections">
          {#each featured.collections as d (d.id)}{@render cover(d, null)}{/each}
        </div>
      </section>
    {/if}
    {#if more.length}
      <section class="block" aria-labelledby="more-t">
        <div class="head-block">
          <h2 id="more-t">{t('home.more_mods')}</h2>
          <div class="ctrl">{@render shelfCtrl('more-mods')}</div>
        </div>
        <div class="hint" id="more-mods">
          {#each more as d, i (d.id)}{@render cover(d, i + 6)}{/each}
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  .start {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    --cover-width: clamp(176px, 13vw, 232px);
  }
  .bisect-notice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 0;
    padding: 10px 14px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
  }

  .desk {
    padding: 18px 24px 16px;
    border-bottom: 1px solid var(--line);
  }
  .desk-head {
    display: flex;
    align-items: baseline;
    gap: 16px;
    margin-bottom: 12px;
  }
  h1 {
    margin: 0;
    font-size: var(--t-xl);
    font-weight: 600;
    line-height: 1.25;
  }
  .system {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
    margin: 0 0 0 auto;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .system dt {
    color: var(--ink-3);
  }
  .system dd {
    margin: 0 10px 0 0;
  }
  .system dd:last-child {
    margin-right: 0;
  }
  .welcome .explains {
    margin: 0;
    max-width: 62ch;
    color: var(--ink-2);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }
  .btn.huge {
    height: 42px;
    padding: 0 22px;
    gap: 9px;
    font-size: var(--t-l);
    font-weight: 600;
  }

  .strip {
    display: flex;
    align-items: stretch;
    gap: 12px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: stretch;
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 2px;
    scroll-snap-type: x proximity;
    scrollbar-width: thin;
  }
  .tabs.overflows {
    padding-right: 32px;
    mask-image: linear-gradient(to right, #000 calc(100% - 32px), transparent);
  }
  .adds {
    flex: none;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 2px;
  }
  .tab {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 160px;
    max-width: 200px;
    scroll-snap-align: start;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
    transition:
      border-color var(--dur) var(--ease),
      background var(--dur) var(--ease);
  }
  .tab:hover {
    border-color: var(--line-strong);
  }
  .tab.on {
    flex: 0 0 auto;
    max-width: 460px;
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .tab-pick {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 9px 12px;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .tab-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .tab-pick {
    flex: 1;
  }
  .tab-actions {
    padding-right: 10px;
  }
  .btn.icon-only {
    padding: 0 9px;
  }
  .p-name {
    font-size: var(--t-l);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .p-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 10px;
    color: var(--ink-2);
    font-size: var(--t-s);
    white-space: nowrap;
  }
  .tab-state {
    padding: 0 12px 9px;
    font-size: var(--t-s);
  }
  .p-use {
    color: var(--ink-2);
  }
  .tab-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .running {
    font-weight: 600;
    white-space: nowrap;
  }
  .tab.unreadable {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 0 6px 0 12px;
    font-size: var(--t-s);
  }
  .p-reason {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .est {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .ok-text {
    color: var(--ok);
  }
  .schema-line {
    width: 70px;
    height: 8px;
    border-radius: var(--radio);
    background: var(--surface-3);
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 3px;
    text-decoration-color: color-mix(in srgb, currentColor 40%, transparent);
  }
  .link:hover {
    text-decoration-color: currentColor;
  }
  .create {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .create .field {
    width: 240px;
    height: 32px;
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .checkbox input {
    accent-color: var(--accent);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 26px;
    padding: 18px 24px 28px;
  }
  .head-block {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }
  h2 {
    margin: 0;
    font-size: var(--t-xl);
    font-weight: 600;
  }
  .ctrl {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-left: auto;
  }
  .read {
    margin-right: 4px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .read :global(svg) {
    vertical-align: -2px;
  }
  .div {
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--line);
  }
  .btn.icon {
    padding: 0 6px;
  }
  .ctrl :global(.rotates) {
    animation: rotate 900ms linear infinite;
  }
  @keyframes rotate {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ctrl :global(.rotates) {
      animation: none;
    }
  }

  .mosaic {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1fr) minmax(0, 1fr);
    gap: 18px 16px;
  }
  .mosaic .big {
    grid-row: span 2;
  }
  .mosaic .big .img {
    flex: 1 1 0;
    min-height: 0;
    aspect-ratio: auto;
  }
  .hint {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: var(--cover-width);
    gap: 16px;
    overflow-x: auto;
    padding-bottom: 6px;
    scroll-snap-type: x proximity;
    scrollbar-width: thin;
  }
  .cover {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
    scroll-snap-align: start;
  }
  .img {
    position: relative;
    display: block;
    aspect-ratio: 16 / 9;
    margin-bottom: 5px;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    overflow: hidden;
    background: var(--surface-3);
    transition: border-color var(--dur) var(--ease);
  }
  .cover:hover .img,
  .cover:focus-visible .img {
    border-color: var(--accent);
  }
  .pic {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .no-img {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--ink-3);
    font-size: 28px;
    font-weight: 600;
  }
  .big .no-img {
    font-size: 64px;
  }
  .timestamp {
    position: absolute;
    left: 6px;
    bottom: 6px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: var(--radio);
    background: var(--shell);
    color: var(--shell-ink);
    font-size: var(--t-xs);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    font-weight: 600;
  }
  .rank {
    flex: none;
    color: var(--ink-3);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .big .title {
    font-size: var(--t-xl);
  }
  .meta {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--line);
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .big .meta {
    font-size: var(--t-s);
  }
  .excerpt {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    max-width: 72ch;
    margin-top: 4px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .subs {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    white-space: nowrap;
  }
  .schema .img {
    border-color: transparent;
  }
  .schema .title span,
  .schema .meta span {
    display: block;
    height: 9px;
    width: 70%;
    margin-top: 4px;
    border-radius: var(--radio);
    background: var(--surface-3);
  }
  .schema .meta {
    border-top-color: transparent;
  }
  .schema .meta span {
    width: 45%;
  }
  .nothing {
    margin: 0;
    color: var(--ink-3);
  }
  .warning-block {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
    color: var(--ink-2);
  }
  .warning-block :global(.red) {
    color: var(--danger);
  }
  .warning-block .btn {
    margin-left: auto;
  }
  .a-title {
    margin: 0;
    color: var(--ink);
    font-weight: 600;
  }
  .a-detail {
    margin: 2px 0 0;
    font-size: var(--t-s);
  }
  .hidden {
    display: none;
  }

  @media (max-width: 1180px) {
    .desk-head {
      flex-wrap: wrap;
    }
    .system {
      margin-left: 0;
      flex-basis: 100%;
    }
    .mosaic {
      grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 1fr);
    }
  }
</style>
