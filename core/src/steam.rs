use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::vdf::{self, Value};
use crate::{CoreResult, Error};

pub fn steam_folder() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        let candidates = [
            (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Valve\Steam",
                "InstallPath",
            ),
        ];
        for (root, key, value) in candidates {
            if let Some(p) = crate::system::log_text(root, key, value) {
                let p = PathBuf::from(p.replace('/', "\\"));
                if p.join("steamapps").is_dir() {
                    return Some(p);
                }
            }
        }
    }
    let default_value = PathBuf::from(r"C:\Program Files (x86)\Steam");
    default_value
        .join("steamapps")
        .is_dir()
        .then_some(default_value)
}

fn read_vdf(path: &Path) -> CoreResult<Value> {
    let text = fs::read_to_string(path).map_err(Error::io(path))?;
    vdf::read(&text).map_err(|e| Error::Vdf(format!("{}: {e}", path.display())))
}

pub fn libraries(steam: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam.to_path_buf()];
    if let Ok(v) = read_vdf(&steam.join("steamapps").join("libraryfolders.vdf"))
        && let Some(lf) = v.get("libraryfolders")
    {
        for (_, b) in lf.children() {
            if let Some(p) = b.get("path").and_then(Value::text) {
                let p = PathBuf::from(p);
                if !out.iter().any(|o| same(o, &p)) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase().trim_end_matches('\\')
        == b.to_string_lossy().to_lowercase().trim_end_matches('\\')
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppManifest {
    pub appid: u32,
    pub name: String,
    pub installdir: String,
    pub buildid: String,
    pub branch: Option<String>,
    pub updated: Option<u64>,
}

pub fn read_app_manifest(path: &Path) -> CoreResult<AppManifest> {
    let v = read_vdf(path)?;
    let a = v
        .get("AppState")
        .ok_or_else(|| Error::Vdf(format!("{}: falta AppState", path.display())))?;
    let t = |k: &str| {
        a.get(k)
            .and_then(Value::text)
            .unwrap_or_default()
            .to_string()
    };
    Ok(AppManifest {
        appid: t("appid").parse().unwrap_or(0),
        name: t("name"),
        installdir: t("installdir"),
        buildid: t("buildid"),
        branch: a
            .path(&["UserConfig", "BetaKey"])
            .and_then(Value::text)
            .map(str::to_string),
        updated: t("LastUpdated").parse().ok(),
    })
}

pub fn find_app(libraries: &[PathBuf], appid: u32) -> Option<(PathBuf, AppManifest, PathBuf)> {
    for b in libraries {
        let acf = b.join("steamapps").join(format!("appmanifest_{appid}.acf"));
        if let Ok(m) = read_app_manifest(&acf) {
            let folder = b.join("steamapps").join("common").join(&m.installdir);
            if folder.is_dir() {
                return Some((b.clone(), m, folder));
            }
        }
    }
    None
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WorkshopItem {
    pub size: u64,
    pub updated: u64,
    pub manifest: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WorkshopItemDetails {
    pub manifest: String,
    pub last_manifest: Option<String>,
    pub last_updated: Option<u64>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct WorkshopState {
    pub needs_update: bool,
    pub items: BTreeMap<u64, WorkshopItem>,
    pub details: BTreeMap<u64, WorkshopItemDetails>,
}

impl WorkshopState {
    pub fn pending_ids(&self) -> Vec<u64> {
        self.details
            .iter()
            .filter(|(id, d)| {
                let installed = self.items.get(id).map(|e| e.manifest.as_str());
                match installed {
                    None => true,
                    Some(m) => d.last_manifest.as_deref().is_some_and(|u| u != m),
                }
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

pub fn read_workshop_state(library: &Path, appid: u32) -> CoreResult<WorkshopState> {
    let path = library
        .join("steamapps")
        .join("workshop")
        .join(format!("appworkshop_{appid}.acf"));
    let v = read_vdf(&path)?;
    let a = v
        .get("AppWorkshop")
        .ok_or_else(|| Error::Vdf(format!("{}: falta AppWorkshop", path.display())))?;
    let mut est = WorkshopState {
        needs_update: a.get("NeedsUpdate").and_then(Value::text) == Some("1"),
        ..Default::default()
    };
    if let Some(install) = a.get("WorkshopItemsInstalled") {
        for (id, e) in install.children() {
            let Ok(id) = id.parse() else { continue };
            let n = |k: &str| {
                e.get(k)
                    .and_then(Value::text)
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0)
            };
            est.items.insert(
                id,
                WorkshopItem {
                    size: n("size"),
                    updated: n("timeupdated"),
                    manifest: e
                        .get("manifest")
                        .and_then(Value::text)
                        .unwrap_or_default()
                        .to_string(),
                },
            );
        }
    }
    if let Some(detail) = a.get("WorkshopItemDetails") {
        for (id, e) in detail.children() {
            let Ok(id) = id.parse() else { continue };
            let t = |k: &str| e.get(k).and_then(Value::text).map(str::to_string);
            est.details.insert(
                id,
                WorkshopItemDetails {
                    manifest: t("manifest").unwrap_or_default(),
                    last_manifest: t("latest_manifest"),
                    last_updated: t("latest_timeupdated").and_then(|x| x.parse().ok()),
                },
            );
        }
    }
    Ok(est)
}

pub fn workshop_content_folder(library: &Path, appid: u32) -> PathBuf {
    library
        .join("steamapps")
        .join("workshop")
        .join("content")
        .join(appid.to_string())
}

pub fn launch_options(steam: &Path, appid: u32) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(users) = fs::read_dir(steam.join("userdata")) else {
        return out;
    };
    for u in users.flatten() {
        let path = u.path().join("config").join("localconfig.vdf");
        let Ok(v) = read_vdf(&path) else { continue };
        let options = v
            .path(&[
                "UserLocalConfigStore",
                "Software",
                "Valve",
                "Steam",
                "apps",
                &appid.to_string(),
            ])
            .and_then(|a| a.get("LaunchOptions"))
            .and_then(Value::text);
        if let Some(o) = options {
            out.push((u.file_name().to_string_lossy().to_string(), o.to_string()));
        }
    }
    out
}
