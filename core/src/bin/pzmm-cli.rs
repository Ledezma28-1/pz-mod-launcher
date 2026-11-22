use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use pzmm_core::game::{self, Game, Modification};
use pzmm_core::mods::{self, Layout, ScannedMod, Source};
use pzmm_core::profiles::{self, ModEntry, ProfileStore};
use pzmm_core::version::Version;
use pzmm_core::{PZ_APP_ID, backups, conflicts, integrity, launch, order, steam, zb};

const USAGE: &str = "uso: pzmm-cli env | scan | profile create|list|mods|import | cmdline <id> | launch <id> | zb status|install|uninstall | backup restore | order | conflicts | predict <id> <console.txt> | patches [<id>] [--zb <v>] | probe <id> [--on|--off] | maps <id> | lotpack <folder> <x> <y> [<folder2>] | export | sandbox | workshop | console | integrity";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let r = match args.first().map(String::as_str) {
        Some("env") => environment(),
        Some("scan") => scan(rest),
        Some("profile") => profile(rest),
        Some("cmdline") => line(rest),
        Some("launch") => launch(rest),
        Some("zb") => zb_order(rest),
        Some("backup") => copy(rest),
        Some("order") => order_cli(rest),
        Some("conflicts") => conflicts_cli(rest),
        Some("overrides") => overrides_cli(rest),
        Some("predict") => predict_cli(rest),
        Some("patches") => patches_cli(rest),
        Some("probe") => probe_cli(rest),
        Some("lotpack") => lotpack_cli(rest),
        Some("maps") => maps_cli(rest),
        Some("workshop") => workshop_cli(rest),
        Some("export") => export_cli(rest),
        Some("sandbox") => sandbox_cli(rest),
        Some("integrity") => integrity_cli(),
        Some("console") => console_cli(rest),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn environment() -> pzmm_core::CoreResult<()> {
    let j = game::detect()?;
    println!("Juego        {}", j.folder.display());
    if let Some(m) = &j.manifest {
        println!(
            "Steam        build {} · rama {}",
            m.buildid,
            m.branch.as_deref().unwrap_or("?")
        );
    }
    match &j.version {
        Some(v) => println!("Versión      {} ({})", v.version, v.text()),
        None => println!("Versión      no se pudo leer de projectzomboid.jar"),
    }
    println!("Java         {}", j.java.as_deref().unwrap_or("?"));
    if let Some(l) = &j.launcher {
        println!(
            "Lanzador     {} · classpath {:?} · {} vmArgs",
            l.class_main,
            l.classpath,
            l.vm_args.len()
        );
    }
    let in_game = zb::jar_version(&j.folder.join("ZombieBuddy.jar"));
    println!(
        "ZombieBuddy  en el juego: {}",
        in_game.as_deref().unwrap_or("no")
    );
    if let Some(b) = &j.library {
        for c in zb::workshop_copies(&steam::workshop_content_folder(b, PZ_APP_ID)) {
            println!(
                "             Workshop {} ({}): {} · dll {}",
                c.channel,
                c.workshop_id,
                c.version.as_deref().unwrap_or("?"),
                if c.dll { "sí" } else { "no" }
            );
        }
    }
    if let (Some(v), Some(zbv)) = (&j.version, &in_game)
        && let Some(warning) = zb::incompatible_with_game(v.version, zbv)
    {
        println!("  ¡AVISO!    {warning}");
    }
    println!("Modificaciones de otras herramientas:");
    if j.modifications.is_empty() {
        println!("  ninguna");
    }
    for m in &j.modifications {
        match m {
            Modification::ZombieBuddy {
                jar,
                dll,
                jar_new_pending,
            } => {
                println!("  ZombieBuddy: jar {jar}, dll {dll}, .new pendiente {jar_new_pending}")
            }
            Modification::ArgJson { arg } => {
                println!("  ProjectZomboid64.json: {arg}")
            }
            Modification::BatOptions { value } => {
                println!("  ProjectZomboid64.bat: _JAVA_OPTIONS={value}")
            }
            Modification::Pzopt { files, revision } => {
                println!(
                    "  pzopt: {files} archivos, para la revisión {}",
                    revision.as_deref().unwrap_or("?")
                )
            }
            Modification::LooseClasses { folder, files } => {
                println!("  clases sueltas en {folder}\\: {files}")
            }
        }
    }
    Ok(())
}

fn scan(args: &[String]) -> pzmm_core::CoreResult<()> {
    let j = game::detect()?;
    let version = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    let cache = args
        .iter()
        .position(|a| a == "--cache")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Zomboid")));
    let mut all_mods: Vec<ScannedMod> = Vec::new();
    if let Some(b) = &j.library {
        all_mods.extend(mods::scan_workshop(
            &steam::workshop_content_folder(b, PZ_APP_ID),
            version,
        ));
    }
    if let Some(c) = &cache {
        all_mods.extend(mods::scan_folder(
            &c.join("mods"),
            Source::Local,
            "local:",
            version,
        ));
    }
    if args.iter().any(|a| a == "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&all_mods).unwrap_or_default()
        );
        return Ok(());
    }

    let items: std::collections::BTreeSet<u64> = all_mods
        .iter()
        .filter_map(|m| match m.source {
            Source::Workshop { id } => Some(id),
            _ => None,
        })
        .collect();
    println!(
        "Juego {version}: {} elementos del Workshop, {} mods en total",
        items.len(),
        all_mods.len()
    );
    let mut by_layout: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_folder: BTreeMap<String, usize> = BTreeMap::new();
    for m in &all_mods {
        let (e, c) = match &m.layout {
            Layout::B42 { folder_version, .. } => (
                "B42",
                folder_version
                    .clone()
                    .unwrap_or_else(|| "(solo common)".into()),
            ),
            Layout::OnlyB41 => ("solo B41", "-".into()),
            Layout::Empty => ("vacía", "-".into()),
        };
        *by_layout.entry(e.into()).or_default() += 1;
        *by_folder.entry(c).or_default() += 1;
    }
    println!("Estructura   {by_layout:?}");
    println!("Carpeta que usará el juego: {by_folder:?}");
    let errors: Vec<&ScannedMod> = all_mods.iter().filter(|m| m.error.is_some()).collect();
    println!("Con problemas: {}", errors.len());
    for m in errors.iter().take(15) {
        println!("  {:45} {}", m.key, m.error.as_deref().unwrap_or(""));
    }

    let mut ids: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for m in &all_mods {
        if let Some(id) = m.id() {
            ids.entry(id).or_default().push(&m.key);
        }
    }
    let repeated: Vec<_> = ids.iter().filter(|(_, v)| v.len() > 1).collect();
    println!("IDs repetidos: {}", repeated.len());
    for (id, keys) in repeated.iter().take(10) {
        println!("  {id}: {}", keys.join(", "));
    }

    println!("Mods Java (JAR que elegiría ZB 2.3.2 / 2.3.4 / 3.0.0):");
    let cfg = zb::config_folder();
    let approval = cfg.as_deref().map(zb::read_approvals).unwrap_or_default();
    let conflict = cfg.as_deref().map(zb::trusted_authors).unwrap_or_default();
    for m in all_mods
        .iter()
        .filter(|m| m.info.as_ref().is_some_and(|i| i.is_java()))
    {
        let info = m.info.as_ref().expect("filtrado");
        let mut cols = Vec::new();
        for v in ["2.3.2", "2.3.4", "3.0.0"] {
            cols.push(match zb::pick_jar(info, v) {
                Some(zb::JarChoice::Chosen(c)) => {
                    let ok = if zb::jar_path(m, &c.jar).is_some() {
                        ""
                    } else {
                        " (¡no existe!)"
                    };
                    format!("{}{ok}", c.jar.rsplit('/').next().unwrap_or(&c.jar))
                }
                Some(zb::JarChoice::VersionIncompatible { .. }) => "no carga: versión".into(),
                Some(other) => format!("{other:?}"),
                None => "-".into(),
            });
        }
        if let Some(zb::JarChoice::Chosen(c)) = zb::pick_jar(info, "2.3.4")
            && let (Some(jar), Some(id)) = (zb::jar_path(m, &c.jar), m.id())
        {
            let e = zb::approval_state(&jar, id, &approval, &conflict)
                .map(|e| format!("{e:?}"))
                .unwrap_or_else(|e| e.to_string());
            cols.push(format!("aprobación: {e}"));
        }
        println!(
            "  {:22} {:28} | {}",
            m.id().unwrap_or("?"),
            m.key,
            cols.join(" | ")
        );
    }
    Ok(())
}

fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn missing(que: &str) -> pzmm_core::Error {
    pzmm_core::Error::NotFound(format!("falta {que}\n{USAGE}"))
}

fn positional<'a>(args: &'a [String], i: usize, what: &str) -> pzmm_core::CoreResult<&'a str> {
    args.get(i)
        .map(String::as_str)
        .filter(|a| !a.starts_with("--"))
        .ok_or_else(|| missing(what))
}

fn guard_list(
    a: &ProfileStore,
    p: &profiles::Profile,
    args: &[String],
) -> pzmm_core::CoreResult<()> {
    if !p.mods.is_empty() && !args.iter().any(|x| x == "--yes") {
        return Err(pzmm_core::Error::Backup(format!(
            "el perfil {} ya tiene {} mods: añade --yes para sustituir la lista (se guarda una copia en perfil.toml.bak)",
            p.id,
            p.mods.len()
        )));
    }
    let f = a.folder(&p.id).join(profiles::FILE);
    let bak = a.folder(&p.id).join(format!("{}.bak", profiles::FILE));
    std::fs::copy(&f, &bak).map_err(pzmm_core::Error::io(&bak))?;
    Ok(())
}

fn store(args: &[String]) -> pzmm_core::CoreResult<ProfileStore> {
    match option(args, "--dir") {
        Some(d) => Ok(ProfileStore::at(std::path::Path::new(d))),
        None => ProfileStore::default_location().ok_or_else(|| missing("USERPROFILE")),
    }
}

fn scan_mods(j: &Game) -> Vec<ScannedMod> {
    let v = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    let mut all_mods: Vec<ScannedMod> = j
        .library
        .as_ref()
        .map(|b| mods::scan_workshop(&steam::workshop_content_folder(b, PZ_APP_ID), v))
        .unwrap_or_default();
    if let Some(h) = std::env::var_os("USERPROFILE") {
        let local_dir = PathBuf::from(h).join("Zomboid").join("mods");
        all_mods.extend(mods::scan_folder(&local_dir, Source::Local, "local:", v));
    }
    all_mods
}

fn scan_for(j: &Game, cache: &std::path::Path) -> Vec<ScannedMod> {
    let mut all = scan_mods(j);
    let v = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    all.extend(mods::scan_profile_folder(cache, v));
    all
}

fn profile(args: &[String]) -> pzmm_core::CoreResult<()> {
    let a = store(args)?;
    match args.first().map(String::as_str) {
        Some("create") => {
            let name = args.get(1).ok_or_else(|| missing("el nombre"))?;
            let p = a.create(name)?;
            println!("creado {} en {}", p.id, a.folder(&p.id).display());
        }
        Some("list") => {
            for p in a.list() {
                let active = p.mods.iter().filter(|m| m.enabled).count();
                println!("{:40} {:30} {active} mods activos", p.id, p.name);
            }
        }
        Some("mods") => {
            let id = positional(args, 1, "el id del perfil")?;
            let list = positional(args, 2, "la lista de IDs de mod")?;
            let mut p = a.load(id)?;
            guard_list(&a, &p, args)?;
            let scanned = scan_mods(&game::detect()?);
            p.mods = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|mid| {
                    let key = scanned
                        .iter()
                        .find(|m| m.id() == Some(mid))
                        .map(|m| m.key.clone())
                        .unwrap_or_default();
                    if key.is_empty() {
                        println!("  aviso: {mid} no está en el Workshop ni en local");
                    }
                    ModEntry {
                        key,
                        id: mid.to_string(),
                        enabled: true,
                        pinned: false,
                        section: None,
                    }
                })
                .collect();
            a.save(&p)?;
            println!("{}: {} mods", p.id, p.mods.len());
        }
        Some("import") => {
            let id = positional(args, 1, "el id del perfil")?;
            let list = positional(args, 2, "la ruta de default.txt")?;
            guard_list(&a, &a.load(id)?, args)?;
            do_import(&a, id, std::path::Path::new(list))?;
        }
        _ => return Err(missing("create | list | mods | import")),
    }
    Ok(())
}

type Prepared = (Game, ProfileStore, profiles::Profile, launch::LaunchLine);

fn prepare(args: &[String]) -> pzmm_core::CoreResult<Prepared> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let p = a.load(id)?;
    let j = game::detect()?;
    let options = std::env::var_os("USERPROFILE")
        .map(|h| PathBuf::from(h).join("Zomboid").join("options.ini"));
    profiles::prepare_cache(&a.cache(&p.id), &p, options.as_deref())?;
    let seen = scan_for(&j, &a.cache(&p.id));
    let cache = a.cache(&p.id);
    if p.launch.probe {
        let media = game::media_index(&j.folder);
        let names = probe_names(
            &conflicts::profile_entries(&p, &seen),
            &seen,
            &context_of(&j, &p, &media),
        );
        pzmm_core::probe::install(&cache, &names, &p.mod_list().text())?;
        println!("Sonda: {} funciones a comprobar", names.len());
    } else {
        pzmm_core::probe::remove(&cache)?;
    }
    let link = pzmm_core::links::sync(
        &a.cache(&p.id).join("mods"),
        &pzmm_core::links::wanted(&conflicts::profile_entries(&p, &seen)),
    )?;
    if !link.created.is_empty() || !link.removed.is_empty() || !link.taken.is_empty() {
        println!(
            "Enlaces: creados {:?} · quitados {:?} · ocupados {:?}",
            link.created, link.removed, link.taken
        );
    }
    let active: Vec<&str> = p
        .mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.id.as_str())
        .collect();
    let has_java = !active.is_empty()
        && seen.iter().any(|m| {
            m.id().is_some_and(|i| active.contains(&i))
                && m.info.as_ref().is_some_and(|i| i.is_java())
        });
    let use_zb = match p.zombiebuddy.usage.as_str() {
        "si" => true,
        "no" => false,
        _ => has_java,
    };
    if use_zb && !j.folder.join("zbNative.dll").is_file() {
        return Err(pzmm_core::Error::NotFound(
            "zbNative.dll en la carpeta del juego (instala ZombieBuddy)".into(),
        ));
    }
    let zb_opts = use_zb.then(|| launch::ZbOptions {
        policy: p.zombiebuddy.policy.clone(),
        verbosity: p.zombiebuddy.verbosity,
    });
    let l = launch::build(&j, &p, &a.cache(&p.id), zb_opts.as_ref())?;
    Ok((j, a, p, l))
}

