mod dto;
mod logging;
mod maps_view;

use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use pzmm_core::app_settings::{self, AppSettings};
use pzmm_core::conflicts::{self, Context};
use pzmm_core::game::{self, Game};
use pzmm_core::maintenance;
use pzmm_core::mods::{self, ScannedMod};
use pzmm_core::profiles::{self, ModEntry, Profile, ProfileStore};
use pzmm_core::sections::Layout;
use pzmm_core::steamworks::{self, Steamworks};
use pzmm_core::version::Version;
use pzmm_core::{PZ_APP_ID, backups, bisect, diagnosis, launch, links, order, steam, zb};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use dto::*;

type R<T> = Result<T, String>;

#[derive(Default)]
struct AppState {
    game: Mutex<Option<Game>>,
    mods: Mutex<Option<Arc<Vec<ScannedMod>>>>,
    media: Mutex<Option<Arc<std::collections::BTreeSet<String>>>>,
    session: Mutex<Option<Session>>,
    steam: Mutex<Option<(Steamworks, Instant)>>,
    steam_idle: std::sync::atomic::AtomicU64,
    sandbox: Mutex<Option<(String, Arc<pzmm_core::sandbox::Definition>)>>,
    counts: Mutex<std::collections::HashMap<String, (SignatureCount, Count)>>,
    settings: Mutex<AppSettings>,
    last_scan: Mutex<Option<LastScan>>,
    map_sources: Mutex<std::collections::HashMap<String, PathBuf>>,
}

struct LastScan {
    mods: Arc<Vec<ScannedMod>>,
    version: Version,
    library: Option<PathBuf>,
    manifests: std::collections::BTreeMap<u64, String>,
}

type SignatureCount = (u64, usize);

fn signature_count(p: &Profile, vis: &Arc<Vec<ScannedMod>>) -> SignatureCount {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    serde_json::to_string(&(&p.mods, &p.accepted_overrides, &p.zombiebuddy))
        .unwrap_or_default()
        .hash(&mut h);
    (h.finish(), Arc::as_ptr(vis) as usize)
}

struct Session {
    profile: String,
    child: Child,
    start: Instant,
    before: Option<(Vec<PathBuf>, pzmm_core::integrity::Snapshot)>,
    pid: u32,
    cache: PathBuf,
    stopped_by_user: bool,
    launched_at: SystemTime,
}

fn watched(j: &Game) -> Vec<PathBuf> {
    let mut v = vec![j.folder.clone()];
    if let Some(h) = std::env::var_os("USERPROFILE") {
        v.push(PathBuf::from(&h).join("Zomboid"));
        v.push(PathBuf::from(&h).join(".zombie_buddy"));
    }
    v.retain(|c| c.is_dir());
    v
}

const INTEGRITY_LIMITS: [(&str, usize); 1] = [("Saves", 3)];

fn expected_change(folder: &std::path::Path, rel: &str) -> bool {
    folder.ends_with(".zombie_buddy") && rel == "authors.json"
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn err<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    logging::error(&s);
    s
}

fn store() -> R<ProfileStore> {
    ProfileStore::default_location()
        .ok_or_else(|| "no se encuentra la carpeta del usuario".to_string())
}

fn app_data(name: &str) -> R<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("PZModManager").join(name))
        .ok_or_else(|| "no se encuentra %APPDATA%".to_string())
}

fn settings_file() -> R<PathBuf> {
    app_data("ajustes.json")
}

fn load_settings() -> AppSettings {
    let read = settings_file().and_then(|f| app_settings::read(&f));
    read.unwrap_or_else(|e| {
        logging::error(&format!("ajustes: {e}"));
        AppSettings::default()
    })
}

#[tauri::command(async)]
fn settings_get(app: AppHandle) -> R<AppSettings> {
    Ok(app
        .state::<AppState>()
        .settings
        .lock()
        .map_err(err)?
        .clone())
}

#[tauri::command(async)]
fn settings_set(app: AppHandle, s: AppSettings) -> R<()> {
    app_settings::validate(&s)?;
    let e = app.state::<AppState>();
    let mut current = lock(&e.settings);
    let s = app_settings::merge(&current, s);
    app_settings::write(&settings_file()?, &s).map_err(err)?;
    *current = s;
    Ok(())
}

fn backups_folder() -> R<PathBuf> {
    app_data("copias")
}

fn chosen_game_file() -> Option<PathBuf> {
    app_data("juego.txt").ok()
}

fn game(e: &AppState) -> R<Game> {
    let mut g = lock(&e.game);
    if g.is_none() {
        let chosen = chosen_game_file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.join("projectzomboid.jar").is_file());
        let j = match chosen {
            Some(c) => game::from_folder(&c),
            None => game::detect(),
        };
        *g = Some(j.map_err(err)?);
    }
    Ok(g.clone().expect("recién detectado"))
}

fn game_folder_change_allowed(session_running: bool) -> R<()> {
    if session_running {
        return Err("el juego está abierto: ciérralo antes".into());
    }
    Ok(())
}

#[tauri::command(async)]
fn folders_info(app: AppHandle) -> R<FoldersInfo> {
    let e = app.state::<AppState>();
    let backups = backups_folder()?;
    Ok(FoldersInfo {
        game: game(&e).ok().map(|j| j.folder.display().to_string()),
        profiles: store()?.root.display().to_string(),
        backups_exists: backups.is_dir(),
        backups_bytes: maintenance::dir_size(&backups),
        backups: backups.display().to_string(),
    })
}

const APP_ID: &str = "dev.pzmodmanager.app";

fn webview_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join(APP_ID).join("EBWebView"))
}

fn clear_webview_if_pending(s: &mut AppSettings) {
    if !s.clear_webview_cache {
        return;
    }
    if let Some(w) = webview_dir() {
        let freed = maintenance::clear_webview_cache(&w);
        logging::info(&format!(
            "caché del WebView vaciada: {:.1} MB",
            freed as f64 / 1_048_576.0
        ));
    }
    s.clear_webview_cache = false;
    if let Err(e) = settings_file().and_then(|f| app_settings::write(&f, s).map_err(err)) {
        logging::error(&format!("ajustes: {e}"));
    }
}

const WORKSHOP_FILES: [&str; 2] = ["workshop.json", "destacados.json"];

#[tauri::command(async)]
fn steam_profile() -> Option<SteamProfileView> {
    let p = pzmm_core::steam_profile::read(&steam::steam_folder()?)?;
    Some(SteamProfileView {
        steamid: p.steamid.to_string(),
        name: p.name,
        minutes: p.minutes,
        last_played: p.last_played,
        avatar: p.avatar_png.map(|b| {
            format!(
                "data:image/png;base64,{}",
                pzmm_core::steam_profile::base64(&b)
            )
        }),
    })
}

#[tauri::command(async)]
fn maintenance_info(app: AppHandle) -> R<MaintenanceInfo> {
    let file_size = |n: &str| {
        app_data(n)
            .ok()
            .and_then(|f| std::fs::metadata(f).ok())
            .map_or(0, |m| m.len())
    };
    Ok(MaintenanceInfo {
        webview_bytes: webview_dir().map_or(0, |w| maintenance::webview_cache_size(&w)),
        webview_pending: app
            .state::<AppState>()
            .settings
            .lock()
            .map_err(err)?
            .clear_webview_cache,
        workshop_bytes: WORKSHOP_FILES.iter().map(|n| file_size(n)).sum(),
        logs_bytes: logging::folder().map_or(0, |f| maintenance::dir_size(&f)),
    })
}

#[tauri::command(async)]
fn maintenance_clear(what: String) -> R<u64> {
    let files: Vec<PathBuf> = match what.as_str() {
        "workshop" => {
            pzmm_core::steamworks::forget_cards();
            WORKSHOP_FILES
                .iter()
                .filter_map(|n| app_data(n).ok())
                .collect()
        }
        "logs" => logging::folder()
            .map(|f| maintenance::old_logs(&f, &logging::today_name()))
            .unwrap_or_default(),
        _ => return Err("opción no válida".into()),
    };
    let mut freed = 0;
    for f in files {
        let size = std::fs::metadata(&f).map_or(0, |m| m.len());
        match std::fs::remove_file(&f) {
            Ok(()) => freed += size,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(err(format!("{}: {e}", f.display()))),
        }
    }
    Ok(freed)
}

#[tauri::command(async)]
fn maintenance_webview(app: AppHandle, pending: bool) -> R<()> {
    let e = app.state::<AppState>();
    let mut s = lock(&e.settings);
    let mut new = s.clone();
    new.clear_webview_cache = pending;
    app_settings::write(&settings_file()?, &new).map_err(err)?;
    *s = new;
    Ok(())
}

fn window_on_play(app: &AppHandle, starting: bool) {
    use app_settings::WindowOnPlay;
    let mode = match app.state::<AppState>().settings.lock() {
        Ok(s) => s.window_on_play,
        Err(_) => return,
    };
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let r = match (mode, starting) {
        (WindowOnPlay::Nothing, _) | (WindowOnPlay::Minimize, false) => Ok(()),
        (_, true) => w.minimize(),
        (WindowOnPlay::MinimizeRestore, false) => w.unminimize().and_then(|_| w.set_focus()),
    };
    if let Err(e) = r {
        logging::error(&format!("ventana al jugar: {e}"));
    }
}

#[tauri::command]
async fn choose_game(app: AppHandle, path: String) -> R<()> {
    in_thread(move || {
        game_folder_change_allowed(
            app.state::<AppState>()
                .session
                .lock()
                .map_err(err)?
                .is_some(),
        )?;
        let folder = PathBuf::from(path.trim().trim_matches('"'));
        game::from_folder(&folder).map_err(|_| {
            format!(
                "{} no parece la carpeta de Project Zomboid: falta projectzomboid.jar",
                folder.display()
            )
        })?;
        let f = chosen_game_file().ok_or("no se encuentra %APPDATA%")?;
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d).map_err(err)?;
        }
        std::fs::write(&f, folder.display().to_string()).map_err(err)?;
        let e = app.state::<AppState>();
        *lock(&e.game) = None;
        *lock(&e.mods) = None;
        *lock(&e.last_scan) = None;
        *lock(&e.media) = None;
        Ok(())
    })
    .await
}

fn version_of(j: &Game) -> Version {
    j.version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0))
}

fn user_mods_folder() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Zomboid").join("mods"))
}

fn profile_mods(e: &AppState, id: &str) -> R<Arc<Vec<ScannedMod>>> {
    let vis = visible_mods(e, false)?;
    let extra = mods::scan_profile_folder(&store()?.cache(id), version_of(&game(e)?));
    if extra.is_empty() {
        return Ok(vis);
    }
    let mut all = (*vis).clone();
    all.extend(extra);
    Ok(Arc::new(all))
}

fn visible_mods(e: &AppState, force: bool) -> R<Arc<Vec<ScannedMod>>> {
    let j = game(e)?;
    let mut g = lock(&e.mods);
    if force || g.is_none() {
        let t0 = Instant::now();
        let v = version_of(&j);
        let manifests: std::collections::BTreeMap<u64, String> = j
            .library
            .as_ref()
            .and_then(|b| steam::read_workshop_state(b, PZ_APP_ID).ok())
            .map(|w| {
                w.items
                    .into_iter()
                    .map(|(id, i)| (id, i.manifest))
                    .collect()
            })
            .unwrap_or_default();
        let mut last = lock(&e.last_scan);
        let reuse = last
            .as_ref()
            .filter(|l| !force && l.version == v && l.library == j.library);
        let keep = |id: u64| {
            reuse.is_some_and(|l| {
                l.manifests.contains_key(&id) && l.manifests.get(&id) == manifests.get(&id)
            })
        };
        let old: &[ScannedMod] = reuse.map(|l| l.mods.as_slice()).unwrap_or(&[]);
        let mut all = j
            .library
            .as_ref()
            .map(|b| {
                mods::scan_workshop_reusing(
                    &steam::workshop_content_folder(b, PZ_APP_ID),
                    v,
                    old,
                    &keep,
                )
            })
            .unwrap_or_default();
        let reused = all
            .iter()
            .filter(|m| matches!(m.source, mods::Source::Workshop { id } if keep(id)))
            .count();
        if let Some(local_dir) = user_mods_folder() {
            all.extend(mods::scan_folder(
                &local_dir,
                mods::Source::Local,
                "local:",
                v,
            ));
        }
        logging::info(&format!(
            "escaneo: {} mods ({reused} sin cambios reutilizados) en {:?}",
            all.len(),
            t0.elapsed()
        ));
        let all = Arc::new(all);
        *last = Some(LastScan {
            mods: all.clone(),
            version: v,
            library: j.library.clone(),
            manifests,
        });
        *g = Some(all);
    }
    Ok(g.clone().expect("recién escaneado"))
}

fn game_media(e: &AppState) -> R<Arc<std::collections::BTreeSet<String>>> {
    let j = game(e)?;
    let mut g = lock(&e.media);
    if g.is_none() {
        *g = Some(Arc::new(game::media_index(&j.folder)));
    }
    Ok(g.clone().expect("recién calculado"))
}

async fn in_thread<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[tauri::command]
async fn environment(app: AppHandle) -> R<Environment> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let app2 = app.clone();
        std::thread::spawn(move || {
            let e = app2.state::<AppState>();
            let _ = visible_mods(&e, false);
        });
        let app3 = app.clone();
        std::thread::spawn(move || {
            let e = app3.state::<AppState>();
            let _ = game_media(&e);
        });
        Ok(Environment::of(&j, steam::steam_folder().as_deref()))
    })
    .await
}

