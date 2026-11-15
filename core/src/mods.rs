use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::modinfo::ModInfo;
use crate::paths;
use crate::version::{self, Version};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind")]
pub enum Source {
    Workshop { id: u64 },
    Local,
}

#[derive(Debug, Clone, Serialize)]
pub enum Layout {
    B42 {
        folder_version: Option<String>,
        common: bool,
        versions: Vec<String>,
    },
    OnlyB41,
    Empty,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScannedMod {
    pub key: String,
    pub source: Source,
    pub folder: PathBuf,
    pub layout: Layout,
    pub path_info: Option<PathBuf>,
    pub info: Option<ModInfo>,
    pub error: Option<String>,
    #[serde(skip)]
    index: std::sync::OnceLock<FileIndex>,
    #[serde(skip)]
    maps: std::sync::OnceLock<Vec<String>>,
    #[serde(skip)]
    scripts: std::sync::OnceLock<Vec<(String, Vec<crate::scripts::ScriptObject>)>>,
    #[serde(skip)]
    script_refs: std::sync::OnceLock<std::collections::BTreeSet<String>>,
    #[serde(skip)]
    lua_defs: std::sync::OnceLock<std::collections::HashMap<String, Vec<crate::lua::Def>>>,
    #[serde(skip)]
    contents: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Content>>>,
}

#[derive(Debug, Clone, Default)]
struct Content {
    size: Option<Option<u64>>,
    hash: Option<Option<String>>,
}

#[derive(Debug, Clone, Default)]
struct FileIndex {
    paths: Vec<String>,
    roots: Vec<u8>,
    shadows: Vec<bool>,
}

impl ScannedMod {
    pub fn id(&self) -> Option<&str> {
        self.info.as_ref().and_then(|i| i.id.as_deref())
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        match &self.layout {
            Layout::B42 {
                folder_version,
                common,
                ..
            } => {
                let mut r = Vec::new();
                if *common {
                    r.push(self.folder.join("common"));
                }
                if let Some(v) = folder_version {
                    r.push(self.folder.join(v));
                }
                r
            }
            Layout::OnlyB41 => vec![self.folder.clone()],
            Layout::Empty => Vec::new(),
        }
    }

    pub fn files(&self) -> BTreeMap<String, PathBuf> {
        self.files_by_root()
            .into_iter()
            .map(|(rel, (path, ..))| (rel, path))
            .collect()
    }

    fn files_by_root(&self) -> BTreeMap<String, (PathBuf, usize, bool)> {
        let mut out = BTreeMap::new();
        for (n, root) in self.roots().into_iter().enumerate() {
            let long = paths::long(&root);
            for e in walkdir::WalkDir::new(&long).into_iter().flatten() {
                if !e.file_type().is_file() {
                    continue;
                }
                let Some(rel) = paths::relative(&long, e.path()) else {
                    continue;
                };
                if rel.eq_ignore_ascii_case("mod.info") {
                    continue;
                }
                let rel = rel.to_lowercase();
                let shadows = out.contains_key(&rel);
                out.insert(rel, (e.path().to_path_buf(), n, shadows));
            }
        }
        out
    }

    pub fn maps(&self) -> Vec<String> {
        self.maps.get_or_init(|| self.read_maps()).clone()
    }

