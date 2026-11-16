
import { onConsole, onChangedMods, onSession, onViewProgress, api, inTauri, message } from './api';
import { fixMod, t } from './i18n.svelte';
import { steam } from './steam.svelte';
import { channelOf, shouldInstall, updateToNotify, type Channel } from './zb-flow';
import { PALETTES, type Palette } from './palettes';
import { DISCLAIMER_VERSION, shouldShow } from './disclaimer';
import { justUpdated, latestRelease, releaseUrl, type Release } from './update';
import type { SteamProfile, AppSettingsData, BisectAnswer, BisectView, Fix, Environment, GameListChange, Issue, SessionState, OrderProposal, ProfileSummary, ProfileView, ViewProgress, WorkshopState } from './types';

export const ZB_IDS: Record<Channel, number> = { estable: 3619862853, beta: 3812624292 };

export type Screen = 'home' | 'help' | 'profile' | 'library' | 'workshop' | 'zb' | 'install' | 'settings';
export type Tab = 'order' | 'conflicts' | 'export' | 'sandbox' | 'console' | 'settings';
export type Theme = 'system' | 'light' | 'dark';
const STORED_THEME: Record<Theme, string> = { system: 'sistema', light: 'claro', dark: 'oscuro' };

class State {
  environment = $state<Environment | null>(null);
  errorEnvironment = $state<string | null>(null);
  loadingEnvironment = $state(true);

  profiles = $state<ProfileSummary[]>([]);
  bisect = $state<BisectView | null>(null);
  settings = $state<AppSettingsData | null>(null);
  bisectPlayed = $state<number | null>(null);
  unreadable = $state<[string, string, string][]>([]);
  profileId = $state<string | null>(null);
  view = $state<ProfileView | null>(null);
  loadingView = $state(false);
  viewProgress = $state<ViewProgress | null>(null);
  errorView = $state<string | null>(null);
  proposal = $state<OrderProposal | null>(null);

  selection = $state<string | null>(null);
  screen = $state<Screen>('home');
  goWorkshop = $state<{ id: number | null; collection: boolean } | null>(null);
  tab = $state<Tab>('order');

  session = $state<SessionState | null>(null);
  gameList = $state<GameListChange | null>(null);
  versionMods = $state(0);
  starting = $state(false);
  taken = $state(false);

  state = $state<{
    text: string;
    kind: 'ok' | 'error';
    undo?: () => Promise<unknown>;
    action?: { text: string; f: () => Promise<unknown> };
  } | null>(null);
  theme = $state<Theme>(savedTheme());
  palette = $state<Palette>(savedPalette());
  scale = $state(savedScale());
  steamIdle = $state(savedIdle());

  console = $state.raw<{ profile: string | null; lines: string[]; dropped: number }>({ profile: null, lines: [], dropped: 0 });
  errors = $state.raw<{ profile: string | null; list: Issue[] }>({ profile: null, list: [] });
  consolePanel = $state(readBool('panel-consola'));
  zbPending = $state<{ channel: Channel; since: number; from: string | null } | null>(null);
  zbError = $state<{ channel: Channel; text: string } | null>(null);
  zbAskUninstall = $state<Channel | null>(null);
  zbLocal = $state<Record<number, WorkshopState>>({});
  steamProfile = $state<SteamProfile | null>(null);
  disclaimerAtStart = $state(shouldShow(readText('disclaimer-hidden')));
  disclaimerOpen = $state(inTauri && shouldShow(readText('disclaimer-hidden')));
  update = $state<Release | null>(null);
  updateCheck = $state(readText('update-check') !== 'off');
  private zbTimer: ReturnType<typeof setInterval> | null = null;
  private reread: ReturnType<typeof setTimeout> | null = null;

  row = $derived(this.view?.rows.find((f) => f.id === this.selection) ?? null);

