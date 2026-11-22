use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::version::Version;
use crate::{CoreResult, Error, paths};

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct JavaCandidate {
    pub index: u32,
    pub jar: String,
    pub zb_min: Option<String>,
    pub zb_max: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct JavaInfo {
    pub candidates: Vec<JavaCandidate>,
    pub pkg: Option<String>,
    pub preload: bool,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ModInfo {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub posters: Vec<String>,
    pub url: Option<String>,
    pub version_mod: Option<String>,
    pub packs: Vec<String>,
    pub tiledefs: Vec<String>,
    pub requires: Vec<String>,
    pub incompatible: Vec<String>,
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    pub version_min: Option<String>,
    pub version_max: Option<String>,
    pub java: Option<JavaInfo>,
    pub others: BTreeMap<String, String>,
    pub game_require: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequireTypos {
    pub spaced: Vec<String>,
    pub semicolon: Vec<String>,
    pub ignored: Vec<String>,
}

impl RequireTypos {
    pub fn any(&self) -> bool {
        !(self.spaced.is_empty() && self.semicolon.is_empty() && self.ignored.is_empty())
    }

    pub fn blocks(&self) -> bool {
        !(self.spaced.is_empty() && self.semicolon.is_empty())
    }
}

pub fn id_list(value: &str) -> Vec<String> {
    value
        .split([',', ';'])
        .map(|s| s.trim().trim_start_matches('\\').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn add_ids(to: &mut Vec<String>, value: &str) {
    for id in id_list(value) {
        if !to.contains(&id) {
            to.push(id);
        }
    }
}

pub fn normalize_id(id: &str) -> String {
    id.trim().trim_start_matches('\\').trim().to_string()
}

type PartialCandidate = (Option<String>, Option<String>, Option<String>);

fn suffix(key: &str, prefix: &str) -> Option<u32> {
    let rest = key.strip_prefix(prefix)?;
    if rest.is_empty() {
        Some(0)
    } else {
        rest.parse().ok()
    }
}

impl ModInfo {
    pub fn read_file(path: &Path) -> CoreResult<ModInfo> {
        let bytes = fs::read(paths::long(path)).map_err(Error::io(path))?;
        Ok(ModInfo::read(&String::from_utf8_lossy(&bytes)))
    }

    pub fn read(text: &str) -> ModInfo {
        let mut m = ModInfo::default();
        let mut cand: BTreeMap<u32, PartialCandidate> = BTreeMap::new();
        let mut java = JavaInfo::default();
        let mut has_java = false;

        for line in text.trim_start_matches('\u{feff}').lines() {
            if let Some((_, value)) = line.split_once("require=") {
                let mut items: Vec<String> = value
                    .trim_end()
                    .replace('\\', "")
                    .split(',')
                    .map(str::to_string)
                    .collect();
                while items.last().is_some_and(String::is_empty) {
                    items.pop();
                }
                m.game_require = Some(items);
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let v = v.trim();
            let first = |field: &mut Option<String>| {
                if field.is_none() && !v.is_empty() {
                    *field = Some(v.to_string());
                }
            };
            match key.as_str() {
                "id" => first(&mut m.id),
                "name" => first(&mut m.name),
                "description" => first(&mut m.description),
                "author" => first(&mut m.author),
                "category" => first(&mut m.category),
                "icon" => first(&mut m.icon),
                "url" => first(&mut m.url),
                "modversion" => first(&mut m.version_mod),
                "versionmin" => first(&mut m.version_min),
                "versionmax" => first(&mut m.version_max),
                "poster" if !v.is_empty() => m.posters.push(v.to_string()),
                "pack" if !v.is_empty() => m.packs.push(v.to_string()),
                "tiledef" if !v.is_empty() => m.tiledefs.push(v.to_string()),
                "require" => add_ids(&mut m.requires, v),
                "incompatible" => add_ids(&mut m.incompatible, v),
                "loadmodafter" => add_ids(&mut m.load_after, v),
                "loadmodbefore" => add_ids(&mut m.load_before, v),
                "javapkgname" => {
                    has_java = true;
                    if java.pkg.is_none() && !v.is_empty() {
                        java.pkg = Some(v.to_string());
                    }
                }
                "javapreload" => java.preload = v.eq_ignore_ascii_case("true"),
                c => {
                    if let Some(i) = suffix(c, "javajarfile") {
                        has_java = true;
                        let e = cand.entry(i).or_default();
                        if e.0.is_none() && !v.is_empty() {
                            e.0 = Some(v.to_string());
                        }
                    } else if let Some(i) = suffix(c, "zbversionmin") {
                        cand.entry(i).or_default().1 =
                            Some(v.to_string()).filter(|s| !s.is_empty());
                    } else if let Some(i) = suffix(c, "zbversionmax") {
                        cand.entry(i).or_default().2 =
                            Some(v.to_string()).filter(|s| !s.is_empty());
                    } else if !c.is_empty() {
                        m.others
                            .entry(c.to_string())
                            .or_insert_with(|| v.to_string());
                    }
                }
            }
        }
        if has_java {
            java.candidates = cand
                .into_iter()
                .filter_map(|(index, (jar, zb_min, zb_max))| {
                    Some(JavaCandidate {
                        index,
                        jar: jar?,
                        zb_min,
                        zb_max,
                    })
                })
                .collect();
            m.java = Some(java);
        }
        m
    }

    pub fn version_min(&self) -> Option<Version> {
        self.version_min.as_deref().and_then(Version::from_mod_info)
    }

    pub fn version_max(&self) -> Option<Version> {
        self.version_max.as_deref().and_then(Version::from_mod_info)
    }

    pub fn require_typos(&self) -> RequireTypos {
        let game = self.game_require.clone().unwrap_or_default();
        let mut t = RequireTypos::default();
        for g in &game {
            if g.contains(';') {
                t.semicolon.push(g.clone());
            } else if g.trim() != g && !g.trim().is_empty() {
                t.spaced.push(g.clone());
            }
        }
        let seen: Vec<String> = game
            .iter()
            .flat_map(|g| g.split(';').map(normalize_id).collect::<Vec<_>>())
            .collect();
        t.ignored = self
            .requires
            .iter()
            .filter(|r| !seen.contains(r))
            .cloned()
            .collect();
        t
    }

    pub fn is_java(&self) -> bool {
        self.java.as_ref().is_some_and(|j| !j.candidates.is_empty())
    }
}
