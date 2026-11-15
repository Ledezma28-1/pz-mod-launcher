use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::{CoreResult, Error, PZ_APP_ID};

type Ptr = *mut c_void;

const CB_CALL_COMPLETED: i32 = 703;
const CB_QUERY_UGC: i32 = 3401;
const CB_SUBSCRIBED: i32 = 1313;
const CB_UNSUBSCRIBED: i32 = 1315;

const SIZE_DETAILS: usize = 16384;
mod off {
    pub const ID: usize = 0;
    pub const RESULT: usize = 8;
    pub const KIND: usize = 12;
    pub const TITLE: (usize, usize) = (24, 129);
    pub const DESCRIPTION: (usize, usize) = (153, 8000);
    pub const OWNER: usize = 8160;
    pub const CREATED: usize = 8168;
    pub const UPDATED: usize = 8172;
    pub const BANNED: usize = 8184;
    pub const TAGS: (usize, usize) = (8187, 1025);
    pub const SIZE: usize = 9492;
    pub const VOTES_UP: usize = 9756;
    pub const VOTES_DOWN: usize = 9760;
    pub const NOTE: usize = 9764;
    pub const NUM_CHILDREN: usize = 9768;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    Trend,
    MostSubscribed,
    Recent,
    Votes,
}

impl Order {
    fn value(self) -> i32 {
        match self {
            Order::Trend => 3,
            Order::MostSubscribed => 12,
            Order::Recent => 1,
            Order::Votes => 0,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Query {
    #[serde(default)]
    pub text: String,
    pub order: Order,
    #[serde(default)]
    pub tags: Vec<String>,
    pub page: u32,
    #[serde(default)]
    pub collections: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub collection: bool,
    pub owner: u64,
    pub author: Option<String>,
    pub created: u32,
    pub updated: u32,
    pub tags: Vec<String>,
    pub size: i32,
    pub votes_up: u32,
    pub votes_down: u32,
    pub score: f32,
    pub subscribers: u64,
    pub preview_view: String,
    pub children: Vec<u64>,
    pub banned: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub items: Vec<Item>,
    pub total: u32,
    pub page: u32,
    pub from_cache: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalState {
    pub id: u64,
    pub subscribed: bool,
    pub installed: bool,
    pub update: bool,
    pub downloading: bool,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub long: bool,
    pub names: bool,
}

fn names() -> &'static Mutex<BTreeMap<u64, String>> {
    static N: OnceLock<Mutex<BTreeMap<u64, String>>> = OnceLock::new();
    N.get_or_init(Default::default)
}
const TTL_CARD: Duration = Duration::from_secs(600);
type Saved = BTreeMap<u64, (Item, Instant, bool)>;
fn cards() -> &'static Mutex<Saved> {
    static F: OnceLock<Mutex<Saved>> = OnceLock::new();
    F.get_or_init(Default::default)
}
pub fn forget_cards() {
    if let Ok(mut c) = cards().lock() {
        c.clear();
    }
}

fn changed() -> &'static Mutex<std::collections::BTreeSet<u64>> {
    static C: OnceLock<Mutex<std::collections::BTreeSet<u64>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub fn available(game: &Path) -> CoreResult<()> {
    let dll = game.join("steam_api64.dll");
    if !dll.is_file() {
        return Err(Error::NotFound(dll.display().to_string()));
    }
    Ok(())
}

macro_rules! api {
    ($( $field:ident : $name:literal => fn($($arg:ty),*) $(-> $ret:ty)? ;)*) => {
        #[allow(non_snake_case)]
        struct Api { $( $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )* }
        impl Api {
            /// SAFETY: `lib` is a loaded module; each symbol is converted to the signature of the
            /// Steamworks flat API (`steam_api_flat.h`) of SDK version 021.
            unsafe fn load(lib: windows_sys::Win32::Foundation::HMODULE) -> CoreResult<Api> {
                use windows_sys::Win32::System::LibraryLoader::GetProcAddress;
                Ok(Api { $( $field: {
                    let p = unsafe { GetProcAddress(lib, concat!($name, "\0").as_ptr()) }
                        .ok_or_else(|| Error::NotFound(format!(
                            "{} en steam_api64.dll del juego (otra versión de Steamworks)", $name)))?;
                    unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p) }
                }, )* })
            }
        }
    };
}

