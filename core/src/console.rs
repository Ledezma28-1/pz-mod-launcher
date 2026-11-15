use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Log,
    Debug,
    Other,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub line: usize,
    pub hour: Option<String>,
    pub level: Level,
    pub channel: String,
    pub origin: String,
    pub text: String,
    pub rest: Vec<String>,
}

pub fn header(l: &str) -> Option<(Option<String>, Level, String, String, String)> {
    let (hour, l) = match l.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((h, r)) => (h.split(' ').nth(1).map(|x| x.chars().take(8).collect()), r),
        None => (None, l),
    };
    let (level_txt, r) = l.split_once(':')?;
    let level = match level_txt.trim() {
        "ERROR" => Level::Error,
        "WARN" => Level::Warn,
        "LOG" => Level::Log,
        "DEBUG" | "TRACE" => Level::Debug,
        _ => return None,
    };
    let r = r.trim_start();
    let (channel, r) = r.split_once(char::is_whitespace).unwrap_or((r, ""));
    let (origin, text) = r.split_once('>')?;
    let mut origin = origin.trim().to_string();
    let mut text = text.trim_start();
    if let Some((class_file, rest)) = text.split_once('>') {
        let c = class_file.trim();
        if !c.is_empty() && !c.contains(char::is_whitespace) && (c.contains('.') || c.contains('$'))
        {
            origin = format!("{origin} at {c}");
            text = rest.trim_start();
        }
    }
    Some((hour, level, channel.to_string(), origin, text.to_string()))
}

pub fn entries(text: &str) -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    for (i, l) in text.lines().enumerate() {
        match header(l) {
            Some((hour, level, channel, origin, text)) => v.push(Entry {
                line: i + 1,
                hour,
                level,
                channel,
                origin,
                text,
                rest: Vec::new(),
            }),
            None => match v.last_mut() {
                Some(e) => e.rest.push(l.to_string()),
                None => v.push(Entry {
                    line: i + 1,
                    hour: None,
                    level: Level::Other,
                    channel: String::new(),
                    origin: String::new(),
                    text: l.to_string(),
                    rest: Vec::new(),
                }),
            },
        }
    }
    v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Lua,
    Java,
    General,
    Script,
    Require,
    Zb,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Step {
    pub func: String,
    pub file: String,
    pub line: Option<u32>,
    pub from_mod: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnownMod {
    pub id: String,
    pub name: String,
    pub folder: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub kind: Kind,
    pub severity: &'static str,
    pub message: String,
    pub mod_name: Option<String>,
    pub mod_id: Option<String>,
    pub mods_involved: Vec<String>,
    pub file: Option<String>,
    pub line_file: Option<u32>,
    pub trace: Vec<Step>,
    pub java: Vec<String>,
    pub times: usize,
    pub key: String,
    pub noise: bool,
    pub from_game: bool,
    pub first: usize,
    pub last: usize,
    pub first_hour: Option<String>,
    pub last_hour: Option<String>,
}

pub fn step(l: &str) -> Option<Step> {
    let l = l.trim();
    let r = l.strip_prefix("function:")?;
    let (func, r) = r.split_once("-- file:")?;
    let (file_line, origin) = r.split_once('|').unwrap_or((r, ""));
    let (file, line) = match file_line.split_once("line #") {
        Some((a, n)) => (a.trim(), n.trim().parse().ok()),
        None => (file_line.trim(), None),
    };
    let origin = origin.trim().trim_end_matches('.');
    let from_mod = origin
        .strip_prefix("MOD:")
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    Some(Step {
        func: func.trim().to_string(),
        file: file.to_string(),
        line,
        from_mod,
    })
}

fn recognize(name: Option<&str>, file: Option<&str>, mods: &[KnownMod]) -> Option<String> {
    let same = |a: &str, b: &str| {
        a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
    };
    if let Some(n) = name {
        let n = n.trim();
        if let Some(m) = mods.iter().find(|m| same(&m.name, n) || same(&m.id, n)) {
            return Some(m.id.clone());
        }
    }
    let path = file?.replace('\\', "/");
    let folders: Vec<&str> = path
        .match_indices("/mods/")
        .filter_map(|(i, _)| path[i + 6..].split_once('/').map(|(f, _)| f))
        .collect();
    mods.iter()
        .find(|m| !m.folder.is_empty() && folders.iter().any(|f| same(f, &m.folder)))
        .map(|m| m.id.clone())
}

fn involved(trace: &[Step], mods: &[KnownMod]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in trace.iter().filter_map(|s| s.from_mod.as_deref()) {
        let who = recognize(Some(name), None, mods).unwrap_or_else(|| name.to_string());
        if !out.contains(&who) {
            out.push(who);
        }
    }
    if out.len() < 2 { Vec::new() } else { out }
}

pub struct ModSource<'a> {
    pub id: String,
    pub pkg: Option<String>,
    pub scanned: &'a crate::mods::ScannedMod,
}

