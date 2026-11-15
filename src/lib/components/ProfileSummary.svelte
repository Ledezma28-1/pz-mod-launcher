<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import { ago } from '../time';
  import type { Diagnosis } from '../types';

  const v = $derived(app.view!);
  const on = $derived(v.rows.filter((r) => r.enabled));
  const of = (k: string) => on.filter((r) => r.kind === k).length;
  const java = $derived(of('java'));
  const local = $derived(on.filter((r) => r.source === 'local').length);
  const missing = $derived(on.filter((r) => !r.found).length);
  const zb = $derived(app.environment?.zb.in_game ?? null);

  let last = $state<Diagnosis | null>(null);
  let read = $state(false);
  $effect(() => {
    const id = v.id;
    void app.session?.finished;
    let alive = true;
    read = false;
    api
      .sessionLastDiagnosis(id)
      .then((d) => alive && (last = d))
      .catch(() => alive && (last = null))
      .finally(() => alive && (read = true));
    return () => (alive = false);
  });
  const fine = $derived(!!last && (last.end === 'normal' || last.end === 'stopped_by_user') && !last.not_loaded.length);
</script>

<div class="summary">
  <div class="head">
    <h2>{v.name}</h2>
    <p class="sub">{t('summary.active', { a: on.length, n: v.rows.length })}</p>
  </div>

  <section>
    <h3>{t('profile.tab.conflicts')}</h3>
    {#if v.counts.blocks + v.counts.warning + v.counts.info === 0}
      <p class="line severity-ok"><Check size={14} strokeWidth={2} />{t('summary.clean')}</p>
    {:else}
      <ul class="lines">
        {#if v.counts.blocks}
          <li><button class="link severity-blocks" onclick={() => (app.tab = 'conflicts')}><CircleX size={14} strokeWidth={2} />{t('profile.block', { n: v.counts.blocks })}</button></li>
        {/if}
        {#if v.counts.warning}
          <li><button class="link severity-warning" onclick={() => (app.tab = 'conflicts')}><TriangleAlert size={14} strokeWidth={2} />{t('summary.warnings', { n: v.counts.warning })}</button></li>
        {/if}
        {#if v.counts.info}
          <li><button class="link severity-info" onclick={() => (app.tab = 'conflicts')}><Info size={14} strokeWidth={2} />{t('summary.infos', { n: v.counts.info })}</button></li>
        {/if}
      </ul>
    {/if}
  </section>

  <section>
    <h3>{t('summary.mods')}</h3>
    <dl>
      <dt>{t('summary.java')}</dt><dd class="num">{java}</dd>
      <dt>{t('summary.maps')}</dt><dd class="num">{of('map')}</dd>
      <dt>{t('summary.vehicles')}</dt><dd class="num">{of('vehicle')}</dd>
      <dt>{t('summary.local')}</dt><dd class="num">{local}</dd>
      {#if missing}<dt>{t('summary.missing')}</dt><dd class="num severity-blocks">{missing}</dd>{/if}
      {#if java}
        <dt>ZombieBuddy</dt>
        <dd class="mono" class:severity-blocks={!zb}>{zb ?? t('summary.zb_missing')}</dd>
      {/if}
    </dl>
  </section>

  <section>
    <h3>{t('summary.last')}</h3>
    {#if !read}
      <p class="dim">{t('detail.loading')}</p>
    {:else if !last}
      <p class="dim">{t('summary.never')}</p>
    {:else}
      <p class="line" class:severity-ok={fine} class:severity-blocks={!fine}>
        {#if fine}<Check size={14} strokeWidth={2} />{:else}<CircleX size={14} strokeWidth={2} />{/if}
        {t(`summary.end.${last.end}`, { c: last.code ?? '?' })}
        <span class="dim">· {ago(last.when)}</span>
      </p>
      {#if !fine}
        <button class="link" onclick={() => (app.tab = 'console')}>{t('summary.see_console')}</button>
      {/if}
    {/if}
  </section>

  <p class="hint">{t('summary.hint')}</p>
</div>

<style>
  .head {
    padding: 16px 18px 14px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    font-size: var(--t-l);
    line-height: 1.3;
  }
  .sub {
    margin: 2px 0 0;
    color: var(--ink-3);
    font-size: var(--t-s);
  }
  section {
    padding: 12px 18px;
    border-bottom: 1px solid var(--line);
    font-size: var(--t-s);
  }
  h3 {
    margin-bottom: 8px;
    font-size: var(--t-s);
    color: var(--ink-2);
  }
  .lines {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .line,
  .link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }
  .link {
    padding: 2px 0;
    border: 0;
    background: none;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  section > .link {
    margin-top: 6px;
    color: var(--accent);
  }
  .severity-ok {
    color: var(--ok);
  }
  dl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 4px 10px;
    margin: 0;
  }
  dt {
    color: var(--ink-3);
  }
  dd {
    margin: 0;
    text-align: right;
  }
  .dim {
    color: var(--ink-3);
  }
  p.dim {
    margin: 0;
  }
  .hint {
    margin: 0;
    padding: 14px 18px;
    color: var(--ink-3);
    font-size: var(--t-s);
  }
</style>
