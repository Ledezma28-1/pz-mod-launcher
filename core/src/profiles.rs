use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mod_list::{self, ModList};
use crate::modinfo::normalize_id;
use crate::{CoreResult, Error, system};

pub const FILE: &str = "perfil.toml";
pub const CACHE: &str = "Zomboid";
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModEntry {
    #[serde(default)]
    #[serde(rename = "clave")]
    pub key: String,
    pub id: String,
    #[serde(default = "default_true")]
    #[serde(rename = "activo")]
    pub enabled: bool,
    #[serde(default)]
    #[serde(rename = "fijado")]
    pub pinned: bool,
    #[serde(default, rename = "seccion", skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchSettings {
    #[serde(default)]
    pub xmx: String,
    #[serde(default)]
    pub jvm_extra: Vec<String>,
    #[serde(default)]
    #[serde(rename = "juego_extra")]
    pub game_extra: Vec<String>,
    #[serde(default = "modfolders_default")]
    pub modfolders: Vec<String>,
    #[serde(default, rename = "sonda", skip_serializing_if = "std::ops::Not::not")]
    pub probe: bool,
}

fn modfolders_default() -> Vec<String> {
    ["workshop", "steam", "mods"].map(String::from).to_vec()
}

impl LaunchSettings {
    pub fn from_foreign(&self) -> (LaunchSettings, Vec<String>) {
        let mut ignored: Vec<String> = Vec::new();
        let xmx = self.xmx.trim();
        let xmx = if xmx.is_empty() || is_valid_xmx(xmx) {
            xmx.to_string()
        } else {
            ignored.push(format!("-Xmx{xmx}"));
            String::new()
        };
        ignored.extend(self.jvm_extra.iter().cloned());
        ignored.extend(self.game_extra.iter().cloned());
        let known = modfolders_default();
        let mut modfolders: Vec<String> = Vec::new();
        for m in &self.modfolders {
            if known.iter().any(|c| c.eq_ignore_ascii_case(m.trim())) {
                modfolders.push(m.trim().to_ascii_lowercase());
            } else {
                ignored.push(format!("-modfolders {m}"));
            }
        }
        if modfolders.is_empty() {
            modfolders = known;
        }
        (
            LaunchSettings {
                xmx,
                jvm_extra: Vec::new(),
                game_extra: Vec::new(),
                modfolders,
                probe: false,
            },
            ignored,
        )
    }
}

pub fn is_valid_id(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || ",;={}".contains(c))
}

pub fn is_valid_map(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || ";={}".contains(c))
}

pub fn is_valid_xmx(x: &str) -> bool {
    if !x.is_ascii() || x.len() < 2 {
        return false;
    }
    let (n, u) = x.split_at(x.len() - 1);
    !n.is_empty() && n.len() <= 6 && n.bytes().all(|b| b.is_ascii_digit()) && "mMgG".contains(u)
}