fn line(args: &[String]) -> pzmm_core::CoreResult<()> {
    let (_, _, _, l) = prepare(args)?;
    println!("{}", l.text());
    Ok(())
}

fn launch(args: &[String]) -> pzmm_core::CoreResult<()> {
    let (j, a, p, l) = prepare(args)?;
    let cache = a.cache(&p.id);
    println!("Línea: {}", l.text());
    let mut watched = vec![j.folder.clone()];
    if let Some(h) = std::env::var_os("USERPROFILE") {
        watched.push(PathBuf::from(&h).join("Zomboid"));
        watched.push(PathBuf::from(&h).join(".zombie_buddy"));
    }
    watched.retain(|c| c.is_dir());
    let before = args
        .iter()
        .any(|x| x == "--verify")
        .then(|| integrity::take(&watched));
    let idle: u64 = option(args, "--close-idle")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let max: u64 = option(args, "--max")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);
    let mut child = launch::launch(&l)?;
    println!("Juego en marcha (PID {})", child.id());
    let wait_idle = if idle > 0 { idle } else { u64::MAX / 4 };
    let end = launch::wait(
        &mut child,
        &cache.join("console.txt"),
        Duration::from_secs(wait_idle),
        Duration::from_secs(max),
    );
    if !matches!(end, launch::End::Exited(_)) {
        let _ = child.kill();
        let _ = child.wait();
    }
    println!("Fin: {end:?}");
    let text = std::fs::read_to_string(cache.join("console.txt")).unwrap_or_default();
    let inf = launch::read_console(&text);
    let steam_ok = if inf.steam { "sí" } else { "no" };
    println!(
        "Versión {} · Steam {steam_ok}",
        inf.version.as_deref().unwrap_or("?")
    );
    println!("Cargados ({}): {}", inf.loaded.len(), inf.loaded.join(", "));
    let requested: Vec<&str> = p
        .mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.id.as_str())
        .collect();
    let not_loaded: Vec<&&str> = requested
        .iter()
        .filter(|m| !inf.loaded.iter().any(|c| c == **m))
        .collect();
    println!("Pedidos que el juego NO cargó: {not_loaded:?}");
    println!("Archivos que se pisan: {}", inf.overrides.len());
    for e in inf.exceptions.iter().take(5) {
        println!("  excepción: {e}");
    }
    for z in inf
        .zb
        .iter()
        .filter(|z| z.contains("loaded") || z.contains("java mod list"))
        .take(8)
    {
        println!("  {z}");
    }
    let mut list =
        pzmm_core::mod_list::ModList::read_file(&cache.join("mods").join("default.txt"))?;
    list.mods.retain(|m| m != pzmm_core::probe::ID);
    let equal = if list.mods == p.mod_list().mods {
        "equal"
    } else {
        "¡CAMBIÓ!"
    };
    println!(
        "default.txt tras cerrar: {} mods ({equal})",
        list.mods.len()
    );
    if let Some(before) = before {
        let diff = integrity::compare(&before, &integrity::take(&watched));
        println!(
            "Integridad (juego, Zomboid, .zombie_buddy): {} diferencias",
            diff.total()
        );
        let every = diff
            .added
            .iter()
            .map(|x| ("new", x))
            .chain(diff.deleted.iter().map(|x| ("borrado", x)))
            .chain(diff.changed.iter().map(|x| ("changed", x)));
        for (c, (folder, rel)) in every.take(20) {
            println!("  {c:9} {}  {rel}", folder.display());
        }
    }
    Ok(())
}

fn zb_order(args: &[String]) -> pzmm_core::CoreResult<()> {
    match args.first().map(String::as_str) {
        Some("install") => {}
        Some("status") => return zb_state(),
        Some("uninstall") => {
            let game = PathBuf::from(option(args, "--game").ok_or_else(|| missing("--game"))?);
            let cps = PathBuf::from(option(args, "--backups").ok_or_else(|| missing("--backups"))?);
            let steam = if args.iter().any(|a| a == "--ignore-steam") {
                None
            } else {
                steam::steam_folder()
            };
            let copy = zb::uninstall(
                &game,
                &cps,
                steam.as_deref(),
                args.iter().any(|a| a == "--remove-references"),
                !args.iter().any(|a| a == "--no-check"),
            )?;
            println!("ZombieBuddy desinstalado · copia en {}", copy.display());
            return Ok(());
        }
        _ => return Err(missing("install | status | uninstall")),
    }
    let game = PathBuf::from(option(args, "--game").ok_or_else(|| missing("--game"))?);
    let libs = PathBuf::from(option(args, "--from").ok_or_else(|| missing("--from"))?);
    let cps = PathBuf::from(option(args, "--backups").ok_or_else(|| missing("--backups"))?);
    let install = zb::install(&game, &libs, &cps, !args.iter().any(|a| a == "--no-check"))?;
    println!(
        "ZombieBuddy {} -> {} · copia en {}",
        install.before.as_deref().unwrap_or("(no había)"),
        install.after.as_deref().unwrap_or("?"),
        install.copy.display()
    );
    Ok(())
}

fn copy(args: &[String]) -> pzmm_core::CoreResult<()> {
    if args.first().map(String::as_str) != Some("restore") {
        return Err(missing("restore"));
    }
    let c = PathBuf::from(
        args.get(1)
            .ok_or_else(|| missing("la carpeta de la copia"))?,
    );
    let game = PathBuf::from(option(args, "--game").ok_or_else(|| missing("--game"))?);
    let n = backups::restore(&c, &game, None)?;
    println!("restaurados {n} archivos desde {}", c.display());
    Ok(())
}

