use crate::{CoreResult, Error};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Block(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Block(children) => children
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Text(_) => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Value> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            Value::Block(_) => None,
        }
    }

    pub fn children(&self) -> &[(String, Value)] {
        match self {
            Value::Block(h) => h,
            Value::Text(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Card {
    Str(String),
    Opens,
    Closes,
}

fn cards(text: &str) -> CoreResult<Vec<Card>> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' => out.push(Card::Opens),
            '}' => out.push(Card::Closes),
            '"' => {
                let mut s = String::new();
                loop {
                    match it.next() {
                        None => return Err(Error::Vdf("cadena sin cerrar".into())),
                        Some('"') => break,
                        Some('\\') => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => return Err(Error::Vdf("escape al final".into())),
                        },
                        Some(o) => s.push(o),
                    }
                }
                out.push(Card::Str(s));
            }
            '/' if it.peek() == Some(&'/') => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                let mut s = String::from(c);
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    it.next();
                }
                out.push(Card::Str(s));
            }
        }
    }
    Ok(out)
}

fn block(f: &[Card], i: &mut usize, root: bool) -> CoreResult<Vec<(String, Value)>> {
    let mut children = Vec::new();
    while *i < f.len() {
        match &f[*i] {
            Card::Closes if !root => {
                *i += 1;
                return Ok(children);
            }
            Card::Closes => return Err(Error::Vdf("llave de cierre de más".into())),
            Card::Opens => return Err(Error::Vdf("bloque sin clave".into())),
            Card::Str(k) => {
                *i += 1;
                match f.get(*i) {
                    Some(Card::Str(v)) => {
                        children.push((k.clone(), Value::Text(v.clone())));
                        *i += 1;
                    }
                    Some(Card::Opens) => {
                        *i += 1;
                        children.push((k.clone(), Value::Block(block(f, i, false)?)));
                    }
                    _ => return Err(Error::Vdf(format!("clave «{k}» sin valor"))),
                }
            }
        }
    }
    if root {
        Ok(children)
    } else {
        Err(Error::Vdf("bloque sin cerrar".into()))
    }
}

pub fn read(text: &str) -> CoreResult<Value> {
    let f = cards(text.trim_start_matches('\u{feff}'))?;
    let mut i = 0;
    Ok(Value::Block(block(&f, &mut i, true)?))
}