fn needle(i: &Issue) -> Option<String> {
    match i.kind {
        Kind::Script => i.message.split('"').nth(1),
        Kind::Require => i
            .message
            .split_once("require(\"")
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(n, _)| n),
        _ => None,
    }
    .map(str::trim)
    .filter(|n| n.len() >= 3)
    .map(str::to_string)
}

type FoundCache = std::sync::Mutex<std::collections::HashMap<(String, bool, u64), Vec<String>>>;
static FOUND: std::sync::OnceLock<FoundCache> = std::sync::OnceLock::new();

pub fn attribute(issues: &mut [Issue], mods: &[ModSource], budget: std::time::Duration) {
    use std::hash::{Hash, Hasher};
    let t0 = std::time::Instant::now();
    for i in issues
        .iter_mut()
        .filter(|i| i.mod_id.is_none() && i.kind == Kind::Java)
    {
        let tag = i.mod_name.as_deref().map(str::to_lowercase);
        if let Some(m) = mods.iter().find(|m| {
            m.pkg.as_deref().is_some_and(|p| {
                let dotted = format!("{p}.");
                i.java.iter().any(|l| l.contains(&dotted))
                    || tag.as_deref().is_some_and(|t| {
                        p.rsplit('.')
                            .next()
                            .is_some_and(|last| last.to_lowercase() == t)
                    })
            })
        }) {
            i.mod_id = Some(m.id.clone());
        }
    }
    let scanned: Vec<&crate::mods::ScannedMod> = mods.iter().map(|m| m.scanned).collect();
    crate::mods::ScannedMod::script_refs_all(&scanned);
    for i in issues
        .iter_mut()
        .filter(|i| i.mod_id.is_none() && i.kind == Kind::Script)
    {
        let Some(n) = needle(i) else { continue };
        let users: Vec<String> = mods
            .iter()
            .filter(|m| m.scanned.script_refs().contains(&n))
            .map(|m| m.id.clone())
            .collect();
        i.mod_id = users.first().cloned();
        if users.len() > 1 {
            i.mods_involved = users;
        }
    }
    let set = {
        let mut h = std::hash::DefaultHasher::new();
        for m in mods {
            (&m.id, &m.scanned.folder).hash(&mut h);
        }
        h.finish()
    };
    let cache = FOUND.get_or_init(Default::default);
    let mut wanted: Vec<(String, bool, Vec<String>, bool)> = Vec::new();
    let searched = |i: &Issue| {
        i.mod_id.is_none() && i.kind == Kind::Require && !i.from_game && i.mod_name.is_none()
    };
    for i in issues.iter().filter(|i| searched(i)) {
        if let Some(n) = needle(i)
            && !wanted.iter().any(|w| w.0 == n)
        {
            let script = i.kind == Kind::Script;
            let known = cache
                .lock()
                .ok()
                .and_then(|c| c.get(&(n.clone(), script, set)).cloned());
            let done = known.is_some();
            wanted.push((n, script, known.unwrap_or_default(), done));
        }
    }
    let full = |script: bool, found: &Vec<String>| found.len() >= if script { 3 } else { 2 };
    let mut complete = true;
    if wanted.iter().any(|w| !w.3) {
        'mods: for m in mods {
            for rel in m.scanned.relative_paths() {
                let script = rel.starts_with("media/scripts/") && rel.ends_with(".txt");
                let lua = rel.ends_with(".lua");
                if !(script || lua)
                    || !wanted
                        .iter()
                        .any(|w| !w.3 && w.1 == script && !full(w.1, &w.2))
                {
                    continue;
                }
                if t0.elapsed() > budget {
                    complete = false;
                    break 'mods;
                }
                let Some(text) = m
                    .scanned
                    .path_of(rel)
                    .and_then(|p| std::fs::read(p).ok())
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                else {
                    continue;
                };
                for (n, s, found, known) in wanted.iter_mut() {
                    if !*known
                        && *s == script
                        && !full(*s, found)
                        && !found.contains(&m.id)
                        && text.contains(n.as_str())
                    {
                        found.push(m.id.clone());
                    }
                }
            }
        }
    }
    if complete && let Ok(mut c) = cache.lock() {
        for (n, s, found, known) in &wanted {
            if !known {
                c.insert((n.clone(), *s, set), found.clone());
            }
        }
    }
    for i in issues.iter_mut().filter(|i| searched(i)) {
        let Some(n) = needle(i) else { continue };
        let Some((_, script, found, _)) = wanted.iter().find(|w| w.0 == n) else {
            continue;
        };
        if found.len() == 1 || (*script && !found.is_empty()) {
            i.mod_id = found.first().cloned();
        }
        if found.len() > 1 {
            i.mods_involved = found.clone();
        }
    }
}

