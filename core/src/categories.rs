use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::modinfo::normalize_id;
use crate::mods::ScannedMod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModCategory {
    CoreRequirement,
    Tweaks,
    Resource,
    Map,
    Vehicle,
    Code,
    Clothes,
    Ui,
    Other,
    Translation,
    Undefined,
}

impl ModCategory {
    pub fn key(self) -> &'static str {
        match self {
            ModCategory::CoreRequirement => "coreRequirement",
            ModCategory::Tweaks => "tweaks",
            ModCategory::Resource => "resource",
            ModCategory::Map => "map",
            ModCategory::Vehicle => "vehicle",
            ModCategory::Code => "code",
            ModCategory::Clothes => "clothes",
            ModCategory::Ui => "ui",
            ModCategory::Other => "other",
            ModCategory::Translation => "translation",
            ModCategory::Undefined => "undefined",
        }
    }

    pub fn parse(s: &str) -> Option<ModCategory> {
        [
            ModCategory::CoreRequirement,
            ModCategory::Tweaks,
            ModCategory::Resource,
            ModCategory::Map,
            ModCategory::Vehicle,
            ModCategory::Code,
            ModCategory::Clothes,
            ModCategory::Ui,
            ModCategory::Other,
            ModCategory::Translation,
            ModCategory::Undefined,
        ]
        .into_iter()
        .find(|c| c.key() == s.trim())
    }
}

pub fn from_mod(m: &ScannedMod) -> (ModCategory, bool) {
    if let Some(c) = m
        .info
        .as_ref()
        .and_then(|i| i.category.as_deref())
        .and_then(ModCategory::parse)
        .filter(|c| *c != ModCategory::Undefined)
    {
        return (c, false);
    }
    let name = m
        .info
        .as_ref()
        .and_then(|i| i.name.clone())
        .unwrap_or_default()
        .to_lowercase();
    let is_tweak = [
        "framework",
        " api",
        "_api",
        "tweak",
        "interface",
        "utilit",
        "bugfix",
    ]
    .iter()
    .any(|k| name.contains(k));
    let paths = m.relative_paths();
    let has = |p: &str| {
        let i = paths.partition_point(|r| r.as_str() < p);
        paths.get(i).is_some_and(|r| r.starts_with(p))
    };
    let models = has("media/models_x/") || has("media/models/");
    let textures = has("media/textures/") || has("media/texturepacks/");
    let vehicle_models = has("media/models_x/vehicles/") || has("media/models/vehicles/");
    let code = has("media/lua/client/")
        || has("media/lua/server/")
        || has("media/scripts/")
        || has("media/shared/");
    let skinned = has("media/models_x/skinned/") && has("media/textures/");
    let ui = has("media/textures/ui/") && has("media/ui/");
    let translation = has("media/lua/shared/translate/");
    let resource = has("media/resource/");
    let map = !m.maps().is_empty();

    let mut c = ModCategory::Undefined;
    let mut apply = |cat: ModCategory, yes: bool| {
        if yes && c > cat {
            c = cat;
        }
    };
    apply(
        ModCategory::Translation,
        translation && !(code || models || textures),
    );
    apply(ModCategory::Ui, ui);
    apply(ModCategory::Clothes, skinned);
    apply(
        ModCategory::Code,
        code && !(models || textures || ui || resource),
    );
    apply(ModCategory::Tweaks, is_tweak);
    apply(ModCategory::Vehicle, vehicle_models && textures);
    apply(ModCategory::Map, map);
    apply(
        ModCategory::Resource,
        (textures || resource) && !(code || models || map || ui),
    );
    if c == ModCategory::Undefined {
        c = ModCategory::Other;
    }
    (c, true)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    On,
    Category,
    #[default]
    Off,
}

