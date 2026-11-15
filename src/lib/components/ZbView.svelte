<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Check from '@lucide/svelte/icons/check';
  import Minus from '@lucide/svelte/icons/minus';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import Copy from '@lucide/svelte/icons/copy';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Trash from '@lucide/svelte/icons/trash';
  import { api, message } from '../api';
  import { app, readText, ZB_IDS } from '../state.svelte';
  import { channelOf, zbAction, type Action, type Channel } from '../zb-flow';
  import { t } from '../i18n.svelte';

  const zb = $derived(app.environment!.zb);
  let working = $state(false);

  const inGameChannel = $derived(channelOf(zb.in_game, zb.workshop, readText('zb-channel')));
  $effect(() => {
    void app.zbStates();
  });
  async function act(channel: Channel, a: Action) {
    working = true;
    try {
      if (a.kind === 'subscribe') await app.zbSubscribeInstall(channel);
      else await app.zbInstall(channel);
    } finally {
      working = false;
    }
  }
  function askedUninstall() {
    app.zbAskUninstall = null;
    confirm = true;
    queueMicrotask(() => sectionRemove?.scrollIntoView({ block: 'start' }));
  }

  let confirm = $state(false);
  let checking = $state(false);
  let sectionRemove: HTMLElement | undefined = $state();
  const launch = $derived([zb.ref_json && 'ProjectZomboid64.json', zb.ref_bat && 'ProjectZomboid64.bat'].filter(Boolean).join(' · '));
  const steamPending = $derived(zb.ref_steam.length > 0);

  async function check() {
    checking = true;
    try {
      await app.loadEnvironment();
    } finally {
      checking = false;
    }
  }

  async function copyOption() {
    try {
      await navigator.clipboard.writeText('-agentlib:zbNative');
      app.notify(t('zb.remove.copied'));
    } catch {
      app.notify(t('sb.not_copied'), 'error');
    }
  }

  async function uninstall() {
    working = true;
    try {
      const c = await api.uninstallZb();
      app.notify(t('zb.uninstalled', { c }));
      confirm = false;
      await app.loadEnvironment();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      working = false;
    }
  }

  const channels = (Object.keys(ZB_IDS) as Channel[]).map((channel) => ({ channel, id: ZB_IDS[channel] }));
</script>