pub fn analyze(text: &str, mods: &[KnownMod]) -> Vec<Issue> {
    let es = entries(text);
    let mut out: Vec<Issue> = Vec::new();
    let mut used = vec![false; es.len()];
    type GroupKey = (Kind, String, Option<String>, Option<u32>, Option<String>);
    let mut groups: std::collections::HashMap<GroupKey, usize> = std::collections::HashMap::new();
    let mut push_issue = |mut issue: Issue, e: &Entry| {
        issue.key = normalize(&issue.message);
        issue.first = e.line;
        issue.last = e.line;
        issue.first_hour = e.hour.clone();
        issue.last_hour = e.hour.clone();
        let id = (
            issue.kind,
            issue.key.clone(),
            issue.file.clone(),
            issue.line_file,
            issue.mod_name.clone(),
        );
        if let Some(&i) = groups.get(&id) {
            let o = &mut out[i];
            o.times += 1;
            o.last = e.line;
            o.last_hour = e.hour.clone();
        } else {
            groups.insert(id, out.len());
            out.push(issue);
        }
    };
    let base = bare;
    for i in 0..es.len() {
        if used[i] {
            continue;
        }
        let e = &es[i];
        if e.origin.contains("flushErrorMessage") && !e.text.contains("stack trace") {
            let message = if e.text.starts_with('-') || e.text.is_empty() {
                e.rest
                    .iter()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty() && !l.starts_with('-'))
                    .unwrap_or("")
                    .to_string()
            } else {
                e.text.clone()
            };
            let mut issue = base(
                Kind::Lua,
                "error",
                message.trim_end_matches('.').to_string(),
            );
            for j in i + 1..(i + 5).min(es.len()) {
                let s = &es[j];
                if s.origin.contains("flushErrorMessage") && s.text.contains("stack trace") {
                    issue.trace = s.rest.iter().filter_map(|l| step(l)).collect();
                    used[j] = true;
                } else if s.origin.contains("ExceptionLogger") {
                    issue.java = s
                        .rest
                        .iter()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .take(12)
                        .collect();
                    used[j] = true;
                    if es.get(j + 1).is_some_and(|x| {
                        x.origin.contains("flushErrorMessage") && x.text.contains("stack trace")
                    }) {
                        used[j + 1] = true;
                    }
                    break;
                } else if s.origin.contains("flushErrorMessage") {
                    break;
                }
            }
            if let Some(p) = issue
                .trace
                .iter()
                .find(|p| p.from_mod.is_some())
                .or(issue.trace.first())
            {
                issue.mod_name = p.from_mod.clone();
                issue.file = Some(p.file.clone());
                issue.line_file = p.line;
            }
            issue.mod_id = recognize(issue.mod_name.as_deref(), issue.file.as_deref(), mods);
            issue.mods_involved = involved(&issue.trace, mods);
            push_issue(issue, e);
        } else if e.origin.contains("flushErrorMessage") {
            let mut issue = base(Kind::Lua, "error", String::new());
            issue.trace = e.rest.iter().filter_map(|l| step(l)).collect();
            if let Some(j) =
                (i + 1..(i + 3).min(es.len())).find(|&j| es[j].origin.contains("ExceptionLogger"))
            {
                issue.java = es[j]
                    .rest
                    .iter()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .take(12)
                    .collect();
                used[j] = true;
                if es.get(j + 1).is_some_and(|x| {
                    x.origin.contains("flushErrorMessage") && x.text.contains("stack trace")
                }) {
                    used[j + 1] = true;
                }
            }
            issue.message = issue
                .java
                .first()
                .map(|l| message_of_java(l))
                .unwrap_or_else(|| "error de Lua".to_string());
            if let Some(p) = issue
                .trace
                .iter()
                .find(|p| p.from_mod.is_some())
                .or(issue.trace.first())
            {
                issue.mod_name = p.from_mod.clone();
                issue.file = Some(p.file.clone());
                issue.line_file = p.line;
            }
            issue.mod_id = recognize(issue.mod_name.as_deref(), issue.file.as_deref(), mods);
            issue.mods_involved = involved(&issue.trace, mods);
            push_issue(issue, e);
        } else if e.origin.contains("ExceptionLogger") {
            let java: Vec<String> = e
                .rest
                .iter()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .take(12)
                .collect();
            let message = java.first().cloned().unwrap_or_else(|| e.text.clone());
            let mut issue = base(Kind::Java, "error", message);
            issue.java = java;
            push_issue(issue, e);
        } else if let Some(issue) = zb_line(&e.text, mods) {
            push_issue(issue, e);
        } else if let Some(mut issue) = java_link(&e.text) {
            issue.mod_id = recognize(issue.mod_name.as_deref(), None, mods);
            push_issue(issue, e);
        } else if let Some(r) = e.text.strip_prefix("ERROR: ") {
            push_issue(base(Kind::Script, "warning", r.trim().to_string()), e);
        } else if e.level == Level::Error {
            let mut m = e.text.trim().to_string();
            if m.is_empty() || m.starts_with('-') {
                m = e
                    .rest
                    .iter()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .unwrap_or("")
                    .to_string();
            }
            push_issue(base(Kind::General, "error", m), e);
        } else if e.level == Level::Warn
            && e.text.contains("require(\"")
            && e.text.trim_end_matches('.').ends_with("failed")
        {
            let mut issue = base(
                Kind::Require,
                "warning",
                e.text.trim().trim_end_matches('.').to_string(),
            );
            if let Some((who, file)) = lua_origin(&e.origin) {
                issue.file = Some(format!("{file}.lua"));
                if who == "Vanilla" {
                    issue.from_game = true;
                } else {
                    issue.mod_name = Some(who.to_string());
                    issue.mod_id = recognize(Some(who), None, mods);
                }
            }
            push_issue(issue, e);
        }
    }
    out
}

