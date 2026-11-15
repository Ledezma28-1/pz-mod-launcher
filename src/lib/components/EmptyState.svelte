<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import type { Snippet } from 'svelte';

  let { title, text, error = false, children }: { title: string; text?: string; error?: boolean; children?: Snippet } = $props();
</script>

<div class="empty" class:error role={error ? 'alert' : undefined}>
  <h2>{#if error}<CircleX size={20} strokeWidth={2} class="inline-icon" />{/if}{title}</h2>
  {#if text}<p>{text}</p>{/if}
  {#if children}<div class="actions">{@render children()}</div>{/if}
</div>

<style>
  .empty {
    width: min(64ch, 100%);
    margin: 12vh auto 24px;
    padding: 0 24px;
  }
  h2 {
    font-size: var(--t-xl);
  }
  .error h2 {
    color: var(--danger);
  }
  p {
    margin: 8px 0 0;
    color: var(--ink-2);
    line-height: 1.55;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 18px;
  }
</style>
