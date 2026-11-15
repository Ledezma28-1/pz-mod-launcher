<script lang="ts">
  import Confirm from './Confirm.svelte';
  import Square from '@lucide/svelte/icons/square';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';

  let { frame = false }: { frame?: boolean } = $props();

  const inGame = $derived.by(() => {
    const p = app.session;
    if (!p || app.starting) return false;
    return p.requested > 0 ? p.loaded >= p.requested : p.seconds > 45;
  });
  let stopping = $state(false);

  async function stop() {
    stopping = true;
    try {
      await api.stop();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      stopping = false;
    }
  }
</script>

{#snippet button(onPress: (e?: Event) => void)}
  <button class="btn" class:frame class:big={frame} disabled={stopping || (!app.session && !app.starting)} onclick={onPress} title={inGame ? t('session.close_help') : t('session.stop_help')}>
    <Square size={13} strokeWidth={2} fill="currentColor" />{inGame ? t('session.close') : t('session.stop')}
  </button>
{/snippet}

{#if inGame}
  <Confirm text={t('session.close_confirm')} yes={t('session.close')} onsi={stop} {button} />
{:else}
  {@render button(stop)}
{/if}

<style>
  .btn {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, var(--line-strong));
  }
  .btn.frame {
    background: var(--shell-2);
    color: var(--shell-ink);
    border-color: var(--shell-line-strong);
  }
  .btn.frame :global(svg) {
    color: var(--shell-danger);
  }
  .btn.frame:hover {
    background: var(--shell-3);
  }
</style>