fn bare(kind: Kind, severity: &'static str, message: String) -> Issue {
    Issue {
        kind,
        severity,
        message,
        mod_name: None,
        mod_id: None,
        mods_involved: Vec::new(),
        file: None,
        line_file: None,
        trace: Vec::new(),
        java: Vec::new(),
        times: 1,
        key: String::new(),
        noise: false,
        from_game: false,
        first: 0,
        last: 0,
        first_hour: None,
        last_hour: None,
    }
}

fn zb_line(text: &str, mods: &[KnownMod]) -> Option<Issue> {
    let r = text.trim().strip_prefix("[ZB]")?.trim();
    if let Some(r) = r.strip_prefix("Excluded:") {
        let r = r.trim();
        let end = r.find("mod.info")? + "mod.info".len();
        let (path, reason) = r.split_at(end);
        let reason = reason.trim().trim_start_matches('(').trim_end_matches(')');
        if reason.starts_with("loaded as Java agent") {
            return None;
        }
        let mut i = bare(Kind::Zb, "warning", format!("Excluded ({reason})"));
        i.file = Some(path.trim().to_string());
        let named = reason
            .split("modId=")
            .nth(1)
            .map(|x| x.trim_end_matches(')').trim());
        i.mod_id = recognize(named, Some(path), mods);
        i.mod_name = named.map(str::to_string);
        return Some(i);
    }
    const BAD: [(&str, &str); 7] = [
        ("ERROR transforming", "error"),
        ("Target class IGNORED", "error"),
        ("multiple MethodDelegation patches", "warning"),
        ("JAR not found", "error"),
        ("JAR does not contain package", "error"),
        ("Cannot open JAR", "error"),
        ("Invalid package name", "error"),
    ];
    let (_, severity) = BAD.iter().find(|(p, _)| r.starts_with(p))?;
    Some(bare(Kind::Zb, severity, r.to_string()))
}

