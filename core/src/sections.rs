use serde::{Deserialize, Serialize};

use crate::modinfo::normalize_id;
use crate::profiles::{ModEntry, Profile};

const MAX_NAME: usize = 60;

pub fn clean_name(s: &str) -> Option<String> {
    let s = s.trim();
    let n = s.chars().count();
    (n > 0 && n <= MAX_NAME && !s.chars().any(char::is_control)).then(|| s.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub sections: Vec<String>,
    pub mods: Vec<(String, Option<String>)>,
}

type Edit = Result<Layout, String>;

impl Profile {
    pub fn section_index(&self, m: &ModEntry) -> usize {
        m.section
            .as_ref()
            .and_then(|s| self.sections.iter().position(|x| x == s))
            .map_or(0, |i| i + 1)
    }

    pub fn regroup(&mut self) {
        let mut mods = std::mem::take(&mut self.mods);
        mods.sort_by_key(|m| self.section_index(m));
        self.mods = mods;
    }

    pub fn layout(&self) -> Layout {
        Layout {
            sections: self.sections.clone(),
            mods: self
                .mods
                .iter()
                .map(|m| (m.id.clone(), m.section.clone()))
                .collect(),
        }
    }

    pub fn restore_layout(&mut self, l: Layout) {
        let ids: Vec<String> = l.mods.iter().map(|(id, _)| normalize_id(id)).collect();
        for m in &mut self.mods {
            let id = normalize_id(&m.id);
            if let Some(i) = ids.iter().position(|x| *x == id) {
                m.section = l.mods[i].1.clone();
            }
        }
        self.sections = l.sections;
        apply_order(self, &ids);
    }

    fn find_section(&self, name: &str) -> Result<usize, String> {
        self.sections
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| "no existe esa sección".to_string())
    }

    fn new_name(&self, name: &str, except: Option<usize>) -> Result<String, String> {
        let name = clean_name(name).ok_or("nombre de sección no válido")?;
        let low = name.to_lowercase();
        let taken = self
            .sections
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && s.to_lowercase() == low);
        if taken {
            return Err("ya hay una sección con ese nombre".into());
        }
        Ok(name)
    }

    pub fn section_create(&mut self, name: &str, at_mod: Option<&str>) -> Edit {
        let name = self.new_name(name, None)?;
        let prev = self.layout();
        let Some(at) = at_mod else {
            self.sections.push(name);
            return Ok(prev);
        };
        let at = normalize_id(at);
        let i = self
            .mods
            .iter()
            .position(|m| normalize_id(&m.id) == at)
            .ok_or("el mod no está en el perfil")?;
        let k = self.section_index(&self.mods[i]);
        let n = (i..self.mods.len())
            .take_while(|&j| self.section_index(&self.mods[j]) == k)
            .count();
        for m in &mut self.mods[i..i + n] {
            m.section = Some(name.clone());
        }
        self.sections.insert(k, name);
        self.regroup();
        Ok(prev)
    }

    pub fn section_rename(&mut self, old: &str, new: &str) -> Edit {
        let i = self.find_section(old)?;
        let new = self.new_name(new, Some(i))?;
        let prev = self.layout();
        for m in self
            .mods
            .iter_mut()
            .filter(|m| m.section.as_deref() == Some(old))
        {
            m.section = Some(new.clone());
        }
        self.sections[i] = new;
        Ok(prev)
    }

    pub fn section_move(&mut self, name: &str, to: usize) -> Edit {
        let i = self.find_section(name)?;
        let prev = self.layout();
        let s = self.sections.remove(i);
        self.sections.insert(to.min(self.sections.len()), s);
        self.regroup();
        Ok(prev)
    }

    pub fn section_delete(&mut self, name: &str) -> Edit {
        let i = self.find_section(name)?;
        let prev = self.layout();
        let above = i.checked_sub(1).map(|j| self.sections[j].clone());
        for m in self
            .mods
            .iter_mut()
            .filter(|m| m.section.as_deref() == Some(name))
        {
            m.section = above.clone();
        }
        self.sections.remove(i);
        self.regroup();
        Ok(prev)
    }

    pub fn section_assign(&mut self, ids: &[String], name: Option<&str>, first: bool) -> Edit {
        if let Some(n) = name {
            self.find_section(n)?;
        }
        let prev = self.layout();
        let ids: Vec<String> = ids.iter().map(|i| normalize_id(i)).collect();
        let (mut moved, rest): (Vec<ModEntry>, Vec<ModEntry>) = std::mem::take(&mut self.mods)
            .into_iter()
            .partition(|m| ids.contains(&normalize_id(&m.id)));
        for m in &mut moved {
            m.section = name.map(str::to_string);
        }
        self.mods = if first {
            moved.into_iter().chain(rest).collect()
        } else {
            rest.into_iter().chain(moved).collect()
        };
        self.regroup();
        Ok(prev)
    }

    pub fn pull_requirements(&mut self, edges: &[crate::order::Edge]) {
        use crate::order::Rule;
        let mut at: std::collections::BTreeMap<String, usize> = self
            .mods
            .iter()
            .map(|m| (normalize_id(&m.id), self.section_index(m)))
            .collect();
        let mut pulled: std::collections::BTreeMap<String, usize> = Default::default();
        let mut seq = 0;
        let mut changed = true;
        while changed {
            changed = false;
            for a in edges {
                if !matches!(a.rule, Rule::Requires | Rule::ZbFixed) {
                    continue;
                }
                let (Some(&sb), Some(&sa)) = (at.get(&a.before), at.get(&a.after)) else {
                    continue;
                };
                if sb > sa {
                    at.insert(a.before.clone(), sa);
                    seq += 1;
                    pulled.insert(a.before.clone(), seq);
                    changed = true;
                }
            }
        }
        if pulled.is_empty() {
            return;
        }
        let (mut moved, rest): (Vec<ModEntry>, Vec<ModEntry>) = std::mem::take(&mut self.mods)
            .into_iter()
            .partition(|m| pulled.contains_key(&normalize_id(&m.id)));
        for m in &mut moved {
            let k = at[&normalize_id(&m.id)];
            m.section = k.checked_sub(1).map(|i| self.sections[i].clone());
        }
        moved.sort_by_key(|m| std::cmp::Reverse(pulled[&normalize_id(&m.id)]));
        self.mods = moved.into_iter().chain(rest).collect();
        self.regroup();
    }

    pub fn sanitize_sections(&mut self) {
        let mut kept: Vec<String> = Vec::new();
        for s in std::mem::take(&mut self.sections) {
            if let Some(c) = clean_name(&s)
                && !kept.iter().any(|k| k.to_lowercase() == c.to_lowercase())
            {
                kept.push(c);
            }
        }
        for m in &mut self.mods {
            m.section = m
                .section
                .as_deref()
                .and_then(clean_name)
                .filter(|c| kept.contains(c));
        }
        self.sections = kept;
        self.regroup();
    }

    pub fn move_mod(&mut self, from: usize, to: usize, section: Option<&str>) -> Edit {
        if from >= self.mods.len() || to >= self.mods.len() {
            return Err("posición fuera de la lista".into());
        }
        if let Some(n) = section {
            self.find_section(n)?;
        }
        let prev = self.layout();
        let mut m = self.mods.remove(from);
        m.section = section.map(str::to_string);
        self.mods.insert(to, m);
        self.regroup();
        Ok(prev)
    }
}