#[tauri::command]
async fn rescan(app: AppHandle) -> R<usize> {
    in_thread(move || {
        let e = app.state::<AppState>();
        lock(&e.counts).clear();
        *lock(&e.game) = None;
        *lock(&e.media) = None;
        Ok(visible_mods(&e, true)?.len())
    })
    .await
}

#[tauri::command(async)]
fn profiles() -> R<Vec<ProfileSummary>> {
    Ok(store()?.list().iter().map(ProfileSummary::of).collect())
}

#[tauri::command(async)]
fn profiles_unreadable() -> R<Vec<(String, String, String)>> {
    let a = store()?;
    Ok(a.unreadable()
        .into_iter()
        .map(|(id, reason)| {
            let path = a.folder(&id).join(profiles::FILE).display().to_string();
            (id, path, reason)
        })
        .collect())
}

#[tauri::command]
async fn profile_create(app: AppHandle, name: String, do_import: bool) -> R<ProfileSummary> {
    in_thread(move || {
        let a = store()?;
        let name = name.trim();
        if name.is_empty() {
            return Err("el perfil necesita un nombre".into());
        }
        let mut p = a.create(name).map_err(err)?;
        let defaults = app
            .state::<AppState>()
            .settings
            .lock()
            .map_err(err)?
            .new_profile
            .clone();
        app_settings::apply_new_profile(&mut p, &defaults);
        a.save(&p).map_err(err)?;
        if do_import {
            let e = app.state::<AppState>();
            let vis = visible_mods(&e, false)?;
            let path = std::env::var_os("USERPROFILE")
                .map(|h| {
                    PathBuf::from(h)
                        .join("Zomboid")
                        .join("mods")
                        .join("default.txt")
                })
                .ok_or("no se encuentra la carpeta del usuario")?;
            if let Ok(l) = pzmm_core::mod_list::ModList::read_file(&path) {
                p.mods = profiles::entries_from(&l, |id| {
                    let found = vis.iter().find(|m| m.id() == Some(id));
                    found.map(|m| m.key.clone())
                });
                p.maps = l.maps;
                a.save(&p).map_err(err)?;
            }
        }
        Ok(ProfileSummary::of(&p))
    })
    .await
}

#[tauri::command(async)]
fn profile_rename(id: String, name: String) -> R<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err("el perfil necesita un nombre".into());
    }
    modify(&id, |p| {
        p.name = name.to_string();
        Ok(())
    })
}

#[tauri::command(async)]
fn profile_delete(app: AppHandle, id: String) -> R<()> {
    check_not_playing(&app, &id)?;
    let a = store()?;
    let folder = a.folder(&id);
    if !folder.join(profiles::FILE).is_file() {
        return Err("no es la carpeta de un perfil".into());
    }
    trash::delete(&folder).map_err(err)
}

static EDIT_LOCK: Mutex<()> = Mutex::new(());

fn modify(id: &str, f: impl FnOnce(&mut Profile) -> R<()>) -> R<()> {
    let _g = EDIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let a = store()?;
    let mut p = a.load(id).map_err(err)?;
    f(&mut p)?;
    a.save(&p).map_err(err)
}

#[tauri::command]
async fn profile_set_enabled(
    app: AppHandle,
    id: String,
    mod_id: String,
    enabled: bool,
) -> R<ProfileView> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        let searched = pzmm_core::modinfo::normalize_id(&mod_id);
        let key = vis
            .iter()
            .find(|m| {
                m.id().map(pzmm_core::modinfo::normalize_id).as_deref() == Some(searched.as_str())
            })
            .map(|m| m.key.clone());
        modify(&id, |p| {
            if p.set_enabled(&mod_id, enabled, key.as_deref()) {
                Ok(())
            } else if enabled {
                Err(format!("«{mod_id}» no está instalado"))
            } else {
                Err("el mod no está en el perfil".into())
            }
        })?;
        view_of(&app, &id)
    })
    .await
}

fn game_list_of(id: &str) -> R<Option<pzmm_core::mod_list::ModList>> {
    let path = store()?.cache(id).join("mods").join("default.txt");
    if !path.is_file() {
        return Ok(None);
    }
    pzmm_core::mod_list::ModList::read_file(&path)
        .map(Some)
        .map_err(err)
}

fn no_game_open(app: &AppHandle) -> R<()> {
    let e = app.state::<AppState>();
    if lock(&e.session).is_some() || !pzmm_core::system::game_processes().is_empty() {
        return Err("cierra el juego antes de cambiar la lista".into());
    }
    Ok(())
}

#[tauri::command(async)]
fn profile_game_list(id: String) -> R<Option<profiles::GameListChange>> {
    let Some(game) = game_list_of(&id)? else {
        return Ok(None);
    };
    let a = store()?;
    if !profiles::game_changed_list(&a.cache(&id)) {
        return Ok(None);
    }
    Ok(a.load(&id).map_err(err)?.game_list_change(&game))
}

#[tauri::command]
async fn profile_take_game_list(app: AppHandle, id: String) -> R<()> {
    in_thread(move || {
        no_game_open(&app)?;
        let game = game_list_of(&id)?.ok_or("el juego no dejó ninguna lista")?;
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        modify(&id, |p| {
            p.take_game_list(&game, |mid| {
                vis.iter()
                    .find(|m| m.id().map(pzmm_core::modinfo::normalize_id).as_deref() == Some(mid))
                    .map(|m| m.key.clone())
            });
            Ok(())
        })?;
        profiles::remember_game_list(&store()?.cache(&id)).map_err(err)
    })
    .await
}

fn enabled_ids(p: &Profile) -> Vec<String> {
    p.mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| pzmm_core::modinfo::normalize_id(&m.id))
        .collect()
}

#[tauri::command]
async fn saves_gaps(app: AppHandle, id: String) -> R<Vec<pzmm_core::saves::SaveGap>> {
    in_thread(move || {
        let p = store()?.load(&id).map_err(err)?;
        let mut gaps = pzmm_core::saves::gaps(&store()?.cache(&id), &enabled_ids(&p));
        if gaps.is_empty() {
            return Ok(gaps);
        }
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        for g in &mut gaps {
            g.with_map = g
                .missing
                .iter()
                .filter(|m| {
                    vis.iter().any(|v| {
                        v.id().map(pzmm_core::modinfo::normalize_id).as_deref() == Some(m.as_str())
                            && !v.maps().is_empty()
                    })
                })
                .cloned()
                .collect();
        }
        Ok(gaps)
    })
    .await
}

#[tauri::command]
async fn saves_add(app: AppHandle, id: String, dirs: Vec<String>) -> R<usize> {
    in_thread(move || {
        no_game_open(&app)?;
        let p = store()?.load(&id).map_err(err)?;
        let cache = store()?.cache(&id);
        let mut n = 0;
        for d in &dirs {
            n += pzmm_core::saves::add(&cache, d, &enabled_ids(&p)).map_err(err)?;
        }
        logging::info(&format!(
            "partidas: {n} mods añadidos en {} partidas de «{}»",
            dirs.len(),
            p.name
        ));
        Ok(n)
    })
    .await
}

#[tauri::command]
async fn saves_restore(app: AppHandle, id: String, dirs: Vec<String>) -> R<()> {
    in_thread(move || {
        no_game_open(&app)?;
        let cache = store()?.cache(&id);
        for d in &dirs {
            pzmm_core::saves::restore(&cache, d).map_err(err)?;
        }
        Ok(())
    })
    .await
}

#[tauri::command(async)]
fn saves_ignore(id: String, dirs: Vec<String>) -> R<()> {
    let p = store()?.load(&id).map_err(err)?;
    let cache = store()?.cache(&id);
    for g in pzmm_core::saves::gaps(&cache, &enabled_ids(&p)) {
        if dirs.contains(&g.dir) {
            pzmm_core::saves::ignore(&cache, &g.dir, &g.missing).map_err(err)?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn profile_discard_game_list(app: AppHandle, id: String) -> R<()> {
    in_thread(move || {
        no_game_open(&app)?;
        let a = store()?;
        let p = a.load(&id).map_err(err)?;
        profiles::write_game_list(&a.cache(&id), &p.mod_list().text(), false).map_err(err)
    })
    .await
}

#[tauri::command(async)]
fn profile_win(id: String, mod_id: String, rivals: Vec<String>) -> R<()> {
    modify(&id, |p| {
        if p.win_over_many(&mod_id, &rivals) {
            Ok(())
        } else {
            Err("el mod no está en el perfil".into())
        }
    })
}

#[tauri::command(async)]
fn profile_accept(id: String, pairs: Vec<(String, String)>) -> R<()> {
    modify(&id, |p| {
        for (loses, wins) in &pairs {
            p.accept(loses, wins);
        }
        Ok(())
    })
}

#[tauri::command(async)]
fn profile_remove_many(id: String, mod_ids: Vec<String>) -> R<Vec<(usize, ModEntry)>> {
    let mut removed = Vec::new();
    modify(&id, |p| {
        removed = p.remove_many(&mod_ids);
        Ok(())
    })?;
    Ok(removed)
}

#[tauri::command(async)]
fn profile_put_back(id: String, removed: Vec<(usize, ModEntry)>) -> R<()> {
    modify(&id, |p| {
        p.put_back(removed);
        Ok(())
    })
}

#[tauri::command(async)]
fn profile_set_enabled_many(id: String, mod_ids: Vec<String>, enabled: bool) -> R<usize> {
    let mut n = 0;
    modify(&id, |p| {
        n = p.set_enabled_many(&mod_ids, enabled).len();
        Ok(())
    })?;
    Ok(n)
}

#[tauri::command(async)]
fn profile_pin(id: String, mod_id: String, pinned: bool) -> R<()> {
    modify(&id, |p| {
        let m = p
            .mods
            .iter_mut()
            .find(|m| m.id == mod_id)
            .ok_or("el mod no está en el perfil")?;
        m.pinned = pinned;
        Ok(())
    })
}

fn edit_layout(id: &str, f: impl FnOnce(&mut Profile) -> Result<Layout, String>) -> R<Layout> {
    let mut prev = None;
    modify(id, |p| {
        prev = Some(f(p)?);
        Ok(())
    })?;
    prev.ok_or_else(|| "sin cambios".into())
}

#[tauri::command(async)]
fn profile_move(id: String, from: usize, to: usize, section: Option<String>) -> R<Layout> {
    edit_layout(&id, |p| p.move_mod(from, to, section.as_deref()))
}

#[tauri::command(async)]
fn section_create(id: String, name: String, at_mod: Option<String>) -> R<Layout> {
    edit_layout(&id, |p| p.section_create(&name, at_mod.as_deref()))
}

#[tauri::command(async)]
fn section_rename(id: String, old: String, new: String) -> R<Layout> {
    edit_layout(&id, |p| p.section_rename(&old, &new))
}

#[tauri::command(async)]
fn section_move(id: String, name: String, to: usize) -> R<Layout> {
    edit_layout(&id, |p| p.section_move(&name, to))
}

#[tauri::command(async)]
fn section_delete(id: String, name: String) -> R<Layout> {
    edit_layout(&id, |p| p.section_delete(&name))
}

#[tauri::command(async)]
fn section_assign(
    id: String,
    mod_ids: Vec<String>,
    name: Option<String>,
    first: Option<bool>,
) -> R<Layout> {
    edit_layout(&id, |p| {
        p.section_assign(&mod_ids, name.as_deref(), first == Some(true))
    })
}

#[tauri::command(async)]
fn profile_restore_layout(id: String, layout: Layout) -> R<()> {
    modify(&id, |p| {
        p.restore_layout(layout);
        Ok(())
    })
}

#[tauri::command]
async fn sections_from_categories(
    app: AppHandle,
    id: String,
    names: std::collections::BTreeMap<String, String>,
) -> R<Layout> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = profile_mods(&e, &id)?;
        let p = store()?.load(&id).map_err(err)?;
        let resolved = conflicts::profile_entries(&p, &vis);
        let list: Vec<(&str, bool, Option<&ScannedMod>)> = resolved
            .iter()
            .map(|x| (x.id.as_str(), x.enabled, x.found))
            .collect();
        let rules = pzmm_core::categories::user_rules();
        let (_, info) = pzmm_core::categories::plan_order(&list, &rules, &p.accepted_overrides);
        let entries: Vec<_> = resolved.iter().map(|x| x.order_entry()).collect();
        let edges = pzmm_core::order::edges(&entries);
        edit_layout(&id, |p| {
            if !p.sections.is_empty() {
                return Err("el perfil ya tiene secciones".into());
            }
            let prev = p.layout();
            for m in &mut p.mods {
                let key = info
                    .get(&pzmm_core::modinfo::normalize_id(&m.id))
                    .map_or("undefined", |i| i.category.key());
                let name = names
                    .get(key)
                    .and_then(|n| pzmm_core::sections::clean_name(n))
                    .unwrap_or_else(|| key.to_string());
                if !p.sections.contains(&name) {
                    p.sections.push(name.clone());
                }
                m.section = Some(name);
            }
            p.regroup();
            p.pull_requirements(&edges);
            Ok(prev)
        })
    })
    .await
}

