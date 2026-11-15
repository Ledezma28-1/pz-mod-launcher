
import type { AppSettingsData, BisectView, Diagnosis, ModDetails, Environment, Row, Finding, LibraryMod, ProfileSummary, ModKind, ProfileView } from './types';

const state = new URLSearchParams(location.search).get('state') ?? 'normal';
const zbDemo = new URLSearchParams(location.search).get('zb');

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

const environment: Environment = {
  game: {
    folder: 'C:\\Program Files (x86)\\Steam\\steamapps\\common\\ProjectZomboid',
    version: '42.21',
    version_text: '42.21.0 4a0e9546ec',
    build: '25485521',
    branch: 'public',
    java: 'Zulu25.30+17-CA (25.0.1+8-LTS)',
  },
  steam: 'C:\\Program Files (x86)\\Steam',
  zb: {
    in_game: '2.3.4',
    dll: true,
    workshop: [{ channel: 'estable', id: 3619862853, version: new URLSearchParams(location.search).get('zb') === 'update' ? '2.3.5' : '2.3.4' }],
    ref_json: true,
    ref_bat: true,
    ref_steam: new URLSearchParams(location.search).get('zbsteam') === '0' ? [] : ['1848419510'],
    warning: new URLSearchParams(location.search).get('zbwarn') ? 'ZombieBuddy 2.3.2 no carga mods Java en 42.21: actualízalo.' : null,
  },
  modifications: ['ProjectZomboid64.json: -agentlib:zbNative', 'ProjectZomboid64.bat: -agentlib:zbNative'],
};

type Seed = [id: string, name: string, kind: Row['kind'], version: string | null, ws: number, author: string];

const seeds: Seed[] = [
  ['ZombieBuddy', 'ZombieBuddy', 'java', '2.3.4', 3619862853, 'Zed'],
  ['zdk', 'Zed Development Kit', 'lua', '2.3', 3677147974, 'Zed'],
  ['ZBetterModList', 'ZBetter Mod List', 'java', '1.0', 3709229404, 'Zed'],
  ['ZBBetterFPS', 'ZBBetterFPS', 'java', '2.4', 3622986450, 'Zed'],
  ['ZomboidRichPresence', 'Zomboid Rich Presence', 'java', '1.1.2', 3785376350, 'kernz'],
  ['Lugli_FastLoading', 'Fast Loading', 'java', '0.9', 3787104045, 'ViniciosLugli'],
  ['MoodleFramework', 'Moodle Framework', 'lua', null, 3396446795, 'Cyrus'],
  ['NeatUI_Framework', 'Neat UI Framework', 'lua', '1.4', 3508537032, 'Rocco'],
  ['Neat_Crafting', 'Neat Crafting', 'lua', '2.1', 3519423422, 'Rocco'],
  ['Neat_Building', 'Neat Building', 'lua', '1.2', 3524155612, 'Rocco'],
  ['TombGothTex', "Tomb's Player Body - Goth Textures", 'lua', '1.2', 3531765630, 'Tomb'],
  ['TombBodyTex', "Tomb's Player Body - Textures", 'lua', '1.6', 3429790870, 'Tomb'],
  ['SPNCC', 'Spongie’s Character Customisation', 'lua', null, 3403490889, 'Spongie'],
  ['damnlib', 'damnlib', 'lua', '1.0', 3171167894, 'Damnathor'],
  ['63beetle', "'63 Volkswagen Beetle", 'vehicle', null, 3005903549, 'KI5'],
  ['63Type2Van', "'63 Volkswagen Type 2", 'vehicle', null, 3041122351, 'KI5'],
  ['RavenCreekB42', 'Raven Creek', 'map', '42.13', 3447272250, 'Raven Creek Team'],
  ['SpawnSelector', 'Spawn Selector', 'lua', '1.8', 3026723485, 'Albion'],
  ['ChainsawB42', 'Chainsaw B42', 'lua', '3.0', 3258343790, 'Lowis'],
  ['ebfchainsaw', 'Electric Brown Fox Chainsaw', 'lua', '1.1', 2991201484, 'EBF'],
  ['LargeProgressIndicator', 'Large Progress Indicator', 'lua', null, 2926455909, 'Hypnotoad'],
  ['ExpandedHelicopterEvents', 'Expanded Helicopter Events', 'lua', '6.4', 2458631365, 'Chuckleberry Finn'],
];

const carsExtra = Math.min(2000, Math.max(0, Number(new URLSearchParams(location.search).get('mods')) || 0));
for (let i = 0; i < carsExtra; i++) seeds.push([`coche${i}`, `Coche ${i}`, 'vehicle', null, 3000000000 + i, 'KI5']);

let active: Record<string, boolean> = Object.fromEntries(seeds.map(([id]) => [id, id !== 'Lugli_FastLoading']));
let order = seeds.map(([id]) => id);
if (carsExtra) order = [...order.filter((id) => id !== 'damnlib'), 'damnlib'];

