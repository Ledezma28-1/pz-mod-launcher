use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CoreResult, Error, paths};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupEntry {
    #[serde(rename = "ruta")]
    pub path: String,
    #[serde(rename = "tamano")]
    pub size: Option<u64>,
    pub sha256: Option<String>,
    #[serde(default)]
    #[serde(rename = "dejado_apuntado")]
    pub left_recorded: bool,
    #[serde(default)]
    #[serde(rename = "dejado_sha256")]
    pub left_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "raiz")]
    pub root: PathBuf,
    #[serde(rename = "motivo")]
    pub reason: String,
    #[serde(rename = "creado")]
    pub created: u64,
    #[serde(rename = "archivos")]
    pub files: Vec<BackupEntry>,
}

pub fn sha256(path: &Path) -> CoreResult<String> {
    let mut f = File::open(paths::long(path)).map_err(Error::io(path))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(Error::io(path))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn join(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

use crate::system::now;

pub fn create(
    root: &Path,
    relative: &[String],
    target: &Path,
    reason: &str,
) -> CoreResult<Manifest> {
    if target.exists() {
        return Err(Error::Backup(format!(
            "ya existe {}: no se sobrescribe una copia",
            target.display()
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for rel in relative {
        if !seen.insert(rel.clone()) {
            continue;
        }
        let origin = join(root, rel);
        if !paths::long(&origin).is_file() {
            files.push(BackupEntry {
                path: rel.clone(),
                size: None,
                sha256: None,
                left_recorded: false,
                left_sha256: None,
            });
            continue;
        }
        let hash = sha256(&origin)?;
        let copy = join(&target.join("archivos"), rel);
        fs::create_dir_all(paths::long(copy.parent().unwrap_or(target)))
            .map_err(Error::io(&copy))?;
        fs::copy(paths::long(&origin), paths::long(&copy)).map_err(Error::io(&origin))?;
        if sha256(&copy)? != hash {
            return Err(Error::Backup(format!(
                "la copia de {rel} no coincide con el original"
            )));
        }
        let size = fs::metadata(paths::long(&origin))
            .map_err(Error::io(&origin))?
            .len();
        files.push(BackupEntry {
            path: rel.clone(),
            size: Some(size),
            sha256: Some(hash),
            left_recorded: false,
            left_sha256: None,
        });
    }
    let m = Manifest {
        root: root.to_path_buf(),
        reason: reason.to_string(),
        created: now(),
        files,
    };
    let json = serde_json::to_string_pretty(&m).map_err(|source| Error::Json {
        path: target.into(),
        source,
    })?;
    fs::create_dir_all(target).map_err(Error::io(target))?;
    fs::write(target.join("manifiesto.json"), json).map_err(Error::io(target))?;
    Ok(m)
}

fn write(copy: &Path, m: &Manifest) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(m).map_err(|source| Error::Json {
        path: copy.into(),
        source,
    })?;
    crate::profiles::write_atomic(&copy.join("manifiesto.json"), &json)
}

pub fn record_left(copy: &Path, root: &Path) -> CoreResult<()> {
    let mut m = read(copy)?;
    for e in &mut m.files {
        e.left_sha256 = current_hash(&join(root, &e.path))?;
        e.left_recorded = true;
    }
    write(copy, &m)
}

pub fn read(copy: &Path) -> CoreResult<Manifest> {
    let path = copy.join("manifiesto.json");
    let t = fs::read_to_string(&path).map_err(Error::io(&path))?;
    serde_json::from_str(&t).map_err(|source| Error::Json { path, source })
}

fn current_hash(p: &Path) -> CoreResult<Option<String>> {
    if paths::long(p).is_file() {
        sha256(p).map(Some)
    } else {
        Ok(None)
    }
}

pub type LeftState<'a> = &'a dyn Fn(&str) -> Option<Option<String>>;

pub fn restore(copy: &Path, root: &Path, expected: Option<LeftState>) -> CoreResult<usize> {
    let m = read(copy)?;
    for e in &m.files {
        let current = current_hash(&join(root, &e.path))?;
        let acceptable = current == e.sha256
            || current.is_none()
            || (e.left_recorded && current == e.left_sha256)
            || expected
                .and_then(|f| f(&e.path))
                .is_some_and(|x| x == current);
        if !acceptable {
            return Err(Error::Backup(format!(
                "{} cambió después de la copia: no se restaura nada",
                e.path
            )));
        }
    }
    let mut n = 0;
    for e in &m.files {
        let target = join(root, &e.path);
        match &e.sha256 {
            Some(h) => {
                if current_hash(&target)?.as_ref() == Some(h) {
                    continue;
                }
                fs::create_dir_all(paths::long(target.parent().unwrap_or(root)))
                    .map_err(Error::io(&target))?;
                fs::copy(
                    paths::long(&join(&copy.join("archivos"), &e.path)),
                    paths::long(&target),
                )
                .map_err(Error::io(&target))?;
                if current_hash(&target)?.as_ref() != Some(h) {
                    return Err(Error::Backup(format!(
                        "{} no quedó igual que en la copia",
                        e.path
                    )));
                }
                n += 1;
            }
            None => {
                if paths::long(&target).is_file() {
                    fs::remove_file(paths::long(&target)).map_err(Error::io(&target))?;
                    n += 1;
                }
            }
        }
    }
    Ok(n)
}

pub fn remove(copy: &Path, root: &Path) -> CoreResult<usize> {
    let m = read(copy)?;
    for e in &m.files {
        let current = current_hash(&join(root, &e.path))?;
        if current.is_some() && current != e.sha256 {
            return Err(Error::Backup(format!(
                "{} cambió después de la copia: no se borra nada",
                e.path
            )));
        }
    }
    let mut n = 0;
    for e in m.files.iter().filter(|e| e.sha256.is_some()) {
        let p = join(root, &e.path);
        if paths::long(&p).is_file() {
            fs::remove_file(paths::long(&p)).map_err(Error::io(&p))?;
            n += 1;
        }
        let mut d = p.parent().map(Path::to_path_buf);
        while let Some(dir) = d {
            if dir == root || !dir.starts_with(root) {
                break;
            }
            let empty = fs::read_dir(paths::long(&dir))
                .map(|mut it| it.next().is_none())
                .unwrap_or(false);
            if !empty || fs::remove_dir(paths::long(&dir)).is_err() {
                break;
            }
            d = dir.parent().map(Path::to_path_buf);
        }
    }
    Ok(n)
}