#[tauri::command]
async fn profile_add(
    app: AppHandle,
    id: String,
    keys: Vec<String>,
    enabled: Option<bool>,
    section: Option<String>,
) -> R<()> {
    let enabled = enabled.unwrap_or(true);
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        modify(&id, |p| {
            let section = match section.as_deref() {
                None => p.sections.last().cloned(),
                Some("") => None,
                Some(s) if p.sections.iter().any(|x| x == s) => Some(s.to_string()),
                Some(_) => return Err("no existe esa sección".into()),
            };
            for c in &keys {
                let m = vis
                    .iter()
                    .find(|m| &m.key == c)
                    .ok_or("mod no encontrado")?;
                let mid = m.id().ok_or("el mod no tiene id")?.to_string();
                match p.mods.iter_mut().find(|x| x.id == mid) {
                    Some(x) => x.enabled |= enabled,
                    None => p.mods.push(ModEntry {
                        key: c.clone(),
                        id: mid,
                        enabled,
                        pinned: false,
                        section: section.clone(),
                    }),
                }
            }
            p.regroup();
            Ok(())
        })
    })
    .await
}

#[tauri::command(async)]
fn profile_remove(id: String, mod_id: String) -> R<()> {
    modify(&id, |p| {
        p.mods.retain(|m| m.id != mod_id);
        Ok(())
    })
}

#[tauri::command(async)]
fn profile_settings(id: String, settings: ProfileSettings) -> R<()> {
    modify(&id, |p| {
        if !["prompt", "deny-new", "allow-all"].contains(&settings.policy.as_str()) {
            return Err("política de ZombieBuddy no válida".into());
        }
        if !["auto", "si", "no"].contains(&settings.use_zb.as_str()) {
            return Err("opción de ZombieBuddy no válida".into());
        }
        let xmx = settings.xmx.trim();
        if !xmx.is_empty() && !profiles::is_valid_xmx(xmx) {
            return Err("memoria no válida: usa un tamaño como 8g o 3072m".into());
        }
        p.launch.xmx = xmx.to_string();
        p.launch.game_extra = settings
            .game_extra
            .split_whitespace()
            .map(str::to_string)
            .collect();
        p.zombiebuddy.policy = settings.policy;
        p.zombiebuddy.usage = settings.use_zb;
        p.launch.probe = settings.probe;
        Ok(())
    })
}

#[tauri::command(async)]
fn probe_report(app: AppHandle, id: String) -> R<Option<dto::ProbeReport>> {
    let e = app.state::<AppState>();
    let a = store()?;
    let p = a.load(&id).map_err(err)?;
    if !pzmm_core::probe::same_list(&a.cache(&p.id), &p.mod_list().text()) {
        return Ok(None);
    }
    let seen = pzmm_core::probe::read(&a.cache(&p.id));
    let Some(phase) = ["start", "boot"]
        .into_iter()
        .find(|f| seen.iter().any(|s| s.phase == *f))
    else {
        return Ok(None);
    };
    let j = game(&e)?;
    let vis = profile_mods(&e, &id)?;
    let media = game_media(&e)?;
    let asked = pzmm_core::probe::asked(&a.cache(&p.id));
    let predicted: Vec<(String, String)> = conflicts::lua_predictions(
        &conflicts::profile_entries(&p, &vis),
        &vis,
        &context(&j, &p, &media),
        if phase == "start" {
            conflicts::LuaPhase::Start
        } else {
            conflicts::LuaPhase::Boot
        },
    )
    .into_iter()
    .filter(|(n, _)| asked.contains(n))
    .collect();
    let mismatches = pzmm_core::probe::check(&predicted, &seen, phase);
    Ok(Some(dto::ProbeReport {
        phase: phase.to_string(),
        total: predicted.len(),
        mismatches,
    }))
}

fn context<'a>(
    j: &'a Game,
    p: &Profile,
    media: &'a std::collections::BTreeSet<String>,
) -> Context<'a> {
    let cfg = zb::config_folder();
    Context {
        game: version_of(j),
        game_files: Some(media),
        game_folder: Some(&j.folder),
        zb: zb::jar_version(&j.folder.join("ZombieBuddy.jar")),
        policy_zb: p.zombiebuddy.policy.clone(),
        approvals: cfg.as_deref().map(zb::read_approvals).unwrap_or_default(),
        trust: cfg.as_deref().map(zb::trusted_authors).unwrap_or_default(),
        modifications: j.modifications.clone(),
        accepted_overrides: p
            .accepted_overrides
            .iter()
            .map(|a| (a.loses.clone(), a.wins.clone()))
            .collect(),
        maps: p.maps.clone(),
        sections: p.sections.clone(),
        urban: pzmm_core::maps::game_urban(&j.folder),
    }
}

#[tauri::command]
async fn profile_view(app: AppHandle, id: String) -> R<ProfileView> {
    in_thread(move || view_of(&app, &id)).await
}

#[derive(Clone, serde::Serialize)]
struct ViewProgress {
    profile: String,
    stage: &'static str,
    done: usize,
    total: usize,
}

fn view_of(app: &AppHandle, id: &str) -> R<ProfileView> {
    {
        let id = id.to_string();
        let e = app.state::<AppState>();
        let tell = |stage: &'static str, done: usize, total: usize| {
            let _ = app.emit(
                "view-progress",
                ViewProgress {
                    profile: id.clone(),
                    stage,
                    done,
                    total,
                },
            );
        };
        let counted = |stage: &'static str| {
            let tell = &tell;
            move |done: usize, total: usize| {
                if done == total || done.is_multiple_of((total / 100).max(1)) {
                    tell(stage, done, total);
                }
            }
        };
        let j = game(&e)?;
        tell("mods", 0, 0);
        let vis = profile_mods(&e, &id)?;
        let p = store()?.load(&id).map_err(err)?;
        let resolved = conflicts::profile_entries(&p, &vis);
        tell("game", 0, 0);
        let media = game_media(&e)?;
        let enabled: Vec<&ScannedMod> = resolved
            .iter()
            .filter(|x| x.enabled)
            .filter_map(|x| x.found)
            .collect();
        let start = Instant::now();
        logging::info(&format!("análisis de {id}: {} activos", enabled.len()));
        ScannedMod::index_all_with(&enabled, &counted("files"));
        let with_scripts: Vec<&ScannedMod> = enabled
            .iter()
            .copied()
            .filter(|m| {
                m.relative_paths()
                    .iter()
                    .any(|r| r.starts_with("media/scripts/"))
            })
            .collect();
        ScannedMod::scripts_all_with(&with_scripts, &counted("scripts"));
        let with_lua: Vec<&ScannedMod> = enabled
            .iter()
            .copied()
            .filter(|m| {
                m.relative_paths()
                    .iter()
                    .any(|r| r.starts_with("media/lua/"))
            })
            .collect();
        ScannedMod::lua_all_with(&with_lua, &counted("lua"));
        tell("conflicts", 0, 0);
        let ctx = context(&j, &p, &media);
        let findings = conflicts::analyze_with(&resolved, &vis, &ctx, &counted("conflicts"));
        let v = ProfileView::of(&p, &resolved, findings);
        logging::info(&format!("análisis de {id}: hecho en {:?}", start.elapsed()));
        let signature = signature_count(&p, &vis);
        e.counts
            .lock()
            .map_err(err)?
            .insert(id, (signature, v.counts.clone()));
        Ok(v)
    }
}

#[tauri::command]
async fn profile_export(app: AppHandle, id: String) -> R<Export> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = profile_mods(&e, &id)?;
        let p = store()?.load(&id).map_err(err)?;
        let resolved = conflicts::profile_entries(&p, &vis);
        Ok(Export::of(&p, &resolved))
    })
    .await
}

#[tauri::command]
async fn profile_maps_view(app: AppHandle, id: String, language: String) -> R<maps_view::MapsView> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let media = game_media(&e)?;
        let vis = profile_mods(&e, &id)?;
        let p = store()?.load(&id).map_err(err)?;
        let resolved = conflicts::profile_entries(&p, &vis);
        Ok(maps_view::build(
            &j.folder,
            &media,
            &vis,
            &p,
            &resolved,
            &language,
            &e.map_sources,
        ))
    })
    .await
}

#[tauri::command(async)]
fn profile_maps(id: String, maps: Vec<String>, server_excluded: Vec<String>) -> R<()> {
    modify(&id, |p| {
        let mut m: Vec<String> = maps
            .into_iter()
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty() && x != profiles::BASE_MAP)
            .collect();
        m.dedup();
        m.push(profiles::BASE_MAP.to_string());
        p.maps = m;
        p.server_excluded_maps = server_excluded
            .into_iter()
            .filter(|x| profiles::is_valid_map(x) && x != profiles::BASE_MAP)
            .collect();
        Ok(())
    })
}

#[tauri::command]
async fn profile_sort(
    app: AppHandle,
    id: String,
    apply: bool,
    section: Option<String>,
) -> R<OrderProposal> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = profile_mods(&e, &id)?;
        let a = store()?;
        let mut p = a.load(&id).map_err(err)?;
        let resolved = conflicts::profile_entries(&p, &vis);
        let entries: Vec<order::Entry> = resolved.iter().map(|x| x.order_entry()).collect();
        let rules = pzmm_core::categories::user_rules();
        let list: Vec<(&str, bool, Option<&ScannedMod>)> = resolved
            .iter()
            .map(|x| (x.id.as_str(), x.enabled, x.found))
            .collect();
        let (options, info) =
            pzmm_core::categories::plan_order(&list, &rules, &p.accepted_overrides);
        let section_of: Vec<usize> = p.mods.iter().map(|m| p.section_index(m)).collect();
        let only = match &section {
            Some(name) => Some(
                p.sections
                    .iter()
                    .position(|s| s == name)
                    .ok_or("no existe esa sección")?
                    + 1,
            ),
            None => None,
        };
        let mut r = pzmm_core::sections::sort_sections(&entries, &options, &section_of, only);
        let version = version_of(&game(&e)?);
        conflicts::mark_winner_moves(&mut r, &resolved, &vis, version);
        let proposal = OrderProposal::of(&r, &info, rules.len());
        if apply {
            let _g = EDIT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            p = a.load(&id).map_err(err)?;
            pzmm_core::sections::apply_order(&mut p, &r.order);
            a.save(&p).map_err(err)?;
        }
        Ok(proposal)
    })
    .await
}

#[tauri::command]
async fn library(app: AppHandle) -> R<Vec<LibraryMod>> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let pending_ids: std::collections::BTreeSet<u64> = j
            .library
            .as_ref()
            .and_then(|b| steam::read_workshop_state(b, PZ_APP_ID).ok())
            .map(|w| w.pending_ids().into_iter().collect())
            .unwrap_or_default();
        Ok(visible_mods(&e, false)?
            .iter()
            .map(|m| LibraryMod::of(m, &pending_ids))
            .collect())
    })
    .await
}

#[tauri::command]
async fn workshop_waiting(app: AppHandle) -> R<Vec<u64>> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let Some(b) = j.library.as_ref() else {
            return Ok(Vec::new());
        };
        let content = steam::workshop_content_folder(b, PZ_APP_ID);
        Ok(steam::read_workshop_state(b, PZ_APP_ID)
            .map(|w| w.pending_ids())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| !content.join(id.to_string()).is_dir())
            .collect())
    })
    .await
}

fn changed_mods(app: &AppHandle) {
    if let Some(e) = app.try_state::<AppState>() {
        *lock(&e.mods) = None;
        *lock(&e.sandbox) = None;
        lock(&e.counts).clear();
    }
    pzmm_core::maps::forget();
    let _ = app.emit("mods-changed", ());
}

fn watch_mods(app: AppHandle) {
    std::thread::spawn(move || {
        let mut lib: Option<PathBuf> = None;
        let signature = |lib: &Option<PathBuf>| -> Vec<Option<std::time::SystemTime>> {
            let mut paths: Vec<PathBuf> = Vec::new();
            if let Some(b) = lib {
                paths.push(
                    b.join("steamapps")
                        .join(format!("appmanifest_{PZ_APP_ID}.acf")),
                );
                paths.push(steam::workshop_content_folder(b, PZ_APP_ID));
                paths.push(
                    b.join("steamapps")
                        .join("workshop")
                        .join(format!("appworkshop_{PZ_APP_ID}.acf")),
                );
            }
            if let Some(local) = user_mods_folder() {
                paths.push(local.clone());
                for m in std::fs::read_dir(&local).into_iter().flatten().flatten() {
                    let dir = m.path();
                    paths.push(dir.join("mod.info"));
                    for sub in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                        if sub.file_type().is_ok_and(|t| t.is_dir()) {
                            paths.push(sub.path().join("mod.info"));
                        }
                    }
                    paths.push(dir);
                }
            }
            paths
                .iter()
                .map(|r| std::fs::metadata(r).and_then(|m| m.modified()).ok())
                .collect()
        };
        let mut previous = signature(&lib);
        loop {
            std::thread::sleep(Duration::from_secs(3));
            if lib.is_none() {
                let e = app.state::<AppState>();
                lib = e
                    .game
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().and_then(|j| j.library.clone()));
                if lib.is_some() {
                    previous = signature(&lib);
                }
                continue;
            }
            let current = {
                let e = app.state::<AppState>();
                e.game
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().and_then(|j| j.library.clone()))
            };
            if current.is_some() && current != lib {
                lib = current;
                previous = signature(&lib);
                changed_mods(&app);
                continue;
            }
            let now = signature(&lib);
            if now != previous {
                if now.first() != previous.first() {
                    let e = app.state::<AppState>();
                    let _ = e.game.lock().map(|mut g| *g = None);
                    let _ = e.media.lock().map(|mut g| *g = None);
                    let _ = e.counts.lock().map(|mut g| g.clear());
                }
                previous = now;
                changed_mods(&app);
            }
        }
    });
}

#[tauri::command]
async fn mod_details(app: AppHandle, key: String) -> R<ModDetails> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let vis = visible_mods(&e, false)?;
        let m = vis
            .iter()
            .find(|m| m.key == key)
            .ok_or("mod no encontrado")?;
        Ok(ModDetails::of(
            m,
            zb::jar_version(&j.folder.join("ZombieBuddy.jar")).as_deref(),
        ))
    })
    .await
}