fn zb_state() -> pzmm_core::CoreResult<()> {
    let j = game::detect()?;
    let steam_dir = steam::steam_folder();
    let in_game = zb::jar_version(&j.folder.join("ZombieBuddy.jar"));
    println!(
        "En la carpeta del juego: {}",
        in_game.as_deref().unwrap_or("no instalado")
    );
    println!(
        "zbNative.dll: {}",
        if j.folder.join("zbNative.dll").is_file() {
            "sí"
        } else {
            "no"
        }
    );
    if let Some(b) = &j.library {
        let backups = zb::workshop_copies(&steam::workshop_content_folder(b, PZ_APP_ID));
        for (channel, id) in [
            ("estable", zb::WORKSHOP_STABLE),
            ("beta", zb::WORKSHOP_BETA),
        ] {
            match backups.iter().find(|c| c.workshop_id == id) {
                Some(c) => println!(
                    "Workshop {channel} ({id}): {}",
                    c.version.as_deref().unwrap_or("?")
                ),
                None => println!(
                    "Workshop {channel} ({id}): no suscrito · {}",
                    zb::steam_workshop_url(id)
                ),
            }
        }
    }
    let r = zb::references(&j.folder, steam_dir.as_deref());
    println!(
        "Lo cargan: JSON {} · BAT {} · opciones de Steam {}",
        if r.json { "sí" } else { "no" },
        if r.bat { "sí" } else { "no" },
        if r.steam.is_empty() {
            "no".to_string()
        } else {
            format!("sí ({})", r.steam.join(", "))
        }
    );
    if let (Some(v), Some(z)) = (&j.version, &in_game)
        && let Some(a) = zb::incompatible_with_game(v.version, z)
    {
        println!("AVISO: {a}");
    }
    Ok(())
}

fn resolve_entries<'a>(
    p: &profiles::Profile,
    visible: &'a [ScannedMod],
) -> Vec<conflicts::ProfileEntry<'a>> {
    p.mods
        .iter()
        .map(|e| conflicts::ProfileEntry {
            id: e.id.clone(),
            enabled: e.enabled,
            pinned: e.pinned,
            found: visible
                .iter()
                .find(|m| !e.key.is_empty() && m.key == e.key)
                .or_else(|| visible.iter().find(|m| m.id() == Some(e.id.as_str()))),
            section: p.section_index(e),
        })
        .collect()
}

fn order_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let mut p = a.load(id)?;
    let j = game::detect()?;
    let vis = scan_for(&j, &a.cache(&p.id));
    let resolved = resolve_entries(&p, &vis);
    let entries: Vec<order::Entry> = resolved.iter().map(|e| e.order_entry()).collect();
    let before: Vec<String> = p
        .mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.id.clone())
        .collect();
    let edges = order::edges(&entries);
    let broken_before = order::unmet_edges(&before, &edges);
    let rules = pzmm_core::categories::user_rules();
    let list: Vec<(&str, bool, Option<&ScannedMod>)> = resolved
        .iter()
        .map(|x| (x.id.as_str(), x.enabled, x.found))
        .collect();
    let (options, info) = pzmm_core::categories::plan_order(&list, &rules, &p.accepted_overrides);
    println!("{} mods con reglas en sorting_rules.txt", rules.len());
    {
        let mut count: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for i in info.values() {
            let c = count.entry(i.category.key()).or_default();
            c.0 += 1;
            if i.guessed {
                c.1 += 1;
            }
        }
        for (c, (n, guessed)) in &count {
            println!("  {c:<16} {n:>4} ({guessed} adivinadas)");
        }
    }
    let section_of: Vec<usize> = p.mods.iter().map(|m| p.section_index(m)).collect();
    let mut r = pzmm_core::sections::sort_sections(&entries, &options, &section_of, None);
    let version = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    conflicts::mark_winner_moves(&mut r, &resolved, &vis, version);
    if args.iter().any(|a| a == "--list") {
        for id in r.order.iter() {
            println!("= {id}");
        }
    }
    let after: Vec<String> = r
        .order
        .iter()
        .filter(|id| {
            before
                .iter()
                .any(|x| pzmm_core::modinfo::normalize_id(x) == **id)
        })
        .cloned()
        .collect();
    let broken_after = order::unmet_edges(&after, &edges);
    let hard_ones = |v: &[order::Edge]| v.iter().filter(|x| x.rule.is_hard()).count();
    println!(
        "{} mods activos, {} reglas entre ellos ({} duras)",
        before.len(),
        edges.len(),
        hard_ones(&edges)
    );
    println!(
        "Orden actual: incumple {} reglas ({} duras) · orden automático: incumple {} ({} duras)",
        broken_before.len(),
        hard_ones(&broken_before),
        broken_after.len(),
        hard_ones(&broken_after)
    );
    for c in &r.cycles {
        println!(
            "  ciclo{}: {}",
            if c.hard { " DURO" } else { "" },
            c.mods.join(" → ")
        );
    }
    println!("Movimientos: {}", r.moves.len());
    for (effect, title) in [
        (order::Effect::Rule, "Por una regla"),
        (order::Effect::Winner, "Cambian qué mod gana un archivo"),
        (order::Effect::Category, "Solo por tipo de mod"),
    ] {
        let group: Vec<&order::Move> = r.moves.iter().filter(|m| m.effect == effect).collect();
        if group.is_empty() {
            continue;
        }
        println!("{title} ({})", group.len());
        for m in group.iter().take(25) {
            let reasons: Vec<String> = m
                .reasons
                .iter()
                .map(|x| format!("{:?} {}→{}", x.rule, x.before, x.after))
                .collect();
            println!(
                "  {:30} {:>3} → {:<3} {}",
                m.id,
                m.from + 1,
                m.to + 1,
                reasons.join("; ")
            );
        }
    }
    if args.iter().any(|x| x == "--apply") {
        pzmm_core::sections::apply_order(&mut p, &r.order);
        a.save(&p)?;
        println!("Orden guardado en el perfil.");
    }
    Ok(())
}

fn maps_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    use pzmm_core::maps;
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let p = a.load(id)?;
    let j = game::detect()?;
    let vis = scan_for(&j, &a.cache(&p.id));
    let resolved = resolve_entries(&p, &vis);
    let t0 = std::time::Instant::now();
    let all: Vec<maps::MapEntry> = maps::dedupe_folders(
        resolved
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| e.found)
            .flat_map(maps::entries)
            .collect(),
    );
    let detected: Vec<String> = all.iter().map(|m| m.folder.clone()).collect();
    let order = profiles::order_maps(&p.maps, &detected);
    let media = game::media_index(&j.folder);
    let vanilla = maps::vanilla_cells(&media);
    let urban = maps::game_urban(&j.folder).unwrap_or_default();
    let refs: Vec<&maps::MapEntry> = all.iter().collect();
    let cells = maps::cells_of(&order, &refs, &vanilla, &urban);
    for (i, folder) in order.iter().enumerate() {
        let Some(m) = all.iter().find(|m| m.folder == *folder) else {
            println!("{:>3}. {folder}", i + 1);
            continue;
        };
        let near: Vec<_> = m.cells.iter().filter(|c| urban.touches(**c)).collect();
        let town: Vec<Option<u32>> = near
            .iter()
            .map(|c| maps::town_squares(m, **c, &urban))
            .collect();
        println!(
            "{:>3}. {folder} ({} celdas, {} junto a un pueblo, casillas de pueblo {town:?})",
            i + 1,
            m.cells.len(),
            near.len()
        );
    }
    println!(
        "{} celdas · vanilla {} · urbanas {} · {} ms",
        cells.len(),
        vanilla.len(),
        urban.cells().count(),
        t0.elapsed().as_millis()
    );
    for c in cells.iter().filter(|c| c.state != maps::CellState::Free) {
        println!(
            "  {}_{} {:?}: {}",
            c.cell.0,
            c.cell.1,
            c.state,
            c.maps.join(" > ")
        );
    }
    Ok(())
}

