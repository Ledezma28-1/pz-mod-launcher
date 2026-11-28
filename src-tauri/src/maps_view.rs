use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use pzmm_core::conflicts::ProfileEntry;
use pzmm_core::maps::{self, Cell, CellInfo, MapEntry, Pyramid};
use pzmm_core::mods::{ScannedMod, Source};
use pzmm_core::profiles::{self, Profile};
use serde::Serialize;

#[derive(Serialize)]
pub struct PyramidDto {
    pub source: String,
    pub bounds: [i32; 4],
    pub levels: u8,
}

#[derive(Serialize)]
pub struct MapDto {
    pub key: String,
    pub folder: String,
    pub title: String,
    pub mod_key: String,
    pub mod_id: Option<String>,
    pub mod_name: String,
    pub workshop_id: Option<u64>,
    pub active: bool,
    pub order: Option<usize>,
    pub server: bool,
    pub cells: Vec<Cell>,
    pub pyramid: Option<PyramidDto>,
    pub thumb: Option<String>,
    pub spawns: Vec<(i32, i32)>,
    pub requires: Vec<String>,
    pub require_keys: Vec<String>,
    pub require_ids: Vec<String>,
    pub compatible: bool,
}

#[derive(Serialize)]
pub struct TownDto {
    pub name: String,
    pub spawns: Vec<(i32, i32)>,
}

#[derive(Serialize)]
pub struct MapsView {
    pub maps: Vec<MapDto>,
    pub cells: Vec<CellInfo>,
    pub towns: Vec<TownDto>,
    pub town_zones: Vec<maps::TownZone>,
    pub bounds: [i32; 4],
    pub world: [i32; 4],
    pub vanilla: Option<PyramidDto>,
    pub server_line: Vec<String>,
    pub base_excluded: bool,
    pub saved_order: bool,
}

pub fn source_key(prefix: &str, path: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    path.hash(&mut h);
    if let Ok(m) = fs::metadata(path) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    format!("{prefix}{:x}", h.finish())
}

pub fn missing_requirements<'a>(
    m: &ScannedMod,
    visible: &'a [ScannedMod],
    enabled_ids: &BTreeSet<String>,
) -> Vec<&'a ScannedMod> {
    use pzmm_core::modinfo::normalize_id;
    let mut out: Vec<&ScannedMod> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut todo: Vec<String> = m
        .info
        .as_ref()
        .map(|i| i.requires.clone())
        .unwrap_or_default();
    while let Some(r) = todo.pop() {
        let id = normalize_id(&r);
        if !seen.insert(id.clone()) || enabled_ids.contains(&id) {
            continue;
        }
        if let Some(found) = visible
            .iter()
            .find(|v| v.id().map(normalize_id).as_deref() == Some(id.as_str()))
        {
            todo.extend(
                found
                    .info
                    .as_ref()
                    .map(|i| i.requires.clone())
                    .unwrap_or_default(),
            );
            out.push(found);
        }
    }
    out
}

fn pyramid_dto(p: &Option<Pyramid>, source: String) -> Option<PyramidDto> {
    p.as_ref().map(|p| PyramidDto {
        source,
        bounds: p.bounds,
        levels: p.levels,
    })
}

