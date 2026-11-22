use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum When {
    Load,
    Event(String),
    Later,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub line: u32,
    pub wraps: bool,
    pub when: When,
    pub reset: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Sym(&'static str),
    Value,
    Str(String),
}

const SYMS: [&str; 24] = [
    "...", "..", "==", "~=", "<=", ">=", "::", "=", ".", ":", ",", ";", "(", ")", "{", "}", "[",
    "]", "+", "-", "*", "/", "<", ">",
];

fn long_open(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'[') {
        return None;
    }
    let eq = s[1..].iter().take_while(|&&c| c == b'=').count();
    (s.get(1 + eq) == Some(&b'[')).then_some(eq)
}

fn tokens(src: &str) -> Vec<(Tok, u32)> {
    let s = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line) = (0usize, 1u32);
    let count_lines = |a: &[u8]| a.iter().filter(|&&c| c == b'\n').count() as u32;
    let skip_long = |i: usize, eq: usize| -> usize {
        let close: Vec<u8> = std::iter::once(b']')
            .chain(std::iter::repeat_n(b'=', eq))
            .chain(std::iter::once(b']'))
            .collect();
        let from = i + eq + 2;
        s[from..]
            .windows(close.len())
            .position(|w| w == close.as_slice())
            .map_or(s.len(), |p| from + p + close.len())
    };
    while i < s.len() {
        let c = s[i];
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"--") {
            let end = match long_open(&s[i + 2..]) {
                Some(eq) => skip_long(i + 2, eq),
                None => s[i..]
                    .iter()
                    .position(|&c| c == b'\n')
                    .map_or(s.len(), |p| i + p),
            };
            line += count_lines(&s[i..end]);
            i = end;
        } else if let Some(eq) = long_open(&s[i..]) {
            let end = skip_long(i, eq);
            line += count_lines(&s[i..end]);
            out.push((Tok::Value, line));
            i = end;
        } else if c == b'"' || c == b'\'' {
            let mut j = i + 1;
            while j < s.len() && s[j] != c && s[j] != b'\n' {
                j += if s[j] == b'\\' { 2 } else { 1 };
            }
            let inner = &src[(i + 1).min(j)..j.min(s.len())];
            let path_like = !inner.is_empty()
                && inner.len() < 128
                && inner
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'/' | b'.' | b'-'));
            out.push((
                if path_like {
                    Tok::Str(inner.to_string())
                } else {
                    Tok::Value
                },
                line,
            ));
            i = (j + 1).min(s.len());
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let j = i + s[i..]
                .iter()
                .position(|&c| !(c.is_ascii_alphanumeric() || c == b'_'))
                .unwrap_or(s.len() - i);
            out.push((Tok::Name(src[i..j].to_string()), line));
            i = j;
        } else if c.is_ascii_digit() {
            let j = i + s[i..]
                .iter()
                .position(|&c| !(c.is_ascii_alphanumeric() || c == b'.' || c == b'_'))
                .unwrap_or(s.len() - i);
            out.push((Tok::Value, line));
            i = j;
        } else if let Some(sym) = SYMS.iter().find(|y| s[i..].starts_with(y.as_bytes())) {
            out.push((Tok::Sym(sym), line));
            i += sym.len();
        } else {
            i += src[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

fn is(t: Option<&(Tok, u32)>, name: &str) -> bool {
    matches!(t, Some((Tok::Name(n), _)) if n == name)
}

fn sym(t: Option<&(Tok, u32)>, y: &str) -> bool {
    matches!(t, Some((Tok::Sym(s), _)) if *s == y)
}

fn dotted(t: &[(Tok, u32)], at: usize) -> Option<(Vec<String>, usize)> {
    let mut parts = Vec::new();
    let mut i = at;
    loop {
        let Some((Tok::Name(n), _)) = t.get(i) else {
            return None;
        };
        parts.push(n.clone());
        i += 1;
        if sym(t.get(i), ".") || sym(t.get(i), ":") {
            i += 1;
        } else {
            return Some((parts, i));
        }
    }
}

const KEYWORDS: [&str; 21] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "while",
];

struct Frame {
    function: bool,
    name: Option<String>,
    event: Option<String>,
    brackets: i32,
    server: bool,
    missing: Option<String>,
}

struct Found {
    parts: Vec<String>,
    line: u32,
    within: Option<(Option<String>, Option<String>)>,
    reset: bool,
    guarded: bool,
}

fn guarded(stack: &[Frame], name: &[String]) -> bool {
    let full = name.join(".");
    stack.iter().any(|f| {
        f.server
            || f.missing.as_deref().is_some_and(|m| {
                full == m || full.strip_prefix(m).is_some_and(|r| r.starts_with('.'))
            })
    })
}

pub fn definitions(src: &str) -> Vec<Def> {
    let t = tokens(src);
    let mut locals: BTreeSet<String> = BTreeSet::new();
    let mut found: Vec<Found> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut brackets = 0i32;
    let mut handlers: Vec<(String, String)> = Vec::new();
    let mut called: BTreeSet<String> = BTreeSet::new();
    let mut called_inside: BTreeSet<String> = BTreeSet::new();
    let within = |stack: &[Frame]| {
        stack
            .iter()
            .find(|f| f.function)
            .map(|f| (f.name.clone(), f.event.clone()))
    };
    let at = |i: usize, k: usize| i.checked_sub(k).and_then(|p| t.get(p));
    let mut i = 0;
    while i < t.len() {
        let level = stack.last().map_or(0, |f| f.brackets);
        let statement = brackets == level;
        let top = stack.is_empty() && statement;
        match &t[i].0 {
            Tok::Name(n) if n == "require" && top => {
                let arg = if sym(t.get(i + 1), "(") { i + 2 } else { i + 1 };
                if let Some((Tok::Str(path), line)) = t.get(arg) {
                    found.push(Found {
                        parts: vec!["^".to_string(), path.clone()],
                        line: *line,
                        within: None,
                        reset: false,
                        guarded: false,
                    });
                }
            }
            Tok::Name(n) if n == "for" => {
                let mut j = i + 1;
                while let Some((Tok::Name(x), _)) = t.get(j) {
                    if x == "in" {
                        break;
                    }
                    locals.insert(x.clone());
                    if !sym(t.get(j + 1), ",") {
                        break;
                    }
                    j += 2;
                }
            }
            Tok::Name(n) if n == "local" => {
                if is(t.get(i + 1), "function") {
                    if let Some((Tok::Name(f), _)) = t.get(i + 2) {
                        locals.insert(f.clone());
                    }
                } else {
                    let mut j = i + 1;
                    while let Some((Tok::Name(x), _)) = t.get(j) {
                        locals.insert(x.clone());
                        if !sym(t.get(j + 1), ",") {
                            break;
                        }
                        j += 2;
                    }
                }
            }
            Tok::Name(n) if n == "function" => {
                let named = dotted(&t, i + 1).filter(|(_, after)| sym(t.get(*after), "("));
                let open = named.as_ref().map_or(i + 1, |(_, after)| *after);
                if sym(t.get(open), "(") {
                    let mut j = open + 1;
                    while let Some((Tok::Name(x), _)) = t.get(j) {
                        locals.insert(x.clone());
                        j += if sym(t.get(j + 1), ",") { 2 } else { 1 };
                    }
                }
                locals.insert("self".to_string());
                if statement
                    && !is(at(i, 1), "local")
                    && let Some((parts, _)) = &named
                {
                    found.push(Found {
                        parts: parts.clone(),
                        line: t[i].1,
                        within: within(&stack),
                        reset: false,
                        guarded: guarded(&stack, parts),
                    });
                }
                let name = match &named {
                    Some((parts, _)) if parts.len() == 1 => Some(parts[0].clone()),
                    None if sym(at(i, 1), "=") && is(at(i, 3), "local") => match at(i, 2) {
                        Some((Tok::Name(x), _)) => Some(x.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let event = if sym(at(i, 1), "(")
                    && is(at(i, 2), "Add")
                    && sym(at(i, 3), ".")
                    && sym(at(i, 5), ".")
                    && is(at(i, 6), "Events")
                {
                    match at(i, 4) {
                        Some((Tok::Name(e), _)) => Some(e.clone()),
                        _ => None,
                    }
                } else {
                    None
                };
                stack.push(Frame {
                    function: true,
                    name,
                    event,
                    brackets,
                    server: false,
                    missing: None,
                });
            }
            Tok::Name(n) if n == "if" || n == "do" || n == "repeat" => {
                let server = n == "if"
                    && t[i + 1..]
                        .iter()
                        .take_while(|(x, _)| !matches!(x, Tok::Name(y) if y == "then"))
                        .any(|(x, _)| matches!(x, Tok::Name(y) if y == "isServer"));
                let missing = (n == "if" && is(t.get(i + 1), "not"))
                    .then(|| dotted(&t, i + 2))
                    .flatten()
                    .filter(|(_, after)| is(t.get(*after), "then"))
                    .map(|(parts, _)| parts.join("."));
                stack.push(Frame {
                    function: false,
                    name: None,
                    event: None,
                    brackets,
                    server,
                    missing,
                });
            }
            Tok::Name(n) if n == "end" || n == "until" => {
                stack.pop();
            }
            Tok::Name(n) if statement && !KEYWORDS.contains(&n.as_str()) => {
                let prev = at(i, 1);
                let starts = !(sym(prev, ".")
                    || sym(prev, ":")
                    || sym(prev, ",")
                    || is(prev, "local")
                    || is(prev, "function"));
                if starts && let Some((parts, after)) = dotted(&t, i) {
                    if top
                        && parts.len() == 3
                        && parts[0] == "Events"
                        && parts[2] == "Add"
                        && sym(t.get(after), "(")
                        && sym(t.get(after + 2), ")")
                        && let Some((Tok::Name(h), _)) = t.get(after + 1)
                    {
                        handlers.push((h.clone(), parts[1].clone()));
                    } else if parts.len() == 1 && sym(t.get(after), "(") {
                        if top {
                            called.insert(parts[0].clone());
                        } else {
                            called_inside.insert(parts[0].clone());
                        }
                    }
                    if sym(t.get(after), "=") {
                        if is(t.get(after + 1), "function") {
                            let guarded = guarded(&stack, &parts);
                            found.push(Found {
                                parts,
                                line: t[i].1,
                                within: within(&stack),
                                reset: false,
                                guarded,
                            });
                            i = after + 1;
                            continue;
                        }
                        let keeps = dotted(&t, after + 1)
                            .is_some_and(|(p, a)| p == parts && is(t.get(a), "or"));
                        if top && !keeps {
                            found.push(Found {
                                parts,
                                line: t[i].1,
                                within: None,
                                reset: true,
                                guarded: false,
                            });
                        }
                    }
                    i = after;
                    continue;
                }
            }
            Tok::Str(m)
                if m.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && sym(at(i, 1), ",")
                    && matches!(at(i, 2), Some((Tok::Name(_), _)))
                    && (sym(t.get(i + 1), ",") || sym(t.get(i + 1), ")")) =>
            {
                found.push(Found {
                    parts: vec!["*".to_string(), m.clone()],
                    line: t[i].1,
                    within: Some((None, None)),
                    reset: false,
                    guarded: false,
                });
            }
            Tok::Sym("(") | Tok::Sym("{") | Tok::Sym("[") => brackets += 1,
            Tok::Sym(")") | Tok::Sym("}") | Tok::Sym("]") => brackets = (brackets - 1).max(0),
            _ => {}
        }
        i += 1;
    }
    let read = values_read(&t);
    found
        .into_iter()
        .filter(|f| !f.guarded)
        .filter(|f| !locals.contains(&f.parts[0]))
        .map(|f| {
            let name = if f.parts[0] == "^" {
                format!("^{}", f.parts[1])
            } else {
                f.parts.join(".")
            };
            let when = match f.within {
                _ if f.parts[0] == "*" => When::Later,
                None => When::Load,
                Some((_, Some(e))) => When::Event(e),
                Some((Some(n), None)) => match handlers.iter().find(|(h, _)| *h == n) {
                    Some((_, e)) => When::Event(e.clone()),
                    None if called.contains(&n) && !called_inside.contains(&n) => When::Load,
                    None => When::Later,
                },
                Some((None, None)) => When::Later,
            };
            let hook = f.parts[0] == "*";
            Def {
                wraps: hook || (!f.reset && read.contains(&name)),
                name,
                line: f.line,
                when,
                reset: f.reset,
            }
        })
        .collect()
}

fn values_read(t: &[(Tok, u32)]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i < t.len() {
        let prev = i.checked_sub(1).and_then(|p| t.get(p));
        if matches!(t[i].0, Tok::Name(_))
            && !(sym(prev, ".") || sym(prev, ":") || is(prev, "function"))
            && let Some((p, after)) = dotted(t, i)
        {
            if p.len() > 1
                && !["(", "[", "=", ":", "{"]
                    .iter()
                    .any(|y| sym(t.get(after), y))
                && !matches!(t.get(after), Some((Tok::Value | Tok::Str(_), _)))
            {
                out.insert(p.join("."));
            }
            i = after;
            continue;
        }
        i += 1;
    }
    out
}

const MAX_FILE: u64 = 16 << 20;

pub fn definitions_of(path: &Path) -> Vec<Def> {
    let path = crate::paths::long(path);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_FILE) {
        return Vec::new();
    }
    std::fs::read(path)
        .map(|b| definitions(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

pub fn is_code(rel: &str) -> bool {
    rel.ends_with(".lua")
        && rel.strip_prefix("media/lua/").is_some_and(|x| {
            (x.starts_with("shared/") || x.starts_with("client/") || x.starts_with("server/"))
                && !x.contains("/translate/")
        })
}

pub fn run_order<'a>(
    game: impl Iterator<Item = &'a str>,
    mods: &[Vec<&'a str>],
) -> Vec<(&'a str, Option<usize>)> {
    let is_code = |r: &str, folder: &str| {
        r.ends_with(".lua")
            && r.strip_prefix("media/lua/")
                .is_some_and(|x| x.starts_with(folder) && !x.contains("/translate/"))
    };
    let game: Vec<&str> = game.collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for folder in ["shared/", "client/", "server/"] {
        let mut of_game: Vec<&str> = game
            .iter()
            .copied()
            .filter(|r| is_code(r, folder))
            .collect();
        of_game.sort_by_key(|r| r.to_lowercase());
        for r in of_game {
            if seen.insert(r) {
                out.push((r, None));
            }
        }
        for (k, m) in mods.iter().enumerate() {
            let mut own: Vec<&str> = m.iter().copied().filter(|r| is_code(r, folder)).collect();
            own.sort_by_key(|r| r.to_lowercase());
            for r in own {
                if seen.insert(r) {
                    out.push((r, Some(k)));
                }
            }
        }
    }
    out
}