fn lotpack_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let missing =
        || pzmm_core::Error::NotFound("uso: lotpack <carpeta> <x> <y> [<carpeta2>]".into());
    let (Some(dir), Some(x), Some(y)) = (args.first(), args.get(1), args.get(2)) else {
        return Err(missing());
    };
    let cover = |d: &str| {
        let d = PathBuf::from(d);
        pzmm_core::lotpack::coverage(
            &d.join(format!("{x}_{y}.lotheader")),
            &d.join(format!("world_{x}_{y}.lotpack")),
        )
    };
    let a = cover(dir)?;
    println!("ocupadas {} de 65536", a.count());
    if let Some(other) = args.get(3) {
        let b = cover(other)?;
        println!("segunda: ocupadas {} de 65536", b.count());
        println!("en común {}", a.overlap(&b));
    }
    Ok(())
}

fn patches_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let j = game::detect()?;
    let vis = scan_mods(&j);
    let zb_version = args
        .iter()
        .position(|a| a == "--zb")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| zb::jar_version(&j.folder.join("ZombieBuddy.jar")))
        .unwrap_or_default();
    let mods: Vec<&ScannedMod> = match args.first().filter(|a| !a.starts_with("--")) {
        Some(id) => {
            let p = store(args)?.load(id)?;
            resolve_entries(&p, &vis)
                .into_iter()
                .filter(|e| e.enabled)
                .filter_map(|e| e.found)
                .collect()
        }
        None => vis.iter().collect(),
    };
    let mut patched = Vec::new();
    for m in mods {
        let Some(list) = zb::mod_patches(m, &zb_version) else {
            continue;
        };
        let id = m.id().unwrap_or(&m.key).to_string();
        println!("{id} ({} parches)", list.len());
        for x in &list {
            println!(
                "  {}{}{}",
                x.class_name,
                if x.method_name.is_empty() {
                    String::new()
                } else {
                    format!(".{}", x.method_name)
                },
                if x.replaces { "  [sustituye]" } else { "" }
            );
        }
        patched.push((id, list));
    }
    let h = conflicts::java_patch_overlaps(&patched);
    println!("{} coincidencias de métodos", h.len());
    for x in &h {
        println!("  {:?}: {}", x.severity, x.text());
    }
    Ok(())
}

fn context_of<'a>(
    j: &'a Game,
    p: &profiles::Profile,
    media: &'a std::collections::BTreeSet<String>,
) -> conflicts::Context<'a> {
    let cfg = zb::config_folder();
    conflicts::Context {
        game: j
            .version
            .as_ref()
            .map(|v| v.version)
            .unwrap_or(Version::new(42, 0)),
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

fn probe_names(
    entries: &[conflicts::ProfileEntry],
    visible: &[ScannedMod],
    ctx: &conflicts::Context,
) -> Vec<String> {
    conflicts::lua_contested(entries, visible, ctx)
}

fn probe_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let mut p = a.load(id)?;
    if let Some(on) = args.iter().find_map(|x| match x.as_str() {
        "--on" => Some(true),
        "--off" => Some(false),
        _ => None,
    }) {
        p.launch.probe = on;
        a.save(&p)?;
        println!("sonda {}", if on { "activada" } else { "desactivada" });
        return Ok(());
    }
    let j = game::detect()?;
    let vis = scan_for(&j, &a.cache(&p.id));
    let media = game::media_index(&j.folder);
    let resolved = resolve_entries(&p, &vis);
    let ctx = context_of(&j, &p, &media);
    let predicted = |phase| conflicts::lua_predictions(&resolved, &vis, &ctx, phase);
    let seen = pzmm_core::probe::read(&a.cache(&p.id));
    if seen.is_empty() {
        println!(
            "{} funciones previstas; la sonda aún no ha respondido (actívala con --on y arranca el perfil)",
            predicted(conflicts::LuaPhase::Start).len()
        );
        return Ok(());
    }
    if !pzmm_core::probe::same_list(&a.cache(&p.id), &p.mod_list().text()) {
        println!(
            "La lista del perfil cambió desde la última partida con sonda: arráncalo otra vez para cotejar."
        );
        return Ok(());
    }
    for (phase, lua_phase) in [
        ("boot", conflicts::LuaPhase::Boot),
        ("start", conflicts::LuaPhase::Start),
    ] {
        if !seen.iter().any(|s| s.phase == phase) {
            continue;
        }
        let asked = pzmm_core::probe::asked(&a.cache(&p.id));
        let predicted: Vec<(String, String)> = predicted(lua_phase)
            .into_iter()
            .filter(|(n, _)| asked.contains(n))
            .collect();
        let m = pzmm_core::probe::check(&predicted, &seen, phase);
        println!(
            "{phase}: {} de {} como se previó",
            predicted.len() - m.len(),
            predicted.len()
        );
        for x in m.iter().take(40) {
            println!(
                "  {}: previsto {} · el juego {}",
                x.name,
                x.predicted,
                x.seen.as_deref().unwrap_or("(no la vio)")
            );
        }
    }
    Ok(())
}

fn conflicts_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let p = a.load(id)?;
    let j = game::detect()?;
    let vis = scan_for(&j, &a.cache(&p.id));
    let resolved = resolve_entries(&p, &vis);
    let media = game::media_index(&j.folder);
    let ctx = context_of(&j, &p, &media);
    if args.iter().any(|x| x == "--measure") {
        for back in 1..=3 {
            let t0 = std::time::Instant::now();
            let h = conflicts::analyze(&resolved, &vis, &ctx);
            println!(
                "vuelta {back}: {} hallazgos en {} ms",
                h.len(),
                t0.elapsed().as_millis()
            );
        }
    }
    let h = conflicts::analyze(&resolved, &vis, &ctx);
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for x in &h {
        *count.entry(format!("{:?}", x.severity)).or_default() += 1;
    }
    println!(
        "{} activos · hallazgos: {count:?}",
        resolved.iter().filter(|e| e.enabled).count()
    );
    let limit: usize = option(args, "--max")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let only = option(args, "--severity");
    for x in h
        .iter()
        .filter(|x| only.is_none_or(|g| format!("{:?}", x.severity).eq_ignore_ascii_case(g)))
        .take(limit)
    {
        println!("[{:?}] {}", x.severity, x.text());
    }
    if args.iter().any(|x| x == "--json") {
        println!("{}", serde_json::to_string_pretty(&h).unwrap_or_default());
    }
    Ok(())
}