pub fn build(
    game_folder: &std::path::Path,
    media: &BTreeSet<String>,
    visible: &[ScannedMod],
    p: &Profile,
    entries: &[ProfileEntry],
    language: &str,
    sources: &Mutex<HashMap<String, PathBuf>>,
) -> MapsView {
    let enabled: Vec<&ScannedMod> = entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| e.found)
        .collect();
    let enabled_ids: BTreeSet<&str> = enabled.iter().filter_map(|m| m.id()).collect();
    let mut seen_keys: BTreeSet<&str> = BTreeSet::new();
    let others: Vec<&ScannedMod> = visible
        .iter()
        .filter(|m| m.id().is_some_and(|i| !enabled_ids.contains(i)))
        .filter(|m| seen_keys.insert(m.key.as_str()))
        .collect();
    let active: Vec<(&ScannedMod, MapEntry)> = maps::dedupe_by_folder(
        enabled
            .iter()
            .flat_map(|m| maps::entries(m).into_iter().map(move |e| (*m, e)))
            .collect(),
        |(_, e)| &e.folder,
    );
    let enabled_norm: BTreeSet<String> = enabled
        .iter()
        .filter_map(|m| m.id())
        .map(pzmm_core::modinfo::normalize_id)
        .collect();
    let candidates: Vec<(&ScannedMod, MapEntry)> = others
        .iter()
        .flat_map(|m| maps::entries(m).into_iter().map(move |e| (*m, e)))
        .collect();

    let detected: Vec<String> = active.iter().map(|(_, e)| e.folder.clone()).collect();
    let order = profiles::order_maps(&p.maps, &detected);
    let server_line = profiles::server_maps(&order, &p.server_excluded_maps);
    let vanilla_cells = maps::vanilla_cells(media);
    let urban = maps::game_urban(game_folder).unwrap_or_default();
    let active_refs: Vec<&MapEntry> = active.iter().map(|(_, e)| e).collect();
    let cells = maps::cells_of(&order, &active_refs, &vanilla_cells, &urban);

    let mut reg: HashMap<String, PathBuf> = HashMap::new();
    let vanilla_zip = game_folder.join("media/maps/Muldraugh, KY/pyramid.zip");
    let vanilla = maps::read_pyramid(&vanilla_zip).ok().flatten();
    let vanilla_key = source_key("v", &vanilla_zip);
    if vanilla.is_some() {
        reg.insert(vanilla_key.clone(), vanilla_zip);
    }
    let mut dto = |m: &ScannedMod, e: &MapEntry, is_active: bool| {
        let pyramid = e
            .pyramid_zip
            .as_ref()
            .filter(|_| e.pyramid.is_some())
            .map(|z| {
                let k = source_key("m", z);
                reg.insert(k.clone(), z.clone());
                k
            });
        let thumb = e.thumb.as_ref().map(|t| {
            let k = source_key("t", t);
            reg.insert(k.clone(), t.clone());
            k
        });
        let needs = if is_active {
            Vec::new()
        } else {
            missing_requirements(m, visible, &enabled_norm)
        };
        MapDto {
            key: e.key.clone(),
            folder: e.folder.clone(),
            title: e.title.clone(),
            mod_key: e.mod_key.clone(),
            mod_id: e.mod_id.clone(),
            mod_name: m
                .info
                .as_ref()
                .and_then(|x| x.name.clone())
                .unwrap_or_else(|| e.mod_id.clone().unwrap_or_default()),
            workshop_id: match m.source {
                Source::Workshop { id } => Some(id),
                Source::Local => None,
            },
            active: is_active,
            order: is_active
                .then(|| order.iter().position(|o| *o == e.folder))
                .flatten(),
            server: is_active && server_line.contains(&e.folder),
            cells: e.cells.clone(),
            pyramid: pyramid.and_then(|s| pyramid_dto(&e.pyramid, s)),
            thumb,
            spawns: e.spawns.clone(),
            requires: m
                .info
                .as_ref()
                .map(|x| x.requires.clone())
                .unwrap_or_default(),
            require_keys: needs.iter().map(|r| r.key.clone()).collect(),
            require_ids: needs
                .iter()
                .filter_map(|r| r.id().map(str::to_string))
                .collect(),
            compatible: is_active || maps::compatible(e, &active_refs),
        }
    };
    let mut out: Vec<MapDto> = active
        .iter()
        .chain(candidates.iter())
        .enumerate()
        .map(|(i, (m, e))| dto(m, e, i < active.len()))
        .collect();
    out.sort_by_key(|m| (!m.active, m.order.unwrap_or(usize::MAX)));

    let mut world = [0, 0, 0, 0];
    for (n, &(x, y)) in vanilla_cells.iter().enumerate() {
        if n == 0 {
            world = [x, y, x, y];
        }
        world = [
            world[0].min(x),
            world[1].min(y),
            world[2].max(x),
            world[3].max(y),
        ];
    }
    let mut bounds = [0, 0, 0, 0];
    let all_cells = vanilla_cells
        .iter()
        .chain(out.iter().flat_map(|m| m.cells.iter()));
    for (n, &(x, y)) in all_cells.enumerate() {
        if n == 0 {
            bounds = [x, y, x, y];
        }
        bounds = [
            bounds[0].min(x),
            bounds[1].min(y),
            bounds[2].max(x),
            bounds[3].max(y),
        ];
    }
    sources
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(reg);
    MapsView {
        towns: maps::towns(game_folder, language)
            .into_iter()
            .map(|t| TownDto {
                name: t.name,
                spawns: t.spawns,
            })
            .collect(),
        town_zones: urban.zones.clone(),
        maps: out,
        cells,
        bounds,
        world,
        vanilla: pyramid_dto(&vanilla, vanilla_key),
        server_line,
        base_excluded: p
            .server_excluded_maps
            .iter()
            .any(|m| m == profiles::BASE_MAP),
        saved_order: p.maps.iter().any(|m| m != profiles::BASE_MAP),
    }
}

type Tile = (u8, u32, u32);

pub fn tile_request(path: &str) -> Option<(String, Option<Tile>)> {
    let parts: Vec<&str> = path.strip_prefix('/')?.split('/').collect();
    let source = parts.first()?;
    if source.is_empty() || !source.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    match parts[..] {
        [_] => Some((source.to_string(), None)),
        [_, level, file] => {
            let level: u8 = level.parse().ok().filter(|l| *l < 32)?;
            let (x, y) = file.strip_suffix(".png")?.split_once('x')?;
            let all_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
            if !all_digits(x) || !all_digits(y) {
                return None;
            }
            Some((
                source.to_string(),
                Some((level, x.parse().ok()?, y.parse().ok()?)),
            ))
        }
        _ => None,
    }
}

pub fn serve(sources: &Mutex<HashMap<String, PathBuf>>, path: &str) -> Option<Vec<u8>> {
    let (source, tile) = tile_request(path)?;
    let file = sources
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&source)?
        .clone();
    let Some((level, x, y)) = tile else {
        return source
            .starts_with('t')
            .then(|| fs::read(&file).ok())
            .flatten();
    };
    maps::read_tile(&file, level, x, y)
}
