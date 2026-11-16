use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::class_file::{Class, ConstValue};
use crate::{CoreResult, Error};

const END: &str = "\r\n";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Kind {
    Boolean {
        default_value: bool,
    },
    Integer {
        min: i64,
        max: i64,
        default_value: i64,
    },
    Decimal {
        min: f64,
        max: f64,
        default_value: f64,
    },
    List {
        values: u32,
        default_value: u32,
    },
    Text {
        default_value: String,
        multiline: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxOption {
    pub name: String,
    pub kind: Kind,
    pub translation: Option<String>,
    pub value_translations: Option<String>,
    pub page: Option<String>,
    pub from_mod: Option<String>,
}

impl SandboxOption {
    pub fn table(&self) -> Option<&str> {
        self.name.split_once('.').map(|(t, _)| t)
    }
    pub fn short(&self) -> &str {
        self.name
            .split_once('.')
            .map(|(_, c)| c)
            .unwrap_or(&self.name)
    }
    fn key_text(&self) -> &str {
        self.translation.as_deref().unwrap_or(self.short())
    }
    pub fn default_value(&self) -> Value {
        match &self.kind {
            Kind::Boolean { default_value } => Value::Boolean(*default_value),
            Kind::Integer { default_value, .. } => Value::Integer(*default_value),
            Kind::Decimal { default_value, .. } => Value::Decimal(*default_value),
            Kind::List { default_value, .. } => Value::Integer(*default_value as i64),
            Kind::Text { default_value, .. } => Value::Text(default_value.clone()),
        }
    }
    pub fn clamp(&self, v: &Value) -> Value {
        match (&self.kind, v) {
            (Kind::Boolean { .. }, Value::Boolean(b)) => Value::Boolean(*b),
            (Kind::Boolean { default_value }, Value::Text(s)) => Value::Boolean(match s.trim() {
                "true" => true,
                "false" => false,
                _ => *default_value,
            }),
            (Kind::Integer { min, max, .. }, v) => match v.as_f64() {
                Some(x) => Value::Integer((x as i64).max(*min).min(*max)),
                None => self.default_value(),
            },
            (Kind::List { values, .. }, v) => match v.as_f64() {
                Some(x) => Value::Integer((x as i64).min(*values as i64).max(1)),
                None => self.default_value(),
            },
            (Kind::Decimal { min, max, .. }, v) => match v.as_f64() {
                Some(x) => Value::Decimal(if x < *min {
                    *min
                } else if x > *max {
                    *max
                } else {
                    x
                }),
                None => self.default_value(),
            },
            (Kind::Text { .. }, v) => Value::Text(v.as_text()),
            _ => self.default_value(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Boolean(bool),
    Integer(i64),
    Decimal(f64),
    Text(String),
}

impl Value {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(i) => Some(*i as f64),
            Value::Decimal(d) => Some(*d),
            Value::Text(s) => s.trim().parse().ok(),
            Value::Boolean(_) => None,
        }
    }
    fn as_text(&self) -> String {
        match self {
            Value::Boolean(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Decimal(d) => java_decimal(*d),
            Value::Text(s) => s.clone(),
        }
    }
    pub fn as_string(&self) -> String {
        self.as_text()
    }
    pub fn as_lua(&self) -> String {
        match self {
            Value::Text(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            v => v.as_text(),
        }
    }
}

pub fn java_decimal(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let a = v.abs();
    if v == 0.0 || (1e-3..1e7).contains(&a) {
        let s = format!("{v}");
        if s.contains('.') { s } else { format!("{s}.0") }
    } else {
        let s = format!("{v:E}");
        let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
        let m = if m.contains('.') {
            m.to_string()
        } else {
            format!("{m}.0")
        };
        format!("{m}E{e}")
    }
}

#[derive(Debug, Clone)]
enum V {
    Int(i64),
    Dec(f64),
    Str(String),
    Opt(usize),
    New(String),
    ClassRef(String),
    Static(String),
    Other,
}

impl V {
    fn int(&self) -> Option<i64> {
        match self {
            V::Int(i) => Some(*i),
            V::Dec(d) => Some(*d as i64),
            _ => None,
        }
    }
    fn dec(&self) -> Option<f64> {
        match self {
            V::Int(i) => Some(*i as f64),
            V::Dec(d) => Some(*d),
            _ => None,
        }
    }
    fn str(&self) -> Option<String> {
        match self {
            V::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}

fn signature(desc: &str) -> (usize, bool) {
    let (params, ret) = desc
        .strip_prefix('(')
        .and_then(|d| d.split_once(')'))
        .unwrap_or(("", "V"));
    let b = params.as_bytes();
    let (mut i, mut n) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'L' => {
                while i < b.len() && b[i] != b';' {
                    i += 1;
                }
                i += 1;
                n += 1;
            }
            b'[' => i += 1,
            _ => {
                i += 1;
                n += 1;
            }
        }
    }
    (n, ret != "V")
}

fn length(code: &[u8], pc: usize) -> usize {
    let op = code[pc];
    let u32_at = |i: usize| -> usize {
        code.get(i..i + 4)
            .map(|s| i32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
            .unwrap_or(0)
    };
    match op {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => 2,
        0x11
        | 0x13
        | 0x14
        | 0x84
        | 0x99..=0xa8
        | 0xb2..=0xb8
        | 0xbb
        | 0xbd
        | 0xc0
        | 0xc1
        | 0xc6
        | 0xc7 => 3,
        0xc5 => 4,
        0xb9 | 0xba | 0xc8 | 0xc9 => 5,
        0xc4 => {
            if code.get(pc + 1) == Some(&0x84) {
                6
            } else {
                4
            }
        }
        0xaa => {
            let base = (pc + 4) & !3;
            let (low, height) = (u32_at(base + 4) as i32, u32_at(base + 8) as i32);
            base + 12 + ((height - low + 1).max(0) as usize) * 4 - pc
        }
        0xab => {
            let base = (pc + 4) & !3;
            base + 8 + u32_at(base + 4) * 8 - pc
        }
        _ => 1,
    }
}

struct Reader {
    jar: zip::ZipArchive<std::fs::File>,
    options: Vec<SandboxOption>,
}

impl Reader {
    fn bytes_of(&mut self, class_name: &str) -> CoreResult<Vec<u8>> {
        use std::io::Read;
        let mut bytes = Vec::new();
        let mut e = self
            .jar
            .by_name(&format!("{class_name}.class"))
            .map_err(|_| Error::NotFound(format!("{class_name}.class en el jar")))?;
        e.read_to_end(&mut bytes)
            .map_err(|e| Error::Class(format!("{class_name}: {e}")))?;
        Ok(bytes)
    }

    fn enum_constants(&mut self, class_name: &str) -> CoreResult<Vec<String>> {
        Class::read(self.bytes_of(class_name)?)?.enum_constants()
    }

    fn walk(&mut self, class_name: &str) -> CoreResult<()> {
        let mut bytes = Vec::new();
        {
            use std::io::Read;
            let mut e = self
                .jar
                .by_name(&format!("{class_name}.class"))
                .map_err(|_| Error::NotFound(format!("{class_name}.class en el jar")))?;
            e.read_to_end(&mut bytes)
                .map_err(|e| Error::Class(format!("{class_name}: {e}")))?;
        }
        let c = Class::read(bytes)?;
        for code in c.code_of("<init>")? {
            self.parse(&c, &code)?;
        }
        Ok(())
    }

    fn parse(&mut self, c: &Class, code: &[u8]) -> CoreResult<()> {
        let mut stack: Vec<V> = Vec::new();
        let mut pc = 0;
        let u16_at = |i: usize| -> u16 {
            code.get(i..i + 2)
                .map(|s| u16::from_be_bytes([s[0], s[1]]))
                .unwrap_or(0)
        };
        while pc < code.len() {
            let op = code[pc];
            let step = length(code, pc);
            let take = |stack: &mut Vec<V>| stack.pop().unwrap_or(V::Other);
            match op {
                0x01 => stack.push(V::Other),
                0x02..=0x08 => stack.push(V::Int(op as i64 - 3)),
                0x09 | 0x0a => stack.push(V::Int(op as i64 - 9)),
                0x0b..=0x0d => stack.push(V::Dec((op - 0x0b) as f64)),
                0x0e | 0x0f => stack.push(V::Dec((op - 0x0e) as f64)),
                0x10 => stack.push(V::Int(code[pc + 1] as i8 as i64)),
                0x11 => stack.push(V::Int(u16_at(pc + 1) as i16 as i64)),
                0x12..=0x14 => {
                    let i = if op == 0x12 {
                        code[pc + 1] as u16
                    } else {
                        u16_at(pc + 1)
                    };
                    stack.push(match c.constant(i) {
                        Some(ConstValue::Integer(v)) => V::Int(v),
                        Some(ConstValue::Decimal(v)) => V::Dec(v),
                        Some(ConstValue::Str(s)) => V::Str(s),
                        None => c.class_name(i).map(V::ClassRef).unwrap_or(V::Other),
                    });
                }
                0x15..=0x2d => stack.push(V::Other),
                0x2e..=0x35 => {
                    take(&mut stack);
                    take(&mut stack);
                    stack.push(V::Other);
                }
                0x36..=0x4e | 0x57 | 0x58 | 0xb3 => {
                    take(&mut stack);
                }
                0x4f..=0x56 => {
                    for _ in 0..3 {
                        take(&mut stack);
                    }
                }
                0x59 => {
                    let t = stack.last().cloned().unwrap_or(V::Other);
                    stack.push(t);
                }
                0x74..=0x77 => match take(&mut stack) {
                    V::Int(i) => stack.push(V::Int(-i)),
                    V::Dec(d) => stack.push(V::Dec(-d)),
                    o => stack.push(o),
                },
                0x86 | 0x87 | 0x89 | 0x8a => {
                    let v = take(&mut stack);
                    stack.push(v.dec().map(V::Dec).unwrap_or(V::Other));
                }
                0x8b | 0x8c | 0x8e | 0x8f => {
                    let v = take(&mut stack);
                    stack.push(v.int().map(V::Int).unwrap_or(V::Other));
                }
                0x85 | 0x88 | 0x8d | 0x90..=0x93 => {}
                0xb2 => stack.push(
                    c.reference(u16_at(pc + 1))
                        .map(|(_, n, _)| V::Static(n))
                        .unwrap_or(V::Other),
                ),
                0xb4 => {
                    take(&mut stack);
                    stack.push(V::Other);
                }
                0xb5 => {
                    take(&mut stack);
                    take(&mut stack);
                }
                0xbb => stack.push(V::New(c.class_name(u16_at(pc + 1)).unwrap_or_default())),
                0xbc | 0xbd => {
                    take(&mut stack);
                    stack.push(V::Other);
                }
                0xc0 => {}
                0xb6..=0xb9 => {
                    let (cl, name, desc) = c.reference(u16_at(pc + 1)).unwrap_or_default();
                    let (n, returns) = signature(&desc);
                    let mut args: Vec<V> = (0..n).map(|_| take(&mut stack)).collect();
                    args.reverse();
                    let receiver = if op == 0xb8 {
                        V::Other
                    } else {
                        take(&mut stack)
                    };
                    let result = self.call(&cl, &name, &receiver, &args)?;
                    if returns {
                        stack.push(result);
                    }
                }
                _ => stack.clear(),
            }
            pc += step.max(1);
        }
        Ok(())
    }

    fn call(&mut self, cl: &str, name: &str, receiver: &V, a: &[V]) -> CoreResult<V> {
        let make = |o: Option<Kind>, name: Option<String>, this: &mut Self| -> V {
            match (o, name) {
                (Some(kind), Some(n)) => {
                    this.options.push(SandboxOption {
                        name: n,
                        kind,
                        translation: None,
                        value_translations: None,
                        page: None,
                        from_mod: None,
                    });
                    V::Opt(this.options.len() - 1)
                }
                _ => V::Other,
            }
        };
        if cl == "zombie/SandboxOptions" {
            let n = a.first().and_then(V::str);
            let kind = match name {
                "newBooleanOption" => a.get(1).and_then(V::int).map(|d| Kind::Boolean {
                    default_value: d != 0,
                }),
                "newIntegerOption" => match (a.get(1), a.get(2), a.get(3)) {
                    (Some(x), Some(y), Some(z)) => Some(Kind::Integer {
                        min: x.int().unwrap_or(0),
                        max: y.int().unwrap_or(0),
                        default_value: z.int().unwrap_or(0),
                    }),
                    _ => None,
                },
                "newDoubleOption" => match (a.get(1), a.get(2), a.get(3)) {
                    (Some(x), Some(y), Some(z)) => Some(Kind::Decimal {
                        min: x.dec().unwrap_or(0.0),
                        max: y.dec().unwrap_or(0.0),
                        default_value: z.dec().unwrap_or(0.0),
                    }),
                    _ => None,
                },
                "newEnumOption" => match (a.get(1), a.get(2)) {
                    (Some(V::ClassRef(enum_class)), Some(V::Static(constant))) => {
                        let ec = enum_class.clone();
                        let k = constant.clone();
                        let constants = self.enum_constants(&ec)?;
                        constants
                            .iter()
                            .position(|x| *x == k)
                            .map(|ord| Kind::List {
                                values: constants.len() as u32,
                                default_value: ord as u32 + 1,
                            })
                    }
                    (Some(x), Some(y)) => Some(Kind::List {
                        values: x.int().unwrap_or(1) as u32,
                        default_value: y.int().unwrap_or(1) as u32,
                    }),
                    _ => None,
                },
                "newStringOption" => Some(Kind::Text {
                    default_value: a.get(1).and_then(V::str).unwrap_or_default(),
                    multiline: false,
                }),
                _ => return Ok(V::Other),
            };
            return Ok(make(kind, n, self));
        }
        if let V::Opt(i) = receiver {
            let s = a.first().and_then(V::str);
            match name {
                "setTranslation" => self.options[*i].translation = s,
                "setValueTranslation" => self.options[*i].value_translations = s,
                "setPageName" => self.options[*i].page = s,
                _ => return Ok(V::Other),
            }
            return Ok(V::Opt(*i));
        }
        if name == "<init>"
            && let V::New(new_class) = receiver
        {
            if let Some(kind_srv) = new_class.strip_prefix("zombie/network/ServerOptions$") {
                let n = a.get(1).and_then(V::str);
                let kind = match kind_srv {
                    "BooleanServerOption" => a.get(2).and_then(V::int).map(|d| Kind::Boolean {
                        default_value: d != 0,
                    }),
                    "IntegerServerOption" => Some(Kind::Integer {
                        min: a.get(2).and_then(V::int).unwrap_or(0),
                        max: a.get(3).and_then(V::int).unwrap_or(0),
                        default_value: a.get(4).and_then(V::int).unwrap_or(0),
                    }),
                    "DoubleServerOption" => Some(Kind::Decimal {
                        min: a.get(2).and_then(V::dec).unwrap_or(0.0),
                        max: a.get(3).and_then(V::dec).unwrap_or(0.0),
                        default_value: a.get(4).and_then(V::dec).unwrap_or(0.0),
                    }),
                    "EnumServerOption" => Some(Kind::List {
                        values: a.get(2).and_then(V::int).unwrap_or(1) as u32,
                        default_value: a.get(3).and_then(V::int).unwrap_or(1) as u32,
                    }),
                    "StringServerOption" | "TextServerOption" => Some(Kind::Text {
                        default_value: a.get(2).and_then(V::str).unwrap_or_default(),
                        multiline: kind_srv == "TextServerOption",
                    }),
                    _ => None,
                };
                make(kind, n, self);
            } else if new_class.starts_with("zombie/SandboxOptions$") {
                let cn = new_class.clone();
                self.walk(&cn)?;
            }
        }
        Ok(V::Other)
    }
}

fn open(jar: &Path) -> CoreResult<Reader> {
    let f = std::fs::File::open(crate::paths::long(jar)).map_err(Error::io(jar))?;
    let file = zip::ZipArchive::new(f).map_err(|source| Error::Zip {
        path: jar.to_path_buf(),
        source,
    })?;
    Ok(Reader {
        jar: file,
        options: Vec::new(),
    })
}

pub fn game_options(jar: &Path) -> CoreResult<Vec<SandboxOption>> {
    let mut l = open(jar)?;
    l.walk("zombie/SandboxOptions")?;
    Ok(l.options)
}

pub fn server_options(jar: &Path) -> CoreResult<Vec<SandboxOption>> {
    let mut l = open(jar)?;
    l.walk("zombie/network/ServerOptions")?;
    Ok(l.options)
}

pub fn mod_options(text: &str, mod_id: &str) -> Vec<SandboxOption> {
    let without_comments: String = {
        let mut s = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find("/*") {
            s.push_str(&rest[..i]);
            rest = rest[i + 2..]
                .find("*/")
                .map(|j| &rest[i + 2 + j + 2..])
                .unwrap_or("");
        }
        s.push_str(rest);
        s.lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut out = Vec::new();
    let mut rest = without_comments.as_str();
    while let Some(i) = rest.find("option") {
        let tail = &rest[i + 6..];
        let Some(a) = tail.find('{') else { break };
        let Some(b) = tail[a..].find('}') else { break };
        let id = tail[..a].trim().trim_end_matches('=').trim().to_string();
        let body = &tail[a + 1..a + b];
        rest = &tail[a + b + 1..];
        if id.is_empty() || id.contains(char::is_whitespace) {
            continue;
        }
        let mut k: BTreeMap<String, String> = BTreeMap::new();
        for par in body.split(',') {
            if let Some((c, v)) = par.split_once('=') {
                k.insert(c.trim().to_string(), v.trim().to_string());
            }
        }
        let i64_ = |c: &str| k.get(c).and_then(|v| v.parse::<i64>().ok());
        let f64_ = |c: &str| k.get(c).and_then(|v| v.parse::<f64>().ok());
        let kind = match k.get("type").map(|s| s.as_str()) {
            Some("boolean") => Some(Kind::Boolean {
                default_value: k.get("default").map(|v| v == "true").unwrap_or(false),
            }),
            Some("integer") => match (i64_("min"), i64_("max"), i64_("default")) {
                (Some(min), Some(max), Some(default_value)) => Some(Kind::Integer {
                    min,
                    max,
                    default_value,
                }),
                _ => None,
            },
            Some("double") => match (f64_("min"), f64_("max"), f64_("default")) {
                (Some(min), Some(max), Some(default_value)) => Some(Kind::Decimal {
                    min,
                    max,
                    default_value,
                }),
                _ => None,
            },
            Some("enum") => match (i64_("numValues"), i64_("default")) {
                (Some(n), Some(d)) if n > 0 && d > 0 => Some(Kind::List {
                    values: n as u32,
                    default_value: d as u32,
                }),
                _ => None,
            },
            Some("string") => Some(Kind::Text {
                default_value: k.get("default").cloned().unwrap_or_default(),
                multiline: false,
            }),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        let not_empty = |c: &str| k.get(c).filter(|v| !v.is_empty()).cloned();
        out.push(SandboxOption {
            name: id,
            kind,
            translation: not_empty("translation"),
            value_translations: not_empty("valueTranslation"),
            page: not_empty("page"),
            from_mod: Some(mod_id.to_string()),
        });
    }
    out
}

#[derive(Debug, Default, Clone)]
pub struct Texts(BTreeMap<String, String>);

impl Texts {
    pub fn load(&mut self, text: &str, is_json: bool) {
        if is_json {
            if let Ok(serde_json::Value::Object(m)) =
                serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}'))
            {
                for (k, v) in m {
                    if let serde_json::Value::String(s) = v {
                        self.0.entry(k).or_insert(s);
                    }
                }
            }
            return;
        }
        for line in text.lines() {
            let l = line.trim();
            let Some((k, v)) = l.split_once('=') else {
                continue;
            };
            let v = v.trim().trim_end_matches(',').trim();
            if let Some(s) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                self.0
                    .entry(k.trim().to_string())
                    .or_insert_with(|| s.to_string());
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn sandbox_name(&self, o: &SandboxOption) -> String {
        self.get(&format!("Sandbox_{}", o.key_text()))
            .map(clear)
            .unwrap_or_else(|| o.short().to_string())
    }

    pub fn help_sandbox(&self, o: &SandboxOption) -> Option<String> {
        let c = o.key_text();
        let decimal_help = matches!(o.kind, Kind::Decimal { .. })
            .then(|| self.get(&format!("Sandbox_{c}_help")))
            .flatten();
        decimal_help
            .or_else(|| self.get(&format!("Sandbox_{c}_tooltip")))
            .map(clear)
    }

    pub fn sandbox_values(&self, o: &SandboxOption) -> Vec<Option<String>> {
        let Kind::List { values, .. } = o.kind else {
            return Vec::new();
        };
        let c = o.value_translations.as_deref().unwrap_or(o.key_text());
        (1..=values)
            .map(|i| self.get(&format!("Sandbox_{c}_option{i}")).map(clear))
            .collect()
    }

    pub fn help_server(&self, o: &SandboxOption) -> Option<String> {
        self.get(&format!("UI_ServerOption_{}_tooltip", o.name))
            .map(clear)
    }

    pub fn server_values(&self, o: &SandboxOption) -> Vec<Option<String>> {
        let Kind::List { values, .. } = o.kind else {
            return Vec::new();
        };
        (1..=values)
            .map(|i| {
                self.get(&format!("UI_ServerOption_{}_option{i}", o.name))
                    .map(clear)
            })
            .collect()
    }
}

impl Texts {
    fn get_br(&self, key: &str) -> Option<String> {
        self.get(key).map(|s| {
            s.replace(
                "<br>", "
",
            )
            .replace(
                "<BR>", "
",
            )
        })
    }

    fn game_tooltip(&self, o: &SandboxOption, server: bool) -> Option<String> {
        let c = o.key_text();
        let base = if server {
            self.get_br(&format!("UI_ServerOption_{}_tooltip", o.name))
        } else if matches!(o.kind, Kind::Integer { .. } | Kind::Decimal { .. })
            && o.table() == Some("ZombieConfig")
        {
            self.get_br(&format!("Sandbox_{c}_help"))
        } else {
            self.get_br(&format!("Sandbox_{c}_tooltip"))
        };
        let min_max = |a: String, b: String, d: String| {
            self.get_br("Sandbox_MinMaxDefault")
                .map(|s| s.replace("%1", &a).replace("%2", &b).replace("%3", &d))
        };
        let extra = match &o.kind {
            Kind::Integer {
                min,
                max,
                default_value,
            } => min_max(min.to_string(), max.to_string(), default_value.to_string()),
            Kind::Decimal {
                min,
                max,
                default_value,
            } => min_max(
                format!("{min:.2}"),
                format!("{max:.2}"),
                format!("{default_value:.2}"),
            ),
            Kind::List { default_value, .. } if !server => self
                .raw_values(o)
                .get((*default_value as usize).wrapping_sub(1))
                .cloned()
                .flatten()
                .and_then(|v| self.get_br("Sandbox_Default").map(|s| s.replace("%1", &v))),
            _ => None,
        };
        match (base, extra) {
            (None, e) => e,
            (Some(b), None) => Some(b),
            (Some(b), Some(e)) => Some(format!("{b}\\n{e}")),
        }
    }

    fn raw_values(&self, o: &SandboxOption) -> Vec<Option<String>> {
        let Kind::List { values, .. } = o.kind else {
            return Vec::new();
        };
        let c = o.value_translations.as_deref().unwrap_or(o.key_text());
        (1..=values)
            .map(|i| self.get_br(&format!("Sandbox_{c}_option{i}")))
            .collect()
    }
}

fn clear(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("<br>", "\n")
        .replace("<BR>", "\n")
        .replace("\\\"", "\"")
        .replace("\\\\", "\u{1}")
        .replace('\\', "\"")
        .replace('\u{1}', "\\")
        .trim()
        .to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub name: String,
    pub options: Vec<(String, Option<String>)>,
}

pub fn pages(lua: &str) -> (Vec<Page>, Vec<Page>) {
    let Some(i) = lua.find("SettingsTable = {") else {
        return (Vec::new(), Vec::new());
    };
    let (mut ini, mut sandbox) = (Vec::new(), Vec::new());
    let mut group = 0;
    for line in lua[i..].lines().skip(1) {
        let sin = line.trim_start();
        let spaces = &line[..line.len() - sin.len()];
        let level = spaces.chars().filter(|c| *c == '\t').count()
            + spaces.chars().filter(|c| *c == ' ').count() / 4;
        if line.starts_with('}') {
            break;
        }
        let name = sin
            .split_once("name = \"")
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(n, _)| n.to_string());
        let title = sin
            .split_once("title = \"")
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(n, _)| n.to_string());
        match (level, name) {
            (2, Some(n)) => {
                group = match n.as_str() {
                    "INI" => 1,
                    "Sandbox" => 2,
                    _ => 0,
                }
            }
            (4, Some(n)) if sin.starts_with("name") => {
                let target = if group == 1 { &mut ini } else { &mut sandbox };
                target.push(Page {
                    name: n,
                    options: Vec::new(),
                });
            }
            (5, Some(n)) if sin.starts_with('{') => {
                let target = if group == 1 { &mut ini } else { &mut sandbox };
                if let Some(p) = target.last_mut() {
                    p.options.push((n, title));
                }
            }
            _ => {}
        }
    }
    ini.retain(|p| !p.options.is_empty());
    sandbox.retain(|p| !p.options.is_empty());
    (ini, sandbox)
}

pub fn read_lua(text: &str) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut table: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let l = line.split("--").next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        if !inside {
            if l.ends_with('{') {
                inside = true;
            }
            continue;
        }
        if l.starts_with('}') {
            if table.pop().is_none() {
                break;
            }
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let k = k.trim().to_string();
        let v = v.trim().trim_end_matches(',').trim();
        if v == "{" {
            table.push(k);
            continue;
        }
        if k.eq_ignore_ascii_case("version") && table.is_empty() {
            continue;
        }
        let name = match table.last() {
            Some(t) => format!("{t}.{k}"),
            None => k,
        };
        out.insert(name, value_lua(v));
    }
    out
}

fn value_lua(v: &str) -> Value {
    if let Some(s) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Value::Text(s.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    match v {
        "true" => Value::Boolean(true),
        "false" => Value::Boolean(false),
        _ => {
            if let Ok(i) = v.parse::<i64>() {
                Value::Integer(i)
            } else if let Ok(d) = v.parse::<f64>() {
                Value::Decimal(d)
            } else {
                Value::Text(v.to_string())
            }
        }
    }
}

pub fn read_ini(text: &str) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim_start().starts_with("--") {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.is_empty() || k.eq_ignore_ascii_case("version") {
            continue;
        }
        out.insert(k.to_string(), Value::Text(v.to_string()));
    }
    out
}

fn comment(prefix: &str, text: &str, indent: &str) -> String {
    let t = text
        .replace("\\n", " ")
        .replace("\\\"", "\"")
        .replace('\n', &format!("{END}{indent}{prefix} "));
    format!("{indent}{prefix} {t}{END}")
}

pub fn write_lua(
    options: &[SandboxOption],
    values: &BTreeMap<String, Value>,
    tx: &Texts,
) -> String {
    let mut s = format!("SandboxVars = {{{END}    VERSION = 6,{END}");
    let value = |o: &SandboxOption| {
        o.clamp(values.get(&o.name).unwrap_or(&o.default_value()))
            .as_lua()
    };
    let notes = |o: &SandboxOption, indent: &str| {
        let mut n = String::new();
        if let Some(a) = tx.game_tooltip(o, false) {
            n.push_str(&comment("--", &a, indent));
        }
        for (i, v) in tx.raw_values(o).into_iter().enumerate() {
            if let Some(v) = v {
                let v = if o.table().is_none() {
                    v.replace("\\\"", "\"")
                } else {
                    v
                };
                n.push_str(&format!("{indent}-- {} = {v}{END}", i + 1));
            }
        }
        n
    };
    let mut tables: Vec<&str> = Vec::new();
    for o in options {
        match o.table() {
            None => {
                s.push_str(&notes(o, "    "));
                s.push_str(&format!("    {} = {},{END}", o.name, value(o)));
            }
            Some(t) if !tables.contains(&t) => tables.push(t),
            _ => {}
        }
    }
    for t in tables {
        s.push_str(&format!("    {t} = {{{END}"));
        for o in options.iter().filter(|o| o.table() == Some(t)) {
            s.push_str(&notes(o, "        "));
            s.push_str(&format!("        {} = {},{END}", o.short(), value(o)));
        }
        s.push_str(&format!("    }},{END}"));
    }
    s.push_str(&format!("}}{END}"));
    s
}

pub fn write_ini(
    options: &[SandboxOption],
    values: &BTreeMap<String, Value>,
    version: Option<u32>,
    help: impl Fn(&SandboxOption) -> (Option<String>, Vec<Option<String>>),
) -> String {
    let mut s = String::new();
    if let Some(v) = version {
        s.push_str(&format!("Version={v}{END}"));
    }
    for (k, o) in options.iter().enumerate() {
        let (text, list) = help(o);
        if let Some(a) = text {
            s.push_str(&comment("#", &a, ""));
        }
        for (i, v) in list.into_iter().enumerate() {
            if let Some(v) = v {
                let v = v.replace("\\\"", "\"");
                s.push_str(&format!("    -- {} = {v}{END}", i + 1));
            }
        }
        let v = o.clamp(values.get(&o.name).unwrap_or(&o.default_value()));
        s.push_str(&format!("{}={}", o.name, v.as_string()));
        if k + 1 < options.len() {
            s.push_str(END);
            s.push_str(END);
        }
    }
    s
}

pub const PROFILE_PREFIX: [&str; 3] = ["Mods", "WorkshopItems", "Map"];

#[derive(Debug, Clone, Serialize)]
pub struct UiOption {
    pub name: String,
    pub title: String,
    pub help: Option<String>,
    pub kind: Kind,
    pub values: Vec<String>,
    pub from_mod: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiGroup {
    pub title: Option<String>,
    pub options: Vec<UiOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiPage {
    pub id: String,
    pub title: String,
    pub from_mod: Option<String>,
    pub groups: Vec<UiGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    pub server: Vec<UiPage>,
    pub sandbox: Vec<UiPage>,
}

pub struct Definition {
    pub server: Vec<SandboxOption>,
    pub sandbox: Vec<SandboxOption>,
    pub texts: Texts,
    pages_ini: Vec<Page>,
    pages_sb: Vec<Page>,
}

fn read_if_has(p: &Path) -> Option<String> {
    std::fs::read_to_string(crate::paths::long(p)).ok()
}

pub fn definition(
    game: &Path,
    mods: &[&crate::mods::ScannedMod],
    language: &str,
) -> CoreResult<Definition> {
    definition_with_progress(game, mods, language, &mut |_, _, _| {})
}

pub fn definition_with_progress(
    game: &Path,
    mods: &[&crate::mods::ScannedMod],
    language: &str,
    progress: &mut dyn FnMut(&'static str, usize, usize),
) -> CoreResult<Definition> {
    let total = 4 + mods.len();
    progress("server", 0, total);
    let mut l = open(&game.join("projectzomboid.jar"))?;
    l.walk("zombie/network/ServerOptions")?;
    let server = std::mem::take(&mut l.options);
    progress("sandbox", 1, total);
    l.walk("zombie/SandboxOptions")?;
    let mut sandbox = l.options;
    progress("mods", 2, total);
    for (i, m) in mods.iter().enumerate() {
        progress("mods", 2 + i, total);
        let Some(id) = m.id() else { continue };
        let file = m
            .roots()
            .into_iter()
            .rev()
            .map(|r| r.join("media").join("sandbox-options.txt"))
            .find(|p| p.is_file());
        if let Some(t) = file.as_deref().and_then(read_if_has) {
            for o in mod_options(&t, id) {
                if !sandbox.iter().any(|x| x.name == o.name) {
                    sandbox.push(o);
                }
            }
        }
    }
    progress("texts", 2 + mods.len(), total);
    let mut texts = Texts::default();
    let mut languages = vec![language.to_uppercase()];
    if languages[0] != "EN" {
        languages.push("EN".into());
    }
    let game_tr = game
        .join("media")
        .join("lua")
        .join("shared")
        .join("Translate");
    for lang in &languages {
        for m in mods {
            for root in m.roots().into_iter().rev() {
                let d = root
                    .join("media")
                    .join("lua")
                    .join("shared")
                    .join("Translate")
                    .join(lang);
                if let Some(t) = read_if_has(&d.join("Sandbox.json")) {
                    texts.load(&t, true);
                }
                if let Some(t) = read_if_has(&d.join(format!("Sandbox_{lang}.txt"))) {
                    texts.load(&t, false);
                }
            }
        }
        for file in ["Sandbox.json", "UI.json"] {
            if let Some(t) = read_if_has(&game_tr.join(lang).join(file)) {
                texts.load(&t, true);
            }
        }
    }
    progress("pages", 3 + mods.len(), total);
    let lua = read_if_has(
        &game
            .join("media")
            .join("lua")
            .join("client")
            .join("OptionScreens")
            .join("ServerSettingsScreen.lua"),
    )
    .unwrap_or_default();
    let (pages_ini, pages_sb) = pages(&lua);
    progress("pages", total, total);
    Ok(Definition {
        server,
        sandbox,
        texts,
        pages_ini,
        pages_sb,
    })
}

impl Definition {
    fn ui_sandbox(&self, o: &SandboxOption) -> UiOption {
        let mut values: Vec<String> = self
            .texts
            .sandbox_values(o)
            .into_iter()
            .enumerate()
            .map(|(i, v)| v.unwrap_or_else(|| (i + 1).to_string()))
            .collect();
        if o.name == "StartYear" {
            values = (0..values.len()).map(|i| (1993 + i).to_string()).collect();
        } else if o.name == "StartDay" {
            values = (1..=values.len()).map(|i| i.to_string()).collect();
        }
        UiOption {
            name: o.name.clone(),
            title: self.texts.sandbox_name(o),
            help: self.texts.help_sandbox(o),
            kind: o.kind.clone(),
            values,
            from_mod: o.from_mod.clone(),
        }
    }

    fn server_ui(&self, o: &SandboxOption) -> UiOption {
        UiOption {
            name: o.name.clone(),
            title: o.name.clone(),
            help: self.texts.help_server(o),
            kind: o.kind.clone(),
            values: self
                .texts
                .server_values(o)
                .into_iter()
                .enumerate()
                .map(|(i, v)| v.unwrap_or_else(|| (i + 1).to_string()))
                .collect(),
            from_mod: None,
        }
    }

    fn make_groups(
        &self,
        p: &Page,
        search: impl Fn(&str) -> Option<UiOption>,
        group_title: impl Fn(&str) -> String,
    ) -> Vec<UiGroup> {
        let mut groups: Vec<UiGroup> = vec![UiGroup {
            title: None,
            options: Vec::new(),
        }];
        for (name, title) in &p.options {
            if let Some(t) = title {
                groups.push(UiGroup {
                    title: Some(group_title(t)),
                    options: Vec::new(),
                });
            }
            if let Some(o) = search(name) {
                groups.last_mut().expect("hay grupo").options.push(o);
            }
        }
        groups.retain(|g| !g.options.is_empty());
        groups
    }

    pub fn schema(&self) -> Schema {
        let tx = &self.texts;
        let mut used: std::collections::BTreeSet<&str> = PROFILE_PREFIX.into_iter().collect();
        let mut server: Vec<UiPage> = Vec::new();
        for p in &self.pages_ini {
            let groups = self.make_groups(
                p,
                |n| {
                    (!PROFILE_PREFIX.contains(&n))
                        .then(|| self.server.iter().find(|o| o.name == n))
                        .flatten()
                        .map(|o| self.server_ui(o))
                },
                |t| t.to_string(),
            );
            for g in &groups {
                for o in &g.options {
                    used.insert(
                        self.server
                            .iter()
                            .find(|x| x.name == o.name)
                            .map(|x| x.name.as_str())
                            .unwrap_or(""),
                    );
                }
            }
            if !groups.is_empty() {
                server.push(UiPage {
                    id: format!("ini:{}", p.name),
                    title: tx
                        .get(&format!("UI_ServerSettingGroup_{}", p.name))
                        .map(str::to_string)
                        .unwrap_or_else(|| p.name.clone()),
                    from_mod: None,
                    groups,
                });
            }
        }
        let rest: Vec<UiOption> = self
            .server
            .iter()
            .filter(|o| !used.contains(o.name.as_str()))
            .map(|o| self.server_ui(o))
            .collect();
        if !rest.is_empty() {
            server.push(UiPage {
                id: "ini:others".into(),
                title: String::new(),
                from_mod: None,
                groups: vec![UiGroup {
                    title: None,
                    options: rest,
                }],
            });
        }

        let mut used_sb: std::collections::BTreeSet<&str> = Default::default();
        let mut sandbox: Vec<UiPage> = Vec::new();
        for p in &self.pages_sb {
            let groups = self.make_groups(
                p,
                |n| {
                    self.sandbox
                        .iter()
                        .find(|o| o.name == n)
                        .map(|o| self.ui_sandbox(o))
                },
                |t| {
                    tx.get(&format!("Sandbox_Title_{t}"))
                        .map(str::to_string)
                        .unwrap_or_else(|| t.to_string())
                },
            );
            for g in &groups {
                for o in &g.options {
                    if let Some(x) = self.sandbox.iter().find(|x| x.name == o.name) {
                        used_sb.insert(x.name.as_str());
                    }
                }
            }
            if !groups.is_empty() {
                sandbox.push(UiPage {
                    id: format!("sb:{}", p.name),
                    title: tx
                        .get(&format!("Sandbox_{}", p.name))
                        .map(str::to_string)
                        .unwrap_or_else(|| p.name.clone()),
                    from_mod: None,
                    groups,
                });
            }
        }
        let mut without_page: Vec<UiOption> = Vec::new();
        for o in self.sandbox.iter().filter(|o| o.from_mod.is_some()) {
            if used_sb.contains(o.name.as_str()) {
                continue;
            }
            let ui = self.ui_sandbox(o);
            match &o.page {
                Some(pg) if sandbox.iter().any(|p| p.id == format!("sb:{pg}")) => {
                    let p = sandbox
                        .iter_mut()
                        .find(|p| p.id == format!("sb:{pg}"))
                        .expect("comprobado");
                    let title = o.from_mod.clone();
                    match p.groups.last_mut() {
                        Some(g) if g.title == title => g.options.push(ui),
                        _ => p.groups.push(UiGroup {
                            title,
                            options: vec![ui],
                        }),
                    }
                }
                Some(pg) => {
                    let id = format!("mod:{pg}");
                    match sandbox.iter_mut().find(|p| p.id == id) {
                        Some(p) => p.groups[0].options.push(ui),
                        None => sandbox.push(UiPage {
                            id,
                            title: tx
                                .get(&format!("Sandbox_{pg}"))
                                .map(str::to_string)
                                .unwrap_or_else(|| pg.clone()),
                            from_mod: o.from_mod.clone(),
                            groups: vec![UiGroup {
                                title: None,
                                options: vec![ui],
                            }],
                        }),
                    }
                }
                None => without_page.push(ui),
            }
        }
        let rest_of_game: Vec<UiOption> = self
            .sandbox
            .iter()
            .filter(|o| o.from_mod.is_none() && !used_sb.contains(o.name.as_str()))
            .map(|o| self.ui_sandbox(o))
            .collect();
        without_page.extend(rest_of_game);
        if !without_page.is_empty() {
            sandbox.push(UiPage {
                id: "sb:others".into(),
                title: String::new(),
                from_mod: None,
                groups: vec![UiGroup {
                    title: None,
                    options: without_page,
                }],
            });
        }
        Schema { server, sandbox }
    }

    pub fn complete(
        options: &[SandboxOption],
        parsed_map: &BTreeMap<String, Value>,
    ) -> BTreeMap<String, Value> {
        options
            .iter()
            .map(|o| {
                let v = parsed_map
                    .get(&o.name)
                    .map(|v| o.clamp(v))
                    .unwrap_or_else(|| o.default_value());
                (o.name.clone(), v)
            })
            .collect()
    }

    pub fn lua_text(&self, values: &BTreeMap<String, Value>) -> String {
        write_lua(&self.sandbox, values, &self.texts)
    }

    pub fn cfg_text(&self, values: &BTreeMap<String, Value>) -> String {
        write_ini(&self.sandbox, values, Some(6), |o| {
            (self.texts.game_tooltip(o, false), self.texts.raw_values(o))
        })
    }

    pub fn ini_text(&self, values: &BTreeMap<String, Value>) -> String {
        write_ini(&self.server, values, None, |o| {
            (self.texts.game_tooltip(o, true), Vec::new())
        })
    }
}

pub fn game_presets(game: &Path) -> Vec<String> {
    let d = game
        .join("media")
        .join("lua")
        .join("shared")
        .join("Sandbox");
    let mut v: Vec<String> = std::fs::read_dir(d)
        .map(|it| {
            it.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix(".lua")
                        .filter(|s| *s != "SandboxVars")
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn names_with(dir: &Path, suffix: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .strip_suffix(suffix)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn is_valid_name(n: &str) -> bool {
    let n = n.trim();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    !n.is_empty()
        && n.len() <= 64
        && !n.contains(['/', '\\', ':', ';', '.', '"', '<', '>', '|', '?', '*'])
        && !RESERVED.iter().any(|r| r.eq_ignore_ascii_case(n))
}