#[tauri::command]
async fn launch(app: AppHandle, id: String) -> R<u32> {
    in_thread(move || {
        static LAUNCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        struct Launching;
        impl Drop for Launching {
            fn drop(&mut self) {
                LAUNCHING.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        if LAUNCHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err("el juego ya está arrancando".into());
        }
        let _launching = Launching;
        let e = app.state::<AppState>();
        if lock(&e.session).is_some() || !pzmm_core::system::game_processes().is_empty() {
            return Err("el juego ya está abierto".into());
        }
        let j = game(&e)?;
        let vis = profile_mods(&e, &id)?;
        let a = store()?;
        let p = a.load(&id).map_err(err)?;
        let cache = a.cache(&p.id);
        let options = std::env::var_os("USERPROFILE")
            .map(|h| PathBuf::from(h).join("Zomboid").join("options.ini"));
        profiles::prepare_cache(&cache, &p, options.as_deref()).map_err(err)?;
        if p.launch.probe {
            let media = game_media(&e)?;
            let entries = conflicts::profile_entries(&p, &vis);
            let ctx = context(&j, &p, &media);
            let names = conflicts::lua_contested(&entries, &vis, &ctx);
            pzmm_core::probe::install(&cache, &names, &p.mod_list().text()).map_err(err)?;
        } else {
            pzmm_core::probe::remove(&cache).map_err(err)?;
        }
        let wanted = links::wanted(&conflicts::profile_entries(&p, &vis));
        let changes = links::sync(&cache.join("mods"), &wanted).map_err(err)?;
        if !changes.taken.is_empty() {
            return Err(format!(
                "la carpeta mods del perfil ya tiene una carpeta real con el nombre de: {}",
                changes.taken.join(", ")
            ));
        }
        let active: Vec<&str> = p
            .mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.id.as_str())
            .collect();
        let has_java = vis.iter().any(|m| {
            m.id().is_some_and(|i| active.contains(&i))
                && m.info.as_ref().is_some_and(|i| i.is_java())
        });
        let use_zb = match p.zombiebuddy.usage.as_str() {
            "si" => true,
            "no" => false,
            _ => has_java,
        };
        if use_zb && !j.folder.join("zbNative.dll").is_file() {
            return Err("este perfil usa mods Java y ZombieBuddy no está instalado".into());
        }
        let zb_opts = use_zb.then(|| launch::ZbOptions {
            policy: p.zombiebuddy.policy.clone(),
            verbosity: p.zombiebuddy.verbosity,
        });
        let line = launch::build(&j, &p, &cache, zb_opts.as_ref()).map_err(err)?;
        *lock(&e.steam) = None;
        let folders = watched(&j);
        let t0 = Instant::now();
        let checking = should_snapshot(&lock(&e.settings));
        let before =
            checking.then(|| pzmm_core::integrity::take_with_limits(&folders, &INTEGRITY_LIMITS));
        logging::info(&format!(
            "arrancar «{}» ({} mods activos, ZB {}): instantánea de {} archivos en {:?}",
            p.name,
            active.len(),
            if use_zb { "sí" } else { "no" },
            before
                .as_ref()
                .map_or(0, |b| b.folders.values().map(|m| m.len()).sum::<usize>()),
            t0.elapsed()
        ));
        let child = launch::launch(&line).map_err(err)?;
        let pid = child.id();
        *lock(&e.session) = Some(Session {
            profile: id.clone(),
            child,
            start: Instant::now(),
            before: before.map(|b| (folders, b)),
            pid,
            cache: cache.clone(),
            stopped_by_user: false,
            launched_at: SystemTime::now() - Duration::from_secs(2),
        });
        watch(app.clone(), cache.join("console.txt"), active.len());
        window_on_play(&app, true);
        Ok(pid)
    })
    .await
}

fn read_new(path: &std::path::Path, from: &mut u64, rest: &mut Vec<u8>) -> (Vec<String>, bool) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return (Vec::new(), false);
    };
    let length = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut reset = false;
    if length < *from {
        *from = 0;
        rest.clear();
        reset = true;
    }
    if length == *from || f.seek(SeekFrom::Start(*from)).is_err() {
        return (Vec::new(), reset);
    }
    let before = rest.len();
    if f.take(4 << 20).read_to_end(rest).is_err() {
        rest.truncate(before);
        return (Vec::new(), reset);
    }
    *from += (rest.len() - before) as u64;
    let Some(end) = rest.iter().rposition(|&b| b == b'\n') else {
        return (Vec::new(), reset);
    };
    let tail = rest.split_off(end + 1);
    let mut lines: Vec<String> = String::from_utf8_lossy(rest)
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    lines.pop();
    *rest = tail;
    (lines, reset)
}

fn watch(app: AppHandle, console: PathBuf, requested: usize) {
    let launch = std::time::SystemTime::now() - Duration::from_secs(2);
    std::thread::spawn(move || {
        let (mut from, mut rest) = (0u64, Vec::new());
        let mut loaded = 0usize;
        let mut recent_lines: std::collections::VecDeque<String> =
            std::collections::VecDeque::new();
        let mut back = 0u64;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            back += 1;
            let e = app.state::<AppState>();
            let profile = match e.session.lock() {
                Ok(g) => match g.as_ref() {
                    Some(s) => s.profile.clone(),
                    None => return,
                },
                Err(_) => return,
            };
            let fresh = std::fs::metadata(&console)
                .and_then(|m| m.modified())
                .is_ok_and(|m| m >= launch);
            if fresh {
                let (lines, reset) = read_new(&console, &mut from, &mut rest);
                if reset {
                    loaded = 0;
                    recent_lines.clear();
                }
                if !lines.is_empty() || reset {
                    loaded += launch::read_console(&lines.join("\n")).loaded.len();
                    for l in &lines {
                        recent_lines.push_back(l.clone());
                        if recent_lines.len() > 20 {
                            recent_lines.pop_front();
                        }
                    }
                    let _ = app.emit(
                        "console",
                        &ConsoleLines {
                            profile: profile.clone(),
                            lines,
                            reset,
                        },
                    );
                }
            }
            let Ok(mut g) = e.session.lock() else { return };
            let Some(session) = g.as_mut() else { return };
            let exit = session.child.try_wait().ok().flatten();
            if exit.is_none() && back % 2 == 1 {
                continue;
            }
            let mut state = SessionState {
                profile: session.profile.clone(),
                seconds: session.start.elapsed().as_secs(),
                requested,
                loaded,
                finished: exit.is_some(),
                code: exit.and_then(|s| s.code()),
                recent_lines: match exit {
                    Some(s) if !s.success() => recent_lines.iter().cloned().collect(),
                    _ => Vec::new(),
                },
                integrity: None,
                integrity_skipped: false,
                diagnosis: None,
            };
            if !state.finished {
                let _ = app.emit("session", &state);
                continue;
            }
            let ended = g.take();
            drop(g);
            let (lines, _) = read_new(&console, &mut from, &mut rest);
            if !lines.is_empty() {
                let _ = app.emit(
                    "console",
                    &ConsoleLines {
                        profile: profile.clone(),
                        lines,
                        reset: false,
                    },
                );
            }
            let (before, ended) = match ended {
                Some(s) => (
                    s.before,
                    Some((s.profile, s.pid, s.cache, s.stopped_by_user, s.launched_at)),
                ),
                None => (None, None),
            };
            state.integrity_skipped = ended.is_some() && before.is_none();
            if let Some((folders, before)) = before {
                let after = pzmm_core::integrity::take_with_limits(&folders, &INTEGRITY_LIMITS);
                let diff = pzmm_core::integrity::compare(&before, &after);
                let all: Vec<String> = diff
                    .added
                    .iter()
                    .map(|x| ("nuevo", x))
                    .chain(diff.deleted.iter().map(|x| ("borrado", x)))
                    .chain(diff.changed.iter().map(|x| ("cambiado", x)))
                    .filter(|(_, (c, r))| !expected_change(c, r))
                    .map(|(q, (c, r))| format!("{q}: {}/{r}", c.display()))
                    .collect();
                for x in &all {
                    logging::info(&format!("integridad: {x}"));
                }
                state.integrity = Some(Integrity {
                    total: all.len(),
                    examples: all.into_iter().take(8).collect(),
                });
            }
            logging::info(&format!(
                "partida terminada: código {:?}, {} s, {} cambios fuera del perfil",
                state.code,
                state.seconds,
                state.integrity.as_ref().map(|i| i.total).unwrap_or(0)
            ));
            if let Some((id, pid, cache, stopped, since)) = ended {
                let ended = Ended {
                    pid,
                    stopped,
                    since,
                    code: state.code,
                };
                state.diagnosis = diagnose(&app, &id, &cache, &ended)
                    .map_err(|e| logging::error(&format!("diagnóstico: {e}")))
                    .ok();
            }
            window_on_play(&app, false);
            let _ = app.emit("session", &state);
            return;
        }
    });
}

struct Ended {
    pid: u32,
    stopped: bool,
    since: SystemTime,
    code: Option<i32>,
}

fn find_crash_report(cache: &std::path::Path, pid: u32, since: SystemTime) -> Option<PathBuf> {
    let recent = |p: &std::path::Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .filter(|t| *t >= since)
    };
    let own = cache.join(format!("hs_err_pid{pid}.log"));
    if recent(&own).is_some() {
        return Some(own);
    }
    std::fs::read_dir(cache)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("hs_err_pid") && n.ends_with(".log"))
        })
        .filter_map(|p| recent(&p).map(|t| (t, p)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

const DIAGNOSIS_FILE: &str = "pzmm-diagnosis.json";
const LAST_GOOD_FILE: &str = "last-good.json";
const NOISE_FILE: &str = "console-noise.json";
const NOISE_MAX: usize = 500;

#[derive(serde::Serialize, serde::Deserialize)]
struct Noise {
    game: String,
    keys: Vec<String>,
}

fn read_noise(game: &str) -> Vec<String> {
    app_data(NOISE_FILE)
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str::<Noise>(&t).ok())
        .filter(|n| n.game == game)
        .map(|n| n.keys)
        .unwrap_or_default()
}

fn learn_noise(issues: &[pzmm_core::console::Issue], game: &str) -> R<()> {
    let mut keys = read_noise(game);
    for i in issues {
        if !keys.contains(&i.key) {
            keys.push(i.key.clone());
        }
    }
    let extra = keys.len().saturating_sub(NOISE_MAX);
    keys.drain(..extra);
    let f = app_data(NOISE_FILE)?;
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    let noise = Noise {
        game: game.to_string(),
        keys,
    };
    profiles::write_atomic(&f, &serde_json::to_string(&noise).map_err(err)?).map_err(err)
}

fn game_text(e: &AppState) -> String {
    game(e)
        .ok()
        .and_then(|j| j.version.as_ref().map(|v| v.text()))
        .unwrap_or_default()
}

fn console_sources<'a>(
    vis: &'a [ScannedMod],
    p: Option<&Profile>,
) -> Vec<pzmm_core::console::ModSource<'a>> {
    let to = |m: &'a ScannedMod| {
        Some(pzmm_core::console::ModSource {
            id: m.id()?.to_string(),
            pkg: m
                .info
                .as_ref()
                .and_then(|i| i.java.as_ref())
                .and_then(|j| j.pkg.clone()),
            scanned: m,
        })
    };
    match p {
        Some(p) => conflicts::profile_entries(p, vis)
            .iter()
            .filter(|x| x.enabled)
            .filter_map(|x| x.found)
            .filter_map(to)
            .collect(),
        None => vis.iter().filter_map(to).collect(),
    }
}

fn issues_of(
    app: &AppHandle,
    id: Option<&str>,
    text: &str,
    budget: Duration,
) -> R<Vec<pzmm_core::console::Issue>> {
    let e = app.state::<AppState>();
    let noise = read_noise(&game_text(&e));
    let mut v = pzmm_core::console::analyze(text, &known_mods(app, id)?);
    for i in &mut v {
        i.noise = noise.contains(&i.key);
    }
    let vis = visible_mods(&e, false)?;
    let p = id.map(|id| store()?.load(id).map_err(err)).transpose()?;
    pzmm_core::console::attribute(&mut v, &console_sources(&vis, p.as_ref()), budget);
    Ok(v)
}

