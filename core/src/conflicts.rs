use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::Serialize;

use crate::game::Modification;
use crate::modinfo::normalize_id;
use crate::mods::{Layout, ScannedMod, Source};
use crate::order::{self, Effect, Entry, OrderResult};
use crate::scripts;
use crate::version::Version;
use crate::zb::{self, ApprovalState, JarChoice};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    Blocks,
    Warning,
    Info,
    Detail,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum Kind {
    MissingDependency {
        requires: String,
        inactive_installed: bool,
    },
    ModNotFound,
    WontLoad {
        because: Vec<String>,
    },
    RequireTypo {
        spaced: Vec<String>,
        semicolon: Vec<String>,
        ignored: Vec<String>,
    },
    Cycle {
        edges: Vec<order::Edge>,
        hard: bool,
    },
    DuplicatedId {
        keys: Vec<String>,
        used: Option<String>,
    },
    Incompatible {
        declared: Vec<(String, String)>,
    },
    GameVersion {
        min: Option<String>,
        max: Option<String>,
        game: String,
    },
    OnlyB41,
    NoValidFolder {
        game: String,
    },
    OverriddenFiles {
        pairs: Vec<Pair>,
    },
    OverridesGame {
        files: Vec<String>,
        total: usize,
        behavior: usize,
    },
    TiledefRepeated {
        num: String,
    },
    ScriptRedefined {
        wins: String,
        objects: Vec<String>,
        total: usize,
        identical: usize,
        properties: Vec<String>,
        related: bool,
        accepted: bool,
    },
    MapCellsShared {
        maps: (String, String),
        cells: usize,
        sample: Vec<String>,
    },
    ZbNotInstalled,
    ZbVersion {
        zb: String,
        ranges: Vec<(Option<String>, Option<String>)>,
    },
    ZbJarInvalid {
        reason: String,
    },
    ZbJarMissing {
        jar: String,
    },
    ZbRepeatedPackage {
        pkg: String,
        load: String,
    },
    ZbIncompatibleGame {
        zb: String,
        game: String,
        reason: String,
    },
    ZbApproval {
        state: ApprovalState,
        policy: String,
    },
    ZbModNotEnabled,
    JavaPatchOverlap {
        methods: Vec<String>,
        replaced: Vec<String>,
    },
    SectionOrder {
        needs: String,
        section: String,
    },
    SectionSoftRule {
        after: String,
        section: String,
    },
    ModifiedInstall {
        detail: String,
    },
    JavaLinkBroken {
        missing: Vec<String>,
        total: usize,
        game: String,
    },
    LuaGlobalReplaced {
        wins: Option<String>,
        functions: Vec<LuaFn>,
        total: usize,
        related: bool,
        accepted: bool,
        movable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LuaFn {
    pub name: String,
    #[serde(default)]
    pub remade: bool,
    pub wins_at: String,
    pub lost: Vec<LuaDef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LuaDef {
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub at: String,
    pub wraps: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pair {
    pub loses: String,
    pub wins: String,
    pub files: Vec<String>,
    pub total: usize,
    pub identical: usize,
    pub not_load: Vec<String>,
    pub total_not_load: usize,
    pub lost_objects: Vec<String>,
    pub total_lost: usize,
    pub related: bool,
    pub accepted: bool,
    pub behavior: usize,
}

impl Pair {
    pub fn warns(&self) -> bool {
        (self.behavior > 0 || self.total_lost > 0) && !self.related && !self.accepted
    }
}

pub fn is_appearance(rel: &str) -> bool {
    const EXT: [&str; 14] = [
        ".png", ".dds", ".jpg", ".jpeg", ".fbx", ".x", ".ogg", ".wav", ".mp3", ".bank", ".ttf",
        ".fnt", ".glb", ".pack",
    ];
    rel.starts_with("media/clothing/")
        || rel.starts_with("media/textures/")
        || rel.starts_with("media/models_x/")
        || rel.starts_with("media/depthmaps/")
        || EXT.iter().any(|e| rel.ends_with(e))
}

fn changes_behavior(rel: &str) -> bool {
    rel.ends_with(".lua") || (rel.starts_with("media/scripts/") && rel.ends_with(".txt"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", content = "data")]
pub enum Fix {
    Enable(String),
    Disable(String),
    OpenWorkshop(u64),
    UpdateZb,
    Win(String, Vec<String>),
    Accept(Vec<(String, String)>),
    RemoveCopy(String),
    Keep(String, Vec<String>),
    MoveToSection(String, String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub mods: Vec<String>,
    pub kind: Kind,
    pub fixes: Vec<Fix>,
}

impl Finding {
    pub fn text(&self) -> String {
        let m = self.mods.join(", ");
        match &self.kind {
            Kind::MissingDependency { requires, inactive_installed: true } if self.severity != Severity::Blocks => {
                format!("«{m}» requiere «{requires}», que está instalado pero no activo: el juego lo cargará igualmente, justo antes de «{m}»")
            }
            Kind::MissingDependency { requires, inactive_installed: true } => {
                format!("«{m}» requiere «{requires}», que está instalado pero el juego no podrá cargarlo: tampoco cargará «{m}»")
            }
            Kind::MissingDependency { requires, .. } => {
                format!("«{m}» requiere «{requires}», que no está instalado: el juego no cargará «{m}» (y no avisa)")
            }
            Kind::ModNotFound => format!("«{m}» está en la lista pero no se encuentra en el Workshop ni en local"),
            Kind::WontLoad { because } => format!(
                "«{m}» no cargará: depende de {} y «{}» no carga",
                because.iter().map(|b| format!("«{b}»")).collect::<Vec<_>>().join(" → "),
                because.last().map(String::as_str).unwrap_or("?")
            ),
            Kind::RequireTypo { spaced, semicolon, ignored } => {
                let mut parts = Vec::new();
                for x in spaced {
                    parts.push(format!("el juego busca «{x}» con el espacio (required mod \"{x}\" not found) y no carga «{m}»"));
                }
                for x in semicolon {
                    parts.push(format!("«{x}»: el juego solo separa por comas, no encuentra ese mod y no carga «{m}» (sin avisar)"));
                }
                if !ignored.is_empty() {
                    parts.push(format!("el juego no lee require {} (clave con mayúscula o línea anterior a otra require=)", ignored.join(", ")));
                }
                format!("mod.info de «{m}»: {}", parts.join("; "))
            }
            Kind::Cycle { hard, .. } => format!(
                "ciclo entre {m}{}",
                if *hard {
                    " (de require: el juego carga los requisitos sin vigilar bucles y se cierra al arrancar con StackOverflowError)"
                } else {
                    " (cede la regla blanda)"
                }
            ),
            Kind::DuplicatedId { keys, used: Some(u) } => format!(
                "«{m}» existe en varios sitios: el juego usa {u} e ignora {}",
                keys.iter().filter(|k| *k != u).cloned().collect::<Vec<_>>().join(", ")
            ),
            Kind::DuplicatedId { keys, .. } => format!("«{m}» existe en varios sitios: {}", keys.join(", ")),
            Kind::Incompatible { declared } => format!(
                "no deben ir juntos ({}): el juego los carga igualmente",
                declared
                    .iter()
                    .map(|(a, b)| format!("«{a}» declara incompatible a «{b}»"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Kind::GameVersion { min, max, game } => format!(
                "«{m}» pide el juego {}–{} y tienes {game}: el juego no lo cargará",
                min.as_deref().unwrap_or("…"),
                max.as_deref().unwrap_or("…")
            ),
            Kind::OnlyB41 => format!("«{m}» solo tiene estructura de Build 41: Build 42 no lo carga"),
            Kind::NoValidFolder { game } => format!("«{m}» no tiene carpeta de versión para {game} ni mod.info en common"),
            Kind::OverriddenFiles { pairs } => format!(
                "{m} tienen archivos en las mismas rutas: {}",
                pairs
                    .iter()
                    .map(|p| format!(
                        "«{}» pisa a «{}» en {}{}{}{}{}{}",
                        p.wins,
                        p.loses,
                        p.total,
                        if p.total_lost > 0 {
                            format!(
                                ", «{}» pierde {} objeto(s) de script ({}{})",
                                p.loses,
                                p.total_lost,
                                p.lost_objects.join(", "),
                                if p.total_lost > p.lost_objects.len() { ", …" } else { "" }
                            )
                        } else {
                            String::new()
                        },
                        if p.related {
                            " (relacionados: uno requiere al otro o son del mismo elemento)"
                        } else if p.accepted {
                            " (dado por bueno)"
                        } else {
                            ""
                        },
                        if p.identical > 0 { format!(", {} idénticos", p.identical) } else { String::new() },
                        if p.total_not_load > 0 { format!(", {} que el juego no carga", p.total_not_load) } else { String::new() },
                        if p.files.is_empty() { String::new() } else { format!(" (ej.: {})", p.files.join(", ")) },
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Kind::OverridesGame { files, total, behavior } => format!(
                "«{m}» sustituye {total} archivo(s) del juego{}. Ej.: {}",
                if *behavior > 0 { format!(", {behavior} de Lua o scripts") } else { String::new() },
                files.join(", ")
            ),
            Kind::TiledefRepeated { num } => format!("{m} usan el mismo tiledef {num}"),
            Kind::JavaLinkBroken { missing, total, game } => format!(
                "«{m}» usa {total} cosa(s) del juego que no existen en {game} (compilado para otra versión): {}",
                missing.join(", ")
            ),
            Kind::LuaGlobalReplaced { wins, functions, total, related, movable, .. } => format!(
                "{total} función(es) Lua global(es) definidas por varios: manda {} y no se ejecutan las de {}{}{}. Ej.: {}",
                wins.as_deref().map_or("el juego".to_string(), |w| format!("«{w}»")),
                functions
                    .iter()
                    .flat_map(|f| f.lost.iter().map(|l| format!("«{}»", l.mod_id)))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", "),
                if *related { " (relacionados: probablemente un parche)" } else { "" },
                if *movable { "" } else { " (un archivo pisa a otro: el orden no lo cambia)" },
                functions
                    .iter()
                    .map(|f| format!(
                        "{} {} {} (pierden: {})",
                        f.name,
                        if f.remade { "borrada al recrear su tabla en" } else { "en" },
                        f.wins_at,
                        f.lost
                            .iter()
                            .map(|l| format!("{} {}{}", l.mod_id, l.at, if l.wraps { ", envolvía" } else { "" }))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
            Kind::ScriptRedefined { total: 0, identical, .. } => {
                format!("{m} definen {identical} objeto(s) de script idénticos: da igual cuál mande")
            }
            Kind::ScriptRedefined { wins, objects, total, identical, properties, related, accepted } => format!(
                "{m} definen {total} objeto(s) de script distintos con el mismo nombre ({}{}): mandan las propiedades de «{wins}» porque su archivo va después por nombre; el orden de mods no lo cambia{}{}{}",
                objects.join(", "),
                if *total > objects.len() { ", …" } else { "" },
                if properties.is_empty() { String::new() } else { format!(" (cambian: {})", properties.join(", ")) },
                if *identical > 0 { format!(" ({identical} más son idénticos)") } else { String::new() },
                if *related { " (relacionados: uno requiere al otro o son del mismo elemento)" } else if *accepted { " (dado por bueno)" } else { "" }
            ),
            Kind::MapCellsShared { maps: (a, b), cells, sample } => format!(
                "los mapas «{a}» y «{b}» comparten {cells} celda(s) ({}): manda «{a}» porque va antes en el orden de mapas",
                sample.join(", ")
            ),
            Kind::ZbNotInstalled => format!("«{m}» es un mod Java y ZombieBuddy no está instalado en el juego"),
            Kind::ZbVersion { zb, ranges } => format!(
                "«{m}» no vale para ZombieBuddy {zb} (pide {}): ZB no cargará su parte Java",
                ranges
                    .iter()
                    .map(|(a, b)| format!("{}–{}", a.as_deref().unwrap_or("…"), b.as_deref().unwrap_or("…")))
                    .collect::<Vec<_>>()
                    .join(" o ")
            ),
            Kind::ZbJarInvalid { reason } => format!("«{m}»: mod.info Java no válido ({reason})"),
            Kind::ZbJarMissing { jar } => format!("«{m}»: el JAR {jar} no existe"),
            Kind::ZbRepeatedPackage { pkg, load } => {
                format!("{m} usan el paquete Java {pkg}: ZB solo carga «{load}»")
            }
            Kind::ZbIncompatibleGame { reason, .. } => reason.clone(),
            Kind::ZbApproval { state, policy } => match (state, policy.as_str()) {
                (ApprovalState::Denied, _) => format!("«{m}»: lo denegaste en ZombieBuddy; no se cargará"),
                (ApprovalState::Pending, "prompt") => {
                    format!("«{m}»: JAR nuevo o cambiado; ZombieBuddy te pedirá aprobarlo al arrancar")
                }
                (_, "deny-new") => format!("«{m}»: sin aprobación guardada; con la política deny-new no se cargará"),
                _ => format!("«{m}»: {state:?}"),
            },
            Kind::ZbModNotEnabled => {
                "hay mods Java activos y el mod «ZombieBuddy» no está en la lista: sin él no se autoactualiza".into()
            }
            Kind::JavaPatchOverlap { methods, replaced } if !replaced.is_empty() => format!(
                "{m} sustituyen los mismos métodos del juego ({}): ZombieBuddy solo admite una sustitución por método, uno de los dos no funcionará",
                replaced.join(", ")
            ),
            Kind::JavaPatchOverlap { methods, .. } => format!(
                "{m} parchean los mismos métodos del juego ({}): suelen encadenarse; si el juego falla ahí, son los primeros sospechosos",
                methods.join(", ")
            ),
            Kind::SectionOrder { needs, .. } => format!(
                "«{m}» necesita «{needs}», que está en una sección posterior: el juego lo cargará antes igualmente"
            ),
            Kind::SectionSoftRule { after, section } => format!(
                "«{m}» pide cargar después de «{after}», que está en la sección posterior «{section}»: no se aplica"
            ),
            Kind::ModifiedInstall { detail } => format!("instalación del juego modificada: {detail}"),
        }
    }
}

pub fn java_patch_overlaps(patched: &[(String, Vec<zb::JavaPatch>)]) -> Vec<Finding> {
    let methods = |p: &[zb::JavaPatch]| -> BTreeMap<String, bool> {
        let mut out: BTreeMap<String, bool> = BTreeMap::new();
        for x in p.iter().filter(|x| !x.method_name.is_empty()) {
            *out.entry(format!("{}.{}", x.class_name, x.method_name))
                .or_default() |= x.replaces;
        }
        out
    };
    let all: Vec<_> = patched.iter().map(|(id, p)| (id, methods(p))).collect();
    let mut out = Vec::new();
    for (i, (a, ma)) in all.iter().enumerate() {
        for (b, mb) in &all[i + 1..] {
            let shared: Vec<String> = ma.keys().filter(|k| mb.contains_key(*k)).cloned().collect();
            if shared.is_empty() {
                continue;
            }
            let replaced: Vec<String> = shared
                .iter()
                .filter(|k| ma[*k] && mb[*k])
                .cloned()
                .collect();
            out.push(Finding {
                severity: if replaced.is_empty() {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                mods: vec![(*a).clone(), (*b).clone()],
                kind: Kind::JavaPatchOverlap {
                    methods: shared,
                    replaced,
                },
                fixes: vec![],
            });
        }
    }
    out
}

pub fn used_copy<'a>(copies: &[&'a ScannedMod]) -> Option<&'a ScannedMod> {
    let steam: Vec<&&ScannedMod> = copies
        .iter()
        .filter(|m| matches!(m.source, crate::mods::Source::Workshop { .. }))
        .collect();
    match steam.len() {
        1 => Some(*steam[0]),
        0 if copies.len() == 1 => copies.first().copied(),
        _ => None,
    }
}

pub struct Context<'a> {
    pub game: Version,
    pub game_files: Option<&'a std::collections::BTreeSet<String>>,
    pub game_folder: Option<&'a Path>,
    pub zb: Option<String>,
    pub policy_zb: String,
    pub approvals: Vec<zb::Approval>,
    pub trust: Vec<u64>,
    pub modifications: Vec<Modification>,
    pub accepted_overrides: Vec<(String, String)>,
    pub maps: Vec<String>,
    pub sections: Vec<String>,
}

pub struct ProfileEntry<'a> {
    pub id: String,
    pub enabled: bool,
    pub pinned: bool,
    pub found: Option<&'a ScannedMod>,
    pub section: usize,
}

const SHOWS: usize = 4;

pub fn is_merged(rel: &str) -> bool {
    const BY_MOD: [&str; 9] = [
        "media/sandbox-options.txt",
        "media/registries.lua",
        "media/fileguidtable.xml",
        "media/clothing/clothing.xml",
        "media/perks.txt",
        "media/seating.txt",
        "media/tilegeometry.txt",
        "media/tiledepthtextureassignments.txt",
        "media/hairstyles/hairstyles.xml",
    ];
    rel.starts_with("media/lua/shared/translate/")
        || BY_MOD.contains(&rel)
        || rel.rsplit('/').next().is_some_and(|n| n.starts_with('.'))
}

const EXTENSIONS_READ: [&str; 28] = [
    "png",
    "jpg",
    "jpeg",
    "txt",
    "xml",
    "lua",
    "json",
    "x",
    "fbx",
    "glb",
    "ogg",
    "wav",
    "bank",
    "pack",
    "tiles",
    "lotheader",
    "lotpack",
    "bin",
    "tmx",
    "fnt",
    "ini",
    "cfg",
    "floor",
    "vert",
    "frag",
    "glsl",
    "csv",
    "pzby",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    Reads,
    NotLoaded,
    Unverified,
}

pub fn read(rel: &str) -> Load {
    if rel
        .strip_prefix("media/actiongroups/")
        .is_some_and(|r| !r.contains('/'))
    {
        return Load::NotLoaded;
    }
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rsplit_once('.') {
        Some((_, ext)) if EXTENSIONS_READ.contains(&ext) => Load::Reads,
        _ => Load::Unverified,
    }
}

fn fingerprint(path: &Path) -> Option<String> {
    type Cache = Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let md = std::fs::metadata(path).ok()?;
    let (size, date) = (md.len(), md.modified().ok()?);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((t, f, h)) = cache.lock().ok()?.get(path)
        && *t == size
        && *f == date
    {
        return Some(h.clone());
    }
    let h = crate::backups::sha256(path).ok()?;
    cache
        .lock()
        .ok()?
        .insert(path.to_path_buf(), (size, date, h.clone()));
    Some(h)
}

type GameMemo = HashMap<PathBuf, (Option<u64>, Option<Option<String>>)>;
static GAME_FILES: std::sync::LazyLock<Mutex<GameMemo>> =
    std::sync::LazyLock::new(Default::default);

pub(crate) fn forget_game_files() {
    if let Ok(mut g) = GAME_FILES.lock() {
        g.clear();
    }
    if let Ok(mut g) = GAME_LUA.lock() {
        g.clear();
    }
}

static GAME_LUA: std::sync::LazyLock<Mutex<HashMap<String, Vec<crate::lua::Def>>>> =
    std::sync::LazyLock::new(Default::default);

fn with_game_lua<R>(
    game: &Path,
    rels: &[&str],
    f: impl FnOnce(&HashMap<String, Vec<crate::lua::Def>>) -> R,
) -> R {
    let missing: Vec<&str> = {
        let g = GAME_LUA.lock().ok();
        rels.iter()
            .copied()
            .filter(|r| g.as_ref().is_none_or(|g| !g.contains_key(*r)))
            .collect()
    };
    if !missing.is_empty() {
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(2, 8);
        let read: Vec<(String, Vec<crate::lua::Def>)> = std::thread::scope(|s| {
            let parts: Vec<_> = missing
                .chunks(missing.len().div_ceil(threads))
                .map(|chunk| {
                    s.spawn(move || {
                        chunk
                            .iter()
                            .map(|r| (r.to_string(), crate::lua::definitions_of(&game.join(r))))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            parts
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        if let Ok(mut g) = GAME_LUA.lock() {
            g.extend(read);
        }
    }
    match GAME_LUA.lock() {
        Ok(g) => f(&g),
        Err(_) => f(&HashMap::new()),
    }
}

fn game_size(path: &Path) -> Option<u64> {
    if let Some((s, _)) = GAME_FILES.lock().ok().and_then(|g| g.get(path).cloned()) {
        return s;
    }
    let s = std::fs::metadata(path).ok().map(|m| m.len());
    if let Ok(mut g) = GAME_FILES.lock() {
        g.entry(path.to_path_buf()).or_insert((s, None)).0 = s;
    }
    s
}

fn game_hash(path: &Path) -> Option<String> {
    if let Some(Some(h)) = GAME_FILES
        .lock()
        .ok()
        .and_then(|g| g.get(path).map(|x| x.1.clone()))
    {
        return h;
    }
    let h = crate::backups::sha256(path).ok();
    if let Ok(mut g) = GAME_FILES.lock() {
        g.entry(path.to_path_buf()).or_insert((None, None)).1 = Some(h.clone());
    }
    h
}

fn same_file(a: &ScannedMod, b: &ScannedMod, rel: &str) -> bool {
    match (a.size_of(rel), b.size_of(rel)) {
        (Some(x), Some(y)) if x == y => a.hash_of(rel).is_some_and(|h| Some(h) == b.hash_of(rel)),
        _ => false,
    }
}

fn same_as_game(m: &ScannedMod, rel: &str, game: &Path) -> bool {
    let path = crate::paths::long(&game.join(rel));
    match (m.size_of(rel), game_size(&path)) {
        (Some(x), Some(y)) if x == y => m.hash_of(rel).is_some_and(|h| Some(h) == game_hash(&path)),
        _ => false,
    }
}

struct Run<'a> {
    pos: usize,
    owner: Option<usize>,
    rel: &'a str,
    def: Cow<'a, crate::lua::Def>,
    moved: bool,
}

type ByName<'a> = HashMap<Cow<'a, str>, Vec<Run<'a>>>;

fn lua_runs<'a>(mods: &[&'a ScannedMod], ctx: &Context<'a>) -> ByName<'a> {
    let is_lua = |r: &&String| crate::lua::is_code(r);
    let mod_rels: Vec<Vec<&str>> = mods
        .iter()
        .map(|m| {
            m.relative_paths()
                .iter()
                .filter(is_lua)
                .map(String::as_str)
                .collect()
        })
        .collect();
    let game_rels: Vec<&str> = ctx
        .game_files
        .map(|g| g.iter().filter(is_lua).map(String::as_str).collect())
        .unwrap_or_default();
    let mut winner: HashMap<&str, usize> = HashMap::new();
    for (k, rels) in mod_rels.iter().enumerate() {
        for r in rels {
            winner.insert(r, k);
        }
    }
    let order = crate::lua::run_order(game_rels.iter().copied(), &mod_rels);
    let with_lua: Vec<&ScannedMod> = mods
        .iter()
        .zip(&mod_rels)
        .filter(|(_, r)| !r.is_empty())
        .map(|(m, _)| *m)
        .collect();
    ScannedMod::lua_all(&with_lua);
    let by_path: HashMap<&str, Option<usize>> = order.iter().copied().collect();
    let requires_of = |rel: &str| -> Vec<&'a str> {
        let Some(&w) = winner.get(rel) else {
            return Vec::new();
        };
        mods[w]
            .lua_definitions()
            .get(rel)
            .into_iter()
            .flatten()
            .filter_map(|d| d.name.strip_prefix('^'))
            .filter_map(|p| {
                let p = p.trim_start_matches('/').to_lowercase();
                ["shared", "client", "server"].iter().find_map(|f| {
                    let r = format!("media/lua/{f}/{p}.lua");
                    by_path.get_key_value(r.as_str()).map(|(k, _)| *k)
                })
            })
            .collect()
    };
    let mut done: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut sequence: Vec<(&'a str, Option<usize>)> = Vec::with_capacity(order.len());
    for &(rel, first) in &order {
        let mut stack: Vec<(&'a str, Option<usize>, bool)> = vec![(rel, first, false)];
        while let Some((r, f, expanded)) = stack.pop() {
            if done.contains(r) {
                continue;
            }
            if expanded {
                done.insert(r);
                sequence.push((r, f));
                continue;
            }
            stack.push((r, f, true));
            for q in requires_of(r).into_iter().rev() {
                if !done.contains(q) {
                    stack.push((q, by_path.get(q).copied().flatten(), false));
                }
            }
        }
    }
    let order = sequence;
    let mut by_name: ByName<'a> = HashMap::new();
    for (pos, (rel, first)) in order.iter().enumerate() {
        let Some(&w) = winner.get(rel) else { continue };
        let m: &'a ScannedMod = mods[w];
        let Some(defs) = m.lua_definitions().get(*rel) else {
            continue;
        };
        for def in defs.iter().filter(|d| !d.name.starts_with('^')) {
            by_name
                .entry(Cow::Borrowed(&def.name))
                .or_default()
                .push(Run {
                    pos,
                    owner: Some(w),
                    rel,
                    def: Cow::Borrowed(def),
                    moved: *first != Some(w),
                });
        }
    }
    if let Some(game) = ctx.game_folder
        && !by_name.is_empty()
    {
        let tables: std::collections::HashSet<&'a str> = by_name
            .keys()
            .filter_map(|n| match n {
                Cow::Borrowed(n) => Some(*n),
                Cow::Owned(_) => None,
            })
            .flat_map(|n| n.match_indices('.').map(move |(i, _)| &n[..i]))
            .collect();
        let own: Vec<&str> = game_rels
            .iter()
            .copied()
            .filter(|r| !winner.contains_key(r))
            .collect();
        with_game_lua(game, &own, |defs| {
            for (pos, (rel, _)) in order.iter().enumerate() {
                if winner.contains_key(rel) {
                    continue;
                }
                for def in defs.get(*rel).into_iter().flatten() {
                    let wanted = by_name.contains_key(def.name.as_str())
                        || (def.reset && tables.contains(def.name.as_str()));
                    if wanted {
                        by_name
                            .entry(Cow::Owned(def.name.clone()))
                            .or_default()
                            .push(Run {
                                pos,
                                owner: None,
                                rel,
                                def: Cow::Owned(def.clone()),
                                moved: false,
                            });
                    }
                }
            }
        });
    }
    by_name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaPhase {
    Boot,
    Start,
}

const BOOT_EVENTS: [&str; 2] = ["OnGameBoot", "OnMainMenuEnter"];
const START_EVENTS: [&str; 5] = [
    "OnInitWorld",
    "OnInitGlobalModData",
    "OnLoad",
    "OnGameStart",
    "OnCreatePlayer",
];

fn stage(r: &Run, phase: LuaPhase) -> Option<(usize, usize, u32)> {
    use crate::lua::When;
    let s = match &r.def.when {
        When::Load if phase == LuaPhase::Boot && r.rel.starts_with("media/lua/server/") => {
            return None;
        }
        When::Load => 0,
        When::Event(e) => match (
            BOOT_EVENTS.iter().position(|x| x == e),
            START_EVENTS.iter().position(|x| x == e),
        ) {
            (Some(p), _) => 1 + p,
            (None, Some(p)) if phase == LuaPhase::Start => 10 + p,
            _ => return None,
        },
        When::Later => return None,
    };
    Some((s, r.pos, r.def.line))
}

fn timeline<'b, 'a>(
    name: &str,
    by_name: &'b ByName<'a>,
    phase: LuaPhase,
    strict: bool,
) -> Option<Timeline<'b, 'a>> {
    let defs = by_name.get(name)?;
    let method = name.rsplit('.').next().unwrap_or(name);
    let hooked = by_name
        .get(format!("*.{method}").as_str())
        .is_some_and(|v| v.iter().any(|r| r.owner.is_some()));
    if (strict && hooked)
        || defs.iter().any(|r| {
            r.owner.is_some() && (strict || !r.def.wraps) && r.def.when == crate::lua::When::Later
        })
    {
        return None;
    }
    let mut v: Vec<((usize, usize, u32), &Run)> = defs
        .iter()
        .filter(|r| !r.def.reset)
        .filter_map(|r| stage(r, phase).map(|k| (k, r)))
        .collect();
    v.sort_by_key(|(k, _)| *k);
    let last_reset = name
        .match_indices('.')
        .filter_map(|(i, _)| by_name.get(&name[..i]))
        .flatten()
        .filter(|r| r.def.reset)
        .filter_map(|r| stage(r, phase).map(|k| (k, r)))
        .max_by_key(|(k, _)| *k);
    let (removed, alive): (Vec<_>, Vec<_>) = v
        .into_iter()
        .partition(|(k, _)| last_reset.is_some_and(|(l, _)| *k < l));
    Some(Timeline {
        alive: alive.into_iter().map(|(_, r)| r).collect(),
        removed: removed.into_iter().map(|(_, r)| r).collect(),
        reset: last_reset.map(|(_, r)| r),
    })
}

struct Timeline<'b, 'a> {
    alive: Vec<&'b Run<'a>>,
    removed: Vec<&'b Run<'a>>,
    reset: Option<&'b Run<'a>>,
}

fn lua_globals(
    loaded: &[(&ProfileEntry, &ScannedMod)],
    loaded_ids: &[String],
    ctx: &Context,
    requires: &dyn Fn(usize, usize) -> bool,
    out: &mut Vec<Finding>,
) {
    let mods: Vec<&ScannedMod> = loaded.iter().map(|(_, m)| *m).collect();
    let by_name = lua_runs(&mods, ctx);
    let file = |r: &str| r.rsplit('/').next().unwrap_or(r).to_string();
    let content = |r: &Run| match r.owner {
        Some(o) => loaded[o].1.hash_of(r.rel),
        None => ctx
            .game_folder
            .and_then(|g| game_hash(&crate::paths::long(&g.join(r.rel)))),
    };
    type Key = (Option<usize>, BTreeSet<usize>);
    let mut groups: BTreeMap<Key, (Vec<LuaFn>, bool)> = BTreeMap::new();
    let mut names: Vec<&str> = by_name
        .iter()
        .filter(|(n, v)| {
            !n.starts_with("*.") && v.iter().any(|r| !r.def.reset && r.owner.is_some())
        })
        .filter(|(n, v)| {
            v.len() > 1
                || n.match_indices('.')
                    .filter_map(|(i, _)| by_name.get(&n[..i]))
                    .flatten()
                    .any(|r| r.def.reset)
        })
        .map(|(n, _)| n.as_ref())
        .collect();
    names.sort();
    for name in names {
        let Some(Timeline {
            alive,
            removed,
            reset,
        }) = timeline(name, &by_name, LuaPhase::Start, false)
        else {
            continue;
        };
        let reset_content = reset.and_then(content);
        let removed: Vec<&Run> = removed
            .into_iter()
            .filter(|r| r.owner.is_some() && r.owner != reset.and_then(|x| x.owner))
            .filter(|r| reset_content.is_none() || content(r) != reset_content)
            .collect();
        let remade = !removed.is_empty();
        let (wins, lost): (Option<&Run>, Vec<&Run>) = if remade {
            (reset, removed)
        } else {
            let k = alive.iter().rposition(|r| !r.def.wraps);
            let wins = k.map(|k| alive[k]);
            let wins_content = wins.and_then(content);
            let lost = alive[..k.unwrap_or(0)]
                .iter()
                .copied()
                .filter(|r| r.owner.is_some() && r.owner != wins.and_then(|w| w.owner))
                .filter(|r| wins_content.is_none() || content(r) != wins_content)
                .collect();
            (wins, lost)
        };
        if lost.is_empty() {
            continue;
        }
        let owner = wins.and_then(|w| w.owner);
        let moved = wins.is_some_and(|w| w.moved) || lost.iter().any(|r| r.moved) || remade;
        let key: Key = (owner, lost.iter().filter_map(|r| r.owner).collect());
        let g = groups.entry(key).or_default();
        g.1 |= moved;
        g.0.push(LuaFn {
            name: name.to_string(),
            remade,
            wins_at: wins.map_or_else(String::new, |w| format!("{}:{}", file(w.rel), w.def.line)),
            lost: lost
                .iter()
                .map(|r| LuaDef {
                    mod_id: loaded_ids[r.owner.unwrap_or_default()].clone(),
                    at: format!("{}:{}", file(r.rel), r.def.line),
                    wraps: r.def.wraps,
                })
                .collect(),
        });
    }
    for ((wins, losers), (functions, moved)) in groups {
        let related =
            wins.is_some_and(|w| losers.iter().all(|&l| requires(l, w) || requires(w, l)));
        let pairs: Vec<(String, String)> = wins
            .map(|w| {
                losers
                    .iter()
                    .map(|&l| (loaded_ids[l].clone(), loaded_ids[w].clone()))
                    .collect()
            })
            .unwrap_or_default();
        let accepted = !pairs.is_empty()
            && pairs.iter().all(|(l, w)| {
                ctx.accepted_overrides
                    .iter()
                    .any(|(a, b)| normalize_id(a) == *l && normalize_id(b) == *w)
            });
        let warns = !related && !accepted;
        let mut fixes = Vec::new();
        if warns {
            if let Some(w) = wins
                && !moved
            {
                for &l in &losers {
                    fixes.push(Fix::Win(loaded_ids[l].clone(), vec![loaded_ids[w].clone()]));
                }
            }
            if !pairs.is_empty() {
                fixes.push(Fix::Accept(pairs));
            }
        }
        let mut mods: Vec<String> = losers.iter().map(|&l| loaded_ids[l].clone()).collect();
        if let Some(w) = wins {
            mods.push(loaded_ids[w].clone());
        }
        out.push(Finding {
            severity: if warns {
                Severity::Warning
            } else {
                Severity::Info
            },
            mods,
            kind: Kind::LuaGlobalReplaced {
                wins: wins.map(|w| loaded_ids[w].clone()),
                total: functions.len(),
                functions: functions.into_iter().take(SHOWS).collect(),
                related,
                accepted,
                movable: !moved,
            },
            fixes,
        });
    }
}

pub fn lua_predictions(
    entries: &[ProfileEntry],
    visible: &[ScannedMod],
    ctx: &Context,
    phase: LuaPhase,
) -> Vec<(String, String)> {
    let mods = loaded_mods(entries, visible, ctx);
    let by_name = lua_runs(&mods, ctx);
    let mut out: Vec<(String, String)> = contested(&by_name)
        .filter_map(|name| {
            let alive = timeline(name, &by_name, phase, true)?.alive;
            let last = alive.last()?;
            Some((
                name.to_string(),
                last.rel.rsplit('/').next().unwrap_or(last.rel).to_string(),
            ))
        })
        .collect();
    out.sort();
    out
}

pub fn lua_contested(
    entries: &[ProfileEntry],
    visible: &[ScannedMod],
    ctx: &Context,
) -> Vec<String> {
    let mods = loaded_mods(entries, visible, ctx);
    let by_name = lua_runs(&mods, ctx);
    let mut out: Vec<String> = contested(&by_name).map(str::to_string).collect();
    out.sort();
    out
}

fn contested<'b>(by_name: &'b ByName) -> impl Iterator<Item = &'b str> {
    by_name
        .iter()
        .filter(|(n, v)| {
            !n.starts_with("*.")
                && v.iter()
                    .filter(|r| !r.def.reset)
                    .map(|r| r.owner)
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
        })
        .map(|(n, _)| n.as_ref())
}

fn loaded_mods<'a>(
    entries: &[ProfileEntry<'a>],
    visible: &'a [ScannedMod],
    ctx: &Context,
) -> Vec<&'a ScannedMod> {
    let mut by_id: BTreeMap<String, Vec<&ScannedMod>> = BTreeMap::new();
    for m in visible {
        if let Some(id) = m.id() {
            by_id.entry(normalize_id(id)).or_default().push(m);
        }
    }
    let in_list: BTreeMap<String, &ScannedMod> = entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| e.found.map(|m| (normalize_id(&e.id), m)))
        .collect();
    let active: Vec<&ProfileEntry> = entries.iter().filter(|e| e.enabled).collect();
    game_load_order(&active, &by_id, ctx.game)
        .into_iter()
        .filter_map(|id| {
            in_list.get(&id).copied().or_else(|| {
                by_id
                    .get(&id)
                    .and_then(|v| used_copy(v).or(v.first().copied()))
            })
        })
        .collect()
}

fn same_content(a: Option<PathBuf>, b: Option<&PathBuf>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).ok();
    match (size(&a), size(b)) {
        (Some(x), Some(y)) if x == y => fingerprint(&a).is_some_and(|h| Some(h) == fingerprint(b)),
        _ => false,
    }
}

fn valid_for(m: &ScannedMod, game: Version) -> bool {
    m.info.as_ref().is_some_and(|i| {
        !(i.version_min().is_some_and(|v| v > game) || i.version_max().is_some_and(|v| v < game))
    })
}

fn game_load_order(
    active: &[&ProfileEntry],
    by_id: &BTreeMap<String, Vec<&ScannedMod>>,
    game: Version,
) -> Vec<String> {
    let in_list: BTreeMap<String, &ScannedMod> = active
        .iter()
        .filter_map(|e| e.found.map(|m| (normalize_id(&e.id), m)))
        .collect();
    let search = |id: &str| {
        in_list
            .get(id)
            .copied()
            .or_else(|| {
                by_id
                    .get(id)
                    .and_then(|v| used_copy(v).or(v.first().copied()))
            })
            .filter(|m| valid_for(m, game))
    };
    fn put<'a>(
        id: &str,
        search: &dyn Fn(&str) -> Option<&'a ScannedMod>,
        in_progress: &mut BTreeSet<String>,
        out: &mut Vec<String>,
    ) -> bool {
        if out.iter().any(|x| x == id) || in_progress.contains(id) {
            return true;
        }
        let Some(m) = search(id) else { return false };
        in_progress.insert(id.to_string());
        let reqs: Vec<String> = m
            .info
            .iter()
            .flat_map(|i| i.game_require.iter().flatten())
            .filter(|r| !r.is_empty())
            .cloned()
            .collect();
        let ok = reqs.iter().all(|r| put(r, search, in_progress, out));
        in_progress.remove(id);
        if ok {
            out.push(id.to_string());
        }
        ok
    }
    let mut out = Vec::new();
    let mut in_progress = BTreeSet::new();
    for e in active {
        put(&normalize_id(&e.id), &search, &mut in_progress, &mut out);
    }
    out
}

pub fn predicted_load(
    entries: &[ProfileEntry],
    visible: &[ScannedMod],
    game: Version,
) -> Vec<String> {
    let mut by_id: BTreeMap<String, Vec<&ScannedMod>> = BTreeMap::new();
    for m in visible {
        if let Some(id) = m.id() {
            by_id.entry(normalize_id(id)).or_default().push(m);
        }
    }
    let active: Vec<&ProfileEntry> = entries.iter().filter(|e| e.enabled).collect();
    game_load_order(&active, &by_id, game)
}

pub fn predicted_overrides(
    entries: &[ProfileEntry],
    visible: &[ScannedMod],
    game: Version,
    in_game: &dyn Fn(&str) -> bool,
) -> BTreeSet<(String, String)> {
    let order = predicted_load(entries, visible, game);
    let find = |id: &str| {
        entries
            .iter()
            .filter(|e| e.enabled)
            .find(|e| normalize_id(&e.id) == id)
            .and_then(|e| e.found)
            .or_else(|| {
                visible
                    .iter()
                    .find(|m| m.id().map(normalize_id).as_deref() == Some(id))
            })
    };
    let loaded: Vec<(&String, &ScannedMod)> = order
        .iter()
        .filter_map(|id| Some((id, find(id)?)))
        .collect();
    ScannedMod::index_all(&loaded.iter().map(|(_, m)| *m).collect::<Vec<_>>());
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = BTreeSet::new();
    for (id, m) in &loaded {
        let own: BTreeSet<&str> = m.shadowed_paths().collect();
        for rel in m.relative_paths() {
            if own.contains(rel.as_str()) || seen.contains(rel.as_str()) || in_game(rel) {
                out.insert(((*id).clone(), rel.clone()));
            }
            seen.insert(rel.as_str());
        }
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct PredictionCheck {
    pub order_ok: bool,
    pub not_loaded: Vec<String>,
    pub unexpected: Vec<String>,
    pub overrides_missed: usize,
    pub overrides_extra: usize,
    pub examples: Vec<String>,
}

pub fn check_prediction(
    order: &[String],
    overrides: &BTreeSet<(String, String)>,
    real_order: &[String],
    real_overrides: &[(String, String)],
) -> PredictionCheck {
    let real_order: Vec<String> = real_order
        .iter()
        .map(|m| normalize_id(m))
        .filter(|m| m != crate::probe::ID)
        .collect();
    let real: BTreeSet<(String, String)> = real_overrides
        .iter()
        .filter(|(_, p)| p.starts_with("media/"))
        .map(|(m, p)| (normalize_id(m), p.to_lowercase()))
        .collect();
    let predicted: BTreeSet<(String, String)> = overrides
        .iter()
        .filter(|(_, p)| p.starts_with("media/"))
        .cloned()
        .collect();
    let missed: Vec<_> = real.difference(&predicted).collect();
    let extra: Vec<_> = predicted.difference(&real).collect();
    let mut examples: Vec<String> = missed
        .iter()
        .take(10)
        .map(|(m, p)| format!("no previsto: {m}: {p}"))
        .collect();
    examples.extend(
        extra
            .iter()
            .take(10)
            .map(|(m, p)| format!("no pasó: {m}: {p}")),
    );
    PredictionCheck {
        order_ok: order == real_order.as_slice(),
        not_loaded: order
            .iter()
            .filter(|m| !real_order.contains(m))
            .cloned()
            .collect(),
        unexpected: real_order
            .iter()
            .filter(|m| !order.contains(m))
            .cloned()
            .collect(),
        overrides_missed: missed.len(),
        overrides_extra: extra.len(),
        examples,
    }
}

fn winners(
    active: &[&ProfileEntry],
    by_id: &BTreeMap<String, Vec<&ScannedMod>>,
    game: Version,
) -> BTreeMap<String, (String, PathBuf)> {
    let in_list: BTreeMap<String, &ScannedMod> = active
        .iter()
        .filter_map(|e| e.found.map(|m| (normalize_id(&e.id), m)))
        .collect();
    let loaded: Vec<(String, &ScannedMod)> = game_load_order(active, by_id, game)
        .into_iter()
        .filter_map(|id| {
            let m = in_list.get(&id).copied().or_else(|| {
                by_id
                    .get(&id)
                    .and_then(|v| used_copy(v).or(v.first().copied()))
            })?;
            Some((id, m))
        })
        .collect();
    ScannedMod::index_all(&loaded.iter().map(|(_, m)| *m).collect::<Vec<_>>());
    let mut owners: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (_, m)) in loaded.iter().enumerate() {
        for rel in m
            .relative_paths()
            .iter()
            .filter(|r| r.starts_with("media/") && !is_merged(r) && read(r) == Load::Reads)
        {
            owners.entry(rel.as_str()).or_default().push(i);
        }
    }
    owners
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .filter_map(|(rel, v)| {
            let (id, m) = &loaded[*v.last()?];
            Some((rel.to_string(), (id.clone(), m.path_of(rel)?)))
        })
        .collect()
}

pub fn mark_winner_moves(
    result: &mut OrderResult,
    entries: &[ProfileEntry],
    visible: &[ScannedMod],
    game: Version,
) {
    let mut by_id: BTreeMap<String, Vec<&ScannedMod>> = BTreeMap::new();
    for m in visible {
        if let Some(id) = m.id() {
            by_id.entry(normalize_id(id)).or_default().push(m);
        }
    }
    let active: Vec<&ProfileEntry> = entries.iter().filter(|e| e.enabled).collect();
    let slot: BTreeMap<&str, usize> = result
        .order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut proposed = active.clone();
    proposed.sort_by_key(|e| {
        slot.get(normalize_id(&e.id).as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    let before = winners(&active, &by_id, game);
    let after = winners(&proposed, &by_id, game);
    let mut changed: BTreeSet<&str> = BTreeSet::new();
    for (rel, (old, old_path)) in &before {
        if let Some((new, new_path)) = after.get(rel)
            && old != new
            && !same_content(Some(old_path.clone()), Some(new_path))
        {
            changed.insert(old);
            changed.insert(new);
        }
    }
    for m in &mut result.moves {
        if m.effect == Effect::Category && changed.contains(m.id.as_str()) {
            m.effect = Effect::Winner;
        }
    }
}

fn loads_alone(
    id: &str,
    active: &[&ProfileEntry],
    by_id: &BTreeMap<String, Vec<&ScannedMod>>,
    game: Version,
) -> bool {
    let probe = ProfileEntry {
        id: id.to_string(),
        enabled: true,
        pinned: false,
        found: None,
        section: 0,
    };
    let mut list: Vec<&ProfileEntry> = active.to_vec();
    list.push(&probe);
    game_load_order(&list, by_id, game).iter().any(|x| x == id)
}

pub fn analyze(entries: &[ProfileEntry], visible: &[ScannedMod], ctx: &Context) -> Vec<Finding> {
    let mut out = Vec::new();
    let active: Vec<&ProfileEntry> = entries.iter().filter(|e| e.enabled).collect();
    let enabled_ids: BTreeSet<String> = active.iter().map(|e| normalize_id(&e.id)).collect();
    let mut by_id: BTreeMap<String, Vec<&ScannedMod>> = BTreeMap::new();
    for m in visible {
        if let Some(id) = m.id() {
            by_id.entry(normalize_id(id)).or_default().push(m);
        }
    }
    let game_txt = ctx.game.to_string();
    let game_order = game_load_order(&active, &by_id, ctx.game);
    let position: BTreeMap<&str, usize> = game_order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    for e in &active {
        let Some(m) = e.found else {
            out.push(Finding {
                severity: Severity::Blocks,
                mods: vec![e.id.clone()],
                kind: Kind::ModNotFound,
                fixes: vec![Fix::Disable(e.id.clone())],
            });
            continue;
        };
        match &m.layout {
            Layout::OnlyB41 => out.push(Finding {
                severity: Severity::Blocks,
                mods: vec![e.id.clone()],
                kind: Kind::OnlyB41,
                fixes: vec![Fix::Disable(e.id.clone())],
            }),
            Layout::B42 {
                folder_version: None,
                common,
                ..
            } if m.path_info.is_none() || !common => out.push(Finding {
                severity: Severity::Blocks,
                mods: vec![e.id.clone()],
                kind: Kind::NoValidFolder {
                    game: game_txt.clone(),
                },
                fixes: vec![Fix::Disable(e.id.clone())],
            }),
            _ => {}
        }
        let Some(info) = &m.info else { continue };
        let (min, max) = (info.version_min(), info.version_max());
        if min.is_some_and(|v| v > ctx.game) || max.is_some_and(|v| v < ctx.game) {
            out.push(Finding {
                severity: Severity::Blocks,
                mods: vec![e.id.clone()],
                kind: Kind::GameVersion {
                    min: info.version_min.clone(),
                    max: info.version_max.clone(),
                    game: game_txt.clone(),
                },
                fixes: vec![Fix::Disable(e.id.clone())],
            });
        }
        for r in &info.requires {
            let r = normalize_id(r);
            if enabled_ids.contains(&r) {
                continue;
            }
            let installed = by_id.contains_key(&r);
            let load = installed && loads_alone(&r, &active, &by_id, ctx.game);
            let mut fixes = Vec::new();
            if installed {
                fixes.push(Fix::Enable(r.clone()));
            } else if let crate::mods::Source::Workshop { id } = &m.source {
                fixes.push(Fix::OpenWorkshop(*id));
            }
            if !installed {
                fixes.push(Fix::Disable(e.id.clone()));
            }
            out.push(Finding {
                severity: if load {
                    Severity::Warning
                } else {
                    Severity::Blocks
                },
                mods: vec![e.id.clone()],
                kind: Kind::MissingDependency {
                    requires: r,
                    inactive_installed: installed,
                },
                fixes,
            });
        }
        let typos = info.require_typos();
        if typos.any() {
            out.push(Finding {
                severity: if typos.blocks() {
                    Severity::Blocks
                } else {
                    Severity::Warning
                },
                mods: vec![e.id.clone()],
                kind: Kind::RequireTypo {
                    spaced: typos.spaced,
                    semicolon: typos.semicolon,
                    ignored: typos.ignored,
                },
                fixes: vec![],
            });
        }
    }

    let lookup = |id: &str| {
        active
            .iter()
            .find(|e| normalize_id(&e.id) == id)
            .and_then(|e| e.found)
            .or_else(|| {
                by_id
                    .get(id)
                    .and_then(|v| used_copy(v).or(v.first().copied()))
            })
    };
    let mut wont_load = Vec::new();
    for e in &active {
        let id = normalize_id(&e.id);
        let blocked = out.iter().any(|f| {
            f.severity == Severity::Blocks && f.mods.len() == 1 && normalize_id(&f.mods[0]) == id
        });
        if e.found.is_none() || blocked || position.contains_key(id.as_str()) {
            continue;
        }
        let mut because: Vec<String> = Vec::new();
        let mut current = id.clone();
        while let Some(m) = lookup(&current) {
            let Some(r) = m
                .info
                .iter()
                .flat_map(|i| i.game_require.iter().flatten())
                .find(|r| !r.is_empty() && !position.contains_key(r.as_str()))
            else {
                break;
            };
            if because.contains(r) || because.len() >= 8 {
                break;
            }
            because.push(r.clone());
            current = r.clone();
        }
        if !because.is_empty() {
            wont_load.push(Finding {
                severity: Severity::Blocks,
                mods: vec![e.id.clone()],
                kind: Kind::WontLoad { because },
                fixes: vec![Fix::Disable(e.id.clone())],
            });
        }
    }
    out.extend(wont_load);

    let mut declared: BTreeSet<(String, String)> = BTreeSet::new();
    for e in &active {
        let Some(info) = e.found.and_then(|m| m.info.as_ref()) else {
            continue;
        };
        let me = normalize_id(&e.id);
        for x in &info.incompatible {
            let x = normalize_id(x);
            if enabled_ids.contains(&x) && x != me {
                declared.insert((me.clone(), x));
            }
        }
    }
    let mut groups: Vec<BTreeSet<String>> = Vec::new();
    for (a, b) in &declared {
        let with: Vec<usize> = (0..groups.len())
            .filter(|&i| groups[i].contains(a) || groups[i].contains(b))
            .collect();
        let mut g: BTreeSet<String> = [a.clone(), b.clone()].into();
        for &i in with.iter().rev() {
            g.extend(groups.remove(i));
        }
        groups.push(g);
    }
    groups.sort();
    for g in groups {
        let pairs: Vec<(String, String)> = declared
            .iter()
            .filter(|(a, _)| g.contains(a))
            .cloned()
            .collect();
        let fixes = g
            .iter()
            .map(|m| {
                let others: BTreeSet<String> = pairs
                    .iter()
                    .filter_map(|(a, b)| {
                        if a == m {
                            Some(b.clone())
                        } else if b == m {
                            Some(a.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                Fix::Keep(m.clone(), others.into_iter().collect())
            })
            .collect();
        out.push(Finding {
            severity: Severity::Blocks,
            mods: g.into_iter().collect(),
            kind: Kind::Incompatible { declared: pairs },
            fixes,
        });
    }

    for (id, ms) in &by_id {
        if ms.len() > 1 && enabled_ids.contains(id) {
            let used = used_copy(ms).map(|m| m.key.clone());
            let keys: Vec<String> = ms.iter().map(|m| m.key.clone()).collect();
            let fixes = match &used {
                Some(u) => keys
                    .iter()
                    .filter(|k| *k != u)
                    .map(|k| Fix::RemoveCopy(k.clone()))
                    .collect(),
                None => vec![],
            };
            out.push(Finding {
                severity: Severity::Warning,
                mods: vec![id.clone()],
                kind: Kind::DuplicatedId { keys, used },
                fixes,
            });
        }
    }

    let entry_order: Vec<Entry> = entries
        .iter()
        .map(|e| Entry {
            id: e.id.clone(),
            enabled: e.enabled,
            pinned: e.pinned,
            info: e.found.and_then(|m| m.info.as_ref()),
        })
        .collect();
    let res_order = order::sort(&entry_order);
    for c in &res_order.cycles {
        out.push(Finding {
            severity: if c.hard {
                Severity::Blocks
            } else {
                Severity::Warning
            },
            mods: c.mods.clone(),
            kind: Kind::Cycle {
                edges: c.edges.clone(),
                hard: c.hard,
            },
            fixes: if c.hard {
                c.mods.iter().cloned().map(Fix::Disable).collect()
            } else {
                vec![]
            },
        });
    }

    if !ctx.sections.is_empty() {
        out.extend(section_findings(
            entries,
            &order::edges(&entry_order),
            &ctx.sections,
        ));
    }

    let mut loaded: Vec<(&ProfileEntry, &ScannedMod)> = active
        .iter()
        .filter_map(|e| e.found.map(|m| (*e, m)))
        .collect();
    loaded.sort_by_key(|(e, _)| {
        position
            .get(normalize_id(&e.id).as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    let all_found = loaded.clone();
    loaded.retain(|(e, _)| position.contains_key(normalize_id(&e.id).as_str()));
    let loaded_ids: Vec<String> = loaded.iter().map(|(e, _)| normalize_id(&e.id)).collect();
    let to_index: Vec<&ScannedMod> = loaded
        .iter()
        .enumerate()
        .filter(|(i, _)| position.contains_key(loaded_ids[*i].as_str()))
        .map(|(_, (_, m))| *m)
        .collect();
    ScannedMod::index_all(&to_index);
    let mut owners: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (_, m)) in loaded.iter().enumerate() {
        if !position.contains_key(loaded_ids[i].as_str()) {
            continue;
        }
        for rel in m
            .relative_paths()
            .iter()
            .filter(|r| r.starts_with("media/") && !is_merged(r))
        {
            owners.entry(rel.as_str()).or_default().push(i);
        }
    }
    let requires_deep = |b: usize, a: &str| {
        let mut stack = vec![b];
        let mut seen = BTreeSet::new();
        while let Some(x) = stack.pop() {
            if !seen.insert(x) || seen.len() > 64 {
                continue;
            }
            for r in loaded[x].1.info.iter().flat_map(|i| &i.requires) {
                let r = normalize_id(r);
                if r == a {
                    return true;
                }
                if let Some(j) = loaded_ids.iter().position(|i| *i == r) {
                    stack.push(j);
                }
            }
        }
        false
    };
    let requires = |a: usize, b: usize| {
        let same_item = matches!(
            (&loaded[a].1.source, &loaded[b].1.source),
            (Source::Workshop { id: x }, Source::Workshop { id: y }) if x == y
        );
        let after = loaded[b].1.info.as_ref().is_some_and(|i| {
            i.load_after
                .iter()
                .any(|r| normalize_id(r) == loaded_ids[a])
        }) || loaded[a].1.info.as_ref().is_some_and(|i| {
            i.load_before
                .iter()
                .any(|r| normalize_id(r) == loaded_ids[b])
        });
        same_item || after || requires_deep(b, &loaded_ids[a])
    };

    let mut script_files: Vec<(&str, usize)> = owners
        .iter()
        .filter(|(rel, _)| rel.starts_with("media/scripts/") && rel.ends_with(".txt"))
        .filter_map(|(rel, v)| Some((*rel, *v.last()?)))
        .collect();
    script_files.sort_by(|a, b| {
        scripts::file_order(
            &a.0["media/scripts/".len()..],
            &b.0["media/scripts/".len()..],
        )
    });
    let with_scripts: BTreeSet<usize> = script_files.iter().map(|(_, i)| *i).collect();
    ScannedMod::scripts_all(
        &with_scripts
            .iter()
            .map(|i| loaded[*i].1)
            .collect::<Vec<_>>(),
    );
    type Defined<'a> = BTreeMap<(&'a str, &'a str, &'a str), Vec<(usize, u64, &'a str)>>;
    let mut defined: Defined = BTreeMap::new();
    for (rel, i) in &script_files {
        let objects = loaded[*i].1.script_objects();
        let Some((_, objects)) = objects.iter().find(|(r, _)| r == rel) else {
            continue;
        };
        for o in objects.iter().filter(|o| scripts::compared(&o.kind)) {
            let v = defined
                .entry((o.kind.as_str(), o.module.as_str(), o.name.as_str()))
                .or_default();
            match v.last_mut() {
                Some(last) if last.0 == *i => {
                    last.1 = o.body;
                    last.2 = rel;
                }
                _ => v.push((*i, o.body, rel)),
            }
        }
    }
    type Differ<'a> = Vec<((&'a str, &'a str, &'a str), &'a str, &'a str)>;
    let mut by_pair: BTreeMap<(usize, usize), (Differ, usize)> = BTreeMap::new();
    for (&(kind, module, name), v) in &defined {
        let Some(&(wins, body_wins, file_wins)) = v.last() else {
            continue;
        };
        let mut losers: BTreeMap<usize, (u64, &str)> = BTreeMap::new();
        for &(x, b, f) in v.iter().filter(|(x, ..)| *x != wins) {
            losers.insert(x, (b, f));
        }
        for (loses, (body, file)) in losers {
            let e = by_pair.entry((loses, wins)).or_default();
            if body == body_wins {
                e.1 += 1;
            } else {
                e.0.push(((kind, module, name), file, file_wins));
            }
        }
    }
    let differing = |loses: usize, wins: usize, objects: &Differ| -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        let props = |m: usize, rel: &str, key: (&str, &str, &str)| {
            loaded[m]
                .1
                .path_of(rel)
                .map(|p| scripts::properties_of(&p))
                .unwrap_or_default()
                .into_iter()
                .find(|(o, _)| (o.kind.as_str(), o.module.as_str(), o.name.as_str()) == key)
                .map(|(_, p)| p)
                .unwrap_or_default()
        };
        for &(key, file_l, file_w) in objects.iter() {
            let theirs = props(loses, file_l, key);
            for (k, v) in props(wins, file_w, key) {
                if theirs.iter().any(|(k2, v2)| *k2 == k && *v2 != v) {
                    out.insert(k);
                }
            }
        }
        out.into_iter().collect()
    };
    for ((loses, wins), (objects, identical)) in by_pair {
        let properties = differing(loses, wins, &objects);
        let objects: Vec<String> = objects
            .iter()
            .map(|((kind, module, name), ..)| format!("{kind} {module}.{name}"))
            .collect();
        let (l, w) = (loaded_ids[loses].clone(), loaded_ids[wins].clone());
        let related = requires(loses, wins) || requires(wins, loses);
        let accepted = ctx
            .accepted_overrides
            .iter()
            .any(|(a, b)| normalize_id(a) == l && normalize_id(b) == w);
        let matters = properties.is_empty() || properties.iter().any(|p| !scripts::cosmetic(p));
        let warns = !objects.is_empty() && matters && !related && !accepted;
        out.push(Finding {
            severity: if warns {
                Severity::Warning
            } else if objects.is_empty() {
                Severity::Detail
            } else {
                Severity::Info
            },
            mods: vec![l.clone(), w.clone()],
            kind: Kind::ScriptRedefined {
                wins: w.clone(),
                total: objects.len(),
                objects: objects.into_iter().take(SHOWS).collect(),
                identical,
                properties,
                related,
                accepted,
            },
            fixes: if warns {
                vec![Fix::Accept(vec![(l, w)])]
            } else {
                vec![]
            },
        });
    }
    let requires_of_far = |x: &str, a: &str| {
        let mut stack = vec![x.to_string()];
        let mut seen = BTreeSet::new();
        while let Some(y) = stack.pop() {
            if !seen.insert(y.clone()) {
                continue;
            }
            let m = loaded_ids
                .iter()
                .position(|i| *i == y)
                .map(|i| loaded[i].1)
                .or_else(|| {
                    by_id
                        .get(&y)
                        .and_then(|v| used_copy(v).or(v.first().copied()))
                });
            for r in m
                .iter()
                .flat_map(|m| m.info.iter())
                .flat_map(|i| &i.requires)
            {
                let r = normalize_id(r);
                if r == a {
                    return true;
                }
                stack.push(r);
            }
        }
        false
    };
    #[derive(Default)]
    struct Count {
        real: Vec<String>,
        identical: usize,
        not_load: Vec<String>,
        lost: Vec<String>,
        behavior: usize,
    }
    let mut by_pair: BTreeMap<(usize, usize), Count> = BTreeMap::new();
    let mut of_game: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut shares: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (rel, ds) in &owners {
        if let Some(&last) = ds.last()
            && ctx.game_files.is_some_and(|a| a.contains(*rel))
        {
            let same = ctx
                .game_folder
                .is_some_and(|g| same_as_game(loaded[last].1, rel, g));
            if !same {
                of_game.entry(last).or_default().push(rel.to_string());
            }
        }
        let Some((&wins, before)) = ds.split_last() else {
            continue;
        };
        for &x in ds {
            for &y in ds {
                if x != y {
                    shares.insert((x, y));
                }
            }
        }
        let reads = read(rel) == Load::Reads;
        let path_wins = if reads && !before.is_empty() {
            loaded[wins].1.path_of(rel)
        } else {
            None
        };
        for &p in before {
            let c = by_pair.entry((p, wins)).or_default();
            if !reads {
                c.not_load.push(rel.to_string());
            } else if same_file(loaded[p].1, loaded[wins].1, rel) {
                c.identical += 1;
            } else {
                c.real.push(rel.to_string());
                if !is_appearance(rel) {
                    c.behavior += 1;
                }
                if rel.starts_with("media/scripts/")
                    && rel.ends_with(".txt")
                    && let (Some(a), Some(b)) = (loaded[p].1.path_of(rel), path_wins.as_ref())
                {
                    let kept = scripts::objects_of(b);
                    c.lost.extend(
                        scripts::objects_of(&a)
                            .iter()
                            .filter(|o| !kept.iter().any(|k| k.same_name(o)))
                            .map(scripts::ScriptObject::label),
                    );
                }
            }
        }
    }
    fn root(g: &mut [usize], mut x: usize) -> usize {
        while g[x] != x {
            g[x] = g[g[x]];
            x = g[x];
        }
        x
    }
    let mut groups: BTreeMap<(bool, usize), Vec<(usize, usize)>> = BTreeMap::new();
    for real in [true, false] {
        let index_pairs: Vec<(usize, usize)> = by_pair
            .iter()
            .filter(|(_, c)| c.real.is_empty() != real)
            .map(|(&k, _)| k)
            .collect();
        let mut group: Vec<usize> = (0..loaded.len()).collect();
        for &(a, b) in &index_pairs {
            let (ra, rb) = (root(&mut group, a), root(&mut group, b));
            group[ra.max(rb)] = ra.min(rb);
        }
        for (a, b) in index_pairs {
            let r = root(&mut group, a);
            groups.entry((!real, r)).or_default().push((a, b));
        }
    }
    for (_, index_pairs) in groups {
        let pairs: Vec<Pair> = index_pairs
            .iter()
            .map(|&(a, b)| {
                let c = by_pair.remove(&(a, b)).unwrap_or_default();
                let (ia, ib) = (loaded_ids[a].clone(), loaded_ids[b].clone());
                Pair {
                    accepted: ctx
                        .accepted_overrides
                        .iter()
                        .any(|(p, g)| normalize_id(p) == ia && normalize_id(g) == ib),
                    related: requires(a, b) || requires(b, a),
                    total: c.real.len(),
                    files: c.real.into_iter().take(SHOWS).collect(),
                    identical: c.identical,
                    total_not_load: c.not_load.len(),
                    not_load: c.not_load.into_iter().take(SHOWS).collect(),
                    total_lost: c.lost.len(),
                    lost_objects: c.lost.into_iter().take(SHOWS).collect(),
                    loses: ia,
                    wins: ib,
                    behavior: c.behavior,
                }
            })
            .collect();
        let mut pairs = pairs;
        pairs.sort_by_key(|p| std::cmp::Reverse((p.total_lost, p.behavior)));
        let mut members: Vec<usize> = index_pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
        members.sort();
        members.dedup();
        let warns = pairs.iter().any(Pair::warns);
        let no_effect = pairs.iter().all(|p| p.total == 0);
        let mut fixes = Vec::new();
        let only_lost = pairs.iter().any(|p| p.total > 0)
            && pairs
                .iter()
                .filter(|p| p.total > 0)
                .all(|p| p.total_lost > 0);
        for &x in members.iter().filter(|_| !only_lost) {
            let idx = &loaded_ids[x];
            if !pairs.iter().any(|p| p.loses == *idx && p.total > 0) {
                continue;
            }
            if members
                .iter()
                .any(|&o| o != x && requires_of_far(&loaded_ids[o], idx))
            {
                continue;
            }
            let rivals: Vec<String> = members
                .iter()
                .filter(|&&o| shares.contains(&(x, o)))
                .map(|&o| loaded_ids[o].clone())
                .collect();
            fixes.push(Fix::Win(idx.clone(), rivals));
        }
        let to_accept: Vec<(String, String)> = pairs
            .iter()
            .filter(|p| p.warns())
            .map(|p| (p.loses.clone(), p.wins.clone()))
            .collect();
        if !to_accept.is_empty() {
            fixes.push(Fix::Accept(to_accept));
        }
        out.push(Finding {
            severity: if warns {
                Severity::Warning
            } else if no_effect {
                Severity::Detail
            } else {
                Severity::Info
            },
            mods: members.iter().map(|&i| loaded_ids[i].clone()).collect(),
            kind: Kind::OverriddenFiles { pairs },
            fixes: if no_effect { vec![] } else { fixes },
        });
    }
    for (i, mut files) in of_game {
        files.sort_by_key(|f| !changes_behavior(f));
        let behavior = files.iter().filter(|f| changes_behavior(f)).count();
        out.push(Finding {
            severity: if behavior > 0 {
                Severity::Info
            } else {
                Severity::Detail
            },
            mods: vec![loaded[i].0.id.clone()],
            kind: Kind::OverridesGame {
                total: files.len(),
                files: files.into_iter().take(SHOWS).collect(),
                behavior,
            },
            fixes: vec![],
        });
    }

    lua_globals(&loaded, &loaded_ids, ctx, &requires, &mut out);

    let mut tiledefs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (e, m) in &loaded {
        for t in m.info.iter().flat_map(|i| &i.tiledefs) {
            if let Some(n) = t.split_whitespace().last() {
                tiledefs
                    .entry(n.to_string())
                    .or_default()
                    .push(e.id.clone());
            }
        }
    }
    for (num, mut ms) in tiledefs {
        let mut seen = BTreeSet::new();
        ms.retain(|m| seen.insert(normalize_id(m)));
        if ms.len() < 2 {
            continue;
        }
        out.push(Finding {
            severity: Severity::Warning,
            mods: ms,
            kind: Kind::TiledefRepeated { num },
            fixes: vec![],
        });
    }

    let mut map_cells: Vec<(String, String, BTreeSet<String>)> = Vec::new();
    for (e, m) in &loaded {
        for name in m.maps() {
            let prefix = format!("media/maps/{}/", name.to_lowercase());
            let cells: BTreeSet<String> = m
                .relative_paths()
                .iter()
                .filter_map(|p| p.strip_prefix(&prefix)?.strip_suffix(".lotheader"))
                .filter(|c| !c.contains('/'))
                .map(str::to_string)
                .collect();
            if !cells.is_empty() {
                map_cells.push((name, e.id.clone(), cells));
            }
        }
    }
    let detected: Vec<String> = map_cells.iter().map(|(n, ..)| n.clone()).collect();
    let order = crate::profiles::order_maps(&ctx.maps, &detected);
    map_cells.sort_by_key(|(n, ..)| order.iter().position(|o| o == n).unwrap_or(usize::MAX));
    for (i, (a, ma, ca)) in map_cells.iter().enumerate() {
        for (b, mb, cb) in &map_cells[i + 1..] {
            let shared: Vec<&String> = ca.intersection(cb).collect();
            if shared.is_empty() {
                continue;
            }
            let mut mods = vec![ma.clone()];
            if mb != ma {
                mods.push(mb.clone());
            }
            out.push(Finding {
                severity: if mods.len() == 1 {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                mods,
                kind: Kind::MapCellsShared {
                    maps: (a.clone(), b.clone()),
                    cells: shared.len(),
                    sample: shared.iter().take(SHOWS).map(|c| c.to_string()).collect(),
                },
                fixes: vec![],
            });
        }
    }

    let java: Vec<(&ProfileEntry, &ScannedMod)> = all_found
        .iter()
        .copied()
        .filter(|(_, m)| {
            m.info.as_ref().is_some_and(|i| i.is_java()) && m.id() != Some("ZombieBuddy")
        })
        .collect();
    if !java.is_empty() {
        match &ctx.zb {
            None => {
                for (e, _) in &java {
                    out.push(Finding {
                        severity: Severity::Blocks,
                        mods: vec![e.id.clone()],
                        kind: Kind::ZbNotInstalled,
                        fixes: vec![Fix::UpdateZb],
                    });
                }
            }
            Some(v) => {
                if let Some(reason) = zb::incompatible_with_game(ctx.game, v) {
                    out.push(Finding {
                        severity: Severity::Blocks,
                        mods: vec!["ZombieBuddy".into()],
                        kind: Kind::ZbIncompatibleGame {
                            zb: v.clone(),
                            game: game_txt.clone(),
                            reason: reason.into(),
                        },
                        fixes: vec![Fix::UpdateZb],
                    });
                }
                let mut packages: BTreeMap<String, Vec<String>> = BTreeMap::new();
                let mut patched: Vec<(String, String, Vec<zb::JavaPatch>)> = Vec::new();
                for (e, m) in &java {
                    let info = m.info.as_ref().expect("es_java");
                    match zb::pick_jar(info, v) {
                        Some(JarChoice::Chosen(c)) => {
                            match zb::jar_path(m, &c.jar) {
                                None => out.push(Finding {
                                    severity: Severity::Blocks,
                                    mods: vec![e.id.clone()],
                                    kind: Kind::ZbJarMissing { jar: c.jar.clone() },
                                    fixes: vec![],
                                }),
                                Some(jar) => {
                                    if let Some(p) = &info.java.as_ref().expect("es_java").pkg
                                        && let Ok(list) = zb::patches(&jar, p)
                                    {
                                        patched.push((e.id.clone(), p.clone(), list));
                                    }
                                    if let Some(game) = ctx.game_folder
                                        && let Ok(missing) =
                                            zb::link_errors(&jar, &game.join("projectzomboid.jar"))
                                        && !missing.is_empty()
                                    {
                                        out.push(Finding {
                                            severity: Severity::Warning,
                                            mods: vec![e.id.clone()],
                                            kind: Kind::JavaLinkBroken {
                                                total: missing.len(),
                                                missing: missing.into_iter().take(SHOWS).collect(),
                                                game: game_txt.clone(),
                                            },
                                            fixes: vec![],
                                        });
                                    }
                                    if let Ok(state) =
                                        zb::approval_state(&jar, &e.id, &ctx.approvals, &ctx.trust)
                                    {
                                        let load = matches!(
                                            (&state, ctx.policy_zb.as_str()),
                                            (_, "allow-all")
                                                | (ApprovalState::Approved, _)
                                                | (ApprovalState::TrustedAuthor(_), "prompt")
                                        );
                                        let severity = match (&state, ctx.policy_zb.as_str()) {
                                            _ if load => None,
                                            (ApprovalState::Pending, "prompt") => {
                                                Some(Severity::Info)
                                            }
                                            _ => Some(Severity::Warning),
                                        };
                                        if let Some(severity) = severity {
                                            out.push(Finding {
                                                severity,
                                                mods: vec![e.id.clone()],
                                                kind: Kind::ZbApproval {
                                                    state,
                                                    policy: ctx.policy_zb.clone(),
                                                },
                                                fixes: vec![],
                                            });
                                        }
                                    }
                                }
                            }
                            if let Some(p) = &info.java.as_ref().expect("es_java").pkg {
                                packages.entry(p.clone()).or_default().push(e.id.clone());
                            }
                        }
                        Some(JarChoice::VersionIncompatible { zb, ranges }) => out.push(Finding {
                            severity: Severity::Blocks,
                            mods: vec![e.id.clone()],
                            kind: Kind::ZbVersion { zb, ranges },
                            fixes: vec![Fix::UpdateZb, Fix::Disable(e.id.clone())],
                        }),
                        Some(JarChoice::ServerOnly) | None => {}
                        Some(other) => out.push(Finding {
                            severity: Severity::Blocks,
                            mods: vec![e.id.clone()],
                            kind: Kind::ZbJarInvalid {
                                reason: format!("{other:?}"),
                            },
                            fixes: vec![],
                        }),
                    }
                }
                patched.retain(|(id, pkg, _)| packages.get(pkg).and_then(|v| v.last()) == Some(id));
                let patched: Vec<_> = patched.into_iter().map(|(id, _, p)| (id, p)).collect();
                out.extend(java_patch_overlaps(&patched));
                for (pkg, ms) in packages.into_iter().filter(|(_, v)| v.len() > 1) {
                    let load = ms.last().cloned().unwrap_or_default();
                    out.push(Finding {
                        severity: Severity::Warning,
                        mods: ms,
                        kind: Kind::ZbRepeatedPackage { pkg, load },
                        fixes: vec![],
                    });
                }
            }
        }
        if !enabled_ids.contains("ZombieBuddy") {
            out.push(Finding {
                severity: Severity::Info,
                mods: vec![],
                kind: Kind::ZbModNotEnabled,
                fixes: vec![Fix::Enable("ZombieBuddy".into())],
            });
        }
    }

    for m in &ctx.modifications {
        let detail = match m {
            Modification::Pzopt { files, revision } => format!(
                "pzopt ({files} archivos, revisión {})",
                revision.as_deref().unwrap_or("?")
            ),
            Modification::LooseClasses { folder, files } => {
                format!("{files} clases sueltas en {folder}\\ (tapan las del juego)")
            }
            _ => continue,
        };
        out.push(Finding {
            severity: Severity::Info,
            mods: vec![],
            kind: Kind::ModifiedInstall { detail },
            fixes: vec![],
        });
    }

    out.sort_by_key(|h| {
        let lost = matches!(&h.kind, Kind::OverriddenFiles { pairs }
            if pairs.iter().any(|p| p.total_lost > 0 && p.warns()));
        (h.severity, !lost)
    });
    out
}

fn section_findings(
    entries: &[ProfileEntry],
    edges: &[order::Edge],
    names: &[String],
) -> Vec<Finding> {
    let section: BTreeMap<String, usize> = entries
        .iter()
        .map(|e| (normalize_id(&e.id), e.section))
        .collect();
    let name = |k: usize| {
        k.checked_sub(1)
            .and_then(|i| names.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let slot: BTreeMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (normalize_id(&e.id), i))
        .collect();
    let mut out = Vec::new();
    let mut needed: BTreeMap<&str, (usize, Vec<&str>)> = BTreeMap::new();
    for a in edges {
        let (Some(&sb), Some(&sa)) = (section.get(&a.before), section.get(&a.after)) else {
            continue;
        };
        if sb <= sa {
            continue;
        }
        match a.rule {
            order::Rule::Requires | order::Rule::ZbFixed => {
                let n = needed.entry(&a.before).or_insert((sa, Vec::new()));
                n.0 = n.0.min(sa);
                if !n.1.contains(&a.after.as_str()) {
                    n.1.push(&a.after);
                }
            }
            order::Rule::LoadAfter | order::Rule::LoadBefore => out.push(Finding {
                severity: Severity::Detail,
                mods: vec![a.after.clone()],
                kind: Kind::SectionSoftRule {
                    after: a.before.clone(),
                    section: name(sb),
                },
                fixes: vec![],
            }),
            _ => {}
        }
    }
    for (needs, (sa, mut by)) in needed {
        by.sort_by_key(|m| slot.get(*m).copied().unwrap_or(usize::MAX));
        out.push(Finding {
            severity: Severity::Warning,
            mods: by.into_iter().map(str::to_string).collect(),
            kind: Kind::SectionOrder {
                needs: needs.to_string(),
                section: name(sa),
            },
            fixes: vec![Fix::MoveToSection(needs.to_string(), name(sa))],
        });
    }
    out
}

pub fn profile_entries<'a>(
    profile: &crate::profiles::Profile,
    visible: &'a [ScannedMod],
) -> Vec<ProfileEntry<'a>> {
    profile
        .mods
        .iter()
        .map(|e| ProfileEntry {
            id: e.id.clone(),
            enabled: e.enabled,
            pinned: e.pinned,
            found: visible
                .iter()
                .find(|m| !e.key.is_empty() && m.key == e.key)
                .or_else(|| visible.iter().find(|m| m.id() == Some(e.id.as_str()))),
            section: profile.section_index(e),
        })
        .collect()
}
