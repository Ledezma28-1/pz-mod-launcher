<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import House from '@lucide/svelte/icons/house';
  import Library from '@lucide/svelte/icons/library';
  import Compass from '@lucide/svelte/icons/compass';
  import Puzzle from '@lucide/svelte/icons/puzzle';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import Settings from '@lucide/svelte/icons/settings';
  import CircleHelp from '@lucide/svelte/icons/circle-help';
  import Coffee from '@lucide/svelte/icons/coffee';
  import Download from '@lucide/svelte/icons/download';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { app, type Screen } from '../state.svelte';
  import { api, message } from '../api';
  import { i18n, t } from '../i18n.svelte';
  import { ago } from '../time';
  import { initialOf } from '../initial';

  let creating = $state(false);
  let name = $state('');

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const n = name.trim();
    if (!n) return;
    await app.createProfile(n, false);
    name = '';
    creating = false;
  }

  const sections: { id: Screen; icon: typeof Library; key: Parameters<typeof t>[0] }[] = [
    { id: 'library', icon: Library, key: 'nav.library' },
    { id: 'workshop', icon: Compass, key: 'nav.workshop' },
    { id: 'zb', icon: Puzzle, key: 'nav.zombiebuddy' },
    { id: 'install', icon: HardDrive, key: 'nav.install' },
  ];
</script>

