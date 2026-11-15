<script lang="ts">
  import ListRestart from '@lucide/svelte/icons/list-restart';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';

  const c = $derived(app.gameList!);
  const id = $derived(app.profileId!);

  const parts = $derived(
    [
      c.enabled.length && t('gamelist.enabled', { n: c.enabled.length }),
      c.disabled.length && t('gamelist.disabled', { n: c.disabled.length }),
      c.reordered && t('gamelist.reordered'),
      c.maps && t('gamelist.maps'),
    ].filter(Boolean),
  );

  async function take() {
    await app.run(() => api.takeGameList(id), t('gamelist.taken'));
    await app.loadGameList();
  }

  async function discard() {
    await app.run(() => api.discardGameList(id), t('gamelist.discarded'));
    await app.loadGameList();
  }
</script>

<section class="banner" aria-label={t('gamelist.title')}>
  <ListRestart size={15} strokeWidth={2} />
  <p>
    {#if c.emptied}
      <strong>{t('gamelist.emptied')}</strong>
    {:else}
      <strong>{t('gamelist.title')}</strong> {parts.join(' · ')}.
      {#if c.enabled.length || c.disabled.length}
        <span class="ids" title={[...c.enabled.map((m) => `+${m}`), ...c.disabled.map((m) => `−${m}`)].join('\n')}>
          {[...c.enabled.map((m) => `+${m}`), ...c.disabled.map((m) => `−${m}`)].slice(0, 4).join(', ')}{c.enabled.length + c.disabled.length > 4 ? '…' : ''}
        </span>
      {/if}
    {/if}
  </p>
  <div class="actions">
    {#if !c.emptied}
      <button class="btn" onclick={take} disabled={app.taken}>{t('gamelist.take')}</button>
    {/if}
    <button class="btn subtle" onclick={discard} disabled={app.taken} title={t('gamelist.discard_help')}>{t('gamelist.discard')}</button>
  </div>
</section>

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 16px 0;
    padding: 7px 10px;
    border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--line));
    border-radius: var(--radio);
    background: var(--warn-soft);
    color: var(--ink);
    font-size: var(--t-s);
  }
  .banner > :global(svg) {
    flex: none;
    color: var(--warn);
  }
  p {
    flex: 1;
    min-width: 0;
    margin: 0;
  }
  strong {
    font-weight: 600;
  }
  .ids {
    display: block;
    overflow: hidden;
    color: var(--ink-2);
    font-family: var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    flex: none;
    gap: 4px;
  }
</style>