fn java_link(text: &str) -> Option<Issue> {
    const LINK: [&str; 5] = [
        "java.lang.NoSuchFieldError",
        "java.lang.NoSuchMethodError",
        "java.lang.NoClassDefFoundError",
        "java.lang.AbstractMethodError",
        "java.lang.IncompatibleClassChangeError",
    ];
    let at = LINK.iter().filter_map(|k| text.find(k)).min()?;
    let message = text[at..].trim_end_matches(')').trim().to_string();
    let mut i = bare(Kind::Java, "warning", message);
    i.java = vec![text.trim().to_string()];
    i.mod_name = text
        .trim()
        .strip_prefix('[')
        .and_then(|r| r.split(['/', ']']).next())
        .filter(|t| !t.is_empty() && !t.contains(' '))
        .map(str::to_string);
    Some(i)
}

fn lua_origin(origin: &str) -> Option<(&str, &str)> {
    let r = &origin[origin.find("Lua(")? + 4..];
    let (who, r) = r.split_once(").")?;
    let file = r.split_whitespace().next()?;
    (!who.is_empty() && !file.is_empty()).then_some((who, file))
}

pub fn normalize(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let chars: Vec<char> = message.chars().collect();
    let mut i = 0;
    let path_start = |i: usize| {
        let rest: String = chars[i..].iter().take(7).collect();
        (chars.len() > i + 2
            && chars[i].is_ascii_alphabetic()
            && chars[i + 1] == ':'
            && (chars[i + 2] == '\\' || chars[i + 2] == '/'))
            || rest.starts_with("/home/")
            || rest.starts_with("/Users/")
    };
    while i < chars.len() {
        let before = if i == 0 { ' ' } else { chars[i - 1] };
        let word_before = before.is_alphanumeric() || before == '_';
        if !word_before && path_start(i) {
            let last_sep = (i..chars.len())
                .rev()
                .find(|&k| chars[k] == '\\' || chars[k] == '/')
                .unwrap_or(i);
            let end = (last_sep..chars.len())
                .find(|&k| chars[k].is_whitespace() || chars[k] == ':')
                .unwrap_or(chars.len());
            out.push_str("<path>");
            i = end;
            continue;
        }
        if !word_before && chars[i].is_ascii_digit() {
            let mut j = i;
            if chars[i] == '0' && chars.get(i + 1).is_some_and(|c| *c == 'x' || *c == 'X') {
                j += 2;
                while j < chars.len() && chars[j].is_ascii_hexdigit() {
                    j += 1;
                }
            } else {
                while j < chars.len()
                    && (chars[j].is_ascii_digit()
                        || (chars[j] == '.' && chars.get(j + 1).is_some_and(char::is_ascii_digit)))
                {
                    j += 1;
                }
            }
            if !chars.get(j).is_some_and(|c| c.is_alphabetic() || *c == '_') {
                out.push('N');
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn message_of_java(l: &str) -> String {
    let m = l.split_once(": ").map(|(_, m)| m).unwrap_or(l);
    let m = m.rsplit_once(" at ").map(|(m, _)| m).unwrap_or(m);
    m.trim().trim_end_matches('.').to_string()
}

pub fn last_lines(text: &str, n: usize) -> Vec<String> {
    let v: Vec<&str> = text.lines().collect();
    v[v.len().saturating_sub(n)..]
        .iter()
        .map(|s| s.to_string())
        .collect()
}
