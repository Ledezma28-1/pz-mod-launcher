<script lang="ts">
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import { untrack } from 'svelte';
  import { t } from '../i18n.svelte';
  import { app } from '../state.svelte';
  import EmptyState from './EmptyState.svelte';

  let { reset }: { reset: () => void } = $props();

  const where = $derived(`${app.screen}|${app.tab}|${app.profileId}|${app.selection}`);
  const failedAt = untrack(() => where);
  $effect(() => {
    if (where !== failedAt) reset();
  });
</script>

<EmptyState title={t('error.screen')} text={t('error.screen.text')} error>
  <button class="btn" onclick={reset}>{t('error.screen.retry')}</button>
  <button class="btn" onclick={() => app.openLogs()}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.logs_open')}</button>
</EmptyState>
