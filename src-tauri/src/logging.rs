use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_FILES: usize = 10;

static LOCK: Mutex<()> = Mutex::new(());

pub fn folder() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("PZModManager").join("registro"))
}

fn date(seconds: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = (seconds / 86400) as i64;
    let rest = seconds % 86400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, rest / 3600, rest % 3600 / 60, rest % 60)
}

fn file_name(y: i64, m: u32, d: u32) -> String {
    format!("pzmm-{y:04}-{m:02}-{d:02}.log")
}

pub fn today_name() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d, ..) = date(now);
    file_name(y, m, d)
}

fn note(level: &str, text: &str) {
    let Some(dir) = folder() else { return };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d, h, mi, s) = date(now);
    let _g = LOCK.lock();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join(file_name(y, m, d));
    let is_new = !file.exists();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
    {
        let _ = writeln!(
            f,
            "{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z [{level}] {}",
            text.replace('\n', " | ")
        );
    }
    if is_new {
        clear(&dir);
    }
}

fn clear(dir: &std::path::Path) {
    let Ok(it) = std::fs::read_dir(dir) else {
        return;
    };
    let mut v: Vec<PathBuf> = it
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("pzmm-") && n.ends_with(".log"))
        })
        .collect();
    v.sort();
    let extra = v.len().saturating_sub(KEEP_FILES);
    for p in v.into_iter().take(extra) {
        let _ = std::fs::remove_file(p);
    }
}

pub fn info(text: &str) {
    note("INFO", text);
}

pub fn error(text: &str) {
    note("ERROR", text);
}

pub fn watch_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        note("PANICO", &p.to_string());
        previous(p);
    }));
}
