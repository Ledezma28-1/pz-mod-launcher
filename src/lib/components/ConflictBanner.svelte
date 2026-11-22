<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { app } from '../state.svelte';
  import { clusterTitle, fixHelp, fixText, findingText, t, whyText } from '../i18n.svelte';
  import { clusterByReason } from '../conflict-groups';
  import type { Fix } from '../types';

  const v = $derived(app.view!);
  const lines = $derived(clusterByReason(v.findings.filter((h) => h.severity === 'Blocks')));
  let open = $state(typeof window === 'undefined' || window.innerHeight >= 760);

  const first = (fixes: Fix[]) => (fixes.length > 2 && fixes.every((f) => f.action === 'Keep') ? [] : fixes.slice(0, 1));
</script>

<section class="banner" aria-label={t('profile.block', { n: v.counts.blocks })}>
  <div class="top">
    <button class="head" aria-expanded={open} onclick={() => (open = !open)}>
      {#if open}<ChevronDown size={15} strokeWidth={2} />{:else}<ChevronRight size={15} strokeWidth={2} />{/if}
      <CircleX size={15} strokeWidth={2} />
      <strong>{t('profile.block', { n: v.counts.blocks })}</strong>
    </button>
    <button class="all" onclick={() => (app.tab = 'conflicts')}>{t('conflict.see_all')}</button>
  </div>
  {#if open}
    <ul>
      {#each lines as c (c.id)}
        {@const e = c.entries[0]}
        <li>
          {#if c.entries.length > 1}
            <button class="cause" onclick={() => (app.tab = 'conflicts')}>
              <strong>{clusterTitle(c)}</strong> <span class="names mono">{c.entries.map((x) => x.mod).join(' · ')}</span>
            </button>
            <div class="fixes">
              {#if c.disable.length > 1}
                <button class="btn" onclick={() => app.disableMany(c.disable)} disabled={app.taken}>{t('conflict.disable_n', { n: c.disable.length })}</button>
              {/if}
            </div>
          {:else}
            {@const h = e.findings[0]}
            <button class="cause" title={e.findings.map(findingText).join('\n')} onclick={() => (app.selection = h.mods[0] ?? null)}>
              {#if c.key}<span class="mono">{e.mod}</span> {e.findings.map(whyText).join(' · ')}
              {:else if h.kind.kind === 'Incompatible' && h.mods.length > 2}{t('conflict.incompat_many', { n: h.mods.length })}
              {:else}{findingText(h)}{/if}
            </button>
            <div class="fixes">
              {#each first(e.fixes) as a (JSON.stringify(a))}
                <button class="btn" onclick={() => app.fix(a)} disabled={app.taken} title={fixHelp(a)}>{fixText(a)}</button>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .banner {
    margin: 10px 16px 0;
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--line));
    border-radius: var(--radio);
    background: var(--danger-soft);
    color: var(--ink);
  }
  .top {
    display: flex;
    align-items: center;
  }
  .all {
    flex: none;
    margin-right: 10px;
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--danger);
    font-size: var(--t-s);
    cursor: pointer;
  }
  .all:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 0;
    padding: 7px 10px;
    border: 0;
    background: none;
    color: var(--danger);
    text-align: left;
    cursor: pointer;
  }
  .head strong {
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 10px 8px 54px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 4px;
  }
  li {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    font-size: var(--t-s);
  }
  .fixes {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
    max-width: 55%;
  }
  .cause {
    flex: 1;
    min-width: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    padding: 3px 0 0;
    font: inherit;
    color: inherit;
    border: 0;
    background: none;
    text-align: left;
    cursor: pointer;
  }
  .cause:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .cause strong {
    font-weight: 600;
  }
  .mono {
    font-size: var(--t-xs);
  }
  .names {
    color: var(--ink-2);
  }
  .btn {
    flex: none;
    height: 24px;
  }
</style>
