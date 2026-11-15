use std::path::{Path, PathBuf};

const PREFIX: &str = r"\\?\";
const PREFIX_UNC: &str = r"\\?\UNC\";

pub fn long(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if !cfg!(windows) || text.starts_with(PREFIX) || !path.is_absolute() {
        return path.to_path_buf();
    }
    let normal: String = text.replace('/', "\\");
    let mut parts: Vec<&str> = Vec::new();
    for p in normal.split('\\') {
        match p {
            "." | "" if !parts.is_empty() => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    let joined = parts.join("\\");
    if let Some(rest) = normal.strip_prefix(r"\\") {
        let rest: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty()).collect();
        return PathBuf::from(format!("{PREFIX_UNC}{}", rest.join("\\")));
    }
    PathBuf::from(format!("{PREFIX}{joined}"))
}

pub fn display(path: &Path) -> String {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(PREFIX_UNC) {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(PREFIX).unwrap_or(&text).to_string()
}

pub fn relative(base: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}
