<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Save from '@lucide/svelte/icons/save';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Copy from '@lucide/svelte/icons/copy';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { onSandboxProgress, api, message } from '../api';
  import { app } from '../state.svelte';
  import { i18n, t } from '../i18n.svelte';
  import type { SandboxSchema, SandboxOption, SandboxPage, SandboxProgress, SandboxValue } from '../types';

  const id = $derived(app.view!.id);

  let data = $state<SandboxSchema | null>(null);
  let error = $state<string | null>(null);
  let sandbox = $state<Record<string, SandboxValue>>({});
  let server = $state<Record<string, SandboxValue>>({});
  let origin = $state('default');
  let loading = $state(false);
  let page = $state<string | null>(null);
  let search = $state('');
  let onlyChanged = $state(false);
  let name = $state('servertest');
  let asServer = $state(true);
  let asPreset = $state(true);
  let saving = $state(false);
  let menuCopy = $state(false);
  let boxCopy: HTMLDivElement | undefined = $state();
  const backups = ['ini', 'lua', 'cfg', 'mods'] as const;

  async function copy(que: (typeof backups)[number]) {
    menuCopy = false;
    try {
      const x = await api.sandboxTexts(id, i18n.language, $state.snapshot(sandbox), $state.snapshot(server));
      await navigator.clipboard.writeText(x[que]);
      app.notify(t('sb.copied', { q: t(`sb.copy.${que}`), n: x[que].split('\n').length - 1 }));
    } catch (e) {
      app.notify(e instanceof Error || typeof e === 'string' ? message(e) : t('sb.not_copied'), 'error');
    }
  }

  let progress = $state<SandboxProgress | null>(null);
  $effect(() => {
    let remove: (() => void) | undefined;
    let alive = true;
    onSandboxProgress((p) => (progress = p)).then((f) => (alive ? (remove = f) : f()));
    return () => {
      alive = false;
      remove?.();
    };
  });

  let showLoad = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => () => clearTimeout(timer));

  let turn = 0;

  async function loadSchema() {
    const mine = ++turn;
    error = null;
    data = null;
    progress = null;
    showLoad = false;
    clearTimeout(timer);
    timer = setTimeout(() => (showLoad = true), 200);
    try {
      const d = await api.sandboxSchema(id, i18n.language);
      if (mine !== turn) return;
      data = d;
      page = d.schema.server[0]?.id ?? d.schema.sandbox[0]?.id ?? null;
      const qs = new URLSearchParams(location.search);
      if (qs.get('sbpage')) page = qs.get('sbpage');
      if (qs.get('sbsearch')) search = qs.get('sbsearch')!;
      if (d.servers.length) {
        name = d.servers[0];
        await loadValues(`server:${d.servers[0]}`, mine);
      } else {
        await loadValues('default', mine);
      }
    } catch (e) {
      if (mine === turn) error = message(e);
    } finally {
      if (mine === turn) clearTimeout(timer);
    }
  }

  $effect(() => {
    void id;
    void i18n.language;
    loadSchema();
  });

  async function loadValues(o: string, mine = turn) {
    loading = true;
    try {
      const v = await api.sandboxValues(id, i18n.language, o);
      if (mine !== turn) return;
      sandbox = v.sandbox;
      if (o === 'default' || o.startsWith('server:') || Object.keys(server).length === 0) server = v.server;
      origin = o;
      if (o.startsWith('server:') || o.startsWith('profile:')) name = o.split(':')[1];
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      loading = false;
    }
  }

  const every = $derived.by(() => {
    if (!data) return [] as { page: SandboxPage; section: 'server' | 'sandbox' | 'mods' }[];
    return [
      ...data.schema.server.map((p) => ({ page: p, section: 'server' as const })),
      ...data.schema.sandbox.map((p) => ({ page: p, section: p.from_mod || p.id.startsWith('mod:') ? ('mods' as const) : ('sandbox' as const) })),
    ];
  });
  const isServer = (p: SandboxPage) => p.id.startsWith('ini:');
  const values = (p: SandboxPage) => (isServer(p) ? server : sandbox);

  function default_value(o: SandboxOption): SandboxValue {
    return o.kind.default_value as SandboxValue;
  }
  function equal(a: SandboxValue | undefined, b: SandboxValue) {
    if (typeof b === 'number') return Number(a) === b;
    return a === b;
  }
  const changed = (p: SandboxPage, o: SandboxOption) => !equal(values(p)[o.name], default_value(o));
  const changedIn = (p: SandboxPage) => p.groups.reduce((n, g) => n + g.options.filter((o) => changed(p, o)).length, 0);

  const pageTitle = (p: SandboxPage) => (p.title ? p.title : t(isServer(p) ? 'sb.others_server' : 'sb.others_sandbox'));

  const q = $derived(search.trim().toLowerCase());
  const matches = (o: SandboxOption) =>
    !q || o.title.toLowerCase().includes(q) || o.name.toLowerCase().includes(q) || (o.help ?? '').toLowerCase().includes(q);

  const view = $derived.by(() => {
    const filters = q !== '' || onlyChanged;
    const sources = filters ? every.map((x) => x.page) : every.filter((x) => x.page.id === page).map((x) => x.page);
    return sources
      .map((p) => ({
        page: p,
        groups: p.groups
          .map((g) => ({ ...g, options: g.options.filter((o) => matches(o) && (!onlyChanged || changed(p, o))) }))
          .filter((g) => g.options.length),
      }))
      .filter((x) => x.groups.length);
  });

  function pin(p: SandboxPage, o: SandboxOption, v: SandboxValue) {
    if (isServer(p)) server[o.name] = v;
    else sandbox[o.name] = v;
  }

  function num(p: SandboxPage, o: SandboxOption, text: string) {
    const n = Number(text.replace(',', '.'));
    if (!Number.isFinite(n)) return;
    const tp = o.kind;
    if (tp.t === 'integer' || tp.t === 'decimal') {
      const bounded = Math.min(tp.max, Math.max(tp.min, tp.t === 'integer' ? Math.round(n) : n));
      pin(p, o, bounded);
    }
  }

  function textDefault(o: SandboxOption): string {
    const tp = o.kind;
    if (tp.t === 'boolean') return tp.default_value ? t('common.yes') : t('common.no');
    if (tp.t === 'list') return o.values[tp.default_value - 1] ?? String(tp.default_value);
    if (tp.t === 'text') return tp.default_value === '' ? t('sb.empty') : tp.default_value;
    return String(tp.default_value);
  }
  const range = (o: SandboxOption) => {
    const tp = o.kind;
    if (tp.t !== 'integer' && tp.t !== 'decimal') return '';
    const f = (x: number) => (Math.abs(x) >= 2147483647 ? '∞' : String(x));
    return t('common.range', { a: f(tp.min), b: f(tp.max) });
  };

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!asServer && !asPreset) return;
    saving = true;
    try {
      const written = await api.sandboxSave(id, i18n.language, name, $state.snapshot(sandbox), $state.snapshot(server), asServer, asPreset);
      app.notify(t('sb.saved', { n: written.length, name }));
      if (data) {
        const d = await api.sandboxSchema(id, i18n.language);
        data.servers = d.servers;
        data.profile_presets = d.profile_presets;
      }
    } catch (err) {
      app.notify(message(err), 'error');
    } finally {
      saving = false;
    }
  }
