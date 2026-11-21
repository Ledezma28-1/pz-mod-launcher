use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::class_file::{self, Class};
use crate::steam::{self, AppManifest};
use crate::version::Version;
use crate::{CoreResult, Error, PZ_APP_ID};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct GameVersion {
    pub version: Version,
    #[serde(alias = "sufijo")]
    pub suffix: String,
    pub revision: Option<String>,
}

impl GameVersion {
    pub fn text(&self) -> String {
        let suffix = if self.suffix.trim().is_empty() {
            ".0".to_string()
        } else {
            self.suffix.clone()
        };
        match &self.revision {
            Some(r) => format!("{}{} {r}", self.version, suffix),
            None => format!("{}{}", self.version, suffix),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Launcher {
    pub class_main: String,
    pub classpath: Vec<String>,
    pub vm_args: Vec<String>,
    pub per_windows: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, Serialize)]
pub enum Modification {
    ZombieBuddy {
        jar: bool,
        dll: bool,
        jar_new_pending: bool,
    },
    ArgJson {
        arg: String,
    },
    BatOptions {
        value: String,
    },
    Pzopt {
        files: usize,
        revision: Option<String>,
    },
    LooseClasses {
        folder: String,
        files: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Game {
    pub folder: PathBuf,
    pub library: Option<PathBuf>,
    pub manifest: Option<AppManifest>,
    pub version: Option<GameVersion>,
    pub java: Option<String>,
    pub launcher: Option<Launcher>,
    pub modifications: Vec<Modification>,
}

const GAME_FOLDERS: &[&str] = &[
    "jre64",
    "media",
    "launcher",
    "license",
    "mods",
    "workshop",
    "steamapps",
    "win64",
];

pub fn foreign_arg(a: &str) -> bool {
    let a = a.to_ascii_lowercase();
    a.starts_with("-agentlib:zbnative")
        || a.starts_with("-javaagent:")
        || a.starts_with("-dpzopt.")
        || a.starts_with("-xx:aotcache")
        || a.starts_with("-xlog:aot")
}

pub fn detect() -> CoreResult<Game> {
    let steam = steam::steam_folder().ok_or_else(|| Error::NotFound("Steam".into()))?;
    let libs = steam::libraries(&steam);
    let (lib, man, folder) = steam::find_app(&libs, PZ_APP_ID)
        .ok_or_else(|| Error::NotFound(format!("Project Zomboid en {} bibliotecas", libs.len())))?;
    let mut j = from_folder(&folder)?;
    j.library = Some(lib);
    j.manifest = Some(man);
    Ok(j)
}

pub fn from_folder(folder: &Path) -> CoreResult<Game> {
    let jar = folder.join("projectzomboid.jar");
    if !jar.is_file() {
        return Err(Error::NotFound(format!(
            "projectzomboid.jar en {}",
            folder.display()
        )));
    }
    Ok(Game {
        folder: folder.to_path_buf(),
        library: None,
        manifest: None,
        version: version_saved(&jar),
        java: read_java(folder),
        launcher: read_launcher(&folder.join("ProjectZomboid64.json")).ok(),
        modifications: modifications(folder),
    })
}

fn version_saved(jar: &Path) -> Option<GameVersion> {
    let md = fs::metadata(jar).ok()?;
    let signature = format!("{}|{:?}", md.len(), md.modified().ok()?);
    let copy = std::env::var_os("APPDATA").map(|a| {
        PathBuf::from(a)
            .join("PZModManager")
            .join("version-juego.json")
    });
    let saved = copy
        .as_ref()
        .and_then(|c| fs::read_to_string(c).ok())
        .and_then(|t| serde_json::from_str::<(String, GameVersion)>(&t).ok());
    if let Some((f, v)) = saved
        && f == signature
    {
        return Some(v);
    }
    let v = read_version(jar).ok().flatten()?;
    if let Some(c) = copy
        && let Some(d) = c.parent()
        && fs::create_dir_all(d).is_ok()
    {
        let _ = fs::write(
            &c,
            serde_json::to_string(&(signature, &v)).unwrap_or_default(),
        );
    }
    Some(v)
}

pub fn read_version(jar: &Path) -> CoreResult<Option<GameVersion>> {
    let mut classes =
        class_file::read_entries(jar, &["zombie/core/Core.class", "zombie/GitVersion.class"])?;
    let git = classes.pop().flatten();
    let Some(core) = classes.pop().flatten() else {
        return Ok(None);
    };
    let Some((major, minor, suffix)) = Class::read(core)?.game_version() else {
        return Ok(None);
    };
    let revision = git.and_then(|b| Class::read(b).ok()).and_then(|c| {
        c.strings_utf8()
            .find(|s| (7..=40).contains(&s.len()) && s.chars().all(|ch| ch.is_ascii_hexdigit()))
            .map(str::to_string)
    });
    Ok(Some(GameVersion {
        version: Version::new(major, minor),
        suffix,
        revision,
    }))
}

fn read_java(folder: &Path) -> Option<String> {
    let text = fs::read_to_string(folder.join("jre64").join("release")).ok()?;
    let field = |k: &str| {
        text.lines().find_map(|l| {
            l.strip_prefix(k)
                .and_then(|r| r.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_string())
        })
    };
    match (field("IMPLEMENTOR_VERSION"), field("JAVA_RUNTIME_VERSION")) {
        (Some(i), Some(v)) => Some(format!("{i} ({v})")),
        (a, b) => a.or(b),
    }
}

pub fn read_launcher(path: &Path) -> CoreResult<Launcher> {
    let text = fs::read_to_string(path).map_err(Error::io(path))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let list = |x: &serde_json::Value| -> Vec<String> {
        x.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut per_windows: Vec<(String, Vec<String>)> = v["windows"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, x)| (k.clone(), list(&x["vmArgs"])))
                .collect()
        })
        .unwrap_or_default();
    per_windows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Launcher {
        class_main: v["mainClass"]
            .as_str()
            .unwrap_or_default()
            .replace('/', "."),
        classpath: list(&v["classpath"]),
        vm_args: list(&v["vmArgs"]),
        per_windows,
    })
}

pub fn modifications(folder: &Path) -> Vec<Modification> {
    let mut out = Vec::new();
    let (jar, dll) = (
        folder.join("ZombieBuddy.jar").is_file(),
        folder.join("zbNative.dll").is_file(),
    );
    let new_jar_pending = folder.join("ZombieBuddy.jar.new").is_file();
    if jar || dll || new_jar_pending {
        out.push(Modification::ZombieBuddy {
            jar,
            dll,
            jar_new_pending: new_jar_pending,
        });
    }
    if let Ok(l) = read_launcher(&folder.join("ProjectZomboid64.json")) {
        let all = l
            .vm_args
            .iter()
            .chain(l.per_windows.iter().flat_map(|(_, a)| a));
        for a in all.filter(|a| foreign_arg(a)) {
            out.push(Modification::ArgJson { arg: a.clone() });
        }
    }
    if let Ok(bat) = fs::read_to_string(folder.join("ProjectZomboid64.bat")) {
        for l in bat.lines() {
            if let Some(v) = l.trim().strip_prefix("SET _JAVA_OPTIONS=")
                && !v.trim().is_empty()
            {
                out.push(Modification::BatOptions {
                    value: v.trim().to_string(),
                });
            }
        }
    }
    if let Ok(m) = fs::read_to_string(folder.join("pzopt-installed.txt")) {
        let revision = m.lines().find_map(|l| {
            l.strip_prefix("# revision=")
                .map(|r| r.split_whitespace().next().unwrap_or("").to_string())
        });
        let files = m
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .count();
        out.push(Modification::Pzopt { files, revision });
    }
    if let Ok(entries) = fs::read_dir(folder) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !e.path().is_dir() || GAME_FOLDERS.contains(&name.to_ascii_lowercase().as_str()) {
                continue;
            }
            let n = walkdir::WalkDir::new(e.path())
                .into_iter()
                .flatten()
                .filter(|f| {
                    f.path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("class"))
                })
                .count();
            if n > 0 {
                out.push(Modification::LooseClasses {
                    folder: name,
                    files: n,
                });
            }
        }
    }
    out
}

pub fn media_index(folder: &Path) -> std::collections::BTreeSet<String> {
    crate::conflicts::forget_game_files();
    crate::maps::forget_game();
    let media = folder.join("media");
    walkdir::WalkDir::new(&media)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| crate::paths::relative(folder, e.path()).map(|r| r.to_lowercase()))
        .collect()
}