const findings: Finding[] = [
  {
    severity: 'Blocks',
    mods: ['ebfchainsaw'],
    kind: { kind: 'GameVersion', min: '42.20.0', max: '42.20.99', game: '42.21' },
    fixes: [{ action: 'Disable', data: 'ebfchainsaw' }],
  },
  {
    severity: 'Blocks',
    mods: ['ZBBetterFPS'],
    kind: { kind: 'ZbVersion', zb: '2.3.4', ranges: [['2.4.0', '2.99.99'], ['3.0.0', null]] },
    fixes: [{ action: 'UpdateZb' }, { action: 'Disable', data: 'ZBBetterFPS' }],
  },
  {
    severity: 'Blocks',
    mods: ['ExpandedHelicopterEvents'],
    kind: { kind: 'MissingDependency', requires: 'EasyConfigChucked', inactive_installed: false },
    fixes: [{ action: 'OpenWorkshop', data: 2458631365 }, { action: 'Disable', data: 'ExpandedHelicopterEvents' }],
  },
  {
    severity: 'Warning',
    mods: ['BetterSortCC'],
    kind: { kind: 'MissingDependency', requires: 'tsarslib', inactive_installed: true },
    fixes: [{ action: 'Enable', data: 'tsarslib' }],
  },
  {
    severity: 'Warning',
    mods: ['63beetle', '63Type2Van'],
    kind: {
      kind: 'OverriddenFiles',
      pairs: [
        {
          loses: '63beetle',
          wins: '63Type2Van',
          total: 2,
          files: ['media/textures/item_btl63seatf.png', 'media/textures/item_btl63seatr.png'],
          identical: 5,
          not_load: [],
          total_not_load: 0,
          lost_objects: [],
          total_lost: 0,
          related: false,
          accepted: false,
        },
      ],
    },
    fixes: [
      { action: 'Win', data: ['63beetle', ['63Type2Van']] },
      { action: 'Accept', data: [['63beetle', '63Type2Van']] },
    ],
  },
  {
    severity: 'Warning',
    mods: ['TombBodyTex', 'TombGothTex', 'SPNCC'],
    kind: {
      kind: 'OverriddenFiles',
      pairs: [
        {
          loses: 'TombBodyTex',
          wins: 'SPNCC',
          total: 22,
          files: ['media/textures/body/muscle/f_muscle02.png', 'media/textures/body/muscle/f_muscle03.png'],
          identical: 0,
          not_load: [],
          total_not_load: 0,
          lost_objects: [],
          total_lost: 0,
          related: false,
          accepted: false,
        },
        {
          loses: 'TombGothTex',
          wins: 'SPNCC',
          total: 8,
          files: ['media/textures/body/f_body01.png'],
          identical: 2,
          not_load: [],
          total_not_load: 0,
          lost_objects: [],
          total_lost: 0,
          related: false,
          accepted: false,
        },
      ],
    },
    fixes: [
      { action: 'Win', data: ['TombBodyTex', ['TombGothTex', 'SPNCC']] },
      { action: 'Win', data: ['TombGothTex', ['TombBodyTex', 'SPNCC']] },
      {
        action: 'Accept',
        data: [
          ['TombBodyTex', 'SPNCC'],
          ['TombGothTex', 'SPNCC'],
        ],
      },
    ],
  },
  {
    severity: 'Detail',
    mods: ['Neat_Crafting', 'Neat_Building'],
    kind: {
      kind: 'OverriddenFiles',
      pairs: [
        {
          loses: 'Neat_Crafting',
          wins: 'Neat_Building',
          total: 0,
          files: [],
          identical: 27,
          not_load: ['media/ui/categoryicon/base.psd'],
          total_not_load: 1,
          lost_objects: [],
          total_lost: 0,
          related: false,
          accepted: false,
        },
      ],
    },
    fixes: [],
  },
  {
    severity: 'Detail',
    mods: ['TombBodyTex'],
    kind: { kind: 'OverridesGame', total: 34, files: ['media/textures/body/femalebody01.png'], behavior: 0 },
    fixes: [],
  },
  {
    severity: 'Warning',
    mods: ['FancyHandworkB42_19', 'SwapIt'],
    kind: {
      kind: 'LuaGlobalReplaced',
      wins: 'SwapIt',
      functions: [
        {
          name: 'ISHotbar.equipItem',
          wins_at: 'swapit main.lua:81',
          remade: false,
          lost: [
            { mod: 'FancyHandworkB42_19', at: '!fhhotbar.lua:10', wraps: true },
            { mod: 'FancyHandworkB42_19', at: 'fh_hotbaroffhand.lua:24', wraps: true },
          ],
        },
      ],
      total: 1,
      related: false,
      accepted: false,
      movable: true,
    },
    fixes: [
      { action: 'Win', data: ['FancyHandworkB42_19', ['SwapIt']] },
      { action: 'Accept', data: [['FancyHandworkB42_19', 'SwapIt']] },
    ],
  },
  {
    severity: 'Warning',
    mods: ['ViewpointTrueBallistics'],
    kind: {
      kind: 'JavaLinkBroken',
      missing: ['zombie.network.fields.hit.WeaponHit (clase)', 'zombie.util.list.PZArrayList (clase)', 'zombie.iso.IsoGridSquare.getObjects: ()Lzombie/util/list/PZArrayList;'],
      total: 10,
      game: '42.22',
    },
    fixes: [],
  },
];
if (new URLSearchParams(location.search).get('incompat')) {
  findings.push(
    {
      severity: 'Blocks',
      mods: ['firearmmod', 'firearmmodbeta', 'firearmmodvanilla'],
      kind: { kind: 'Incompatible', declared: [['firearmmod', 'firearmmodbeta'], ['firearmmod', 'firearmmodvanilla'], ['firearmmodbeta', 'firearmmod']] },
      fixes: [
        { action: 'Keep', data: ['firearmmod', ['firearmmodbeta', 'firearmmodvanilla']] },
        { action: 'Keep', data: ['firearmmodbeta', ['firearmmod']] },
        { action: 'Keep', data: ['firearmmodvanilla', ['firearmmod']] },
      ],
    },
    ((): Finding => {
      const v = ['BodyTex', 'BodyTexDOLL', 'BodyTexNUDE', 'GothTex', 'GothTexDOLL', 'GothTexNUDE'].map((s) => `AddSlopFacesNeckFixTomb${s}`);
      return {
        severity: 'Blocks',
        mods: v,
        kind: { kind: 'Incompatible', declared: v.flatMap((a) => v.filter((b) => b !== a).map((b) => [a, b] as [string, string])) },
        fixes: v.map((a) => ({ action: 'Keep', data: [a, v.filter((b) => b !== a)] })),
      };
    })(),
    {
      severity: 'Blocks',
      mods: ['FeedThatAnimal', 'FeedThatAnimalMP'],
      kind: { kind: 'Incompatible', declared: [['FeedThatAnimalMP', 'FeedThatAnimal']] },
      fixes: [
        { action: 'Keep', data: ['FeedThatAnimal', ['FeedThatAnimalMP']] },
        { action: 'Keep', data: ['FeedThatAnimalMP', ['FeedThatAnimal']] },
      ],
    },
  );
}

