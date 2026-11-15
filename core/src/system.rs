#[cfg(windows)]
pub(crate) use windows_sys::Win32::System::Registry::HKEY;

#[cfg(windows)]
fn width(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub(crate) fn log_text(root: HKEY, key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    let (key, value) = (width(key), width(value));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: buffers valid during the call; `size` is their size in bytes.
    let r = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..n.min(buf.len())]))
}

#[cfg(windows)]
pub(crate) fn log_number(root: HKEY, key: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_DWORD, RegGetValueW};
    let (key, value) = (width(key), width(value));
    let mut n: u32 = 0;
    let mut size = 4u32;
    // SAFETY: `n` lives during the call and is 4 bytes long.
    let r = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut n as *mut u32).cast(),
            &mut size,
        )
    };
    (r == ERROR_SUCCESS).then_some(n)
}

pub fn windows_version() -> Option<Vec<u32>> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
        let key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let major = log_number(HKEY_LOCAL_MACHINE, key, "CurrentMajorVersionNumber")?;
        let minor = log_number(HKEY_LOCAL_MACHINE, key, "CurrentMinorVersionNumber").unwrap_or(0);
        let build = log_text(HKEY_LOCAL_MACHINE, key, "CurrentBuildNumber")?
            .parse()
            .ok()?;
        Some(vec![major, minor, build])
    }
    #[cfg(not(windows))]
    None
}

pub fn game_processes() -> Vec<u32> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };
        let mut out = Vec::new();
        // SAFETY: Windows process snapshot; `e` carries its size in `dwSize` and the
        // snapshot is closed at the end.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut has = Process32FirstW(snap, &mut e) != 0;
            while has {
                let length = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                let name = String::from_utf16_lossy(&e.szExeFile[..length]).to_ascii_lowercase();
                let pid = e.th32ProcessID;
                let is_game = match name.as_str() {
                    "projectzomboid64.exe" | "projectzomboid32.exe" => true,
                    "java.exe" | "javaw.exe" => process_path(pid).is_some_and(|r| java_of_game(&r)),
                    _ => false,
                };
                if is_game {
                    out.push(pid);
                }
                has = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        out
    }
    #[cfg(not(windows))]
    Vec::new()
}

fn java_of_game(exe: &std::path::Path) -> bool {
    let Some(bin) = exe.parent() else {
        return false;
    };
    let Some(jre) = bin.parent() else {
        return false;
    };
    bin.file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
        && jre
            .parent()
            .is_some_and(|j| j.join("projectzomboid.jar").is_file())
}

#[cfg(windows)]
fn process_path(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    // SAFETY: the buffer and its size go together; the handle is always closed.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut n) != 0;
        CloseHandle(h);
        ok.then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])))
    }
}

pub trait NoWindow {
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindow for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x0800_0000)
        }
        #[cfg(not(windows))]
        self
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let s = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{a:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

#[cfg(all(test, windows))]
mod tests_processes {
    #[test]
    fn java_of_game_by_its_folder() {
        let root = std::env::temp_dir().join(format!("pzmm-java-{}", std::process::id()));
        let bin = root.join("jre64").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert!(!super::java_of_game(&bin.join("java.exe")));
        std::fs::write(root.join("projectzomboid.jar"), b"").unwrap();
        assert!(super::java_of_game(&bin.join("java.exe")));
        assert!(!super::java_of_game(&root.join("java.exe")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fast() {
        let t = std::time::Instant::now();
        super::game_processes();
        assert!(t.elapsed().as_millis() < 300, "{:?}", t.elapsed());
    }

    #[test]
    fn does_not_find_itself() {
        let own: Vec<u32> = super::game_processes();
        assert!(own.is_empty(), "procesos encontrados: {own:?}");
    }
}
