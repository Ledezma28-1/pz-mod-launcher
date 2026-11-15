use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::game::{self, Game};
use crate::profiles::Profile;
use crate::system::NoWindow;
use crate::{CoreResult, Error, system};

#[derive(Debug, Clone, Serialize)]
pub struct LaunchLine {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub directory: PathBuf,
}

impl LaunchLine {
    pub fn text(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .map(|a| {
                if a.contains(' ') {
                    format!("\"{a}\"")
                } else {
                    a
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ZbOptions {
    pub policy: String,
    pub verbosity: i32,
}

fn args_windows(per_windows: &[(String, Vec<String>)], system: &[u32]) -> Vec<String> {
    let parse = |k: &str| {
        k.split('.')
            .filter_map(|n| n.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    per_windows
        .iter()
        .filter(|(k, _)| parse(k).as_slice() <= system)
        .max_by(|a, b| parse(&a.0).cmp(&parse(&b.0)))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

pub const CRASH_FILE_PATTERN: &str = "hs_err_pid%p.log";

pub fn absolute_cache(game: &Path, cache: &Path) -> CoreResult<PathBuf> {
    let abs = std::path::absolute(cache).map_err(Error::io(cache))?;
    let norm = |p: &Path| crate::paths::display(&crate::paths::long(p)).to_lowercase();
    let (j, c) = (norm(game), norm(&abs));
    if c == j || c.starts_with(&format!("{}\\", j.trim_end_matches('\\'))) {
        return Err(Error::Backup(format!(
            "la carpeta del perfil ({}) no puede estar dentro de la del juego",
            abs.display()
        )));
    }
    Ok(abs)
}

pub fn build(
    game: &Game,
    profile: &Profile,
    cache: &Path,
    zb: Option<&ZbOptions>,
) -> CoreResult<LaunchLine> {
    let cache = &absolute_cache(&game.folder, cache)?;
    let launcher = game
        .launcher
        .as_ref()
        .ok_or_else(|| Error::NotFound("ProjectZomboid64.json legible".into()))?;
    let java = game.folder.join("jre64").join("bin").join("java.exe");
    if !java.is_file() {
        return Err(Error::NotFound(java.display().to_string()));
    }
    let mut args: Vec<String> = Vec::new();
    if let Some(o) = zb {
        let mut a = String::from("-agentlib:zbNative");
        let mut params = Vec::new();
        if ["deny-new", "allow-all"].contains(&o.policy.as_str()) {
            params.push(format!("policy={}", o.policy));
        }
        if o.verbosity != 0 {
            params.push(format!("verbosity={}", o.verbosity));
        }
        if !params.is_empty() {
            a.push('=');
            a.push_str(&params.join(","));
        }
        args.push(a);
    }
    let xmx = profile.launch.xmx.trim();
    for a in &launcher.vm_args {
        if game::foreign_arg(a) {
            continue;
        }
        if !xmx.is_empty() && a.starts_with("-Xmx") {
            args.push(format!("-Xmx{xmx}"));
        } else {
            args.push(a.clone());
        }
    }
    if !xmx.is_empty() && !launcher.vm_args.iter().any(|a| a.starts_with("-Xmx")) {
        args.push(format!("-Xmx{xmx}"));
    }
    let so = system::windows_version().unwrap_or_else(|| vec![10, 0, 0]);
    args.extend(
        args_windows(&launcher.per_windows, &so)
            .into_iter()
            .filter(|a| !game::foreign_arg(a)),
    );
    args.extend(
        profile
            .launch
            .jvm_extra
            .iter()
            .filter(|a| !is_dangerous_arg(a))
            .cloned(),
    );
    args.push(format!(
        "-XX:ErrorFile={}",
        cache.join(CRASH_FILE_PATTERN).display()
    ));
    args.push("-cp".into());
    args.push(launcher.classpath.join(";"));
    args.push(launcher.class_main.clone());
    args.push(format!("-cachedir={}", cache.display()));
    if !profile.launch.modfolders.is_empty() {
        args.push("-modfolders".into());
        args.push(profile.launch.modfolders.join(","));
    }
    args.extend(
        profile
            .launch
            .game_extra
            .iter()
            .filter(|a| !is_dangerous_arg(a))
            .cloned(),
    );
    Ok(LaunchLine {
        program: java,
        args,
        directory: game.folder.clone(),
    })
}

pub fn is_dangerous_arg(a: &str) -> bool {
    let a = a.trim().to_ascii_lowercase();
    [
        "-javaagent",
        "-agentpath",
        "-agentlib",
        "-xbootclasspath",
        "-xx:onerror",
        "-xx:onoutofmemoryerror",
        "-xx:errorfile",
        "-djava.library.path",
        "-cachedir",
        "-modfolders",
    ]
    .iter()
    .any(|p| a.starts_with(p))
}

pub fn launch(line: &LaunchLine) -> CoreResult<Child> {
    Command::new(&line.program)
        .args(&line.args)
        .no_window()
        .current_dir(&line.directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Error::io(&line.program))
}

#[derive(Debug, Clone, Serialize)]
pub enum End {
    Exited(Option<i32>),
    Idle,
    TimedOut,
}

pub fn wait(child: &mut Child, console: &Path, idle: Duration, max: Duration) -> End {
    let start = Instant::now();
    let (mut size, mut change) = (u64::MAX, Instant::now());
    loop {
        std::thread::sleep(Duration::from_secs(1));
        if let Ok(Some(e)) = child.try_wait() {
            return End::Exited(e.code());
        }
        let t = fs::metadata(console).map(|m| m.len()).unwrap_or(0);
        if t != size {
            (size, change) = (t, Instant::now());
        } else if t > 0 && change.elapsed() >= idle {
            return End::Idle;
        }
        if start.elapsed() >= max {
            return End::TimedOut;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConsoleReport {
    pub version: Option<String>,
    pub steam: bool,
    pub loaded: Vec<String>,
    pub overrides: Vec<(String, String)>,
    pub exceptions: Vec<String>,
    pub zb: Vec<String>,
}

pub fn read_console(text: &str) -> ConsoleReport {
    let mut inf = ConsoleReport::default();
    for l in text.lines() {
        let body = l.split_once("> ").map(|(_, c)| c).unwrap_or(l);
        if let Some(v) = body.strip_prefix("version=") {
            inf.version
                .get_or_insert_with(|| v.split(" demo=").next().unwrap_or(v).to_string());
        } else if body.starts_with("SteamUtils initialised successfully") {
            inf.steam = true;
        } else if l.contains(": Mod ") && body.starts_with("loading ") {
            inf.loaded.push(body["loading ".len()..].trim().to_string());
        } else if let Some(r) = body.strip_prefix("mod \"")
            && let Some((m, path)) = r.split_once("\" overrides ")
        {
            inf.overrides.push((m.to_string(), path.trim().to_string()));
        } else if body.starts_with("[ZB]") {
            inf.zb.push(body.to_string());
        } else if body.starts_with("Exception in thread")
            || (body.starts_with("java.") && (body.contains("Exception") || body.contains("Error")))
        {
            inf.exceptions.push(body.to_string());
        }
    }
    inf
}