if (new URLSearchParams(location.search).get('noisy')) {
  const block = (mod: string, kind: Finding['kind'], fixes: Finding['fixes'] = []): Finding => ({
    severity: 'Blocks',
    mods: [mod],
    kind,
    fixes: [...fixes, { action: 'Disable', data: mod }],
  });
  findings.push(
    block('RollingHillsPhysics', { kind: 'GameVersion', min: '42.21', max: '42.21', game: '42.22' }),
    block('RollingHillsPhysics', { kind: 'MissingDependency', requires: 'ViewpointRollingHills', inactive_installed: false }, [{ action: 'OpenWorkshop', data: 3600000001 }]),
    block('twistonfirestatsmp', { kind: 'GameVersion', min: '42.19', max: '42.19', game: '42.22' }),
    ...['TICS', 'vac_fb41_patch', 'vac_tihfp_patch', 'vac_ufc_patch', 'OldGuns41', 'Brita41'].map((m) => block(m, { kind: 'OnlyB41' })),
    block('vac_tihfp_patch', { kind: 'MissingDependency', requires: 'TIHFP', inactive_installed: false }, [{ action: 'OpenWorkshop', data: 3600000002 }]),
    ...['MZInteriorsPetyarbuilt', 'MZInteriorsKI5'].map((m) =>
      block(m, { kind: 'MissingDependency', requires: 'MZInteriorsCore', inactive_installed: false }, [{ action: 'OpenWorkshop', data: 3600000003 }]),
    ),
    {
      severity: 'Info',
      mods: ['HALFLINGS', 'RollingHillsPhysics'],
      kind: { kind: 'JavaPatchOverlap', methods: ['zombie.core.skinnedmodel.model.ModelSlotRenderData.init', 'zombie.vehicles.BaseVehicle.update'], replaced: [] },
      fixes: [],
    },
    {
      severity: 'Info',
      mods: ['RollingHillsPhysics', 'MZInteriorsPetyarbuilt'],
      kind: { kind: 'JavaPatchOverlap', methods: ['zombie.core.skinnedmodel.model.ModelSlotRenderData.init'], replaced: [] },
      fixes: [],
    },
  );
}

const requires: Record<string, string[]> = {
  ZBetterModList: ['ZombieBuddy', 'zdk'],
  ZBBetterFPS: ['ZombieBuddy'],
  ZomboidRichPresence: ['ZombieBuddy'],
  TombGothTex: ['TombBodyTex'],
  '63beetle': ['damnlib'],
  '63Type2Van': ['damnlib'],
  Neat_Crafting: ['NeatUI_Framework'],
  Neat_Building: ['NeatUI_Framework'],
  ExpandedHelicopterEvents: ['EasyConfigChucked'],
};
for (let i = 0; i < carsExtra; i++) requires[`coche${i}`] = ['damnlib'];

function view(): ProfileView {
  const rows: Row[] = order.map((id, i) => {
    const s = seeds.find((x) => x[0] === id)!;
    const worst = findings
      .filter((h) => h.mods.includes(id))
      .map((h) => h.severity)
      .sort((a, b) => ['Blocks', 'Warning', 'Info', 'Detail'].indexOf(a) - ['Blocks', 'Warning', 'Info', 'Detail'].indexOf(b))[0];
    return {
      index: i,
      id,
      key: `ws:${s[4]}/${id}`,
      name: s[1],
      enabled: active[id],
      pinned: id === 'ZombieBuddy',
      found: true,
      kind: s[2],
      version_mod: s[3],
      source: 'workshop',
      workshop_id: s[4],
      author: s[5],
      severity: worst ? (worst.toLowerCase() as Row['severity']) : null,
    };
  });
  const edges = Object.entries(requires).flatMap(([after, before]) =>
    before
      .filter((a) => order.includes(a))
      .map((a) => ({
        before: a,
        after,
        rule: 'Requires' as const,
        is_hard: true,
        broken: order.indexOf(a) > order.indexOf(after),
      })),
  );
  const count = (g: Finding['severity']) => findings.filter((h) => h.severity === g).length;
  return {
    id: 'mi-partida',
    name: 'Supervivencia con Java',
    rows,
    findings,
    edges,
    settings: { xmx: '8g', game_extra: '-debug', policy: 'prompt', use_zb: 'auto', probe: !!new URLSearchParams(location.search).get('probe') },
    counts: { active: rows.filter((f) => f.enabled).length, blocks: count('Blocks'), warning: count('Warning'), info: count('Info'), detail: count('Detail') },
  };
}

