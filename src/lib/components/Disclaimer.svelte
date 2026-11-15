<script lang="ts">
  import { arrows } from '../keys';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import { app } from '../state.svelte';
  import { i18n, setLanguage, t, type Language } from '../i18n.svelte';

  let hide = $state(!app.disclaimerAtStart);
  let title: HTMLHeadingElement | undefined = $state();
  $effect(() => title?.focus());
</script>

<section class="screen" aria-labelledby="disclaimer-t">
  <div class="sheet">
    <div class="lang" role="radiogroup" aria-label={t('settings.language')} use:arrows={'radio'}>
      {#each [['es', 'Español'], ['en', 'English']] as [id, name] (id)}
        <button role="radio" lang={id} aria-checked={i18n.language === id} tabindex={i18n.language === id ? 0 : -1} onclick={() => setLanguage(id as Language)}>{name}</button>
      {/each}
    </div>
    <h1 id="disclaimer-t" tabindex="-1" bind:this={title}>{t('disclaimer.title')}</h1>
    <p>{t('disclaimer.p1')}</p>
    <div class="java">
      <ShieldAlert size={18} strokeWidth={1.75} class="ico" />
      <div>
        <h2>{t('disclaimer.java_title')}</h2>
        <p>{t('disclaimer.p2')}</p>
      </div>
    </div>
    <p>{t('disclaimer.p3')}</p>
    <div class="actions">
      <label class="checkbox"><input type="checkbox" bind:checked={hide} />{t('disclaimer.hide')}</label>
      <button class="btn primary big" onclick={() => app.closeDisclaimer(hide)}>{t('disclaimer.ok')}</button>
    </div>
  </div>
</section>

<style>
  .screen {
    height: 100%;
    overflow: auto;
    display: grid;
    place-items: start center;
    padding: 48px 24px;
  }
  .sheet {
    max-width: 64ch;
  }
  .lang {
    display: inline-flex;
    margin-bottom: 18px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    overflow: hidden;
  }
  .lang button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-right: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: var(--t-s);
    cursor: pointer;
  }
  .lang button:last-child {
    border-right: 0;
  }
  .lang button[aria-checked='true'] {
    background: var(--accent-fill);
    color: var(--accent-ink);
  }
  h1 {
    margin: 0 0 14px;
    font-size: var(--t-xl);
    font-weight: 600;
  }
  h1:focus {
    outline: none;
  }
  h2 {
    margin: 0 0 4px;
    font-size: var(--t-l);
    font-weight: 600;
  }
  p {
    margin: 0 0 12px;
    line-height: 1.55;
    color: var(--ink-2);
  }
  .java {
    display: flex;
    gap: 10px;
    margin: 4px 0 14px;
    padding: 12px 14px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--warn-soft);
  }
  .java :global(.ico) {
    flex: none;
    margin-top: 2px;
    color: var(--warn);
  }
  .java p {
    margin: 0;
    color: var(--ink);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 20px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-2);
  }
  .checkbox input {
    accent-color: var(--accent);
  }
</style>
