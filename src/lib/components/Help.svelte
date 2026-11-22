<script lang="ts">
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Bug from '@lucide/svelte/icons/bug';
  import { issueUrl } from '../update';
  import { api, message } from '../api';
  import { app, type Screen, type Tab } from '../state.svelte';
  import { t } from '../i18n.svelte';

  type Key = Parameters<typeof t>[0];
  type Go = { screen: Screen; tab?: Tab; label: Key };

  const topics: { id: string; paragraphs: number; go?: Go }[] = [
    { id: 'profiles', paragraphs: 2, go: { screen: 'home', label: 'nav.home' } },
    { id: 'mods', paragraphs: 4, go: { screen: 'library', label: 'nav.library' } },
    { id: 'order', paragraphs: 3, go: { screen: 'profile', tab: 'order', label: 'profile.tab.order' } },
    { id: 'conflicts', paragraphs: 2, go: { screen: 'profile', tab: 'conflicts', label: 'profile.tab.conflicts' } },
    { id: 'zb', paragraphs: 2, go: { screen: 'zb', label: 'nav.zombiebuddy' } },
    { id: 'play', paragraphs: 2, go: { screen: 'home', label: 'nav.home' } },
    { id: 'console', paragraphs: 2, go: { screen: 'profile', tab: 'console', label: 'profile.tab.console' } },
    { id: 'diagnosis', paragraphs: 2, go: { screen: 'profile', tab: 'console', label: 'profile.tab.console' } },
    { id: 'server', paragraphs: 2, go: { screen: 'profile', tab: 'sandbox', label: 'profile.tab.sandbox' } },
    { id: 'share', paragraphs: 1, go: { screen: 'profile', tab: 'export', label: 'profile.tab.export' } },
    { id: 'backups', paragraphs: 1, go: { screen: 'install', label: 'nav.install' } },
    { id: 'settings', paragraphs: 2, go: { screen: 'settings', label: 'nav.settings' } },
    { id: 'failures', paragraphs: 3 },
  ];

  const title = (id: string) => t(`help.${id}.t` as Key);
  const paragraphs = (tm: (typeof topics)[number]) =>
    Array.from({ length: tm.paragraphs }, (_, i) => t(`help.${tm.id}.p${i + 1}` as Key));

  function parts(text: string) {
    return text.split(/(`[^`]+`|\[[^\]\s][^\]]*\])/).map((v) =>
      v.startsWith('`') ? { kind: 'code', v: v.slice(1, -1) } : v.startsWith('[') ? { kind: 'kbd', v: v.slice(1, -1) } : { kind: 'text', v },
    );
  }

  const fold = (s: string) => s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase();
  let query = $state('');
  const shown = $derived.by(() => {
    const q = fold(query.trim());
    return q ? topics.filter((tm) => fold([title(tm.id), ...paragraphs(tm)].join(' ')).includes(q)) : topics;
  });

  const canGo = (g: Go) => !g.tab || !!app.profileId;
  async function go(g: Go) {
    if (g.tab && app.profileId) {
      await app.chooseProfile(app.profileId);
      app.tab = g.tab;
    } else app.screen = g.screen;
  }

  function report() {
    const env = app.environment;
    api.openUrl(issueUrl(__VERSION__, env?.game.version ?? null, env?.zb.in_game ?? null)).catch((e) => app.notify(message(e), 'error'));
  }
</script>

<section class="screen">
  <header class="head">
    <h1>{t('nav.help')}</h1>
    <p class="text">{t('help.intro')}</p>
  </header>
  <div class="content">
    <div class="side">
      <input
        class="field"
        type="search"
        bind:value={query}
        placeholder={t('help.filter')}
        aria-label={t('help.filter')}
        onkeydown={(e) => e.key === 'Escape' && (query = '')}
      />
      <nav class="index" aria-label={t('help.index')}>
        {#each shown as tm (tm.id)}
          <a href="#ayuda-{tm.id}">{title(tm.id)}</a>
        {/each}
      </nav>
    </div>
    <div class="topics">
      {#each shown as tm (tm.id)}
        <article id="ayuda-{tm.id}">
          <h2>{title(tm.id)}</h2>
          {#each paragraphs(tm) as text, i (i)}
            <p>{#each parts(text) as part, j (j)}{#if part.kind === 'code'}<code>{part.v}</code>{:else if part.kind === 'kbd'}<kbd>{part.v}</kbd>{:else}{part.v}{/if}{/each}</p>
          {/each}
          {#if tm.go && canGo(tm.go)}
            {@const g = tm.go}
            <button class="btn" onclick={() => go(g)}>{t('help.go', { s: t(g.label) })}<ArrowRight size={14} strokeWidth={1.75} /></button>
          {/if}
          {#if tm.id === 'failures'}
            <div class="buttons">
              <button class="btn" onclick={() => app.openLogs()}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.logs_open')}</button>
              <button class="btn" onclick={report}><Bug size={14} strokeWidth={1.75} />{t('help.report')}</button>
            </div>
          {/if}
        </article>
      {:else}
        <p class="none" role="status">{t('help.no_results', { q: query.trim() })}</p>
      {/each}
    </div>
  </div>
</section>

<style>
  .screen {
    height: 100%;
    overflow: auto;
    scroll-behavior: smooth;
  }
  .head {
    padding: 14px 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  h1 {
    font-size: var(--t-xl);
  }
  .text {
    margin: 4px 0 0;
    max-width: 75ch;
    color: var(--ink-2);
  }
  .content {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    gap: 24px;
    padding: 18px 16px 40px;
  }
  .side {
    position: sticky;
    top: 18px;
    align-self: start;
    display: grid;
    gap: 10px;
  }
  .side .field {
    width: 100%;
  }
  .index {
    display: grid;
    gap: 2px;
  }
  .index a {
    padding: 4px 8px;
    border-radius: var(--radio);
    color: var(--ink-2);
    font-size: var(--t-s);
    text-decoration: none;
  }
  .index a:hover {
    background: var(--surface-2);
    color: var(--ink);
  }
  .topics {
    display: grid;
    gap: 22px;
    max-width: 72ch;
  }
  article {
    scroll-margin-top: 12px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: var(--t-l);
    font-weight: 600;
  }
  p {
    margin: 0 0 8px;
    line-height: 1.55;
  }
  p code {
    padding: 0 3px;
    border: 1px solid var(--line);
    border-radius: 3px;
    background: var(--surface);
    overflow-wrap: anywhere;
  }
  kbd {
    display: inline-block;
    min-width: 1.6em;
    padding: 0 4px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 3px;
    background: var(--surface);
    font: inherit;
    font-size: var(--t-s);
    line-height: 1.35;
    text-align: center;
  }
  .none {
    color: var(--ink-2);
  }
</style>