const profiles: ProfileSummary[] = [
  { id: 'mi-partida', name: 'Supervivencia con Java', active: 21, total: 22, last_use: Math.floor(Date.now() / 1000) - 7200 },
  { id: 'vainilla', name: 'Vainilla', active: 0, total: 0, last_use: Math.floor(Date.now() / 1000) - 86400 * 9 },
  { id: 'servidor-amigos', name: 'Servidor de los amigos', active: 64, total: 70, last_use: Math.floor(Date.now() / 1000) - 86400 * 2 },
];

function detail(key: string): ModDetails {
  const id = key.split('/')[1];
  const s = seeds.find((x) => x[0] === id)!;
  const java = s[2] === 'java';
  return {
    key,
    id,
    name: s[1],
    author: s[5],
    description:
      id === 'ZBetterModList'
        ? 'Mejora la lista de mods del menú principal: búsqueda, filtros y orden de carga visibles.\nRequiere ZombieBuddy.'
        : 'Descripción de demostración del mod (sintética).',
    url: null,
    poster: null,
    folder: `C:\\Program Files (x86)\\Steam\\steamapps\\workshop\\content\\108600\\${s[4]}\\mods\\${id}`,
    folder_version: s[2] === 'map' ? '42.13' : '42',
    versions: ['42', '42.13'],
    requires: requires[id] ?? [],
    incompatible: [],
    load_before: [],
    load_after: [],
    version_min: id === 'ebfchainsaw' ? '42.20.0' : null,
    version_max: id === 'ebfchainsaw' ? '42.20.99' : null,
    workshop_id: s[4],
    category: s[2] === 'map' ? 'map' : s[2] === 'vehicle' ? 'vehicle' : 'other',
    guessed_category: true,
    java: java
      ? {
          pkg: `demo.${id.toLowerCase()}`,
          candidates:
            id === 'ZBBetterFPS'
              ? [
                  { jar: 'media/java/client/ZBBetterFPS-2x.jar', min: '2.4.0', max: '2.99.99' },
                  { jar: 'media/java/client/ZBBetterFPS.jar', min: '3.0.0', max: null },
                ]
              : [{ jar: `media/java/client/${id}.jar`, min: '2.0.0', max: '2.99.99' }],
          chosen: id === 'ZBBetterFPS' ? null : `media/java/client/${id}.jar`,
          patches: [
            { class_name: 'zombie.GameWindow', method_name: 'frameStep', replaces: false },
            { class_name: 'zombie.core.Core', method_name: 'EndFrameUI', replaces: false },
            { class_name: 'zombie.ZomboidFileSystem', method_name: 'loadMod', replaces: true },
          ],
        }
      : null,
  };
}

const library: LibraryMod[] = seeds.map(([id, name, kind, version, ws, author]) => ({
  key: `ws:${ws}/${id}`,
  id,
  name,
  author,
  source: 'workshop',
  workshop_id: ws,
  kind,
  version_mod: version,
  error: null,
  pending: ws === 3619862853,
}));
const outsideOfProfile: [string, string, ModKind, string | null, number, string][] = [
  ['tsarslib', "Tsar's Common Library", 'lua', '2.1', 2392709985, 'Tsar'],
  ['BetterSortCC', 'Better Sorting', 'lua', '1.3', 2313387159, 'Blindcoder'],
  ['ArmorPlus', 'Armor Plus', 'lua', '0.8', 3401122334, 'Kiwi'],
  ['AuthenticZLite', 'Authentic Z - Lite', 'lua', '1.0', 2335368829, 'Authentic Peach'],
];
for (const [id, name, kind, version, ws, author] of outsideOfProfile) {
  library.push({ key: `ws:${ws}/${id}`, id, name, author, source: 'workshop', workshop_id: ws, kind, version_mod: version, error: null, pending: false });
}

const wsState = (id: number, subscribed: boolean) => ({ id, subscribed, installed: subscribed, update: false, downloading: false, downloaded: 0, total: 0 });
const wsItems = seeds.slice(0, 14).map(([id, name, , , ws, author], i) => ({
  id: ws,
  title: name,
  summary: `Descripción de demostración de ${name} (${id}).`,
  collection: false,
  author,
  updated: 1792000000 - i * 86400 * 3,
  tags: i % 3 === 0 ? ['Build 41', 'Build 42'] : ['Build 42'],
  size: 1048576 * (i + 1),
  votes_up: 900 - i * 40,
  votes_down: 20 + i * 3,
  subscribers: 820000 - i * 51000,
  preview_view: '',
  children: [] as number[],
  state: wsState(ws, i % 2 === 0),
}));

let sbCache: Promise<{ schema: unknown; values: unknown } | null> | null = null;
function sbReal() {
  const url = new URLSearchParams(location.search).get('sbjson');
  if (!url) return Promise.resolve(null);
  sbCache ??= fetch(url).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  return sbCache;
}

let viewsOpened = 0;

