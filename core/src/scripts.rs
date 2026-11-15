use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScriptObject {
    pub kind: String,
    pub module: String,
    pub name: String,
    pub body: u64,
}

fn body_hash(body: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    for w in body.split_whitespace() {
        w.hash(&mut h);
    }
    h.finish()
}

impl ScriptObject {
    pub fn same_name(&self, o: &ScriptObject) -> bool {
        self.kind == o.kind && self.module == o.module && self.name == o.name
    }

    pub fn label(&self) -> String {
        format!("{} {}.{}", self.kind, self.module, self.name)
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = match rest[i + 2..].find("*/") {
            Some(j) => &rest[i + 2 + j + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse(text: &str) -> Vec<ScriptObject> {
    parse_bodies(text).into_iter().map(|(o, _)| o).collect()
}

pub fn parse_bodies(text: &str) -> Vec<(ScriptObject, String)> {
    let text = strip_comments(text);
    let mut texts: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut header = String::new();
    let mut module: Option<String> = None;
    let mut body = String::new();
    let mut current: Option<usize> = None;
    for c in text.chars() {
        match c {
            '{' => {
                let h = header
                    .trim_matches(|c: char| c.is_whitespace() || c == ',')
                    .to_string();
                header.clear();
                match depth {
                    0 => {
                        module = h
                            .strip_prefix("module")
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty());
                    }
                    1 => {
                        if let (Some(m), Some((kind, name))) =
                            (&module, h.split_once(char::is_whitespace))
                            && !kind.eq_ignore_ascii_case("imports")
                        {
                            out.push(ScriptObject {
                                kind: kind.to_lowercase(),
                                module: m.clone(),
                                name: name.trim().to_string(),
                                body: 0,
                            });
                            current = Some(out.len() - 1);
                        }
                        body.clear();
                    }
                    _ => body.push(c),
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                header.clear();
                match depth {
                    0 => module = None,
                    1 => {
                        if let Some(i) = current.take() {
                            out[i].body = body_hash(&body);
                            texts.resize(out.len(), String::new());
                            texts[i] = body.clone();
                        }
                        body.clear();
                    }
                    _ => body.push(c),
                }
            }
            _ if depth <= 1 => header.push(c),
            _ => body.push(c),
        }
    }
    texts.resize(out.len(), String::new());
    out.into_iter().zip(texts).collect()
}

pub fn properties(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut item = String::new();
    let flush = |item: &mut String, out: &mut Vec<(String, String)>| {
        if let Some((k, v)) = item.split_once('=') {
            let k = k.trim().to_lowercase();
            if !k.is_empty() && !k.contains(char::is_whitespace) {
                out.push((k, v.split_whitespace().collect::<Vec<_>>().join(" ")));
            }
        }
        item.clear();
    };
    for c in body.chars() {
        match c {
            '{' => {
                depth += 1;
                item.clear();
            }
            '}' => depth = depth.saturating_sub(1),
            ',' | '\n' if depth == 0 => flush(&mut item, &mut out),
            _ if depth == 0 => item.push(c),
            _ => {}
        }
    }
    flush(&mut item, &mut out);
    out
}

pub fn properties_of(path: &Path) -> Vec<(ScriptObject, Vec<(String, String)>)> {
    type Cache =
        Mutex<HashMap<PathBuf, (u64, SystemTime, Vec<(ScriptObject, Vec<(String, String)>)>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let Ok(md) = std::fs::metadata(crate::paths::long(path)) else {
        return Vec::new();
    };
    let Ok(date) = md.modified() else {
        return Vec::new();
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(g) = cache.lock()
        && let Some((size, d, v)) = g.get(path)
        && (*size, *d) == (md.len(), date)
    {
        return v.clone();
    }
    let bytes = std::fs::read(crate::paths::long(path)).unwrap_or_default();
    let v: Vec<_> = parse_bodies(&String::from_utf8_lossy(&bytes))
        .into_iter()
        .map(|(o, b)| (o, properties(&b)))
        .collect();
    if let Ok(mut g) = cache.lock() {
        g.insert(path.to_path_buf(), (md.len(), date, v.clone()));
    }
    v
}

pub fn cosmetic(key: &str) -> bool {
    matches!(
        key,
        "displayname"
            | "icon"
            | "tooltip"
            | "displaycategory"
            | "worldstaticmodel"
            | "staticmodel"
            | "iconsfortexture"
    )
}

pub fn objects_of(path: &Path) -> Vec<ScriptObject> {
    read_cached(path).0
}

pub fn refs_of(path: &Path) -> Vec<String> {
    read_cached(path).1
}

fn read_cached(path: &Path) -> (Vec<ScriptObject>, Vec<String>) {
    type Read = (Vec<ScriptObject>, Vec<String>);
    type Cache = Mutex<HashMap<PathBuf, (u64, SystemTime, Read)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let Ok(md) = std::fs::metadata(crate::paths::long(path)) else {
        return Default::default();
    };
    let Ok(date) = md.modified() else {
        return Default::default();
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(g) = cache.lock()
        && let Some((size, d, v)) = g.get(path)
        && (*size, *d) == (md.len(), date)
    {
        return v.clone();
    }
    let bytes = std::fs::read(crate::paths::long(path)).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let v = (parse(&text), template_refs(&text));
    if let Ok(mut g) = cache.lock() {
        g.insert(path.to_path_buf(), (md.len(), date, v.clone()));
    }
    v
}

pub fn template_refs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let text = strip_comments(text);
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(i) = text[from..].find("template") {
        let at = from + i;
        from = at + "template".len();
        if text[..at].chars().next_back().is_some_and(word) {
            continue;
        }
        let r = text[from..].trim_start_matches('!').trim_start();
        let Some(v) = r.strip_prefix('=') else {
            continue;
        };
        let v = v
            .split([',', '}'])
            .next()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        for (k, s) in v.split('/').map(str::trim).enumerate() {
            if k % 2 == 0 && !s.is_empty() && !out.iter().any(|o| o == s) {
                out.push(s.to_string());
            }
        }
    }
    out
}

pub fn compared(kind: &str) -> bool {
    matches!(
        kind,
        "item" | "craftrecipe" | "recipe" | "evolvedrecipe" | "fixing" | "entity" | "fluid"
    )
}

pub fn file_order(a: &str, b: &str) -> std::cmp::Ordering {
    let template = |p: &str| {
        !p.rsplit('/')
            .next()
            .unwrap_or(p)
            .to_lowercase()
            .starts_with("template_")
    };
    template(a).cmp(&template(b)).then_with(|| a.cmp(b))
}
