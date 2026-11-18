use std::path::Path;

use base64::Engine;
use pzmm_core::conflicts::{Finding, ProfileEntry, Severity};
use pzmm_core::game::{Game, Modification};
use pzmm_core::mods::{Layout, ScannedMod, Source};
use pzmm_core::order::{self, OrderResult};
use pzmm_core::profiles::Profile;
use pzmm_core::{PZ_APP_ID, steam, zb};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct GameInfo {
    pub folder: String,
    pub version: String,
    pub version_text: String,
    pub build: Option<String>,
    pub branch: Option<String>,
    pub java: Option<String>,
}

#[derive(Serialize)]
pub struct ZbWorkshopCopy {
    pub channel: String,
    pub id: u64,
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct ZbInfo {
    pub in_game: Option<String>,
    pub dll: bool,
    pub workshop: Vec<ZbWorkshopCopy>,
    pub ref_json: bool,
    pub ref_bat: bool,
    pub ref_steam: Vec<String>,
    pub warning: Option<String>,
}

#[derive(Serialize)]
pub struct Environment {
    pub game: GameInfo,
    pub steam: Option<String>,
    pub zb: ZbInfo,
    pub modifications: Vec<String>,
}

impl Environment {
    pub fn of(j: &Game, steam_dir: Option<&Path>) -> Environment {
        let in_game = zb::jar_version(&j.folder.join("ZombieBuddy.jar"));
        let workshop = j
            .library
            .as_ref()
            .map(|b| zb::workshop_copies(&steam::workshop_content_folder(b, PZ_APP_ID)))
            .unwrap_or_default()
            .into_iter()
            .map(|c| ZbWorkshopCopy {
                channel: c.channel.to_string(),
                id: c.workshop_id,
                version: c.version,
            })
            .collect();
        let r = zb::references(&j.folder, steam_dir);
        let warning = match (&j.version, &in_game) {
            (Some(v), Some(z)) => zb::incompatible_with_game(v.version, z).map(str::to_string),
            _ => None,
        };
        Environment {
            game: GameInfo {
                folder: j.folder.display().to_string(),
                version: j
                    .version
                    .as_ref()
                    .map(|v| v.version.to_string())
                    .unwrap_or_default(),
                version_text: j.version.as_ref().map(|v| v.text()).unwrap_or_default(),
                build: j.manifest.as_ref().map(|m| m.buildid.clone()),
                branch: j.manifest.as_ref().and_then(|m| m.branch.clone()),
                java: j.java.clone(),
            },
            steam: steam_dir.map(|s| s.display().to_string()),
            zb: ZbInfo {
                in_game,
                dll: j.folder.join("zbNative.dll").is_file(),
                workshop,
                ref_json: r.json,
                ref_bat: r.bat,
                ref_steam: r.steam,
                warning,
            },
            modifications: j
                .modifications
                .iter()
                .filter_map(|m| match m {
                    Modification::Pzopt { files, revision } => Some(format!(
                        "pzopt: {files} archivos (revisión {})",
                        revision.as_deref().unwrap_or("?")
                    )),
                    Modification::LooseClasses { folder, files } => {
                        Some(format!("{files} clases sueltas en {folder}\\"))
                    }
                    Modification::ArgJson { arg } => Some(format!("ProjectZomboid64.json: {arg}")),
                    Modification::BatOptions { value } => {
                        Some(format!("ProjectZomboid64.bat: {value}"))
                    }
                    Modification::ZombieBuddy { .. } => None,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub active: usize,
    pub total: usize,
    pub last_use: Option<u64>,
    pub sections: Vec<String>,
}

impl ProfileSummary {
    pub fn of(p: &Profile) -> ProfileSummary {
        ProfileSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            active: p.mods.iter().filter(|m| m.enabled).count(),
            total: p.mods.len(),
            last_use: p.last_use,
            sections: p.sections.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ProfileSettings {
    pub xmx: String,
    pub game_extra: String,
    pub policy: String,
    pub use_zb: String,
    #[serde(default)]
    pub probe: bool,
}

#[derive(Serialize)]
pub struct ProbeReport {
    pub phase: String,
    pub total: usize,
    pub mismatches: Vec<pzmm_core::probe::Mismatch>,
}

fn kind_of(m: &ScannedMod) -> &'static str {
    let Some(i) = &m.info else { return "lua" };
    if i.is_java() {
        return "java";
    }
    let cat = i.category.as_deref().unwrap_or("").to_ascii_lowercase();
    if cat.contains("map") {
        "map"
    } else if cat.contains("vehicle") {
        "vehicle"
    } else {
        "lua"
    }
}

fn source_of(m: &ScannedMod) -> (&'static str, Option<u64>) {
    match m.source {
        Source::Workshop { id } => ("workshop", Some(id)),
        Source::Local => ("local", None),
    }
}

fn severity_txt(g: Severity) -> &'static str {
    match g {
        Severity::Blocks => "blocks",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Detail => "detail",
    }
}

#[derive(Serialize)]
pub struct Row {
    pub index: usize,
    pub id: String,
    pub key: String,
    pub name: String,
    pub enabled: bool,
    pub pinned: bool,
    pub found: bool,
    pub kind: String,
    pub version_mod: Option<String>,
    pub source: String,
    pub workshop_id: Option<u64>,
    pub author: Option<String>,
    pub severity: Option<String>,
    pub section: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeDto {
    pub before: String,
    pub after: String,
    pub rule: String,
    pub is_hard: bool,
    pub broken: bool,
}

#[derive(Serialize, Clone)]
pub struct Count {
    pub active: usize,
    pub blocks: usize,
    pub warning: usize,
    pub info: usize,
    pub detail: usize,
}

impl Count {
    pub fn of(p: &Profile, findings: &[Finding]) -> Count {
        let count = |g: Severity| findings.iter().filter(|h| h.severity == g).count();
        Count {
            active: p.mods.iter().filter(|m| m.enabled).count(),
            blocks: count(Severity::Blocks),
            warning: count(Severity::Warning),
            info: count(Severity::Info),
            detail: count(Severity::Detail),
        }
    }
}

#[derive(Serialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub rows: Vec<Row>,
    pub findings: Vec<Finding>,
    pub edges: Vec<EdgeDto>,
    pub settings: ProfileSettings,
    pub counts: Count,
    pub sections: Vec<String>,
}

impl ProfileView {
    pub fn of(p: &Profile, resolved: &[ProfileEntry], findings: Vec<Finding>) -> ProfileView {
        let worst = |id: &str| {
            findings
                .iter()
                .filter(|h| h.severity != Severity::Detail && h.mods.iter().any(|m| m == id))
                .map(|h| h.severity)
                .min()
                .map(severity_txt)
        };
        let rows = resolved
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (source, workshop_id) = e.found.map(source_of).unwrap_or(("?", None));
                let info = e.found.and_then(|m| m.info.as_ref());
                Row {
                    index: i,
                    id: e.id.clone(),
                    key: e.found.map(|m| m.key.clone()).unwrap_or_default(),
                    name: info
                        .and_then(|x| x.name.clone())
                        .unwrap_or_else(|| e.id.clone()),
                    enabled: e.enabled,
                    pinned: e.pinned,
                    found: e.found.is_some(),
                    kind: e.found.map(kind_of).unwrap_or("lua").to_string(),
                    version_mod: info.and_then(|x| x.version_mod.clone()),
                    source: source.to_string(),
                    workshop_id,
                    author: info.and_then(|x| x.author.clone()),
                    severity: worst(&e.id).map(str::to_string),
                    section: p
                        .mods
                        .get(i)
                        .filter(|m| p.section_index(m) > 0)
                        .and_then(|m| m.section.clone()),
                }
            })
            .collect();
        let entries: Vec<order::Entry> = resolved
            .iter()
            .map(|x| order::Entry {
                id: x.id.clone(),
                enabled: x.enabled,
                pinned: x.pinned,
                info: x.found.and_then(|m| m.info.as_ref()),
            })
            .collect();
        let edges = order::edges(&entries);
        let current: Vec<String> = resolved
            .iter()
            .filter(|e| e.enabled)
            .map(|e| e.id.clone())
            .collect();
        let unmet = order::unmet_edges(&current, &edges);
        let edges = edges
            .into_iter()
            .filter(|a| a.rule != order::Rule::ZbFixed || unmet.contains(a))
            .map(|a| EdgeDto {
                broken: unmet.contains(&a),
                is_hard: a.rule.is_hard(),
                rule: format!("{:?}", a.rule),
                before: a.before,
                after: a.after,
            })
            .collect();
        ProfileView {
            id: p.id.clone(),
            name: p.name.clone(),
            sections: p.sections.clone(),
            counts: Count::of(p, &findings),
            rows,
            findings,
            edges,
            settings: ProfileSettings {
                xmx: p.launch.xmx.clone(),
                game_extra: p.launch.game_extra.join(" "),
                policy: p.zombiebuddy.policy.clone(),
                use_zb: p.zombiebuddy.usage.clone(),
                probe: p.launch.probe,
            },
        }
    }
}

#[derive(Serialize)]
pub struct ReasonDto {
    pub before: String,
    pub after: String,
    pub rule: String,
}

#[derive(Serialize)]
pub struct MoveDto {
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub reasons: Vec<ReasonDto>,
    pub category: Option<String>,
    pub guessed: bool,
    pub position: Option<String>,
    pub effect: order::Effect,
}

#[derive(Serialize)]
pub struct CycleDto {
    pub mods: Vec<String>,
    pub hard: bool,
}

#[derive(Serialize)]
pub struct OrderProposal {
    pub order: Vec<String>,
    pub moves: Vec<MoveDto>,
    pub cycles: Vec<CycleDto>,
    pub rules_file: usize,
}

impl OrderProposal {
    pub fn of(
        r: &OrderResult,
        info: &std::collections::BTreeMap<String, pzmm_core::categories::OrderInfo>,
        rules_file: usize,
    ) -> OrderProposal {
        use pzmm_core::categories::Position;
        OrderProposal {
            rules_file,
            order: r.order.clone(),
            moves: r
                .moves
                .iter()
                .map(|m| MoveDto {
                    category: info.get(&m.id).map(|i| i.category.key().to_string()),
                    guessed: info.get(&m.id).is_some_and(|i| i.guessed),
                    position: info.get(&m.id).and_then(|i| match (i.first, i.last) {
                        (Position::On, _) => Some("loadFirst".to_string()),
                        (_, Position::On) => Some("loadLast".to_string()),
                        (Position::Category, _) => Some("loadFirstCategory".to_string()),
                        (_, Position::Category) => Some("loadLastCategory".to_string()),
                        _ => None,
                    }),
                    id: m.id.clone(),
                    from: m.from,
                    to: m.to,
                    effect: m.effect,
                    reasons: m
                        .reasons
                        .iter()
                        .filter(|a| a.rule != order::Rule::ZbFixed || m.reasons.len() == 1)
                        .map(|a| ReasonDto {
                            before: a.before.clone(),
                            after: a.after.clone(),
                            rule: format!("{:?}", a.rule),
                        })
                        .collect(),
                })
                .collect(),
            cycles: r
                .cycles
                .iter()
                .map(|c| CycleDto {
                    mods: c.mods.clone(),
                    hard: c.hard,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct LibraryMod {
    pub key: String,
    pub id: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub source: String,
    pub workshop_id: Option<u64>,
    pub kind: String,
    pub version_mod: Option<String>,
    pub error: Option<String>,
    pub pending: bool,
}

impl LibraryMod {
    pub fn of(m: &ScannedMod, pending_ids: &std::collections::BTreeSet<u64>) -> LibraryMod {
        let (source, workshop_id) = source_of(m);
        let info = m.info.as_ref();
        LibraryMod {
            key: m.key.clone(),
            id: m.id().map(str::to_string),
            name: info
                .and_then(|i| i.name.clone())
                .or_else(|| m.id().map(str::to_string))
                .unwrap_or_else(|| m.key.clone()),
            author: info.and_then(|i| i.author.clone()),
            source: source.to_string(),
            workshop_id,
            kind: kind_of(m).to_string(),
            version_mod: info.and_then(|i| i.version_mod.clone()),
            error: m.error.clone(),
            pending: workshop_id.is_some_and(|id| pending_ids.contains(&id)),
        }
    }
}

#[derive(Serialize)]
pub struct CandidateDto {
    pub jar: String,
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Serialize)]
pub struct JavaDto {
    pub pkg: Option<String>,
    pub candidates: Vec<CandidateDto>,
    pub chosen: Option<String>,
    pub patches: Vec<zb::JavaPatch>,
}

#[derive(Serialize)]
pub struct ModDetails {
    pub key: String,
    pub id: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub poster: Option<String>,
    pub folder: String,
    pub folder_version: Option<String>,
    pub versions: Vec<String>,
    pub requires: Vec<String>,
    pub incompatible: Vec<String>,
    pub load_before: Vec<String>,
    pub load_after: Vec<String>,
    pub version_min: Option<String>,
    pub version_max: Option<String>,
    pub workshop_id: Option<u64>,
    pub java: Option<JavaDto>,
    pub category: String,
    pub guessed_category: bool,
}

pub fn clear_description(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        match rest[i..].find('>') {
            Some(j) => {
                let tag = rest[i + 1..i + j].trim().to_ascii_uppercase();
                if tag == "LINE" || tag == "BR" {
                    out.push('\n');
                }
                rest = &rest[i + j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn image_in_base64(path: &Path) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    if md.len() > 3 * 1024 * 1024 {
        return None;
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return None,
    };
    let data = std::fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(data)
    ))
}

impl ModDetails {
    pub fn of(m: &ScannedMod, zb_version: Option<&str>) -> ModDetails {
        let info = m.info.as_ref();
        let (_, workshop_id) = source_of(m);
        let (folder_version, versions) = match &m.layout {
            Layout::B42 {
                folder_version,
                versions,
                ..
            } => (folder_version.clone(), versions.clone()),
            _ => (None, Vec::new()),
        };
        let poster = info.and_then(|i| i.posters.first()).and_then(|p| {
            m.roots()
                .iter()
                .rev()
                .map(|r| r.join(p))
                .find(|r| r.is_file())
                .and_then(|r| image_in_base64(&r))
        });
        let java = info
            .and_then(|i| i.java.as_ref())
            .filter(|j| !j.candidates.is_empty())
            .map(|j| JavaDto {
                pkg: j.pkg.clone(),
                candidates: j
                    .candidates
                    .iter()
                    .map(|c| CandidateDto {
                        jar: c.jar.clone(),
                        min: c.zb_min.clone(),
                        max: c.zb_max.clone(),
                    })
                    .collect(),
                chosen: zb_version.and_then(|v| match zb::pick_jar(info.expect("tiene java"), v) {
                    Some(zb::JarChoice::Chosen(c)) => Some(c.jar),
                    _ => None,
                }),
                patches: zb::mod_patches(m, zb_version.unwrap_or_default()).unwrap_or_default(),
            });
        let (category, guessed_category) = pzmm_core::categories::from_mod(m);
        ModDetails {
            category: category.key().to_string(),
            guessed_category,
            key: m.key.clone(),
            id: m.id().map(str::to_string),
            name: info
                .and_then(|i| i.name.clone())
                .unwrap_or_else(|| m.key.clone()),
            author: info.and_then(|i| i.author.clone()),
            description: info
                .and_then(|i| i.description.as_deref())
                .map(clear_description),
            url: info.and_then(|i| i.url.clone()),
            poster,
            folder: m.folder.display().to_string(),
            folder_version,
            versions,
            requires: info.map(|i| i.requires.clone()).unwrap_or_default(),
            incompatible: info.map(|i| i.incompatible.clone()).unwrap_or_default(),
            load_before: info.map(|i| i.load_before.clone()).unwrap_or_default(),
            load_after: info.map(|i| i.load_after.clone()).unwrap_or_default(),
            version_min: info.and_then(|i| i.version_min.clone()),
            version_max: info.and_then(|i| i.version_max.clone()),
            workshop_id,
            java,
        }
    }
}

#[derive(Serialize)]
pub struct SessionState {
    pub profile: String,
    pub seconds: u64,
    pub requested: usize,
    pub loaded: usize,
    pub finished: bool,
    pub code: Option<i32>,
    pub recent_lines: Vec<String>,
    pub integrity: Option<Integrity>,
    pub integrity_skipped: bool,
    pub diagnosis: Option<Diagnosis>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Diagnosis {
    pub end: pzmm_core::diagnosis::EndKind,
    pub code: Option<i32>,
    pub crash: Option<pzmm_core::diagnosis::JavaCrash>,
    pub not_loaded: Vec<NotLoadedDto>,
    pub suspects: Vec<pzmm_core::diagnosis::Suspect>,
    pub errors: usize,
    pub when: u64,
    #[serde(default)]
    pub report: String,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub game_changed: Option<(Option<String>, Option<String>)>,
    #[serde(default)]
    pub zb_changed: Option<(Option<String>, Option<String>)>,
    #[serde(default)]
    pub prediction: Option<pzmm_core::conflicts::PredictionCheck>,
}

#[derive(Serialize)]
pub struct FoldersInfo {
    pub game: Option<String>,
    pub profiles: String,
    pub backups: String,
    pub backups_exists: bool,
    pub backups_bytes: u64,
}

#[derive(Serialize)]
pub struct SteamProfileView {
    pub steamid: String,
    pub name: String,
    pub minutes: Option<u64>,
    pub last_played: Option<i64>,
    pub avatar: Option<String>,
}

#[derive(Serialize)]
pub struct MaintenanceInfo {
    pub webview_bytes: u64,
    pub webview_pending: bool,
    pub workshop_bytes: u64,
    pub logs_bytes: u64,
}

#[derive(Serialize)]
pub struct BisectView {
    pub profile: String,
    pub origin: String,
    pub suspects: usize,
    pub testing: Vec<String>,
    pub round: u32,
    pub rounds_left: u32,
    pub outcome: Option<pzmm_core::bisect::Outcome>,
    pub mode: pzmm_core::bisect::Mode,
    pub flaky: bool,
    pub suggestion: Option<pzmm_core::bisect::Answer>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NotLoadedDto {
    pub id: String,
    pub reason: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct Integrity {
    pub total: usize,
    pub examples: Vec<String>,
}

#[derive(Serialize)]
pub struct Backup {
    pub name: String,
    pub reason: String,
    pub created: u64,
    pub files: usize,
}

impl Backup {
    pub fn of(dir: &Path) -> Option<Backup> {
        let m = pzmm_core::backups::read(dir).ok()?;
        Some(Backup {
            name: dir.file_name()?.to_string_lossy().to_string(),
            reason: m.reason,
            created: m.created,
            files: m.files.len(),
        })
    }
}

#[derive(Serialize)]
pub struct WorkshopItem {
    pub id: u64,
    pub title: String,
    pub summary: String,
    pub collection: bool,
    pub author: Option<String>,
    pub updated: u32,
    pub tags: Vec<String>,
    pub size: i32,
    pub votes_up: u32,
    pub votes_down: u32,
    pub subscribers: u64,
    pub preview_view: String,
    pub children: Vec<u64>,
    pub state: pzmm_core::steamworks::LocalState,
}

impl WorkshopItem {
    pub fn of(e: pzmm_core::steamworks::Item, state: pzmm_core::steamworks::LocalState) -> Self {
        let text = pzmm_core::steamworks::bbcode_to_text(&e.description);
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let summary: String = line.chars().take(220).collect();
        WorkshopItem {
            id: e.id,
            title: e.title,
            summary,
            collection: e.collection,
            author: e.author,
            updated: e.updated,
            tags: e.tags,
            size: e.size,
            votes_up: e.votes_up,
            votes_down: e.votes_down,
            subscribers: e.subscribers,
            preview_view: e.preview_view,
            children: e.children,
            state,
        }
    }
}

#[derive(Serialize)]
pub struct WorkshopPage {
    pub items: Vec<WorkshopItem>,
    pub total: u32,
    pub page: u32,
    pub from_cache: bool,
}

impl WorkshopPage {
    pub fn of(
        p: pzmm_core::steamworks::Page,
        state: impl Fn(u64) -> pzmm_core::steamworks::LocalState,
    ) -> Self {
        WorkshopPage {
            items: p
                .items
                .into_iter()
                .map(|e| {
                    let est = state(e.id);
                    WorkshopItem::of(e, est)
                })
                .collect(),
            total: p.total,
            page: p.page,
            from_cache: p.from_cache,
        }
    }
}

#[derive(Serialize)]
pub struct WorkshopDetails {
    #[serde(flatten)]
    pub item: WorkshopItem,
    pub description: String,
    pub bbcode: String,
    pub created: u32,
}

impl WorkshopDetails {
    pub fn of(e: pzmm_core::steamworks::Item, state: pzmm_core::steamworks::LocalState) -> Self {
        let description = pzmm_core::steamworks::bbcode_to_text(&e.description);
        let bbcode = e.description.clone();
        let created = e.created;
        WorkshopDetails {
            item: WorkshopItem::of(e, state),
            description,
            bbcode,
            created,
        }
    }
}

#[derive(Serialize)]
pub struct WorkshopCollection {
    pub collection: WorkshopDetails,
    pub items: Vec<WorkshopItem>,
    pub missing: Vec<u64>,
}

#[derive(Serialize)]
pub struct ExportRow {
    pub slot: usize,
    pub name: String,
    pub id: String,
    pub workshop_id: Option<u64>,
    pub enabled: bool,
    pub found: bool,
}

#[derive(Serialize)]
pub struct ExportMap {
    pub folder: String,
    pub mod_name: Option<String>,
}

#[derive(Serialize)]
pub struct Export {
    pub mods: Vec<ExportRow>,
    pub maps: Vec<ExportMap>,
    pub saved_maps: bool,
    pub lost_maps: Vec<String>,
    pub default_txt: String,
}

impl Export {
    pub fn of(p: &Profile, resolved: &[ProfileEntry]) -> Export {
        let mods: Vec<ExportRow> = resolved
            .iter()
            .enumerate()
            .map(|(i, e)| ExportRow {
                slot: i + 1,
                name: e
                    .found
                    .and_then(|m| m.info.as_ref())
                    .and_then(|x| x.name.clone())
                    .unwrap_or_else(|| e.id.clone()),
                id: e.id.clone(),
                workshop_id: e.found.and_then(|m| match m.source {
                    Source::Workshop { id } => Some(id),
                    _ => None,
                }),
                enabled: e.enabled,
                found: e.found.is_some(),
            })
            .collect();
        let mut detected: Vec<(String, String)> = Vec::new();
        for e in resolved.iter().filter(|e| e.enabled) {
            let Some(m) = e.found else { continue };
            let name = m
                .info
                .as_ref()
                .and_then(|x| x.name.clone())
                .unwrap_or_else(|| e.id.clone());
            for folder in m.maps() {
                if !detected.iter().any(|(c, _)| *c == folder) {
                    detected.push((folder, name.clone()));
                }
            }
        }
        let folders: Vec<String> = detected.iter().map(|(c, _)| c.clone()).collect();
        let order = pzmm_core::profiles::order_maps(&p.maps, &folders);
        let maps = order
            .into_iter()
            .map(|c| ExportMap {
                mod_name: detected
                    .iter()
                    .find(|(x, _)| *x == c)
                    .map(|(_, n)| n.clone()),
                folder: c,
            })
            .collect();
        Export {
            mods,
            maps,
            saved_maps: p.maps.iter().any(|m| m != pzmm_core::profiles::BASE_MAP),
            lost_maps: p
                .maps
                .iter()
                .filter(|m| *m != pzmm_core::profiles::BASE_MAP && !folders.contains(m))
                .cloned()
                .collect(),
            default_txt: p.mod_list().text(),
        }
    }
}

#[derive(Serialize)]
pub struct SandboxSchema {
    pub schema: pzmm_core::sandbox::Schema,
    pub game_presets: Vec<String>,
    pub profile_presets: Vec<String>,
    pub user_presets: Vec<String>,
    pub servers: Vec<String>,
}

#[derive(Serialize)]
pub struct SandboxValues {
    pub sandbox: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
    pub server: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
}

#[derive(Serialize)]
pub struct SandboxTexts {
    pub ini: String,
    pub lua: String,
    pub cfg: String,
    pub mods: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FeaturedItem {
    pub id: u64,
    #[serde(alias = "titulo")]
    pub title: String,
    #[serde(alias = "autor")]
    pub author: Option<String>,
    #[serde(alias = "suscriptores")]
    pub subscribers: u64,
    #[serde(alias = "vista_previa")]
    pub preview_view: String,
    #[serde(alias = "coleccion")]
    pub collection: bool,
    #[serde(alias = "hijos")]
    pub children: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Featured {
    #[serde(alias = "leido")]
    pub parsed: u64,
    pub mods: Vec<FeaturedItem>,
    #[serde(alias = "colecciones")]
    pub collections: Vec<FeaturedItem>,
}

pub const SHARED_FORMAT: &str = "pz-mod-manager/perfil";

#[derive(Serialize, Deserialize)]
pub struct SharedMod {
    pub id: String,
    #[serde(default)]
    #[serde(rename = "clave")]
    pub key: String,
    #[serde(default)]
    pub workshop_id: Option<u64>,
    #[serde(default = "default_true")]
    #[serde(rename = "activo")]
    pub enabled: bool,
    #[serde(default)]
    #[serde(rename = "fijado")]
    pub pinned: bool,
    #[serde(default, rename = "seccion", skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
pub struct SharedProfile {
    #[serde(rename = "formato")]
    pub format: String,
    pub version: u32,
    #[serde(rename = "nombre")]
    pub name: String,
    pub mods: Vec<SharedMod>,
    #[serde(default)]
    #[serde(rename = "mapas")]
    pub maps: Vec<String>,
    #[serde(default)]
    #[serde(rename = "arranque")]
    pub launch: pzmm_core::profiles::LaunchSettings,
    #[serde(default)]
    pub zombiebuddy: pzmm_core::profiles::ZbSettings,
    #[serde(default, rename = "secciones", skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
}

#[derive(Serialize)]
pub struct Imported {
    pub profile: ProfileSummary,
    pub missing: Vec<u64>,
    pub ignored: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct ConsoleLines {
    pub profile: String,
    pub lines: Vec<String>,
    pub reset: bool,
}
