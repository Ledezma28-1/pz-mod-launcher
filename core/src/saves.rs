use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::mod_list::ModList;
use crate::modinfo::normalize_id;
use crate::profiles::write_atomic;
use crate::{CoreResult, Error};

const IGNORED: &str = "pzmm-saves-ignored.json";

#[derive(Debug, Clone, Serialize)]
pub struct SaveGap {
    pub dir: String,
    pub name: String,
    pub mode: String,
    pub modified: u64,
    pub missing: Vec<String>,
    pub with_map: Vec<String>,
}

fn save_dirs(cache: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(modes) = fs::read_dir(cache.join("Saves")) else {
        return out;
    };
    for mode in modes.flatten().filter(|m| m.path().is_dir()) {
        let mode_name = mode.file_name().to_string_lossy().to_string();
        if mode_name.eq_ignore_ascii_case("Multiplayer") {
            continue;
        }
        for s in fs::read_dir(mode.path()).into_iter().flatten().flatten() {
            if s.path().join("mods.txt").is_file() {
                out.push(format!("{mode_name}/{}", s.file_name().to_string_lossy()));
            }
        }
    }
    out.sort();
    out
}

fn folder(cache: &Path, dir: &str) -> CoreResult<PathBuf> {
    if !save_dirs(cache).iter().any(|d| d == dir) {
        return Err(Error::NotFound(format!("la partida {dir}")));
    }
    Ok(cache.join("Saves").join(dir))
}

fn read_ignored(cache: &Path) -> BTreeMap<String, Vec<String>> {
    fs::read_to_string(cache.join(IGNORED))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn gaps(cache: &Path, enabled: &[String]) -> Vec<SaveGap> {
    let ignored = read_ignored(cache);
    save_dirs(cache)
        .into_iter()
        .filter_map(|dir| {
            let path = cache.join("Saves").join(&dir);
            let list = ModList::read_file(&path.join("mods.txt")).ok()?;
            let have: Vec<String> = list.mods.iter().map(|m| normalize_id(m)).collect();
            let skip = ignored.get(&dir);
            let missing: Vec<String> = enabled
                .iter()
                .map(|m| normalize_id(m))
                .filter(|m| !have.contains(m) && !skip.is_some_and(|s| s.contains(m)))
                .collect();
            if missing.is_empty() {
                return None;
            }
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            let (mode, name) = dir.split_once('/').unwrap_or(("", &dir));
            Some(SaveGap {
                mode: mode.to_string(),
                name: name.to_string(),
                dir: dir.clone(),
                modified,
                missing,
                with_map: Vec::new(),
            })
        })
        .collect()
}

pub fn merge(save: &[String], order: &[String], add: &[String]) -> Vec<String> {
    let order: Vec<String> = order.iter().map(|m| normalize_id(m)).collect();
    let mut out: Vec<String> = save.to_vec();
    for a in order
        .iter()
        .filter(|o| add.iter().any(|x| normalize_id(x) == **o))
    {
        let at = order.iter().position(|o| o == a).unwrap_or(0);
        let after = order[..at]
            .iter()
            .rev()
            .find_map(|prev| out.iter().position(|m| normalize_id(m) == *prev));
        out.insert(after.map_or(0, |i| i + 1), format!("\\{a}"));
    }
    out
}

pub fn add(cache: &Path, dir: &str, enabled: &[String]) -> CoreResult<usize> {
    let path = folder(cache, dir)?.join("mods.txt");
    let text = fs::read_to_string(&path).map_err(Error::io(&path))?;
    let mut list = ModList::read(&text);
    let have: Vec<String> = list.mods.iter().map(|m| normalize_id(m)).collect();
    let ignored = read_ignored(cache).remove(dir).unwrap_or_default();
    let new: Vec<String> = enabled
        .iter()
        .map(|m| normalize_id(m))
        .filter(|m| !have.contains(m) && !ignored.contains(m))
        .collect();
    if new.is_empty() {
        return Ok(0);
    }
    let bak = path.with_extension("txt.bak");
    fs::write(&bak, &text).map_err(Error::io(&bak))?;
    list.mods = merge(&list.mods, enabled, &new);
    write_atomic(&path, &list.text())?;
    Ok(new.len())
}

pub fn restore(cache: &Path, dir: &str) -> CoreResult<()> {
    let path = folder(cache, dir)?.join("mods.txt");
    let bak = path.with_extension("txt.bak");
    let text = fs::read_to_string(&bak).map_err(Error::io(&bak))?;
    write_atomic(&path, &text)
}

pub fn ignore(cache: &Path, dir: &str, ids: &[String]) -> CoreResult<()> {
    folder(cache, dir)?;
    let mut all = read_ignored(cache);
    let e = all.entry(dir.to_string()).or_default();
    for id in ids.iter().map(|i| normalize_id(i)) {
        if !e.contains(&id) {
            e.push(id);
        }
    }
    let path = cache.join(IGNORED);
    let text = serde_json::to_string_pretty(&all).map_err(|source| Error::Json {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &text)
}
