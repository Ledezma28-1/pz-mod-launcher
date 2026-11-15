use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::class_file;
use crate::modinfo::{JavaCandidate, ModInfo};
use crate::mods::{Layout, ScannedMod};

pub const WORKSHOP_STABLE: u64 = 3619862853;
pub const WORKSHOP_BETA: u64 = 3812624292;

pub const FIXED_ORDER: [&str; 3] = ["ZombieBuddy", "zdk", "ZModUnbork"];

fn component(p: Option<&str>) -> u64 {
    p.map(|s| {
        s.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
    })
    .and_then(|s| s.parse().ok())
    .unwrap_or(0)
}

pub fn compare(a: &str, b: &str) -> Ordering {
    let unknown = |v: &str| v.is_empty() || v == "unknown";
    match (unknown(a), unknown(b)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('.').collect(), b.split('.').collect());
    for i in 0..pa.len().max(pb.len()) {
        match component(pa.get(i).copied()).cmp(&component(pb.get(i).copied())) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

pub fn in_range(zb: &str, min: Option<&str>, max: Option<&str>) -> bool {
    if zb.is_empty() || zb == "unknown" {
        return true;
    }
    let empty = |s: &&str| !s.trim().is_empty();
    !(min
        .filter(empty)
        .is_some_and(|m| compare(zb, m) == Ordering::Less)
        || max
            .filter(empty)
            .is_some_and(|m| compare(zb, m) == Ordering::Greater))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum JarChoice {
    Chosen(JavaCandidate),
    NoPackage,
    ServerOnly,
    VersionIncompatible {
        zb: String,
        ranges: Vec<(Option<String>, Option<String>)>,
    },
    JarInvalid(String),
}

pub fn pick_jar(info: &ModInfo, zb_version: &str) -> Option<JarChoice> {
    let java = info.java.as_ref()?;
    if java.candidates.is_empty() {
        return None;
    }
    if java.pkg.as_deref().is_none_or(|p| p.trim().is_empty()) {
        return Some(JarChoice::NoPackage);
    }
    let mut any_client = false;
    for c in &java.candidates {
        if !c.jar.ends_with(".jar") {
            return Some(JarChoice::JarInvalid(c.jar.clone()));
        }
        if c.jar.contains("media/java/server/") {
            continue;
        }
        any_client = true;
        if in_range(zb_version, c.zb_min.as_deref(), c.zb_max.as_deref()) {
            return Some(JarChoice::Chosen(c.clone()));
        }
    }
    if !any_client {
        return Some(JarChoice::ServerOnly);
    }
    Some(JarChoice::VersionIncompatible {
        zb: zb_version.to_string(),
        ranges: java
            .candidates
            .iter()
            .map(|c| (c.zb_min.clone(), c.zb_max.clone()))
            .collect(),
    })
}

pub fn jar_path(m: &ScannedMod, jar: &str) -> Option<PathBuf> {
    let base = m.path_info.as_ref()?.parent()?.to_path_buf();
    let mut candidates = vec![base.join(jar)];
    if let Layout::B42 {
        folder_version: Some(v),
        ..
    } = &m.layout
    {
        candidates.push(m.folder.join(v).join(jar));
        candidates.push(m.folder.join("common").join(jar));
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn jar_version(jar: &Path) -> Option<String> {
    type Cache = std::sync::Mutex<
        std::collections::HashMap<PathBuf, (u64, std::time::SystemTime, Option<String>)>,
    >;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let md = std::fs::metadata(jar).ok()?;
    let signature = (md.len(), md.modified().ok()?);
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(g) = cache.lock()
        && let Some((t, f, v)) = g.get(jar)
        && (*t, *f) == signature
    {
        return v.clone();
    }
    let v = class_file::manifest_attribute(jar, "Implementation-Version")
        .ok()
        .flatten();
    if let Ok(mut g) = cache.lock() {
        g.insert(jar.to_path_buf(), (signature.0, signature.1, v.clone()));
    }
    v
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct JavaPatch {
    pub class_name: String,
    pub method_name: String,
    pub replaces: bool,
}

pub fn patches(jar: &Path, pkg: &str) -> crate::CoreResult<Vec<JavaPatch>> {
    type Cache = std::sync::Mutex<
        std::collections::HashMap<(PathBuf, String), (u64, std::time::SystemTime, Vec<JavaPatch>)>,
    >;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let md = std::fs::metadata(jar).map_err(crate::Error::io(jar))?;
    let signature = (md.len(), md.modified().map_err(crate::Error::io(jar))?);
    let key = (jar.to_path_buf(), pkg.to_string());
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(g) = cache.lock()
        && let Some((t, f, v)) = g.get(&key)
        && (*t, *f) == signature
    {
        return Ok(v.clone());
    }
    let prefix = format!("{}/", pkg.trim().replace('.', "/"));
    let mut out = Vec::new();
    for (_, bytes) in class_file::read_classes(jar, &prefix)? {
        let Ok(c) = class_file::Class::read(bytes) else {
            continue;
        };
        for a in c.annotations().unwrap_or_default() {
            if a.kind != "Lme/zed_0xff/zombie_buddy/Patch;"
                && a.kind != "Lme/zed_0xff/zombie_buddy/annotations/Patch;"
            {
                continue;
            }
            let Some(class_name) = a.str("className").filter(|s| !s.is_empty()) else {
                continue;
            };
            out.push(JavaPatch {
                class_name: class_name.to_string(),
                method_name: a.str("methodName").unwrap_or_default().to_string(),
                replaces: a.get("isAdvice") == Some(&class_file::AnnotationValue::Int(0)),
            });
        }
    }
    out.sort();
    out.dedup();
    if let Ok(mut g) = cache.lock() {
        g.insert(key, (signature.0, signature.1, out.clone()));
    }
    Ok(out)
}

pub fn link_errors(jar: &Path, game_jar: &Path) -> crate::CoreResult<Vec<String>> {
    type Cache = std::sync::Mutex<
        std::collections::HashMap<PathBuf, ((u64, std::time::SystemTime), Vec<String>)>,
    >;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let stamp = |p: &Path| -> crate::CoreResult<(u64, std::time::SystemTime)> {
        let md = std::fs::metadata(p).map_err(crate::Error::io(p))?;
        Ok((md.len(), md.modified().map_err(crate::Error::io(p))?))
    };
    let (mine, game) = (stamp(jar)?, stamp(game_jar)?);
    let signature = (mine.0 ^ game.0.rotate_left(17), mine.1.max(game.1));
    let key = jar.to_path_buf();
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(g) = cache.lock()
        && let Some((s, v)) = g.get(&key)
        && *s == signature
    {
        return Ok(v.clone());
    }
    let mut uses: std::collections::BTreeSet<(String, String, String)> = Default::default();
    let mut classes: std::collections::BTreeSet<String> = Default::default();
    for (_, bytes) in class_file::read_classes(jar, "")? {
        let Ok(c) = class_file::Class::read(bytes) else {
            continue;
        };
        uses.extend(
            c.references()
                .into_iter()
                .filter(|(owner, ..)| owner.starts_with("zombie/")),
        );
        classes.extend(
            c.class_refs()
                .into_iter()
                .filter(|x| x.starts_with("zombie/")),
        );
    }
    let mut game_classes = GameClasses::new(game_jar);
    let mut out: Vec<String> = Vec::new();
    for c in &classes {
        if game_classes.get(c).is_none() {
            out.push(format!("{} (clase)", c.replace('/', ".")));
        }
    }
    for (owner, name, kind) in &uses {
        if game_classes.get(owner).is_none() {
            continue;
        }
        if game_classes.has(owner, name, kind) == Some(false) {
            out.push(format!("{}.{name}: {kind}", owner.replace('/', ".")));
        }
    }
    if let Ok(mut g) = cache.lock() {
        g.insert(key, (signature, out.clone()));
    }
    Ok(out)
}

struct GameClasses;

type ClassCache = (
    Option<(PathBuf, u64)>,
    std::collections::HashMap<String, Option<class_file::Members>>,
    Option<zip::ZipArchive<std::fs::File>>,
);

fn class_cache() -> &'static std::sync::Mutex<ClassCache> {
    static C: std::sync::OnceLock<std::sync::Mutex<ClassCache>> = std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

const OBJECT_METHODS: [&str; 9] = [
    "toString",
    "equals",
    "hashCode",
    "getClass",
    "notify",
    "notifyAll",
    "wait",
    "clone",
    "finalize",
];

impl GameClasses {
    fn new(jar: &Path) -> Self {
        let size = std::fs::metadata(jar).map(|m| m.len()).unwrap_or(0);
        if let Ok(mut g) = class_cache().lock()
            && g.0.as_ref() != Some(&(jar.to_path_buf(), size))
        {
            g.0 = Some((jar.to_path_buf(), size));
            g.1 = Default::default();
            g.2 = std::fs::File::open(crate::paths::long(jar))
                .ok()
                .and_then(|f| zip::ZipArchive::new(f).ok());
        }
        GameClasses
    }

    fn get(&mut self, class: &str) -> Option<class_file::Members> {
        if let Ok(g) = class_cache().lock()
            && let Some(v) = g.1.get(class)
        {
            return v.clone();
        }
        let mut g = class_cache().lock().ok()?;
        let v = g.2.as_mut().and_then(|z| {
            use std::io::Read;
            let mut e = z.by_name(&format!("{class}.class")).ok()?;
            let mut bytes = Vec::with_capacity(e.size().min(1 << 24) as usize);
            e.by_ref().take(1 << 24).read_to_end(&mut bytes).ok()?;
            class_file::Class::read(bytes).ok()?.members().ok()
        });
        g.1.insert(class.to_string(), v.clone());
        v
    }

    fn has(&mut self, class: &str, name: &str, kind: &str) -> Option<bool> {
        let method = kind.starts_with('(');
        let mut stack = vec![class.to_string()];
        let mut seen = std::collections::BTreeSet::new();
        let mut unknown = false;
        while let Some(c) = stack.pop() {
            if !seen.insert(c.clone()) || seen.len() > 64 {
                continue;
            }
            if c == "java/lang/Object" {
                if method && OBJECT_METHODS.contains(&name) {
                    return Some(true);
                }
                continue;
            }
            if !c.starts_with("zombie/") {
                unknown = true;
                continue;
            }
            let Some(m) = self.get(&c) else {
                unknown = true;
                continue;
            };
            let list = if method { &m.methods } else { &m.fields };
            if list.iter().any(|(n, k)| n == name && k == kind) {
                return Some(true);
            }
            stack.extend(m.super_class.clone());
            stack.extend(m.interfaces.clone());
        }
        if unknown { None } else { Some(false) }
    }
}

pub fn mod_patches(m: &ScannedMod, zb_version: &str) -> Option<Vec<JavaPatch>> {
    let info = m.info.as_ref()?;
    let Some(JarChoice::Chosen(c)) = pick_jar(info, zb_version) else {
        return None;
    };
    let jar = jar_path(m, &c.jar)?;
    patches(&jar, info.java.as_ref()?.pkg.as_deref()?).ok()
}

#[derive(Debug, Clone, Serialize)]
pub struct ZbCopy {
    pub channel: &'static str,
    pub workshop_id: u64,
    pub version: Option<String>,
    pub libs: PathBuf,
    pub dll: bool,
}

pub fn workshop_copies(content: &Path) -> Vec<ZbCopy> {
    [("estable", WORKSHOP_STABLE), ("beta", WORKSHOP_BETA)]
        .into_iter()
        .filter_map(|(channel, id)| {
            let libs = content
                .join(id.to_string())
                .join("mods")
                .join("ZombieBuddy")
                .join("libs");
            let jar = libs.join("ZombieBuddy.jar");
            jar.is_file().then(|| ZbCopy {
                channel,
                workshop_id: id,
                version: jar_version(&jar),
                dll: libs.join("zbNative.dll").is_file(),
                libs,
            })
        })
        .collect()
}

pub fn incompatible_with_game(game: crate::version::Version, zb: &str) -> Option<&'static str> {
    if game >= crate::version::Version::new(42, 21) && compare(zb, "2.3.4") == Ordering::Less {
        return Some(
            "En 42.21 o posterior, ZombieBuddy anterior a 2.3.4 no carga los mods Java (issue #53).",
        );
    }
    None
}

pub const GAME_FILES: [&str; 4] = [
    "ZombieBuddy.jar",
    "zbNative.dll",
    "ZombieBuddy.jar.new",
    "ZombieBuddy.jar.bak",
];

#[derive(Debug, Clone, Serialize)]
pub struct ZbInstall {
    pub copy: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// `ZombieBuddy.jar.new`: `zbNative.dll` would apply it and overwrite the new JAR.
pub fn install(
    game: &Path,
    libs: &Path,
    backups: &Path,
    check_game: bool,
) -> crate::CoreResult<ZbInstall> {
    use crate::{Error, backups as cp, system};
    use std::fs;
    if check_game && !system::game_processes().is_empty() {
        return Err(Error::Backup(
            "el juego está abierto: ciérralo antes de instalar ZombieBuddy".into(),
        ));
    }
    for f in ["ZombieBuddy.jar", "zbNative.dll"] {
        if !libs.join(f).is_file() {
            return Err(Error::NotFound(format!("{f} en {}", libs.display())));
        }
    }
    for f in ["ZombieBuddy.jar", "zbNative.dll"] {
        let tmp = game.join(format!("{f}.pzmm-tmp"));
        if tmp.is_file() {
            fs::remove_file(&tmp).map_err(Error::io(&tmp))?;
        }
    }
    let before = jar_version(&game.join("ZombieBuddy.jar"));
    let copy = backups.join(format!("{}-zombiebuddy", system::timestamp(system::now())));
    let rels: Vec<String> = GAME_FILES.iter().map(|s| s.to_string()).collect();
    cp::create(game, &rels, &copy, "instalar o actualizar ZombieBuddy")?;

    let step = || -> crate::CoreResult<()> {
        for f in ["ZombieBuddy.jar", "zbNative.dll"] {
            let (origin, target) = (libs.join(f), game.join(f));
            if target.is_file() && cp::sha256(&target)? == cp::sha256(&origin)? {
                continue;
            }
            let tmp = game.join(format!("{f}.pzmm-tmp"));
            let put = || -> crate::CoreResult<()> {
                fs::copy(&origin, &tmp).map_err(Error::io(&tmp))?;
                if cp::sha256(&tmp)? != cp::sha256(&origin)? {
                    return Err(Error::Backup(format!("la copia de {f} salió distinta")));
                }
                fs::rename(&tmp, &target).map_err(Error::io(&target))
            };
            if let Err(e) = put() {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
        }
        let new_jar = game.join("ZombieBuddy.jar.new");
        if new_jar.is_file() {
            fs::remove_file(&new_jar).map_err(Error::io(&new_jar))?;
        }
        Ok(())
    };
    if let Err(e) = step() {
        let _ = cp::record_left(&copy, game);
        let undone = cp::restore(&copy, game, None);
        return Err(Error::Backup(format!(
            "no se pudo instalar ZombieBuddy ({e}); {}",
            match undone {
                Ok(_) => "se dejó todo como estaba".to_string(),
                Err(e2) => format!("¡y no se pudo deshacer: {e2}! Copia en {}", copy.display()),
            }
        )));
    }
    cp::record_left(&copy, game)?;
    Ok(ZbInstall {
        copy,
        before,
        after: jar_version(&game.join("ZombieBuddy.jar")),
    })
}

/// Where `zbNative` is named outside our launch. While any reference remains,
/// `zbNative.dll` cannot be deleted: the game would not start that way.
#[derive(Debug, Clone, Default, Serialize)]
pub struct References {
    pub json: bool,
    pub bat: bool,
    /// Steam users with `-agentlib:zbNative` in their launch options.
    pub steam: Vec<String>,
}

impl References {
    pub fn any(&self) -> bool {
        self.json || self.bat || !self.steam.is_empty()
    }
}

fn is_agent_zb(a: &str) -> bool {
    a.trim()
        .to_ascii_lowercase()
        .starts_with("-agentlib:zbnative")
}

pub fn references(game: &Path, steam: Option<&Path>) -> References {
    let json = std::fs::read_to_string(game.join("ProjectZomboid64.json"))
        .map(|t| t.to_ascii_lowercase().contains("-agentlib:zbnative"))
        .unwrap_or(false);
    let bat = std::fs::read_to_string(game.join("ProjectZomboid64.bat"))
        .map(|t| {
            t.lines().any(|l| {
                let l = l.trim().to_ascii_lowercase();
                l.strip_prefix("set _java_options=")
                    .is_some_and(|v| v.split_whitespace().any(is_agent_zb))
            })
        })
        .unwrap_or(false);
    let steam = steam
        .map(|s| {
            crate::steam::launch_options(s, crate::PZ_APP_ID)
                .into_iter()
                .filter(|(_, o)| o.split_whitespace().any(is_agent_zb))
                .map(|(u, _)| u)
                .collect()
        })
        .unwrap_or_default();
    References { json, bat, steam }
}

/// Removes from the JSON only the `"-agentlib:zbNative…"` entries (the official installer rewrites it whole and
/// loses fields). `None` if there was none or the result is not valid JSON.
pub fn remove_from_json(text: &str) -> Option<String> {
    let mut t = text.to_string();
    let mut changed = false;
    loop {
        let low = t.to_ascii_lowercase();
        let Some(ini) = low.find("\"-agentlib:zbnative") else {
            break;
        };
        let end = ini + 1 + t[ini + 1..].find('"')? + 1;
        let tail = t[end..].trim_start();
        let (from, to) = if tail.starts_with(',') {
            let comma = end + t[end..].find(',')?;
            let sig = comma + 1 + (t[comma + 1..].len() - t[comma + 1..].trim_start().len());
            (ini, sig)
        } else {
            let before = t[..ini].trim_end();
            if before.ends_with(',') {
                (before.len() - 1, end)
            } else {
                (ini, end)
            }
        };
        t.replace_range(from..to, "");
        changed = true;
    }
    if !changed {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&t).ok()?;
    Some(t)
}

/// Removes `-agentlib:zbNative…` from the `.bat`'s `SET _JAVA_OPTIONS=` line (keeps the `\r\n`).
pub fn remove_from_bat(text: &str) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| {
            let (body, cr) = l.strip_suffix('\r').map_or((l, ""), |c| (c, "\r"));
            let indent = &body[..body.len() - body.trim_start().len()];
            match body.trim_start().strip_prefix("SET _JAVA_OPTIONS=") {
                Some(v) if v.split_whitespace().any(is_agent_zb) => {
                    changed = true;
                    let rest: Vec<&str> =
                        v.split_whitespace().filter(|a| !is_agent_zb(a)).collect();
                    format!("{indent}SET _JAVA_OPTIONS={}{cr}", rest.join(" "))
                }
                _ => l.to_string(),
            }
        })
        .collect();
    changed.then(|| lines.join("\n"))
}

/// Steam still names it in its options it does nothing: without `zbNative.dll` the game would not start.
pub fn uninstall(
    game: &Path,
    backups: &Path,
    steam: Option<&Path>,
    remove_references: bool,
    check_game: bool,
) -> crate::CoreResult<PathBuf> {
    use crate::{Error, backups as cp, profiles::write_atomic, system};
    use std::fs;
    if check_game && !system::game_processes().is_empty() {
        return Err(Error::Backup(
            "el juego está abierto: ciérralo antes".into(),
        ));
    }
    let refs = references(game, steam);
    if !refs.steam.is_empty() {
        return Err(Error::Backup(format!(
            "las opciones de lanzamiento de Steam (usuario {}) tienen -agentlib:zbNative: quítalo en Steam \
             (Propiedades del juego > Opciones de lanzamiento) antes de desinstalar",
            refs.steam.join(", ")
        )));
    }
    if (refs.json || refs.bat) && !remove_references {
        return Err(Error::Backup(
            "ProjectZomboid64.json o .bat cargan zbNative: hay que quitarlo de ahí también".into(),
        ));
    }
    let mut rels: Vec<String> = GAME_FILES.iter().map(|s| s.to_string()).collect();
    rels.push("ProjectZomboid64.json".into());
    rels.push("ProjectZomboid64.bat".into());
    let copy = backups.join(format!(
        "{}-desinstalar-zombiebuddy",
        system::timestamp(system::now())
    ));
    cp::create(game, &rels, &copy, "desinstalar ZombieBuddy")?;
    let step = || -> crate::CoreResult<()> {
        let json = game.join("ProjectZomboid64.json");
        if let Ok(t) = fs::read_to_string(&json)
            && let Some(n) = remove_from_json(&t)
        {
            write_atomic(&json, &n)?;
        }
        let bat = game.join("ProjectZomboid64.bat");
        if let Ok(t) = fs::read_to_string(&bat)
            && let Some(n) = remove_from_bat(&t)
        {
            write_atomic(&bat, &n)?;
        }
        // never delete zbNative.dll while the JSON or .bat still load it (the edit may have failed)
        let left = references(game, None);
        if left.json || left.bat {
            return Err(Error::Backup(
                "ProjectZomboid64.json o .bat siguen cargando zbNative y no se pudo quitar".into(),
            ));
        }
        for f in GAME_FILES {
            let p = game.join(f);
            if p.is_file() {
                fs::remove_file(&p).map_err(Error::io(&p))?;
            }
        }
        Ok(())
    };
    if let Err(e) = step() {
        let _ = cp::record_left(&copy, game);
        let undone = cp::restore(&copy, game, None);
        return Err(Error::Backup(format!(
            "no se pudo desinstalar ({e}); {}",
            match undone {
                Ok(_) => "se dejó todo como estaba".to_string(),
                Err(e2) => format!("¡y no se pudo deshacer: {e2}! Copia en {}", copy.display()),
            }
        )));
    }
    cp::record_left(&copy, game)?;
    Ok(copy)
}

/// Refuses to restore a backup that would delete `zbNative.dll` (taken before ZB was installed)
/// while the JSON, the `.bat` or Steam still load it, or a backup of another game folder.
pub fn check_restore(copy: &Path, game: &Path, steam: Option<&Path>) -> crate::CoreResult<()> {
    use crate::Error;
    let m = crate::backups::read(copy)?;
    let norm = |p: &Path| {
        let t = p.to_string_lossy().to_ascii_lowercase();
        let t = t.strip_prefix(r"\\?\").unwrap_or(&t);
        t.trim_end_matches(std::path::is_separator).to_string()
    };
    if norm(&m.root) != norm(game) {
        return Err(Error::Backup(
            "esa copia es de otra carpeta del juego".into(),
        ));
    }
    let deletes_dll = m
        .files
        .iter()
        .any(|e| e.sha256.is_none() && e.path.eq_ignore_ascii_case("zbNative.dll"))
        && game.join("zbNative.dll").is_file();
    if deletes_dll && references(game, steam).any() {
        return Err(Error::Backup(
            "restaurar borraría zbNative.dll y el juego todavía la carga (-agentlib:zbNative): desinstala ZombieBuddy en su lugar"
                .into(),
        ));
    }
    Ok(())
}

pub fn steam_workshop_url(id: u64) -> String {
    format!("steam://url/CommunityFilePage/{id}")
}

pub fn config_folder() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".zombie_buddy"))
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub id: String,
    pub jar_hash: String,
    pub decision: bool,
    pub author: Option<u64>,
}

pub fn read_approvals(config: &Path) -> Vec<Approval> {
    let Ok(t) = std::fs::read_to_string(config.join("mod_approvals.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else {
        return Vec::new();
    };
    v["mods"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some(Approval {
                        id: e["id"].as_str()?.to_string(),
                        jar_hash: e["jar_hash"].as_str()?.to_ascii_lowercase(),
                        decision: e["decision"].as_bool()?,
                        author: e["author_id"].as_u64(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn trusted_authors(config: &Path) -> Vec<u64> {
    std::fs::read_to_string(config.join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v["trusted_authors"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
        })
        .unwrap_or_default()
}

pub fn signer(jar: &Path) -> Option<u64> {
    let mut zbs = jar.as_os_str().to_owned();
    zbs.push(".zbs");
    let t = std::fs::read_to_string(PathBuf::from(zbs)).ok()?;
    t.lines()
        .find_map(|l| l.trim().strip_prefix("SteamID64:"))
        .filter(|s| s.len() == 17 && s.chars().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse().ok())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ApprovalState {
    Approved,
    Denied,
    TrustedAuthor(u64),
    Pending,
}

fn jar_hash(jar: &Path) -> crate::CoreResult<String> {
    type Cache =
        std::sync::Mutex<std::collections::HashMap<PathBuf, (u64, std::time::SystemTime, String)>>;
    static CACHE: std::sync::OnceLock<Cache> = std::sync::OnceLock::new();
    let signature = std::fs::metadata(jar)
        .ok()
        .and_then(|m| Some((m.len(), m.modified().ok()?)));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(sig) = signature
        && let Ok(g) = cache.lock()
        && let Some((t, f, h)) = g.get(jar)
        && (*t, *f) == sig
    {
        return Ok(h.clone());
    }
    let hash = crate::backups::sha256(jar)?;
    if let Some((t, f)) = signature
        && let Ok(mut g) = cache.lock()
    {
        g.insert(jar.to_path_buf(), (t, f, hash.clone()));
    }
    Ok(hash)
}

pub fn approval_state(
    jar: &Path,
    id: &str,
    approvals: &[Approval],
    trust: &[u64],
) -> crate::CoreResult<ApprovalState> {
    let hash = jar_hash(jar)?;
    if let Some(a) = approvals.iter().find(|a| a.id == id && a.jar_hash == hash) {
        return Ok(if a.decision {
            ApprovalState::Approved
        } else {
            ApprovalState::Denied
        });
    }
    Ok(match signer(jar) {
        Some(s) if trust.contains(&s) => ApprovalState::TrustedAuthor(s),
        _ => ApprovalState::Pending,
    })
}
