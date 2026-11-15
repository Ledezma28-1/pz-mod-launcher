use std::fs;
use std::path::Path;

use crate::{CoreResult, Error};

pub const RESET_TEXT: &str =
    "If this file does not exist, default.txt will be reset to empty (no mods active).";
pub const RESET_FILE: &str = "reset-mods-42_00.txt";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModList {
    pub mods: Vec<String>,
    pub maps: Vec<String>,
}

impl ModList {
    pub fn read(text: &str) -> ModList {
        let mut l = ModList::default();
        let mut block = "";
        for line in text.lines() {
            let t = line.trim();
            match t {
                "mods" | "maps" => block = if t == "mods" { "mods" } else { "maps" },
                "{" => {}
                "}" => block = "",
                _ => {
                    let Some((k, v)) = t.split_once('=') else {
                        continue;
                    };
                    let v = v.trim().trim_end_matches(',').trim().to_string();
                    match (block, k.trim()) {
                        ("mods", "mod") if !v.is_empty() => l.mods.push(v),
                        ("maps", "map") if !v.is_empty() => l.maps.push(v),
                        _ => {}
                    }
                }
            }
        }
        l
    }

    pub fn read_file(path: &Path) -> CoreResult<ModList> {
        Ok(ModList::read(
            &fs::read_to_string(path).map_err(Error::io(path))?,
        ))
    }

    pub fn text(&self) -> String {
        let mut s = String::from("VERSION = 1,\n\nmods\n{\n");
        for m in &self.mods {
            s.push_str(&format!("    mod = {m},\n"));
        }
        s.push_str("}\n\nmaps\n{\n");
        for m in &self.maps {
            s.push_str(&format!("    map = {m},\n"));
        }
        s.push_str("}\n");
        s
    }
}