    fn read_maps(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for root in self.roots() {
            let dir = paths::long(&root.join("media").join("maps"));
            let Ok(it) = fs::read_dir(&dir) else { continue };
            let mut names: Vec<String> = it
                .flatten()
                .filter(|e| e.path().join("map.info").is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            for n in names {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out
    }

    pub fn relative_paths(&self) -> &[String] {
        &self.index().paths
    }

    pub fn shadowed_paths(&self) -> impl Iterator<Item = &str> {
        let ix = self.index();
        ix.paths
            .iter()
            .zip(&ix.shadows)
            .filter(|(_, s)| **s)
            .map(|(p, _)| p.as_str())
    }

    fn index(&self) -> &FileIndex {
        self.index.get_or_init(|| {
            let mut ix = FileIndex::default();
            for (rel, (_, n, shadows)) in self.files_by_root() {
                ix.paths.push(rel);
                ix.roots.push(n as u8);
                ix.shadows.push(shadows);
            }
            ix
        })
    }

    pub fn size_of(&self, rel: &str) -> Option<u64> {
        if let Some(s) = self
            .contents
            .lock()
            .ok()
            .and_then(|g| g.get(rel).and_then(|c| c.size))
        {
            return s;
        }
        let s = self
            .path_of(rel)
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len());
        if let Ok(mut g) = self.contents.lock() {
            g.entry(rel.to_string()).or_default().size = Some(s);
        }
        s
    }

    pub fn hash_of(&self, rel: &str) -> Option<String> {
        if let Some(h) = self
            .contents
            .lock()
            .ok()
            .and_then(|g| g.get(rel).and_then(|c| c.hash.clone()))
        {
            return h;
        }
        let h = self
            .path_of(rel)
            .and_then(|p| crate::backups::sha256(&p).ok());
        if let Ok(mut g) = self.contents.lock() {
            g.entry(rel.to_string()).or_default().hash = Some(h.clone());
        }
        h
    }

    pub fn script_objects(&self) -> &[(String, Vec<crate::scripts::ScriptObject>)] {
        self.scripts.get_or_init(|| {
            self.relative_paths()
                .iter()
                .filter(|r| r.starts_with("media/scripts/") && r.ends_with(".txt"))
                .filter_map(|r| Some((r.clone(), crate::scripts::objects_of(&self.path_of(r)?))))
                .collect()
        })
    }

    pub fn script_refs(&self) -> &std::collections::BTreeSet<String> {
        self.script_refs.get_or_init(|| {
            self.relative_paths()
                .iter()
                .filter(|r| r.starts_with("media/scripts/") && r.ends_with(".txt"))
                .filter_map(|r| self.path_of(r))
                .flat_map(|p| crate::scripts::refs_of(&p))
                .collect()
        })
    }

    pub fn lua_definitions(&self) -> &std::collections::HashMap<String, Vec<crate::lua::Def>> {
        self.lua_defs.get_or_init(|| {
            self.relative_paths()
                .iter()
                .filter(|r| crate::lua::is_code(r))
                .filter_map(|r| Some((r.clone(), crate::lua::definitions_of(&self.path_of(r)?))))
                .collect()
        })
    }

    pub fn lua_all(mods: &[&ScannedMod]) {
        Self::lua_all_with(mods, &|_, _| {});
    }

    pub fn lua_all_with(mods: &[&ScannedMod], progress: &(dyn Fn(usize, usize) + Sync)) {
        Self::each_in_threads(
            mods,
            |m| {
                m.lua_definitions();
            },
            progress,
        );
    }

    pub fn script_refs_all(mods: &[&ScannedMod]) {
        Self::each_in_threads(
            mods,
            |m| {
                m.script_refs();
            },
            &|_, _| {},
        );
    }

    pub fn scripts_all(mods: &[&ScannedMod]) {
        Self::scripts_all_with(mods, &|_, _| {});
    }

    pub fn scripts_all_with(mods: &[&ScannedMod], progress: &(dyn Fn(usize, usize) + Sync)) {
        Self::each_in_threads(
            mods,
            |m| {
                m.script_objects();
            },
            progress,
        );
    }

    fn each_in_threads(
        mods: &[&ScannedMod],
        f: impl Fn(&ScannedMod) + Sync,
        progress: &(dyn Fn(usize, usize) + Sync),
    ) {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(m) = mods.get(i) else { break };
                        f(m);
                        let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        progress(d, mods.len());
                    }
                });
            }
        });
    }

    pub fn index_all(mods: &[&ScannedMod]) {
        Self::index_all_with(mods, &|_, _| {});
    }

    pub fn index_all_with(mods: &[&ScannedMod], progress: &(dyn Fn(usize, usize) + Sync)) {
        Self::each_in_threads(
            mods,
            |m| {
                m.relative_paths();
            },
            progress,
        );
    }

    pub fn path_of(&self, rel: &str) -> Option<PathBuf> {
        let ix = self.index();
        let i = ix.paths.binary_search_by(|p| p.as_str().cmp(rel)).ok()?;
        let root = self.roots().into_iter().nth(ix.roots[i] as usize)?;
        Some(paths::long(&root.join(rel)))
    }
}

