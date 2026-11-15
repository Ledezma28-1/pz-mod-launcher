<script lang="ts">
  import { arrows, overflow } from '../keys';
  import Play from '@lucide/svelte/icons/play';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Check from '@lucide/svelte/icons/check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import StopButton from './StopButton.svelte';
  import { message } from '../api';
  import { app, type Tab } from '../state.svelte';
  import { t } from '../i18n.svelte';

  const v = $derived(app.view!);

  async function sort() {
    try {
      await app.proposeOrder();
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  const tabs: { id: Tab; key: Parameters<typeof t>[0] }[] = [
    { id: 'order', key: 'profile.tab.order' },
    { id: 'conflicts', key: 'profile.tab.conflicts' },
    { id: 'export', key: 'profile.tab.export' },
    { id: 'sandbox', key: 'profile.tab.sandbox' },
    { id: 'console', key: 'profile.tab.console' },
    { id: 'settings', key: 'profile.tab.settings' },
  ];

  const consoleErrors = $derived(app.errors.profile === v.id ? app.errors.list.filter((e) => e.severity === 'error').length : 0);

  const playing = $derived(app.starting || (app.session && !app.session.finished && app.session.profile === v.id));
</script>

<header class="header">
  <div class="row1">
    <div class="title">
      <h1>{v.name}</h1>
      <div class="summary num">
        <span>{t('profile.active', { n: v.counts.active })} <span class="dim">{t('profile.of', { n: v.rows.length })}</span></span>
        {#if v.counts.blocks > 0}
          <button class="state severity-blocks" onclick={() => (app.tab = 'conflicts')}>
            <CircleX size={14} strokeWidth={2} />{t('profile.block', { n: v.counts.blocks })}
          </button>
        {/if}
        {#if v.counts.warning > 0}
          <button class="state severity-warning" onclick={() => (app.tab = 'conflicts')}>
            <TriangleAlert size={14} strokeWidth={2} />{t('profile.warnings', { n: v.counts.warning })}
          </button>
        {/if}
        {#if v.counts.blocks === 0 && v.counts.warning === 0}
          <span class="state ok"><Check size={14} strokeWidth={2} />{t('profile.all_fine')}</span>
        {/if}
      </div>
    </div>

    <div class="actions">
      <button class="btn" onclick={sort} disabled={app.taken || v.rows.length === 0}>
        <ArrowUpDown size={15} strokeWidth={1.75} />{t('profile.sort')}
      </button>
      {#if playing}
        <div class="session" role="status">
          <LoaderCircle size={15} strokeWidth={2} class="rotates" />
          <span>
            {#if app.session && app.session.loaded > 0}
              {t('session.loading', { c: app.session.loaded, p: app.session.requested })}
            {:else if app.session}
              {t('session.running')}
            {:else}
              {t('session.starting')}
            {/if}
          </span>
        </div>
        <StopButton />
      {:else}
        <button class="btn primary big" onclick={() => app.play()}>
          <Play size={15} strokeWidth={2} fill="currentColor" />{t('profile.play')}
        </button>
      {/if}
    </div>
  </div>

  <div class="tabs" role="tablist" aria-label={t('profile.tabs')} use:arrows={'tab'} use:overflow>
    {#each tabs as p (p.id)}
      <button role="tab" class="tab" aria-selected={app.tab === p.id} tabindex={app.tab === p.id ? 0 : -1} onclick={() => (app.tab = p.id)}>
        {t(p.key)}
        {#if p.id === 'console' && consoleErrors > 0}
          <span class="counter num red" title={t('console.n_errors', { n: consoleErrors })}>{consoleErrors}</span>
        {/if}
        {#if p.id === 'conflicts' && v.counts.blocks + v.counts.warning > 0}
          <span class="counter num" class:red={v.counts.blocks > 0} title={t('profile.counter', { b: v.counts.blocks, w: v.counts.warning, i: v.counts.info })}>{v.counts.blocks + v.counts.warning}<span class="sr-only"> ({t('profile.counter', { b: v.counts.blocks, w: v.counts.warning, i: v.counts.info })})</span></span>
        {/if}
      </button>
    {/each}
  </div>
</header>

<style>
  .header {
    background: var(--shell);
    color: var(--shell-ink);
    border-bottom: 1px solid var(--shell-line);
    padding: 12px 16px 0;
    --ink: var(--shell-ink);
    --ink-2: var(--shell-ink-2);
    --ink-3: var(--shell-ink-2);
    --surface: var(--shell-2);
    --surface-2: var(--shell-3);
    --surface-3: var(--shell-3);
    --line-strong: var(--shell-line-strong);
    --danger: var(--shell-danger);
    --warn: var(--shell-warn);
    --ok: var(--shell-ok);
    --accent: var(--shell-accent);
  }
  .row1 {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  h1 {
    font-size: var(--t-xl);
    line-height: 1.25;
  }
  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 14px;
    margin-top: 3px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .dim {
    color: var(--ink-3);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--t-s);
    font-weight: 500;
  }
  button.state {
    cursor: pointer;
  }
  button.state:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .state.ok {
    color: var(--ok);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .session {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 4px 0 12px;
    border: 1px solid var(--accent);
    border-radius: var(--radio);
    color: var(--accent);
    font-size: var(--t-s);
    font-weight: 500;
  }
  .session :global(.rotates) {
    animation: rotate 900ms linear infinite;
  }
  @keyframes rotate {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .session :global(.rotates) {
      animation: none;
    }
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin-top: 10px;
    overflow-x: auto;
    scrollbar-width: thin;
  }
  .tab {
    flex-shrink: 0;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--ink-2);
    font-size: var(--t-s);
    font-weight: 500;
    cursor: pointer;
  }
  .tab:hover {
    color: var(--ink);
  }
  .tabs:global(.more) {
    mask-image: linear-gradient(to right, #000 calc(100% - 32px), transparent);
  }
  @media (max-width: 1240px) {
    .tab {
      padding: 0 8px;
    }
  }
  .tab[aria-selected='true'] {
    color: var(--ink);
    border-bottom-color: var(--accent);
  }
  .counter {
    padding: 0 6px;
    border-radius: var(--radio-pill);
    background: var(--surface-3);
    color: var(--ink-2);
    font-size: var(--t-xs);
    line-height: 17px;
  }
  .counter.red {
    background: color-mix(in srgb, var(--danger) 22%, transparent);
    color: var(--danger);
  }
</style>