fn finish_attribution(
    app: &AppHandle,
    id: Option<String>,
    mut issues: Vec<pzmm_core::console::Issue>,
) {
    use pzmm_core::console::Kind;
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    let pending = issues
        .iter()
        .any(|i| i.mod_id.is_none() && !i.from_game && i.kind == Kind::Require);
    if !pending || lock(&app.state::<AppState>().session).is_some() {
        return;
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let e = app.state::<AppState>();
        if let Ok(vis) = visible_mods(&e, false) {
            let p = id.and_then(|id| store().ok()?.load(&id).ok());
            pzmm_core::console::attribute(
                &mut issues,
                &console_sources(&vis, p.as_ref()),
                Duration::from_secs(120),
            );
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

fn mtime(p: &std::path::Path) -> Option<u64> {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn mod_stamps(
    resolved: &[conflicts::ProfileEntry],
    workshop: Option<&steam::WorkshopState>,
) -> std::collections::BTreeMap<String, u64> {
    resolved
        .iter()
        .filter(|x| x.enabled)
        .filter_map(|x| {
            let m = x.found?;
            let manifest = match (&m.source, workshop) {
                (mods::Source::Workshop { id }, Some(w)) => {
                    w.items.get(id).and_then(|i| i.manifest.parse::<u64>().ok())
                }
                _ => None,
            };
            let t = manifest.or_else(|| {
                std::iter::once(m.path_info.clone())
                    .chain(m.roots().into_iter().map(|r| Some(r.join("media"))))
                    .flatten()
                    .filter_map(|p| mtime(&p))
                    .max()
            })?;
            Some((pzmm_core::modinfo::normalize_id(&x.id), t))
        })
        .collect()
}

fn read_last_good(cache: &std::path::Path) -> Option<diagnosis::LastGood> {
    std::fs::read_to_string(cache.join(LAST_GOOD_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

fn should_snapshot(s: &pzmm_core::app_settings::AppSettings) -> bool {
    s.integrity_check
}

fn session_console(path: &std::path::Path, since: SystemTime) -> R<String> {
    let fresh = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t >= since);
    if !fresh {
        return Ok(String::new());
    }
    match std::fs::read(path) {
        Ok(b) => Ok(String::from_utf8_lossy(&b).into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(err(format!("{}: {e}", path.display()))),
    }
}

fn why_not_loaded(findings: &[conflicts::Finding], id: &str) -> Option<String> {
    let about = |f: &&conflicts::Finding| {
        f.mods
            .iter()
            .any(|m| pzmm_core::modinfo::normalize_id(m) == id)
    };
    findings
        .iter()
        .filter(about)
        .find(|f| f.severity == conflicts::Severity::Blocks)
        .or_else(|| findings.iter().find(about))
        .and_then(|f| serde_json::to_value(&f.kind).ok())
        .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
}

fn diagnose(app: &AppHandle, id: &str, cache: &std::path::Path, ended: &Ended) -> R<Diagnosis> {
    let e = app.state::<AppState>();
    let crash = find_crash_report(cache, ended.pid, ended.since)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| diagnosis::parse_hs_err(&String::from_utf8_lossy(&b)));
    let text = session_console(&cache.join("console.txt"), ended.since)?;
    let report = launch::read_console(&text);
    let p = store()?.load(id).map_err(err)?;
    let requested: Vec<String> = p
        .mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.id.clone())
        .collect();
    let missing = diagnosis::not_loaded(&requested, &report);
    let vis = visible_mods(&e, false)?;
    let j = game(&e)?;
    let media = game_media(&e)?;
    let resolved = conflicts::profile_entries(&p, &vis);
    let findings = conflicts::analyze(&resolved, &vis, &context(&j, &p, &media));
    let blocked: Vec<String> = resolved
        .iter()
        .filter(|x| x.enabled)
        .map(|x| pzmm_core::modinfo::normalize_id(&x.id))
        .filter(|id| {
            findings.iter().any(|f| {
                f.severity == conflicts::Severity::Blocks
                    && f.mods
                        .iter()
                        .any(|m| pzmm_core::modinfo::normalize_id(m) == *id)
            })
        })
        .collect();
    let loaded_all = diagnosis::loaded_all(&requested, &report, &blocked);
    let issues = issues_of(app, Some(id), &text, Duration::from_secs(1))?;
    finish_attribution(app, Some(id.to_string()), issues.clone());
    let packages: Vec<(String, String)> = resolved
        .iter()
        .filter(|x| x.enabled)
        .filter_map(|x| x.found)
        .filter_map(|m| {
            let pkg = m.info.as_ref()?.java.as_ref()?.pkg.clone()?;
            Some((m.id()?.to_string(), pkg))
        })
        .collect();
    let zb_version = zb::jar_version(&j.folder.join("ZombieBuddy.jar")).unwrap_or_default();
    let java_patches: Vec<(String, Vec<String>)> = resolved
        .iter()
        .filter(|x| x.enabled)
        .filter_map(|x| x.found)
        .filter_map(|m| {
            let classes = zb::mod_patches(m, &zb_version)?
                .into_iter()
                .map(|p| p.class_name)
                .collect();
            Some((m.id()?.to_string(), classes))
        })
        .collect();
    let end = diagnosis::classify_end(ended.code, ended.stopped, crash.as_ref(), &text, loaded_all);
    let version = j.version.as_ref().map(|v| v.text()).unwrap_or_default();
    let zb = zb::jar_version(&j.folder.join("ZombieBuddy.jar"));
    let workshop = j
        .library
        .as_ref()
        .and_then(|b| steam::read_workshop_state(b, PZ_APP_ID).ok());
    let stamps = mod_stamps(&resolved, workshop.as_ref());
    let good = read_last_good(cache);
    let changed = diagnosis::changed_since(good.as_ref(), &stamps);
    let game_changed = diagnosis::version_change(good.as_ref(), |g| g.game.clone(), Some(&version));
    let zb_changed = diagnosis::version_change(good.as_ref(), |g| g.zb.clone(), zb.as_deref());
    let prediction = (!report.loaded.is_empty()).then(|| {
        let v = version_of(&j);
        let order = conflicts::predicted_load(&resolved, &vis, v);
        let predicted = conflicts::predicted_overrides(&resolved, &vis, v, &|r| media.contains(r));
        let c = conflicts::check_prediction(&order, &predicted, &report.loaded, &report.overrides);
        if !c.order_ok || c.overrides_missed + c.overrides_extra > 0 {
            logging::info(&format!(
                "predicción: orden {}, {} sin cargar, {} inesperados, {} pisados no previstos, {} previstos que no pasaron",
                if c.order_ok { "igual" } else { "distinto" },
                c.not_loaded.len(),
                c.unexpected.len(),
                c.overrides_missed,
                c.overrides_extra
            ));
            for x in c.examples.iter().take(20) {
                logging::info(&format!("predicción: {x}"));
            }
        }
        c
    });
    if end == diagnosis::EndKind::Normal {
        let good = diagnosis::LastGood {
            mods: stamps,
            format: diagnosis::LAST_GOOD_FORMAT,
            game: Some(version.clone()),
            zb: zb.clone(),
        };
        let json = serde_json::to_string_pretty(&good).map_err(err)?;
        profiles::write_atomic(&cache.join(LAST_GOOD_FILE), &json).map_err(err)?;
    }
    let searching = store()?.folder(id).join(BISECT_FILE).exists();
    let vanilla = !searching
        && resolved
            .iter()
            .filter(|x| x.enabled)
            .all(|x| pzmm_core::modinfo::normalize_id(&x.id) == "ZombieBuddy");
    if vanilla
        && !text.is_empty()
        && let Err(e) = learn_noise(&issues, &version)
    {
        logging::error(&format!("ruido de la consola: {e}"));
    }
    let signature = diagnosis::signature(end, crash.as_ref(), &issues, &missing);
    let enabled: Vec<(String, Option<u64>)> = resolved
        .iter()
        .filter(|x| x.enabled)
        .map(|x| {
            let ws = x.found.and_then(|m| match m.source {
                mods::Source::Workshop { id } => Some(id),
                mods::Source::Local => None,
            });
            (x.id.clone(), ws)
        })
        .collect();
    let user = std::env::var_os("USERPROFILE").and_then(|h| {
        PathBuf::from(h)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    });
    let report = diagnosis::report_text(&diagnosis::ReportInput {
        game_version: &version,
        zb_version: zb.as_deref(),
        profile_name: &p.name,
        enabled: &enabled,
        end,
        code: ended.code,
        crash: crash.as_ref(),
        not_loaded: &missing,
        issues: &issues,
        user: user.as_deref(),
    });
    let d = Diagnosis {
        end,
        code: ended.code,
        suspects: diagnosis::suspects(
            crash.as_ref(),
            &issues,
            &missing,
            &changed,
            &packages,
            &java_patches,
        ),
        changed,
        signature,
        game_changed,
        zb_changed,
        prediction,
        not_loaded: missing
            .iter()
            .map(|m| NotLoadedDto {
                id: m.clone(),
                reason: why_not_loaded(&findings, m),
            })
            .collect(),
        crash,
        errors: issues.iter().filter(|i| i.severity == "error").count(),
        when: pzmm_core::system::now(),
        report,
    };
    let json = serde_json::to_string_pretty(&d).map_err(err)?;
    profiles::write_atomic(&cache.join(DIAGNOSIS_FILE), &json).map_err(err)?;
    logging::info(&format!(
        "diagnóstico: {:?}, {} sin cargar, {} sospechosos",
        d.end,
        d.not_loaded.len(),
        d.suspects.len()
    ));
    Ok(d)
}

fn read_diagnosis(id: &str) -> R<Option<Diagnosis>> {
    let f = store()?.cache(id).join(DIAGNOSIS_FILE);
    match std::fs::read_to_string(&f) {
        Ok(t) => Ok(serde_json::from_str(&t).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(format!("{}: {e}", f.display()))),
    }
}

#[tauri::command(async)]
fn session_last_diagnosis(id: String) -> R<Option<Diagnosis>> {
    read_diagnosis(&id)
}

#[tauri::command]
async fn session_report(id: String) -> R<String> {
    in_thread(move || match read_diagnosis(&id)? {
        Some(d) => saved_report(&d),
        None => Err("no hay diagnóstico de la última partida".into()),
    })
    .await
}

fn saved_report(d: &Diagnosis) -> R<String> {
    if d.report.is_empty() {
        return Err("no hay diagnóstico de la última partida".into());
    }
    Ok(d.report.clone())
}

const BISECT_FILE: &str = "bisect.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct BisectFile {
    origin: String,
    state: bisect::Bisect,
    #[serde(default)]
    round_started: u64,
}

fn prior_weights(d: Option<&Diagnosis>) -> std::collections::BTreeMap<String, f64> {
    let mut w = std::collections::BTreeMap::new();
    for s in d.map(|d| d.suspects.as_slice()).unwrap_or_default() {
        let x = match &s.reason {
            diagnosis::SuspectReason::CrashFrame(_) => 1000.0,
            diagnosis::SuspectReason::PatchesCrashClass(_) => 200.0,
            diagnosis::SuspectReason::LuaErrors(n) => 50.0 + 10.0 * (*n).min(20) as f64,
            diagnosis::SuspectReason::Changed => 30.0,
            diagnosis::SuspectReason::NotLoaded => 10.0,
        };
        let e = w
            .entry(pzmm_core::modinfo::normalize_id(&s.id))
            .or_insert(1.0);
        *e = f64::max(*e, x);
    }
    w
}

fn apply_enabled(p: &mut Profile, set: &[String]) {
    let on: std::collections::BTreeSet<&str> = set.iter().map(String::as_str).collect();
    for m in &mut p.mods {
        m.enabled = on.contains(pzmm_core::modinfo::normalize_id(&m.id).as_str());
    }
}

fn requires_map(vis: &[ScannedMod]) -> bisect::Requires {
    vis.iter()
        .filter_map(|m| {
            let id = m.id()?;
            let info = m.info.as_ref()?;
            Some((
                pzmm_core::modinfo::normalize_id(id),
                info.requires
                    .iter()
                    .map(|r| pzmm_core::modinfo::normalize_id(r))
                    .collect(),
            ))
        })
        .collect()
}

fn not_playing(running: Option<&str>, id: &str) -> R<()> {
    if running == Some(id) {
        return Err("la búsqueda está jugando una ronda: cierra el juego antes".into());
    }
    Ok(())
}

fn check_not_playing(app: &AppHandle, id: &str) -> R<()> {
    let e = app.state::<AppState>();
    let g = lock(&e.session);
    not_playing(g.as_ref().map(|s| s.profile.as_str()), id)
}

fn read_bisect(id: &str) -> R<BisectFile> {
    let f = store()?.folder(id).join(BISECT_FILE);
    let t =
        std::fs::read_to_string(&f).map_err(|_| "este perfil no es una búsqueda".to_string())?;
    let mut b: BisectFile = serde_json::from_str(&t).map_err(err)?;
    bisect::refresh(&mut b.state);
    Ok(b)
}

fn bisect_apply(app: &AppHandle, id: &str, f: &mut BisectFile) -> R<BisectView> {
    let e = app.state::<AppState>();
    let req = requires_map(&visible_mods(&e, false)?);
    let set = bisect::enabled_set(&f.state, &req);
    f.round_started = pzmm_core::system::now();
    let json = serde_json::to_string_pretty(f).map_err(err)?;
    profiles::write_atomic(&store()?.folder(id).join(BISECT_FILE), &json).map_err(err)?;
    modify(id, |p| {
        apply_enabled(p, &set);
        Ok(())
    })?;
    Ok(bisect_view(id, f))
}

fn bisect_view(id: &str, f: &BisectFile) -> BisectView {
    let outcome = bisect::result(&f.state);
    let suggestion = match (&outcome, read_diagnosis(id)) {
        (None, Ok(Some(d))) if d.when >= f.round_started => {
            bisect::suggest(f.state.target.as_deref(), d.end, d.signature.as_deref())
        }
        _ => None,
    };
    BisectView {
        profile: id.to_string(),
        origin: f.origin.clone(),
        suspects: bisect::remaining(&f.state),
        testing: f.state.testing.clone(),
        round: f.state.rounds.len() as u32 + 1,
        rounds_left: bisect::rounds_left(&f.state),
        outcome,
        mode: bisect::mode(&f.state),
        flaky: f.state.flaky,
        suggestion,
    }
}

static BISECT_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
async fn bisect_start(app: AppHandle, id: String, name: String, flaky: bool) -> R<BisectView> {
    in_thread(move || {
        let _g = BISECT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if find_open_bisect()?.is_some() {
            return Err("ya hay una búsqueda abierta: termínala o cancélala antes".into());
        }
        let origin = store()?.load(&id).map_err(err)?;
        let enabled: Vec<String> = origin
            .mods
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.id.clone())
            .collect();
        let e = app.state::<AppState>();
        let req = requires_map(&visible_mods(&e, false)?);
        let last = read_diagnosis(&id).ok().flatten();
        let state = bisect::start_with(
            &enabled,
            &req,
            &prior_weights(last.as_ref()),
            flaky,
            last.and_then(|d| d.signature),
        );
        let temp = duplicate_profile(&id, &name)?;
        logging::info(&format!(
            "búsqueda del culpable: «{}» → «{}», {} sospechosos{}",
            origin.name,
            temp.name,
            state.suspects.len(),
            if flaky { ", fallo intermitente" } else { "" }
        ));
        let mut f = BisectFile {
            origin: id,
            state,
            round_started: 0,
        };
        bisect_apply(&app, &temp.id, &mut f).inspect_err(|_| {
            if let Ok(s) = store() {
                let _ = trash::delete(s.folder(&temp.id));
            }
        })
    })
    .await
}

#[tauri::command]
async fn bisect_answer(
    app: AppHandle,
    id: String,
    answer: bisect::Answer,
    round: u32,
) -> R<BisectView> {
    in_thread(move || {
        check_not_playing(&app, &id)?;
        let _g = BISECT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut f = read_bisect(&id)?;
        if bisect::result(&f.state).is_some() || f.state.rounds.len() as u32 + 1 != round {
            return Ok(bisect_view(&id, &f));
        }
        bisect::answer(&mut f.state, answer);
        bisect_apply(&app, &id, &mut f)
    })
    .await
}

#[tauri::command]
async fn bisect_undo(app: AppHandle, id: String) -> R<BisectView> {
    in_thread(move || {
        check_not_playing(&app, &id)?;
        let mut f = read_bisect(&id)?;
        bisect::undo(&mut f.state);
        bisect_apply(&app, &id, &mut f)
    })
    .await
}

#[tauri::command(async)]
fn bisect_cancel(app: AppHandle, id: String) -> R<()> {
    check_not_playing(&app, &id)?;
    read_bisect(&id)?;
    trash::delete(store()?.folder(&id)).map_err(err)
}

fn find_open_bisect() -> R<Option<String>> {
    Ok(store()?
        .list()
        .into_iter()
        .map(|p| p.id)
        .find(|id| read_bisect(id).is_ok()))
}

#[tauri::command]
async fn bisect_open() -> R<Option<BisectView>> {
    in_thread(move || match find_open_bisect()? {
        Some(id) => Ok(Some(bisect_view(&id, &read_bisect(&id)?))),
        None => Ok(None),
    })
    .await
}

const CONSOLE_LINES: usize = 20_000;

fn console_file_path(id: &str) -> R<PathBuf> {
    Ok(store()?.cache(id).join("console.txt"))
}

type ConsoleCache = Option<(PathBuf, u64, Vec<u8>, String)>;
static CONSOLE_TEXT: Mutex<ConsoleCache> = Mutex::new(None);

fn read_console_profile(id: &str) -> R<String> {
    use std::io::{Read, Seek};
    let path = console_file_path(id)?;
    let mut f = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(err(format!("{}: {e}", path.display()))),
    };
    let len = f.metadata().map_err(err)?.len();
    let mut head = vec![0u8; 128.min(len as usize)];
    f.read_exact(&mut head).map_err(err)?;
    let mut c = lock(&CONSOLE_TEXT);
    if let Some((p, read, h, text)) = c.as_mut()
        && *p == path
        && *read <= len
        && *h == head
    {
        f.seek(std::io::SeekFrom::Start(*read)).map_err(err)?;
        let mut more = Vec::new();
        f.read_to_end(&mut more).map_err(err)?;
        let upto = more.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        text.push_str(&String::from_utf8_lossy(&more[..upto]));
        *read += upto as u64;
        return Ok(text.clone());
    }
    f.seek(std::io::SeekFrom::Start(0)).map_err(err)?;
    let mut all = Vec::new();
    f.read_to_end(&mut all).map_err(err)?;
    let text = String::from_utf8_lossy(&all).into_owned();
    *c = Some((path, all.len() as u64, head, text.clone()));
    Ok(text)
}

fn known_mods(app: &AppHandle, id: Option<&str>) -> R<Vec<pzmm_core::console::KnownMod>> {
    let e = app.state::<AppState>();
    let vis = visible_mods(&e, false)?;
    let to_known = |m: &ScannedMod| {
        Some(pzmm_core::console::KnownMod {
            id: m.id()?.to_string(),
            name: m
                .info
                .as_ref()
                .and_then(|i| i.name.clone())
                .unwrap_or_default(),
            folder: m
                .folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
    };
    Ok(match id {
        Some(id) => {
            let p = store()?.load(id).map_err(err)?;
            conflicts::profile_entries(&p, &vis)
                .iter()
                .filter_map(|x| x.found)
                .filter_map(to_known)
                .collect()
        }
        None => vis.iter().filter_map(to_known).collect(),
    })
}

#[tauri::command]
async fn console_read(id: String) -> R<Vec<String>> {
    in_thread(move || {
        Ok(pzmm_core::console::last_lines(
            &read_console_profile(&id)?,
            CONSOLE_LINES,
        ))
    })
    .await
}

#[tauri::command]
async fn console_issues(app: AppHandle, id: String) -> R<Vec<pzmm_core::console::Issue>> {
    in_thread(move || {
        let text = read_console_profile(&id)?;
        let playing = lock(&app.state::<AppState>().session).is_some();
        let budget = Duration::from_millis(if playing { 0 } else { 300 });
        let v = issues_of(&app, Some(&id), &text, budget)?;
        finish_attribution(&app, Some(id), v.clone());
        Ok(v)
    })
    .await
}

#[tauri::command]
async fn console_analyze(app: AppHandle, text: String) -> R<Vec<pzmm_core::console::Issue>> {
    in_thread(move || {
        let v = issues_of(&app, None, &text, Duration::from_millis(300))?;
        finish_attribution(&app, None, v.clone());
        Ok(v)
    })
    .await
}

#[tauri::command]
fn console_path(id: String) -> R<String> {
    Ok(console_file_path(&id)?.display().to_string())
}

#[tauri::command]
fn session_stop(state: State<AppState>) -> R<()> {
    if let Some(p) = lock(&state.session).as_mut() {
        p.stopped_by_user = true;
        p.child.kill().map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
async fn zb_install(app: AppHandle, channel: String) -> R<String> {
    in_thread(move || {
        let e = app.state::<AppState>();
        lock(&e.counts).clear();
        let j = game(&e)?;
        let b = j
            .library
            .as_ref()
            .ok_or("no se sabe en qué biblioteca de Steam está el juego")?;
        let id = if channel == "beta" {
            zb::WORKSHOP_BETA
        } else {
            zb::WORKSHOP_STABLE
        };
        let copy = zb::workshop_copies(&steam::workshop_content_folder(b, PZ_APP_ID))
            .into_iter()
            .find(|c| c.workshop_id == id)
            .ok_or("no estás suscrito a ZombieBuddy en el Workshop de ese canal")?;
        let install = zb::install(&j.folder, &copy.libs, &backups_folder()?, true).map_err(err)?;
        *lock(&e.game) = None;
        Ok(install.after.unwrap_or_default())
    })
    .await
}

#[tauri::command]
async fn zb_uninstall(app: AppHandle) -> R<String> {
    in_thread(move || {
        let e = app.state::<AppState>();
        lock(&e.counts).clear();
        if lock(&e.session).is_some() {
            return Err("cierra el juego antes de desinstalar ZombieBuddy".into());
        }
        let j = game(&e)?;
        let copy = zb::uninstall(
            &j.folder,
            &backups_folder()?,
            steam::steam_folder().as_deref(),
            true,
            true,
        )
        .map_err(err)?;
        *lock(&e.game) = None;
        Ok(copy
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default())
    })
    .await
}

#[tauri::command(async)]
fn backups_list() -> R<Vec<Backup>> {
    let dir = backups_folder()?;
    let mut v: Vec<Backup> = std::fs::read_dir(&dir)
        .map(|it| it.flatten().filter_map(|e| Backup::of(&e.path())).collect())
        .unwrap_or_default();
    v.sort_by_key(|c| std::cmp::Reverse(c.created));
    Ok(v)
}

#[tauri::command]
async fn backup_restore(app: AppHandle, name: String) -> R<usize> {
    in_thread(move || {
        if !pzmm_core::system::game_processes().is_empty() {
            return Err("cierra el juego antes de restaurar".into());
        }
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err("nombre de copia no válido".into());
        }
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let copy = backups_folder()?.join(&name);
        zb::check_restore(&copy, &j.folder, steam::steam_folder().as_deref()).map_err(err)?;
        let n = backups::restore(&copy, &j.folder, None).map_err(err)?;
        *lock(&e.game) = None;
        Ok(n)
    })
    .await
}

const STEAM_IDLE: Duration = Duration::from_secs(60);

fn steam_idle(e: &AppState) -> Duration {
    match e.steam_idle.load(std::sync::atomic::Ordering::Relaxed) {
        0 => STEAM_IDLE,
        s => Duration::from_secs(s),
    }
}

#[tauri::command]
fn workshop_idle(state: State<AppState>, seconds: u64) {
    state.steam_idle.store(
        seconds.clamp(10, 1800),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn with_steam<T>(app: &AppHandle, f: impl FnOnce(&mut Steamworks) -> R<T>) -> R<T> {
    let e = app.state::<AppState>();
    if lock(&e.session).is_some() || !pzmm_core::system::game_processes().is_empty() {
        return Err("cierra el juego para usar el Workshop desde aquí".into());
    }
    let folder = game(&e)?.folder;
    let mut g = lock(&e.steam);
    if g.is_none() {
        *g = Some((Steamworks::start(&folder).map_err(err)?, Instant::now()));
    }
    let (s, last_used) = g.as_mut().expect("recién abierta");
    *last_used = Instant::now();
    f(s)
}

fn watch_steam(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let e = app.state::<AppState>();
            let Ok(mut g) = e.steam.lock() else { continue };
            match g.as_ref() {
                Some((_, last_used)) if last_used.elapsed() > steam_idle(&e) => *g = None,
                Some((s, _)) => s.pump(),
                None => {}
            }
        }
    });
}

#[tauri::command]
fn workshop_tags() -> Vec<&'static str> {
    steamworks::TAGS.to_vec()
}

#[tauri::command]
async fn workshop_search(app: AppHandle, query: steamworks::Query) -> R<WorkshopPage> {
    in_thread(move || {
        with_steam(&app, |s| {
            let p = s.search(&query).map_err(err)?;
            Ok(WorkshopPage::of(p, |id| s.state(id)))
        })
    })
    .await
}

#[tauri::command]
async fn workshop_details(
    app: AppHandle,
    ids: Vec<u64>,
    complete: bool,
) -> R<Vec<WorkshopDetails>> {
    in_thread(move || {
        with_steam(&app, |s| {
            let o = steamworks::Options {
                long: complete,
                names: false,
            };
            let v = s.details(&ids, o).map_err(err)?;
            Ok(v.into_iter()
                .map(|e| {
                    let est = s.state(e.id);
                    WorkshopDetails::of(e, est)
                })
                .collect())
        })
    })
    .await
}

#[tauri::command]
async fn workshop_collection(app: AppHandle, id: u64) -> R<WorkshopCollection> {
    in_thread(move || {
        with_steam(&app, |s| {
            let c = s
                .details(
                    &[id],
                    steamworks::Options {
                        long: true,
                        names: true,
                    },
                )
                .map_err(err)?
                .into_iter()
                .find(|e| e.id == id && e.collection)
                .ok_or("esa colección no existe o es privada")?;
            let children = s
                .details(
                    &c.children,
                    steamworks::Options {
                        long: false,
                        names: false,
                    },
                )
                .map_err(err)?;
            let missing: Vec<u64> = c
                .children
                .iter()
                .filter(|h| !children.iter().any(|x| x.id == **h && !x.title.is_empty()))
                .copied()
                .collect();
            let est = s.state(c.id);
            let mut card = WorkshopDetails::of(c, est);
            if card.item.preview_view.is_empty()
                && let Some(h) = children.iter().find(|h| !h.preview_view.is_empty())
            {
                card.item.preview_view = h.preview_view.clone();
            }
            Ok(WorkshopCollection {
                collection: card,
                items: children
                    .into_iter()
                    .filter(|e| !e.title.is_empty())
                    .map(|e| {
                        let est = s.state(e.id);
                        WorkshopItem::of(e, est)
                    })
                    .collect(),
                missing,
            })
        })
    })
    .await
}

#[tauri::command]
async fn workshop_check(app: AppHandle, ids: Vec<u64>) -> R<Vec<(u64, u32)>> {
    in_thread(move || with_steam(&app, |s| s.check(&ids).map_err(err))).await
}

#[tauri::command(async)]
fn workshop_cache_read() -> R<Option<String>> {
    Ok(std::fs::read_to_string(app_data("workshop.json")?).ok())
}

#[tauri::command]
async fn workshop_cache_write(data: String) -> R<()> {
    in_thread(move || {
        let f = app_data("workshop.json")?;
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d).map_err(err)?;
        }
        pzmm_core::profiles::write_atomic(&f, &data).map_err(err)
    })
    .await
}

#[tauri::command]
async fn workshop_states(app: AppHandle, ids: Vec<u64>) -> R<Vec<steamworks::LocalState>> {
    in_thread(move || with_steam(&app, |s| Ok(ids.iter().map(|id| s.state(*id)).collect()))).await
}

#[tauri::command]
async fn workshop_download(app: AppHandle, ids: Vec<u64>) -> R<Vec<u64>> {
    in_thread(move || {
        with_steam(&app, |s| {
            Ok(ids.iter().copied().filter(|id| !s.download(*id)).collect())
        })
    })
    .await
}

#[tauri::command]
async fn workshop_subscribe(app: AppHandle, ids: Vec<u64>) -> R<Vec<(u64, String)>> {
    in_thread(move || {
        with_steam(&app, |s| {
            Ok(ids
                .iter()
                .filter_map(|id| s.subscribe(*id).err().map(|e| (*id, e.to_string())))
                .collect())
        })
    })
    .await
}

#[tauri::command]
async fn workshop_unsubscribe(app: AppHandle, id: u64) -> R<()> {
    in_thread(move || with_steam(&app, |s| s.unsubscribe(id).map_err(err))).await
}

#[tauri::command]
async fn profile_touch(id: String) -> R<()> {
    in_thread(move || {
        modify(&id, |p| {
            p.last_use = Some(pzmm_core::system::now());
            Ok(())
        })
    })
    .await
}

#[tauri::command(async)]
fn featured_read() -> R<Option<Featured>> {
    let f = app_data("destacados.json")?;
    match std::fs::read_to_string(&f) {
        Ok(t) => Ok(serde_json::from_str(&t).ok()),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
async fn featured_refresh(app: AppHandle) -> R<Featured> {
    in_thread(move || {
        let read = |collections: bool| -> R<Vec<FeaturedItem>> {
            with_steam(&app, |s| {
                let c = steamworks::Query {
                    text: String::new(),
                    order: steamworks::Order::Trend,
                    tags: if collections {
                        Vec::new()
                    } else {
                        vec!["Build 42".into()]
                    },
                    page: 1,
                    collections,
                };
                let p = s.search(&c).map_err(err)?;
                Ok(WorkshopPage::of(p, |id| s.state(id))
                    .items
                    .into_iter()
                    .take(12)
                    .map(|e| FeaturedItem {
                        id: e.id,
                        title: e.title,
                        author: e.author,
                        subscribers: e.subscribers,
                        preview_view: e.preview_view,
                        collection: e.collection,
                        children: e.children.len(),
                    })
                    .collect())
            })
        };
        let d = Featured {
            parsed: pzmm_core::system::now(),
            mods: read(false)?,
            collections: read(true)?,
        };
        let f = app_data("destacados.json")?;
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        profiles::write_atomic(&f, &serde_json::to_string(&d).map_err(err)?).map_err(err)?;
        Ok(d)
    })
    .await
}

#[tauri::command(async)]
fn logs_folder() -> R<String> {
    let d = logging::folder().ok_or("no se encuentra %APPDATA%")?;
    std::fs::create_dir_all(&d).map_err(err)?;
    Ok(d.display().to_string())
}

#[tauri::command(async)]
fn profile_duplicate(id: String, name: String) -> R<ProfileSummary> {
    Ok(ProfileSummary::of(&duplicate_profile(&id, &name)?))
}

fn duplicate_profile(id: &str, name: &str) -> R<Profile> {
    let a = store()?;
    let origin = a.load(id).map_err(err)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("el perfil necesita un nombre".into());
    }
    let mut p = a.create(name).map_err(err)?;
    p.mods = origin.mods.clone();
    p.sections = origin.sections.clone();
    p.maps = origin.maps.clone();
    p.server_excluded_maps = origin.server_excluded_maps.clone();
    p.launch = origin.launch.clone();
    p.zombiebuddy = origin.zombiebuddy.clone();
    let copy = || -> R<()> {
        a.save(&p).map_err(err)?;
        let (of, a_) = (a.cache(&origin.id), a.cache(&p.id));
        let _ = std::fs::copy(of.join("options.ini"), a_.join("options.ini"));
        for sub in ["Server", "Sandbox Presets"] {
            copy_folder(&of.join(sub), &a_.join(sub)).map_err(err)?;
        }
        Ok(())
    };
    if let Err(e) = copy() {
        let _ = trash::delete(a.folder(&p.id));
        return Err(e);
    }
    logging::info(&format!(
        "perfil duplicado: «{}» → «{}»",
        origin.name, p.name
    ));
    Ok(p)
}

fn copy_folder(of: &std::path::Path, a: &std::path::Path) -> std::io::Result<()> {
    if !of.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(a)?;
    for e in std::fs::read_dir(of)?.flatten() {
        let path = e.path();
        if path.is_dir() {
            copy_folder(&path, &a.join(e.file_name()))?;
        } else {
            std::fs::copy(&path, a.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn workshop_of_key(key: &str) -> Option<u64> {
    key.strip_prefix("ws:")?.split('/').next()?.parse().ok()
}

#[tauri::command(async)]
fn log_ui_error(text: String) {
    logging::error(&format!("interfaz: {text}"));
}

#[tauri::command(async)]
fn profile_share(id: String) -> R<String> {
    let p = store()?.load(&id).map_err(err)?;
    let c = SharedProfile {
        format: SHARED_FORMAT.into(),
        version: 1,
        name: p.name.clone(),
        mods: p
            .mods
            .iter()
            .map(|m| SharedMod {
                id: m.id.clone(),
                key: m.key.clone(),
                workshop_id: workshop_of_key(&m.key),
                enabled: m.enabled,
                pinned: m.pinned,
                section: m.section.clone(),
            })
            .collect(),
        maps: p.maps.clone(),
        sections: p.sections.clone(),
        server_excluded_maps: p.server_excluded_maps.clone(),
        launch: profiles::LaunchSettings {
            xmx: p.launch.xmx.clone(),
            modfolders: p.launch.modfolders.clone(),
            ..Default::default()
        },
        zombiebuddy: Default::default(),
    };
    let dir =
        pzmm_core::system::downloads_folder().ok_or("no se encuentra la carpeta Descargas")?;
    let base: String = p
        .name
        .chars()
        .map(|ch| if r#"\/:*?"<>|"#.contains(ch) { '_' } else { ch })
        .collect();
    let mut path = dir.join(format!("{}.pzprofile", base.trim()));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{} ({n}).pzprofile", base.trim()));
        n += 1;
    }
    let text = serde_json::to_string_pretty(&c).map_err(err)?;
    std::fs::write(&path, text).map_err(err)?;
    logging::info(&format!(
        "perfil compartido: «{}» → {}",
        p.name,
        path.display()
    ));
    Ok(path.display().to_string())
}

#[tauri::command]
async fn profile_import(app: AppHandle, text: String) -> R<Imported> {
    in_thread(move || {
        let c: SharedProfile = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("no es un perfil de PZ Mod Manager: {e}"))?;
        if c.format != SHARED_FORMAT {
            return Err("no es un perfil de PZ Mod Manager".into());
        }
        if c.version > 1 {
            return Err("el perfil es de una versión más nueva de PZ Mod Manager".into());
        }
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        let a = store()?;
        let name = if c.name.trim().is_empty() {
            "Perfil importado"
        } else {
            c.name.trim()
        };
        let mut p = a.create(name).map_err(err)?;
        let mut missing: Vec<u64> = Vec::new();
        let norm = pzmm_core::modinfo::normalize_id;
        for m in &c.mods {
            if !profiles::is_valid_id(&m.id) || p.mods.iter().any(|x| norm(&x.id) == norm(&m.id)) {
                continue;
            }
            let local = vis
                .iter()
                .find(|v| v.key == m.key)
                .or_else(|| vis.iter().find(|v| v.id().map(norm) == Some(norm(&m.id))));
            if local.is_none()
                && let Some(w) = m.workshop_id.or_else(|| workshop_of_key(&m.key))
                && !missing.contains(&w)
            {
                missing.push(w);
            }
            p.mods.push(ModEntry {
                key: local
                    .map(|v| v.key.clone())
                    .unwrap_or_else(|| m.key.clone()),
                id: m.id.clone(),
                enabled: m.enabled,
                pinned: m.pinned,
                section: m.section.clone(),
            });
        }
        p.sections = c.sections.clone();
        p.sanitize_sections();
        p.maps = c
            .maps
            .into_iter()
            .filter(|m| profiles::is_valid_map(m))
            .collect();
        p.server_excluded_maps = c
            .server_excluded_maps
            .into_iter()
            .filter(|m| profiles::is_valid_map(m) && m != profiles::BASE_MAP)
            .collect();
        let (launch, ignored) = c.launch.from_foreign();
        p.launch = launch;
        if let Err(e) = a.save(&p) {
            let _ = trash::delete(a.folder(&p.id));
            return Err(err(e));
        }
        logging::info(&format!(
            "perfil importado: «{}» ({} mods, {} sin descargar)",
            p.name,
            p.mods.len(),
            missing.len()
        ));
        if !ignored.is_empty() {
            logging::info(&format!(
                "perfil importado: argumentos de arranque descartados: {}",
                ignored.join(" ")
            ));
        }
        Ok(Imported {
            profile: ProfileSummary::of(&p),
            missing,
            ignored,
        })
    })
    .await
}

#[tauri::command]
async fn mod_trash(app: AppHandle, key: String) -> R<()> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let vis = visible_mods(&e, false)?;
        let m = vis
            .iter()
            .find(|m| m.key == key)
            .ok_or("mod no encontrado")?;
        if !matches!(m.source, mods::Source::Local) {
            return Err("los mods del Workshop se quitan anulando la suscripción".into());
        }
        let base = user_mods_folder().ok_or("no se encuentra la carpeta mods")?;
        if m.folder.parent() != Some(base.as_path()) {
            return Err("el mod no está en la carpeta mods del usuario".into());
        }
        let md = std::fs::symlink_metadata(&m.folder).map_err(err)?;
        if md.file_type().is_symlink() || !md.is_dir() {
            return Err("no es una carpeta normal (¿un enlace?): ábrela y quítala a mano".into());
        }
        trash::delete(&m.folder).map_err(err)?;
        logging::info(&format!("mod local a la papelera: {}", m.folder.display()));
        *lock(&e.mods) = None;
        Ok(())
    })
    .await
}

#[tauri::command]
async fn profile_count(app: AppHandle, id: String) -> R<Count> {
    in_thread(move || {
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let vis = profile_mods(&e, &id)?;
        let p = store()?.load(&id).map_err(err)?;
        let signature = signature_count(&p, &vis);
        if let Some((f, r)) = lock(&e.counts).get(&id)
            && *f == signature
        {
            return Ok(r.clone());
        }
        let resolved = conflicts::profile_entries(&p, &vis);
        let media = game_media(&e)?;
        let ctx = context(&j, &p, &media);
        let start = Instant::now();
        let active = resolved.iter().filter(|x| x.enabled).count();
        logging::info(&format!("recuento de {id}: {active} activos"));
        let r = Count::of(&p, &conflicts::analyze(&resolved, &vis, &ctx));
        logging::info(&format!("recuento de {id}: hecho en {:?}", start.elapsed()));
        e.counts
            .lock()
            .map_err(err)?
            .insert(id, (signature, r.clone()));
        Ok(r)
    })
    .await
}

#[derive(Clone, serde::Serialize)]
struct SandboxProgress {
    stage: &'static str,
    done: usize,
    total: usize,
}

fn definition_sandbox(
    app: &AppHandle,
    id: &str,
    language: &str,
) -> R<(Arc<pzmm_core::sandbox::Definition>, Profile)> {
    let e = app.state::<AppState>();
    let j = game(&e)?;
    let vis = visible_mods(&e, false)?;
    let p = store()?.load(id).map_err(err)?;
    let resolved = conflicts::profile_entries(&p, &vis);
    let mods: Vec<&ScannedMod> = resolved
        .iter()
        .filter(|x| x.enabled)
        .filter_map(|x| x.found)
        .collect();
    let key = format!(
        "{id}|{language}|{}",
        mods.iter()
            .map(|m| m.key.as_str())
            .collect::<Vec<_>>()
            .join(";")
    );
    let mut g = lock(&e.sandbox);
    if let Some((k, d)) = g.as_ref()
        && *k == key
    {
        return Ok((d.clone(), p));
    }
    let d = Arc::new(
        pzmm_core::sandbox::definition_with_progress(
            &j.folder,
            &mods,
            language,
            &mut |stage, done, total| {
                let _ = app.emit("sandbox-progress", SandboxProgress { stage, done, total });
            },
        )
        .map_err(err)?,
    );
    *g = Some((key, d.clone()));
    Ok((d, p))
}

fn language_tr(language: &str) -> &'static str {
    if language == "en" { "EN" } else { "ES" }
}

#[tauri::command]
async fn sandbox_schema(app: AppHandle, id: String, language: String) -> R<SandboxSchema> {
    in_thread(move || {
        let (def, _) = definition_sandbox(&app, &id, language_tr(&language))?;
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let cache = store()?.cache(&id);
        let user_presets = std::env::var_os("USERPROFILE")
            .map(|h| PathBuf::from(h).join("Zomboid").join("Sandbox Presets"));
        Ok(SandboxSchema {
            schema: def.schema(),
            game_presets: pzmm_core::sandbox::game_presets(&j.folder),
            profile_presets: pzmm_core::sandbox::names_with(&cache.join("Sandbox Presets"), ".cfg"),
            user_presets: user_presets
                .map(|d| pzmm_core::sandbox::names_with(&d, ".cfg"))
                .unwrap_or_default(),
            servers: pzmm_core::sandbox::names_with(&cache.join("Server"), ".ini"),
        })
    })
    .await
}

#[tauri::command]
async fn sandbox_values(
    app: AppHandle,
    id: String,
    language: String,
    origin: String,
) -> R<SandboxValues> {
    use pzmm_core::sandbox::{self as sb, Definition};
    in_thread(move || {
        let (def, _) = definition_sandbox(&app, &id, language_tr(&language))?;
        let e = app.state::<AppState>();
        let j = game(&e)?;
        let cache = store()?.cache(&id);
        let read =
            |p: PathBuf| std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()));
        let valid = |n: &str| -> R<String> {
            if sb::is_valid_name(n) {
                Ok(n.trim().to_string())
            } else {
                Err("nombre no válido".into())
            }
        };
        let (sandbox, server) = match origin.split_once(':') {
            None if origin == "default" => (Default::default(), Default::default()),
            Some(("game", n)) => (
                sb::read_lua(&read(
                    j.folder
                        .join("media/lua/shared/Sandbox")
                        .join(format!("{}.lua", valid(n)?)),
                )?),
                Default::default(),
            ),
            Some(("profile", n)) => (
                sb::read_ini(&read(
                    cache
                        .join("Sandbox Presets")
                        .join(format!("{}.cfg", valid(n)?)),
                )?),
                Default::default(),
            ),
            Some(("user", n)) => {
                let h = std::env::var_os("USERPROFILE").ok_or("sin carpeta de usuario")?;
                (
                    sb::read_ini(&read(
                        PathBuf::from(h)
                            .join("Zomboid/Sandbox Presets")
                            .join(format!("{}.cfg", valid(n)?)),
                    )?),
                    Default::default(),
                )
            }
            Some(("server", n)) => {
                let n = valid(n)?;
                let srv = cache.join("Server");
                let lua = read(srv.join(format!("{n}_SandboxVars.lua"))).unwrap_or_default();
                (
                    sb::read_lua(&lua),
                    sb::read_ini(&read(srv.join(format!("{n}.ini")))?),
                )
            }
            _ => return Err("origen desconocido".into()),
        };
        Ok(SandboxValues {
            sandbox: Definition::complete(&def.sandbox, &sandbox),
            server: Definition::complete(&def.server, &server),
        })
    })
    .await
}

