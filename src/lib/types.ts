
export type Severity = 'Blocks' | 'Warning' | 'Info' | 'Detail';
export type RowSeverity = 'blocks' | 'warning' | 'info';
export type ModKind = 'java' | 'map' | 'vehicle' | 'lua';
export type MoveEffect = 'rule' | 'winner' | 'category';

export interface Environment {
  game: {
    folder: string;
    version: string;
    version_text: string;
    build: string | null;
    branch: string | null;
    java: string | null;
  };
  steam: string | null;
  zb: {
    in_game: string | null;
    dll: boolean;
    workshop: { channel: string; id: number; version: string | null }[];
    ref_json: boolean;
    ref_bat: boolean;
    ref_steam: string[];
    warning: string | null;
  };
  modifications: string[];
}

export interface ProfileSummary {
  id: string;
  name: string;
  active: number;
  total: number;
  last_use: number | null;
  sections?: string[];
}

export interface FeaturedItem {
  id: number;
  title: string;
  author: string | null;
  subscribers: number;
  preview_view: string;
  collection: boolean;
  children: number;
}

export interface Featured {
  parsed: number;
  mods: FeaturedItem[];
  collections: FeaturedItem[];
}

export interface ModEntry {
  key: string;
  id: string;
  enabled: boolean;
  pinned: boolean;
}

export type Removed = [number, ModEntry];

export interface Row {
  index: number;
  id: string;
  key: string;
  name: string;
  enabled: boolean;
  pinned: boolean;
  found: boolean;
  kind: ModKind;
  version_mod: string | null;
  source: 'workshop' | 'local' | '?';
  workshop_id: number | null;
  author: string | null;
  severity: RowSeverity | null;
  section: string | null;
}

export interface Edge {
  before: string;
  after: string;
  rule: 'Requires' | 'LoadAfter' | 'LoadBefore' | 'ZbFixed' | 'UserPinned' | 'FileRule' | 'UserOverride';
  is_hard: boolean;
  broken: boolean;
}

export type ApprovalState = 'Approved' | 'Denied' | 'Pending' | { TrustedAuthor: number };

export type FindingKind =
  | { kind: 'MissingDependency'; requires: string; inactive_installed: boolean }
  | { kind: 'ModNotFound' }
  | { kind: 'WontLoad'; because: string[] }
  | { kind: 'Cycle'; hard: boolean }
  | { kind: 'DuplicatedId'; keys: string[]; used: string | null }
  | { kind: 'RequireTypo'; spaced: string[]; semicolon: string[]; ignored: string[] }
  | { kind: 'Incompatible'; declared: [string, string][] }
  | { kind: 'GameVersion'; min: string | null; max: string | null; game: string }
  | { kind: 'OnlyB41' }
  | { kind: 'NoValidFolder'; game: string }
  | { kind: 'OverriddenFiles'; pairs: Pair[] }
  | { kind: 'OverridesGame'; files: string[]; total: number; behavior: number }
  | { kind: 'JavaLinkBroken'; missing: string[]; total: number; game: string }
  | { kind: 'LuaGlobalReplaced'; wins: string | null; functions: LuaFn[]; total: number; related: boolean; accepted: boolean; movable: boolean }
  | { kind: 'TiledefRepeated'; num: string }
  | { kind: 'ScriptRedefined'; wins: string; objects: string[]; total: number; identical: number; properties: string[]; related: boolean; accepted: boolean }
  | { kind: 'MapCellsShared'; maps: [string, string]; cells: number; sample: string[]; squares: number | null }
  | { kind: 'MapOverTown'; map: string; cells: number }
  | { kind: 'ZbNotInstalled' }
  | { kind: 'ZbVersion'; zb: string; ranges: [string | null, string | null][] }
  | { kind: 'ZbJarInvalid'; reason: string }
  | { kind: 'ZbJarMissing'; jar: string }
  | { kind: 'ZbRepeatedPackage'; pkg: string; load: string }
  | { kind: 'ZbIncompatibleGame'; zb: string; game: string; reason: string }
  | { kind: 'ZbApproval'; state: ApprovalState; policy: string }
  | { kind: 'ZbModNotEnabled' }
  | { kind: 'JavaPatchOverlap'; methods: string[]; replaced: string[] }
  | { kind: 'SectionOrder'; needs: string; section: string }
  | { kind: 'SectionSoftRule'; after: string; section: string }
  | { kind: 'ModifiedInstall'; detail: string };

