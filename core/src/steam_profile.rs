use std::path::Path;

use crate::vdf::{self, Value};

const STEAMID64_BASE: u64 = 76561197960265728;
const PZ_APP: &str = "108600";
const MAX_AVATAR: u64 = 1_048_576;

#[derive(Debug, Clone, PartialEq)]
pub struct SteamProfile {
    pub steamid: u64,
    pub name: String,
    pub minutes: Option<u64>,
    pub last_played: Option<i64>,
    pub avatar_png: Option<Vec<u8>>,
}

pub fn account_id(steamid64: u64) -> u32 {
    steamid64.wrapping_sub(STEAMID64_BASE) as u32
}

pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn read_vdf(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    vdf::read(&String::from_utf8_lossy(&bytes)).ok()
}

pub fn read(steam_root: &Path) -> Option<SteamProfile> {
    let users = read_vdf(&steam_root.join("config").join("loginusers.vdf"))?;
    let field = |v: &Value, k: &str| v.get(k).and_then(Value::text).map(str::to_string);
    let (id, user) = users
        .get("users")?
        .children()
        .iter()
        .filter_map(|(k, v)| Some((k.parse::<u64>().ok()?, v)))
        .max_by_key(|(_, v)| {
            let recent = field(v, "MostRecent").as_deref() == Some("1");
            let ts = field(v, "Timestamp")
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);
            (recent, ts)
        })?;
    let app = read_vdf(
        &steam_root
            .join("userdata")
            .join(account_id(id).to_string())
            .join("config")
            .join("localconfig.vdf"),
    );
    let app = app.as_ref().and_then(|v| {
        v.path(&[
            "UserLocalConfigStore",
            "Software",
            "Valve",
            "Steam",
            "apps",
            PZ_APP,
        ])
    });
    let avatar = steam_root
        .join("config")
        .join("avatarcache")
        .join(format!("{id}.png"));
    Some(SteamProfile {
        steamid: id,
        name: field(user, "PersonaName").unwrap_or_default(),
        minutes: app.and_then(|a| field(a, "Playtime")?.parse().ok()),
        last_played: app.and_then(|a| field(a, "LastPlayed")?.parse().ok()),
        avatar_png: std::fs::metadata(&avatar)
            .is_ok_and(|m| m.len() <= MAX_AVATAR)
            .then(|| std::fs::read(&avatar).ok())
            .flatten(),
    })
}