fn subfolders(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(paths::long(dir))
        .map(|it| {
            it.flatten()
                .filter(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => true,
                    Ok(t) if t.is_file() => false,
                    _ => e.path().is_dir(),
                })
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn resolve(folder: &Path, source: Source, key: String, game: Version) -> ScannedMod {
    let subs = subfolders(folder);
    let versions: Vec<String> = subs
        .iter()
        .filter(|s| Version::parse(s).is_some())
        .cloned()
        .collect();
    let common = subs.iter().any(|s| s == "common");
    let (layout, path_info) = if common || !versions.is_empty() {
        let chosen =
            version::pick_folder(versions.iter().map(String::as_str), game).map(str::to_string);
        let info = chosen
            .as_ref()
            .map(|v| folder.join(v).join("mod.info"))
            .filter(|p| p.is_file())
            .or_else(|| Some(folder.join("common").join("mod.info")).filter(|p| p.is_file()));
        (
            Layout::B42 {
                folder_version: chosen,
                common,
                versions,
            },
            info,
        )
    } else if folder.join("mod.info").is_file() {
        (Layout::OnlyB41, Some(folder.join("mod.info")))
    } else {
        (Layout::Empty, None)
    };
    let (info, error) = match &path_info {
        Some(p) => match ModInfo::read_file(p) {
            Ok(i) if i.id.is_some() => (Some(i), None),
            Ok(i) => (Some(i), Some("mod.info sin id".to_string())),
            Err(e) => (None, Some(e.to_string())),
        },
        None => match &layout {
            Layout::B42 {
                folder_version: None,
                ..
            } => (
                None,
                Some(format!(
                    "ninguna carpeta de versión vale para {game} y common no tiene mod.info"
                )),
            ),
            _ => (None, Some("sin mod.info".to_string())),
        },
    };
    ScannedMod {
        key,
        source,
        folder: folder.to_path_buf(),
        layout,
        path_info,
        info,
        error,
        index: std::sync::OnceLock::new(),
        maps: std::sync::OnceLock::new(),
        scripts: std::sync::OnceLock::new(),
        script_refs: std::sync::OnceLock::new(),
        lua_defs: std::sync::OnceLock::new(),
        contents: Default::default(),
    }
}

pub fn scan_folder(dir: &Path, source: Source, prefix: &str, game: Version) -> Vec<ScannedMod> {
    subfolders(dir)
        .into_iter()
        .map(|n| resolve(&dir.join(&n), source.clone(), format!("{prefix}{n}"), game))
        .collect()
}

pub fn scan_profile_folder(cache: &Path, game: Version) -> Vec<ScannedMod> {
    let dir = cache.join("mods");
    subfolders(&dir)
        .into_iter()
        .filter(|n| n != crate::probe::ID && !crate::links::is_link(&dir.join(n)))
        .map(|n| resolve(&dir.join(&n), Source::Local, format!("cache:{n}"), game))
        .collect()
}

pub fn scan_workshop(content: &Path, game: Version) -> Vec<ScannedMod> {
    scan_workshop_reusing(content, game, &[], &|_| false)
}

pub fn scan_workshop_reusing(
    content: &Path,
    game: Version,
    old: &[ScannedMod],
    keep: &dyn Fn(u64) -> bool,
) -> Vec<ScannedMod> {
    let items: Vec<(u64, Vec<ScannedMod>)> = subfolders(content)
        .into_iter()
        .filter_map(|item| item.parse::<u64>().ok())
        .map(|id| {
            let before: Vec<ScannedMod> = if keep(id) {
                old.iter()
                    .filter(|m| m.source == Source::Workshop { id })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            (id, before)
        })
        .collect();
    let read: std::sync::Mutex<BTreeMap<u64, Vec<ScannedMod>>> = Default::default();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8);
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((id, before)) = items.get(i) else {
                        break;
                    };
                    if !before.is_empty() {
                        continue;
                    }
                    let mods = scan_item(content, *id, game);
                    if let Ok(mut g) = read.lock() {
                        g.insert(*id, mods);
                    }
                }
            });
        }
    });
    let mut read = read.into_inner().unwrap_or_default();
    let mut out = Vec::new();
    for (id, before) in items {
        if before.is_empty() {
            out.extend(read.remove(&id).unwrap_or_default());
        } else {
            out.extend(before);
        }
    }
    out
}

fn scan_item(content: &Path, id: u64, game: Version) -> Vec<ScannedMod> {
    scan_folder(
        &content.join(id.to_string()).join("mods"),
        Source::Workshop { id },
        &format!("ws:{id}/"),
        game,
    )
}
