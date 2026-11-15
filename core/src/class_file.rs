use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::{CoreResult, Error, paths};

#[derive(Debug, Clone)]
enum Constant {
    Utf8(String),
    Class(u16),
    Str(u16),
    Ref { class_file: u16, kind_name: u16 },
    KindName { name: u16, kind: u16 },
    Integer(i64),
    Decimal(f64),
    Other,
    Gap,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Integer(i64),
    Decimal(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationValue {
    Int(i64),
    Decimal(f64),
    Str(String),
    Enum(String),
    Array(Vec<AnnotationValue>),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub kind: String,
    pub values: Vec<(String, AnnotationValue)>,
}

impl Annotation {
    pub fn get(&self, name: &str) -> Option<&AnnotationValue> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            AnnotationValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Members {
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<(String, String)>,
    pub methods: Vec<(String, String)>,
}

pub struct Class {
    constants: Vec<Constant>,
    bytes: Vec<u8>,
    end_constants: usize,
}

fn u16_at(b: &[u8], i: usize) -> CoreResult<u16> {
    b.get(i..i + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| Error::Class("se acabó el archivo".into()))
}

impl Class {
    pub fn read(bytes: Vec<u8>) -> CoreResult<Class> {
        if bytes.get(0..4) != Some(&[0xCA, 0xFE, 0xBA, 0xBE]) {
            return Err(Error::Class("no empieza por CAFEBABE".into()));
        }
        let n = u16_at(&bytes, 8)? as usize;
        let mut c = vec![Constant::Gap; n.max(1)];
        let mut i = 10;
        let mut k = 1;
        while k < n {
            let tag = *bytes
                .get(i)
                .ok_or_else(|| Error::Class("tabla cortada".into()))?;
            c[k] = match tag {
                1 => {
                    let l = u16_at(&bytes, i + 1)? as usize;
                    let s = bytes
                        .get(i + 3..i + 3 + l)
                        .ok_or_else(|| Error::Class("utf8 cortado".into()))?;
                    i += 3 + l;
                    Constant::Utf8(String::from_utf8_lossy(s).into_owned())
                }
                3 => {
                    let v = i32::from_be_bytes(
                        bytes
                            .get(i + 1..i + 5)
                            .ok_or_else(|| Error::Class("entero cortado".into()))?
                            .try_into()
                            .expect("4 bytes"),
                    );
                    i += 5;
                    Constant::Integer(v as i64)
                }
                4 => {
                    let v = f32::from_be_bytes(
                        bytes
                            .get(i + 1..i + 5)
                            .ok_or_else(|| Error::Class("float cortado".into()))?
                            .try_into()
                            .expect("4 bytes"),
                    );
                    i += 5;
                    Constant::Decimal(v as f64)
                }
                5 | 6 => {
                    let b8: [u8; 8] = bytes
                        .get(i + 1..i + 9)
                        .ok_or_else(|| Error::Class("long/double cortado".into()))?
                        .try_into()
                        .expect("8 bytes");
                    i += 9;
                    c[k] = if tag == 5 {
                        Constant::Integer(i64::from_be_bytes(b8))
                    } else {
                        Constant::Decimal(f64::from_be_bytes(b8))
                    };
                    k += 2;
                    continue;
                }
                7 => {
                    let v = u16_at(&bytes, i + 1)?;
                    i += 3;
                    Constant::Class(v)
                }
                8 => {
                    let v = u16_at(&bytes, i + 1)?;
                    i += 3;
                    Constant::Str(v)
                }
                16 | 19 | 20 => {
                    i += 3;
                    Constant::Other
                }
                9..=11 => {
                    let (a, b) = (u16_at(&bytes, i + 1)?, u16_at(&bytes, i + 3)?);
                    i += 5;
                    Constant::Ref {
                        class_file: a,
                        kind_name: b,
                    }
                }
                12 => {
                    let (a, b) = (u16_at(&bytes, i + 1)?, u16_at(&bytes, i + 3)?);
                    i += 5;
                    Constant::KindName { name: a, kind: b }
                }
                17 | 18 => {
                    i += 5;
                    Constant::Other
                }
                15 => {
                    i += 4;
                    Constant::Other
                }
                t => {
                    return Err(Error::Class(format!(
                        "etiqueta {t} desconocida en la constante {k}"
                    )));
                }
            };
            k += 1;
        }
        Ok(Class {
            constants: c,
            bytes,
            end_constants: i,
        })
    }

    pub fn constant(&self, i: u16) -> Option<ConstValue> {
        match self.constants.get(i as usize)? {
            Constant::Integer(v) => Some(ConstValue::Integer(*v)),
            Constant::Decimal(v) => Some(ConstValue::Decimal(*v)),
            Constant::Str(n) => Some(ConstValue::Str(self.utf8(*n)?.to_string())),
            _ => None,
        }
    }

    pub fn reference(&self, i: u16) -> Option<(String, String, String)> {
        match self.constants.get(i as usize)? {
            Constant::Ref {
                class_file,
                kind_name,
            } => {
                let class_file = self.text(*class_file)?;
                match self.constants.get(*kind_name as usize)? {
                    Constant::KindName { name, kind } => Some((
                        class_file,
                        self.utf8(*name)?.to_string(),
                        self.utf8(*kind)?.to_string(),
                    )),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn class_name(&self, i: u16) -> Option<String> {
        match self.constants.get(i as usize)? {
            Constant::Class(n) => self.utf8(*n).map(str::to_string),
            _ => None,
        }
    }

    pub fn enum_constants(&self) -> CoreResult<Vec<String>> {
        let b = &self.bytes;
        let mut i = self.end_constants + 6;
        let n_interfaces = u16_at(b, i)? as usize;
        i += 2 + 2 * n_interfaces;
        let n_fields = u16_at(b, i)? as usize;
        i += 2;
        let mut out = Vec::new();
        for _ in 0..n_fields {
            let access = u16_at(b, i)?;
            let name = self.utf8(u16_at(b, i + 2)?).unwrap_or_default().to_string();
            if access & 0x4000 != 0 {
                out.push(name);
            }
            let n_attr = u16_at(b, i + 6)? as usize;
            i += 8;
            for _ in 0..n_attr {
                let length = u32::from_be_bytes(
                    b.get(i + 2..i + 6)
                        .ok_or_else(|| Error::Class("atributo cortado".into()))?
                        .try_into()
                        .expect("4 bytes"),
                ) as usize;
                i += 6 + length;
            }
        }
        Ok(out)
    }

    pub fn members(&self) -> CoreResult<Members> {
        let b = &self.bytes;
        let mut i = self.end_constants + 4;
        let super_class = self.class_name(u16_at(b, i)?);
        i += 2;
        let n_interfaces = u16_at(b, i)? as usize;
        i += 2;
        let mut interfaces = Vec::new();
        for k in 0..n_interfaces {
            interfaces.extend(self.class_name(u16_at(b, i + 2 * k)?));
        }
        i += 2 * n_interfaces;
        let table = |i: &mut usize| -> CoreResult<Vec<(String, String)>> {
            let n = u16_at(b, *i)? as usize;
            *i += 2;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                let name = self
                    .utf8(u16_at(b, *i + 2)?)
                    .unwrap_or_default()
                    .to_string();
                let kind = self
                    .utf8(u16_at(b, *i + 4)?)
                    .unwrap_or_default()
                    .to_string();
                out.push((name, kind));
                let n_attr = u16_at(b, *i + 6)? as usize;
                *i += 8;
                for _ in 0..n_attr {
                    let length = u32::from_be_bytes(
                        b.get(*i + 2..*i + 6)
                            .ok_or_else(|| Error::Class("atributo cortado".into()))?
                            .try_into()
                            .expect("4 bytes"),
                    ) as usize;
                    *i += 6 + length;
                }
            }
            Ok(out)
        };
        let fields = table(&mut i)?;
        let methods = table(&mut i)?;
        Ok(Members {
            super_class,
            interfaces,
            fields,
            methods,
        })
    }

    pub fn references(&self) -> Vec<(String, String, String)> {
        (1..self.constants.len() as u16)
            .filter(|&i| matches!(self.constants[i as usize], Constant::Ref { .. }))
            .filter_map(|i| self.reference(i))
            .collect()
    }

    pub fn class_refs(&self) -> Vec<String> {
        (1..self.constants.len() as u16)
            .filter_map(|i| self.class_name(i))
            .map(|c| {
                let c = c.trim_start_matches('[');
                c.strip_prefix('L')
                    .and_then(|x| x.strip_suffix(';'))
                    .unwrap_or(c)
                    .to_string()
            })
            .collect()
    }

    pub fn code_of(&self, method: &str) -> CoreResult<Vec<Vec<u8>>> {
        let b = &self.bytes;
        let mut i = self.end_constants + 6;
        let n_interfaces = u16_at(b, i)? as usize;
        i += 2 + 2 * n_interfaces;
        let skip_attributes = |mut i: usize| -> CoreResult<usize> {
            let n = u16_at(b, i)? as usize;
            i += 2;
            for _ in 0..n {
                let length = u32::from_be_bytes(
                    b.get(i + 2..i + 6)
                        .ok_or_else(|| Error::Class("atributo cortado".into()))?
                        .try_into()
                        .expect("4 bytes"),
                ) as usize;
                i += 6 + length;
            }
            Ok(i)
        };
        let n_fields = u16_at(b, i)? as usize;
        i += 2;
        for _ in 0..n_fields {
            i = skip_attributes(i + 6)?;
        }
        let n_methods = u16_at(b, i)? as usize;
        i += 2;
        let mut out = Vec::new();
        for _ in 0..n_methods {
            let name = self.utf8(u16_at(b, i + 2)?).unwrap_or_default().to_string();
            let mut j = i + 6;
            let n_attr = u16_at(b, j)? as usize;
            j += 2;
            for _ in 0..n_attr {
                let attr = self.utf8(u16_at(b, j)?).unwrap_or_default();
                let length = u32::from_be_bytes(
                    b.get(j + 2..j + 6)
                        .ok_or_else(|| Error::Class("atributo cortado".into()))?
                        .try_into()
                        .expect("4 bytes"),
                ) as usize;
                if name == method && attr == "Code" {
                    let n = u32::from_be_bytes(
                        b.get(j + 10..j + 14)
                            .ok_or_else(|| Error::Class("código cortado".into()))?
                            .try_into()
                            .expect("4 bytes"),
                    ) as usize;
                    out.push(
                        b.get(j + 14..j + 14 + n)
                            .ok_or_else(|| Error::Class("código cortado".into()))?
                            .to_vec(),
                    );
                }
                j += 6 + length;
            }
            i = j;
        }
        Ok(out)
    }

    fn utf8(&self, i: u16) -> Option<&str> {
        match self.constants.get(i as usize)? {
            Constant::Utf8(s) => Some(s),
            _ => None,
        }
    }

    fn text(&self, i: u16) -> Option<String> {
        self.text_at(i, 0)
    }

    fn text_at(&self, i: u16, depth: u8) -> Option<String> {
        if depth > 3 {
            return None;
        }
        match self.constants.get(i as usize)? {
            Constant::Utf8(s) => Some(s.clone()),
            Constant::Class(n) | Constant::Str(n) => self.utf8(*n).map(str::to_string),
            Constant::KindName { name, kind } => {
                Some(format!("{}:{}", self.utf8(*name)?, self.utf8(*kind)?))
            }
            Constant::Ref {
                class_file,
                kind_name,
            } => Some(format!(
                "{}.{}",
                self.text_at(*class_file, depth + 1)?,
                self.text_at(*kind_name, depth + 1)?
            )),
            _ => None,
        }
    }

    fn index(&self, text: &str) -> Option<u16> {
        (1..self.constants.len() as u16).find(|&i| {
            matches!(
                self.constants[i as usize],
                Constant::Class(_) | Constant::Ref { .. }
            ) && self.text(i).as_deref() == Some(text)
        })
    }

    pub fn strings_utf8(&self) -> impl Iterator<Item = &str> {
        self.constants.iter().filter_map(|c| match c {
            Constant::Utf8(s) => Some(s.as_str()),
            _ => None,
        })
    }

    pub fn this_class(&self) -> Option<String> {
        self.class_name(u16_at(&self.bytes, self.end_constants + 2).ok()?)
    }

    pub fn annotations(&self) -> CoreResult<Vec<Annotation>> {
        let b = &self.bytes;
        let attribute_length = |i: usize| -> CoreResult<usize> {
            Ok(u32::from_be_bytes(
                b.get(i + 2..i + 6)
                    .ok_or_else(|| Error::Class("atributo cortado".into()))?
                    .try_into()
                    .expect("4 bytes"),
            ) as usize)
        };
        let mut i = self.end_constants + 6;
        i += 2 + 2 * u16_at(b, i)? as usize;
        for _ in 0..2 {
            let n = u16_at(b, i)? as usize;
            i += 2;
            for _ in 0..n {
                let n_attr = u16_at(b, i + 6)? as usize;
                i += 8;
                for _ in 0..n_attr {
                    i += 6 + attribute_length(i)?;
                }
            }
        }
        let n_attr = u16_at(b, i)? as usize;
        i += 2;
        let mut visible = Vec::new();
        let mut invisible = Vec::new();
        for _ in 0..n_attr {
            let name = self.utf8(u16_at(b, i)?).unwrap_or_default();
            let length = attribute_length(i)?;
            let end = i + 6 + length;
            if b.len() < end {
                return Err(Error::Class("atributo cortado".into()));
            }
            let table = match name {
                "RuntimeVisibleAnnotations" => Some(&mut visible),
                "RuntimeInvisibleAnnotations" => Some(&mut invisible),
                _ => None,
            };
            if let Some(table) = table {
                let mut j = i + 8;
                for _ in 0..u16_at(b, i + 6)? {
                    let (a, next) = self.annotation(j, 0)?;
                    table.push(a);
                    j = next;
                }
                if j > end {
                    return Err(Error::Class("anotación cortada".into()));
                }
            }
            i = end;
        }
        visible.extend(invisible);
        Ok(visible)
    }

    fn annotation(&self, i: usize, depth: u8) -> CoreResult<(Annotation, usize)> {
        if depth > MAX_DEPTH {
            return Err(Error::Class("anotaciones anidadas sin fin".into()));
        }
        let b = &self.bytes;
        let kind = self.utf8(u16_at(b, i)?).unwrap_or_default().to_string();
        let n = u16_at(b, i + 2)?;
        let mut j = i + 4;
        let mut values = Vec::new();
        for _ in 0..n {
            let name = self.utf8(u16_at(b, j)?).unwrap_or_default().to_string();
            let (v, next) = self.annotation_value(j + 2, depth + 1)?;
            values.push((name, v));
            j = next;
        }
        Ok((Annotation { kind, values }, j))
    }

    fn annotation_value(&self, i: usize, depth: u8) -> CoreResult<(AnnotationValue, usize)> {
        if depth > MAX_DEPTH {
            return Err(Error::Class("anotaciones anidadas sin fin".into()));
        }
        let b = &self.bytes;
        let tag = *b
            .get(i)
            .ok_or_else(|| Error::Class("anotación cortada".into()))?;
        let index = u16_at(b, i + 1)?;
        Ok(match tag {
            b'B' | b'C' | b'I' | b'S' | b'Z' | b'J' | b'D' | b'F' => {
                let v = match self.constants.get(index as usize) {
                    Some(Constant::Integer(v)) => AnnotationValue::Int(*v),
                    Some(Constant::Decimal(v)) => AnnotationValue::Decimal(*v),
                    _ => AnnotationValue::Other,
                };
                (v, i + 3)
            }
            b's' => (
                AnnotationValue::Str(self.utf8(index).unwrap_or_default().to_string()),
                i + 3,
            ),
            b'c' => (AnnotationValue::Other, i + 3),
            b'e' => (
                AnnotationValue::Enum(self.utf8(u16_at(b, i + 3)?).unwrap_or_default().to_string()),
                i + 5,
            ),
            b'@' => (AnnotationValue::Other, self.annotation(i + 1, depth + 1)?.1),
            b'[' => {
                let mut j = i + 3;
                let mut out = Vec::new();
                for _ in 0..index {
                    let (v, next) = self.annotation_value(j, depth + 1)?;
                    out.push(v);
                    j = next;
                }
                (AnnotationValue::Array(out), j)
            }
            t => {
                return Err(Error::Class(format!(
                    "valor de anotación desconocido: {}",
                    t as char
                )));
            }
        })
    }

    pub fn game_version(&self) -> Option<(u32, u32, String)> {
        let class_file = self.index("zombie/core/GameVersion")?;
        let init = self.index("zombie/core/GameVersion.<init>:(IILjava/lang/String;)V")?;
        let field = self.index("zombie/core/Core.gameVersion:Lzombie/core/GameVersion;")?;
        let b = &self.bytes;
        let integer = |i: usize| -> Option<(u32, usize)> {
            match *b.get(i)? {
                op @ 0x03..=0x08 => Some(((op - 0x03) as u32, 1)),
                0x10 => Some((*b.get(i + 1)? as u32, 2)),
                0x11 => Some((u16_at(b, i + 1).ok()? as u32, 3)),
                _ => None,
            }
        };
        let [c1, c2] = class_file.to_be_bytes();
        for i in 0..b.len().saturating_sub(16) {
            if b[i] != 0xBB || b[i + 1] != c1 || b[i + 2] != c2 || b[i + 3] != 0x59 {
                continue;
            }
            let Some((major, n1)) = integer(i + 4) else {
                continue;
            };
            let Some((minor, n2)) = integer(i + 4 + n1) else {
                continue;
            };
            let j = i + 4 + n1 + n2;
            let (string, n3) = match b.get(j) {
                Some(0x12) => (*b.get(j + 1)? as u16, 2),
                Some(0x13) => (u16_at(b, j + 1).ok()?, 3),
                _ => continue,
            };
            let k = j + n3;
            if b.get(k) == Some(&0xB7)
                && u16_at(b, k + 1).ok() == Some(init)
                && b.get(k + 3) == Some(&0xB3)
                && u16_at(b, k + 4).ok() == Some(field)
            {
                return Some((major, minor, self.text(string).unwrap_or_default()));
            }
        }
        None
    }
}

const MAX_DEPTH: u8 = 32;

const MAX_ENTRY: u64 = 64 << 20;

fn read_limited(e: &mut impl Read, declared: u64, jar: &Path) -> CoreResult<Vec<u8>> {
    let mut v = Vec::with_capacity(declared.min(1 << 20) as usize);
    e.take(MAX_ENTRY + 1)
        .read_to_end(&mut v)
        .map_err(Error::io(jar))?;
    if v.len() as u64 > MAX_ENTRY {
        return Err(Error::Class(format!(
            "{}: una entrada pasa de {} MB",
            jar.display(),
            MAX_ENTRY >> 20
        )));
    }
    Ok(v)
}

pub fn read_entry(jar: &Path, name: &str) -> CoreResult<Option<Vec<u8>>> {
    Ok(read_entries(jar, &[name])?.pop().flatten())
}

pub fn read_entries(jar: &Path, names: &[&str]) -> CoreResult<Vec<Option<Vec<u8>>>> {
    let f = File::open(paths::long(jar)).map_err(Error::io(jar))?;
    let mut z = zip::ZipArchive::new(f).map_err(|source| Error::Zip {
        path: jar.to_path_buf(),
        source,
    })?;
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let mut e = match z.by_name(name) {
            Ok(e) => e,
            Err(zip::result::ZipError::FileNotFound) => {
                out.push(None);
                continue;
            }
            Err(source) => {
                return Err(Error::Zip {
                    path: jar.to_path_buf(),
                    source,
                });
            }
        };
        let size = e.size();
        out.push(Some(read_limited(&mut e, size, jar)?));
    }
    Ok(out)
}

pub fn read_classes(jar: &Path, prefix: &str) -> CoreResult<Vec<(String, Vec<u8>)>> {
    let f = File::open(paths::long(jar)).map_err(Error::io(jar))?;
    let mut z = zip::ZipArchive::new(f).map_err(|source| Error::Zip {
        path: jar.to_path_buf(),
        source,
    })?;
    let mut out = Vec::new();
    for k in 0..z.len() {
        let mut e = z.by_index(k).map_err(|source| Error::Zip {
            path: jar.to_path_buf(),
            source,
        })?;
        let Ok(name) = e.name().map(|n| n.into_owned()) else {
            continue;
        };
        if !name.starts_with(prefix) || !name.ends_with(".class") {
            continue;
        }
        let size = e.size();
        out.push((name, read_limited(&mut e, size, jar)?));
    }
    Ok(out)
}

pub fn manifest_attribute(jar: &Path, attribute: &str) -> CoreResult<Option<String>> {
    let Some(b) = read_entry(jar, "META-INF/MANIFEST.MF")? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&b);
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            if let Some(a) = current.as_mut() {
                a.push_str(rest);
            }
            continue;
        }
        if let Some(a) = current.take()
            && let Some(v) = value_attribute(&a, attribute)
        {
            return Ok(Some(v));
        }
        current = Some(line.to_string());
    }
    Ok(current.and_then(|a| value_attribute(&a, attribute)))
}

fn value_attribute(line: &str, attribute: &str) -> Option<String> {
    let (k, v) = line.split_once(':')?;
    k.trim()
        .eq_ignore_ascii_case(attribute)
        .then(|| v.trim().to_string())
}