export type Fix =
  | { action: 'Enable'; data: string }
  | { action: 'Disable'; data: string }
  | { action: 'OpenWorkshop'; data: number }
  | { action: 'UpdateZb' }
  | { action: 'Win'; data: [string, string[]] }
  | { action: 'Accept'; data: [string, string][] }
  | { action: 'RemoveCopy'; data: string }
  | { action: 'Keep'; data: [string, string[]] }
  | { action: 'MoveToSection'; data: [string, string] };

export interface ViewProgress {
  profile: string;
  stage: 'mods' | 'game' | 'files' | 'scripts' | 'lua' | 'conflicts';
  done: number;
  total: number;
}

export interface SandboxProgress {
  stage: 'server' | 'sandbox' | 'mods' | 'texts' | 'pages';
  done: number;
  total: number;
}

export interface Pair {
  loses: string;
  wins: string;
  files: string[];
  total: number;
  identical: number;
  not_load: string[];
  total_not_load: number;
  lost_objects: string[];
  total_lost: number;
  related: boolean;
  accepted: boolean;
}

export interface LuaFn {
  name: string;
  wins_at: string;
  remade: boolean;
  lost: { mod: string; at: string; wraps: boolean }[];
}

export interface Finding {
  severity: Severity;
  mods: string[];
  kind: FindingKind;
  fixes: Fix[];
}

export interface ProfileSettings {
  xmx: string;
  game_extra: string;
  policy: 'prompt' | 'deny-new' | 'allow-all';
  use_zb: 'auto' | 'si' | 'no';
  probe: boolean;
}

export interface ProbeReport {
  phase: 'boot' | 'start';
  total: number;
  mismatches: { name: string; predicted: string; seen: string | null }[];
}

export interface ProfileView {
  id: string;
  name: string;
  rows: Row[];
  findings: Finding[];
  edges: Edge[];
  settings: ProfileSettings;
  counts: { active: number; blocks: number; warning: number; info: number; detail: number };
  sections: string[];
}

export interface SaveGap {
  dir: string;
  name: string;
  mode: string;
  modified: number;
  missing: string[];
  with_map: string[];
}

export interface Layout {
  sections: string[];
  mods: [string, string | null][];
}

export interface OrderProposal {
  order: string[];
  moves: {
    id: string;
    from: number;
    to: number;
    reasons: { before: string; after: string; rule: Edge['rule'] }[];
    category: string | null;
    guessed: boolean;
    position: string | null;
    effect: MoveEffect;
  }[];
  cycles: { mods: string[]; hard: boolean }[];
  rules_file: number;
}

export interface LibraryMod {
  key: string;
  id: string | null;
  name: string;
  author: string | null;
  source: Row['source'];
  workshop_id: number | null;
  kind: ModKind;
  version_mod: string | null;
  error: string | null;
  pending: boolean;
}

export interface ModDetails {
  key: string;
  id: string | null;
  name: string;
  author: string | null;
  description: string | null;
  url: string | null;
  poster: string | null;
  folder: string;
  folder_version: string | null;
  versions: string[];
  requires: string[];
  incompatible: string[];
  load_before: string[];
  load_after: string[];
  version_min: string | null;
  version_max: string | null;
  workshop_id: number | null;
  java: {
    pkg: string | null;
    candidates: { jar: string; min: string | null; max: string | null }[];
    chosen: string | null;
    patches: { class_name: string; method_name: string; replaces: boolean }[];
  } | null;
  category: string;
  guessed_category: boolean;
}

export interface SessionState {
  profile: string;
  seconds: number;
  requested: number;
  loaded: number;
  finished: boolean;
  code: number | null;
  recent_lines: string[];
  integrity: { total: number; examples: string[] } | null;
  integrity_skipped: boolean;
  diagnosis: Diagnosis | null;
}