export async function demo<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  await wait(state === 'loading' ? 1e9 : 120);
  if (state === 'error' && cmd === 'environment') {
    throw 'no encontrado: Project Zomboid en 2 bibliotecas';
  }
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case 'environment':
      return r(environment);
    case 'steam_profile':
      return r(state === 'nosteam' ? null : { steamid: '76561199808685238', name: 'Superviviente', minutes: 18_760, last_played: Math.floor(Date.now() / 1000) - 86_400 * 2, avatar: null });
    case 'profiles':
      if (state === 'empty') return r([profiles[1], profiles[0], profiles[2]]);
      return r(state === 'noprofiles' ? [] : profiles);
    case 'profiles_unreadable':
      return r(
        new URLSearchParams(location.search).get('unreadable')
          ? [['partida-vieja', 'C:\\Users\\demo\\PZModManager\\perfiles\\partida-vieja\\perfil.toml', 'TOML parse error at line 3, column 9: invalid string']]
          : [],
      );
    case 'choose_game':
      return r(undefined);
    case 'profile_view': {
      if (new URLSearchParams(location.search).get('viewprogress') && viewsOpened++ > 0) return new Promise(() => {});
      const v = view();
      if (state === 'empty' || args.id === 'vainilla') {
        const p = profiles.find((x) => x.id === args.id);
        return r({ ...v, id: args.id, name: p?.name ?? 'Vainilla', rows: [], findings: [], edges: [], counts: { active: 0, blocks: 0, warning: 0, info: 0, detail: 0 } });
      }
      return r(v);
    }
    case 'profile_set_enabled':
      active = { ...active, [args.modId as string]: args.enabled as boolean };
      return r(view());
    case 'profile_game_list':
      return r(
        new URLSearchParams(location.search).get('gamelist') && args.id !== 'vainilla'
          ? { enabled: ['ZBetterModList', 'MoodleFramework'], disabled: ['ZBBetterFPS'], reordered: true, maps: false, emptied: false }
          : null,
      );
    case 'profile_set_enabled_many': {
      let n = 0;
      for (const id of args.modIds as string[]) {
        if ((active[id] ?? true) !== args.enabled) n++;
        active = { ...active, [id]: args.enabled as boolean };
      }
      return r(n);
    }
    case 'profile_move': {
      const [m] = order.splice(args.from as number, 1);
      order.splice(args.to as number, 0, m);
      return r(undefined);
    }
    case 'profile_sort':
      return r({
        order,
        moves: [
          {
            id: 'TrueMusicJukebox',
            from: 3,
            to: 9,
            reasons: [{ before: '63beetle', after: 'TrueMusicJukebox', rule: 'UserOverride' }],
            category: 'other',
            guessed: true,
            position: null,
            effect: 'rule',
          },
          { id: 'ExpandedHelicopterEvents', from: 8, to: 12, reasons: [], category: 'code', guessed: true, position: null, effect: 'winner' },
          { id: 'RavenCreekB42', from: 16, to: 4, reasons: [], category: 'map', guessed: true, position: null, effect: 'category' },
          { id: '63beetle', from: 14, to: 6, reasons: [], category: 'vehicle', guessed: false, position: null, effect: 'category' },
        ],
        cycles: [],
        rules_file: 2,
      });
    case 'mod_details':
      return r(detail(args.key as string));
    case 'library':
      return r(library);
    case 'profile_export': {
      const v = view();
      return r({
        mods: v.rows.map((f, i) => ({ slot: i + 1, name: f.name, id: f.id, workshop_id: f.workshop_id, enabled: f.enabled, found: f.found })),
        maps: [
          { folder: 'Raven Creek B42', mod_name: 'Raven Creek' },
          { folder: 'Spawn Selector', mod_name: 'Spawn Selector' },
          { folder: 'Muldraugh, KY', mod_name: null },
        ],
        saved_maps: false,
        lost_maps: [],
        default_txt: 'VERSION = 1,\n\nmods\n{\n' + v.rows.filter((f) => f.enabled).map((f) => `    mod = ${f.id},`).join('\n') + '\n}\n\nmaps\n{\n}\n',
      });
    }
    case 'sandbox_schema': {
      const sb = new URLSearchParams(location.search).get('sb');
      if (sb === 'loading') return new Promise(() => {});
      if (sb === 'error') return Promise.reject('projectzomboid.jar: no se puede leer (demo)');
      const real = await sbReal();
      if (real) return r(real.schema);
    }
    case 'sandbox_esquema_demo':
      return r({
        schema: {
          server: [
            {
              id: 'ini:Details',
              title: 'Detalles',
              from_mod: null,
              groups: [
                {
                  title: null,
                  options: [
                    { name: 'PublicName', title: 'PublicName', help: 'Nombre del servidor en la lista pública.', kind: { t: 'text', default_value: 'My PZ Server', multiline: false }, values: [], from_mod: null },
                    { name: 'Public', title: 'Public', help: 'Se muestra en la lista de servidores.', kind: { t: 'boolean', default_value: false }, values: [], from_mod: null },
                    { name: 'MaxPlayers', title: 'MaxPlayers', help: null, kind: { t: 'integer', min: 1, max: 100, default_value: 32 }, values: [], from_mod: null },
                  ],
                },
              ],
            },
          ],
          sandbox: [
            {
              id: 'sb:Zombie',
              title: 'Zombi',
              from_mod: null,
              groups: [
                {
                  title: null,
                  options: [
                    { name: 'Zombies', title: 'Número de zombis', help: 'Cambiando esto se establece la opción avanzada "Multiplicador de Población".', kind: { t: 'list', values: 6, default_value: 4 }, values: ['Zombicidio', 'Muy alto', 'Alto', 'Normal', 'Bajo', 'Nada'], from_mod: null },
                    { name: 'ZombieMigrate', title: 'Migración de zombis', help: 'Los zombis pueden moverse a celdas vacías.', kind: { t: 'boolean', default_value: true }, values: [], from_mod: null },
                  ],
                },
                {
                  title: 'Lore de Zombis',
                  options: [
                    { name: 'ZombieLore.Speed', title: 'Velocidad', help: null, kind: { t: 'list', values: 4, default_value: 2 }, values: ['Esprintadores', 'Rápidos', 'Lentos', 'Aleatorio'], from_mod: null },
                  ],
                },
              ],
            },
            {
              id: 'sb:Loot',
              title: 'Botín',
              from_mod: null,
              groups: [{ title: null, options: [{ name: 'FoodLootNew', title: 'Comida', help: 'Toda la comida que puede pudrirse.', kind: { t: 'decimal', min: 0, max: 4, default_value: 0.6 }, values: [], from_mod: null }] }],
            },
            {
              id: 'mod:DAMN_Spawns',
              title: 'KI5 | Guaranteed spawns',
              from_mod: '69charger',
              groups: [{ title: null, options: [{ name: 'DAMN.AllowOinkerSpawns', title: 'Aparición de Oinker', help: null, kind: { t: 'boolean', default_value: true }, values: [], from_mod: '69charger' }] }],
            },
          ],
        },
        game_presets: ['Apocalypse', 'Extinction', 'Outbreak', 'Rising', 'SixMonthsLater'],
        profile_presets: [],
        user_presets: ['New', 're'],
        servers: [],
      });
    case 'sandbox_values': {
      const real = await sbReal();
      if (real) return r(real.values);
    }
    case 'sandbox_valores_demo':
      return r({
        sandbox: { Zombies: 3, ZombieMigrate: true, 'ZombieLore.Speed': 2, FoodLootNew: 0.8, 'DAMN.AllowOinkerSpawns': true },
        server: { PublicName: 'My PZ Server', Public: false, MaxPlayers: 32 },
      });
    case 'sandbox_texts':
      return r({ ini: 'PVP=true\r\n', lua: 'SandboxVars = {\r\n}\r\n', cfg: 'Version=6\r\n', mods: '# Puesto\tMod ID\tWorkshop ID\tNombre\r\n' });
    case 'sandbox_save':
      return r(['Server/servertest.ini', 'Server/servertest_SandboxVars.lua', 'Sandbox Presets/servertest.cfg']);
    case 'workshop_tags':
      return r(['Build 41', 'Build 42', 'Framework', 'Map', 'Vehicles', 'Weapons']);
    case 'workshop_search':
      if (state === 'ws-error') throw 'Steam no está abierto: ábrelo para usar el Workshop desde aquí';
      return r({ items: (args.query as { collections: boolean }).collections ? [] : wsItems, total: 640, page: 1, from_cache: false });
    case 'workshop_details':
      return r((args.ids as number[]).map((id) => ({ ...(wsItems.find((e) => e.id === id) ?? wsItems[0]), description: 'Descripción de demostración.', bbcode: '[h1]Mod de demostración[/h1]\nTexto con [b]negrita[/b], [i]cursiva[/i] y un [url=https://steamcommunity.com]enlace[/url].\n\n[h2]Qué hace[/h2]\n[list][*]Primera cosa[*]Segunda cosa con [u]subrayado[/u][/list]\n[quote=Autor]Una cita de ejemplo.[/quote]\n[code]require=\\ZombieBuddy[/code]\nFinal con [spoiler]un spoiler[/spoiler].', created: 1780000000 })));
    case 'workshop_states':
      return r(
        (args.ids as number[]).map((id) =>
          id === 3619862853
            ? { id, subscribed: true, installed: true, update: false, downloading: false, downloaded: 0, total: 0 }
            : id === 3812624292
              ? { id, subscribed: zbDemo === 'downloading', installed: false, update: false, downloading: zbDemo === 'downloading', downloaded: 37_000_000, total: 52_000_000 }
              : wsState(id, true),
        ),
      );
    case 'workshop_check':
      return r((args.ids as number[]).map((id) => [id, wsItems.find((e) => e.id === id)?.updated ?? 0]));
    case 'workshop_cache_read':
      return r(null);
    case 'workshop_cache_write':
      return r(undefined);
    case 'probe_report':
      return r(new URLSearchParams(location.search).get('probe') ? { phase: 'start', total: 182, mismatches: [] } : null);
    case 'workshop_waiting':
      return r(new URLSearchParams(location.search).get('waiting') ? [3836836552, 3836837888] : []);
    case 'workshop_download':
      return r([]);
    case 'workshop_subscribe':
      return r([]);
    case 'backups_list':
      return r([{ name: '2026-10-21_20-15-48-zombiebuddy', reason: 'instalar o actualizar ZombieBuddy', created: 1792613749, files: 4 }]);
    case 'launch':
      return r(4242);
    case 'profile_count':
      return r(args.id === 'vainilla' ? { active: 0, blocks: 0, warning: 0, info: 0 } : view().counts);
    case 'settings_get':
      return r(settingsDemo);
    case 'settings_set':
      settingsDemo = args.s as AppSettingsData;
      return r(undefined);
    case 'folders_info':
      return r({
        game: 'D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid',
        profiles: 'C:\\Users\\demo\\PZModManager\\perfiles',
        backups: 'C:\\Users\\demo\\AppData\\Roaming\\PZModManager\\copias',
        backups_exists: true,
        backups_bytes: 48_300_000,
      });
    case 'maintenance_info':
      return r({ webview_bytes: 472_900_000, webview_pending: settingsDemo.clear_webview_cache, workshop_bytes: 155_000, logs_bytes: 15_300 });
    case 'maintenance_clear':
      return r(args.what === 'logs' ? 9_800 : 155_000);
    case 'maintenance_webview':
      settingsDemo = { ...settingsDemo, clear_webview_cache: Boolean(args.pending) };
      return r(undefined);
    case 'logs_folder':
      return r('C:\\Users\\demo\\AppData\\Roaming\\PZModManager\\registro');
    case 'profile_share':
      return r('C:\\Users\\demo\\Downloads\\Supervivencia con Java.pzprofile');
    case 'console_read':
      return r(consoleDemo);
    case 'console_issues':
    case 'console_analyze':
      return r(errorsDemo);
    case 'console_path':
      return r('C:\\Users\\demo\\PZModManager\\perfiles\\mi-partida\\Zomboid\\console.txt');
    case 'session_last_diagnosis':
      return r(diagnosisDemo());
    case 'bisect_open':
      return r(bisectDemo);
    case 'bisect_start':
      bisectDemo = { ...bisectStep(0), profile: String(args.id) };
      return r(bisectDemo);
    case 'bisect_answer':
    case 'bisect_undo': {
      const round = (bisectDemo?.round ?? 1) + (cmd === 'bisect_answer' ? 1 : -1);
      bisectDemo = { ...bisectStep(Math.max(0, round - 1)), profile: bisectDemo?.profile ?? 'mi-partida' };
      return r(bisectDemo);
    }
    case 'bisect_cancel':
      bisectDemo = null;
      return r(undefined);
    case 'session_report':
      return r('PZ Mod Manager report\nGame: 42.21.0 4a0e9546ec\nZombieBuddy: 2.3.4\nProfile: Supervivencia con Java (21 enabled mods)\nEnd: java_crash (exit code 1)\n…');
    case 'featured_read':
      return r(state === 'nosteam' || state === 'noprofiles' ? null : featuredDemo());
    case 'featured_refresh':
      await wait(900);
      if (state === 'nosteam') throw 'Steam no está abierto: ábrelo para ver los destacados';
      return r(featuredDemo());
    default:
      return r(undefined);
  }
}

