<script lang="ts">
  import { arrows } from '../keys';
  import { onDestroy, onMount, untrack } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Check from '@lucide/svelte/icons/check';
  import Download from '@lucide/svelte/icons/download';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Plus from '@lucide/svelte/icons/plus';
  import Layers from '@lucide/svelte/icons/layers';
  import ThumbsUp from '@lucide/svelte/icons/thumbs-up';
  import Users from '@lucide/svelte/icons/users';
  import Minus from '@lucide/svelte/icons/minus';
  import List from '@lucide/svelte/icons/list';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import Bbcode from './Bbcode.svelte';
  import { api, inTauri, message } from '../api';
  import { app } from '../state.svelte';
  import { i18n, t } from '../i18n.svelte';
  import { collectionCache, detailsCache, pageCache, key, image, fetchPage, recentlyFetched, revalidate, steam, touch, PAGE_TTL } from '../steam.svelte';
  import { still } from '../still';
  import type { WorkshopCollection, WorkshopQuery, WorkshopItem, WorkshopState, WorkshopDetails, WorkshopSort, WorkshopPage } from '../types';

  type Mode = 'mods' | 'collections';
  const BY_PAGE = 50;

  const requested = app.goWorkshop;
  app.goWorkshop = null;
  let mode = $state<Mode>(requested?.collection ? 'collections' : 'mods');
  let text = $state('');
  let searched = $state('');
  let order = $state<WorkshopSort>('trend');
  let tags = $state<string[]>(requested?.collection ? [] : ['Build 42']);
  let allTags = $state<string[]>([]);
  let menuTags = $state(false);
  let page = $state(1);

  let list = $state<WorkshopItem[] | null>(null);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let sel = $state<number | null>(null);
  let card = $state<WorkshopDetails | null>(null);
  let requirements = $state<WorkshopDetails[] | null>(null);
  let collection = $state<WorkshopCollection | null>(null);
  let chosen = $state<Set<number>>(new Set());
  let loadingCard = $state(false);
  let errorCard = $state<string | null>(null);
  let link = $state('');


  const read = (k: string, def: number) => {
    try {
      const v = Number(localStorage.getItem(k));
      return Number.isFinite(v) && v > 0 ? v : def;
    } catch {
      return def;
    }
  };
  const saveSetting = (k: string, v: number) => {
    try {
      localStorage.setItem(k, String(v));
    } catch {
    }
  };
  let cardWidth = $state(read('ws-ancho-ficha', 380));
  let letter = $state(read('ws-letra', 13));
  const savedView = (): 'list' | 'grid' => {
    if (!inTauri && new URLSearchParams(location.search).get('wsview') === 'grid') return 'grid';
    try {
      return localStorage.getItem('ws-vista') === 'cuadricula' ? 'grid' : 'list';
    } catch {
      return 'list';
    }
  };
  let view = $state(savedView());
  function changeView(v: 'list' | 'grid') {
    view = v;
    try {
      localStorage.setItem('ws-vista', v === 'grid' ? 'cuadricula' : 'lista');
    } catch {
    }
  }
  let screen: HTMLElement | undefined = $state();
  let dragging = $state(false);

  function limit(px: number) {
    const total = screen?.getBoundingClientRect().width ?? 1200;
    return Math.round(Math.max(300, Math.min(total * 0.7, px)));
  }
  function startDrag(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = true;
  }
  function moveDrag(e: PointerEvent) {
    if (!dragging || !screen) return;
    cardWidth = limit(screen.getBoundingClientRect().right - e.clientX);
  }
  function unpinDrag() {
    if (!dragging) return;
    dragging = false;
    saveSetting('ws-ancho-ficha', cardWidth);
  }
  function keySeparator(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    cardWidth = limit(cardWidth + (e.key === 'ArrowLeft' ? 24 : -24));
    saveSetting('ws-ancho-ficha', cardWidth);
  }
  function changeLetter(d: number) {
    letter = Math.max(11, Math.min(20, letter + d));
    saveSetting('ws-letra', letter);
  }

  let states = $state<Record<number, WorkshopState>>({});
  let working = $state<Set<number>>(new Set());

  $effect(() => {
    if (steam.tags.length) allTags = steam.tags;
    else api.workshopTags().then((e) => (allTags = steam.tags = e)).catch(() => {});
  });

  const request = fetchPage;
  const currentQuery = (): WorkshopQuery => ({ text: searched, order, tags: [...tags], page, collections: mode === 'collections' });
  let turn = 0;
  let preloaded = false;

  function apply(p: WorkshopPage) {
    list = p.items;
    total = p.total;
    for (const e of p.items) states[e.id] = e.state;
    if (sel === null && p.items.length) {
      const itemFromUrl = !inTauri ? Number(new URLSearchParams(location.search).get('item')) : 0;
      open(p.items.find((x) => x.id === itemFromUrl) ?? p.items[0]);
    }
  }

  async function load() {
    const c = currentQuery();
    const mine = ++turn;
    error = null;
    const saved = pageCache.get(key(c));
    if (saved) {
      apply(saved.p);
      loading = false;
      document.getElementById('ws-lista')?.scrollTo({ top: 0 });
      if (Date.now() - saved.when > PAGE_TTL) {
        request(c)
          .then((p) => mine === turn && apply(p))
          .catch(() => {});
      }
      return;
    }
    loading = true;
    try {
      const p = await request(c);
      if (mine !== turn) return;
      apply(p);
      if (!preloaded) {
        preloaded = true;
        const other: Mode = mode === 'mods' ? 'collections' : 'mods';
        const m = memory[other];
        request({ text: m.searched, order: m.order, tags: m.tags, page: m.page, collections: other === 'collections' }).catch(() => {});
      }
      document.getElementById('ws-lista')?.scrollTo({ top: 0 });
    } catch (e) {
      if (mine === turn) error = message(e);
    } finally {
      if (mine === turn) loading = false;
    }
  }

  $effect(() => {
    void [searched, order, tags.join('|'), page, mode];
    untrack(load);
  });

  type Memory = { text: string; searched: string; order: WorkshopSort; tags: string[]; page: number; sel: number | null; isCol: boolean };
  const memory: Record<Mode, Memory> = {
    mods: { text: '', searched: '', order: 'trend', tags: ['Build 42'], page: 1, sel: null, isCol: false },
    collections: { text: '', searched: '', order: 'trend', tags: [], page: 1, sel: null, isCol: true },
  };

  function changeMode(m: Mode) {
    if (m === mode) return;
    memory[mode] = { text, searched, order, tags: [...tags], page, sel, isCol: mode === 'collections' };
    const g = memory[m];
    const c: WorkshopQuery = { text: g.searched, order: g.order, tags: g.tags, page: g.page, collections: m === 'collections' };
    if (!pageCache.has(key(c))) list = null;
    mode = m;
    text = g.text;
    searched = g.searched;
    order = g.order;
    tags = [...g.tags];
    page = g.page;
    sel = null;
    card = null;
    collection = null;
    requirements = null;
    errorCard = null;
    if (g.sel !== null) open({ id: g.sel, collection: g.isCol });
  }

  onMount(() => {
    if (requested?.id) open({ id: requested.id, collection: requested.collection });
  });

  function search(e: SubmitEvent) {
    e.preventDefault();
    page = 1;
    searched = text.trim();
  }

  function toggleTag(e: string) {
    tags = tags.includes(e) ? tags.filter((x) => x !== e) : [...tags, e];
    page = 1;
  }

  function fromRow(ev: Event): boolean {
    return !(ev.target instanceof Element && ev.target.closest('button, a, input, select'));
  }
  function rowKey(ev: KeyboardEvent, el: WorkshopItem | { id: number; collection: boolean }) {
    if (ev.target !== ev.currentTarget) return;
    if (ev.key === 'Enter' || ev.key === ' ') {
      ev.preventDefault();
      open(el);
    }
  }

  async function open(el: WorkshopItem | { id: number; collection: boolean }, force = false) {
    if (!force && sel === el.id && !errorCard && (loadingCard || (el.collection ? collection?.collection.id === el.id : card?.id === el.id))) return;
    sel = el.id;
    errorCard = null;
    const colSaved = el.collection ? collectionCache.get(el.id) : undefined;
    const savedCard = el.collection ? undefined : detailsCache.get(el.id);
    if (colSaved || savedCard) {
      touch(el.id);
      card = savedCard?.card ?? null;
      requirements = savedCard?.requirements ?? null;
      collection = colSaved ?? null;
      if (colSaved) chosen = new Set(colSaved.items.filter((h) => !(states[h.id] ?? h.state).subscribed).map((h) => h.id));
      const ids = colSaved ? colSaved.items.map((h) => h.id) : [el.id, ...(savedCard?.requirements.map((r) => r.id) ?? [])];
      void refresh(ids);
      void revalidate(el.id).then((change) => {
        if (change && sel === el.id) open(el, true);
      });
      return;
    }
    if (!force) {
      card = null;
      collection = null;
      requirements = null;
      loadingCard = true;
      await new Promise((r) => setTimeout(r, 150));
      if (sel !== el.id) return;
    }
    try {
      if (el.collection) {
        const c = await api.workshopCollection(el.id);
        if (sel !== el.id) return;
        collection = c;
        collectionCache.set(el.id, c);
        recentlyFetched(el.id);
        for (const h of c.items) states[h.id] = h.state;
        chosen = new Set(c.items.filter((h) => !h.state.subscribed).map((h) => h.id));
      } else {
        const [f] = await api.workshopDetails([el.id], true);
        if (sel !== el.id) return;
        card = f ?? null;
        if (f) {
          states[f.id] = f.state;
          const noAuthor = f.author == null;
          const request = noAuthor ? [f.id, ...f.children] : f.children;
          const back = request.length ? await api.workshopDetails(request, false) : [];
          if (noAuthor) {
            const yo = back.find((r) => r.id === f.id);
            if (yo?.author != null) {
              f.author = yo.author;
              if (card?.id === f.id) card = { ...f };
            }
          }
          const reqs = back.filter((r) => f.children.includes(r.id));
          for (const r of reqs) states[r.id] = r.state;
          if (sel === el.id) requirements = reqs;
          detailsCache.set(f.id, { card: f, requirements: reqs });
          recentlyFetched(f.id);
        }
      }
    } catch (e) {
      if (sel === el.id) errorCard = message(e);
    } finally {
      loadingCard = false;
    }
  }

  function openLink(e: SubmitEvent) {
    e.preventDefault();
    const m = link.match(/[?&]id=(\d+)/) ?? link.trim().match(/^(\d+)$/);
    if (!m) {
      errorCard = t('ws.link_bad');
      return;
    }
    open({ id: Number(m[1]), collection: true });
  }

  let target = $state(app.profileId ?? [...app.profiles].sort((a, b) => (b.last_use ?? 0) - (a.last_use ?? 0))[0]?.id ?? '');
  const profileTarget = $derived(app.profiles.find((p) => p.id === target) ?? null);

  async function subscribe(ids: number[], profile: string | null = null) {
    working = new Set([...working, ...ids]);
    try {
      const failures = await api.workshopSubscribe(ids);
      if (profile) app.addWhenDownloads(ids.filter((id) => !failures.some(([f]) => f === id)), profile);
      if (failures.length) app.notify(failures.map(([id, m]) => `${id}: ${m}`).join(' · '), 'error');
      else app.notify(ids.length === 1 ? t('ws.subscribed_ok') : t('ws.subscribed_many_ok', { n: ids.length }));
      await refresh(ids);
      follow(ids);
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      working = new Set([...working].filter((x) => !ids.includes(x)));
    }
  }

  async function unsubscribe(id: number) {
    working = new Set([...working, id]);
    try {
      await api.workshopUnsubscribe(id);
      app.notify(t('ws.unsubscribed_ok'));
      await refresh([id]);
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      working = new Set([...working].filter((x) => x !== id));
    }
  }

  async function refresh(ids: number[]) {
    try {
      for (const s of await api.workshopStates(ids)) states[s.id] = s;
    } catch {
    }
  }

  const follows = new Set<ReturnType<typeof setInterval>>();
  function follow(ids: number[]) {
    let turns = 0;
    const tic = setInterval(async () => {
      turns++;
      await refresh(ids);
      const ready = ids.every((id) => states[id]?.installed && !states[id]?.downloading && !states[id]?.update);
      if (ready || turns > 120) {
        clearInterval(tic);
        follows.delete(tic);
      }
    }, 1500);
    follows.add(tic);
  }

  onDestroy(() => {
    for (const s of follows) clearInterval(s);
  });

  async function addToProfile(id: number) {
    if (!profileTarget) return;
    try {
      const keys = (await api.library()).filter((m) => m.workshop_id === id && m.id).map((m) => m.key);
      if (keys.length === 0) {
        app.notify(t('ws.no_mods'), 'error');
        return;
      }
      await app.run(() => api.add(profileTarget.id, keys), t('ws.added_a', { n: keys.length, p: profileTarget.name }));
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  const nf = $derived(new Intl.NumberFormat(i18n.language, { notation: 'compact', maximumFractionDigits: 1 }));
  const date = (s: number) => new Date(s * 1000).toLocaleDateString(i18n.language, { day: 'numeric', month: 'short', year: 'numeric' });
  const percent = (e: WorkshopItem) => {
    const n = e.votes_up + e.votes_down;
    return n ? Math.round((e.votes_up / n) * 100) : null;
  };
  const initial = (s: string) =>
    s
      .split(/\s+/)
      .filter((w) => /^[\p{L}\p{N}]/u.test(w))
      .slice(0, 2)
      .map((w) => w[0].toUpperCase())
      .join('');
  const size = (b: number) => (b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);
  const builds = (e: WorkshopItem) => e.tags.filter((x) => /^Build 4[012]$/.test(x)).map((x) => 'B' + x.slice(6));
  const pages = $derived(Math.max(1, Math.min(1000, Math.ceil(total / BY_PAGE))));

  function outsideOfMenu(e: MouseEvent) {
    if (menuTags && !(e.target as HTMLElement).closest('.tags-menu')) menuTags = false;
  }
</script>

<svelte:window onclick={outsideOfMenu} />

{#snippet headerDescription()}
  <div class="head-desc">
    <h3>{t('ws.description')}</h3>
    <div class="letter" role="group" aria-label={t('ws.letter')}>
      <button class="btn subtle" onclick={() => changeLetter(-1)} disabled={letter <= 11} title={t('ws.letter_less')} aria-label={t('ws.letter_less')}><Minus size={13} strokeWidth={2} /></button>
      <span class="num">{letter}</span>
      <button class="btn subtle" onclick={() => changeLetter(1)} disabled={letter >= 20} title={t('ws.letter_more')} aria-label={t('ws.letter_more')}><Plus size={13} strokeWidth={2} /></button>
    </div>
  </div>
{/snippet}

{#snippet pagination()}
  <nav class="pages" aria-label={t('ws.pages')}>
    <button class="btn subtle" disabled={page <= 1 || loading} onclick={() => page--} aria-label={t('ws.previous')}><ChevronLeft size={15} strokeWidth={2} /></button>
    <span class="num">{t('ws.page', { p: page, n: pages, t: nf.format(total) })}</span>
    <button class="btn subtle" disabled={page >= pages || loading} onclick={() => page++} aria-label={t('ws.next')}><ChevronRight size={15} strokeWidth={2} /></button>
  </nav>
{/snippet}

{#snippet chooseProfile()}
  {#if app.profiles.length > 1}
    <select class="field target" bind:value={target} aria-label={t('ws.profile_target')} title={t('ws.profile_target')}>
      {#each app.profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
    </select>
  {/if}
{/snippet}

{#snippet rowState(e: WorkshopItem)}
  {@const s = states[e.id] ?? e.state}
  {#if working.has(e.id)}
    <span class="est">{t('ws.working')}</span>
  {:else if s.downloading || (s.subscribed && s.update)}
    <span class="est"><Download size={13} strokeWidth={2} />{s.total ? t('ws.downloading_pct', { p: Math.round((s.downloaded / s.total) * 100) }) : t('ws.downloading')}</span>
  {:else if s.subscribed}
    <span class="est ok"><Check size={13} strokeWidth={2} />{s.installed ? t('ws.subscribed_installed') : t('ws.subscribed')}</span>
  {:else}
    <button
      class="btn"
      onclick={(ev) => {
        ev.stopPropagation();
        subscribe([e.id]);
      }}>{t('ws.subscribe')}</button
    >
  {/if}
{/snippet}

<section class="screen" bind:this={screen} style:--card-width="{cardWidth}px" class:dragging>
  <div class="main">
    <header class="head">
      <div class="title">
        <h1>{t('ws.title')}</h1>
        <div class="tabs" role="tablist" use:arrows={'tab'}>
          <button role="tab" aria-selected={mode === 'mods'} tabindex={mode === 'mods' ? 0 : -1} class:active={mode === 'mods'} onclick={() => changeMode('mods')}>{t('ws.mods')}</button>
          <button role="tab" aria-selected={mode === 'collections'} tabindex={mode === 'collections' ? 0 : -1} class:active={mode === 'collections'} onclick={() => changeMode('collections')}>{t('ws.collections')}</button>
        </div>
      </div>
      <p class="text">{t('ws.text')}</p>
    </header>

    <div class="bar">
      <form class="search" onsubmit={search}>
        <Search size={14} strokeWidth={1.75} />
        <input class="field" type="search" bind:value={text} placeholder={mode === 'mods' ? t('ws.search') : t('ws.search_col')} aria-label={mode === 'mods' ? t('ws.search') : t('ws.search_col')} />
      </form>
      <select class="field" bind:value={order} aria-label={t('ws.order')} disabled={searched !== ''} onchange={() => (page = 1)}>
        <option value="trend">{t('ws.o.trend')}</option>
        <option value="most_subscribed">{t('ws.o.most_subscribed')}</option>
        <option value="recent">{t('ws.o.recent')}</option>
        <option value="votes">{t('ws.o.votes')}</option>
      </select>
      <div class="tags-menu">
        <button class="btn" aria-haspopup="true" aria-expanded={menuTags} onclick={() => (menuTags = !menuTags)}>
          {t('ws.tags')}{#if tags.length}<span class="num count-et">{tags.length}</span>{/if}<ChevronDown size={13} strokeWidth={2} />
        </button>
        {#if menuTags}
          <div class="menu" role="group" aria-label={t('ws.tags')}>
            {#each allTags as e (e)}
              <label class="option"><input type="checkbox" checked={tags.includes(e)} onchange={() => toggleTag(e)} />{e}</label>
            {/each}
          </div>
        {/if}
      </div>
      {#each tags as e (e)}
        <button class="chip" title={t('ws.remove_tag')} onclick={() => toggleTag(e)}>{e}<CircleX size={12} strokeWidth={2} /></button>
      {/each}
      <div class="view" role="radiogroup" aria-label={t('ws.view')}>
        <button role="radio" aria-checked={view === 'list'} title={t('ws.view_list')} aria-label={t('ws.view_list')} onclick={() => changeView('list')}><List size={15} strokeWidth={1.75} /></button>
        <button role="radio" aria-checked={view === 'grid'} title={t('ws.view_grid')} aria-label={t('ws.view_grid')} onclick={() => changeView('grid')}><LayoutGrid size={15} strokeWidth={1.75} /></button>
      </div>
      {#if mode === 'collections'}
        <form class="link" onsubmit={openLink}>
          <input class="field" bind:value={link} placeholder={t('ws.link')} aria-label={t('ws.link')} spellcheck="false" />
          <button class="btn" type="submit" disabled={!link.trim()}>{t('ws.open')}</button>
        </form>
      {/if}
    </div>

    <div class="list" id="ws-lista" aria-busy={loading}>
      {#if error}
        <div class="empty">
          <p class="severity-blocks row-icon"><CircleX size={15} strokeWidth={2} />{error}</p>
          <div class="actions">
            <button class="btn primary" onclick={load}>{t('error.retry')}</button>
            <button class="btn" onclick={() => api.openWorkshopWeb()}><ExternalLink size={14} strokeWidth={1.75} />{t('ws.open_steam')}</button>
          </div>
        </div>
      {:else if !list || (loading && list.length === 0)}
        {#each Array(9) as _, i (i)}
          <div class="row skeleton" aria-hidden="true"><span class="mini"></span><span class="l1"></span><span class="l2"></span></div>
        {/each}
      {:else if list.length === 0}
        <div class="empty">
          <p>{searched ? t('ws.without_results', { q: searched }) : t('ws.empty')}</p>
          {#if tags.length}<button class="btn" onclick={() => (tags = [])}>{t('ws.remove_filters')}</button>{/if}
        </div>
      {:else if view === 'grid'}
        <div class="grid">
          {#each list as e (e.id)}
            {@const pct = percent(e)}
            <div
              class="card"
              class:sel={sel === e.id}
              role="button"
              tabindex="0"
              onclick={(ev) => fromRow(ev) && open(e)}
              onkeydown={(ev) => rowKey(ev, e)}
            >
              {#if e.preview_view}
                <img class="cover" src={image(e.preview_view, e.updated, 512)} alt="" loading="lazy" referrerpolicy="no-referrer" use:still />
              {:else}
                <span class="cover without-image">{#if e.collection}<Layers size={22} strokeWidth={1.5} />{:else}{initial(e.title)}{/if}</span>
              {/if}
              <span class="name" title={e.title}>{e.title}</span>
              <span class="author">{e.author ?? ''}</span>
              <span class="t-footer">
                <span class="num t-data">
                  {#if e.collection}{t('ws.n_mods', { n: e.children.length })}{:else}<Users size={12} strokeWidth={1.75} />{nf.format(e.subscribers)}{/if}
                  {#if pct !== null}<ThumbsUp size={12} strokeWidth={1.75} />{pct} %{/if}
                </span>
                {#if e.collection}<span class="est">{t('ws.show')}</span>{:else}{@render rowState(e)}{/if}
              </span>
            </div>
          {/each}
        </div>
        {@render pagination()}
      {:else}
        {#each list as e (e.id)}
          {@const pct = percent(e)}
          <div
            class="row"
            class:sel={sel === e.id}
            role="button"
            tabindex="0"
            onclick={(ev) => fromRow(ev) && open(e)}
            onkeydown={(ev) => rowKey(ev, e)}
          >
            {#if e.preview_view}
              <img class="mini" src={image(e.preview_view, e.updated, 128)} alt="" loading="lazy" referrerpolicy="no-referrer" use:still />
            {:else}
              <span class="mini without-image">{#if e.collection}<Layers size={18} strokeWidth={1.5} />{/if}</span>
            {/if}
            <div class="body">
              <div class="l1">
                <span class="name">{e.title}</span>
                <span class="author">{e.author ?? ''}</span>
              </div>
              <div class="l2">{e.collection ? t('ws.n_mods', { n: e.children.length }) + ' · ' : ''}{e.summary}</div>
            </div>
            <div class="data">
              <span class="num" title={t('ws.subscribers')}>{#if !e.collection}<Users size={12} strokeWidth={1.75} />{nf.format(e.subscribers)}{/if}</span>
              <span class="num" title={t('ws.rating')}>{#if pct !== null}<ThumbsUp size={12} strokeWidth={1.75} />{pct} %{/if}</span>
              <span class="builds">{builds(e).join(' ')}</span>
              <span class="date">{date(e.updated)}</span>
            </div>
            <div class="action">
              {#if e.collection}
                <span class="est">{t('ws.show')}</span>
              {:else}
                {@render rowState(e)}
              {/if}
            </div>
          </div>
        {/each}
        {@render pagination()}
      {/if}
    </div>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="separator"
    role="separator"
    aria-orientation="vertical"
    aria-label={t('ws.separator')}
    aria-valuenow={cardWidth}
    tabindex="0"
    title={t('ws.separator')}
    onpointerdown={startDrag}
    onpointermove={moveDrag}
    onpointerup={unpinDrag}
    onpointercancel={unpinDrag}
    ondblclick={() => {
      cardWidth = 380;
      saveSetting('ws-ancho-ficha', cardWidth);
    }}
    onkeydown={keySeparator}
  ></div>

  <aside class="card" aria-live="polite" style:--letter="{letter}px">
    {#if errorCard}
      <p class="severity-blocks row-icon"><CircleX size={15} strokeWidth={2} />{errorCard}</p>
    {:else if loadingCard}
      <div class="schema-card" aria-hidden="true"><span class="img"></span><span class="t"></span><span class="t short"></span></div>
    {:else if card}
      {@const s = states[card.id] ?? card.state}
      {#if card.preview_view}<img class="big" src={image(card.preview_view, card.updated)} alt="" referrerpolicy="no-referrer" use:still />{/if}
      <h2>{card.title}</h2>
      <p class="sub">{#if card.author}{`${card.author} · `}{/if}{t('ws.updated', { f: date(card.updated) })} · {size(card.size)}</p>
      <p class="sub num">
        {t('ws.n_subscribers', { n: nf.format(card.subscribers) })}{#if percent(card) !== null}{' · '}{t('ws.n_rating', { p: percent(card) ?? 0 })}{/if}
      </p>
      {#if card.tags.length}<p class="tags">{#each card.tags as e (e)}<span>{e}</span>{/each}</p>{/if}
      <div class="buttons">
        {#if working.has(card.id)}
          <button class="btn primary" disabled>{t('ws.working')}</button>
        {:else if s.subscribed}
          <button class="btn danger" onclick={() => unsubscribe(card!.id)}>{t('ws.unsubscribe')}</button>
          {#if s.installed && profileTarget}
            <span class="with-target">
              <button class="btn" onclick={() => addToProfile(card!.id)}><Plus size={14} strokeWidth={2} />{app.profiles.length > 1 ? t('ws.add_a') : t('ws.add_a_p', { p: profileTarget.name })}</button>
              {@render chooseProfile()}
            </span>
          {/if}
        {:else}
          {#if profileTarget}
            <span class="with-target">
              <button class="btn primary" onclick={() => subscribe([card!.id], profileTarget.id)}>{app.profiles.length > 1 ? t('ws.subscribe_add_a') : t('ws.subscribe_add', { p: profileTarget.name })}</button>
              {@render chooseProfile()}
            </span>
            <button class="btn subtle" onclick={() => subscribe([card!.id])}>{t('ws.only_subscribe')}</button>
          {:else}
            <button class="btn primary" onclick={() => subscribe([card!.id])}>{t('ws.subscribe')}</button>
          {/if}
        {/if}
        <button class="btn subtle" onclick={() => api.openWorkshop(card!.id)}><ExternalLink size={14} strokeWidth={1.75} />{t('detail.steam')}</button>
      </div>
      {#if s.downloading || (s.subscribed && s.update)}
        <p class="progress"><Download size={13} strokeWidth={2} />{s.total ? t('ws.downloading_pct', { p: Math.round((s.downloaded / s.total) * 100) }) : t('ws.downloading')}</p>
      {/if}
      <h3>{t('ws.requires')}</h3>
      {#if requirements === null}
        <p class="dim">…</p>
      {:else if requirements.length === 0}
        <p class="dim">{t('ws.nothing')}</p>
      {:else}
        <ul class="reqs">
          {#each requirements as r (r.id)}
            {@const rs = states[r.id] ?? r.state}
            <li>
              <button class="link-req" onclick={() => open(r)}>{r.title}</button>
              {#if rs.subscribed}<span class="ok row-icon"><Check size={12} strokeWidth={2} />{t('ws.subscribed')}</span>{:else}<span class="severity-warning">{t('ws.missing')}</span>{/if}
            </li>
          {/each}
        </ul>
        {#if requirements.some((r) => !(states[r.id] ?? r.state).subscribed)}
          <button class="btn" onclick={() => subscribe(requirements!.filter((r) => !(states[r.id] ?? r.state).subscribed).map((r) => r.id))}>{t('ws.subscribe_reqs')}</button>
        {/if}
      {/if}
      {@render headerDescription()}
      <div class="description"><Bbcode text={card.bbcode} /></div>
    {:else if collection}
      {@const c = collection.collection}
      {@const already = collection.items.filter((h) => (states[h.id] ?? h.state).subscribed).length}
      {#if c.preview_view}<img class="big" src={c.preview_view} alt="" referrerpolicy="no-referrer" use:still />{/if}
      <h2>{c.title}</h2>
      <p class="sub">{#if c.author}{`${c.author} · `}{/if}{t('ws.n_mods', { n: c.children.length })} · {t('ws.already_subscribed', { n: already })}</p>
      {#if collection.missing.length}<p class="sub severity-warning">{t('ws.missing_many', { n: collection.missing.length })}</p>{/if}
      <div class="buttons">
        <button class="btn subtle" onclick={() => (chosen = new Set(collection!.items.map((h) => h.id)))}>{t('ws.all')}</button>
        <button class="btn subtle" onclick={() => (chosen = new Set())}>{t('ws.none')}</button>
      </div>
      <ul class="children">
        {#each collection.items as h (h.id)}
          {@const hs = states[h.id] ?? h.state}
          <li>
            <label>
              <input type="checkbox" checked={chosen.has(h.id)} onchange={() => (chosen = chosen.has(h.id) ? new Set([...chosen].filter((x) => x !== h.id)) : new Set([...chosen, h.id]))} />
              {#if h.preview_view}<img class="pic" src={h.preview_view} alt="" loading="lazy" referrerpolicy="no-referrer" use:still />{:else}<span class="gap"></span>{/if}
              <span class="name">{h.title}</span>
            </label>
            {#if hs.subscribed}<span class="ok row-icon" title={t('ws.subscribed')}><Check size={12} strokeWidth={2} /></span>{/if}
          </li>
        {/each}
      </ul>
      <div class="buttons footer">
        {#if [...chosen].some((id) => !(states[id]?.subscribed ?? false))}
          {@const fresh = [...chosen].filter((id) => !(states[id]?.subscribed ?? false))}
          <button class="btn primary" disabled={fresh.some((id) => working.has(id))} onclick={() => subscribe(fresh)}>{t('ws.subscribe_n', { n: fresh.length })}</button>
        {/if}
        <button class="btn subtle" onclick={() => api.openWorkshop(c.id)}><ExternalLink size={14} strokeWidth={1.75} />{t('detail.steam')}</button>
      </div>
      {#if c.bbcode.trim()}{@render headerDescription()}<div class="description"><Bbcode text={c.bbcode} /></div>{/if}
    {:else}
      <p class="dim empty-card">{mode === 'mods' ? t('ws.picks') : t('ws.picks_col')}</p>
    {/if}
  </aside>
</section>

<style>
  .screen {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 5px var(--card-width, 380px);
    height: 100%;
    min-height: 0;
  }
  .screen.dragging {
    cursor: col-resize;
    user-select: none;
  }
  .separator {
    border-left: 1px solid var(--line);
    cursor: col-resize;
    touch-action: none;
  }
  .separator:hover,
  .separator:focus-visible,
  .dragging .separator {
    border-left-color: var(--accent);
    background: var(--accent);
    outline: none;
  }
  .main {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    container-type: inline-size;
  }
  .head {
    padding: 14px 16px 0;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 20px;
  }
  h1 {
    font-size: var(--t-xl);
  }
  .text {
    margin: 3px 0 8px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs button {
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--ink);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
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
    pointer-events: none;
  }
  .search .field {
    width: 240px;
    padding-left: 28px;
  }
  .tags-menu {
    position: relative;
  }
  .with-target {
    display: inline-flex;
    gap: 0;
  }
  .with-target:has(.target) .btn {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .target {
    max-width: 180px;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left: 0;
  }
  .count-et {
    margin-left: 2px;
    padding: 0 6px;
    border-radius: var(--radio-pill);
    background: var(--surface-3);
    font-size: var(--t-xs);
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 10;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 8px;
    width: 360px;
    padding: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    box-shadow: var(--shadow-menu);
    font-size: var(--t-s);
  }
  .option {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 6px;
    border-radius: var(--radio);
    cursor: pointer;
  }
  .option:hover {
    background: var(--surface-2);
  }
  .option input,
  .children input {
    accent-color: var(--accent);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio-pill);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: var(--t-xs);
    cursor: pointer;
  }
  .chip :global(svg) {
    color: var(--ink-3);
  }
  .link {
    display: flex;
    gap: 6px;
    margin-left: 0;
  }
  .link .field {
    width: 230px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
  }
  .row {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) 190px 150px;
    align-items: center;
    gap: 12px;
    height: 64px;
    padding: 0 16px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
    cursor: pointer;
  }
  .row:hover {
    background: color-mix(in srgb, var(--surface-2) 60%, transparent);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 12px;
    padding: 12px 16px;
  }
  .card {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
    overflow: hidden;
    cursor: pointer;
    transition: border-color var(--dur) var(--ease);
  }
  .card:hover {
    border-color: var(--line-strong);
  }
  .card.sel {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .card:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .cover {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    background: var(--surface-3);
    border-bottom: 1px solid var(--line);
  }
  .cover.without-image {
    display: grid;
    place-items: center;
    color: var(--ink-3);
    font-size: 28px;
    font-weight: 600;
  }
  .card .name {
    padding: 8px 10px 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card .author {
    padding: 0 10px;
    color: var(--ink-3);
    font-size: var(--t-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-footer {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 6px;
    margin-top: auto;
    padding: 8px 10px 10px;
    min-height: 44px;
  }
  .t-data {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .view {
    display: inline-flex;
    margin-left: auto;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    overflow: hidden;
  }
  .view button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    border: 0;
    background: var(--surface);
    color: var(--ink-2);
    cursor: pointer;
  }
  .view button + button {
    border-left: 1px solid var(--line-strong);
  }
  .view button[aria-checked='true'] {
    background: var(--surface-3);
    color: var(--ink);
  }
  .mini {
    width: 56px;
    height: 48px;
    border-radius: var(--radio);
    object-fit: cover;
    background: var(--surface-2);
  }
  .without-image {
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }
  .body {
    min-width: 0;
  }
  .l1 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .author {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .l2 {
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .data {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 8px;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .data span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .builds {
    font-weight: 600;
  }
  .action {
    display: flex;
    justify-content: flex-end;
  }
  .action .btn {
    height: 26px;
  }
  .est {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ink-2);
    font-size: var(--t-s);
    white-space: nowrap;
  }
  .ok {
    color: var(--ok);
  }
  .row-icon {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .pages {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 10px;
    padding: 10px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .empty {
    display: grid;
    gap: 12px;
    max-width: 64ch;
    padding: 24px 16px;
    color: var(--ink-2);
  }
  .empty p {
    margin: 0;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .skeleton .mini {
    display: block;
  }
  .skeleton .l1,
  .skeleton .l2 {
    display: block;
    height: 9px;
    border-radius: var(--radio);
    background: var(--surface-2);
  }
  .skeleton .l1 {
    grid-column: 2;
    width: 40%;
    align-self: end;
  }
  .skeleton .l2 {
    grid-column: 2;
    width: 70%;
    align-self: start;
    margin-top: 6px;
  }
  .skeleton {
    grid-template-rows: 1fr 1fr;
  }
  .skeleton .mini {
    grid-row: 1 / 3;
  }

  @container (max-width: 760px) {
    .row {
      grid-template-columns: 56px minmax(0, 1fr) auto;
    }
    .data {
      display: none;
    }
    .search .field {
      width: 200px;
    }
  }
  .card {
    min-height: 0;
    overflow: auto;
    padding: 16px;
    background: var(--surface);
  }
  .big {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: contain;
    border-radius: var(--radio);
    background: var(--surface-2);
    margin-bottom: 12px;
  }
  .card h2 {
    font-size: var(--t-l);
    font-weight: 600;
    line-height: 1.3;
  }
  .card h3 {
    margin: 18px 0 6px;
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .sub {
    margin: 4px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 10px 0 0;
  }
  .tags span {
    padding: 1px 7px;
    border: 1px solid var(--line);
    border-radius: var(--radio-pill);
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .reqs,
  .children {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: var(--t-s);
  }
  .reqs li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 0;
  }
  .link-req {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link-req:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .reqs + .btn {
    margin-top: 6px;
  }
  .children {
    margin-top: 10px;
    border-top: 1px solid var(--line);
  }
  .children li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }
  .children label {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    height: 34px;
    cursor: pointer;
  }
  .children .pic,
  .gap {
    flex: none;
    width: 28px;
    height: 24px;
    border-radius: var(--radio);
    object-fit: cover;
    background: var(--surface-2);
  }
  .footer {
    position: sticky;
    bottom: -16px;
    margin: 0 -16px;
    padding: 10px 16px;
    border-top: 1px solid var(--line);
    background: var(--surface);
  }
  .description {
    font-size: var(--letter, 13px);
  }
  .head-desc {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 18px 0 6px;
  }
  .head-desc h3 {
    margin: 0;
  }
  .letter {
    display: flex;
    align-items: center;
    gap: 2px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .letter .btn {
    height: 22px;
    padding: 0 5px;
  }
  .dim {
    color: var(--ink-3);
    font-size: var(--t-s);
  }
  .empty-card {
    margin: 0;
  }
  .schema-card {
    display: grid;
    gap: 10px;
  }
  .schema-card span {
    border-radius: var(--radio);
    background: var(--surface-2);
  }
  .schema-card .img {
    aspect-ratio: 16 / 9;
  }
  .schema-card .t {
    height: 12px;
    width: 80%;
  }
  .schema-card .short {
    width: 50%;
  }
</style>