export type WindowOnPlay = 'nothing' | 'minimize_restore' | 'minimize';

export interface NewProfile {
  xmx: string;
  game_extra: string[];
  zb_usage: 'auto' | 'si' | 'no';
  zb_policy: 'prompt' | 'deny-new' | 'allow-all';
  import_game_mods: boolean;
}

export interface AppSettingsData {
  window_on_play: WindowOnPlay;
  integrity_check: boolean;
  new_profile: NewProfile;
  clear_webview_cache: boolean;
}

export interface FoldersInfo {
  game: string | null;
  profiles: string;
  backups: string;
  backups_exists: boolean;
  backups_bytes: number;
}

export interface MaintenanceInfo {
  webview_bytes: number;
  webview_pending: boolean;
  workshop_bytes: number;
  logs_bytes: number;
}

export type EndKind = 'normal' | 'stopped_by_user' | 'java_crash' | 'out_of_memory' | 'hung_while_loading' | 'exited';

export interface JavaCrash {
  kind: string;
  frame: string | null;
  java_frames: string[];
  out_of_memory: boolean;
  module: string | null;
  origin: 'graphics_driver' | 'lwjgl' | 'audio' | 'zb_native' | 'jvm' | 'unknown';
}

export interface PredictionCheck {
  order_ok: boolean;
  not_loaded: string[];
  unexpected: string[];
  overrides_missed: number;
  overrides_extra: number;
  examples: string[];
}

export type SuspectReason =
  | { kind: 'crash_frame'; data: string }
  | { kind: 'patches_crash_class'; data: string }
  | { kind: 'lua_errors'; data: number }
  | { kind: 'not_loaded' }
  | { kind: 'changed' };

export interface Suspect {
  id: string;
  reason: SuspectReason;
}

export interface Diagnosis {
  end: EndKind;
  code: number | null;
  crash: JavaCrash | null;
  not_loaded: { id: string; reason: string | null }[];
  suspects: Suspect[];
  errors: number;
  when: number;
  changed: string[];
  signature: string | null;
  game_changed: [string | null, string | null] | null;
  zb_changed: [string | null, string | null] | null;
  prediction: PredictionCheck | null;
}

export interface Imported {
  profile: ProfileSummary;
  missing: number[];
  ignored: string[];
}

export interface Backup {
  name: string;
  reason: string;
  created: number;
  files: number;
}


export type WorkshopSort = 'trend' | 'most_subscribed' | 'recent' | 'votes';

export interface WorkshopQuery {
  text: string;
  order: WorkshopSort;
  tags: string[];
  page: number;
  collections: boolean;
}

export interface WorkshopState {
  id: number;
  subscribed: boolean;
  installed: boolean;
  update: boolean;
  downloading: boolean;
  downloaded: number;
  total: number;
}

export interface WorkshopItem {
  id: number;
  title: string;
  summary: string;
  collection: boolean;
  author: string | null;
  updated: number;
  tags: string[];
  size: number;
  votes_up: number;
  votes_down: number;
  subscribers: number;
  preview_view: string;
  children: number[];
  state: WorkshopState;
}

export interface WorkshopPage {
  items: WorkshopItem[];
  total: number;
  page: number;
  from_cache: boolean;
}

export interface WorkshopDetails extends WorkshopItem {
  description: string;
  bbcode: string;
  created: number;
}

export interface WorkshopCollection {
  collection: WorkshopDetails;
  items: WorkshopItem[];
  missing: number[];
}


export interface ExportRow {
  slot: number;
  name: string;
  id: string;
  workshop_id: number | null;
  enabled: boolean;
  found: boolean;
}

export interface Export {
  mods: ExportRow[];
  maps: { folder: string; mod_name: string | null; server: boolean }[];
  saved_maps: boolean;
  lost_maps: string[];
  default_txt: string;
}


