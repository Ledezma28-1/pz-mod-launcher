
import type {
  Layout,
  SaveGap,
  BisectAnswer,
  ProfileSettings,
  AppSettingsData,
  FoldersInfo,
  MaintenanceInfo,
  Issue,
  ConsoleLines,
  Imported,
  Featured,
  SandboxSchema,
  SandboxTexts,
  SandboxValue,
  SandboxValues,
  Export,
  WorkshopCollection,
  WorkshopQuery,
  WorkshopState,
  WorkshopDetails,
  WorkshopPage,
  Backup,
  ModDetails,
  Diagnosis,
  BisectView,
  Environment,
  SessionState,
  LibraryMod,
  OrderProposal,
  SandboxProgress,
  ViewProgress,
  ProbeReport,
  Removed,
  ProfileSummary,
  ProfileView,
  SteamProfile,
  GameListChange,
} from './types';
import { translateError } from './errors';
import { i18n } from './i18n.svelte';

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type Args = Record<string, unknown>;

async function call<T>(cmd: string, args?: Args): Promise<T> {
  if (inTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  const { demo } = await import('./demo');
  return demo<T>(cmd, args ?? {});
}

export const api = {
  environment: () => call<Environment>('environment'),
  chooseGame: (path: string) => call<void>('choose_game', { path }),
  rescan: () => call<number>('rescan'),
  profiles: () => call<ProfileSummary[]>('profiles'),
  profilesUnreadable: () => call<[string, string, string][]>('profiles_unreadable'),
  createProfile: (name: string, doImport: boolean) => call<ProfileSummary>('profile_create', { name, doImport }),
  renameProfile: (id: string, name: string) => call<void>('profile_rename', { id, name }),
  deleteProfile: (id: string) => call<void>('profile_delete', { id }),
  view: (id: string) => call<ProfileView>('profile_view', { id }),
  counts: (id: string) => call<ProfileView['counts']>('profile_count', { id }),
  duplicateProfile: (id: string, name: string) => call<ProfileSummary>('profile_duplicate', { id, name }),
  shareProfile: (id: string) => call<string>('profile_share', { id }),
  importProfile: (text: string) => call<Imported>('profile_import', { text }),
  modTrash: (key: string) => call<void>('mod_trash', { key }),
  logsFolder: () => call<string>('logs_folder'),
  settingsGet: () => call<AppSettingsData>('settings_get'),
  settingsSet: (s: AppSettingsData) => call<void>('settings_set', { s }),
  foldersInfo: () => call<FoldersInfo>('folders_info'),
  maintenanceInfo: () => call<MaintenanceInfo>('maintenance_info'),
  maintenanceClear: (what: 'workshop' | 'logs') => call<number>('maintenance_clear', { what }),
  maintenanceWebview: (pending: boolean) => call<void>('maintenance_webview', { pending }),
  consoleRead: (id: string) => call<string[]>('console_read', { id }),
  consoleIssues: (id: string) => call<Issue[]>('console_issues', { id }),
  consoleAnalyze: (text: string) => call<Issue[]>('console_analyze', { text }),
  consolePath: (id: string) => call<string>('console_path', { id }),
  setEnabled: (id: string, modId: string, enabled: boolean) => call<ProfileView>('profile_set_enabled', { id, modId, enabled }),
  setEnabledMany: (id: string, modIds: string[], enabled: boolean) => call<number>('profile_set_enabled_many', { id, modIds, enabled }),
  gameList: (id: string) => call<GameListChange | null>('profile_game_list', { id }),
  savesGaps: (id: string) => call<SaveGap[]>('saves_gaps', { id }),
  savesAdd: (id: string, dirs: string[]) => call<number>('saves_add', { id, dirs }),
  savesRestore: (id: string, dirs: string[]) => call<void>('saves_restore', { id, dirs }),
  savesIgnore: (id: string, dirs: string[]) => call<void>('saves_ignore', { id, dirs }),
  takeGameList: (id: string) => call<void>('profile_take_game_list', { id }),
  discardGameList: (id: string) => call<void>('profile_discard_game_list', { id }),
  pin: (id: string, modId: string, pinned: boolean) => call<void>('profile_pin', { id, modId, pinned }),
  moveTo: (id: string, from: number, to: number, section: string | null) =>
    call<Layout>('profile_move', { id, from, to, section }),
  sectionCreate: (id: string, name: string, atMod: string | null) => call<Layout>('section_create', { id, name, atMod }),
  sectionRename: (id: string, old: string, name: string) => call<Layout>('section_rename', { id, old, new: name }),
  sectionMove: (id: string, name: string, to: number) => call<Layout>('section_move', { id, name, to }),
  sectionDelete: (id: string, name: string) => call<Layout>('section_delete', { id, name }),
  sectionAssign: (id: string, modIds: string[], name: string | null, first = false) =>
    call<Layout>('section_assign', { id, modIds, name, first }),
  sectionsFromCategories: (id: string, names: Record<string, string>) =>
    call<Layout>('sections_from_categories', { id, names }),
  restoreLayout: (id: string, layout: Layout) => call<void>('profile_restore_layout', { id, layout }),
  add: (id: string, keys: string[], enabled = true, section: string | null = null) =>
    call<void>('profile_add', { id, keys, enabled, section }),
  remove: (id: string, modId: string) => call<void>('profile_remove', { id, modId }),
  removeMany: (id: string, modIds: string[]) => call<Removed[]>('profile_remove_many', { id, modIds }),
  putBack: (id: string, removed: Removed[]) => call<void>('profile_put_back', { id, removed }),
  win: (id: string, modId: string, rivals: string[]) => call<void>('profile_win', { id, modId, rivals }),
  accept: (id: string, pairs: [string, string][]) => call<void>('profile_accept', { id, pairs }),
  settings: (id: string, settings: ProfileSettings) => call<void>('profile_settings', { id, settings }),
  sort: (id: string, apply: boolean, section: string | null = null) =>
    call<OrderProposal>('profile_sort', { id, apply, section }),
  doExport: (id: string) => call<Export>('profile_export', { id }),
  sandboxSchema: (id: string, language: string) => call<SandboxSchema>('sandbox_schema', { id, language }),
  sandboxValues: (id: string, language: string, origin: string) => call<SandboxValues>('sandbox_values', { id, language, origin }),
  sandboxTexts: (id: string, language: string, sandbox: Record<string, SandboxValue>, server: Record<string, SandboxValue>) =>
    call<SandboxTexts>('sandbox_texts', { id, language, sandbox, server }),
  sandboxSave: (
    id: string,
    language: string,
    name: string,
    sandbox: Record<string, SandboxValue>,
    server: Record<string, SandboxValue>,
    asServer: boolean,
    asPreset: boolean,
  ) => call<string[]>('sandbox_save', { id, language, name, sandbox, server, asServer, asPreset }),
  saveMaps: (id: string, maps: string[]) => call<void>('profile_maps', { id, maps }),
  library: () => call<LibraryMod[]>('library'),
  detail: (key: string) => call<ModDetails>('mod_details', { key }),
  launch: (id: string) => call<number>('launch', { id }),
  profileTouch: (id: string) => call<void>('profile_touch', { id }),
  featuredRead: () => call<Featured | null>('featured_read'),
  featuredRefresh: () => call<Featured>('featured_refresh'),
  stop: () => call<void>('session_stop'),
  sessionLastDiagnosis: (id: string) => call<Diagnosis | null>('session_last_diagnosis', { id }),
  sessionReport: (id: string) => call<string>('session_report', { id }),
  bisectStart: (id: string, name: string, flaky: boolean) => call<BisectView>('bisect_start', { id, name, flaky }),
  bisectAnswer: (id: string, answer: BisectAnswer, round: number) =>
    call<BisectView>('bisect_answer', { id, answer, round }),
  bisectUndo: (id: string) => call<BisectView>('bisect_undo', { id }),
  bisectCancel: (id: string) => call<void>('bisect_cancel', { id }),
  bisectOpen: () => call<BisectView | null>('bisect_open'),
  installZb: (channel: 'estable' | 'beta') => call<string>('zb_install', { channel }),
  uninstallZb: () => call<string>('zb_uninstall'),
  backups: () => call<Backup[]>('backups_list'),
  restore: (name: string) => call<number>('backup_restore', { name }),
  openWorkshop: (id: number) => call<void>('open_workshop', { id }),
  openWorkshopWeb: () => call<void>('open_workshop_web'),
  openUrl: (url: string) => call<void>('open_url', { url }),
  steamProfile: () => call<SteamProfile | null>('steam_profile'),
  probeReport: (id: string) => call<ProbeReport | null>('probe_report', { id }),
  workshopTags: () => call<string[]>('workshop_tags'),
  workshopSearch: (query: WorkshopQuery) => call<WorkshopPage>('workshop_search', { query }),
  workshopDetails: (ids: number[], complete: boolean) => call<WorkshopDetails[]>('workshop_details', { ids, complete }),
  workshopCollection: (id: number) => call<WorkshopCollection>('workshop_collection', { id }),
  workshopStates: (ids: number[]) => call<WorkshopState[]>('workshop_states', { ids }),
  workshopCheck: (ids: number[]) => call<[number, number][]>('workshop_check', { ids }),
  workshopCacheRead: () => call<string | null>('workshop_cache_read'),
  workshopCacheWrite: (data: string) => call<void>('workshop_cache_write', { data }),
  workshopSubscribe: (ids: number[]) => call<[number, string][]>('workshop_subscribe', { ids }),
  workshopWaiting: () => call<number[]>('workshop_waiting'),
  workshopDownload: (ids: number[]) => call<number[]>('workshop_download', { ids }),
  workshopUnsubscribe: (id: number) => call<void>('workshop_unsubscribe', { id }),
  workshopIdle: (seconds: number) => call<void>('workshop_idle', { seconds }),
  openFolder: (path: string) => call<void>('open_folder', { path }),
};

export async function onSession(f: (e: SessionState) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<SessionState>('session', (ev) => f(ev.payload));
}

export async function onConsole(f: (e: ConsoleLines) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<ConsoleLines>('console', (ev) => f(ev.payload));
}

export async function onSandboxProgress(f: (p: SandboxProgress) => void): Promise<() => void> {
  if (!inTauri) {
    const n = Number(new URLSearchParams(location.search).get('sbprogress'));
    if (n) setTimeout(() => f({ stage: n < 3 ? 'sandbox' : n < 8 ? 'mods' : 'texts', done: n, total: 10 }), 100);
    return () => {};
  }
  const { listen } = await import('@tauri-apps/api/event');
  return listen<SandboxProgress>('sandbox-progress', (ev) => f(ev.payload));
}

export async function onViewProgress(f: (p: ViewProgress) => void): Promise<() => void> {
  if (!inTauri) {
    const n = Number(new URLSearchParams(location.search).get('viewprogress'));
    if (n) setInterval(() => f({ profile: '', stage: 'files', done: n, total: 678 }), 250);
    return () => {};
  }
  const { listen } = await import('@tauri-apps/api/event');
  return listen<ViewProgress>('view-progress', (ev) => f(ev.payload));
}

export async function onChangedMods(f: () => void): Promise<() => void> {
  if (!inTauri) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen('mods-changed', () => f());
}

export function message(e: unknown): string {
  const s = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
  return translateError(s, i18n.language);
}
