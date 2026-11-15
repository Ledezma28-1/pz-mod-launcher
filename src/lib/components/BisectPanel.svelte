<script lang="ts">
  import Search from '@lucide/svelte/icons/search';
  import Play from '@lucide/svelte/icons/play';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Info from '@lucide/svelte/icons/info';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { BisectAnswer } from '../types';

  const b = $derived(app.bisect!);
  const originName = $derived(app.profiles.find((p) => p.id === b.origin)?.name ?? b.origin);

  const playing = $derived(app.starting || (!!app.session && !app.session.finished));
  const canAnswer = $derived(
    app.bisectPlayed === b.round && !!app.session?.finished && app.session.profile === b.profile && !playing,
  );

  const norm = (id: string) => id.replace(/^\\/, '').trim();
  const nameOf = (id: string) => app.view?.rows.find((r) => norm(r.id) === norm(id))?.name ?? id;
  const workshopOf = (id: string) => app.view?.rows.find((r) => norm(r.id) === norm(id))?.workshop_id ?? null;

  const answers: BisectAnswer[] = ['failed', 'passed', 'skip'];

  async function play() {
    if (app.profileId !== b.profile) await app.chooseProfile(b.profile);
    await app.play();
  }

  async function disableInOrigin(id: string) {
    await app.runView(() => api.setEnabled(b.origin, id, false), t('bisect.disabled', { m: nameOf(id), p: originName }));
  }
</script>

<section class="bisect" aria-label={t('bisect.title')}>
  <header>
    <Search size={15} strokeWidth={2} class="ico" />
    <h2>{t('bisect.title')}</h2>
    {#if !b.outcome}
      <span class="meta">{t('bisect.round', { n: b.round, m: b.rounds_left })} · {t('bisect.left', { n: b.suspects })}</span>
    {/if}
  </header>

  {#if b.outcome}
    <div class="outcome">
      {#if b.outcome.kind === 'culprit'}
        {@const id = b.outcome.data}
        <p class="verdict"><CircleX size={15} strokeWidth={2} class="bad" />{t('bisect.culprit', { m: nameOf(id) })}</p>
        <div class="actions">
          <button class="btn primary" onclick={() => disableInOrigin(id)}>{t('bisect.disable_in', { p: originName })}</button>
          {#if workshopOf(id)}
            <button class="btn subtle" onclick={() => api.openWorkshop(workshopOf(id)!).catch((e) => app.notify(message(e), 'error'))}>
              <ExternalLink size={14} strokeWidth={1.75} />{t('diag.workshop')}
            </button>
          {/if}
        </div>
      {:else if b.outcome.kind === 'group'}
        <p class="verdict"><CircleX size={15} strokeWidth={2} class="bad" />{t('bisect.group', { m: b.outcome.data.map(nameOf).join(', ') })}</p>
      {:else if b.outcome.kind === 'together'}
        <p class="verdict"><CircleX size={15} strokeWidth={2} class="bad" />{t('bisect.together', { m: b.outcome.data.map(nameOf).join(', ') })}</p>
      {:else if b.outcome.kind === 'not_reproduced'}
        <p class="verdict"><Info size={15} strokeWidth={2} class="ico" />{t('bisect.not_reproduced')}</p>
      {:else}
        <p class="verdict"><CircleCheck size={15} strokeWidth={2} class="ok" />{t('bisect.not_mods')}</p>
      {/if}
      <div class="actions">
        <button class="btn" title={t('bisect.done_help')} disabled={playing} onclick={() => app.endBisect()}>{t('bisect.done')}</button>
        {#if b.round > 1}
          <button class="btn subtle" disabled={playing} onclick={() => app.undoBisect()}><Undo2 size={14} strokeWidth={1.75} />{t('bisect.undo')}</button>
        {/if}
      </div>
    </div>
  {:else}
    {#if b.testing.length}
      <p>{t('bisect.testing', { n: b.testing.length })} <span class="names">{b.testing.slice(0, 6).map(nameOf).join(', ')}{b.testing.length > 6 ? '…' : ''}</span></p>
    {:else}
      <p>{t('bisect.control', { n: b.suspects })}</p>
    {/if}
    {#if b.mode === 'together'}
      <p class="mode">{t('bisect.mode_together')}</p>
    {:else if b.flaky}
      <p class="mode">{t('bisect.mode_flaky')}</p>
    {/if}
    <p class="steps"><Info size={14} strokeWidth={2} class="ico" />{t('bisect.steps')}</p>
    <div class="actions">
      <button class="btn primary" disabled={playing} onclick={play}>
        <Play size={14} strokeWidth={2} />{t('bisect.play')}
      </button>
      {#each answers as a (a)}
        <button
          class="btn"
          class:suggested={canAnswer && b.suggestion === a}
          aria-describedby={canAnswer && b.suggestion === a ? 'bisect-suggestion' : undefined}
          disabled={!canAnswer}
          onclick={() => app.answerBisect(a)}>{t(`bisect.answer.${a}`)}</button
        >
      {/each}
      {#if !canAnswer}<span class="wait">{t('bisect.wait')}</span>{/if}
    </div>
    {#if canAnswer && b.suggestion}
      <p class="suggestion" id="bisect-suggestion">{t(`bisect.suggestion.${b.suggestion}`)}</p>
    {/if}
    <div class="actions minor">
      {#if b.round > 1}
        <button class="btn subtle" disabled={playing} onclick={() => app.undoBisect()}><Undo2 size={14} strokeWidth={1.75} />{t('bisect.undo')}</button>
      {/if}
      <button class="btn subtle danger" disabled={playing} onclick={() => app.endBisect()}>{t('bisect.cancel')}</button>
    </div>
  {/if}
</section>

<style>
  .bisect {
    padding: 10px 14px 12px;
    border-bottom: 1px solid var(--line);
    background: color-mix(in srgb, var(--accent) 6%, var(--surface));
    font-size: var(--t-s);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }
  h2 {
    font-size: var(--t-m, 14px);
  }
  .meta {
    margin-left: auto;
    color: var(--ink-3);
  }
  p {
    margin: 0 0 6px;
  }
  .names {
    color: var(--ink-2);
  }
  .steps,
  .verdict {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .verdict {
    font-weight: 600;
  }
  .verdict :global(.bad) {
    color: var(--danger);
  }
  .verdict :global(.ok) {
    color: var(--ok);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
  }
  .actions.minor {
    margin-top: 4px;
  }
  .wait {
    color: var(--ink-3);
  }
  .btn.suggested:not(:disabled) {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .suggestion,
  .mode {
    margin-top: 6px;
    color: var(--ink-2);
  }
</style>
