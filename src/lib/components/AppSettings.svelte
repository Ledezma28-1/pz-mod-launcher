<script lang="ts">
  import { arrows } from '../keys';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Minus from '@lucide/svelte/icons/minus';
  import Plus from '@lucide/svelte/icons/plus';
  import { api, message } from '../api';
  import { app, SCALE_MAX, SCALE_MIN, type Theme } from '../state.svelte';
  import { steam } from '../steam.svelte';
  import { PALETTES, SWATCH } from '../palettes';
  import { setLanguage, i18n, t, type Language } from '../i18n.svelte';
  import type { FoldersInfo, MaintenanceInfo, NewProfile, WindowOnPlay } from '../types';

  let reading = $state(false);

  async function rescan() {
    reading = true;
    try {
      const n = await api.rescan();
      app.notify(t('settings.rescan_done', { n }));
      await app.loadEnvironment();
    } catch (e) {
      app.notify(message(e), 'error');
    } finally {
      reading = false;
    }
  }

  async function open(path: string) {
    try {
      await api.openFolder(path);
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  async function openLogs() {
    try {
      await open(await api.logsFolder());
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  const themes: Theme[] = ['system', 'light', 'dark'];

  let systemDark = $state(matchMedia('(prefers-color-scheme: dark)').matches);
  $effect(() => {
    const m = matchMedia('(prefers-color-scheme: dark)');
    const f = () => (systemDark = m.matches);
    m.addEventListener('change', f);
    return () => m.removeEventListener('change', f);
  });
  const dark = $derived(app.theme === 'dark' || (app.theme === 'system' && systemDark));
  function paletteKey(e: KeyboardEvent) {
    const d = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const group = e.currentTarget as HTMLElement;
    const i = (PALETTES.indexOf(app.palette) + d + PALETTES.length) % PALETTES.length;
    app.changePalette(PALETTES[i]);
    queueMicrotask(() => group.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
  const pct = (e: number) => `${Math.round(e * 100)} %`;

  const STEP = 0.05;
  const steps = Array.from({ length: Math.round((SCALE_MAX - SCALE_MIN) / STEP) + 1 }, (_, i) => Math.round((SCALE_MIN + i * STEP) * 100) / 100);
  const near = (a: number, b: number) => Math.abs(a - b) < 0.001;
  function scaleTo(e: number) {
    const v = Math.round(Math.min(SCALE_MAX, Math.max(SCALE_MIN, e)) * 100) / 100;
    if (!near(v, app.scale)) app.changeScale(v);
  }
  function scaleKey(e: KeyboardEvent) {
    const moves: Record<string, number> = { ArrowRight: STEP, ArrowUp: STEP, '+': STEP, ArrowLeft: -STEP, ArrowDown: -STEP, '-': -STEP };
    if (e.key in moves) scaleTo(app.scale + moves[e.key]);
    else if (e.key === 'Home') scaleTo(SCALE_MIN);
    else if (e.key === 'End') scaleTo(SCALE_MAX);
    else return;
    e.preventDefault();
  }
  const times = [15, 30, 60, 120, 300, 600];
  const timeText = (s: number) => (s < 60 ? t('settings.seconds', { n: s }) : t('settings.minutes', { n: s / 60 }));
  const windows: WindowOnPlay[] = ['nothing', 'minimize_restore', 'minimize'];

  function size(bytes: number): string {
    const f = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
    if (bytes < 1024 * 1024) return `${f.format(Math.max(1, Math.round(bytes / 1024)))} KB`;
    return `${f.format(bytes / (1024 * 1024))} MB`;
  }

  function change(patch: Partial<typeof app.settings & object>) {
    if (app.settings) void app.saveSettings({ ...app.settings, ...patch });
  }

  let draft = $state<(Omit<NewProfile, 'game_extra'> & { game_extra: string }) | null>(null);
  let draftError = $state<string | null>(null);
  let draftSaved = $state(false);
  $effect(() => {
    const n = app.settings?.new_profile;
    if (n && !draft) draft = { ...n, game_extra: n.game_extra.join(' ') };
  });
  async function saveNew(e: SubmitEvent) {
    e.preventDefault();
    if (!app.settings || !draft) return;
    const new_profile: NewProfile = { ...draft, xmx: draft.xmx.trim(), game_extra: draft.game_extra.split(/\s+/).filter(Boolean) };
    try {
      await api.settingsSet({ ...app.settings, new_profile });
      app.settings = { ...app.settings, new_profile };
      draftError = null;
      draftSaved = true;
      setTimeout(() => (draftSaved = false), 2000);
    } catch (err) {
      draftError = message(err);
    }
  }

  let folders = $state<FoldersInfo | null>(null);
  let maint = $state<MaintenanceInfo | null>(null);
  async function loadInfo() {
    api.foldersInfo().then((f) => (folders = f)).catch((e) => app.notify(message(e), 'error'));
    api.maintenanceInfo().then((m) => (maint = m)).catch((e) => app.notify(message(e), 'error'));
  }
  $effect(() => {
    void loadInfo();
  });

  const playing = $derived(app.starting || (!!app.session && !app.session.finished));
  let changing = $state(false);
  let gamePath = $state('');
  async function useGame(e: SubmitEvent) {
    e.preventDefault();
    try {
      await api.chooseGame(gamePath);
      changing = false;
      app.notify(t('settings.folder.changed'));
      await app.loadEnvironment();
      await loadInfo();
    } catch (err) {
      app.notify(message(err), 'error');
    }
  }

  async function clear(what: 'workshop' | 'logs') {
    try {
      const n = await api.maintenanceClear(what);
      if (what === 'workshop') steam.forget();
      app.notify(t('settings.maint.freed', { n: size(n) }));
      maint = await api.maintenanceInfo();
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }

  async function webview(pending: boolean) {
    try {
      await api.maintenanceWebview(pending);
      if (maint) maint = { ...maint, webview_pending: pending };
      if (app.settings) app.settings = { ...app.settings, clear_webview_cache: pending };
    } catch (e) {
      app.notify(message(e), 'error');
    }
  }
</script>

<section class="screen">
  <header class="head"><h1>{t('nav.settings')}</h1></header>
  <div class="content">
    <h2>{t('settings.section.appearance')}</h2>
    <div class="group">
      <span class="tag">{t('settings.theme')}</span>
      <div class="segmentos" role="radiogroup" aria-label={t('settings.theme')} use:arrows={'radio'}>
        {#each themes as tm (tm)}
          <button role="radio" aria-checked={app.theme === tm} tabindex={app.theme === tm ? 0 : -1} onclick={() => app.changeTheme(tm)}>{t(`settings.theme.${tm}`)}</button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="tag" id="palette-tag">{t('settings.palette')}</span>
      <div class="palettes" role="radiogroup" aria-labelledby="palette-tag" tabindex="-1" onkeydown={paletteKey}>
        {#each PALETTES as p (p)}
          {@const [frame, ground, accent] = SWATCH[p][dark ? 'dark' : 'light']}
          <button role="radio" aria-checked={app.palette === p} tabindex={app.palette === p ? 0 : -1} onclick={() => app.changePalette(p)}>
            <span class="swatch" aria-hidden="true"
              ><span style:background={frame}></span><span style:background={ground}></span><span style:background={accent}></span></span
            >
            <span>{t(`palette.${p}`)}</span>
          </button>
        {/each}
      </div>
    </div>
    <div class="group">
      <span class="tag" id="scale-tag">{t('settings.scale')}</span>
      <div class="with-help">
        <div class="row-scale">
          <div class="stepper">
            <button aria-label={t('settings.scale_less')} title={t('settings.scale_less')} disabled={near(app.scale, SCALE_MIN)} onclick={() => scaleTo(app.scale - STEP)}>
              <Minus size={14} strokeWidth={2} />
            </button>
            <div
              class="dial"
              role="slider"
              tabindex="0"
              aria-labelledby="scale-tag"
              aria-valuemin={Math.round(SCALE_MIN * 100)}
              aria-valuemax={Math.round(SCALE_MAX * 100)}
              aria-valuenow={Math.round(app.scale * 100)}
              aria-valuetext={pct(app.scale)}
              onkeydown={scaleKey}
            >
              <span class="num reading">{pct(app.scale)}</span>
              <span class="ruler" aria-hidden="true">
                {#each steps as s (s)}
                  <button tabindex="-1" class="tick" class:major={Math.round(s * 100) % 10 === 0} class:on={near(s, app.scale)} title={pct(s)} onclick={() => scaleTo(s)}></button>
                {/each}
              </span>
            </div>
            <button aria-label={t('settings.scale_more')} title={t('settings.scale_more')} disabled={near(app.scale, SCALE_MAX)} onclick={() => scaleTo(app.scale + STEP)}>
              <Plus size={14} strokeWidth={2} />
            </button>
          </div>
          <button class="btn subtle" disabled={near(app.scale, 1)} onclick={() => scaleTo(1)}>{t('settings.scale_reset')}</button>
        </div>
        <p class="help">{t('settings.scale_help')}</p>
      </div>
    </div>
    <div class="group">
      <span class="tag">{t('settings.language')}</span>
      <div class="segmentos" role="radiogroup" aria-label={t('settings.language')} use:arrows={'radio'}>
        {#each [['es', 'Español'], ['en', 'English']] as [id, name] (id)}
          <button role="radio" aria-checked={i18n.language === id} tabindex={i18n.language === id ? 0 : -1} onclick={() => setLanguage(id as Language)}>{name}</button>
        {/each}
      </div>
    </div>

    {#if app.settings}
      {@const s = app.settings}
      <h2>{t('settings.section.play')}</h2>
      <div class="group">
        <span class="tag">{t('settings.window')}</span>
        <div class="with-help">
          <div class="segmentos" role="radiogroup" aria-label={t('settings.window')} use:arrows={'radio'}>
            {#each windows as w (w)}
              <button role="radio" aria-checked={s.window_on_play === w} tabindex={s.window_on_play === w ? 0 : -1} onclick={() => change({ window_on_play: w })}>{t(`settings.window.${w}`)}</button>
            {/each}
          </div>
          <p class="help">{t('settings.window_help')}</p>
        </div>
      </div>
      <div class="group">
        <span class="tag">{t('settings.integrity_tag')}</span>
        <div class="with-help">
          <label class="checkbox">
            <input type="checkbox" checked={s.integrity_check} onchange={(e) => change({ integrity_check: e.currentTarget.checked })} />
            {t('settings.integrity')}
          </label>
          <p class="help">{t('settings.integrity_help')}</p>
        </div>
      </div>

      <h2>{t('settings.section.new_profile')}</h2>
      {#if draft}
        <form class="new" onsubmit={saveNew}>
          <p class="help lead">{t('settings.new_help')}</p>
          <div class="group">
            <label class="tag" for="np-xmx">{t('settings.memory')}</label>
            <div class="with-help">
              <input id="np-xmx" class="field mono short" bind:value={draft.xmx} placeholder="8g" />
              <p class="help">{t('settings.memory_help')}</p>
            </div>
          </div>
          <div class="group">
            <label class="tag" for="np-extra">{t('settings.extra')}</label>
            <div class="with-help">
              <input id="np-extra" class="field mono" bind:value={draft.game_extra} />
              <p class="help">{t('settings.extra_help')}</p>
            </div>
          </div>
          <div class="group">
            <label class="tag" for="np-zb">{t('settings.zb_use')}</label>
            <select id="np-zb" class="field" bind:value={draft.zb_usage}>
              <option value="auto">{t('settings.zb_auto')}</option>
              <option value="si">{t('settings.zb_yes')}</option>
              <option value="no">{t('settings.zb_no')}</option>
            </select>
          </div>
          <div class="group">
            <label class="tag" for="np-policy">{t('settings.policy')}</label>
            <select id="np-policy" class="field" bind:value={draft.zb_policy}>
              <option value="prompt">{t('settings.prompt')}</option>
              <option value="deny-new">{t('settings.deny-new')}</option>
              <option value="allow-all">{t('settings.allow-all')}</option>
            </select>
          </div>
          <div class="group">
            <span class="tag"></span>
            <label class="checkbox"><input type="checkbox" bind:checked={draft.import_game_mods} />{t('settings.import_mods')}</label>
          </div>
          <div class="group">
            <span class="tag"></span>
            <div class="buttons">
              <button class="btn primary" type="submit">{t('settings.save')}</button>
              {#if draftSaved}<span class="ok" role="status">{t('settings.saved')}</span>{/if}
              {#if draftError}<span class="error" role="alert"><CircleX size={13} strokeWidth={2} class="inline-icon" />{draftError}</span>{/if}
            </div>
          </div>
        </form>
      {/if}
    {/if}

    <h2>{t('settings.section.folders')}</h2>
    {#if folders}
      <div class="group">
        <span class="tag">{t('settings.folder.game')}</span>
        <div class="with-help">
          <div class="path-row">
            <span class="path mono" title={folders.game ?? ''}>{folders.game ?? t('settings.folder.none')}</span>
            <button class="btn subtle" disabled={!folders.game} onclick={() => folders?.game && open(folders.game)}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.folder.open')}</button>
            {#if !changing}
              <button class="btn subtle" disabled={playing} title={playing ? t('settings.folder.playing') : undefined} onclick={() => ((changing = true), (gamePath = folders?.game ?? ''))}>{t('settings.folder.change')}</button>
            {/if}
          </div>
          {#if changing}
            <form class="path-row" onsubmit={useGame}>
              <input class="field mono wide" bind:value={gamePath} aria-label={t('settings.folder.game')} />
              <button class="btn primary" type="submit" disabled={playing || !gamePath.trim()}>{t('settings.folder.use')}</button>
              <button class="btn subtle" type="button" onclick={() => (changing = false)}>{t('settings.folder.cancel')}</button>
            </form>
            <p class="help">{t('settings.folder.change_help')}</p>
          {/if}
        </div>
      </div>
      <div class="group">
        <span class="tag">{t('settings.folder.profiles')}</span>
        <div class="path-row">
          <span class="path mono" title={folders.profiles}>{folders.profiles}</span>
          <button class="btn subtle" onclick={() => folders && open(folders.profiles)}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.folder.open')}</button>
        </div>
      </div>
      <div class="group">
        <span class="tag">{t('settings.folder.backups')}</span>
        <div class="path-row">
          <span class="path mono" title={folders.backups}>{folders.backups}</span>
          <span class="num size">{folders.backups_exists ? size(folders.backups_bytes) : t('settings.folder.empty')}</span>
          <button class="btn subtle" disabled={!folders.backups_exists} onclick={() => folders && open(folders.backups)}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.folder.open')}</button>
        </div>
      </div>
    {:else}
      <p class="help">{t('settings.calculating')}</p>
    {/if}

    <h2>{t('settings.section.workshop')}</h2>
    <div class="group">
      <label class="tag" for="steam-idle">{t('settings.steam')}</label>
      <div class="with-help">
        <select id="steam-idle" class="field" value={app.steamIdle} onchange={(e) => app.changeIdle(Number(e.currentTarget.value))}>
          {#each times as s (s)}
            <option value={s}>{timeText(s)}</option>
          {/each}
          {#if !times.includes(app.steamIdle)}<option value={app.steamIdle}>{timeText(app.steamIdle)}</option>{/if}
        </select>
        <p class="help">{t('settings.steam_help')}</p>
      </div>
    </div>
    <div class="group">
      <span class="tag"></span>
      <button class="btn" onclick={rescan} disabled={reading}>
        <RefreshCw size={14} strokeWidth={1.75} />{t('settings.rescan')}
      </button>
    </div>

    <h2>{t('settings.section.maintenance')}</h2>
    <div class="group">
      <span class="tag">{t('settings.maint.webview')}</span>
      <div class="with-help">
        <div class="path-row">
          <span class="num size">{maint ? size(maint.webview_bytes) : t('settings.calculating')}</span>
          {#if maint?.webview_pending}
            <span class="pending" role="status">{t('settings.maint.webview_pending')}</span>
            <button class="btn subtle" onclick={() => webview(false)}>{t('settings.maint.webview_keep')}</button>
          {:else}
            <button class="btn" disabled={!maint} onclick={() => webview(true)}>{t('settings.maint.webview_clear')}</button>
          {/if}
        </div>
        <p class="help">{t('settings.maint.webview_help')}</p>
      </div>
    </div>
    <div class="group">
      <span class="tag">{t('settings.maint.workshop')}</span>
      <div class="with-help">
        <div class="path-row">
          <span class="num size">{maint ? size(maint.workshop_bytes) : t('settings.calculating')}</span>
          <button class="btn" disabled={!maint || maint.workshop_bytes === 0} onclick={() => clear('workshop')}>{t('settings.maint.clear')}</button>
        </div>
        <p class="help">{t('settings.maint.workshop_help')}</p>
      </div>
    </div>
    <div class="group">
      <span class="tag">{t('settings.logs')}</span>
      <div class="with-help">
        <div class="path-row">
          <span class="num size">{maint ? size(maint.logs_bytes) : t('settings.calculating')}</span>
          <button class="btn" onclick={openLogs}><FolderOpen size={14} strokeWidth={1.75} />{t('settings.logs_open')}</button>
          <button class="btn subtle" disabled={!maint} onclick={() => clear('logs')}>{t('settings.maint.logs_clear')}</button>
        </div>
        <p class="help">{t('settings.logs_help')}</p>
      </div>
    </div>

    <h2>{t('settings.section.about')}</h2>
    <div class="group">
      <span class="tag">{t('settings.version')}</span>
      <div class="row-disclaimer">
        <span class="value mono">{__VERSION__}</span>
        {#if app.update}
          {@const u = app.update}
          <button class="btn" onclick={() => api.openUrl(u.url).catch((e) => app.notify(message(e), 'error'))}>{t('update.download', { v: u.version })}</button>
        {:else}
          <button class="btn subtle" onclick={() => app.checkUpdate(true)}>{t('update.check_now')}</button>
        {/if}
        <label class="check-start"><input type="checkbox" checked={app.updateCheck} onchange={(e) => app.setUpdateCheck(e.currentTarget.checked)} />{t('update.check')}</label>
      </div>
    </div>
    <div class="group">
      <span class="tag">{t('settings.disclaimer')}</span>
      <div class="row-disclaimer">
        <button class="btn" onclick={() => (app.disclaimerOpen = true)}>{t('settings.disclaimer_see')}</button>
        <label class="check-start"><input type="checkbox" checked={app.disclaimerAtStart} onchange={(e) => app.setDisclaimerAtStart(e.currentTarget.checked)} />{t('settings.disclaimer_start')}</label>
      </div>
    </div>
  </div>
</section>

<style>
  .screen {
    height: 100%;
    overflow: auto;
  }
  .head {
    padding: 14px 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  h1 {
    font-size: var(--t-xl);
  }
  .content {
    display: grid;
    gap: 14px;
    padding: 18px 16px 32px;
  }
  h2 {
    margin: 12px 0 0;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
    font-size: var(--t-m);
    font-weight: 600;
    color: var(--ink);
  }
  h2:first-child {
    margin-top: 0;
  }
  .new {
    display: grid;
    gap: 14px;
  }
  .group {
    display: grid;
    grid-template-columns: 220px minmax(0, auto);
    justify-content: start;
    align-items: start;
    gap: 14px;
  }
  .value {
    line-height: 28px;
    font-size: 12px;
    min-width: 4ch;
  }
  .tag {
    color: var(--ink-2);
    font-size: var(--t-s);
    line-height: 28px;
  }
  .with-help {
    display: grid;
    gap: 4px;
    justify-items: start;
    min-width: 0;
  }
  .help {
    margin: 0;
    max-width: 62ch;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .help.lead {
    margin-bottom: -2px;
  }
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    font-size: var(--t-s);
  }
  .path-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .path {
    max-width: 52ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    line-height: 28px;
  }
  .field.wide {
    width: 52ch;
    max-width: 100%;
  }
  .size {
    min-width: 8ch;
    font-size: var(--t-s);
    line-height: 28px;
  }
  .pending {
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .ok {
    color: var(--ok);
    font-size: var(--t-s);
  }
  .error {
    color: var(--danger);
    font-size: var(--t-s);
  }
  .segmentos {
    display: inline-flex;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    overflow: hidden;
  }
  .segmentos button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-right: 1px solid var(--line-strong);
    background: var(--surface);
    font-size: var(--t-s);
    cursor: pointer;
  }
  .segmentos button:last-child {
    border-right: 0;
  }
  .segmentos button[aria-checked='true'] {
    background: var(--accent-fill);
    color: var(--accent-ink);
  }
  .row-scale {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .stepper {
    display: inline-flex;
    align-items: stretch;
    height: 40px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    overflow: hidden;
    background: var(--surface);
  }
  .stepper > button {
    display: grid;
    place-items: center;
    width: 34px;
    border: 0;
    background: var(--surface);
    color: var(--ink);
    cursor: pointer;
  }
  .stepper > button:first-child {
    border-right: 1px solid var(--line-strong);
  }
  .stepper > button:last-child {
    border-left: 1px solid var(--line-strong);
  }
  .stepper > button:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .stepper > button:disabled {
    color: var(--ink-3);
    cursor: default;
  }
  .dial {
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 3px;
    padding: 0 12px;
    outline: none;
  }
  .dial:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .reading {
    font-size: var(--t-m);
    font-weight: 600;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .ruler {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 8px;
  }
  .tick {
    width: 2px;
    height: 4px;
    padding: 0;
    border: 0;
    background: var(--line-strong);
    cursor: pointer;
  }
  .tick.major {
    height: 7px;
  }
  .tick.on {
    width: 3px;
    height: 8px;
    background: var(--accent);
  }
  .palettes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .palettes button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
    padding: 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    color: var(--ink-2);
    font: inherit;
    font-size: var(--t-s);
    cursor: pointer;
  }
  .palettes button[aria-checked='true'] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--ink);
    font-weight: 600;
  }
  .swatch {
    display: grid;
    grid-template-columns: 1fr 2fr 12px;
    width: 96px;
    height: 28px;
    border: 1px solid var(--line);
    border-radius: 3px;
    overflow: hidden;
  }
  .row-disclaimer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
  }
  .check-start {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .check-start input {
    accent-color: var(--accent);
  }
</style>