fn server_with_profile(
    app: &AppHandle,
    p: &Profile,
    server: &std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
) -> R<(
    std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
    String,
)> {
    use pzmm_core::sandbox::Value;
    let e = app.state::<AppState>();
    let vis = visible_mods(&e, false)?;
    let resolved = conflicts::profile_entries(p, &vis);
    let x = Export::of(p, &resolved);
    let active: Vec<&ExportRow> = x.mods.iter().filter(|m| m.enabled).collect();
    let mut ws: Vec<u64> = Vec::new();
    for m in &active {
        if let Some(w) = m.workshop_id
            && !ws.contains(&w)
        {
            ws.push(w);
        }
    }
    let join = |v: Vec<String>| v.join(";");
    let mut srv = server.clone();
    srv.insert(
        "Mods".into(),
        Value::Text(join(active.iter().map(|m| m.id.clone()).collect())),
    );
    srv.insert(
        "WorkshopItems".into(),
        Value::Text(join(ws.iter().map(|w| w.to_string()).collect())),
    );
    srv.insert(
        "Map".into(),
        Value::Text(join(
            x.maps
                .iter()
                .filter(|m| m.server)
                .map(|m| m.folder.clone())
                .collect(),
        )),
    );
    let mut list = String::from("# Puesto\tMod ID\tWorkshop ID\tNombre\r\n");
    for (i, m) in active.iter().enumerate() {
        let w = m.workshop_id.map(|w| w.to_string()).unwrap_or_default();
        list.push_str(&format!("{}\t{}\t{w}\t{}\r\n", i + 1, m.id, m.name));
    }
    Ok((srv, list))
}