<section class="screen">
  <header class="head">
    <div>
      <h1>{t('zb.title')}</h1>
      <p class="text">{t('zb.text')}</p>
    </div>
    {#if zb.in_game}
      <button class="btn subtle danger" onclick={() => sectionRemove?.scrollIntoView({ block: 'start' })}><Trash size={14} strokeWidth={1.75} />{t('zb.remove.go')}</button>
    {/if}
  </header>

  <div class="content">
    {#if zb.warning}
      <p class="warning severity-blocks"><CircleX size={15} strokeWidth={2} />{zb.warning}</p>
    {/if}

    <dl class="data">
      <dt>{t('zb.in_game')}</dt>
      <dd>
        {#if zb.in_game}<span class="mono strong">{zb.in_game}</span>{:else}<span class="dim">{t('zb.not_installed')}</span>{/if}
      </dd>
      {#each channels as c (c.channel)}
        {@const copy = zb.workshop.find((w) => w.id === c.id)}
        {@const local = app.zbLocal[c.id] ?? null}
        {@const pending = app.zbPending?.channel === c.channel}
        {@const a = zbAction(c.channel, copy, local, zb.in_game, inGameChannel)}
        <dt>{t('zb.workshop', { c: t(`zb.channel.${c.channel}`) })} <span class="mono dim">{c.id}</span></dt>
        <dd class="row">
          {#if copy?.version && a.kind !== 'subscribe'}<span class="mono">{copy.version}</span>{/if}
          {#if a.kind === 'subscribe'}
            <button class="btn" class:primary={!zb.in_game} disabled={working || !!app.zbPending} onclick={() => act(c.channel, a)}>{t('zb.subscribe_install')}</button>
          {:else if a.kind === 'downloading' || (pending && a.kind !== 'current')}
            <span class="dim" role="status">{t('zb.downloading', { n: a.kind === 'downloading' ? a.pct : 100 })}</span>
          {:else if a.kind === 'install'}
            <button class="btn primary" disabled={working} onclick={() => act(c.channel, a)}>{t('zb.install', { v: a.v })}</button>
          {:else if a.kind === 'switch'}
            <button class="btn" disabled={working} onclick={() => act(c.channel, a)}>{t('zb.switch', { c: t(`zb.channel.${c.channel}`), v: a.v, f: t(`zb.channel.${a.from}`), w: a.fromV })}</button>
          {:else if a.kind === 'update'}
            <button class="btn primary" disabled={working} onclick={() => act(c.channel, a)}>{t('zb.update', { v: a.v })}</button>
          {:else}
            <span class="ok"><Check size={14} strokeWidth={2} />{t('zb.on_day')}</span>
          {/if}
          {#if a.kind !== 'subscribe'}
            <button class="btn subtle" disabled={working} onclick={() => app.zbUnsubscribe(c.channel)}>{t('zb.unsubscribe')}</button>
          {/if}
        </dd>
        {#if app.zbError?.channel === c.channel}
          <dd class="row severity-blocks" role="alert"><CircleX size={14} strokeWidth={2} />{app.zbError.text}</dd>
        {/if}
        {#if app.zbAskUninstall === c.channel && zb.in_game}
          <dd class="row ask">
            <span>{t('zb.ask_uninstall')}</span>
            <button class="btn" onclick={askedUninstall}><Trash size={14} strokeWidth={1.75} />{t('zb.uninstall')}</button>
            <button class="btn subtle" onclick={() => (app.zbAskUninstall = null)}>{t('zb.keep')}</button>
          </dd>
        {/if}
      {/each}
      <dt>{t('zb.load')}</dt>
      <dd>
        <ul class="refs">
          {#each [[zb.ref_json, t('zb.json')], [zb.ref_bat, t('zb.bat')], [zb.ref_steam.length > 0, t('zb.steam')]] as [yes, text] (text)}
            <li class:yes>
              {#if yes}<Check size={13} strokeWidth={2} />{:else}<Minus size={13} strokeWidth={2} />{/if}
              <span>{text}</span>
            </li>
          {/each}
        </ul>
      </dd>
    </dl>

    <p class="safety"><ShieldAlert size={15} strokeWidth={1.75} /><span>{t('zb.safety')}</span></p>

    {#if zb.in_game}
      <section class="remove" aria-labelledby="zb-quitar" bind:this={sectionRemove}>
        <h2 id="zb-quitar">{t('zb.uninstall')}</h2>
        <p class="text">{t('zb.remove.text')}</p>
        <ol class="steps">
          <li>
            <Check size={14} strokeWidth={2} />
            <span>{t('zb.remove.step_copy')}</span>
          </li>
          <li>
            <Check size={14} strokeWidth={2} />
            <span>{launch ? t('zb.remove.step_launch', { f: launch }) : t('zb.remove.step_launch_nothing')}</span>
          </li>
          {#if steamPending}
            <li class="pending">
              <TriangleAlert size={14} strokeWidth={2} />
              <div>
                <p><strong>{t('zb.remove.steam_pending')}</strong></p>
                <p>{t('zb.remove.steam_how')}</p>
                <p class="dim">{t('zb.remove.steam_close')}</p>
                <div class="actions">
                  <button class="btn" onclick={copyOption}><Copy size={14} strokeWidth={1.75} />{t('zb.remove.copy')}</button>
                  <button class="btn" onclick={check} disabled={checking}><RefreshCw size={14} strokeWidth={1.75} />{t('zb.remove.check')}</button>
                </div>
              </div>
            </li>
          {:else}
            <li>
              <Check size={14} strokeWidth={2} />
              <span>{t('zb.remove.step_steam')}</span>
            </li>
          {/if}
        </ol>
        <div class="decide">
          {#if confirm && !steamPending}
            <p role="alert">{t('zb.uninstall_warning')}</p>
            <button class="btn" onclick={() => (confirm = false)} disabled={working}>{t('common.cancel')}</button>
            <button class="btn danger-full" onclick={uninstall} disabled={working}>{t('zb.uninstall_if')}</button>
          {:else}
            <button class="btn danger" disabled={working || steamPending} onclick={() => (confirm = true)}><Trash size={14} strokeWidth={1.75} />{t('zb.uninstall')}…</button>
            {#if steamPending}<span class="dim">{t('zb.remove.blocked')}</span>{/if}
          {/if}
        </div>
      </section>
    {/if}
  </div>
</section>

<style>
  .screen {
    height: 100%;
    overflow: auto;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    padding: 14px 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  h1 {
    font-size: var(--t-xl);
  }
  .text {
    margin: 3px 0 0;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .content {
    padding: 16px;
    max-width: 760px;
  }
  .warning {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 14px;
    padding: 10px 12px;
    background: var(--danger-soft);
    border-radius: var(--radio);
  }
  .data {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 12px 16px;
    margin: 0;
    align-items: center;
  }
  dt {
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  dd {
    margin: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row.severity-blocks,
  .row.ask {
    grid-column: 2;
  }
  .row.severity-blocks {
    gap: 6px;
  }
  .row.ask {
    justify-self: start;
    flex-wrap: wrap;
    padding: 8px 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
  }
  .strong {
    font-weight: 600;
  }
  .dim {
    color: var(--ink-3);
  }
  .ok {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--ok);
    font-size: var(--t-s);
  }
  .refs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    font-size: var(--t-s);
    color: var(--ink-3);
  }
  .refs li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .refs li.yes {
    color: var(--ink);
  }
  .safety {
    display: flex;
    gap: 8px;
    margin: 22px 0 0;
    padding-top: 14px;
    border-top: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-s);
    max-width: 70ch;
  }
  .remove {
    margin-top: 22px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .remove h2 {
    font-size: var(--t-l);
    font-weight: 600;
  }
  .steps {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: grid;
    gap: 8px;
    font-size: var(--t-s);
    max-width: 70ch;
  }
  .steps li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .steps li > :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--ok);
  }
  .steps li.pending {
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--line));
    border-radius: var(--radio);
    background: var(--warn-soft);
  }
  .steps li.pending > :global(svg) {
    color: var(--warn);
  }
  .pending p {
    margin: 0 0 6px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .decide {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 16px;
    font-size: var(--t-s);
  }
  .decide p {
    flex-basis: 100%;
    margin: 0 0 4px;
    max-width: 70ch;
  }
  .danger-full {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--surface);
  }
  .danger-full:hover {
    background: var(--danger);
    opacity: 0.9;
  }
  .safety :global(svg) {
    flex: none;
    margin-top: 2px;
  }
</style>