api! {
    init: "SteamAPI_InitFlat" => fn(*mut c_char) -> i32;
    shutdown: "SteamAPI_Shutdown" => fn();
    pipe: "SteamAPI_GetHSteamPipe" => fn() -> i32;
    md_init: "SteamAPI_ManualDispatch_Init" => fn();
    md_frame: "SteamAPI_ManualDispatch_RunFrame" => fn(i32);
    md_next: "SteamAPI_ManualDispatch_GetNextCallback" => fn(i32, *mut Msg) -> bool;
    md_release: "SteamAPI_ManualDispatch_FreeLastCallback" => fn(i32);
    md_result: "SteamAPI_ManualDispatch_GetAPICallResult" => fn(i32, u64, Ptr, i32, i32, *mut bool) -> bool;
    ugc: "SteamAPI_SteamUGC_v021" => fn() -> Ptr;
    friends: "SteamAPI_SteamFriends_v018" => fn() -> Ptr;
    create_page: "SteamAPI_ISteamUGC_CreateQueryAllUGCRequestPage" => fn(Ptr, i32, i32, u32, u32, u32) -> u64;
    create_details: "SteamAPI_ISteamUGC_CreateQueryUGCDetailsRequest" => fn(Ptr, *const u64, u32) -> u64;
    search_text: "SteamAPI_ISteamUGC_SetSearchText" => fn(Ptr, u64, *const c_char) -> bool;
    trend_days: "SteamAPI_ISteamUGC_SetRankedByTrendDays" => fn(Ptr, u64, u32) -> bool;
    tag: "SteamAPI_ISteamUGC_AddRequiredTag" => fn(Ptr, u64, *const c_char) -> bool;
    description_long: "SteamAPI_ISteamUGC_SetReturnLongDescription" => fn(Ptr, u64, bool) -> bool;
    cache_max: "SteamAPI_ISteamUGC_SetAllowCachedResponse" => fn(Ptr, u64, u32) -> bool;
    with_children: "SteamAPI_ISteamUGC_SetReturnChildren" => fn(Ptr, u64, bool) -> bool;
    send: "SteamAPI_ISteamUGC_SendQueryUGCRequest" => fn(Ptr, u64) -> u64;
    result: "SteamAPI_ISteamUGC_GetQueryUGCResult" => fn(Ptr, u64, u32, Ptr) -> bool;
    preview_view: "SteamAPI_ISteamUGC_GetQueryUGCPreviewURL" => fn(Ptr, u64, u32, *mut c_char, u32) -> bool;
    stat: "SteamAPI_ISteamUGC_GetQueryUGCStatistic" => fn(Ptr, u64, u32, i32, *mut u64) -> bool;
    children: "SteamAPI_ISteamUGC_GetQueryUGCChildren" => fn(Ptr, u64, u32, *mut u64, u32) -> bool;
    unpin: "SteamAPI_ISteamUGC_ReleaseQueryUGCRequest" => fn(Ptr, u64) -> bool;
    subscribe: "SteamAPI_ISteamUGC_SubscribeItem" => fn(Ptr, u64) -> u64;
    unsubscribe: "SteamAPI_ISteamUGC_UnsubscribeItem" => fn(Ptr, u64) -> u64;
    download: "SteamAPI_ISteamUGC_DownloadItem" => fn(Ptr, u64, bool) -> bool;
    state: "SteamAPI_ISteamUGC_GetItemState" => fn(Ptr, u64) -> u32;
    download_info: "SteamAPI_ISteamUGC_GetItemDownloadInfo" => fn(Ptr, u64, *mut u64, *mut u64) -> bool;
    num_subscribed: "SteamAPI_ISteamUGC_GetNumSubscribedItems" => fn(Ptr, bool) -> u32;
    subscribed_items: "SteamAPI_ISteamUGC_GetSubscribedItems" => fn(Ptr, *mut u64, u32, bool) -> u32;
    request_user: "SteamAPI_ISteamFriends_RequestUserInformation" => fn(Ptr, u64, bool) -> bool;
    user_name: "SteamAPI_ISteamFriends_GetFriendPersonaName" => fn(Ptr, u64) -> *const c_char;
}