function featuredDemo() {
  const mods = seeds.slice(0, 12).map(([, name, , , ws, author], i) => ({
    id: ws,
    title: name,
    author,
    subscribers: Math.round(820000 / (i + 1.4)),
    preview_view: '',
    collection: false,
    children: 0,
  }));
  const collections = [
    ['Vehículos de KI5 (todos)', 'KI5', 64],
    ['Servidor PvE: calidad de vida', 'Dane', 38],
    ['Mapas de Kentucky ampliado', 'Kentucky Maps', 112],
    ['Solo Build 42: imprescindibles', 'Blair', 27],
    ['Armas y municiones realistas', 'Arsenal(26)', 45],
    ['Supervivencia hardcore', 'Hydrocraft', 51],
    ['Ropa y moda de los 90', 'Authentic Z', 19],
  ].map(([title, author, children], i) => ({
    id: 3100000000 + i,
    title: title as string,
    author: author as string,
    subscribers: Math.round(260000 / (i + 1.2)),
    preview_view: '',
    collection: true,
    children: children as number,
  }));
  return { parsed: Math.floor(Date.now() / 1000) - 3600 * 26, mods, collections };
}

const consoleDemo = [
  'LOG  : General      f:0> version=42.21.0 4a0e9546ec demo=false',
  'LOG  : General      f:0> SteamUtils initialised successfully',
  'LOG  : Mod          f:0> loading ZombieBuddy',
  'LOG  : Mod          f:0> loading zdk',
  'LOG  : Mod          f:0> loading TrueMusicJukebox',
  'WARN : Lua          f:0 at KahluaThread.call                   > require("TMRSound") failed',
  'LOG  : General      f:0> ERROR: template "DAMN85chevyImpalaPD" not found B',
  'LOG  : Lua          f:0> TCDisassembleMenu loaded - right-click Disassemble option active.',
  'ERROR: General      f:0 at KahluaThread.flushErrorMessage      > -------------------------------------------------------------',
  'attempted index: addTrait of non-table: null.',
  'ERROR: General      f:0 at KahluaThread.flushErrorMessage      > dumping Lua stack trace',
  '-----------------------------------------',
  'STACK TRACE',
  '-----------------------------------------',
  'function: trait -- file: Hero.lua line # 8 | MOD: OasisB42_True Music Jukebox.',
  'ERROR: General      f:0 at ExceptionLogger.logException       > Exception thrown',
  '\tjava.lang.RuntimeException: attempted index: addTrait of non-table: null at KahluaThread.tableget(KahluaThread.java:1447).',
  'LOG  : General      f:0> game loading took 75 seconds.',
];
const errorsDemo = [
  {
    kind: 'lua', severity: 'error', message: 'attempted index: addTrait of non-table: null', mod_name: 'OasisB42_True Music Jukebox', mod_id: null,
    file: 'Hero.lua', line_file: 8, trace: [{ func: 'trait', file: 'Hero.lua', line: 8, from_mod: 'OasisB42_True Music Jukebox' }],
    java: ['java.lang.RuntimeException: attempted index: addTrait of non-table: null at KahluaThread.tableget(KahluaThread.java:1447).'],
    times: 3, first: 9, last: 9, first_hour: '21:37:37', last_hour: '21:42:29',
  },
  {
    kind: 'lua', severity: 'error', message: 'Object tried to call nil in gifts', mod_name: 'OasisB42_True Music Jukebox', mod_id: null,
    file: 'Hero.lua', line_file: 23, trace: [{ func: 'gifts', file: 'Hero.lua', line: 23, from_mod: 'OasisB42_True Music Jukebox' }],
    java: [], times: 2, first: 12, last: 12, first_hour: '21:45:27', last_hour: '21:45:27',
  },
  {
    kind: 'java', severity: 'error', message: 'java.lang.IllegalStateException: IsoChunk was already assigned a valid loadID', mod_name: null, mod_id: null,
    file: null, line_file: null, trace: [], java: ['java.lang.IllegalStateException: IsoChunk was already assigned a valid loadID'], times: 1, first: 17, last: 17, first_hour: null, last_hour: null,
  },
  { kind: 'require', severity: 'warning', message: 'require("TMRSound") failed', mod_name: null, mod_id: null, file: null, line_file: null, trace: [], java: [], times: 1, first: 6, last: 6, first_hour: null, last_hour: null },
  { kind: 'script', severity: 'warning', message: 'template "DAMN85chevyImpalaPD" not found B', mod_name: null, mod_id: null, file: null, line_file: null, trace: [], java: [], times: 21, first: 7, last: 7, first_hour: null, last_hour: null },
].map((i) => ({ ...i, key: i.message, noise: i.kind === 'java', from_game: false, mods_involved: [] as string[] }));

