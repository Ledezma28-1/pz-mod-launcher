use std::fs;
use std::path::{Path, PathBuf};

use crate::{CoreResult, Error, paths};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;

pub fn is_link(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

pub fn target(link: &Path) -> Option<PathBuf> {
    let d = fs::read_link(link).ok()?;
    let t = d.to_string_lossy();
    let t = t.strip_prefix(r"\??\").unwrap_or(&t);
    Some(PathBuf::from(paths::display(Path::new(t))))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| {
        paths::display(&paths::long(p))
            .trim_end_matches('\\')
            .to_lowercase()
    };
    n(a) == n(b)
}

pub fn create_junction(link: &Path, target: &Path) -> CoreResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let abs = std::path::absolute(target).map_err(Error::io(target))?;
    if !abs.is_dir() {
        return Err(Error::NotFound(abs.display().to_string()));
    }
    let plain = paths::display(&paths::long(&abs));
    let replacement: Vec<u16> = format!(r"\??\{plain}").encode_utf16().collect();
    let printed: Vec<u16> = plain.encode_utf16().collect();
    let data = 8 + (replacement.len() + 1 + printed.len() + 1) * 2;
    if data > u16::MAX as usize {
        return Err(Error::Backup(format!(
            "ruta demasiado larga para un enlace: {plain}"
        )));
    }
    let mut buf: Vec<u8> = Vec::with_capacity(8 + data);
    buf.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend((data as u16).to_le_bytes());
    buf.extend(0u16.to_le_bytes());
    let n_sub = (replacement.len() * 2) as u16;
    let n_imp = (printed.len() * 2) as u16;
    for v in [0, n_sub, n_sub + 2, n_imp] {
        buf.extend(v.to_le_bytes());
    }
    for c in replacement.iter().chain([&0]).chain(&printed).chain([&0]) {
        buf.extend(c.to_le_bytes());
    }

    fs::create_dir(link).map_err(Error::io(link))?;
    let wide: Vec<u16> = paths::long(&std::path::absolute(link).map_err(Error::io(link))?)
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect();
    // SAFETY: `wide` ends in NUL and `buf` lives for the whole call; the handle is closed.
    let ok = unsafe {
        let h = CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        );
        if h == INVALID_HANDLE_VALUE {
            false
        } else {
            let mut returned = 0u32;
            let r = DeviceIoControl(
                h,
                FSCTL_SET_REPARSE_POINT,
                buf.as_ptr().cast(),
                buf.len() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            );
            CloseHandle(h);
            r != 0
        }
    };
    if !ok {
        let e = std::io::Error::last_os_error();
        let _ = fs::remove_dir(link);
        return Err(Error::io(link)(e));
    }
    Ok(())
}

pub fn wanted(entries: &[crate::conflicts::ProfileEntry]) -> Vec<(String, PathBuf)> {
    use crate::mods::Source;
    entries
        .iter()
        .filter(|x| x.enabled)
        .filter_map(|x| x.found)
        .filter(|m| m.source == Source::Local)
        .filter_map(|m| {
            let n = m.folder.file_name()?.to_string_lossy().to_string();
            Some((n, m.folder.clone()))
        })
        .collect()
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Changes {
    pub created: Vec<String>,
    pub removed: Vec<String>,
    pub taken: Vec<String>,
}

pub fn sync(dir: &Path, wanted: &[(String, PathBuf)]) -> CoreResult<Changes> {
    fs::create_dir_all(dir).map_err(Error::io(dir))?;
    let mut c = Changes::default();
    for e in fs::read_dir(dir).map_err(Error::io(dir))?.flatten() {
        let p = e.path();
        if !is_link(&p) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let valid = wanted.iter().any(|(n, d)| {
            n.eq_ignore_ascii_case(&name) && target(&p).is_some_and(|x| same_path(&x, d))
        });
        if !valid {
            fs::remove_dir(&p).map_err(Error::io(&p))?;
            c.removed.push(name);
        }
    }
    for (name, d) in wanted {
        let p = dir.join(name);
        if is_link(&p) {
            continue;
        }
        if p.exists() {
            c.taken.push(name.clone());
            continue;
        }
        create_junction(&p, d)?;
        c.created.push(name.clone());
    }
    Ok(c)
}