fn do_import(a: &ProfileStore, id: &str, list: &std::path::Path) -> pzmm_core::CoreResult<()> {
    let mut p = a.load(id)?;
    let l = pzmm_core::mod_list::ModList::read_file(list)?;
    let vis = scan_mods(&game::detect()?);
    p.mods = pzmm_core::profiles::entries_from(&l, |id| {
        let found = vis.iter().find(|m| m.id() == Some(id));
        found.map(|m| m.key.clone())
    });
    p.maps = l.maps;
    a.save(&p)?;
    let sin = p.mods.iter().filter(|m| m.key.is_empty()).count();
    println!(
        "{}: importados {} mods ({sin} sin encontrar)",
        p.id,
        p.mods.len()
    );
    Ok(())
}

fn overrides_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let a = store(args)?;
    let p = a.load(id)?;
    let j = game::detect()?;
    let vis = scan_mods(&j);
    let resolved = resolve_entries(&p, &vis);
    let v = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    let in_game = |rel: &str| j.folder.join(rel).is_file();
    for (m, rel) in conflicts::predicted_overrides(&resolved, &vis, v, &in_game) {
        println!("mod \"{m}\" overrides {rel}");
    }
    Ok(())
}

fn predict_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let f = args.get(1).ok_or_else(|| missing("el console.txt"))?;
    let bytes = std::fs::read(f).map_err(pzmm_core::Error::io(std::path::Path::new(f)))?;
    let report = launch::read_console(&String::from_utf8_lossy(&bytes));
    let p = store(args)?.load(id)?;
    let j = game::detect()?;
    let vis = scan_for(&j, &store(args)?.cache(&p.id));
    let resolved = resolve_entries(&p, &vis);
    let v = j
        .version
        .as_ref()
        .map(|v| v.version)
        .unwrap_or(Version::new(42, 0));
    let media = game::media_index(&j.folder);
    let order = conflicts::predicted_load(&resolved, &vis, v);
    let predicted = conflicts::predicted_overrides(&resolved, &vis, v, &|r| media.contains(r));
    let c = conflicts::check_prediction(&order, &predicted, &report.loaded, &report.overrides);
    println!(
        "orden {} ({} previstos, {} en la consola) · sin cargar {:?} · de más {:?} · pisados no previstos {} · previstos que no pasaron {}",
        if c.order_ok { "igual" } else { "distinto" },
        order.len(),
        report.loaded.len(),
        c.not_loaded,
        c.unexpected,
        c.overrides_missed,
        c.overrides_extra
    );
    for x in &c.examples {
        println!("  {x}");
    }
    Ok(())
}

fn workshop_steamworks(args: &[String]) -> pzmm_core::CoreResult<()> {
    use pzmm_core::steamworks::{Options, Order, Query, Steamworks};
    let j = game::detect()?;
    let mut s = Steamworks::start(&j.folder)?;
    let ids: Vec<u64> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
    let print = |e: &pzmm_core::steamworks::Item| {
        println!(
            "{:>11}  {}{}  · {} · {} subs · {}/{} votos · {} · hijos {}",
            e.id,
            e.title,
            if e.collection { " [colección]" } else { "" },
            e.author.as_deref().unwrap_or("?"),
            e.subscribers,
            e.votes_up,
            e.votes_down,
            e.tags.join(","),
            e.children.len()
        );
    };
    match args[0].as_str() {
        "search" => {
            let order = match option(args, "--order").unwrap_or("trend") {
                "subscribed" => Order::MostSubscribed,
                "recent" => Order::Recent,
                "votes" => Order::Votes,
                _ => Order::Trend,
            };
            let text = args
                .get(1)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .unwrap_or_default();
            let c = Query {
                text,
                order,
                tags: option(args, "--tag")
                    .map(|e| vec![e.to_string()])
                    .unwrap_or_default(),
                page: option(args, "--page")
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(1),
                collections: args.iter().any(|a| a == "--collections"),
            };
            let t0 = std::time::Instant::now();
            let p = s.search(&c)?;
            println!(
                "página {} · {} de {} ({} con imagen) · caché {} · {:?}",
                p.page,
                p.items.len(),
                p.items
                    .iter()
                    .filter(|e| !e.preview_view.is_empty())
                    .count(),
                p.total,
                p.from_cache,
                t0.elapsed()
            );
            for e in p.items.iter().take(10) {
                print(e);
            }
        }
        "details" => {
            let complete = Options {
                long: true,
                names: true,
            };
            for e in s.details(&ids, complete)? {
                print(&e);
                println!("    vista previa: {}", e.preview_view);
                let d = pzmm_core::steamworks::bbcode_to_text(&e.description);
                println!(
                    "    {}",
                    d.chars()
                        .take(300)
                        .collect::<String>()
                        .replace('\n', "\n    ")
                );
            }
        }
        "collection" => {
            let id = *ids.first().ok_or_else(|| missing("<id>"))?;
            for back in 1..=2 {
                let t0 = std::time::Instant::now();
                let complete = Options {
                    long: true,
                    names: true,
                };
                let c = s.details(&[id], complete)?;
                let c = c
                    .first()
                    .ok_or_else(|| missing("una colección que exista"))?;
                let t1 = t0.elapsed();
                let light = Options {
                    long: false,
                    names: false,
                };
                let children = s.details(&c.children, light)?;
                let with_image = children
                    .iter()
                    .filter(|h| !h.preview_view.is_empty())
                    .count();
                println!(
                    "vuelta {back}: «{}» {} hijos ({} con imagen) · ficha {:?} · hijos {:?} · total {:?}",
                    c.title,
                    children.len(),
                    with_image,
                    t1,
                    t0.elapsed() - t1,
                    t0.elapsed()
                );
            }
        }
        "status" => {
            for id in &ids {
                println!("{:?}", s.state(*id));
            }
        }
        "subscribe" | "unsubscribe" => {
            let id = *ids.first().ok_or_else(|| missing("<id>"))?;
            println!("antes: {:?}", s.state(id));
            if args[0] == "subscribe" {
                s.subscribe(id)?;
            } else {
                s.unsubscribe(id)?;
            }
            println!("hecho: {:?}", s.state(id));
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let e = s.state(id);
                println!("  {:?}", e);
                if args[0] == "subscribe" && e.installed && !e.downloading {
                    break;
                }
                if args[0] == "unsubscribe" && !e.subscribed {
                    break;
                }
            }
            println!("suscripciones ahora: {}", s.subscribed_items().len());
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn workshop_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    if matches!(
        args.first().map(String::as_str),
        Some("search" | "details" | "collection" | "status" | "subscribe" | "unsubscribe")
    ) {
        return workshop_steamworks(args);
    }
    if args.first().map(String::as_str) != Some("pending") {
        return Err(missing(
            "pending | search | details | status | subscribe | unsubscribe",
        ));
    }
    let j = game::detect()?;
    let b = j
        .library
        .as_ref()
        .ok_or_else(|| missing("la biblioteca de Steam del juego"))?;
    let w = steam::read_workshop_state(b, PZ_APP_ID)?;
    let p = w.pending_ids();
    println!(
        "NeedsUpdate (app) = {} · suscripciones {} · instaladas {} · pendientes {}",
        w.needs_update,
        w.details.len(),
        w.items.len(),
        p.len()
    );
    for id in p {
        let d = &w.details[&id];
        let install = w.items.get(&id).map(|e| e.manifest.as_str());
        println!(
            "  {id}: instalado {} · publicado {}",
            install.unwrap_or("(sin descargar)"),
            d.last_manifest.as_deref().unwrap_or("?")
        );
    }
    Ok(())
}

fn export_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let id = args.first().ok_or_else(|| missing("el id del perfil"))?;
    let p = store(args)?.load(id)?;
    let j = game::detect()?;
    let vis = scan_mods(&j);
    let resolved = conflicts::profile_entries(&p, &vis);
    let active: Vec<_> = resolved.iter().filter(|e| e.enabled).collect();
    let mut ws: Vec<u64> = Vec::new();
    let mut maps: Vec<String> = Vec::new();
    for e in &active {
        if let Some(m) = e.found {
            if let Source::Workshop { id } = m.source
                && !ws.contains(&id)
            {
                ws.push(id);
            }
            for c in m.maps() {
                if !maps.contains(&c) {
                    maps.push(c);
                }
            }
        }
    }
    let order = profiles::order_maps(&p.maps, &maps);
    println!(
        "Mods={}",
        active
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>()
            .join(";")
    );
    println!(
        "WorkshopItems={}",
        ws.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(";")
    );
    println!("Map={}", order.join(";"));
    println!(
        "({} mods activos, {} sin instalar, {} elementos del Workshop, {} mapas)",
        active.len(),
        active.iter().filter(|e| e.found.is_none()).count(),
        ws.len(),
        order.len()
    );
    Ok(())
}