export const demoState = state;

function diagnosisDemo(): Diagnosis | null {
  const kind = new URLSearchParams(location.search).get('diagnosis');
  const base = { code: 1, crash: null, not_loaded: [], suspects: [], errors: 0, when: Math.floor(Date.now() / 1000) - 120, changed: [] as string[], signature: null, game_changed: null, zb_changed: null, prediction: null };
  switch (kind) {
    case 'crash':
      return {
        ...base,
        end: 'java_crash',
        crash: {
          kind: 'EXCEPTION_ACCESS_VIOLATION (0xc0000005)',
          frame: 'C  [lwjgl.dll+0x1a2b]',
          java_frames: ['j  zombie.core.opengl.RenderThread.run()V+12', 'j  zbetterfps.Hook.tick()V+3'],
          out_of_memory: false,
          module: 'lwjgl.dll',
          origin: 'lwjgl',
        },
        game_changed: ['42.21.0', '42.22.0'],
        suspects: [
          { id: 'ZBBetterFPS', reason: { kind: 'crash_frame', data: 'j  zbetterfps.Hook.tick()V+3' } },
          { id: 'TrueMusicJukebox', reason: { kind: 'lua_errors', data: 3 } },
          { id: 'RavenCreekB42', reason: { kind: 'changed' } },
        ],
        errors: 3,
        changed: ['RavenCreekB42'],
        signature: 'java_crash:zbetterfps.Hook.tick()V',
      };
    case 'oom':
      return {
        ...base,
        end: 'out_of_memory',
        crash: { kind: 'java.lang.OutOfMemoryError: Java heap space', frame: null, java_frames: [], out_of_memory: true, module: null, origin: 'unknown' },
      };
    case 'driver':
      return {
        ...base,
        end: 'java_crash',
        crash: {
          kind: 'EXCEPTION_ACCESS_VIOLATION (0xc0000005)',
          frame: 'C  [nvoglv64.dll+0x9c0d1]',
          java_frames: ['j  zombie.core.opengl.RenderThread.run()V+12'],
          out_of_memory: false,
          module: 'nvoglv64.dll',
          origin: 'graphics_driver',
        },
      };
    case 'hang':
      return { ...base, end: 'hung_while_loading', suspects: [{ id: 'ExpandedHelicopterEvents', reason: { kind: 'not_loaded' } }] };
    case 'notloaded':
      return {
        ...base,
        code: 0,
        end: 'normal',
        not_loaded: [
          { id: 'ExpandedHelicopterEvents', reason: 'MissingDependency' },
          { id: 'ebfchainsaw', reason: 'GameVersion' },
        ],
        suspects: [
          { id: 'ExpandedHelicopterEvents', reason: { kind: 'not_loaded' } },
          { id: 'ebfchainsaw', reason: { kind: 'not_loaded' } },
        ],
      };
    default:
      return null;
  }
}

function bisectStep(i: number): BisectView {
  const sizes = [21, 11, 6, 3, 2, 1];
  const done = i >= 5;
  return {
    profile: 'mi-partida',
    origin: 'mi-partida',
    suspects: sizes[Math.min(i, 5)],
    testing: done ? [] : ['ZBBetterFPS', 'zdk', 'TrueMusicJukebox'].slice(0, Math.max(1, 3 - i)),
    round: i + 1,
    rounds_left: done ? 0 : 5 - i,
    outcome: done ? { kind: 'culprit', data: 'ZBBetterFPS' } : null,
    mode: 'single',
    flaky: false,
    suggestion: done ? null : 'failed',
  };
}
const bisectParam = new URLSearchParams(location.search).get('bisect');
let bisectDemo: BisectView | null = bisectParam === 'done' ? bisectStep(5) : bisectParam ? bisectStep(1) : null;

let settingsDemo: AppSettingsData = {
  window_on_play: 'nothing',
  integrity_check: true,
  new_profile: { xmx: '', game_extra: [], zb_usage: 'auto', zb_policy: 'prompt', import_game_mods: false },
  clear_webview_cache: false,
};
