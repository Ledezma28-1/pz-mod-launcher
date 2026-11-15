<script lang="ts">
  import { arrows } from '../keys';
  import Search from '@lucide/svelte/icons/search';
  import Copy from '@lucide/svelte/icons/copy';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import FileUp from '@lucide/svelte/icons/file-up';
  import ArrowDownToLine from '@lucide/svelte/icons/arrow-down-to-line';
  import Pause from '@lucide/svelte/icons/pause';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import X from '@lucide/svelte/icons/x';
  import Radio from '@lucide/svelte/icons/radio';
  import WhatHappened from './WhatHappened.svelte';
  import BisectPanel from './BisectPanel.svelte';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { Issue } from '../types';

  let { compact = false, close }: { compact?: boolean; close?: () => void } = $props();

  const MAX_VISIBLE = 2500;

  type View = 'live' | 'errors';
  let view = $state<View>(new URLSearchParams(location.search).get('console') === 'errors' ? 'errors' : 'live');
  let search = $state('');
  let levels = $state({ error: true, warn: true, log: true });
  let follow = $state(true);
  let expanded = $state<Set<string>>(new Set());
  let showWarnings = $state(false);

  let file = $state<{ name: string; lines: string[]; errors: Issue[] } | null>(null);
  let entry: HTMLInputElement | undefined = $state();

  const profile = $derived(app.session && !app.session.finished ? app.session.profile : app.profileId);
  const live = $derived(!file && !!app.session && !app.session.finished && app.session.profile === profile);

  $effect(() => {
    if (profile && !file) void app.loadConsole(profile);
  });

  const lines = $derived(file ? file.lines : app.console.profile === profile ? app.console.lines : []);
  const dropped = $derived(!file && app.console.profile === profile ? app.console.dropped : 0);
  const errors = $derived(file ? file.errors : app.errors.profile === profile ? app.errors.list : []);

  function levelOf(l: string): 'error' | 'warn' | 'log' | 'other' {
    const c = l.startsWith('[') ? l.slice(l.indexOf('] ') + 2) : l;
    if (c.startsWith('ERROR')) return 'error';
    if (c.startsWith('WARN')) return 'warn';
    if (c.startsWith('LOG') || c.startsWith('DEBUG')) return 'log';
    return 'other';
  }

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    type N = 'error' | 'warn' | 'log';
    const out: { n: number; l: string; level: N }[] = [];
    let last: N = 'log';
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i];
      const raw = levelOf(l);
      const nv: N = raw === 'other' ? last : raw;
      last = nv;
      if (!levels[nv]) continue;
      if (q && !l.toLowerCase().includes(q)) continue;
      out.push({ n: dropped + i + 1, l, level: nv });
    }
    return out.length > MAX_VISIBLE ? out.slice(-MAX_VISIBLE) : out;
  });

  let box: HTMLDivElement | undefined = $state();
  $effect(() => {
    void filtered.length;
    if (follow && box && view === 'live') queueMicrotask(() => box && (box.scrollTop = box.scrollHeight));
  });
  function onScroll() {
    if (!box) return;
    const down = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
    if (!down && follow) follow = false;
    else if (down && !follow) follow = true;
  }
  function atEnd() {
    follow = true;
    if (box) box.scrollTop = box.scrollHeight;
  }

  function modName(id: string): string {
    return app.view?.rows.find((r) => r.id.replace(/^\\/, '') === id)?.name ?? id;
  }

  const errs = $derived(errors.filter((e) => e.severity === 'error'));
  const warnings = $derived(errors.filter((e) => e.severity === 'warning'));
  function make_groups(list: Issue[]) {
    const m = new Map<string, Issue[]>();
    for (const e of list) {
      const k = e.mod_name ?? (e.mod_id ? modName(e.mod_id) : '');
      m.set(k, [...(m.get(k) ?? []), e]);
    }
    return [...m.entries()].sort((a, b) => (a[0] === '' ? 1 : b[0] === '' ? -1 : b[1].length - a[1].length));
  }
  const key = (e: Issue) => `${e.kind}|${e.mod_name}|${e.key ?? e.message}|${e.file}|${e.line_file}`;
  function toggle(e: Issue) {
    const s = new Set(expanded);
    const k = key(e);
    if (s.has(k)) s.delete(k);
    else s.add(k);
    expanded = s;
  }
  function showInConsole(e: Issue) {
    view = 'live';
    follow = false;
    search = e.message.slice(0, 60);
  }
  function goToMod(id: string) {
    app.screen = 'profile';
    app.tab = 'order';
    app.selection = id;
  }
  const inProfile = (id: string | null) => !!id && !file && !!app.view?.rows.some((f) => f.id === id);

  async function copy() {
    try {
      await navigator.clipboard.writeText(filtered.map((x) => x.l).join('\n'));
      app.notify(t('console.copied', { n: filtered.length }));
    } catch {
      app.notify(t('export.not_copied'), 'error');
    }
  }
  async function displayFile() {
    if (!profile) return;
    try {
      await api.openFolder(await api.consolePath(profile));
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }
  async function openLog(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    try {
      const text = await f.text();
      file = { name: f.name, lines: text.split(/\r?\n/), errors: await api.consoleAnalyze(text) };
      follow = false;
    } catch (err) {
      app.notify(message(err), 'error');
    }
  }
</script>

{#snippet issue(e: Issue)}
  {@const open = expanded.has(key(e))}
  <li class="issue" class:open>
    <button class="issue-head" aria-expanded={open} onclick={() => toggle(e)}>
      <ChevronRight size={14} strokeWidth={2} class="arrow" />
      {#if e.severity === 'error'}<CircleX size={14} strokeWidth={2} class="red" />{:else}<TriangleAlert size={14} strokeWidth={2} class="ochre" />{/if}
      <span class="msg">{e.message}</span>
      <span class="kind">{t(`console.kind.${e.kind}`)}</span>
      {#if e.from_game}<span class="noise" title={t('console.from_game_help')}>{t('console.from_game')}</span>
      {:else if e.noise}<span class="noise" title={t('console.noise_help')}>{t('console.noise')}</span>{/if}
      {#if e.times > 1}<span class="times num">×{e.times}</span>{/if}
      {#if e.last_hour}<span class="hour num">{e.last_hour}</span>{/if}
    </button>
    {#if open}
      <div class="issue-detail">
        {#if e.file}
          <p class="where mono">{e.file}{#if e.line_file !== null}:{e.line_file}{/if}</p>
        {/if}
        {#if e.trace.length}
          <ol class="trace mono">
            {#each e.trace as p, i (i)}
              <li>
                <span class="fn">{p.func}</span> · {p.file}{#if p.line !== null}:{p.line}{/if}
                <span class="de">{p.from_mod ?? t('console.game')}</span>
              </li>
            {/each}
          </ol>
        {/if}
        {#if e.java.length}
          <pre class="java mono">{e.java.join('\n')}</pre>
        {/if}
        {#if e.mods_involved.length}
          <p class="line-n">{t(e.kind === 'require' ? 'console.involved_require' : 'console.involved', { m: e.mods_involved.map(modName).join(', ') })}</p>
        {/if}
        <p class="line-n">{t('console.lines', { a: e.first, b: e.last })}</p>
        <div class="issue-actions">
          <button class="btn" onclick={() => showInConsole(e)}>{t('console.show_in_console')}</button>
          {#if inProfile(e.mod_id)}<button class="btn" onclick={() => goToMod(e.mod_id!)}>{t('console.go_to_mod')}</button>{/if}
          <button class="btn subtle" onclick={() => navigator.clipboard.writeText([e.message, ...e.trace.map((p) => `${p.func} -- ${p.file}:${p.line ?? ''} | ${p.from_mod ?? 'Vanilla'}`), ...e.java].join('\n'))}>
            <Copy size={13} strokeWidth={1.75} />{t('export.copy')}
          </button>
        </div>
      </div>
    {/if}
  </li>
{/snippet}

{#snippet groups(list: Issue[])}
  {#each make_groups(list) as [mod, incs] (mod)}
    <section class="group">
      <h3>
        <span>{mod || t('console.no_mod')}</span>
        <span class="num dim">{incs.reduce((n, e) => n + e.times, 0)}</span>
      </h3>
      <ul>
        {#each incs as e (key(e))}{@render issue(e)}{/each}
      </ul>
    </section>
  {/each}
{/snippet}

<div class="console" class:compact>
  {#if app.bisect && !file && !compact && profile === app.bisect.profile}<BisectPanel />{/if}
  {#if profile && !file && !compact}<WhatHappened {profile} />{/if}
  <div class="bar">
    <div class="views" role="tablist" aria-label={t('console.console')} use:arrows={'tab'}>
      <button role="tab" aria-selected={view === 'live'} tabindex={view === 'live' ? 0 : -1} onclick={() => (view = 'live')}>
        {#if live}<Radio size={13} strokeWidth={2} class="alive" />{/if}{live ? t('console.live') : t('console.console')}
      </button>
      <button role="tab" aria-selected={view === 'errors'} tabindex={view === 'errors' ? 0 : -1} onclick={() => (view = 'errors')}>
        {t('console.errors')}
        {#if errs.length}<span class="counter red num">{errs.length}</span>{/if}
      </button>
    </div>

    {#if file}
      <span class="chip"><span class="mono">{file.name}</span><button class="btn subtle mini" aria-label={t('console.close_file')} title={t('console.close_file')} onclick={() => (file = null)}><X size={12} strokeWidth={2} /></button></span>
    {/if}

    {#if view === 'live'}
      <label class="search">
        <Search size={13} strokeWidth={1.75} />
        <input class="field" type="search" bind:value={search} placeholder={t('console.search')} />
      </label>
      <div class="levels">
        <label class="n-error"><input type="checkbox" bind:checked={levels.error} />ERROR</label>
        <label class="n-warn"><input type="checkbox" bind:checked={levels.warn} />WARN</label>
        <label><input type="checkbox" bind:checked={levels.log} />LOG</label>
      </div>
    {:else}
      <label class="checkbox"><input type="checkbox" bind:checked={showWarnings} />{t('console.show_warnings', { n: warnings.length })}</label>
    {/if}

    <div class="right">
      {#if view === 'live'}
        {#if follow}
          <button class="btn subtle" title={t('console.pause_help')} onclick={() => (follow = false)}><Pause size={13} strokeWidth={2} />{t('console.pause')}</button>
        {:else}
          <button class="btn subtle" onclick={atEnd}><ArrowDownToLine size={13} strokeWidth={2} />{t('console.at_end')}</button>
        {/if}
        <button class="btn subtle" onclick={copy}><Copy size={13} strokeWidth={1.75} />{t('export.copy')}</button>
      {/if}
      {#if !file}
        <button class="btn subtle" onclick={displayFile} disabled={!profile}><FolderOpen size={13} strokeWidth={1.75} />{t('console.display')}</button>
      {/if}
      <button class="btn subtle" onclick={() => entry?.click()}><FileUp size={13} strokeWidth={1.75} />{t('console.open_log')}</button>
      {#if close}
        <button class="btn subtle close" aria-label={t('console.close_panel')} title={t('console.close_panel')} onclick={close}><X size={14} strokeWidth={2} /></button>
      {/if}
      <input bind:this={entry} class="hidden" type="file" accept=".txt,.log" tabindex="-1" aria-hidden="true" onchange={openLog} />
    </div>
  </div>

  {#if view === 'live'}
    <div class="lines mono" bind:this={box} onscroll={onScroll} role="log" aria-live="off">
      {#if lines.length === 0}
        <p class="empty">{profile ? t('console.empty') : t('console.without_profile')}</p>
      {:else}
        {#if filtered.length === MAX_VISIBLE}<p class="warning-max">{t('console.max', { n: MAX_VISIBLE })}</p>{/if}
        {#each filtered as x (x.n)}
          <div class="l {x.level}"><span class="n num">{x.n}</span><span class="t">{x.l}</span></div>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="errors">
      {#if errs.length === 0 && (!showWarnings || warnings.length === 0)}
        <p class="empty">{lines.length ? t('console.without_errors') : t('console.empty')}</p>
      {/if}
      {@render groups(errs)}
      {#if showWarnings && warnings.length}
        <h2 class="sub">{t('console.warnings')}</h2>
        {@render groups(warnings)}
      {/if}
    </div>
  {/if}
</div>

<style>
  .console {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--ground);
  }
  .views {
    display: flex;
    gap: 2px;
  }
  .views button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--radio);
    background: none;
    color: var(--ink-2);
    font: inherit;
    font-size: var(--t-s);
    font-weight: 500;
    cursor: pointer;
  }
  .views button[aria-selected='true'] {
    background: var(--surface);
    border-color: var(--line);
    color: var(--ink);
  }
  .views :global(.alive) {
    color: var(--accent);
  }
  .counter {
    padding: 0 6px;
    border-radius: var(--radio-pill);
    font-size: var(--t-xs);
    line-height: 17px;
  }
  .counter.red {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding-left: 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    font-size: var(--t-xs);
  }
  .mini {
    height: 22px;
    padding: 0 4px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 260px;
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
    height: 26px;
    padding-left: 26px;
  }
  .levels {
    display: flex;
    gap: 10px;
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .levels label,
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--t-xs);
  }
  .levels input,
  .checkbox input {
    accent-color: var(--accent);
  }
  .n-error {
    color: var(--danger);
  }
  .n-warn {
    color: var(--warn);
  }
  .right {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }
  .right .btn {
    height: 26px;
    font-size: var(--t-xs);
  }

  .lines {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0 8px;
    font-size: 11.5px;
    line-height: 17px;
  }
  .l {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr);
    gap: 10px;
    padding: 0 12px 0 0;
    content-visibility: auto;
    contain-intrinsic-size: auto 17px;
  }
  .l:hover {
    background: var(--surface-2);
  }
  .n {
    color: var(--ink-3);
    text-align: right;
    user-select: none;
  }
  .t {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .l.error .t {
    color: var(--danger);
  }
  .l.warn .t {
    color: var(--warn);
  }
  .l.log .t {
    color: var(--ink-2);
  }
  .empty,
  .warning-max {
    margin: 0;
    padding: 18px 16px;
    color: var(--ink-3);
    font-family: var(--font);
    font-size: var(--t-s);
  }
  .warning-max {
    padding: 6px 16px;
  }

  .errors {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 16px 20px;
  }
  .group h3 {
    display: flex;
    justify-content: space-between;
    margin: 14px 0 4px;
    font-size: var(--t-s);
    font-weight: 600;
  }
  .sub {
    margin: 22px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-s);
    font-weight: 600;
  }
  .group ul {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: var(--radio);
  }
  .issue + .issue {
    border-top: 1px solid var(--line);
  }
  .issue-head {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    min-height: 30px;
    padding: 4px 10px;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: var(--t-s);
    text-align: left;
    cursor: pointer;
  }
  .issue-head:hover {
    background: var(--surface-2);
  }
  .issue-head :global(.arrow) {
    flex: none;
    color: var(--ink-3);
    transition: transform var(--dur) var(--ease);
  }
  .issue.open .issue-head :global(.arrow) {
    transform: rotate(90deg);
  }
  .issue-head :global(.red) {
    flex: none;
    color: var(--danger);
  }
  .issue-head :global(.ochre) {
    flex: none;
    color: var(--warn);
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .issue.open .msg {
    white-space: normal;
  }
  .kind,
  .hour {
    flex: none;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .noise {
    flex: none;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .times {
    flex: none;
    padding: 0 6px;
    border-radius: var(--radio-pill);
    background: var(--surface-3);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .issue-detail {
    padding: 2px 12px 12px 38px;
    font-size: var(--t-s);
  }
  .where {
    margin: 0 0 6px;
    font-size: 12px;
  }
  .trace {
    margin: 0 0 8px;
    padding-left: 18px;
    font-size: 11.5px;
    line-height: 1.6;
  }
  .fn {
    font-weight: 600;
  }
  .de {
    margin-left: 6px;
    color: var(--ink-3);
  }
  .java {
    margin: 0 0 8px;
    padding: 8px 10px;
    max-height: 180px;
    overflow: auto;
    border-radius: var(--radio);
    background: var(--surface-2);
    font-size: 11px;
    white-space: pre;
  }
  .line-n {
    margin: 0 0 8px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .issue-actions {
    display: flex;
    gap: 6px;
  }
  .dim {
    color: var(--ink-3);
  }
  .compact .bar {
    padding: 4px 10px;
  }
  .close {
    padding: 0 6px;
  }
</style>