  async start() {
    applyTheme(this.theme);
    applyPalette(this.palette);
    applyScale(this.scale);
    if (!inTauri && new URLSearchParams(location.search).get('update')) this.update = { version: '0.2.0', url: 'https://github.com/Ledezma28-1/pz-mod-launcher/releases' };
    if (inTauri && this.updateCheck) setTimeout(() => void this.checkUpdate(false), 4000);
    const seen = !inTauri && new URLSearchParams(location.search).get('updated') ? '0.0.1' : readText('seen-version');
    if (justUpdated(seen, __VERSION__)) {
      this.notify(t('update.updated', { v: __VERSION__ }), 'ok', undefined, { text: t('update.whats_new'), f: () => api.openUrl(releaseUrl(__VERSION__)) });
    }
    writeText('seen-version', __VERSION__);
    api.workshopIdle(this.steamIdle).catch(() => {});
    await onSession((p) => {
      this.session = p;
      this.starting = false;
      if (p.finished) void this.loadSteamProfile();
      if (!p.finished && this.bisect?.profile === p.profile) this.bisectPlayed = this.bisect.round;
      if (p.finished) this.rereadErrors(p.profile, 0);
      if (p.finished && p.profile === this.profileId) void this.loadGameList();
      if (p.finished && p.integrity_skipped) this.notify(t('session.integrity_off'));
      if (p.finished && p.integrity) {
        if (p.integrity.total === 0) this.notify(t('session.intact'));
        else this.notify(t('session.changes', { n: p.integrity.total, e: p.integrity.examples[0] ?? '' }), 'error');
      }
      const d = p.diagnosis;
      if (p.finished && d && d.end !== 'normal' && d.end !== 'stopped_by_user') {
        this.notify(t(`diag.end.${d.end}`, { c: d.code ?? '?' }), 'error', undefined, {
          text: t('diag.show'),
          f: async () => {
            await this.chooseProfile(p.profile);
            this.tab = 'console';
          },
        });
      }
    });
    await onConsole((c) => {
      const same = this.console.profile === c.profile && !c.reset;
      const lines = same ? this.console.lines.concat(c.lines) : c.lines;
      const cut = Math.max(0, lines.length - MAX_LINES);
      const dropped = (same ? this.console.dropped : 0) + cut;
      this.console = { profile: c.profile, lines: cut ? lines.slice(cut) : lines, dropped };
      if (!same) this.errors = { profile: c.profile, list: [] };
      if (c.lines.some((l) => l.includes('ERROR') || l.includes('flushErrorMessage') || l.includes('require('))) this.rereadErrors(c.profile, 1500);
    });
    await onViewProgress((p) => {
      if (!inTauri || (this.loadingView && p.profile === this.profileId)) this.viewProgress = p;
    });
    await onChangedMods(() => {
      this.versionMods++;
      if (this.view) void this.loadView();
      void this.addDownloaded();
      void this.zbAfterChange();
    });
    await this.loadEnvironment();
    this.zbResume();
    void this.zbAfterChange();
    void this.loadSteamProfile();
    if (this.environment) void steam.preload();
  }

  async loadEnvironment() {
    this.loadingEnvironment = true;
    this.errorEnvironment = null;
    try {
      this.environment = await api.environment();
      await this.loadProfiles();
      void this.loadBisect();
      void this.loadSettings();
    } catch (e) {
      this.errorEnvironment = message(e);
    } finally {
      this.loadingEnvironment = false;
    }
  }

  async loadProfiles() {
    this.profiles = await api.profiles();
    api
      .profilesUnreadable()
      .then((v) => (this.unreadable = v))
      .catch(() => {});
    if (!this.profileId || !this.profiles.some((p) => p.id === this.profileId)) {
      const recent = [...this.profiles].sort((a, b) => (b.last_use ?? 0) - (a.last_use ?? 0))[0];
      this.profileId = recent?.id ?? null;
    }
    if (this.profileId) await this.loadView();
    else this.view = null;
  }

  async chooseProfile(id: string, display = true) {
    if (display) this.screen = 'profile';
    this.markUsed(id);
    if (id === this.profileId && this.view) return;
    this.profileId = id;
    this.selection = null;
    this.proposal = null;
    await this.loadView();
  }

  async loadView() {
    const id = this.profileId;
    if (!id) return;
    this.loadingView = !this.view || this.view.id !== id;
    this.viewProgress = null;
    this.errorView = null;
    void this.loadGameList();
    try {
      const v = await api.view(id);
      if (id !== this.profileId) return;
      this.view = v;
      const r = this.profiles.find((p) => p.id === id);
      if (r) {
        r.active = v.counts.active;
        r.total = v.rows.length;
      }
    } catch (e) {
      if (id === this.profileId) this.errorView = message(e);
    } finally {
      if (id === this.profileId) {
        this.loadingView = false;
        this.viewProgress = null;
      }
    }
  }

