use std::cmp::Ordering;
use std::fmt;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Version {
    #[serde(alias = "mayor")]
    pub major: u32,
    #[serde(alias = "menor")]
    pub minor: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32) -> Self {
        Version { major, minor }
    }

    pub fn parse(t: &str) -> Option<Version> {
        let t = t.trim();
        if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        let mut parts = t.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = match parts.next() {
            Some(m) => m.parse().ok()?,
            None => 0,
        };
        if parts.any(|p| p.parse::<u32>().is_err()) {
            return None;
        }
        Some(Version { major, minor })
    }

    pub fn from_mod_info(t: &str) -> Option<Version> {
        let t = t.trim();
        let (major, rest) = t.split_once('.')?;
        let minor: String = rest.chars().take_while(char::is_ascii_digit).collect();
        Some(Version {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

pub fn pick_folder<'a>(names: impl IntoIterator<Item = &'a str>, game: Version) -> Option<&'a str> {
    let mut best: Option<(Version, &str)> = None;
    for n in names {
        let Some(v) = Version::parse(n) else {
            continue;
        };
        if v.major != game.major || v > game {
            continue;
        }
        let wins = match best {
            None => true,
            Some((m, mn)) => match v.cmp(&m) {
                Ordering::Greater => true,
                Ordering::Equal => n.len() < mn.len(),
                Ordering::Less => false,
            },
        };
        if wins {
            best = Some((v, n));
        }
    }
    best.map(|(_, n)| n)
}