#[tauri::command]
async fn sandbox_texts(
    app: AppHandle,
    id: String,
    language: String,
    sandbox: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
    server: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
) -> R<SandboxTexts> {
    in_thread(move || {
        let (def, p) = definition_sandbox(&app, &id, language_tr(&language))?;
        let (srv, mods) = server_with_profile(&app, &p, &server)?;
        Ok(SandboxTexts {
            ini: def.ini_text(&srv),
            lua: def.lua_text(&sandbox),
            cfg: def.cfg_text(&sandbox),
            mods,
        })
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn sandbox_save(
    app: AppHandle,
    id: String,
    language: String,
    name: String,
    sandbox: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
    server: std::collections::BTreeMap<String, pzmm_core::sandbox::Value>,
    as_server: bool,
    as_preset: bool,
) -> R<Vec<String>> {
    use pzmm_core::sandbox as sb;
    in_thread(move || {
        if !sb::is_valid_name(&name) {
            return Err(r#"el nombre no puede estar vacío ni llevar / \ : ; . " < > | ? *"#.into());
        }
        let name = name.trim().to_string();
        let (def, p) = definition_sandbox(&app, &id, language_tr(&language))?;
        let cache = store()?.cache(&id);
        let mut written = Vec::new();
        let write = |path: PathBuf, text: String, written: &mut Vec<String>| -> R<()> {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).map_err(err)?;
            }
            if path.is_file() {
                let mut bak = path.clone().into_os_string();
                bak.push(".bak");
                std::fs::copy(&path, PathBuf::from(bak)).map_err(err)?;
            }
            profiles::write_atomic(&path, &text).map_err(err)?;
            written.push(path.display().to_string());
            Ok(())
        };
        if as_server {
            let (srv, _) = server_with_profile(&app, &p, &server)?;
            let dir = cache.join("Server");
            write(
                dir.join(format!("{name}.ini")),
                def.ini_text(&srv),
                &mut written,
            )?;
            write(
                dir.join(format!("{name}_SandboxVars.lua")),
                def.lua_text(&sandbox),
                &mut written,
            )?;
        }
        if as_preset {
            write(
                cache.join("Sandbox Presets").join(format!("{name}.cfg")),
                def.cfg_text(&sandbox),
                &mut written,
            )?;
        }
        Ok(written)
    })
    .await
}

#[tauri::command]
fn open_workshop(app: AppHandle, id: u64) -> R<()> {
    app.opener()
        .open_url(zb::steam_workshop_url(id), None::<&str>)
        .map_err(err)
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> R<()> {
    let u = url.trim();
    if !(u.starts_with("https://") || u.starts_with("http://")) || u.contains(char::is_whitespace) {
        return Err("solo se abren enlaces web (http o https)".into());
    }
    app.opener().open_url(u, None::<&str>).map_err(err)
}

#[tauri::command]
fn open_workshop_web(app: AppHandle) -> R<()> {
    app.opener()
        .open_url(
            format!("https://steamcommunity.com/app/{PZ_APP_ID}/workshop/"),
            None::<&str>,
        )
        .map_err(err)
}

#[tauri::command]
fn open_folder(app: AppHandle, path: String) -> R<()> {
    app.opener().reveal_item_in_dir(&path).map_err(err)
}

static FOCUS_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn focus_window(app: &AppHandle, focused: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WAITING: AtomicBool = AtomicBool::new(false);
    static FOCUSED: AtomicBool = AtomicBool::new(true);
    FOCUS_TURN.fetch_add(1, Ordering::SeqCst);
    FOCUSED.store(focused, Ordering::SeqCst);
    if focused {
        memory_level(app, false);
        return;
    }
    if WAITING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let turn = FOCUS_TURN.load(Ordering::SeqCst);
            std::thread::sleep(Duration::from_secs(30));
            if FOCUSED.load(Ordering::SeqCst) {
                break;
            }
            if FOCUS_TURN.load(Ordering::SeqCst) == turn {
                memory_level(&app, true);
                break;
            }
        }
        WAITING.store(false, Ordering::SeqCst);
    });
}