  async loadGameList() {
    const id = this.profileId;
    if (!id) return;
    const c = await api.gameList(id).catch(() => null);
    if (id === this.profileId) this.gameList = c;
  }

  async run(f: () => Promise<unknown>, success?: string) {
    this.taken = true;
    try {
      await f();
      if (success) this.notify(success);
      await this.loadView();
    } catch (e) {
      this.notify(message(e), 'error');
    } finally {
      this.taken = false;
    }
  }

  async runView(f: () => Promise<ProfileView>, success?: string) {
    this.taken = true;
    const id = this.profileId;
    try {
      const v = await f();
      if (success) this.notify(success);
      if (id === this.profileId && v.id === id) {
        this.view = v;
        const r = this.profiles.find((p) => p.id === id);
        if (r) {
          r.active = v.counts.active;
          r.total = v.rows.length;
        }
      } else await this.loadView();
    } catch (e) {
      this.notify(message(e), 'error');
    } finally {
      this.taken = false;
    }
  }

  closeDisclaimer(hide: boolean) {
    this.disclaimerOpen = false;
    if (hide) this.setDisclaimerAtStart(false);
  }

  async checkUpdate(manual: boolean) {
    try {
      this.update = await latestRelease(__VERSION__);
      if (manual && !this.update) this.notify(t('update.latest'));
    } catch (e) {
      if (manual) this.notify(t('update.error', { e: message(e) }), 'error');
    }
  }

  setUpdateCheck(on: boolean) {
    this.updateCheck = on;
    writeText('update-check', on ? 'on' : 'off');
  }

  setDisclaimerAtStart(show: boolean) {
    this.disclaimerAtStart = show;
    try {
      if (show) localStorage.removeItem('disclaimer-hidden');
      else localStorage.setItem('disclaimer-hidden', String(DISCLAIMER_VERSION));
    } catch {
    }
  }

  async loadSteamProfile() {
    try {
      this.steamProfile = await api.steamProfile();
    } catch {
      this.steamProfile = null;
    }
  }

  async zbSubscribeInstall(channel: Channel) {
    this.zbError = null;
    try {
      await api.workshopSubscribe([ZB_IDS[channel]]);
    } catch (e) {
      this.zbError = { channel, text: message(e) };
      return;
    }
    const from = this.environment?.zb.workshop.find((c) => c.channel === channel)?.version ?? null;
    this.zbPending = { channel, since: Date.now(), from };
    writeText('zb-pending', JSON.stringify(this.zbPending));
    this.zbTimer ??= setInterval(() => void this.zbAfterChange(), 2000);
    void this.zbAfterChange();
  }

  private zbResume() {
    try {
      const p = JSON.parse(readText('zb-pending') ?? 'null') as { channel: Channel; since: number; from: string | null } | null;
      if (!p || Date.now() - p.since > 86_400_000) {
        writeText('zb-pending', '');
        return;
      }
      this.zbPending = { ...p, since: Date.now() };
      this.zbTimer ??= setInterval(() => void this.zbAfterChange(), 2000);
    } catch {
      writeText('zb-pending', '');
    }
  }

  private zbStop() {
    if (this.zbTimer) clearInterval(this.zbTimer);
    this.zbTimer = null;
    this.zbPending = null;
    writeText('zb-pending', '');
  }

  async zbStates() {
    try {
      const s = await api.workshopStates(Object.values(ZB_IDS));
      this.zbLocal = Object.fromEntries(s.map((x) => [x.id, x]));
    } catch {
    }
  }

  async zbAfterChange() {
    if (!this.environment) return;
    try {
      this.environment = await api.environment();
    } catch {
      return;
    }
    const zb = this.environment.zb;
    const p = this.zbPending;
    if (p) {
      if (shouldInstall(p, zb.workshop, this.zbLocal[ZB_IDS[p.channel]] ?? null)) {
        this.zbStop();
        await this.zbInstall(p.channel);
      } else if (Date.now() - p.since > 180_000) {
        this.zbStop();
        this.zbError = { channel: p.channel, text: t('zb.timeout') };
      } else void this.zbStates();
    }
    const now = this.environment.zb;
    const from = channelOf(now.in_game, now.workshop, readText('zb-channel'));
    const v = updateToNotify(now.in_game, from, now.workshop, readText('zb-notified'));
    if (v && from && !this.zbPending) {
      writeText('zb-notified', v);
      this.notify(t('zb.update_available', { v }), 'ok', undefined, { text: t('zb.update', { v }), f: () => this.zbInstall(from) });
    }
  }

