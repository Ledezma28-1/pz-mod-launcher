use crate::{CoreResult, Error};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const ID: &str = "pzmm_probe";
pub const NAMES: &str = "pzmm-probe-names.txt";
pub const OUT: &str = "pzmm-probe.txt";
const LIST: &str = "pzmm-probe-list.txt";

const MOD_INFO: &str = "name=PZ Mod Manager probe\nid=pzmm_probe\nauthor=PZ Mod Manager\n\
description=Writes which file defined some global Lua functions (only in PZ Mod Manager profiles).\n";

const LUA: &str = r#"-- PZ Mod Manager probe: for each global function listed in pzmm-probe-names.txt, writes the
-- file and first line of the definition that is alive. Reads and writes only in <cache>/Lua.
local function resolve(name)
    local v = _G
    for part in string.gmatch(name, "[^%.]+") do
        if type(v) ~= "table" then return nil end
        v = v[part]
    end
    return v
end

local function dump(phase)
    local r = getFileReader("pzmm-probe-names.txt", false)
    if not r then return end
    local w = getFileWriter("pzmm-probe.txt", true, phase ~= "boot")
    local name = r:readLine()
    while name do
        local ok, f = pcall(resolve, name)
        if ok and type(f) == "function" then
            local ok2, file = pcall(getFilenameOfClosure, f)
            local ok3, line = pcall(getFirstLineOfClosure, f)
            if ok2 and file then
                w:write(phase .. "\t" .. name .. "\t" .. tostring(file) .. "\t" .. tostring(ok3 and line or 0) .. "\n")
            end
        end
        name = r:readLine()
    end
    r:close()
    w:close()
end

Events.OnGameBoot.Add(function() dump("boot") end)
Events.OnGameStart.Add(function() dump("start") end)
"#;

fn folder(cache: &Path) -> PathBuf {
    cache.join("mods").join(ID)
}

pub fn install(cache: &Path, names: &[String], mods: &str) -> CoreResult<()> {
    let root = folder(cache);
    let files = [
        (root.join("common").join("mod.info"), MOD_INFO),
        (root.join("42").join("mod.info"), MOD_INFO),
        (
            root.join("42")
                .join("media")
                .join("lua")
                .join("client")
                .join("zzzz_pzmm_probe.lua"),
            LUA,
        ),
    ];
    for (path, text) in files {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).map_err(Error::io(p))?;
        }
        fs::write(&path, text).map_err(Error::io(&path))?;
    }
    let lua = cache.join("Lua");
    fs::create_dir_all(&lua).map_err(Error::io(&lua))?;
    let list = lua.join(NAMES);
    fs::write(&list, names.join("\n") + "\n").map_err(Error::io(&list))?;
    let launched = lua.join(LIST);
    fs::write(&launched, mods).map_err(Error::io(&launched))?;
    let out = lua.join(OUT);
    if out.is_file() {
        fs::remove_file(&out).map_err(Error::io(&out))?;
    }
    Ok(())
}

pub fn remove(cache: &Path) -> CoreResult<()> {
    let root = folder(cache);
    if root.is_dir() && !crate::links::is_link(&root) {
        fs::remove_dir_all(&root).map_err(Error::io(&root))?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Seen {
    pub phase: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

pub fn read(cache: &Path) -> Vec<Seen> {
    let Ok(text) = fs::read_to_string(cache.join("Lua").join(OUT)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut p = l.split('\t');
            Some(Seen {
                phase: p.next()?.to_string(),
                name: p.next()?.to_string(),
                file: p.next()?.to_string(),
                line: p.next()?.trim().parse().unwrap_or(0),
            })
        })
        .collect()
}

pub fn same_list(cache: &Path, list: &str) -> bool {
    fs::read_to_string(cache.join("Lua").join(LIST)).is_ok_and(|l| l == list)
}

pub fn asked(cache: &Path) -> Vec<String> {
    fs::read_to_string(cache.join("Lua").join(NAMES))
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mismatch {
    pub name: String,
    pub predicted: String,
    pub seen: Option<String>,
}

pub fn check(predicted: &[(String, String)], seen: &[Seen], phase: &str) -> Vec<Mismatch> {
    predicted
        .iter()
        .filter_map(|(name, file)| {
            let s = seen.iter().find(|s| s.phase == phase && s.name == *name);
            let base = |f: &str| f.rsplit(['/', '\\']).next().unwrap_or(f).to_string();
            let same = s.is_some_and(|s| base(&s.file).eq_ignore_ascii_case(file));
            (!same).then(|| Mismatch {
                name: name.clone(),
                predicted: file.clone(),
                seen: s.map(|s| s.file.clone()),
            })
        })
        .collect()
}