<nav class="side" aria-label={t('nav.main')}>
  <div class="mark">
    <svg viewBox="0 0 512 512" width="18" height="18" aria-hidden="true">
      <rect width="512" height="512" rx="72" fill="currentColor" opacity="0.14" />
      <rect x="176" y="120" width="232" height="56" rx="10" fill="currentColor" />
      <rect x="176" y="228" width="232" height="56" rx="10" fill="currentColor" />
      <rect x="176" y="336" width="232" height="56" rx="10" fill="currentColor" />
      <path d="M120 148 V364 M120 148 H160 M120 364 H160" style="stroke: var(--shell-accent)" stroke-width="16" stroke-linecap="round" fill="none" />
    </svg>
    <span>{t('app.name')}</span>
    {#if app.steamProfile}
      {@const p = app.steamProfile}
      <button
        class="steam"
        aria-label="{p.name || p.steamid} · {t('steam.open_profile')}"
        aria-describedby="steam-card"
        onclick={() => api.openUrl(`https://steamcommunity.com/profiles/${p.steamid}`).catch((e) => app.notify(message(e), 'error'))}
      >
        {#if p.avatar}<img src={p.avatar} alt="" />{:else}<span class="initial" aria-hidden="true">{initialOf(p.name, p.steamid)}</span>{/if}
      </button>
      <div class="steam-card" id="steam-card" role="tooltip">
        <strong>{p.name || p.steamid}</strong>
        {#if p.minutes !== null}<span class="num">{t('steam.hours_pz', { h: new Intl.NumberFormat(i18n.language).format(Math.round(p.minutes / 60)) })}</span>{/if}
        {#if p.last_played}<span>{t('steam.last_played', { c: ago(p.last_played) })}</span>{/if}
      </div>
    {/if}
  </div>

  <div class="group start">
    <button class="section" class:current={app.screen === 'home'} aria-current={app.screen === 'home' ? 'page' : undefined} onclick={() => (app.screen = 'home')}>
      <House size={16} strokeWidth={1.75} />
      <span>{t('nav.home')}</span>
    </button>
  </div>

  <div class="group profiles">
    <div class="title">
      <span>{t('nav.profiles')}</span>
      <button class="more" title={t('nav.new')} aria-label={t('nav.new')} onclick={() => (creating = !creating)}>
        <Plus size={15} strokeWidth={1.75} />
      </button>
    </div>
    {#if creating}
      <form class="new" onsubmit={create}>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="field"
          bind:value={name}
          placeholder={t('empty.profiles.name_empty')}
          aria-label={t('nav.new')}
          autofocus
          onkeydown={(e) => e.key === 'Escape' && (creating = false)}
        />
      </form>
    {/if}
    <ul>
      {#each app.profiles as p (p.id)}
        <li>
          <button
            class="profile"
            class:current={app.screen === 'profile' && app.profileId === p.id}
            aria-current={app.screen === 'profile' && app.profileId === p.id ? 'page' : undefined}
            onclick={() => app.chooseProfile(p.id)}
          >
            <span class="name">{p.name}</span>
            <span class="count num">{p.active}</span>
          </button>
        </li>
      {/each}
    </ul>
  </div>

  <div class="group sections">
    {#each sections as s (s.id)}
      {@const Icon = s.icon}
      <button
        class="section"
        class:current={app.screen === s.id}
        aria-current={app.screen === s.id ? 'page' : undefined}
        onclick={() => (app.screen = s.id)}
      >
        <Icon size={16} strokeWidth={1.75} />
        <span>{t(s.key)}</span>
        {#if s.id === 'zb' && app.environment?.zb.warning}
          <span class="warning" title={app.environment.zb.warning}><TriangleAlert size={14} strokeWidth={2} /></span>
          <span class="hidden">{t('nav.zb_warning', { a: app.environment.zb.warning })}</span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="footer">
    <button class="section" class:current={app.screen === 'settings'} aria-current={app.screen === 'settings' ? 'page' : undefined} onclick={() => (app.screen = 'settings')}>
      <Settings size={16} strokeWidth={1.75} />
      <span>{t('nav.settings')}</span>
    </button>
    <button class="section" class:current={app.screen === 'help'} aria-current={app.screen === 'help' ? 'page' : undefined} onclick={() => (app.screen = 'help')}>
      <CircleHelp size={16} strokeWidth={1.75} />
      <span>{t('nav.help')}</span>
    </button>
    <button class="section" onclick={() => api.openUrl('https://buymeacoffee.com/ledezzma').catch((e) => app.notify(message(e), 'error'))} title={t('nav.coffee_help')}>
      <Coffee size={16} strokeWidth={1.75} />
      <span class="name">{t('nav.coffee')}</span>
      <ExternalLink class="out" size={13} strokeWidth={1.75} />
    </button>
    {#if app.update}
      {@const u = app.update}
      <button class="section update" onclick={() => api.openUrl(u.url).catch((e) => app.notify(message(e), 'error'))} title={t('update.available_help')}>
        <Download size={16} strokeWidth={1.75} />
        <span class="name">{t('update.available', { v: u.version })}</span>
        <ExternalLink class="out" size={13} strokeWidth={1.75} />
      </button>
    {/if}
    {#if app.environment}
      <div class="version mono" title={app.environment.game.folder}>
        PZ {app.environment.game.version}{#if app.environment.zb.in_game}&nbsp;· ZB {app.environment.zb.in_game}{/if}
      </div>
    {/if}
  </div>
</nav>

<style>
  .side {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--shell);
    color: var(--shell-ink);
    border-right: 1px solid var(--shell-line);
    min-height: 0;
  }
  .mark {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 14px;
    font-weight: 600;
    font-size: var(--t-m);
    letter-spacing: 0.01em;
    border-bottom: 1px solid var(--shell-line);
  }
  .steam {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-left: auto;
    padding: 0;
    border: 1px solid var(--shell-line);
    border-radius: 50%;
    overflow: hidden;
    background: var(--shell-3);
    color: var(--shell-ink);
    cursor: pointer;
  }
  .steam:hover {
    border-color: var(--shell-ink-2);
  }
  .steam img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .initial {
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .steam-card {
    position: absolute;
    top: calc(100% - 4px);
    right: 8px;
    z-index: 20;
    display: none;
    flex-direction: column;
    gap: 2px;
    min-width: 190px;
    padding: 8px 10px;
    border: 1px solid var(--shell-line);
    border-radius: var(--radio);
    background: var(--shell-2);
    color: var(--shell-ink-2);
    font-size: var(--t-s);
    font-weight: 400;
    letter-spacing: 0;
    box-shadow: var(--shadow-menu);
  }
  .steam-card strong {
    color: var(--shell-ink);
    font-weight: 600;
  }
  .steam:hover + .steam-card,
  .steam:focus-visible + .steam-card {
    display: flex;
  }
  .group {
    padding: 10px 8px 6px;
    min-height: 0;
  }
  .group.start {
    padding-bottom: 0;
  }
  .group.profiles {
    flex: 1;
    overflow-y: auto;
  }
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 6px 6px 8px;
    color: var(--shell-ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .more {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: var(--radio);
    background: transparent;
    color: var(--shell-ink-2);
    cursor: pointer;
  }
  .more:hover {
    background: var(--shell-3);
    color: var(--shell-ink);
  }
  .new {
    padding: 0 4px 6px;
  }
  .new .field {
    width: 100%;
    background: var(--shell-2);
    border-color: var(--shell-3);
    color: var(--shell-ink);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .profile,
  .section {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radio);
    background: transparent;
    color: var(--shell-ink);
    text-align: left;
    cursor: pointer;
    transition: background var(--dur) var(--ease);
  }
  .profile:hover,
  .section:hover {
    background: var(--shell-2);
  }
  .section :global(.out) {
    color: var(--shell-ink-2);
  }
  .section.update {
    color: var(--shell-accent);
    font-weight: 600;
  }
  .profile.current,
  .section.current {
    background: var(--shell-3);
    font-weight: 600;
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--shell-ink-2);
    font-size: var(--t-xs);
  }
  .sections {
    border-top: 1px solid var(--shell-line);
  }
  .section {
    color: var(--shell-ink);
  }
  .section :global(svg) {
    color: var(--shell-ink-2);
    flex: none;
  }
  .section span:not(.warning) {
    flex: 1;
  }
  .section .warning {
    display: inline-flex;
  }
  .section .warning :global(svg) {
    color: var(--shell-danger);
  }
  .footer {
    padding: 6px 8px 10px;
    border-top: 1px solid var(--shell-line);
  }
  .version {
    padding: 6px 8px 0;
    color: var(--shell-ink-2);
    font-size: var(--t-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