  async zbInstall(channel: Channel) {
    this.zbError = null;
    try {
      const v = await api.installZb(channel);
      writeText('zb-channel', channel);
      this.notify(t('zb.installed_ok', { v }));
      this.environment = await api.environment();
    } catch (e) {
      this.zbError = { channel, text: message(e) };
    }
  }

  async zbUnsubscribe(channel: Channel) {
    this.zbError = null;
    const zb = this.environment?.zb;
    const installed = !!zb && channelOf(zb.in_game, zb.workshop, readText('zb-channel')) === channel;
    try {
      await api.workshopUnsubscribe(ZB_IDS[channel]);
    } catch (e) {
      this.zbError = { channel, text: message(e) };
      return;
    }
    if (this.zbPending?.channel === channel) this.zbStop();
    await this.zbStates();
    if (installed) this.zbAskUninstall = channel;
  }

  notify(text: string, kind: 'ok' | 'error' = 'ok', undo?: () => Promise<unknown>, action?: { text: string; f: () => Promise<unknown> }) {
    this.state = { text, kind, undo, action };
  }

  async acceptAction() {
    const a = this.state?.action;
    if (!a) return;
    this.state = null;
    try {
      await a.f();
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async undo() {
    const f = this.state?.undo;
    if (!f) return;
    this.state = null;
    await this.run(f);
  }

  markUsed(id: string) {
    const r = this.profiles.find((p) => p.id === id);
    if (r) r.last_use = Math.floor(Date.now() / 1000);
    api.profileTouch(id).catch(() => {});
  }

  async loadConsole(profile: string) {
    const alive = this.session && !this.session.finished && this.session.profile === profile;
    if (alive && this.console.profile === profile) return;
    if (this.console.profile === profile && this.errors.profile === profile) return;
    try {
      const [lines, list] = await Promise.all([api.consoleRead(profile), api.consoleIssues(profile)]);
      this.console = { profile, lines, dropped: 0 };
      this.errors = { profile, list };
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  private rereadErrors(profile: string, wait: number) {
    if (this.reread && wait > 0) return;
    if (this.reread) clearTimeout(this.reread);
    this.reread = setTimeout(async () => {
      this.reread = null;
      try {
        const list = await api.consoleIssues(profile);
        const before = this.errors.profile === profile ? this.errors.list : [];
        const k = (e: Issue) => `${e.kind}|${e.mod_name}|${e.message}|${e.file}|${e.line_file}`;
        const seen = new Set(before.map(k));
        const fresh = list.filter((e) => e.severity === 'error' && !seen.has(k(e)));
        this.errors = { profile, list };
        if (fresh.length && this.session && !this.session.finished) {
          const e = fresh[fresh.length - 1];
          this.notify(
            fresh.length === 1
              ? t('console.new_error', { m: e.mod_name ?? t('console.no_mod'), e: e.message })
              : t('console.new_errors', { n: fresh.length, m: e.mod_name ?? t('console.no_mod') }),
            'error',
          );
        }
      } catch {
      }
    }, wait);
  }

  toggleConsolePanel() {
    this.consolePanel = !this.consolePanel;
    try {
      localStorage.setItem('panel-consola', this.consolePanel ? '1' : '0');
    } catch {
    }
  }

  private toAdd = new Map<number, string>();

  addWhenDownloads(ids: number[], profile: string) {
    for (const id of ids) this.toAdd.set(id, profile);
    void this.addDownloaded();
  }

  private async addDownloaded() {
    if (this.toAdd.size === 0) return;
    try {
      const lib = await api.library();
      for (const [ws, profile] of [...this.toAdd]) {
        const mods = lib.filter((m) => m.workshop_id === ws && m.id);
        if (mods.length === 0 || mods.some((m) => m.pending)) continue;
        try {
          await api.add(profile, mods.map((m) => m.key));
          this.toAdd.delete(ws);
          const name = this.profiles.find((p) => p.id === profile)?.name ?? '';
          this.notify(t('ws.added_auto', { m: mods[0].name, p: name }));
          if (profile === this.profileId) await this.loadView();
        } catch (e) {
          this.notify(message(e), 'error');
        }
      }
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async importProfile(text: string) {
    try {
      const r = await api.importProfile(text);
      this.profileId = r.profile.id;
      this.markUsed(r.profile.id);
      await this.loadProfiles();
      this.screen = 'profile';
      const ignored = r.ignored.length ? `${t('share.ignored', { a: r.ignored.join(' ') })} ` : '';
      if (r.missing.length) {
        this.notify(ignored + t('share.missing_many', { n: r.missing.length }), r.ignored.length ? 'error' : 'ok', undefined, {
          text: t('ws.subscribe'),
          f: async () => {
            const failures = await api.workshopSubscribe(r.missing);
            this.addWhenDownloads(r.missing.filter((id) => !failures.some(([f]) => f === id)), r.profile.id);
            this.notify(failures.length ? failures.map(([id, m]) => `${id}: ${m}`).join(' · ') : t('share.subscribed', { n: r.missing.length }), failures.length ? 'error' : 'ok');
          },
        });
      } else {
        this.notify(ignored + t('share.imported', { p: r.profile.name }), r.ignored.length ? 'error' : 'ok');
      }
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async duplicateProfile(id: string, name: string) {
    try {
      const p = await api.duplicateProfile(id, t('profile.copy_of', { n: name }));
      this.profileId = p.id;
      this.markUsed(p.id);
      await this.loadProfiles();
      this.screen = 'profile';
      this.notify(t('profile.duplicated', { n: p.name }));
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  openInWorkshop(id: number | null, collection: boolean) {
    this.goWorkshop = { id, collection };
    this.screen = 'workshop';
  }

  async createProfile(name: string, doImport: boolean) {
    try {
      const p = await api.createProfile(name, doImport);
      this.profileId = p.id;
      this.markUsed(p.id);
      await this.loadProfiles();
      this.screen = 'profile';
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async play() {
    if (!this.profileId) return;
    this.markUsed(this.profileId);
    this.starting = true;
    this.session = null;
    try {
      await api.launch(this.profileId);
      if (this.bisect?.profile === this.profileId) this.bisectPlayed = this.bisect.round;
    } catch (e) {
      this.starting = false;
      this.notify(message(e), 'error');
    }
  }

  async loadSettings() {
    try {
      this.settings = await api.settingsGet();
    } catch {
      this.settings = null;
    }
  }

  async saveSettings(s: AppSettingsData): Promise<boolean> {
    try {
      await api.settingsSet(s);
      this.settings = s;
      return true;
    } catch (e) {
      this.notify(message(e), 'error');
      return false;
    }
  }

  async loadBisect() {
    try {
      this.bisect = await api.bisectOpen();
    } catch {
      this.bisect = null;
    }
  }

  async startBisect(origin: string, flaky = false) {
    const name = this.profiles.find((p) => p.id === origin)?.name ?? origin;
    try {
      this.bisect = await api.bisectStart(origin, t('bisect.profile_name', { p: name }), flaky);
      await this.loadProfiles();
      await this.chooseProfile(this.bisect.profile);
      this.tab = 'console';
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async answerBisect(answer: BisectAnswer) {
    if (!this.bisect) return;
    try {
      this.bisect = await api.bisectAnswer(this.bisect.profile, answer, this.bisect.round);
      await this.loadView();
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async undoBisect() {
    if (!this.bisect) return;
    try {
      this.bisect = await api.bisectUndo(this.bisect.profile);
      await this.loadView();
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  async endBisect() {
    const b = this.bisect;
    if (!b) return;
    try {
      await api.bisectCancel(b.profile);
      this.bisect = null;
      await this.loadProfiles();
      await this.chooseProfile(b.origin);
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  changeIdle(seconds: number) {
    this.steamIdle = seconds;
    api.workshopIdle(seconds).catch((e) => this.notify(message(e), 'error'));
    try {
      localStorage.setItem('steam-inactivo', String(seconds));
    } catch {
    }
  }

  async fix(a: Fix) {
    const id = this.profileId;
    if (!id) return;
    const done = (k: 'Enable' | 'Win' | 'Accept') => t(`conflict.done.${k}`, { d: fixMod(a) });
    switch (a.action) {
      case 'Enable':
        return this.runView(() => api.setEnabled(id, a.data, true), done('Enable'));
      case 'Disable':
        return this.runView(() => api.setEnabled(id, a.data, false));
      case 'OpenWorkshop':
        this.openInWorkshop(a.data, false);
        return;
      case 'UpdateZb':
        this.screen = 'zb';
        return;
      case 'Win':
        return this.run(() => api.win(id, a.data[0], a.data[1]), done('Win'));
      case 'Accept':
        return this.run(() => api.accept(id, a.data), done('Accept'));
      case 'Keep':
        return this.run(
          () => api.setEnabledMany(id, a.data[1], false),
          t('conflict.done.Keep', { d: a.data[0], r: a.data[1].map((x) => `«${x}»`).join(', ') }),
        );
      case 'RemoveCopy': {
        const ws = /^ws:(\d+)\//.exec(a.data);
        return this.run(() => (ws ? api.workshopUnsubscribe(Number(ws[1])) : api.modTrash(a.data)));
      }
    }
  }

  async disableMany(ids: string[]) {
    const id = this.profileId;
    if (!id || ids.length === 0) return;
    this.state = null;
    await this.run(() => api.setEnabledMany(id, ids, false));
    if ((this.state as { kind: string } | null)?.kind !== 'error') {
      this.notify(t('select.done_off', { n: ids.length }), 'ok', () => api.setEnabledMany(id, ids, true));
    }
  }

  async proposeOrder() {
    if (!this.profileId) return;
    try {
      this.proposal = await api.sort(this.profileId, false);
      this.tab = 'order';
    } catch (e) {
      this.notify(message(e), 'error');
    }
  }

  changeScale(e: number) {
    this.scale = Math.min(SCALE_MAX, Math.max(SCALE_MIN, e));
    applyScale(this.scale);
    try {
      localStorage.setItem('escala', String(this.scale));
    } catch {
    }
  }

  changePalette(p: Palette) {
    this.palette = p;
    applyPalette(p);
    writeText('palette', p);
  }

  changeTheme(t: Theme) {
    this.theme = t;
    applyTheme(t);
    try {
      localStorage.setItem('tema', STORED_THEME[t]);
    } catch {
    }
  }
}

const MAX_LINES = 20_000;

export function readText(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function writeText(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {
  }
}
function readBool(k: string): boolean {
  try {
    return localStorage.getItem(k) === '1';
  } catch {
    return false;
  }
}

function savedIdle(): number {
  try {
    const v = Number(localStorage.getItem('steam-inactivo'));
    if (Number.isFinite(v) && v >= 10) return v;
  } catch {
  }
  return 60;
}

export const SCALE_MIN = 0.9;
export const SCALE_MAX = 1.5;

function savedScale(): number {
  try {
    const v = Number(localStorage.getItem('escala'));
    if (Number.isFinite(v) && v >= SCALE_MIN && v <= SCALE_MAX) return v;
  } catch {
  }
  return 1;
}

function applyScale(e: number) {
  if (inTauri) {
    import('@tauri-apps/api/webview').then((m) => m.getCurrentWebview().setZoom(e)).catch(() => {});
  } else {
    document.documentElement.style.zoom = String(e);
  }
}

function savedTheme(): Theme {
  try {
    const t = localStorage.getItem('tema');
    const found = (Object.keys(STORED_THEME) as Theme[]).find((k) => STORED_THEME[k] === t);
    if (found) return found;
  } catch {
  }
  return 'system';
}

function savedPalette(): Palette {
  const p = readText('palette');
  return (PALETTES as readonly string[]).includes(p ?? '') ? (p as Palette) : 'launcher';
}
export function applyPalette(p: Palette) {
  const r = document.documentElement;
  if (p === 'launcher') delete r.dataset.palette;
  else r.dataset.palette = p;
}
const systemDark = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
let followed: Theme = 'system';
function applyTheme(t: Theme) {
  followed = t;
  const r = document.documentElement;
  if (t === 'system') delete r.dataset.theme;
  else r.dataset.theme = t;
  r.dataset.mode = t === 'system' ? (systemDark?.matches ? 'dark' : 'light') : t;
}
systemDark?.addEventListener('change', () => applyTheme(followed));

export const app = new State();
if (typeof document !== 'undefined') {
  applyTheme(app.theme);
  applyPalette(app.palette);
}
