use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use serde::Serialize;

use crate::lotpack::{self, Coverage};
use crate::mods::ScannedMod;
use crate::{CoreResult, Error};

pub type Cell = (i32, i32);

pub const CELL: i32 = 256;

fn tables(text: &str) -> impl Iterator<Item = &str> {
    text.split('{')
        .filter_map(|s| s.split_once('}').map(|(t, _)| t))
}

fn field(table: &str, name: &str) -> Option<i32> {
    let mut rest = table;
    while let Some(i) = rest.find(name) {
        let before = rest[..i].chars().next_back();
        let after = rest[i + name.len()..].trim_start();
        rest = &rest[i + name.len()..];
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(v) = after.strip_prefix('=') else {
            continue;
        };
        let v = v.trim_start();
        let end = v
            .char_indices()
            .find(|(k, c)| !(c.is_ascii_digit() || (*k == 0 && *c == '-')))
            .map_or(v.len(), |(k, _)| k);
        if let Ok(n) = v[..end].parse() {
            return Some(n);
        }
    }
    None
}

pub fn read_spawns(text: &str) -> Vec<(i32, i32)> {
    tables(text)
        .filter_map(|t| {
            let (px, py) = (field(t, "posX")?, field(t, "posY")?);
            match (field(t, "worldX"), field(t, "worldY")) {
                (Some(wx), Some(wy)) => Some((
                    wx.checked_mul(300)?.checked_add(px)?,
                    wy.checked_mul(300)?.checked_add(py)?,
                )),
                _ => Some((px, py)),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TownZone {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<i32>>,
}

impl TownZone {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        if x < self.x || y < self.y || x >= self.x + self.w || y >= self.y + self.h {
            return false;
        }
        let Some(p) = &self.points else {
            return true;
        };
        let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let corner = |i: usize| (f64::from(p[2 * i]), f64::from(p[2 * i + 1]));
        let n = p.len() / 2;
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let ((xi, yi), (xj, yj)) = (corner(i), corner(j));
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }
}

pub fn town_zones(objects_lua: &str) -> Vec<TownZone> {
    objects_lua
        .lines()
        .filter(|l| l.contains("type = \"TownZone\""))
        .filter_map(|l| {
            if let Some((_, rest)) = l.split_once("points = {") {
                let n: Vec<i32> = rest
                    .split('}')
                    .next()?
                    .split(',')
                    .filter_map(|v| v.trim().parse().ok())
                    .collect();
                if n.len() < 6 {
                    return None;
                }
                let xs = n.iter().step_by(2);
                let ys = n.iter().skip(1).step_by(2);
                let (x0, x1) = (*xs.clone().min()?, *xs.max()?);
                let (y0, y1) = (*ys.clone().min()?, *ys.max()?);
                return Some(TownZone {
                    x: x0,
                    y: y0,
                    w: x1 - x0 + 1,
                    h: y1 - y0 + 1,
                    points: Some(n[..n.len() / 2 * 2].to_vec()),
                });
            }
            Some(TownZone {
                x: field(l, "x")?,
                y: field(l, "y")?,
                w: field(l, "width")?,
                h: field(l, "height")?,
                points: None,
            })
        })
        .filter(|z| z.w > 0 && z.h > 0)
        .collect()
}

#[derive(Debug, Default)]
pub struct Urban {
    pub zones: Vec<TownZone>,
    by_cell: BTreeMap<Cell, Vec<usize>>,
}

impl Urban {
    pub fn new(zones: Vec<TownZone>) -> Urban {
        let mut by_cell: BTreeMap<Cell, Vec<usize>> = BTreeMap::new();
        for (i, z) in zones.iter().enumerate() {
            for cx in z.x.div_euclid(CELL)..=(z.x + z.w - 1).div_euclid(CELL) {
                for cy in z.y.div_euclid(CELL)..=(z.y + z.h - 1).div_euclid(CELL) {
                    by_cell.entry((cx, cy)).or_default().push(i);
                }
            }
        }
        Urban { zones, by_cell }
    }

    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.by_cell.keys()
    }

    pub fn touches(&self, cell: Cell) -> bool {
        self.by_cell.contains_key(&cell)
    }

    pub fn squares(&self, cell: Cell, cov: &Coverage) -> u32 {
        let Some(ids) = self.by_cell.get(&cell) else {
            return 0;
        };
        let (ox, oy) = (cell.0 * CELL, cell.1 * CELL);
        let mut n = 0;
        for ly in 0..256u16 {
            for lx in 0..256u16 {
                if cov.get(lx, ly)
                    && ids
                        .iter()
                        .any(|&i| self.zones[i].contains(ox + i32::from(lx), oy + i32::from(ly)))
                {
                    n += 1;
                }
            }
        }
        n
    }
}

type UrbanCache = Option<(PathBuf, Arc<Urban>)>;

static URBAN: LazyLock<Mutex<UrbanCache>> = LazyLock::new(|| Mutex::new(None));

pub fn game_urban(game_folder: &Path) -> Option<Arc<Urban>> {
    let mut g = URBAN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((f, u)) = g.as_ref()
        && f == game_folder
    {
        return Some(u.clone());
    }
    let text = fs::read_to_string(game_folder.join("media/maps/Muldraugh, KY/objects.lua")).ok()?;
    let u = Arc::new(Urban::new(town_zones(&text)));
    *g = Some((game_folder.to_path_buf(), u.clone()));
    Some(u)
}

pub fn forget_game() {
    *URBAN.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[derive(Debug, Clone, Serialize)]
pub struct Town {
    pub folder: String,
    pub name: String,
    pub spawns: Vec<(i32, i32)>,
}

fn town_title(game_folder: &Path, folder: &str, language: &str) -> Option<String> {
    let base = game_folder.join("media/lua/shared/Translate");
    [language.to_uppercase(), "EN".to_string()]
        .iter()
        .find_map(|l| {
            let text = fs::read_to_string(base.join(l).join(format!("{folder}.json"))).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            Some(v.get("title")?.as_str()?.trim().to_string())
        })
        .filter(|t| !t.is_empty())
}

pub fn towns(game_folder: &Path, language: &str) -> Vec<Town> {
    let maps = game_folder.join("media").join("maps");
    let Ok(it) = fs::read_dir(&maps) else {
        return Vec::new();
    };
    let mut out: Vec<Town> = it
        .flatten()
        .filter_map(|e| {
            let folder = e.file_name().to_string_lossy().to_string();
            let info = fs::read_to_string(e.path().join("map.info")).ok()?;
            if !info.lines().any(|l| l.trim() == "lots=Muldraugh, KY") {
                return None;
            }
            let spawns = fs::read_to_string(e.path().join("spawnpoints.lua"))
                .map(|t| read_spawns(&t))
                .unwrap_or_default();
            Some(Town {
                name: town_title(game_folder, &folder, language)
                    .unwrap_or_else(|| folder.trim_end_matches(", KY").to_string()),
                folder,
                spawns,
            })
        })
        .collect();
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pyramid {
    pub bounds: [i32; 4],
    pub levels: u8,
}

pub fn read_pyramid(zip_path: &Path) -> CoreResult<Option<Pyramid>> {
    let zip_err = |source| Error::Zip {
        path: zip_path.to_path_buf(),
        source,
    };
    let file = fs::File::open(zip_path).map_err(Error::io(zip_path))?;
    let mut z = zip::ZipArchive::new(file).map_err(zip_err)?;
    let mut text = String::new();
    match z.by_name("pyramid.txt") {
        Ok(mut f) => {
            f.read_to_string(&mut text).map_err(Error::io(zip_path))?;
        }
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(zip_err(e)),
    }
    let Some(bounds) = text.lines().find_map(|l| {
        let v: Vec<i32> = l
            .strip_prefix("bounds=")?
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        <[i32; 4]>::try_from(v).ok()
    }) else {
        return Ok(None);
    };
    let levels = (0..32u8)
        .take_while(|l| {
            let prefix = format!("{l}/");
            z.file_names()
                .any(|n| n.is_ok_and(|n| n.starts_with(&prefix)))
        })
        .count() as u8;
    Ok(Some(Pyramid { bounds, levels }))
}

type OpenZip = (PathBuf, zip::ZipArchive<fs::File>);

static ZIPS: LazyLock<Mutex<Vec<OpenZip>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn read_tile(zip_path: &Path, level: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    let mut zips = ZIPS.lock().unwrap_or_else(|e| e.into_inner());
    let at = match zips.iter().position(|(p, _)| p == zip_path) {
        Some(i) => i,
        None => {
            let z = zip::ZipArchive::new(fs::File::open(zip_path).ok()?).ok()?;
            if zips.len() >= 8 {
                zips.remove(0);
            }
            zips.push((zip_path.to_path_buf(), z));
            zips.len() - 1
        }
    };
    let mut entry = zips[at]
        .1
        .by_name(&format!("{level}/tile{x}x{y}.png"))
        .ok()?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out).ok()?;
    Some(out)
}

#[derive(Debug, Clone)]
pub struct MapEntry {
    pub key: String,
    pub folder: String,
    pub title: String,
    pub lots: Vec<String>,
    pub cells: Vec<Cell>,
    pub dir: PathBuf,
    pub cell_files: BTreeMap<Cell, (PathBuf, PathBuf)>,
    pub mod_key: String,
    pub mod_id: Option<String>,
    pub pyramid: Option<Pyramid>,
    pub pyramid_zip: Option<PathBuf>,
    pub thumb: Option<PathBuf>,
    pub spawns: Vec<(i32, i32)>,
}

fn cell_of(rel: &str, prefix: &str) -> Option<Cell> {
    let (x, y) = rel
        .strip_prefix(prefix)?
        .strip_suffix(".lotheader")?
        .split_once('_')?;
    if y.contains('/') {
        return None;
    }
    Some((x.parse().ok()?, y.parse().ok()?))
}

pub fn entries(m: &ScannedMod) -> Vec<MapEntry> {
    m.maps()
        .into_iter()
        .filter_map(|folder| {
            let prefix = format!("media/maps/{}/", folder.to_lowercase());
            let file = |name: &str| m.path_of(&format!("{prefix}{name}"));
            let info_path = file("map.info")?;
            let dir = info_path.parent()?.to_path_buf();
            let info = fs::read_to_string(&info_path).unwrap_or_default();
            let value = |k: &str| {
                info.lines()
                    .find_map(|l| l.trim().strip_prefix(k)?.strip_prefix('='))
                    .map(|v| v.trim().to_string())
            };
            let cells: Vec<Cell> = m
                .relative_paths()
                .iter()
                .filter_map(|r| cell_of(r, &prefix))
                .collect();
            let cell_files = cells
                .iter()
                .filter_map(|&(x, y)| {
                    let head = file(&format!("{x}_{y}.lotheader"))?;
                    let pack = file(&format!("world_{x}_{y}.lotpack"))?;
                    Some(((x, y), (head, pack)))
                })
                .collect();
            let pyramid_zip = file("pyramid.zip");
            Some(MapEntry {
                key: format!("{}|{folder}", m.key),
                title: value("title")
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| folder.clone()),
                lots: value("lots")
                    .map(|l| {
                        l.split(';')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect()
                    })
                    .unwrap_or_default(),
                cells,
                cell_files,
                pyramid: pyramid_zip
                    .as_deref()
                    .and_then(|z| read_pyramid(z).ok().flatten()),
                pyramid_zip,
                thumb: file("thumb.png"),
                spawns: file("spawnpoints.lua")
                    .and_then(|p| fs::read_to_string(p).ok())
                    .map(|t| read_spawns(&t))
                    .unwrap_or_default(),
                mod_key: m.key.clone(),
                mod_id: m.id().map(str::to_string),
                folder,
                dir,
            })
        })
        .collect()
}

pub fn vanilla_cells(media_index: &BTreeSet<String>) -> BTreeSet<Cell> {
    media_index
        .iter()
        .filter_map(|r| cell_of(r, "media/maps/muldraugh, ky/"))
        .collect()
}

type CoverageCache = HashMap<PathBuf, Option<Arc<Coverage>>>;

static COVERAGE: LazyLock<Mutex<CoverageCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn forget() {
    COVERAGE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn coverage_of(e: &MapEntry, cell: Cell) -> Option<Arc<Coverage>> {
    let (head, pack) = e.cell_files.get(&cell)?;
    if let Some(c) = COVERAGE.lock().unwrap_or_else(|e| e.into_inner()).get(pack) {
        return c.clone();
    }
    let c = lotpack::coverage(head, pack).ok().map(Arc::new);
    COVERAGE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(pack.clone(), c.clone());
    c
}

pub fn dedupe_by_folder<T>(items: Vec<T>, folder: impl Fn(&T) -> &str) -> Vec<T> {
    let mut last: HashMap<String, usize> = HashMap::new();
    for (i, x) in items.iter().enumerate() {
        last.insert(folder(x).to_lowercase(), i);
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(i, x)| last.get(&folder(x).to_lowercase()) == Some(i))
        .map(|(_, x)| x)
        .collect()
}

pub fn dedupe_folders(entries: Vec<MapEntry>) -> Vec<MapEntry> {
    dedupe_by_folder(entries, |m| &m.folder)
}

pub fn town_squares(e: &MapEntry, cell: Cell, urban: &Urban) -> Option<u32> {
    if !urban.touches(cell) {
        return Some(0);
    }
    Some(urban.squares(cell, &*coverage_of(e, cell)?))
}

pub fn overlap(a: &MapEntry, b: &MapEntry, cell: Cell) -> Option<u32> {
    Some(coverage_of(a, cell)?.overlap(&*coverage_of(b, cell)?))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state")]
pub enum CellState {
    Free,
    Shared { squares: Option<u32> },
    Touching,
    SameMod,
}

#[derive(Debug, Clone, Serialize)]
pub struct CellInfo {
    pub cell: Cell,
    pub maps: Vec<String>,
    pub vanilla: bool,
    pub urban: bool,
    #[serde(flatten)]
    pub state: CellState,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clash: Vec<[u16; 3]>,
}

const CLASH_RUNS: usize = 4096;

fn clash(maps: &[&MapEntry], cell: Cell) -> Option<Coverage> {
    let mut by_mod: BTreeMap<&str, Coverage> = BTreeMap::new();
    for m in maps {
        let c = coverage_of(m, cell)?;
        match by_mod.get_mut(m.mod_key.as_str()) {
            Some(g) => g.merge(&c),
            None => {
                by_mod.insert(&m.mod_key, (*c).clone());
            }
        }
    }
    let groups: Vec<Coverage> = by_mod.into_values().collect();
    Some(Coverage::clash(&groups))
}

pub fn cells_of(
    order: &[String],
    entries: &[&MapEntry],
    vanilla: &BTreeSet<Cell>,
    urban: &Urban,
) -> Vec<CellInfo> {
    let rank = |e: &MapEntry| {
        order
            .iter()
            .position(|o| *o == e.folder)
            .unwrap_or(usize::MAX)
    };
    let mut sorted: Vec<&MapEntry> = entries.to_vec();
    sorted.sort_by_key(|e| rank(e));
    let mut by_cell: BTreeMap<Cell, Vec<&MapEntry>> = BTreeMap::new();
    for e in &sorted {
        for c in &e.cells {
            by_cell.entry(*c).or_default().push(e);
        }
    }
    by_cell
        .into_iter()
        .map(|(cell, maps)| {
            let mods: BTreeSet<&str> = maps.iter().map(|m| m.mod_key.as_str()).collect();
            let mut runs = Vec::new();
            let state = if maps.len() == 1 {
                CellState::Free
            } else if mods.len() == 1 {
                let all: Option<u32> = maps.iter().skip(1).map(|b| overlap(maps[0], b, cell)).sum();
                if all == Some(0) {
                    CellState::Free
                } else {
                    CellState::SameMod
                }
            } else {
                match clash(&maps, cell) {
                    Some(c) if c.count() == 0 => CellState::Touching,
                    Some(c) => {
                        runs = c.runs(CLASH_RUNS);
                        CellState::Shared {
                            squares: Some(c.count()),
                        }
                    }
                    None => CellState::Shared { squares: None },
                }
            };
            CellInfo {
                cell,
                maps: maps.iter().map(|m| m.key.clone()).collect(),
                vanilla: vanilla.contains(&cell),
                urban: maps.iter().any(|m| town_squares(m, cell, urban) != Some(0)),
                state,
                clash: runs,
            }
        })
        .collect()
}

pub fn compatible(candidate: &MapEntry, active: &[&MapEntry]) -> bool {
    active
        .iter()
        .filter(|a| a.mod_key != candidate.mod_key)
        .all(|a| {
            a.cells
                .iter()
                .filter(|c| candidate.cells.contains(c))
                .all(|c| overlap(candidate, a, *c) == Some(0))
        })
}
