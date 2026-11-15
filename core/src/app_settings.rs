use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;
use crate::profiles::{self, Profile};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub window_on_play: WindowOnPlay,
    pub integrity_check: bool,
    pub new_profile: NewProfile,
    pub clear_webview_cache: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            window_on_play: WindowOnPlay::Nothing,
            integrity_check: true,
            new_profile: NewProfile::default(),
            clear_webview_cache: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowOnPlay {
    #[default]
    Nothing,
    MinimizeRestore,
    Minimize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewProfile {
    pub xmx: String,
    pub game_extra: Vec<String>,
    pub zb_usage: String,
    pub zb_policy: String,
    pub import_game_mods: bool,
}

impl Default for NewProfile {
    fn default() -> Self {
        NewProfile {
            xmx: String::new(),
            game_extra: Vec::new(),
            zb_usage: "auto".into(),
            zb_policy: "prompt".into(),
            import_game_mods: false,
        }
    }
}

pub fn read(path: &Path) -> Result<AppSettings, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn write(path: &Path, s: &AppSettings) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(s).expect("settings serialize");
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(crate::error::Error::io(d))?;
    }
    profiles::write_atomic(path, &json)
}

pub fn validate(s: &AppSettings) -> Result<(), String> {
    let n = &s.new_profile;
    if !n.xmx.is_empty() && !profiles::is_valid_xmx(&n.xmx) {
        return Err("memoria no válida: usa un tamaño como 8g o 3072m".into());
    }
    if let Some(a) = n
        .game_extra
        .iter()
        .find(|a| crate::launch::is_dangerous_arg(a))
    {
        return Err(format!("opción de arranque no permitida: {a}"));
    }
    if !["auto", "si", "no"].contains(&n.zb_usage.as_str()) {
        return Err("opción de ZombieBuddy no válida".into());
    }
    if !["prompt", "deny-new", "allow-all"].contains(&n.zb_policy.as_str()) {
        return Err("política de ZombieBuddy no válida".into());
    }
    Ok(())
}

pub fn merge(current: &AppSettings, incoming: AppSettings) -> AppSettings {
    AppSettings {
        clear_webview_cache: current.clear_webview_cache,
        ..incoming
    }
}

pub fn apply_new_profile(p: &mut Profile, n: &NewProfile) {
    p.launch.xmx = n.xmx.clone();
    p.launch.game_extra = n.game_extra.clone();
    p.zombiebuddy.usage = n.zb_usage.clone();
    p.zombiebuddy.policy = n.zb_policy.clone();
}
