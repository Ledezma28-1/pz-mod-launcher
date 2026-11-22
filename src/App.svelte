<script lang="ts">
  import type { ViewProgress } from './lib/types';
  import { PALETTES, type Palette } from './lib/palettes';
  import { onMount } from 'svelte';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Check from '@lucide/svelte/icons/check';
  import { inTauri } from './lib/api';
  import { app } from './lib/state.svelte';
  import { t } from './lib/i18n.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import Disclaimer from './lib/components/Disclaimer.svelte';
  import ProfileHeader from './lib/components/ProfileHeader.svelte';
  import ScreenError from './lib/components/ScreenError.svelte';
  import OrderTable from './lib/components/OrderTable.svelte';
  import OrderProposal from './lib/components/OrderProposal.svelte';
  import ConflictList from './lib/components/ConflictList.svelte';
  import ProfileSettings from './lib/components/ProfileSettings.svelte';
  import ProfileExport from './lib/components/ProfileExport.svelte';
  import MapsTab from './lib/components/MapsTab.svelte';
  import SandboxEditor from './lib/components/SandboxEditor.svelte';
  import Console from './lib/components/Console.svelte';
  import Terminal from '@lucide/svelte/icons/square-terminal';
  import DetailPanel from './lib/components/DetailPanel.svelte';
  import Library from './lib/components/Library.svelte';
  import Workshop from './lib/components/Workshop.svelte';
  import Home from './lib/components/Home.svelte';
  import Help from './lib/components/Help.svelte';
  import ZbView from './lib/components/ZbView.svelte';
  import InstallView from './lib/components/InstallView.svelte';
  import AppSettings from './lib/components/AppSettings.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import ConflictBanner from './lib/components/ConflictBanner.svelte';
  import GameListBanner from './lib/components/GameListBanner.svelte';
  import SavesBanner from './lib/components/SavesBanner.svelte';
  import { api, logUiError, message } from './lib/api';

  let gamePath = $state('');

  const HEIGHT_MIN = 120;
  const HEIGHT_DEFAULT = 300;
  let consolePanel: HTMLDivElement | undefined = $state();
  let consoleHeight = $state(readHeight());
  let dragConsole = $state(false);
  function readHeight() {
    try {
      const v = Number(localStorage.getItem('consola-alto'));
      if (Number.isFinite(v) && v >= HEIGHT_MIN) return v;
    } catch {
    }
    return HEIGHT_DEFAULT;
  }
  function limitHeight(px: number) {
    const total = consolePanel?.parentElement?.getBoundingClientRect().height ?? window.innerHeight;
    return Math.round(Math.max(HEIGHT_MIN, Math.min(total - 160, px)));
  }
  function putHeight(px: number) {
    consoleHeight = limitHeight(px);
    try {
      localStorage.setItem('consola-alto', String(consoleHeight));
    } catch {
    }
  }
  function startHeight(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragConsole = true;
  }
  function moveHeight(e: PointerEvent) {
    if (!dragConsole || !consolePanel) return;
    consoleHeight = limitHeight(consolePanel.getBoundingClientRect().bottom - e.clientY);
  }
  function unpinHeight() {
    if (!dragConsole) return;
    dragConsole = false;
    putHeight(consoleHeight);
  }
  function keyHeight(e: KeyboardEvent) {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    e.preventDefault();
    putHeight(consoleHeight + (e.key === 'ArrowUp' ? 24 : -24));
  }
  async function useFolder(e: SubmitEvent) {
    e.preventDefault();
    try {
      await api.chooseGame(gamePath);
      await app.loadEnvironment();
    } catch (err) {
      app.errorEnvironment = message(err);
    }
  }

  function shortcuts(e: KeyboardEvent) {
    const inField = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'z' && !inField && app.state?.undo) {
      e.preventDefault();
      app.undo();
    }
  }

  onMount(async () => {
    await app.start();
    if (!inTauri) {
      const q = new URLSearchParams(location.search);
      const theme = q.get('theme');
      if (theme === 'light' || theme === 'dark') app.changeTheme(theme);
      if (q.get('scale')) app.changeScale(Number(q.get('scale')));
      if (q.get('sel')) app.selection = q.get('sel');
      const order = q.get('order');
      if (order) await app.proposeOrder();
      const tab = q.get('tab');
      if (tab === 'order' || tab === 'conflicts' || tab === 'maps' || tab === 'export' || tab === 'sandbox' || tab === 'console' || tab === 'settings') {
        app.tab = tab;
        app.screen = 'profile';
      }
      if (q.get('sel') || order) app.screen = 'profile';
      if (q.get('panel') === '1') app.consolePanel = true;
      if (q.get('height')) consoleHeight = Number(q.get('height'));
      if (q.get('playing') === '1' && app.profileId) {
        app.session = { profile: app.profileId, seconds: 95, requested: 21, loaded: q.get('loading') ? 9 : 21, finished: false, code: null, recent_lines: [], integrity: null, integrity_skipped: false, diagnosis: null };
      }
      const palette = q.get('palette');
      if (palette && (PALETTES as readonly string[]).includes(palette)) app.changePalette(palette as Palette);
      if (q.get('disclaimer') === '1') app.disclaimerOpen = true;
      if (q.get('zb') === 'ask') app.zbAskUninstall = 'estable';
      if (q.get('zb') === 'downloading') app.zbPending = { channel: 'beta', since: Date.now(), from: null };
      const screen = q.get('screen');
      if (screen === 'home' || screen === 'help' || screen === 'profile' || screen === 'library' || screen === 'workshop' || screen === 'zb' || screen === 'install' || screen === 'settings') app.screen = screen;
    }
  });

  const widths = [72, 58, 81, 64, 77, 52, 69, 83, 60, 74, 56, 79];

  const withPanel = $derived(app.screen === 'profile' && !!app.view && !app.loadingView && app.view.rows.length > 0 && app.tab !== 'settings' && app.tab !== 'maps' && app.tab !== 'export' && app.tab !== 'sandbox' && app.tab !== 'console');
  const showConsolePanel = $derived(app.consolePanel && !(app.screen === 'profile' && app.tab === 'console') && (!!app.profileId || !!app.session));
  const consoleErrors = $derived(app.errors.list.filter((e) => e.severity === 'error').length);

  const STRETCH: Record<ViewProgress['stage'], [number, number]> = {
    mods: [0, 5],
    game: [5, 10],
    files: [10, 45],
    scripts: [45, 60],
    lua: [60, 75],
    conflicts: [75, 100],
  };
  function overall(p: ViewProgress): number {
    const [a, b] = STRETCH[p.stage];
    return p.total ? a + ((b - a) * p.done) / p.total : a;
  }