export type SandboxKind =
  | { t: 'boolean'; default_value: boolean }
  | { t: 'integer'; min: number; max: number; default_value: number }
  | { t: 'decimal'; min: number; max: number; default_value: number }
  | { t: 'list'; values: number; default_value: number }
  | { t: 'text'; default_value: string; multiline: boolean };

export type SandboxValue = boolean | number | string;

export interface SandboxOption {
  name: string;
  title: string;
  help: string | null;
  kind: SandboxKind;
  values: string[];
  from_mod: string | null;
}

export interface SandboxPage {
  id: string;
  title: string;
  from_mod: string | null;
  groups: { title: string | null; options: SandboxOption[] }[];
}

export interface SandboxSchema {
  schema: { server: SandboxPage[]; sandbox: SandboxPage[] };
  game_presets: string[];
  profile_presets: string[];
  user_presets: string[];
  servers: string[];
}

export interface SandboxTexts {
  ini: string;
  lua: string;
  cfg: string;
  mods: string;
}

export interface SandboxValues {
  sandbox: Record<string, SandboxValue>;
  server: Record<string, SandboxValue>;
}


export interface TraceStep {
  func: string;
  file: string;
  line: number | null;
  from_mod: string | null;
}

export interface Issue {
  kind: 'lua' | 'java' | 'general' | 'script' | 'require' | 'zb';
  severity: 'error' | 'warning';
  message: string;
  mod_name: string | null;
  mod_id: string | null;
  mods_involved: string[];
  file: string | null;
  line_file: number | null;
  trace: TraceStep[];
  java: string[];
  times: number;
  key: string;
  noise: boolean;
  from_game: boolean;
  first: number;
  last: number;
  first_hour: string | null;
  last_hour: string | null;
}

export interface ConsoleLines {
  profile: string;
  lines: string[];
  reset: boolean;
}

export type BisectAnswer = 'failed' | 'passed' | 'skip';

export type BisectOutcome =
  | { kind: 'culprit'; data: string }
  | { kind: 'group'; data: string[] }
  | { kind: 'together'; data: string[] }
  | { kind: 'not_the_mods' }
  | { kind: 'not_reproduced' };

export interface BisectView {
  profile: string;
  origin: string;
  suspects: number;
  testing: string[];
  round: number;
  rounds_left: number;
  outcome: BisectOutcome | null;
  mode: 'single' | 'together';
  flaky: boolean;
  suggestion: BisectAnswer | null;
}

export interface SteamProfile {
  steamid: string;
  name: string;
  minutes: number | null;
  last_played: number | null;
  avatar: string | null;
}

export interface GameListChange {
  enabled: string[];
  disabled: string[];
  reordered: boolean;
  maps: boolean;
  emptied: boolean;
}

export type Cell = [number, number];

export interface PyramidInfo {
  source: string;
  bounds: [number, number, number, number];
  levels: number;
}

export interface MapInfo {
  key: string;
  folder: string;
  title: string;
  mod_key: string;
  mod_id: string | null;
  mod_name: string;
  workshop_id: number | null;
  active: boolean;
  order: number | null;
  server: boolean;
  cells: Cell[];
  pyramid: PyramidInfo | null;
  thumb: string | null;
  spawns: [number, number][];
  requires: string[];
  require_keys: string[];
  require_ids: string[];
  compatible: boolean;
}

export type CellState = { state: 'Free' } | { state: 'Shared'; squares: number | null } | { state: 'Touching' } | { state: 'SameMod' };

export type CellInfo = { cell: Cell; maps: string[]; vanilla: boolean; urban: boolean; clash?: [number, number, number][] } & CellState;

export interface TownZone {
  x: number;
  y: number;
  w: number;
  h: number;
  points?: number[];
}

export interface TownInfo {
  name: string;
  spawns: [number, number][];
}

export interface MapsView {
  maps: MapInfo[];
  cells: CellInfo[];
  towns: TownInfo[];
  town_zones: TownZone[];
  bounds: [number, number, number, number];
  world: [number, number, number, number];
  vanilla: PyramidInfo | null;
  server_line: string[];
  saved_order: boolean;
}
