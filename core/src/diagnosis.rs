use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::console::Issue;
use crate::launch::ConsoleReport;
use crate::modinfo::normalize_id;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JavaCrash {
    pub kind: String,
    pub frame: Option<String>,
    pub java_frames: Vec<String>,
    pub out_of_memory: bool,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub origin: CrashOrigin,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashOrigin {
    GraphicsDriver,
    Lwjgl,
    Audio,
    ZbNative,
    Jvm,
    #[default]
    Unknown,
}

fn frame_module(frame: &str) -> Option<String> {
    let (_, rest) = frame.split_once('[')?;
    let end = rest.find(['+', ']']).unwrap_or(rest.len());
    let m = rest[..end].trim();
    (!m.is_empty()).then(|| m.to_string())
}

pub fn origin_of(module: &str) -> CrashOrigin {
    let m = module.to_ascii_lowercase();
    let m = m.strip_suffix(".dll").unwrap_or(&m);
    const DRIVERS: [&str; 7] = [
        "nvoglv", "nvd3dum", "atio6axx", "atig6pxx", "atioglxx", "amdxc", "ig",
    ];
    if m == "zbnative" {
        CrashOrigin::ZbNative
    } else if m == "jvm" {
        CrashOrigin::Jvm
    } else if m.starts_with("lwjgl") {
        CrashOrigin::Lwjgl
    } else if m.starts_with("fmod") || m.starts_with("openal") {
        CrashOrigin::Audio
    } else if DRIVERS.iter().any(|d| {
        if *d == "ig" {
            m.starts_with("ig") && m.ends_with("icd64")
        } else {
            m.starts_with(d)
        }
    }) {
        CrashOrigin::GraphicsDriver
    } else {
        CrashOrigin::Unknown
    }
}

pub fn parse_hs_err(text: &str) -> Option<JavaCrash> {
    let lines: Vec<&str> = text.lines().collect();
    let body = |l: &str| l.trim_start_matches('#').trim().to_string();
    let oom = lines
        .iter()
        .find(|l| l.contains("java.lang.OutOfMemoryError"));
    let insufficient = lines
        .iter()
        .find(|l| l.contains("insufficient memory for the Java Runtime"));
    let fatal = lines
        .iter()
        .position(|l| l.contains("A fatal error has been detected"));
    let kind = if let Some(l) = oom {
        body(l)
    } else if let Some(l) = insufficient {
        body(l)
    } else {
        let l = lines[fatal? + 1..]
            .iter()
            .map(|l| body(l))
            .find(|l| !l.is_empty())?;
        l.split(" at pc=").next().unwrap_or(&l).trim().to_string()
    };
    let frame = lines
        .iter()
        .position(|l| l.contains("Problematic frame:"))
        .and_then(|i| lines.get(i + 1))
        .map(|l| body(l))
        .filter(|l| !l.is_empty());
    let java_frames = lines
        .iter()
        .position(|l| l.starts_with("Java frames:"))
        .map(|i| {
            lines[i + 1..]
                .iter()
                .take_while(|l| !l.trim().is_empty())
                .filter(|l| l.starts_with("J ") || l.starts_with("j "))
                .take(10)
                .map(|l| l.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let module = frame.as_deref().and_then(frame_module);
    Some(JavaCrash {
        kind,
        origin: module.as_deref().map(origin_of).unwrap_or_default(),
        module,
        frame,
        java_frames,
        out_of_memory: oom.is_some() || insufficient.is_some(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndKind {
    Normal,
    StoppedByUser,
    JavaCrash,
    OutOfMemory,
    HungWhileLoading,
    Exited,
}

pub fn classify_end(
    code: Option<i32>,
    stopped_by_user: bool,
    crash: Option<&JavaCrash>,
    console_text: &str,
    loaded_all: bool,
) -> EndKind {
    if stopped_by_user {
        EndKind::StoppedByUser
    } else if crash.is_some_and(|c| c.out_of_memory)
        || (code != Some(0) && ends_out_of_memory(console_text))
    {
        EndKind::OutOfMemory
    } else if crash.is_some() {
        EndKind::JavaCrash
    } else if code == Some(0) {
        EndKind::Normal
    } else if !loaded_all {
        EndKind::HungWhileLoading
    } else {
        EndKind::Exited
    }
}

fn ends_out_of_memory(console_text: &str) -> bool {
    console_text
        .lines()
        .rev()
        .take(50)
        .any(|l| l.contains("java.lang.OutOfMemoryError"))
}

pub fn loaded_all(requested: &[String], report: &ConsoleReport, blocked: &[String]) -> bool {
    let loaded: std::collections::BTreeSet<String> =
        report.loaded.iter().map(|m| normalize_id(m)).collect();
    let blocked: std::collections::BTreeSet<String> =
        blocked.iter().map(|m| normalize_id(m)).collect();
    requested
        .iter()
        .map(|m| normalize_id(m))
        .all(|m| loaded.contains(&m) || blocked.contains(&m))
}

pub fn not_loaded(requested: &[String], report: &ConsoleReport) -> Vec<String> {
    if report.loaded.is_empty() {
        return Vec::new();
    }
    let loaded: std::collections::BTreeSet<String> =
        report.loaded.iter().map(|m| normalize_id(m)).collect();
    requested
        .iter()
        .map(|m| normalize_id(m))
        .filter(|m| !loaded.contains(m))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suspect {
    pub id: String,
    pub reason: SuspectReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum SuspectReason {
    CrashFrame(String),
    PatchesCrashClass(String),
    LuaErrors(usize),
    NotLoaded,
    Changed,
}

fn frame_class(frame: &str) -> Option<&str> {
    let w = frame.split_whitespace().find(|w| w.contains('('))?;
    Some(w.split('(').next()?.rsplit_once('.')?.0)
}

pub fn suspects(
    crash: Option<&JavaCrash>,
    issues: &[Issue],
    not_loaded: &[String],
    changed: &[String],
    java_packages: &[(String, String)],
    java_patches: &[(String, Vec<String>)],
) -> Vec<Suspect> {
    let mut out: Vec<Suspect> = Vec::new();
    let mut push = |id: &str, reason: SuspectReason| {
        let id = normalize_id(id);
        if !out.iter().any(|s| s.id == id) {
            out.push(Suspect { id, reason });
        }
    };
    if let Some(c) = crash {
        for f in &c.java_frames {
            if let Some((id, _)) = java_packages.iter().find(|(_, pkg)| {
                f.split_whitespace()
                    .any(|w| w.starts_with(&format!("{pkg}.")))
            }) {
                push(id, SuspectReason::CrashFrame(f.clone()));
            }
        }
        for f in &c.java_frames {
            let Some(class) = frame_class(f) else {
                continue;
            };
            for (id, classes) in java_patches {
                if classes.iter().any(|k| k == class) {
                    push(id, SuspectReason::PatchesCrashClass(f.clone()));
                }
            }
        }
    }
    let mut errors: Vec<(String, usize)> = Vec::new();
    for i in issues
        .iter()
        .filter(|i| i.severity == "error" && !i.noise && !i.from_game)
    {
        let Some(id) = &i.mod_id else { continue };
        match errors.iter_mut().find(|(m, _)| m == id) {
            Some((_, n)) => *n += i.times,
            None => errors.push((id.clone(), i.times)),
        }
    }
    errors.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (id, n) in errors {
        push(&id, SuspectReason::LuaErrors(n));
    }
    for id in changed {
        push(id, SuspectReason::Changed);
    }
    for id in not_loaded {
        push(id, SuspectReason::NotLoaded);
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LastGood {
    pub mods: BTreeMap<String, u64>,
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default)]
    pub zb: Option<String>,
}

pub const LAST_GOOD_FORMAT: u32 = 2;

pub fn changed_since(good: Option<&LastGood>, now: &BTreeMap<String, u64>) -> Vec<String> {
    let Some(good) = good.filter(|g| g.format >= LAST_GOOD_FORMAT) else {
        return Vec::new();
    };
    now.iter()
        .filter(|(id, stamp)| good.mods.get(*id) != Some(stamp))
        .map(|(id, _)| id.clone())
        .collect()
}

pub fn version_change(
    good: Option<&LastGood>,
    pick: impl Fn(&LastGood) -> Option<String>,
    now: Option<&str>,
) -> Option<(Option<String>, Option<String>)> {
    let good = good.filter(|g| g.format >= LAST_GOOD_FORMAT)?;
    let before = pick(good);
    (before.as_deref() != now).then(|| (before, now.map(str::to_string)))
}

pub fn signature(
    end: EndKind,
    crash: Option<&JavaCrash>,
    issues: &[Issue],
    not_loaded: &[String],
) -> Option<String> {
    const OTHERS: [&str; 4] = ["java.", "jdk.", "sun.", "zombie.core."];
    match end {
        EndKind::Normal | EndKind::StoppedByUser => None,
        EndKind::OutOfMemory => Some("out_of_memory".into()),
        EndKind::JavaCrash => {
            let c = crash?;
            let own = c.java_frames.iter().find_map(|f| {
                let w = f.split_whitespace().find(|w| w.contains('('))?;
                (!OTHERS.iter().any(|p| w.starts_with(p)))
                    .then(|| w.split('+').next().unwrap_or(w).to_string())
            });
            Some(format!(
                "java_crash:{}",
                own.unwrap_or_else(|| c.kind.clone())
            ))
        }
        EndKind::HungWhileLoading if !not_loaded.is_empty() => {
            Some(format!("hung:{}", normalize_id(not_loaded.last()?)))
        }
        _ => issues
            .iter()
            .find(|i| i.severity == "error" && !i.noise && !i.from_game)
            .map(|i| format!("lua:{}", i.key)),
    }
}

pub fn anonymize(text: &str, user: Option<&str>) -> String {
    let lower = text.to_ascii_lowercase();
    let user = user.map(str::to_ascii_lowercase).filter(|u| !u.is_empty());
    let b = lower.as_bytes();
    let seps = |from: usize| {
        lower[from..]
            .bytes()
            .take_while(|c| *c == b'\\' || *c == b'/')
            .count()
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let name_at = (b[i].is_ascii_alphabetic() && b.get(i + 1) == Some(&b':'))
            .then(|| i + 2 + seps(i + 2))
            .filter(|&u| u > i + 2 && lower[u..].starts_with("users"))
            .map(|u| u + 5 + seps(u + 5))
            .filter(|&n| b[n - 1] == b'\\' || b[n - 1] == b'/');
        if let Some(name_start) = name_at {
            let ends_name = |c: char| c == '\\' || c == '/' || c == '"' || c == '\'';
            let name_end = match &user {
                Some(u)
                    if lower[name_start..].starts_with(u.as_str())
                        && lower[name_start + u.len()..]
                            .chars()
                            .next()
                            .is_none_or(|c| ends_name(c) || c.is_whitespace()) =>
                {
                    name_start + u.len()
                }
                _ => lower[name_start..]
                    .find(|c: char| ends_name(c) || c.is_whitespace())
                    .map(|n| name_start + n)
                    .unwrap_or(text.len()),
            };
            out.push_str("<usuario>");
            i = name_end;
            continue;
        }
        let ch = text[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

pub struct ReportInput<'a> {
    pub game_version: &'a str,
    pub zb_version: Option<&'a str>,
    pub profile_name: &'a str,
    pub enabled: &'a [(String, Option<u64>)],
    pub end: EndKind,
    pub code: Option<i32>,
    pub crash: Option<&'a JavaCrash>,
    pub not_loaded: &'a [String],
    pub issues: &'a [Issue],
    pub user: Option<&'a str>,
}

pub fn report_text(r: &ReportInput) -> String {
    use std::fmt::Write;
    let mut t = String::new();
    let _ = writeln!(t, "PZ Mod Manager report");
    let _ = writeln!(t, "Game: {}", r.game_version);
    let _ = writeln!(
        t,
        "ZombieBuddy: {}",
        r.zb_version.unwrap_or("not installed")
    );
    let _ = writeln!(
        t,
        "Profile: {} ({} enabled mods)",
        r.profile_name,
        r.enabled.len()
    );
    let end = serde_json::to_value(r.end)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let _ = writeln!(
        t,
        "End: {end} (exit code {})",
        r.code.map_or("none".into(), |c| c.to_string())
    );
    match r.crash {
        Some(c) => {
            let _ = writeln!(t, "Crash: {}", c.kind);
            if let Some(f) = &c.frame {
                let _ = writeln!(t, "  frame: {f}");
            }
            if let Some(m) = &c.module {
                let _ = writeln!(t, "  module: {m} ({:?})", c.origin);
            }
            for f in &c.java_frames {
                let _ = writeln!(t, "  {f}");
            }
        }
        None => {
            let _ = writeln!(t, "Crash: none");
        }
    }
    let _ = writeln!(
        t,
        "Not loaded: {}",
        if r.not_loaded.is_empty() {
            "none".to_string()
        } else {
            r.not_loaded.join(", ")
        }
    );
    let _ = writeln!(t, "Errors (first 20):");
    for i in r.issues.iter().take(20) {
        let who = i
            .mod_id
            .as_deref()
            .or(i.mod_name.as_deref())
            .unwrap_or("game");
        let _ = writeln!(t, "- [{who}] x{} {}", i.times, i.message);
    }
    let _ = writeln!(t, "Load order:");
    for (n, (id, ws)) in r.enabled.iter().enumerate() {
        match ws {
            Some(w) => {
                let _ = writeln!(t, "{}. {id} ({w})", n + 1);
            }
            None => {
                let _ = writeln!(t, "{}. {id}", n + 1);
            }
        }
    }
    anonymize(&t, r.user)
}
