<script lang="ts">
  import Confirm from './Confirm.svelte';
  import ArchiveRestore from '@lucide/svelte/icons/archive-restore';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t, i18n } from '../i18n.svelte';
  import type { Backup } from '../types';

  const e = $derived(app.environment!);
  let backups = $state<Backup[] | null>(null);

  async function load() {
    try {
      backups = await api.backups();
    } catch (err) {
      app.notify(message(err), 'error');
      backups = [];
    }
  }
  $effect(() => {
    load();
  });

  async function restore(c: Backup) {
    try {
      const n = await api.restore(c.name);
      app.notify(t('install.restored', { n }));
      await app.loadEnvironment();
    } catch (err) {
      app.notify(message(err), 'error');
    }
  }

  const date = (s: number) =>
    new Date(s * 1000).toLocaleString(i18n.language === 'en' ? 'en-GB' : 'es-ES', { dateStyle: 'medium', timeStyle: 'short' });
</script>

<section class="screen">
  <header class="head">
    <h1>{t('install.title')}</h1>
  </header>
  <div class="content">
    <dl class="data">
      <dt>{t('install.version')}</dt>
      <dd><span class="mono">{e.game.version_text}</span>{#if e.game.build}<span class="dim"> · build {e.game.build}{e.game.branch ? ` · ${e.game.branch}` : ''}</span>{/if}</dd>
      <dt>{t('install.folder')}</dt>
      <dd class="mono">{e.game.folder}</dd>
      {#if e.game.java}<dt>{t('install.java')}</dt><dd class="mono">{e.game.java}</dd>{/if}
      <dt>{t('install.mods')}</dt>
      <dd>
        {#if e.modifications.length === 0}
          <span class="dim">{t('install.none')}</span>
        {:else}
          <ul class="mods">{#each e.modifications as m (m)}<li class="mono">{m}</li>{/each}</ul>
        {/if}
      </dd>
    </dl>

    <h2>{t('install.backups')}</h2>
    {#if backups === null}
      <p class="dim">…</p>
    {:else if backups.length === 0}
      <p class="dim">{t('install.copies_empty')}</p>
    {:else}
      <ul class="copies">
        {#each backups as c (c.name)}
          <li>
            <ArchiveRestore size={15} strokeWidth={1.75} />
            <span class="when num">{date(c.created)}</span>
            <span class="reason">{c.reason}</span>
            <span class="dim num">{t('install.files', { n: c.files })}</span>
            <Confirm text={t('install.restore_confirm')} yes={t('install.restore')} onsi={() => restore(c)}>
              {#snippet button(request)}
                <button class="btn" onclick={request}>{t('install.restore')}</button>
              {/snippet}
            </Confirm>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .screen {
    height: 100%;
    overflow: auto;
  }
  .head {
    padding: 14px 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  h1 {
    font-size: var(--t-xl);
  }
  .content {
    padding: 16px;
    max-width: 900px;
  }
  .data {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 10px 16px;
    margin: 0;
  }
  dt {
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .dim {
    color: var(--ink-3);
  }
  .mods {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
    font-size: var(--t-s);
  }
  h2 {
    margin: 26px 0 10px;
    font-size: var(--t-l);
  }
  .copies {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface);
  }
  .copies li {
    display: grid;
    grid-template-columns: 20px 170px 1fr auto auto;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    font-size: var(--t-s);
  }
  .copies li:last-child {
    border-bottom: 0;
  }
  .copies :global(svg) {
    color: var(--ink-3);
  }
</style>