#[repr(C)]
struct Msg {
    user: i32,
    callback: i32,
    data: *mut u8,
    len: i32,
}

pub struct Steamworks {
    api: Api,
    pipe: i32,
    ugc: Ptr,
    friends: Ptr,
}

unsafe impl Send for Steamworks {}

impl Steamworks {
    pub fn start(game: &Path) -> CoreResult<Steamworks> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
        available(game)?;
        let dll = game.join("steam_api64.dll");
        // SAFETY: it only changes this process's environment variables; Steamworks reads the App ID
        // from `SteamAppId` (without writing `steam_appid.txt` anywhere).
        unsafe {
            std::env::set_var("SteamAppId", PZ_APP_ID.to_string());
            std::env::set_var("SteamGameId", PZ_APP_ID.to_string());
        }
        let width: Vec<u16> = dll.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: NUL-terminated path. The module is not freed (Steamworks does not handle that well
        // after `Shutdown`, and loading it again returns the same one).
        let lib = unsafe { LoadLibraryW(width.as_ptr()) };
        if lib.is_null() {
            return Err(Error::io(&dll)(std::io::Error::last_os_error()));
        }
        // SAFETY: flat API symbols with their signatures (see `api!`).
        let api = unsafe { Api::load(lib)? };
        let mut err = [0 as c_char; 1024];
        // SAFETY: 1024-byte buffer, the size `SteamAPI_InitFlat` asks for (SteamErrMsg).
        let r = unsafe { (api.init)(err.as_mut_ptr()) };
        if r != 0 {
            // SAFETY: Steamworks leaves a NUL-terminated string in `err`.
            let msg = unsafe { CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .to_string();
            return Err(Error::Backup(match r {
                2 => "Steam no está abierto: ábrelo para usar el Workshop desde aquí".to_string(),
                _ => format!("Steamworks no se pudo iniciar ({r}): {msg}"),
            }));
        }
        // SAFETY: after `SteamAPI_Init`, as the SDK requires (no callback arrives before).
        unsafe { (api.md_init)() };
        let pipe = unsafe { (api.pipe)() };
        let ugc = unsafe { (api.ugc)() };
        let friends = unsafe { (api.friends)() };
        if ugc.is_null() {
            unsafe { (api.shutdown)() };
            return Err(Error::Backup(
                "Steamworks no dio la interfaz del Workshop".into(),
            ));
        }
        Ok(Steamworks {
            api,
            pipe,
            ugc,
            friends,
        })
    }

    fn wait(&self, call: u64, callback: i32, len: usize) -> CoreResult<Vec<u8>> {
        if call == 0 {
            return Err(Error::Backup("Steam rechazó la petición".into()));
        }
        let end = Instant::now() + Duration::from_secs(15);
        while Instant::now() < end {
            // SAFETY: manual callback dispatch on this session's pipe; each message
            // is freed before asking for the next one.
            unsafe {
                (self.api.md_frame)(self.pipe);
                let mut m = Msg {
                    user: 0,
                    callback: 0,
                    data: std::ptr::null_mut(),
                    len: 0,
                };
                while (self.api.md_next)(self.pipe, &mut m) {
                    if m.callback == CB_CALL_COMPLETED
                        && m.len >= 8
                        && std::ptr::read_unaligned(m.data as *const u64) == call
                    {
                        let mut buf = vec![0u8; len];
                        let mut failure = false;
                        let ok = (self.api.md_result)(
                            self.pipe,
                            call,
                            buf.as_mut_ptr().cast(),
                            len as i32,
                            callback,
                            &mut failure,
                        );
                        (self.api.md_release)(self.pipe);
                        if !ok || failure {
                            return Err(Error::Backup(
                                "Steam no respondió bien (¿sin conexión?)".into(),
                            ));
                        }
                        return Ok(buf);
                    }
                    (self.api.md_release)(self.pipe);
                }
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Err(Error::Backup(
            "Steam no responde (sin conexión o servidores caídos)".into(),
        ))
    }

    fn read_query(&mut self, h: u64, page: u32, o: Options, uncached: bool) -> CoreResult<Page> {
        // SAFETY: `h` is a valid query handle of this session.
        let call = unsafe {
            if uncached {
                (self.api.cache_max)(self.ugc, h, 0);
            }
            (self.api.description_long)(self.ugc, h, o.long);
            (self.api.with_children)(self.ugc, h, true);
            (self.api.send)(self.ugc, h)
        };
        let r = self.wait(call, CB_QUERY_UGC, 288);
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                unsafe { (self.api.unpin)(self.ugc, h) };
                return Err(e);
            }
        };
        let handle = u64::from_le_bytes(r[0..8].try_into().expect("8 bytes"));
        let eres = i32::from_le_bytes(r[8..12].try_into().expect("4 bytes"));
        let returned = u32::from_le_bytes(r[12..16].try_into().expect("4 bytes"));
        let total = u32::from_le_bytes(r[16..20].try_into().expect("4 bytes"));
        let from_cache = r[20] != 0;
        if eres != 1 {
            unsafe { (self.api.unpin)(self.ugc, handle) };
            return Err(Error::Backup(format!(
                "Steam devolvió el error {eres} (EResult) a la consulta"
            )));
        }
        let mut items = Vec::with_capacity(returned as usize);
        let mut buf = vec![0u8; SIZE_DETAILS];
        let mut url = [0 as c_char; 1024];
        for i in 0..returned {
            // SAFETY: buffers of the size the calls expect; index < returned.
            unsafe {
                if !(self.api.result)(self.ugc, handle, i, buf.as_mut_ptr().cast()) {
                    continue;
                }
            }
            let mut e = read_details(&buf);
            let mut subs = 0u64;
            unsafe {
                (self.api.stat)(self.ugc, handle, i, 0, &mut subs);
                if (self.api.preview_view)(self.ugc, handle, i, url.as_mut_ptr(), url.len() as u32)
                {
                    e.preview_view = CStr::from_ptr(url.as_ptr()).to_string_lossy().to_string();
                }
            }
            e.subscribers = subs;
            let n = u32::from_le_bytes(
                buf[off::NUM_CHILDREN..off::NUM_CHILDREN + 4]
                    .try_into()
                    .expect("4 bytes"),
            );
            if n > 0 {
                let mut hs = vec![0u64; n as usize];
                if unsafe { (self.api.children)(self.ugc, handle, i, hs.as_mut_ptr(), n) } {
                    e.children = hs;
                }
            }
            items.push(e);
        }
        unsafe { (self.api.unpin)(self.ugc, handle) };
        self.put_names(&mut items, o.names);
        Ok(Page {
            items,
            total,
            page,
            from_cache,
        })
    }

    fn put_names(&mut self, items: &mut [Item], wait: bool) {
        if self.friends.is_null() {
            return;
        }
        let known = names().lock().map(|n| n.clone()).unwrap_or_default();
        let missing: Vec<u64> = items
            .iter()
            .map(|e| e.owner)
            .filter(|d| *d != 0 && !known.contains_key(d))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut pending_ids = Vec::new();
        for d in missing {
            // SAFETY: valid friends interface; returns true if the data has to be waited for.
            if unsafe { (self.api.request_user)(self.friends, d, true) } {
                pending_ids.push(d);
            } else {
                self.read_name(d);
            }
        }
        if wait && !pending_ids.is_empty() {
            let end = Instant::now() + Duration::from_millis(500);
            while Instant::now() < end {
                unsafe {
                    (self.api.md_frame)(self.pipe);
                    let mut m = Msg {
                        user: 0,
                        callback: 0,
                        data: std::ptr::null_mut(),
                        len: 0,
                    };
                    while (self.api.md_next)(self.pipe, &mut m) {
                        (self.api.md_release)(self.pipe);
                    }
                }
                pending_ids.retain(|d| !self.read_name(*d));
                if pending_ids.is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        let n = names().lock().map(|n| n.clone()).unwrap_or_default();
        for e in items {
            e.author = n.get(&e.owner).cloned();
        }
    }

    fn read_name(&mut self, d: u64) -> bool {
        // SAFETY: returns a NUL-terminated Steam string (or empty if it does not know it).
        let n = unsafe {
            let p = (self.api.user_name)(self.friends, d);
            if p.is_null() {
                return false;
            }
            CStr::from_ptr(p).to_string_lossy().to_string()
        };
        if n.is_empty() || n == "[unknown]" {
            return false;
        }
        if let Ok(mut m) = names().lock() {
            m.insert(d, n);
        }
        true
    }

    pub fn search(&mut self, c: &Query) -> CoreResult<Page> {
        let order = if c.text.trim().is_empty() {
            c.order.value()
        } else {
            11
        };
        let kind = if c.collections { 3 } else { 0 };
        let page = c.page.max(1);
        // SAFETY: valid UGC interface; NUL-terminated strings that live for the whole call.
        let h =
            unsafe { (self.api.create_page)(self.ugc, order, kind, PZ_APP_ID, PZ_APP_ID, page) };
        if h == u64::MAX {
            return Err(Error::Backup("Steam no aceptó la búsqueda".into()));
        }
        unsafe {
            if !c.text.trim().is_empty() {
                let t = string_c(c.text.trim());
                (self.api.search_text)(self.ugc, h, t.as_ptr());
            } else if c.order == Order::Trend {
                (self.api.trend_days)(self.ugc, h, 7);
            }
            for e in &c.tags {
                let t = string_c(e);
                (self.api.tag)(self.ugc, h, t.as_ptr());
            }
        }
        let mut p = self.read_query(
            h,
            page,
            Options {
                long: false,
                names: true,
            },
            false,
        )?;
        let first: Vec<(u64, u64)> = p
            .items
            .iter()
            .filter(|e| e.collection && e.preview_view.is_empty())
            .filter_map(|e| e.children.first().map(|h| (e.id, *h)))
            .collect();
        if !first.is_empty() {
            let ids: Vec<u64> = first.iter().map(|(_, h)| *h).collect();
            let light = Options {
                long: false,
                names: false,
            };
            if let Ok(detail) = self.details(&ids, light) {
                for e in &mut p.items {
                    let Some((_, h)) = first.iter().find(|(c, _)| *c == e.id) else {
                        continue;
                    };
                    if let Some(d) = detail.iter().find(|d| d.id == *h) {
                        e.preview_view = d.preview_view.clone();
                    }
                }
            }
        }
        Ok(p)
    }

    pub fn details(&mut self, ids: &[u64], o: Options) -> CoreResult<Vec<Item>> {
        let mut saved: BTreeMap<u64, Item> = BTreeMap::new();
        if let Ok(c) = cards().lock() {
            for id in ids {
                if let Some((e, when, long)) = c.get(id)
                    && when.elapsed() < TTL_CARD
                    && (*long || !o.long)
                {
                    saved.insert(*id, e.clone());
                }
            }
        }
        let missing: Vec<u64> = ids
            .iter()
            .filter(|id| !saved.contains_key(id))
            .copied()
            .collect();
        for chunk in missing.chunks(50) {
            // SAFETY: pointer and length of a valid slice.
            let h =
                unsafe { (self.api.create_details)(self.ugc, chunk.as_ptr(), chunk.len() as u32) };
            if h == u64::MAX {
                return Err(Error::Backup(
                    "Steam no aceptó la consulta de detalles".into(),
                ));
            }
            let uncached = changed()
                .lock()
                .map(|mut c| chunk.iter().filter(|id| c.remove(id)).count() > 0)
                .unwrap_or(false);
            let fresh = self.read_query(h, 1, o, uncached)?.items;
            if let Ok(mut c) = cards().lock() {
                for e in &fresh {
                    let overrides = o.long
                        || c.get(&e.id)
                            .is_none_or(|(_, when, long)| !*long || when.elapsed() >= TTL_CARD);
                    if overrides {
                        c.insert(e.id, (e.clone(), Instant::now(), o.long));
                    }
                }
            }
            saved.extend(fresh.into_iter().map(|e| (e.id, e)));
        }
        Ok(ids.iter().filter_map(|id| saved.remove(id)).collect())
    }

    pub fn check(&mut self, ids: &[u64]) -> CoreResult<Vec<(u64, u32)>> {
        let mut out = Vec::with_capacity(ids.len());
        let light = Options {
            long: false,
            names: false,
        };
        for chunk in ids.chunks(50) {
            // SAFETY: pointer and length of a valid slice.
            let h =
                unsafe { (self.api.create_details)(self.ugc, chunk.as_ptr(), chunk.len() as u32) };
            if h == u64::MAX {
                return Err(Error::Backup(
                    "Steam no aceptó la consulta de detalles".into(),
                ));
            }
            let seen = self.read_query(h, 1, light, true)?.items;
            if let (Ok(mut f), Ok(mut c)) = (cards().lock(), changed().lock()) {
                for e in &seen {
                    if f.get(&e.id).is_some_and(|(g, _, _)| g.updated != e.updated) {
                        f.remove(&e.id);
                    }
                    c.insert(e.id);
                }
            }
            out.extend(seen.iter().map(|e| (e.id, e.updated)));
        }
        Ok(out)
    }

    pub fn subscribe(&self, id: u64) -> CoreResult<()> {
        let call = unsafe { (self.api.subscribe)(self.ugc, id) };
        check_eresult(&self.wait(call, CB_SUBSCRIBED, 16)?, "suscribirse a", id)?;
        // SAFETY: valid UGC interface. If Steam does not accept it, the download still arrives later.
        unsafe { (self.api.download)(self.ugc, id, true) };
        Ok(())
    }

    pub fn download(&self, id: u64) -> bool {
        // SAFETY: valid UGC interface.
        unsafe { (self.api.download)(self.ugc, id, true) }
    }

    pub fn pump(&self) {
        // SAFETY: same as in `wait`.
        unsafe {
            (self.api.md_frame)(self.pipe);
            let mut m = Msg {
                user: 0,
                callback: 0,
                data: std::ptr::null_mut(),
                len: 0,
            };
            while (self.api.md_next)(self.pipe, &mut m) {
                (self.api.md_release)(self.pipe);
            }
        }
    }

    pub fn unsubscribe(&self, id: u64) -> CoreResult<()> {
        let call = unsafe { (self.api.unsubscribe)(self.ugc, id) };
        check_eresult(
            &self.wait(call, CB_UNSUBSCRIBED, 16)?,
            "anular la suscripción a",
            id,
        )
    }

    pub fn state(&self, id: u64) -> LocalState {
        let (mut downloaded, mut total) = (0u64, 0u64);
        // SAFETY: valid UGC interface; pointers to local variables.
        let (s, info) = unsafe {
            (
                (self.api.state)(self.ugc, id),
                (self.api.download_info)(self.ugc, id, &mut downloaded, &mut total),
            )
        };
        LocalState {
            id,
            subscribed: s & 1 != 0,
            installed: s & 4 != 0,
            update: s & 8 != 0,
            downloading: s & (16 | 32) != 0,
            downloaded: if info { downloaded } else { 0 },
            total: if info { total } else { 0 },
        }
    }

    pub fn subscribed_items(&self) -> Vec<u64> {
        // SAFETY: buffer of the size `GetNumSubscribedItems` reports.
        unsafe {
            let n = (self.api.num_subscribed)(self.ugc, false);
            let mut v = vec![0u64; n as usize];
            let m = (self.api.subscribed_items)(self.ugc, v.as_mut_ptr(), n, false);
            v.truncate(m as usize);
            v
        }
    }
}

impl Drop for Steamworks {
    fn drop(&mut self) {
        // SAFETY: closes the session opened in `start`.
        unsafe { (self.api.shutdown)() };
    }
}

fn string_c(s: &str) -> Vec<c_char> {
    s.bytes()
        .filter(|b| *b != 0)
        .map(|b| b as c_char)
        .chain([0])
        .collect()
}

fn check_eresult(r: &[u8], que: &str, id: u64) -> CoreResult<()> {
    let eres = i32::from_le_bytes(r[0..4].try_into().expect("4 bytes"));
    if eres == 1 {
        Ok(())
    } else {
        Err(Error::Backup(format!(
            "Steam no dejó {que} {id} (EResult {eres})"
        )))
    }
}

fn text(buf: &[u8], (from, len): (usize, usize)) -> String {
    let s = &buf[from..from + len];
    let end = s.iter().position(|b| *b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).to_string()
}

fn read_details(b: &[u8]) -> Item {
    let u32_ = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"));
    let _ = i32::from_le_bytes(b[off::RESULT..off::RESULT + 4].try_into().expect("4"));
    Item {
        id: u64::from_le_bytes(b[off::ID..off::ID + 8].try_into().expect("8 bytes")),
        title: text(b, off::TITLE),
        description: text(b, off::DESCRIPTION),
        collection: u32_(off::KIND) == 2,
        owner: u64::from_le_bytes(b[off::OWNER..off::OWNER + 8].try_into().expect("8 bytes")),
        author: None,
        created: u32_(off::CREATED),
        updated: u32_(off::UPDATED),
        tags: text(b, off::TAGS)
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        size: i32::from_le_bytes(b[off::SIZE..off::SIZE + 4].try_into().expect("4")),
        votes_up: u32_(off::VOTES_UP),
        votes_down: u32_(off::VOTES_DOWN),
        score: f32::from_le_bytes(b[off::NOTE..off::NOTE + 4].try_into().expect("4")),
        subscribers: 0,
        preview_view: String::new(),
        children: Vec::new(),
        banned: b[off::BANNED] != 0,
    }
}

pub const TAGS: [&str; 31] = [
    "Build 40",
    "Build 41",
    "Build 42",
    "Animals",
    "Audio",
    "Balance",
    "Building",
    "Clothing/Armor",
    "Farming",
    "Food",
    "Framework",
    "Hardmode",
    "Interface",
    "Items",
    "Language/Translation",
    "Literature",
    "Map",
    "Military",
    "Misc",
    "Models",
    "Multiplayer",
    "Pop Culture",
    "QoL",
    "Realistic",
    "Silly/Fun",
    "Skills",
    "Textures",
    "Traits",
    "Vehicles",
    "Weapons",
    "WIP",
];

pub fn bbcode_to_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(']') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let tag = tail[1..j].trim().to_ascii_lowercase();
        rest = &tail[j + 1..];
        if tag == "img" || tag == "previewyoutube" || tag.starts_with("previewyoutube=") {
            let close = format!("[/{}]", tag.split('=').next().unwrap_or(""));
            match rest.to_ascii_lowercase().find(&close) {
                Some(k) => rest = &rest[k + close.len()..],
                None => rest = "",
            }
        } else if tag == "*" {
            out.push_str("• ");
        } else if tag == "hr" {
            out.push('\n');
        } else if !tag.is_empty()
            && tag.len() <= 40
            && tag
                .trim_start_matches('/')
                .split('=')
                .next()
                .is_some_and(|n| n.chars().all(|c| c.is_ascii_alphanumeric()))
        {
        } else {
            out.push('[');
            out.push_str(&tail[1..j]);
            out.push(']');
        }
    }
    out.push_str(rest);
    let mut clean = String::with_capacity(out.len());
    let mut empty = 0;
    for l in out.lines() {
        if l.trim().is_empty() {
            empty += 1;
            if empty > 1 {
                continue;
            }
        } else {
            empty = 0;
        }
        clean.push_str(l.trim_end());
        clean.push('\n');
    }
    clean.trim().to_string()
}