</script>

{#snippet rowsSkeleton()}
  <div class="table-schema" aria-hidden="true">
    <div class="enc-schema"></div>
    {#each widths as w, i (i)}
      <div class="row-schema">
        <span class="box"></span><span class="n"></span><span class="name" style:width="{w * 3}px"></span><span class="tp"></span><span class="show"></span>
      </div>
    {/each}
  </div>
{/snippet}

{#snippet failed(_: unknown, reset: () => void)}
  <ScreenError {reset} />
{/snippet}

<svelte:window onkeydown={shortcuts} />

{#if !inTauri}
  <div class="demo" role="note">{t('app.demo')}</div>
{/if}

<div class="window" class:with-demo={!inTauri} class:with-panel={withPanel}>
  {#if app.loadingEnvironment && !app.environment}
    <div class="launch" aria-busy="true">
      <div class="side-skeleton">
        <div class="mark-schema"></div>
        {#each [70, 52, 61] as w, i (i)}<div class="profile-schema" style:width="{w}%"></div>{/each}
      </div>
      <div class="body-skeleton">
        <p>{t('launch.searching')}</p>
        {@render rowsSkeleton()}
      </div>
    </div>
  {:else if app.errorEnvironment}
    <main class="only">
      <EmptyState title={t('error.game.title')} text={t('error.game.text')} error>
        <button class="btn primary big" onclick={() => app.loadEnvironment()}>{t('error.retry')}</button>
        <code class="detail-error">{app.errorEnvironment}</code>
        <form class="choose" onsubmit={useFolder}>
          <label for="game-path">{t('error.choose')}</label>
          <div class="row-choose">
            <input id="game-path" class="field" bind:value={gamePath} placeholder={t('error.example')} />
            <button class="btn" type="submit" disabled={!gamePath.trim()}>{t('error.use')}</button>
          </div>
        </form>
      </EmptyState>
    </main>
  {:else}
    <Sidebar />

    <main class="main">
      <svelte:boundary onerror={logUiError} {failed}>
        {#if app.disclaimerOpen}
          <Disclaimer />
        {:else if app.screen === 'home'}
          <Home />
        {:else if app.screen === 'help'}
          <Help />
        {:else if app.screen === 'profile'}
          {#if app.profiles.length === 0}
            <EmptyState title={t('empty.profiles.title')} text={t('empty.profiles.text')}>
              <button class="btn primary big" onclick={() => app.createProfile(t('empty.profiles.imported_name'), true)}>
                {t('empty.profiles.import')}
              </button>
              <button class="btn big" onclick={() => app.createProfile(t('empty.profiles.name_empty'), false)}>
                {t('empty.profiles.empty')}
              </button>
            </EmptyState>
          {:else if app.errorView}
            <EmptyState title={t('error.profile')} text={app.errorView} error>
              <button class="btn" onclick={() => app.loadView()}>{t('error.profile.retry')}</button>
            </EmptyState>
          {:else if !app.view || app.loadingView}
            <div class="loading" aria-busy="true">
              <div class="reading" role="status" aria-live="polite">
                <p>{t('profile.loading')}</p>
                {#if app.viewProgress}
                  {@const p = app.viewProgress}
                  <div class="progress" role="progressbar" aria-label={t('profile.loading')} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(overall(p))}>
                    <span style:width="{overall(p)}%"></span>
                  </div>
                  <p class="step">{t(`profile.stage.${p.stage}`)}{#if p.total}{' · '}<span class="num">{t(p.stage === 'conflicts' ? 'profile.stage_files' : 'profile.stage_count', { h: p.done, n: p.total })}</span>{/if}</p>
                {/if}
              </div>
              {@render rowsSkeleton()}
            </div>
          {:else}
            <ProfileHeader />
            <svelte:boundary onerror={logUiError} {failed}>
              {#if app.tab === 'order' && app.gameList && !(app.session && !app.session.finished)}<GameListBanner />{/if}
              {#if app.tab === 'order' && app.saveGaps.length > 0 && !(app.session && !app.session.finished)}<SavesBanner />{/if}
              {#if app.tab === 'order' && app.view.counts.blocks > 0}<ConflictBanner />{/if}
              {#if app.tab === 'order'}
                {#if app.view.rows.length === 0}
                  <EmptyState title={t('empty.profile.title')} text={t('empty.profile.text')}>
                    <button class="btn" onclick={() => (app.screen = 'library')}>{t('empty.profile.action')}</button>
                  </EmptyState>
                {:else}
                  {#if app.proposal}<OrderProposal />{/if}
                  <OrderTable />
                {/if}
              {:else if app.tab === 'conflicts'}
                <ConflictList />
              {:else if app.tab === 'maps'}
                {#key app.view.id}<MapsTab />{/key}
              {:else if app.tab === 'export'}
                <ProfileExport />
              {:else if app.tab === 'sandbox'}
                {#key app.view.id}<SandboxEditor />{/key}
              {:else if app.tab === 'console'}
                <Console />
              {:else}
                {#key app.view.id}<ProfileSettings />{/key}
              {/if}
            </svelte:boundary>
            {#if app.session?.finished && app.session.code !== 0 && app.session.recent_lines.length}
              <details class="log" open>
                <summary class="severity-blocks"><CircleX size={13} strokeWidth={2} class="inline-icon" />{t('session.end_error', { c: app.session.code ?? '?' })} · {t('session.log')}</summary>
                <pre class="mono">{app.session.recent_lines.join('\n')}</pre>
              </details>
            {/if}
          {/if}
        {:else if app.screen === 'library'}
          <Library />
        {:else if app.screen === 'workshop'}
          <Workshop />
        {:else if app.screen === 'zb'}
          <ZbView />
        {:else if app.screen === 'install'}
          <InstallView />
        {:else}
          <AppSettings />
        {/if}
      </svelte:boundary>

      {#if showConsolePanel}
        <div class="console-panel" style:height="{consoleHeight}px" bind:this={consolePanel}>
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <div
            class="handle-console"
            class:dragging={dragConsole}
            role="separator"
            aria-orientation="horizontal"
            aria-label={t('console.height')}
            aria-valuenow={consoleHeight}
            aria-valuemin={HEIGHT_MIN}
            tabindex="0"
            title={t('console.height_help')}
            onpointerdown={startHeight}
            onpointermove={moveHeight}
            onpointerup={unpinHeight}
            onpointercancel={unpinHeight}
            ondblclick={() => putHeight(HEIGHT_DEFAULT)}
            onkeydown={keyHeight}
          ></div>
          <Console compact close={() => app.toggleConsolePanel()} />
        </div>
      {/if}

      <footer class="state" role="status" aria-live="polite">
        <button class="btn subtle toggle" class:enabled={app.consolePanel} aria-pressed={app.consolePanel} onclick={() => app.toggleConsolePanel()} title={t('console.panel_help')}>
          <Terminal size={13} strokeWidth={1.75} />{t('console.console')}
          {#if consoleErrors > 0}<span class="count-err num">{consoleErrors}</span>{/if}
        </button>
        {#if app.state}
          <span class:severity-blocks={app.state.kind === 'error'} class="msg">
            {#if app.state.kind === 'error'}<CircleX size={13} strokeWidth={2} />{:else}<Check size={13} strokeWidth={2} />{/if}
            {app.state.text}
          </span>
          {#if app.state.action}
            <button class="btn subtle undo" onclick={() => app.acceptAction()} disabled={app.taken}>{app.state.action.text}</button>
          {/if}
          {#if app.state.undo}
            <button class="btn subtle undo" onclick={() => app.undo()} disabled={app.taken}>{t('common.undo')}</button>
          {/if}
        {/if}
        {#if app.environment}
          <span class="right mono">{app.environment.game.version_text}</span>
        {/if}
      </footer>
    </main>

    {#if withPanel}
      <svelte:boundary onerror={logUiError} {failed}>
        <DetailPanel />
      </svelte:boundary>
    {/if}
  {/if}
</div>

<style>
  .demo {
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: var(--t-xs);
    font-weight: 600;
    border-bottom: 1px solid var(--line);
  }
  .window {
    display: grid;
    grid-template-columns: 232px minmax(0, 1fr);
    height: 100%;
  }
  .window.with-demo {
    height: calc(100% - 24px);
  }
  .window.with-panel {
    grid-template-columns: 232px minmax(0, 1fr) 340px;
  }
  @media (max-width: 1240px) {
    .window.with-panel {
      grid-template-columns: 232px minmax(0, 1fr) 300px;
    }
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--ground);
  }
  .only {
    grid-column: 1 / -1;
    background: var(--ground);
  }
  .detail-error {
    flex-basis: 100%;
    margin-top: 6px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .console-panel {
    position: relative;
    flex: none;
    display: flex;
    max-height: calc(100% - 160px);
    border-top: 1px solid var(--line-strong);
  }
  .handle-console {
    position: absolute;
    top: -4px;
    left: 0;
    right: 0;
    z-index: 3;
    height: 7px;
    cursor: row-resize;
    touch-action: none;
  }
  .handle-console::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 2px;
    height: 3px;
  }
  .handle-console:hover::after,
  .handle-console:focus-visible::after,
  .handle-console.dragging::after {
    background: var(--accent);
  }
  .handle-console:focus-visible {
    outline: none;
  }
  .toggle {
    height: 20px;
    margin-left: -8px;
    padding: 0 8px;
    font-size: var(--t-xs);
  }
  .toggle.enabled {
    background: var(--surface-3);
  }
  .count-err {
    padding: 0 5px;
    border-radius: var(--radio-pill);
    background: var(--danger-soft);
    color: var(--danger);
    font-weight: 600;
    line-height: 15px;
  }
  .undo {
    height: 20px;
    padding: 0 8px;
    font-size: var(--t-xs);
  }
  .state {
    margin-top: auto;
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 26px;
    padding: 0 12px 0 16px;
    border-top: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .msg {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .right {
    margin-left: auto;
    color: var(--ink-3);
  }
  .log {
    border-top: 1px solid var(--line);
    background: var(--surface);
    max-height: 30%;
    overflow: auto;
  }
  .log summary {
    padding: 8px 16px;
    cursor: pointer;
    font-size: var(--t-s);
    font-weight: 500;
  }
  .log pre {
    margin: 0;
    padding: 0 16px 12px;
    font-size: var(--t-xs);
    white-space: pre-wrap;
    color: var(--ink-2);
  }
  .launch {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100%;
  }
  .side-skeleton {
    background: var(--shell);
  }
  .body-skeleton,
  .loading {
    padding: 22px 16px;
    display: grid;
    align-content: start;
    gap: 12px;
  }
  .reading {
    display: grid;
    gap: 8px;
    max-width: 520px;
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
  .loading p.step {
    font-size: var(--t-xs);
    color: var(--ink-3);
  }
  .body-skeleton p,
  .loading p {
    margin: 0 0 6px;
    color: var(--ink-2);
  }
  .choose {
    flex-basis: 100%;
    display: grid;
    gap: 6px;
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .row-choose {
    display: flex;
    gap: 6px;
  }
  .row-choose .field {
    flex: 1;
  }
  .side-skeleton {
    display: grid;
    align-content: start;
    gap: 12px;
    padding: 14px;
  }
  .mark-schema,
  .profile-schema {
    height: 12px;
    border-radius: var(--radio);
    background: var(--shell-3);
  }
  .mark-schema {
    width: 60%;
    margin-bottom: 18px;
  }
  .table-schema {
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .enc-schema {
    height: 28px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
  }
  .row-schema {
    display: grid;
    grid-template-columns: 58px 44px 1fr 88px 92px;
    align-items: center;
    height: 30px;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }
  .row-schema span {
    height: 9px;
    border-radius: var(--radio);
    background: var(--surface-3);
  }
  .row-schema .box {
    width: 14px;
    height: 14px;
    justify-self: end;
    margin-right: 14px;
  }
  .row-schema .n {
    width: 16px;
  }
  .row-schema .name {
    max-width: 90%;
  }
  .row-schema .tp {
    width: 34px;
  }
  .row-schema .show {
    width: 40px;
  }
</style>