pub fn sort_sections(
    entries: &[crate::order::Entry],
    options: &crate::order::Options,
    section_of: &[usize],
    only: Option<usize>,
) -> crate::order::OrderResult {
    let mut out = crate::order::OrderResult {
        order: Vec::with_capacity(entries.len()),
        moves: Vec::new(),
        cycles: Vec::new(),
    };
    let mut start = 0;
    while start < entries.len() {
        let k = section_of[start];
        let end = start + section_of[start..].iter().take_while(|&&x| x == k).count();
        let part = &entries[start..end];
        if only.is_some_and(|o| o != k) {
            out.order.extend(part.iter().map(|e| normalize_id(&e.id)));
        } else {
            let o = crate::order::Options {
                ranges: options.ranges.as_ref().map(|r| r[start..end].to_vec()),
                extra: options.extra.clone(),
                overrides: options.overrides.clone(),
            };
            let r = crate::order::sort_with(part, &o);
            out.order.extend(r.order);
            out.moves.extend(r.moves.into_iter().map(|mut m| {
                m.from += start;
                m.to += start;
                m
            }));
            out.cycles.extend(r.cycles);
        }
        start = end;
    }
    out
}

pub fn apply_order(p: &mut Profile, order: &[String]) {
    let mut rest = std::mem::take(&mut p.mods);
    let mut out = Vec::with_capacity(rest.len());
    for id in order {
        if let Some(i) = rest.iter().position(|m| normalize_id(&m.id) == *id) {
            out.push(rest.remove(i));
        }
    }
    out.append(&mut rest);
    p.mods = out;
    p.regroup();
}