impl Default for LaunchSettings {
    fn default() -> Self {
        LaunchSettings {
            xmx: String::new(),
            jvm_extra: Vec::new(),
            game_extra: Vec::new(),
            modfolders: modfolders_default(),
            probe: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZbSettings {
    #[serde(default = "auto")]
    #[serde(rename = "usar")]
    pub usage: String,
    #[serde(default = "stable")]
    #[serde(rename = "canal")]
    pub channel: String,
    #[serde(default = "prompt")]
    #[serde(rename = "politica")]
    pub policy: String,
    #[serde(default)]
    #[serde(rename = "verbosidad")]
    pub verbosity: i32,
}

fn auto() -> String {
    "auto".into()
}
fn stable() -> String {
    "estable".into()
}
fn prompt() -> String {
    "prompt".into()
}

impl Default for ZbSettings {
    fn default() -> Self {
        ZbSettings {
            usage: auto(),
            channel: stable(),
            policy: prompt(),
            verbosity: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    #[serde(rename = "version_formato")]
    pub version_format: u32,
    pub id: String,
    #[serde(rename = "nombre")]
    pub name: String,
    #[serde(rename = "creado")]
    pub created: u64,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
    #[serde(default)]
    #[serde(rename = "mapas")]
    pub maps: Vec<String>,
    #[serde(default)]
    #[serde(rename = "arranque")]
    pub launch: LaunchSettings,
    #[serde(default)]
    pub zombiebuddy: ZbSettings,
    #[serde(default)]
    #[serde(rename = "ultimo_uso")]
    pub last_use: Option<u64>,
    #[serde(default)]
    #[serde(rename = "aceptados")]
    pub accepted_overrides: Vec<AcceptedOverride>,
    #[serde(default, rename = "secciones", skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[serde(
        default,
        rename = "mapas_fuera_del_servidor",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub server_excluded_maps: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GameListChange {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
    pub reordered: bool,
    pub maps: bool,
    pub emptied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcceptedOverride {
    #[serde(rename = "pierde")]
    pub loses: String,
    #[serde(rename = "gana")]
    pub wins: String,
}

impl Profile {
    pub fn set_enabled_many(&mut self, ids: &[String], enabled: bool) -> Vec<String> {
        let mut changed = Vec::new();
        for m in self.mods.iter_mut().filter(|m| ids.contains(&m.id)) {
            if m.enabled != enabled {
                m.enabled = enabled;
                changed.push(m.id.clone());
            }
        }
        changed
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool, key: Option<&str>) -> bool {
        let id = normalize_id(id);
        if let Some(m) = self.mods.iter_mut().find(|m| normalize_id(&m.id) == id) {
            m.enabled = enabled;
            return true;
        }
        match (enabled, key) {
            (true, Some(c)) => {
                self.mods.push(ModEntry {
                    key: c.to_string(),
                    id,
                    enabled: true,
                    pinned: false,
                    section: self.sections.last().cloned(),
                });
                true
            }
            _ => false,
        }
    }

    pub fn win_over(&mut self, id: &str, of: &str) -> bool {
        let (id, of) = (normalize_id(id), normalize_id(of));
        let Some(i) = self.mods.iter().position(|m| normalize_id(&m.id) == id) else {
            return false;
        };
        if !self.mods.iter().any(|m| normalize_id(&m.id) == of) {
            return false;
        }
        let mut m = self.mods.remove(i);
        let j = self
            .mods
            .iter()
            .position(|m| normalize_id(&m.id) == of)
            .expect("comprobado arriba");
        m.section = self.mods[j].section.clone();
        self.mods.insert(j + 1, m);
        self.accept(&of, &id);
        true
    }

    pub fn win_over_many(&mut self, id: &str, rivals: &[String]) -> bool {
        let rivals: Vec<String> = rivals.iter().map(|r| normalize_id(r)).collect();
        let Some(last) = self
            .mods
            .iter()
            .rev()
            .find(|m| rivals.contains(&normalize_id(&m.id)))
            .map(|m| m.id.clone())
        else {
            return false;
        };
        if !self.win_over(id, &last) {
            return false;
        }
        for r in &rivals {
            if self.mods.iter().any(|m| normalize_id(&m.id) == *r) {
                self.accept(r, id);
            }
        }
        true
    }

    pub fn remove_many(&mut self, ids: &[String]) -> Vec<(usize, ModEntry)> {
        let ids: Vec<String> = ids.iter().map(|i| normalize_id(i)).collect();
        let mut removed = Vec::new();
        let mut i = 0;
        let mut slot = 0;
        while i < self.mods.len() {
            if ids.contains(&normalize_id(&self.mods[i].id)) {
                removed.push((slot, self.mods.remove(i)));
            } else {
                i += 1;
            }
            slot += 1;
        }
        removed
    }

    fn drop_repeated(&mut self) {
        let mut out: Vec<ModEntry> = Vec::with_capacity(self.mods.len());
        for m in std::mem::take(&mut self.mods) {
            match out.iter_mut().find(|x| x.id == m.id) {
                Some(x) => x.enabled |= m.enabled,
                None => out.push(m),
            }
        }
        self.mods = out;
    }

    pub fn put_back(&mut self, removed: Vec<(usize, ModEntry)>) {
        for (slot, m) in removed {
            if self
                .mods
                .iter()
                .any(|x| normalize_id(&x.id) == normalize_id(&m.id))
            {
                continue;
            }
            let slot = slot.min(self.mods.len());
            self.mods.insert(slot, m);
        }
    }

    pub fn accept(&mut self, loses: &str, wins: &str) {
        let (loses, wins) = (normalize_id(loses), normalize_id(wins));
        self.accepted_overrides.retain(|a| {
            !((a.loses == loses && a.wins == wins) || (a.loses == wins && a.wins == loses))
        });
        self.accepted_overrides
            .push(AcceptedOverride { loses, wins });
    }

    pub fn new(name: &str) -> Profile {
        let created = system::now();
        Profile {
            version_format: FORMAT,
            id: identifier(name, created),
            name: name.trim().to_string(),
            created,
            mods: Vec::new(),
            maps: Vec::new(),
            launch: LaunchSettings::default(),
            zombiebuddy: ZbSettings::default(),
            last_use: None,
            accepted_overrides: Vec::new(),
            sections: Vec::new(),
            server_excluded_maps: Vec::new(),
        }
    }

    pub fn game_list_change(&self, game: &ModList) -> Option<GameListChange> {
        let ours: Vec<String> = self
            .mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| normalize_id(&m.id))
            .collect();
        let theirs: Vec<String> = game
            .mods
            .iter()
            .map(|m| normalize_id(m))
            .filter(|m| m != crate::probe::ID)
            .collect();
        let common_ours: Vec<&String> = ours.iter().filter(|m| theirs.contains(m)).collect();
        let common_theirs: Vec<&String> = theirs.iter().filter(|m| ours.contains(m)).collect();
        let c = GameListChange {
            enabled: theirs
                .iter()
                .filter(|m| !ours.contains(m))
                .cloned()
                .collect(),
            disabled: ours
                .iter()
                .filter(|m| !theirs.contains(m))
                .cloned()
                .collect(),
            reordered: common_ours != common_theirs,
            maps: game.maps != self.maps,
            emptied: theirs.is_empty() && !ours.is_empty(),
        };
        (c != GameListChange::default()).then_some(c)
    }

    pub fn take_game_list(&mut self, game: &ModList, key_of: impl Fn(&str) -> Option<String>) {
        let mut theirs: Vec<String> = Vec::new();
        for m in &game.mods {
            let id = normalize_id(m);
            if id != crate::probe::ID && !theirs.contains(&id) {
                theirs.push(id);
            }
        }
        let mut old: Vec<Option<ModEntry>> = std::mem::take(&mut self.mods)
            .into_iter()
            .map(Some)
            .collect();
        let mut next = theirs.iter();
        let last_section = self.sections.last().cloned();
        let take = |id: &str, old: &mut Vec<Option<ModEntry>>| -> ModEntry {
            let found = old
                .iter_mut()
                .find(|e| e.as_ref().is_some_and(|e| normalize_id(&e.id) == id))
                .and_then(Option::take);
            let mut e = found.unwrap_or_else(|| ModEntry {
                key: key_of(id).unwrap_or_default(),
                id: id.to_string(),
                enabled: true,
                pinned: false,
                section: last_section.clone(),
            });
            e.enabled = true;
            e
        };
        let mut seen = std::collections::BTreeSet::new();
        let slots: Vec<bool> = old
            .iter()
            .map(|e| {
                e.as_ref().is_some_and(|e| {
                    let id = normalize_id(&e.id);
                    theirs.contains(&id) && seen.insert(id)
                })
            })
            .collect();
        let mut out = Vec::new();
        for (i, slot) in slots.into_iter().enumerate() {
            if slot {
                match next.next() {
                    Some(id) => out.push(take(id, &mut old)),
                    None => {
                        if let Some(mut e) = old[i].take() {
                            e.enabled = false;
                            out.push(e);
                        }
                    }
                }
            } else {
                let mut e = old[i].take().expect("está");
                e.enabled = false;
                out.push(e);
            }
        }
        for id in next {
            out.push(take(id, &mut old));
        }
        self.mods = out;
        self.maps = game.maps.clone();
        self.regroup();
    }

    pub fn mod_list(&self) -> ModList {
        ModList {
            mods: self
                .mods
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.id.clone())
                .collect(),
            maps: self.maps.clone(),
        }
    }
}

fn identifier(name: &str, created: u64) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-");
    let mut n = created;
    let mut suffix = String::new();
    while n > 0 {
        suffix.insert(0, std::char::from_digit((n % 36) as u32, 36).unwrap_or('0'));
        n /= 36;
    }
    if base.is_empty() {
        format!("perfil-{suffix}")
    } else {
        format!("{base}-{suffix}")
    }
}

pub const BASE_MAP: &str = "Muldraugh, KY";

pub fn order_maps(saved: &[String], detected: &[String]) -> Vec<String> {
    let mut out: Vec<String> = saved
        .iter()
        .filter(|m| detected.contains(m))
        .cloned()
        .collect();
    for m in detected {
        if !out.contains(m) && m != BASE_MAP {
            out.push(m.clone());
        }
    }
    out.retain(|m| m != BASE_MAP);
    out.push(BASE_MAP.to_string());
    out
}

pub fn server_maps(order: &[String], excluded: &[String]) -> Vec<String> {
    let out: Vec<String> = order
        .iter()
        .filter(|m| !excluded.contains(m))
        .cloned()
        .collect();
    if out.is_empty() {
        vec![BASE_MAP.to_string()]
    } else {
        out
    }
}

pub struct ProfileStore {
    pub root: PathBuf,
}

pub fn entries_from(list: &ModList, key_of: impl Fn(&str) -> Option<String>) -> Vec<ModEntry> {
    let mut out: Vec<ModEntry> = Vec::new();
    for id in list.mods.iter().map(|m| normalize_id(m)) {
        if out.iter().any(|m| m.id == id) {
            continue;
        }
        out.push(ModEntry {
            key: key_of(&id).unwrap_or_default(),
            id,
            enabled: true,
            pinned: false,
            section: None,
        });
    }
    out
}

impl ProfileStore {
    pub fn at(root: &Path) -> ProfileStore {
        ProfileStore {
            root: std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    pub fn default_location() -> Option<ProfileStore> {
        let home = std::env::var_os("USERPROFILE")?;
        Some(ProfileStore {
            root: PathBuf::from(home).join("PZModManager").join("perfiles"),
        })
    }

    pub fn folder(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    pub fn cache(&self, id: &str) -> PathBuf {
        self.folder(id).join(CACHE)
    }

    pub fn list(&self) -> Vec<Profile> {
        let mut v: Vec<Profile> = fs::read_dir(&self.root)
            .map(|it| {
                it.flatten()
                    .filter_map(|e| self.load(&e.file_name().to_string_lossy()).ok())
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|p| p.created);
        v
    }

    pub fn unreadable(&self) -> Vec<(String, String)> {
        fs::read_dir(&self.root)
            .map(|it| {
                it.flatten()
                    .filter(|e| e.path().join(FILE).is_file())
                    .filter_map(|e| {
                        let id = e.file_name().to_string_lossy().into_owned();
                        self.load(&id).err().map(|err| (id, err.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn load(&self, id: &str) -> CoreResult<Profile> {
        let path = self.folder(id).join(FILE);
        let t = fs::read_to_string(&path).map_err(Error::io(&path))?;
        let mut p: Profile =
            toml::from_str(&t).map_err(|e| Error::Backup(format!("{}: {e}", path.display())))?;
        p.id = id.to_string();
        p.drop_repeated();
        Ok(p)
    }

    pub fn save(&self, p: &Profile) -> CoreResult<()> {
        let dir = self.folder(&p.id);
        fs::create_dir_all(dir.join(CACHE)).map_err(Error::io(&dir))?;
        let text = toml::to_string_pretty(p).map_err(|e| Error::Backup(e.to_string()))?;
        write_atomic(&dir.join(FILE), &text)
    }

    pub fn create(&self, name: &str) -> CoreResult<Profile> {
        let mut p = Profile::new(name);
        while self.folder(&p.id).exists() {
            p.id.push('x');
        }
        self.save(&p)?;
        Ok(p)
    }
}

pub fn write_atomic(path: &Path, text: &str) -> CoreResult<()> {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("pzmm-tmp-{}-{n}", std::process::id()));
    fs::write(&tmp, text).map_err(Error::io(&tmp))?;
    fs::rename(&tmp, path).map_err(Error::io(path))
}

pub const MIN_OPTIONS: &str = "version=8\n";

const LIST_SIGNATURE: &str = "pzmm-default.sha256";

fn list_file(cache: &Path) -> PathBuf {
    cache.join("mods").join("default.txt")
}

pub fn write_game_list(cache: &Path, text: &str, keep_prev: bool) -> CoreResult<()> {
    let file = list_file(cache);
    if keep_prev && file.is_file() && game_changed_list(cache) {
        let prev = file.with_extension("txt.prev");
        fs::copy(&file, &prev).map_err(Error::io(&prev))?;
    }
    write_atomic(&file, text)?;
    remember_game_list(cache)
}

pub fn remember_game_list(cache: &Path) -> CoreResult<()> {
    let hash = crate::backups::sha256(&list_file(cache))?;
    write_atomic(&cache.join(LIST_SIGNATURE), &hash)
}

pub fn game_changed_list(cache: &Path) -> bool {
    let Ok(saved) = fs::read_to_string(cache.join(LIST_SIGNATURE)) else {
        return true;
    };
    crate::backups::sha256(&list_file(cache)).map_or(true, |h| h != saved.trim())
}

pub fn prepare_cache(
    cache: &Path,
    profile: &Profile,
    origin_options: Option<&Path>,
) -> CoreResult<()> {
    let mods = cache.join("mods");
    fs::create_dir_all(&mods).map_err(Error::io(&mods))?;
    let mut list = profile.mod_list();
    if profile.launch.probe {
        list.mods.push(crate::probe::ID.to_string());
    }
    write_game_list(cache, &list.text(), true)?;
    let reset = mods.join(mod_list::RESET_FILE);
    if !reset.exists() {
        fs::write(&reset, mod_list::RESET_TEXT).map_err(Error::io(&reset))?;
    }
    let options = cache.join("options.ini");
    if !options.is_file() {
        match origin_options.filter(|o| o.is_file()) {
            Some(o) => {
                fs::copy(o, &options).map_err(Error::io(o))?;
            }
            None => write_atomic(&options, MIN_OPTIONS)?,
        }
    }
    Ok(())
}
