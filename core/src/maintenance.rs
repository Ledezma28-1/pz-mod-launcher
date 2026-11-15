use std::path::{Path, PathBuf};

pub const WEBVIEW_CACHES: [&str; 3] = ["Cache", "Code Cache", "GPUCache"];

pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

pub fn clear_webview_cache(ebwebview: &Path) -> u64 {
    let default = ebwebview.join("Default");
    WEBVIEW_CACHES
        .iter()
        .map(|c| default.join(c))
        .filter(|p| p.is_dir())
        .map(|p| {
            let size = dir_size(&p);
            if std::fs::remove_dir_all(&p).is_ok() {
                size
            } else {
                0
            }
        })
        .sum()
}

pub fn webview_cache_size(ebwebview: &Path) -> u64 {
    let default = ebwebview.join("Default");
    WEBVIEW_CACHES
        .iter()
        .map(|c| dir_size(&default.join(c)))
        .sum()
}

pub fn old_logs(folder: &Path, today: &str) -> Vec<PathBuf> {
    let is_log = |n: &str| {
        n.is_ascii()
            && n.len() == "pzmm-2026-10-28.log".len()
            && n.starts_with("pzmm-")
            && n.ends_with(".log")
            && n[5..15].bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| is_log(n) && n != today)
        })
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}
