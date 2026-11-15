use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    #[serde(rename = "carpetas")]
    pub folders: BTreeMap<PathBuf, BTreeMap<String, (u64, u128)>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Diff {
    pub added: Vec<(PathBuf, String)>,
    pub deleted: Vec<(PathBuf, String)>,
    pub changed: Vec<(PathBuf, String)>,
}

impl Diff {
    pub fn total(&self) -> usize {
        self.added.len() + self.deleted.len() + self.changed.len()
    }
}

pub fn take(folders: &[PathBuf]) -> Snapshot {
    let mut install = Snapshot::default();
    for c in folders {
        let long = paths::long(c);
        let mut map = BTreeMap::new();
        for e in walkdir::WalkDir::new(&long).into_iter().flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            let (Some(rel), Ok(md)) = (paths::relative(&long, e.path()), e.metadata()) else {
                continue;
            };
            let date = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            map.insert(rel, (md.len(), date));
        }
        install.folders.insert(c.clone(), map);
    }
    install
}

pub fn take_with_limits(folders: &[PathBuf], limits: &[(&str, usize)]) -> Snapshot {
    let mut install = Snapshot::default();
    for c in folders {
        let long = paths::long(c);
        let limit = |e: &walkdir::DirEntry| -> Option<usize> {
            let rel = e.path().strip_prefix(&long).ok()?;
            let first = rel.components().next()?.as_os_str().to_str()?;
            limits
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(first))
                .map(|(_, p)| *p)
        };
        let mut map = BTreeMap::new();
        let walk = walkdir::WalkDir::new(&long)
            .into_iter()
            .filter_entry(|e| limit(e).is_none_or(|p| e.depth() <= p));
        for e in walk.flatten() {
            let is_dir_limited = e.file_type().is_dir() && limit(&e).is_some();
            if !e.file_type().is_file() && !is_dir_limited {
                continue;
            }
            let (Some(rel), Ok(md)) = (paths::relative(&long, e.path()), e.metadata()) else {
                continue;
            };
            let date = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            if is_dir_limited {
                map.insert(format!("{rel}/"), (0, date));
            } else {
                map.insert(rel, (md.len(), date));
            }
        }
        install.folders.insert(c.clone(), map);
    }
    install
}

pub fn compare(before: &Snapshot, after: &Snapshot) -> Diff {
    let mut d = Diff::default();
    let empty = BTreeMap::new();
    for (c, a) in &before.folders {
        let b = after.folders.get(c).unwrap_or(&empty);
        for (k, v) in a {
            match b.get(k) {
                None => d.deleted.push((c.clone(), k.clone())),
                Some(w) if w != v => d.changed.push((c.clone(), k.clone())),
                _ => {}
            }
        }
        for k in b.keys().filter(|k| !a.contains_key(*k)) {
            d.added.push((c.clone(), k.clone()));
        }
    }
    d
}

pub fn save(install: &Snapshot, path: &Path) -> crate::CoreResult<()> {
    let json = serde_json::to_string(install).map_err(|source| crate::Error::Json {
        path: path.into(),
        source,
    })?;
    std::fs::write(path, json).map_err(crate::Error::io(path))
}

pub fn load(path: &Path) -> crate::CoreResult<Snapshot> {
    let t = std::fs::read_to_string(path).map_err(crate::Error::io(path))?;
    serde_json::from_str(&t).map_err(|source| crate::Error::Json {
        path: path.into(),
        source,
    })
}
