//! Read-only private SkyLight symbols, resolved at runtime. No injection or SIP changes.
use crate::accessibility::{Cf, Owned, array, get, number};
use crate::model::indicator;
use std::{
    ffi::{CStr, c_void},
    sync::OnceLock,
};

struct Api {
    connection: unsafe extern "C" fn() -> i32,
    displays: unsafe extern "C" fn(i32) -> Cf,
    active: unsafe extern "C" fn(i32) -> u64,
}
static API: OnceLock<Option<Api>> = OnceLock::new();
fn api() -> Option<&'static Api> {
    API.get_or_init(|| unsafe {
        let h = libc::dlopen(
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
            libc::RTLD_LAZY,
        );
        if h.is_null() {
            return None;
        }
        // Keep the framework loaded for the process lifetime: these pointers remain valid.
        let symbol = |a: &CStr, b: &CStr| -> *mut c_void {
            let p = libc::dlsym(h, a.as_ptr());
            if p.is_null() {
                libc::dlsym(h, b.as_ptr())
            } else {
                p
            }
        };
        let c = symbol(c"SLSMainConnectionID", c"CGSMainConnectionID");
        let d = symbol(
            c"SLSCopyManagedDisplaySpaces",
            c"CGSCopyManagedDisplaySpaces",
        );
        let a = symbol(c"SLSGetActiveSpace", c"CGSGetActiveSpace");
        if c.is_null() || d.is_null() || a.is_null() {
            return None;
        }
        Some(Api {
            connection: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> i32>(c),
            displays: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(i32) -> Cf>(d),
            active: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(i32) -> u64>(a),
        })
    })
    .as_ref()
}
#[derive(Debug)]
pub struct Snapshot {
    pub title: String,
    pub detail: String,
}
pub fn snapshot() -> Snapshot {
    read().unwrap_or_else(|| Snapshot {
        title: "—".into(),
        detail: "Escritorios no disponibles en esta versión de macOS".into(),
    })
}
fn read() -> Option<Snapshot> {
    let api = api()?;
    let (displays, active) = unsafe {
        let cid = (api.connection)();
        (Owned::new((api.displays)(cid))?, (api.active)(cid))
    };
    for display in array(displays.0) {
        let current = number(get(get(display, "Current Space"), "ManagedSpaceID"))? as u64;
        if current != active {
            continue;
        }
        let spaces = array(get(display, "Spaces"));
        let desktops: Vec<_> = spaces
            .iter()
            .copied()
            .filter(|s| number(get(*s, "type")) == Some(0))
            .collect();
        if let Some(i) = desktops
            .iter()
            .position(|s| number(get(*s, "ManagedSpaceID")) == Some(active as i64))
        {
            return Some(Snapshot {
                title: indicator(i + 1, desktops.len()),
                detail: format!(
                    "Escritorio {} de {} · pantalla activa",
                    i + 1,
                    desktops.len()
                ),
            });
        }
        return Some(Snapshot {
            title: "▣".into(),
            detail: format!("Pantalla completa · {} escritorios", desktops.len()),
        });
    }
    None
}