impl Position {
    fn parse(s: &str) -> Position {
        match s.trim() {
            "true" | "0" | "on" => Position::On,
            "category" => Position::Category,
            _ => Position::Off,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FileRule {
    pub after_of: Vec<String>,
    pub before_of: Vec<String>,
    pub incompatible: Vec<String>,
    pub first: Position,
    pub last: Position,
    pub category: Option<ModCategory>,
}

pub fn rules_file() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| {
        PathBuf::from(h)
            .join("Zomboid")
            .join("Lua")
            .join("sorting_rules.txt")
    })
}

pub fn read_rules(text: &str) -> BTreeMap<String, FileRule> {
    let list = |v: &str| -> Vec<String> {
        v.split(',')
            .map(|x| normalize_id(x.trim()))
            .filter(|x| !x.is_empty())
            .collect()
    };
    let mut out: BTreeMap<String, FileRule> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(id) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            let id = normalize_id(id.trim());
            out.entry(id.clone()).or_default();
            current = Some(id);
            continue;
        }
        let (Some(id), Some((k, v))) = (current.as_ref(), l.split_once('=')) else {
            continue;
        };
        let r = out.entry(id.clone()).or_default();
        match k.trim().to_lowercase().as_str() {
            "loadafter" | "loadmodafter" => r.after_of.extend(list(v)),
            "loadbefore" | "loadmodbefore" => r.before_of.extend(list(v)),
            "incompatiblemods" | "incompatible" => r.incompatible.extend(list(v)),
            "loadfirst" => r.first = Position::parse(v),
            "loadlast" => r.last = Position::parse(v),
            "category" => r.category = ModCategory::parse(v),
            _ => {}
        }
    }
    out
}

pub const FIRST: [&str; 3] = ["ModManager", "ModManagerServer", "modoptions"];

pub struct OrderData<'a> {
    pub id: String,
    pub category: ModCategory,
    pub rule: Option<&'a FileRule>,
}

pub fn ranges(data: &[OrderData]) -> Vec<usize> {
    let pre = |id: &str| {
        FIRST
            .iter()
            .position(|p| p.eq_ignore_ascii_case(id))
            .unwrap_or(usize::MAX)
    };
    let mut idx: Vec<usize> = (0..data.len()).collect();
    idx.sort_by(|&a, &b| {
        let (x, y) = (&data[a], &data[b]);
        let (px, py) = (
            x.rule.map(|r| r.first).unwrap_or_default(),
            y.rule.map(|r| r.first).unwrap_or_default(),
        );
        let (ux, uy) = (
            x.rule.map(|r| r.last).unwrap_or_default(),
            y.rule.map(|r| r.last).unwrap_or_default(),
        );
        let group = |p: Position, u: Position| {
            if p == Position::On {
                0
            } else if u == Position::On {
                2
            } else {
                1
            }
        };
        let cat = |d: &OrderData| d.rule.and_then(|r| r.category).unwrap_or(d.category);
        pre(&x.id)
            .cmp(&pre(&y.id))
            .then(group(px, ux).cmp(&group(py, uy)))
            .then(cat(x).cmp(&cat(y)))
            .then(px.cmp(&py))
            .then(uy.cmp(&ux))
            .then(a.cmp(&b))
    });
    let mut out = vec![0; data.len()];
    for (range, i) in idx.into_iter().enumerate() {
        out[i] = range;
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderInfo {
    pub category: ModCategory,
    pub guessed: bool,
    pub first: Position,
    pub last: Position,
}

pub fn plan_order<'a>(
    mods: &[(&'a str, bool, Option<&'a ScannedMod>)],
    rules: &BTreeMap<String, FileRule>,
    accepted: &[crate::profiles::AcceptedOverride],
) -> (crate::order::Options, BTreeMap<String, OrderInfo>) {
    let mut by_mod = BTreeMap::new();
    let mut data = Vec::with_capacity(mods.len());
    let mut extra = Vec::new();
    let mut previous = ModCategory::Undefined;
    for (id, _, m) in mods {
        let n = normalize_id(id);
        let rule = rules.get(&n);
        let (category, guessed) = m.map(from_mod).unwrap_or((previous, true));
        previous = rule.and_then(|r| r.category).unwrap_or(category);
        if let Some(r) = rule {
            extra.extend(r.after_of.iter().map(|a| (a.clone(), n.clone())));
            extra.extend(r.before_of.iter().map(|b| (n.clone(), b.clone())));
        }
        by_mod.insert(
            n.clone(),
            OrderInfo {
                category: rule.and_then(|r| r.category).unwrap_or(category),
                guessed: rule.and_then(|r| r.category).is_none() && guessed,
                first: rule.map(|r| r.first).unwrap_or_default(),
                last: rule.map(|r| r.last).unwrap_or_default(),
            },
        );
        data.push(OrderData {
            id: n,
            category,
            rule,
        });
    }
    let options = crate::order::Options {
        ranges: Some(ranges(&data)),
        extra,
        overrides: accepted
            .iter()
            .map(|a| (normalize_id(&a.loses), normalize_id(&a.wins)))
            .collect(),
    };
    (options, by_mod)
}

pub fn user_rules() -> BTreeMap<String, FileRule> {
    rules_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| read_rules(&t))
        .unwrap_or_default()
}