fn sandbox_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    use pzmm_core::sandbox::{self, Kind};
    let j = game::detect()?;
    if args.first().map(String::as_str) == Some("schema") {
        let id = args.get(1).ok_or_else(|| missing("el id del perfil"))?;
        let p = store(args)?.load(id)?;
        let vis = scan_mods(&j);
        let resolved = conflicts::profile_entries(&p, &vis);
        let mods: Vec<&ScannedMod> = resolved
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| e.found)
            .collect();
        let t0 = std::time::Instant::now();
        let def = sandbox::definition(
            &j.folder,
            &mods,
            if args.iter().any(|a| a == "--en") {
                "EN"
            } else {
                "ES"
            },
        )?;
        let e = def.schema();
        println!(
            "definición en {:?}: {} opciones de sandbox ({} de mods), {} de servidor",
            t0.elapsed(),
            def.sandbox.len(),
            def.sandbox.iter().filter(|o| o.from_mod.is_some()).count(),
            def.server.len()
        );
        for (name, pages) in [("SERVER", &e.server), ("SANDBOX", &e.sandbox)] {
            let total: usize = pages
                .iter()
                .flat_map(|p| &p.groups)
                .map(|g| g.options.len())
                .sum();
            println!("{name}: {} páginas, {total} opciones", pages.len());
            for pg in pages {
                let n: usize = pg.groups.iter().map(|g| g.options.len()).sum();
                println!(
                    "  {:<34} {:>3}  {}",
                    pg.title,
                    n,
                    pg.from_mod.as_deref().unwrap_or("")
                );
            }
        }
        if let Some(cfg) = option(args, "--cfg") {
            let text = std::fs::read_to_string(cfg)
                .map_err(pzmm_core::Error::io(std::path::Path::new(cfg)))?;
            let parsed_map = sandbox::read_ini(&text);
            let values = sandbox::Definition::complete(&def.sandbox, &parsed_map);
            let written = def.cfg_text(&values);
            let reread = sandbox::read_ini(&written);
            let (mut same, mut distinct, mut undefined) = (0, Vec::new(), Vec::new());
            for (k, v) in &parsed_map {
                match reread.get(k) {
                    Some(w) if w == v => same += 1,
                    Some(w) => distinct.push(format!("{k}: {v:?} -> {w:?}")),
                    None => undefined.push(k.clone()),
                }
            }
            println!(
                "{cfg}: {} claves · {same} iguales · {} distintas · {} que no define ningún mod activo",
                parsed_map.len(),
                distinct.len(),
                undefined.len()
            );
            for d in distinct.iter().take(8) {
                println!("    {d}");
            }
            if !undefined.is_empty() {
                println!(
                    "    sin definir (p. ej.): {:?}",
                    &undefined[..undefined.len().min(5)]
                );
            }
        }
        if let Some(preset) = option(args, "--lua") {
            let f = j
                .folder
                .join("media/lua/shared/Sandbox")
                .join(format!("{preset}.lua"));
            let parsed_map =
                sandbox::read_lua(&std::fs::read_to_string(&f).map_err(pzmm_core::Error::io(&f))?);
            let values = sandbox::Definition::complete(&def.sandbox, &parsed_map);
            let lua = def.lua_text(&values);
            let reread = sandbox::read_lua(&lua);
            let distinct: Vec<String> = parsed_map
                .iter()
                .filter(|(k, v)| {
                    reread
                        .get(*k)
                        .map(|w| {
                            def.sandbox
                                .iter()
                                .find(|o| &o.name == *k)
                                .map(|o| o.clamp(w) != o.clamp(v))
                                .unwrap_or(true)
                        })
                        .unwrap_or(true)
                })
                .map(|(k, v)| format!("{k}: {v:?} -> {:?}", reread.get(k)))
                .collect();
            println!(
                "{preset}.lua: {} claves · {} distintas tras escribir y leer · .lua de {} líneas",
                parsed_map.len(),
                distinct.len(),
                lua.lines().count()
            );
            for d in distinct.iter().take(8) {
                println!("    {d}");
            }
            if let Some(dest) = option(args, "--save") {
                std::fs::write(dest, &lua)
                    .map_err(pzmm_core::Error::io(std::path::Path::new(dest)))?;
            }
        }
        if let Some(base) = option(args, "--server") {
            let ini_p = std::path::PathBuf::from(format!("{base}.ini"));
            let lua_p = std::path::PathBuf::from(format!("{base}_SandboxVars.lua"));
            let ini = std::fs::read_to_string(&ini_p).map_err(pzmm_core::Error::io(&ini_p))?;
            let lua = std::fs::read_to_string(&lua_p).map_err(pzmm_core::Error::io(&lua_p))?;
            let srv = sandbox::Definition::complete(&def.server, &sandbox::read_ini(&ini));
            let sb = sandbox::Definition::complete(&def.sandbox, &sandbox::read_lua(&lua));
            let mut srv_all = srv.clone();
            for (k, v) in sandbox::read_ini(&ini) {
                srv_all.entry(k).or_insert(v);
            }
            let ini2 = def.ini_text(&srv_all);
            let lua2 = def.lua_text(&sb);
            for (name, a, b) in [("ini", &ini, &ini2), ("lua", &lua, &lua2)] {
                let la: Vec<&str> = a.lines().map(str::trim_end).collect();
                let lb: Vec<&str> = b.lines().map(str::trim_end).collect();
                let diff: Vec<String> = la
                    .iter()
                    .zip(&lb)
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                    .map(|(i, (x, y))| format!("{}: {x:?} | {y:?}", i + 1))
                    .collect();
                println!(
                    "{name}: {} líneas del juego, {} nuestras, {} distintas",
                    la.len(),
                    lb.len(),
                    diff.len()
                );
                for d in diff.iter().take(10) {
                    println!("    {d}");
                }
            }
            if let Some(dir) = option(args, "--write") {
                let d = std::path::Path::new(dir);
                std::fs::create_dir_all(d).map_err(pzmm_core::Error::io(d))?;
                for (f, txt) in [
                    ("prueba.ini", ini2),
                    ("prueba_SandboxVars.lua", lua2),
                    ("prueba.cfg", def.cfg_text(&sb)),
                ] {
                    std::fs::write(d.join(f), txt).map_err(pzmm_core::Error::io(&d.join(f)))?;
                }
            }
        }
        if let Some(dest) = option(args, "--json") {
            let read = |f: String| std::fs::read_to_string(f).unwrap_or_default();
            let (sb, srv) = match option(args, "--server") {
                Some(b) => (
                    sandbox::read_lua(&read(format!("{b}_SandboxVars.lua"))),
                    sandbox::read_ini(&read(format!("{b}.ini"))),
                ),
                None => Default::default(),
            };
            let json = serde_json::json!({
                "schema": {
                    "schema": &e,
                    "game_presets": sandbox::game_presets(&j.folder),
                    "profile_presets": [],
                    "user_presets": [],
                    "servers": ["servertest"],
                },
                "values": {
                    "sandbox": sandbox::Definition::complete(&def.sandbox, &sb),
                    "server": sandbox::Definition::complete(&def.server, &srv),
                },
            });
            std::fs::write(dest, json.to_string())
                .map_err(pzmm_core::Error::io(std::path::Path::new(dest)))?;
        }
        if let Some(o) = e
            .sandbox
            .iter()
            .flat_map(|p| &p.groups)
            .flat_map(|g| &g.options)
            .find(|o| o.name == "Zombies")
        {
            println!(
                "ejemplo: {} = «{}» · {:?} · {:?}",
                o.name, o.title, o.help, o.values
            );
        }
        return Ok(());
    }
    let jar = j.folder.join("projectzomboid.jar");
    let t0 = std::time::Instant::now();
    let ops = if args.iter().any(|a| a == "--server") {
        sandbox::server_options(&jar)?
    } else {
        sandbox::game_options(&jar)?
    };
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for o in &ops {
        *count
            .entry(match o.kind {
                Kind::Boolean { .. } => "sí/no",
                Kind::Integer { .. } => "integer",
                Kind::Decimal { .. } => "decimal",
                Kind::List { .. } => "list",
                Kind::Text { .. } => "text",
            })
            .or_default() += 1;
    }
    println!("{} opciones en {:?}: {:?}", ops.len(), t0.elapsed(), count);
    if args.iter().any(|a| a == "--names") {
        for o in &ops {
            println!("{}", o.name);
        }
    } else {
        for o in ops.iter().take(6).chain(ops.iter().rev().take(3)) {
            println!("  {o:?}");
        }
    }
    Ok(())
}

