<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Copy from '@lucide/svelte/icons/copy';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Settings from '@lucide/svelte/icons/settings';
  import Search from '@lucide/svelte/icons/search';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { Diagnosis, Suspect } from '../types';

  let { profile }: { profile: string } = $props();

  let diagnosis = $state<Diagnosis | null>(null);
  let open = $state(true);

  const running = $derived(!!app.session && !app.session.finished && app.session.profile === profile);
  const ended = $derived(app.session?.finished && app.session.profile === profile ? app.session.diagnosis : undefined);

  $effect(() => {
    const id = profile;
    if (ended !== undefined) {
      diagnosis = ended;
      open = true;
      return;
    }
    let alive = true;
    api
      .sessionLastDiagnosis(id)
      .then((d) => alive && (diagnosis = d))
      .catch(() => alive && (diagnosis = null));
    return () => (alive = false);
  });

  let flaky = $state(false);
  const changedOthers = $derived(
    (diagnosis?.changed ?? []).filter((id) => !diagnosis?.suspects.some((s) => s.id === id && s.reason.kind === 'changed')),
  );

  const worth = $derived(
    !!diagnosis && ((diagnosis.end !== 'normal' && diagnosis.end !== 'stopped_by_user') || diagnosis.not_loaded.length > 0),
  );
  const driver = $derived(diagnosis?.crash?.origin === 'graphics_driver');
  const version = (v: string | null) => v ?? t('diag.version_none');
  const serious = $derived(!!diagnosis && (diagnosis.end === 'java_crash' || diagnosis.end === 'out_of_memory'));

  const norm = (id: string) => id.replace(/^\\/, '').trim();
  function row(id: string) {
    return app.view?.id === profile ? app.view.rows.find((r) => norm(r.id) === norm(id)) : undefined;
  }

  function reasonText(s: Suspect): string {
    if (s.reason.kind === 'crash_frame') return t('diag.reason.crash_frame');
    if (s.reason.kind === 'patches_crash_class') return t('diag.reason.patches_crash_class');
    if (s.reason.kind === 'lua_errors') return t('diag.reason.lua_errors', { n: s.reason.data });
    if (s.reason.kind === 'changed') return t('diag.reason.changed');
    return t('diag.reason.not_loaded');
  }

  function whyText(reason: string | null): string {
    const key = `diag.why.${reason}`;
    const known = ['ModNotFound', 'MissingDependency', 'WontLoad', 'GameVersion', 'OnlyB41', 'NoValidFolder', 'Incompatible', 'DuplicatedId', 'Cycle'];
    return reason && known.includes(reason) ? t(key as Parameters<typeof t>[0]) : t('diag.why.other');
  }

  async function disable(id: string) {
    await app.runView(() => api.setEnabled(profile, id, false), t('diag.disabled', { m: row(id)?.name ?? id }));
  }

  async function copyReport() {
    try {
      await navigator.clipboard.writeText(await api.sessionReport(profile));
      app.notify(t('diag.copied'));
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }
</script>

{#if diagnosis && worth && !running}
  <section class="what" class:serious aria-label={t('diag.title')}>
    <button class="head" aria-expanded={open} onclick={() => (open = !open)}>
      {#if open}<ChevronDown size={14} strokeWidth={2} />{:else}<ChevronRight size={14} strokeWidth={2} />{/if}
      {#if serious}<CircleX size={15} strokeWidth={2} class="ico bad" />{:else}<TriangleAlert size={15} strokeWidth={2} class="ico warn" />{/if}
      <span class="verdict">{t(`diag.end.${diagnosis.end}`, { c: diagnosis.code ?? '?' })}</span>
      <span class="title">{t('diag.title')}</span>
    </button>

    {#if open}
      <div class="body">
        {#if diagnosis.crash}
          <p class="crash mono">{t('diag.crash', { k: diagnosis.crash.kind })}</p>
          {#if diagnosis.crash.origin === 'graphics_driver' || diagnosis.crash.origin === 'zb_native'}
            <p class="hint">{t(`diag.origin.${diagnosis.crash.origin}`, { m: diagnosis.crash.module ?? '' })}</p>
          {/if}
        {/if}
        {#if diagnosis.game_changed}
          <p class="changed">{t('diag.game_changed', { a: version(diagnosis.game_changed[0]), b: version(diagnosis.game_changed[1]) })}</p>
        {/if}
        {#if diagnosis.zb_changed}
          <p class="changed">{t('diag.zb_changed', { a: version(diagnosis.zb_changed[0]), b: version(diagnosis.zb_changed[1]) })}</p>
        {/if}
        {#if diagnosis.end === 'out_of_memory'}
          <p class="hint">
            {t('diag.memory_help')}
            <button class="btn subtle" onclick={() => (app.tab = 'settings')}><Settings size={14} strokeWidth={1.75} />{t('diag.memory')}</button>
          </p>
        {/if}

        {#if diagnosis.suspects.length}
          <h3>{t('diag.suspects')}</h3>
          <ul class="list">
            {#each diagnosis.suspects as s (s.id)}
              {@const r = row(s.id)}
              <li>
                <span class="name">{r?.name ?? s.id}</span>
                <span class="why">{reasonText(s)}{#if s.reason.kind === 'not_loaded'}{' · '}{whyText(diagnosis.not_loaded.find((n) => n.id === s.id)?.reason ?? null)}{/if}</span>
                <span class="actions">
                  {#if !r || r.enabled}
                    <button class="btn subtle" onclick={() => disable(s.id)}>{t('diag.disable')}</button>
                  {/if}
                  {#if r?.workshop_id}
                    <button class="btn subtle" onclick={() => api.openWorkshop(r.workshop_id!).catch((e) => app.notify(message(e), 'error'))}>
                      <ExternalLink size={14} strokeWidth={1.75} />{t('diag.workshop')}
                    </button>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="none"><Info size={14} strokeWidth={2} class="ico" />{t('diag.none')}</p>
        {/if}
        {#if changedOthers.length}
          <p class="changed">{t('diag.changed', { m: changedOthers.map((id) => row(id)?.name ?? id).join(', ') })}</p>
        {/if}
        {#if diagnosis.prediction && !diagnosis.prediction.order_ok}
          <p class="changed">{t('diag.prediction_order', { a: diagnosis.prediction.not_loaded.length, b: diagnosis.prediction.unexpected.length })}</p>
        {/if}
        {#if diagnosis.prediction && diagnosis.prediction.overrides_missed + diagnosis.prediction.overrides_extra > 0}
          <p class="changed">{t('diag.prediction_files', { n: diagnosis.prediction.overrides_missed + diagnosis.prediction.overrides_extra })}</p>
        {/if}

        <div class="foot">
          {#if !app.bisect}
            <button class="btn" class:subtle={driver} title={t('diag.find_culprit_help')} onclick={() => app.startBisect(profile, flaky)}>
              <Search size={14} strokeWidth={1.75} />{t('diag.find_culprit')}
            </button>
            <label class="flaky" title={t('diag.flaky_help')}><input type="checkbox" bind:checked={flaky} />{t('diag.flaky')}</label>
          {/if}
          <button class="btn" onclick={copyReport}><Copy size={14} strokeWidth={1.75} />{t('diag.copy')}</button>
        </div>
      </div>
    {/if}
  </section>
{/if}

<style>
  .what {
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--warn) 7%, var(--surface));
  }
  .what.serious {
    background: color-mix(in srgb, var(--danger) 7%, var(--surface));
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 9px 14px;
    border: 0;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .verdict {
    font-weight: 600;
  }
  .title {
    margin-left: auto;
    color: var(--ink-3);
    font-size: var(--t-s);
  }
  .head :global(.bad) {
    color: var(--danger);
  }
  .head :global(.warn) {
    color: var(--warn);
  }
  .body {
    padding: 0 14px 12px 36px;
    font-size: var(--t-s);
  }
  .crash {
    margin: 0 0 8px;
    color: var(--ink-2);
    overflow-wrap: anywhere;
  }
  .hint {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
  }
  h3 {
    margin: 4px 0 6px;
    font-size: var(--t-s);
    color: var(--ink-2);
  }
  .list {
    list-style: none;
    margin: 0 -6px;
    padding: 0;
    display: grid;
  }
  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    min-height: 30px;
    padding: 2px 6px;
    border-radius: var(--radio);
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }
  .list li:hover,
  .list li:focus-within {
    background: color-mix(in srgb, var(--surface-2) 70%, transparent);
  }
  .name {
    font-weight: 600;
  }
  .why {
    color: var(--ink-3);
  }
  .actions {
    display: flex;
    gap: 2px;
    margin-left: auto;
  }
  .none {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--ink-3);
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
  }
  .flaky {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 6px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .changed {
    margin: 8px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
</style>