</script>

{#snippet control(p: SandboxPage, o: SandboxOption)}
  {@const v = values(p)[o.name]}
  {@const tp = o.kind}
  {#if tp.t === 'boolean'}
    <input type="checkbox" checked={v === true} onchange={(e) => pin(p, o, e.currentTarget.checked)} aria-label={o.title} />
  {:else if tp.t === 'list'}
    <select class="field" value={Number(v)} onchange={(e) => pin(p, o, Number(e.currentTarget.value))} aria-label={o.title}>
      {#each o.values as text, i (i)}<option value={i + 1}>{text}</option>{/each}
    </select>
  {:else if tp.t === 'integer' || tp.t === 'decimal'}
    <input
      class="field num"
      inputmode="decimal"
      value={String(v ?? '')}
      aria-label={o.title}
      onchange={(e) => {
        num(p, o, e.currentTarget.value);
        e.currentTarget.value = String(values(p)[o.name]);
      }}
    />
  {:else if tp.multiline}
    <textarea class="field" rows="3" value={String(v ?? '')} oninput={(e) => pin(p, o, e.currentTarget.value)} aria-label={o.title}></textarea>
  {:else}
    <input class="field text-long" value={String(v ?? '')} oninput={(e) => pin(p, o, e.currentTarget.value)} aria-label={o.title} />
  {/if}
{/snippet}

<svelte:window
  onclick={(e) => {
    if (menuCopy && boxCopy && !boxCopy.contains(e.target as Node)) menuCopy = false;
  }}
/>

<div class="editor">
  {#if error}
    <p class="msg severity-blocks"><CircleX size={15} strokeWidth={2} />{error}</p>
  {:else if !data}
    {#if showLoad}
    <div class="load" role="status" aria-live="polite">
      <p class="msg"><LoaderCircle size={15} strokeWidth={2} class="rotates" />{t('sb.loading')}</p>
      <div class="progress" role="progressbar" aria-label={t('sb.loading')} aria-valuemin={0} aria-valuemax={progress?.total ?? 1} aria-valuenow={progress?.done ?? 0}>
        <span style:width="{progress ? (100 * progress.done) / progress.total : 0}%"></span>
      </div>
      {#if progress}
        <p class="step dim">{t(`sb.stage.${progress.stage}`)} · {t('sb.step', { h: Math.min(progress.done + 1, progress.total), n: progress.total })}</p>
      {/if}
    </div>
    {/if}
  {:else}
    <div class="bar">
      <label class="row-field">
        <span class="tag">{t('sb.load_of')}</span>
        <select class="field" value={origin} disabled={loading} aria-label={t('sb.load_of')} onchange={(e) => loadValues(e.currentTarget.value)}>
          <option value="default">{t('sb.default')}</option>
          {#if data.servers.length}
            <optgroup label={t('sb.servers_profile')}>
              {#each data.servers as s (s)}<option value="server:{s}">{s}</option>{/each}
            </optgroup>
          {/if}
          <optgroup label={t('sb.game_presets')}>
            {#each data.game_presets as s (s)}<option value="game:{s}">{s}</option>{/each}
          </optgroup>
          {#if data.profile_presets.length}
            <optgroup label={t('sb.profile_presets')}>
              {#each data.profile_presets as s (s)}<option value="profile:{s}">{s}</option>{/each}
            </optgroup>
          {/if}
          {#if data.user_presets.length}
            <optgroup label={t('sb.user_presets')}>
              {#each data.user_presets as s (s)}<option value="user:{s}">{s}</option>{/each}
            </optgroup>
          {/if}
        </select>
      </label>
      <label class="search">
        <Search size={14} strokeWidth={1.75} />
        <input class="field" type="search" bind:value={search} placeholder={t('sb.search')} />
      </label>
      <label class="checkbox"><input type="checkbox" bind:checked={onlyChanged} />{t('sb.only_changed')}</label>
      <div class="copy" bind:this={boxCopy}>
        <button class="btn" aria-haspopup="menu" aria-expanded={menuCopy} onclick={() => (menuCopy = !menuCopy)}>
          <Copy size={14} strokeWidth={1.75} />{t('sb.copy')}<ChevronDown size={13} strokeWidth={2} />
        </button>
        {#if menuCopy}
          <div
            class="menu"
            role="menu"
            tabindex="-1"
            onkeydown={(e) => {
              if (e.key === 'Escape') menuCopy = false;
            }}
          >
            {#each backups as c (c)}
              <button class="item" role="menuitem" onclick={() => copy(c)}>
                <span class="item-title">{t(`sb.copy.${c}`)}</span>
                <span class="item-help">{t(`sb.copy.${c}_help`)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <form class="save" onsubmit={save}>
        <input class="field name" bind:value={name} aria-label={t('sb.name')} placeholder={t('sb.name')} spellcheck="false" />
        <label class="checkbox" title={t('sb.as_server_help')}><input type="checkbox" bind:checked={asServer} />{t('sb.as_server')}</label>
        <label class="checkbox" title={t('sb.as_preset_help')}><input type="checkbox" bind:checked={asPreset} />{t('sb.as_preset')}</label>
        <button class="btn primary" type="submit" disabled={saving || !name.trim() || (!asServer && !asPreset)}>
          <Save size={14} strokeWidth={1.75} />{t('sb.save')}
        </button>
      </form>
    </div>

    <div class="body">
      <nav class="pages" aria-label={t('sb.pages')}>
        {#each ['server', 'sandbox', 'mods'] as section (section)}
          {@const list = every.filter((x) => x.section === section)}
          {#if list.length}
            <h3>{t(`sb.section.${section}` as Parameters<typeof t>[0])}</h3>
            {#each list as { page: pg } (pg.id)}
              {@const n = changedIn(pg)}
              <button class="page" class:current={page === pg.id && !q && !onlyChanged} onclick={() => {
                page = pg.id;
                search = '';
                onlyChanged = false;
              }}>
                <span class="title">{pageTitle(pg)}</span>
                {#if n}<span class="num changes" title={t('sb.n_changed', { n })}>{n}</span>{/if}
              </button>
            {/each}
          {/if}
        {/each}
      </nav>

      <div class="options" aria-busy={loading}>
        {#if view.length === 0}
          <p class="msg dim">{q ? t('sb.without_results', { q: search }) : t('sb.nothing_changed')}</p>
        {/if}
        {#each view as { page: pg, groups } (pg.id)}
          {#if q || onlyChanged}<h2 class="page-title">{pageTitle(pg)}</h2>{/if}
          {#if isServer(pg) && !q && !onlyChanged}<p class="note">{t('sb.note_server')}</p>{/if}
          {#each groups as g, gi (gi)}
            {#if g.title}<h3 class="group">{g.title}</h3>{/if}
            {#each g.options as o (o.name)}
              {@const mod = changed(pg, o)}
              <div class="option" class:changed={mod}>
                <div class="text">
                  <span class="name">{o.title}</span>
                  {#if o.title !== o.name}<span class="key mono">{o.name}</span>{/if}
                  {#if o.help}<p class="help">{o.help}</p>{/if}
                  <p class="default">
                    {t('sb.default_location')}: {textDefault(o)}{#if range(o)}{' · '}{t('sb.range')}: {range(o)}{/if}
                  </p>
                </div>
                <div class="value">
                  {@render control(pg, o)}
                  <button class="btn subtle icon" class:unchanged={!mod} title={t('sb.reset')} aria-label={t('sb.reset')} onclick={() => pin(pg, o, default_value(o))}>
                    <RotateCcw size={13} strokeWidth={1.75} />
                  </button>
                </div>
              </div>
            {/each}
          {/each}
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    background: var(--ground);
  }
  .row-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .tag {
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 240px;
    min-width: 140px;
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
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--t-s);
    white-space: nowrap;
  }
  .checkbox input,
  .option input[type='checkbox'] {
    accent-color: var(--accent);
  }
  .copy {
    position: relative;
    margin-left: auto;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 10;
    min-width: 320px;
    padding: 4px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    box-shadow: var(--shadow-menu);
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radio);
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--surface-3);
  }
  .item-title {
    font-size: var(--t-s);
    font-weight: 600;
  }
  .item-help {
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .save {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    width: 120px;
  }
  @media (max-width: 1360px) {
    .tag {
      display: none;
    }
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 230px minmax(0, 1fr);
  }
  .pages {
    min-height: 0;
    overflow: auto;
    padding: 6px 6px 16px;
    border-right: 1px solid var(--line);
    background: var(--surface-2);
  }
  .pages h3 {
    margin: 10px 8px 4px;
    color: var(--ink-3);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .page {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    min-height: 26px;
    padding: 3px 8px;
    border: 0;
    border-radius: var(--radio);
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: var(--t-s);
    text-align: left;
    cursor: pointer;
  }
  .page:hover {
    background: var(--surface-3);
  }
  .page.current {
    background: var(--accent-soft);
    font-weight: 600;
  }
  .page .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .changes {
    flex: none;
    padding: 0 6px;
    border-radius: var(--radio-pill);
    background: var(--surface);
    color: var(--accent);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .options {
    min-height: 0;
    overflow: auto;
    padding: 6px 16px 24px;
    background: var(--surface);
  }
  .page-title {
    margin: 16px 0 4px;
    font-size: var(--t-l);
    font-weight: 600;
  }
  .group {
    margin: 16px 0 2px;
    color: var(--ink-2);
    font-size: var(--t-s);
    font-weight: 600;
  }
  .note {
    margin: 8px 0 4px;
    color: var(--ink-2);
    font-size: var(--t-xs);
    max-width: 80ch;
  }
  .option {
    max-width: 1040px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(220px, 300px);
    gap: 16px;
    align-items: start;
    padding: 8px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }
  .option.changed .name {
    font-weight: 600;
  }
  .option.changed .name::after {
    content: ' •';
    color: var(--accent);
  }
  .name {
    font-size: var(--t-m);
  }
  .key {
    margin-left: 6px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .help {
    margin: 2px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
    white-space: pre-line;
    max-width: 75ch;
  }
  .default {
    margin: 2px 0 0;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .value {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
  }
  .value .field {
    width: 100%;
    max-width: 260px;
  }
  .value textarea {
    resize: vertical;
    font: inherit;
  }
  .icon {
    padding: 0 6px;
    height: 24px;
  }
  .unchanged {
    visibility: hidden;
  }
  .msg {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 24px 16px;
  }
  .dim {
    color: var(--ink-3);
  }
  .load {
    margin: auto;
    width: min(420px, 100% - 32px);
    padding: 24px 0;
  }
  .load .msg {
    justify-content: center;
    padding: 0 0 10px;
  }
  .load .step {
    text-align: center;
  }
  .load :global(.rotates) {
    color: var(--accent);
    animation: rotate 900ms linear infinite;
  }
  @keyframes rotate {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .load :global(.rotates) {
      animation: none;
    }
  }
  .progress {
    height: 4px;
    border-radius: var(--radio);
    background: var(--surface-3);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width var(--dur) var(--ease);
  }
  .step {
    margin: 8px 0 0;
    font-size: var(--t-xs);
  }
  .mono {
    font-family: var(--mono);
  }
</style>