fn integrity_cli() -> pzmm_core::CoreResult<()> {
    let j = game::detect()?;
    let mut v = vec![j.folder.clone()];
    if let Some(h) = std::env::var_os("USERPROFILE") {
        v.push(PathBuf::from(&h).join("Zomboid"));
        v.push(PathBuf::from(&h).join(".zombie_buddy"));
    }
    v.retain(|c| c.is_dir());
    let t0 = std::time::Instant::now();
    let i = integrity::take_with_limits(&v, &[("Saves", 3)]);
    for (c, m) in &i.folders {
        println!("{:>8}  {}", m.len(), c.display());
    }
    println!("en {:?}", t0.elapsed());
    Ok(())
}

fn console_cli(args: &[String]) -> pzmm_core::CoreResult<()> {
    let f = args.first().ok_or_else(|| missing("el archivo"))?;
    let bytes = std::fs::read(f).map_err(pzmm_core::Error::io(std::path::Path::new(f)))?;
    let text = String::from_utf8_lossy(&bytes);
    let t0 = std::time::Instant::now();
    let mut v = pzmm_core::console::analyze(&text, &[]);
    println!(
        "{} líneas, {} incidencias en {:?}",
        text.lines().count(),
        v.len(),
        t0.elapsed()
    );
    if let Some(id) = option(args, "--profile") {
        let p = store(args)?.load(id)?;
        let vis = scan_mods(&game::detect()?);
        let resolved = resolve_entries(&p, &vis);
        let sources: Vec<pzmm_core::console::ModSource> = resolved
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| {
                let m = e.found?;
                Some(pzmm_core::console::ModSource {
                    id: m.id()?.to_string(),
                    pkg: m.info.as_ref()?.java.as_ref().and_then(|j| j.pkg.clone()),
                    scanned: m,
                })
            })
            .collect();
        let before = v.iter().filter(|i| i.mod_id.is_some()).count();
        let t1 = std::time::Instant::now();
        let budget = option(args, "--budget")
            .and_then(|b| b.parse().ok())
            .unwrap_or(300);
        pzmm_core::console::attribute(&mut v, &sources, Duration::from_millis(budget));
        println!(
            "atribuidas {} → {} de {} en {:?} · del juego {}",
            before,
            v.iter().filter(|i| i.mod_id.is_some()).count(),
            v.len(),
            t1.elapsed(),
            v.iter().filter(|i| i.from_game).count()
        );
    }
    for i in v
        .iter()
        .filter(|i| {
            args.len() < 2 || args[1].starts_with("--") || format!("{:?}", i.kind) == args[1]
        })
        .take(if args.iter().any(|a| a == "--all") {
            usize::MAX
        } else {
            25
        })
    {
        println!(
            "{:?} ×{:<4} {} | {} {}",
            i.kind,
            i.times,
            i.mod_id
                .as_deref()
                .or(i.mod_name.as_deref())
                .unwrap_or(if i.from_game { "(juego)" } else { "-" }),
            i.message.chars().take(90).collect::<String>(),
            i.file
                .as_deref()
                .map(|a| format!("({a}:{})", i.line_file.unwrap_or(0)))
                .unwrap_or_default()
        );
    }
    Ok(())
}
