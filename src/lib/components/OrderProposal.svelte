<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down';
  import ArrowDown from '@lucide/svelte/icons/arrow-down';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { ruleText, t } from '../i18n.svelte';
  import type { OrderProposal } from '../types';
  import { groupMoves } from '../order-groups';

  const p = $derived(app.proposal!);
  const v = $derived(app.view!);
  const name = (id: string) => v.rows.find((f) => f.id === id)?.name ?? id;

  async function apply() {
    const id = v.id;
    app.proposal = null;
    await app.run(() => api.sort(id, true));
  }

  const categoryText = (c: string) => t(`cat.${c}` as Parameters<typeof t>[0]);
  function because(m: OrderProposal['moves'][number]): string {
    const parts: string[] = m.reasons.slice(0, 2).map(ruleText);
    if (m.position) parts.push(t(`order.pos.${m.position}` as Parameters<typeof t>[0]));
    if (m.category) parts.push(t(m.guessed ? 'order.guessed_category' : 'order.category', { c: categoryText(m.category) }));
    return parts.join('; ');
  }
</script>

<section class="proposal" aria-label={t('order.proposal', { n: p.moves.length })}>
  <div class="head">
    <ArrowUpDown size={15} strokeWidth={1.75} />
    <strong>{p.moves.length ? t('order.proposal', { n: p.moves.length }) : t('order.already_fine')}</strong>
    <div class="buttons">
      <button class="btn subtle" onclick={() => (app.proposal = null)}>{p.moves.length ? t('order.discard') : t('common.close')}</button>
      {#if p.moves.length}<button class="btn primary" onclick={apply}>{t('order.apply')}</button>{/if}
    </div>
  </div>
  <p class="note">
    {t('order.help')}
    {#if p.rules_file}{' '}{t('order.rules_file', { n: p.rules_file })}{/if}
  </p>
  {#each p.cycles as c, i (i)}
    <p class="cycle" class:severity-blocks={c.hard}>{#if c.hard}<CircleX size={13} strokeWidth={2} class="inline-icon" /><span class="sr-only">{t('sev.blocks')}: </span>{/if}{t('order.cycle', { m: c.mods.join(' → ') })}</p>
  {/each}
  <div class="groups">
    {#each groupMoves(p.moves) as g (g.effect)}
      <h3>{t(`order.group_${g.effect}`, { n: g.moves.length })}</h3>
      {#if g.effect === 'category'}<p class="group-note">{t('order.group_category_note')}</p>{/if}
      <ol>
        {#each g.moves as m (m.id)}
          <li>
            <span class="arrow" class:low={m.to > m.from}>
              {#if m.to > m.from}<ArrowDown size={13} strokeWidth={2} />{:else}<ArrowUp size={13} strokeWidth={2} />{/if}
            </span>
            <span class="mod">{name(m.id)}</span>
            <span class="num pos">{m.from + 1} → {m.to + 1}</span>
            {#if because(m)}
              <span class="reason" title={because(m)}>{t('order.reason')} {because(m)}</span>
            {/if}
          </li>
        {/each}
      </ol>
    {/each}
  </div>
</section>

<style>
  .proposal {
    margin: 10px 16px 0;
    border: 1px solid var(--accent);
    border-radius: var(--radio);
    background: var(--surface);
    max-height: 38%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--line);
    color: var(--accent);
  }
  .head strong {
    color: var(--ink);
    font-weight: 600;
  }
  .buttons {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }
  .note {
    margin: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .cycle {
    margin: 0;
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    font-size: var(--t-s);
  }
  .groups {
    overflow: auto;
    min-height: 0;
  }
  h3 {
    margin: 0;
    padding: 8px 12px 2px;
    font-size: var(--t-xs);
    font-weight: 600;
    color: var(--ink-2);
  }
  .group-note {
    margin: 0;
    padding: 0 12px 2px;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 2px 0 4px;
  }
  li {
    display: grid;
    grid-template-columns: 20px minmax(160px, 260px) 80px 1fr;
    align-items: baseline;
    gap: 8px;
    padding: 3px 12px;
    font-size: var(--t-s);
  }
  .arrow {
    color: var(--accent);
    align-self: center;
    display: inline-flex;
  }
  .mod {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pos {
    color: var(--ink-2);
  }
  .reason {
    color: var(--ink-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