#[cfg(windows)]
fn memory_level(app: &AppHandle, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
        ICoreWebView2_19,
    };
    use windows_core::Interface;
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let level = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let _ = w.with_webview(move |pw| {
        // SAFETY: COM interfaces of the view Tauri provides; if the installed WebView2 does not have
        // ICoreWebView2_19 (before 1.0.1823.32), nothing is done.
        unsafe {
            if let Ok(core) = pw.controller().CoreWebView2()
                && let Ok(v19) = core.cast::<ICoreWebView2_19>()
            {
                let _ = v19.SetMemoryUsageTargetLevel(level);
            }
        }
    });
}

#[cfg(not(windows))]
fn memory_level(_app: &AppHandle, _low: bool) {}

static WEBVIEW_VISIBLE: Mutex<Option<bool>> = Mutex::new(None);

fn visibility_change(visible: Option<bool>, minimized: bool) -> Option<bool> {
    (visible != Some(!minimized)).then_some(!minimized)
}

fn sync_webview_visibility(window: &tauri::Window) {
    let minimized = window.is_minimized().unwrap_or(false);
    let Ok(mut g) = WEBVIEW_VISIBLE.lock() else {
        return;
    };
    if let Some(visible) = visibility_change(*g, minimized) {
        *g = Some(visible);
        webview_visible(window.app_handle(), visible);
    }
}

#[cfg(windows)]
fn webview_visible(app: &AppHandle, visible: bool) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.with_webview(move |pw| {
        // SAFETY: the controller Tauri provides for this view, used from its own thread.
        unsafe {
            let _ = pw.controller().SetIsVisible(visible);
        }
    });
}

#[cfg(not(windows))]
fn webview_visible(_app: &AppHandle, _visible: bool) {}

#[cfg(windows)]
fn watch_webview(app: &AppHandle) {
    use std::sync::atomic::{AtomicU32, Ordering};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    };
    static RELOADS: AtomicU32 = AtomicU32::new(0);
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.with_webview(|pw| {
        let handler = webview2_com::ProcessFailedEventHandler::create(Box::new(|core, args| {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            if let Some(a) = args {
                // SAFETY: the event's own arguments, read inside its callback.
                unsafe { a.ProcessFailedKind(&mut kind)? };
            }
            logging::error(&format!("WebView2: falló un proceso (tipo {})", kind.0));
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                && RELOADS.fetch_add(1, Ordering::Relaxed) < 3
                && let Some(c) = core
            {
                // SAFETY: the view that raised the event, used from its own thread.
                unsafe { c.Reload()? };
            }
            Ok(())
        }));
        // SAFETY: the controller Tauri provides for this view, used from its own thread.
        unsafe {
            if let Ok(core) = pw.controller().CoreWebView2() {
                let mut token = 0;
                let _ = core.add_ProcessFailed(&handler, &mut token);
            }
        }
    });
}

#[cfg(not(windows))]
fn watch_webview(_app: &AppHandle) {}

pub fn run() {
    logging::watch_panics();
    logging::info(concat!(
        "PZ Mod Manager ",
        env!("CARGO_PKG_VERSION"),
        " arranca"
    ));
    let mut settings = load_settings();
    clear_webview_if_pending(&mut settings);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            settings: Mutex::new(settings),
            ..AppState::default()
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(focused) => focus_window(window.app_handle(), *focused),
            tauri::WindowEvent::Resized(_) => sync_webview_visibility(window),
            _ => {}
        })
        .register_asynchronous_uri_scheme_protocol("pzmap", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let body = app
                    .try_state::<AppState>()
                    .and_then(|e| maps_view::serve(&e.map_sources, &path));
                let response = match body {
                    Some(b) => tauri::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=3600")
                        .body(b),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new()),
                };
                if let Ok(r) = response {
                    responder.respond(r);
                }
            });
        })
        .setup(|app| {
            watch_mods(app.handle().clone());
            watch_steam(app.handle().clone());
            watch_webview(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            environment,
            choose_game,
            settings_get,
            settings_set,
            folders_info,
            steam_profile,
            maintenance_info,
            maintenance_clear,
            maintenance_webview,
            rescan,
            profiles,
            profiles_unreadable,
            log_ui_error,
            profile_create,
            profile_rename,
            profile_delete,
            profile_set_enabled,
            profile_set_enabled_many,
            profile_game_list,
            profile_take_game_list,
            profile_discard_game_list,
            profile_pin,
            profile_move,
            section_create,
            section_rename,
            section_move,
            section_delete,
            section_assign,
            sections_from_categories,
            profile_restore_layout,
            saves_gaps,
            saves_add,
            saves_restore,
            saves_ignore,
            profile_add,
            profile_remove,
            profile_remove_many,
            profile_put_back,
            profile_win,
            workshop_check,
            workshop_cache_read,
            workshop_cache_write,
            profile_accept,
            profile_settings,
            probe_report,
            profile_view,
            profile_sort,
            profile_export,
            sandbox_schema,
            sandbox_values,
            sandbox_save,
            sandbox_texts,
            profile_touch,
            profile_duplicate,
            profile_share,
            profile_import,
            profile_count,
            mod_trash,
            logs_folder,
            console_read,
            console_issues,
            console_analyze,
            console_path,
            featured_read,
            featured_refresh,
            profile_maps,
            profile_maps_view,
            library,
            mod_details,
            launch,
            session_stop,
            session_last_diagnosis,
            session_report,
            bisect_start,
            bisect_answer,
            bisect_undo,
            bisect_cancel,
            bisect_open,
            zb_install,
            zb_uninstall,
            backups_list,
            backup_restore,
            workshop_tags,
            workshop_search,
            workshop_details,
            workshop_collection,
            workshop_states,
            workshop_waiting,
            workshop_download,
            workshop_subscribe,
            workshop_unsubscribe,
            workshop_idle,
            open_workshop,
            open_workshop_web,
            open_url,
            open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo abrir la ventana");
}
