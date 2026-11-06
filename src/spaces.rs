//! Read-only private SkyLight symbols, resolved at runtime. No injection or SIP changes.
use crate::accessibility::{Cf, Owned, array, equals_text, get, number};
use crate::model::indicator;
use crate::native::Obj;
use objc2::{class, msg_send};
use objc2_foundation::NSString;
use std::{
    ffi::{CStr, c_void},
    sync::OnceLock,
};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(id: u32) -> Cf;
    fn CGMainDisplayID() -> u32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFUUIDCreateString(allocator: Cf, uuid: Cf) -> Cf;
}

#[derive(Debug)]
pub struct Destination {
    pub display: u32,
    /// Position includes fullscreen spaces, unlike the numbered desktop label.
    pub position: usize,
    pub count: usize,
    pub current: u64,
}
pub fn destination(id: u64) -> Option<Destination> {
    let api = api()?;
    let displays = Owned::new(unsafe { (api.displays)((api.connection)()) })?;
    for display in array(displays.0) {
        let spaces = array(get(display, "Spaces"));
        let ids: Vec<_> = spaces
            .iter()
            .map(|s| number(get(*s, "ManagedSpaceID")).map(|id| id as u64))
            .collect::<Option<_>>()?;
        if let Some(position) = position_in_display(&ids, id) {
            return Some(Destination {
                display: display_number(get(display, "Display Identifier"))?,
                position,
                count: ids.len(),
                current: number(get(get(display, "Current Space"), "ManagedSpaceID"))? as u64,
            });
        }
    }
    None
}
pub fn neighbor(id: u64, right: bool) -> Option<u64> {
    let api = api()?;
    let displays = Owned::new(unsafe { (api.displays)((api.connection)()) })?;
    for display in array(displays.0) {
        let ids: Vec<_> = array(get(display, "Spaces"))
            .into_iter()
            .map(|s| number(get(s, "ManagedSpaceID")).map(|n| n as u64))
            .collect::<Option<_>>()?;
        if let Some(position) = position_in_display(&ids, id) {
            return ids
                .get(if right {
                    position.checked_add(1)?
                } else {
                    position.checked_sub(1)?
                })
                .copied();
        }
    }
    None
}
fn position_in_display(ids: &[u64], target: u64) -> Option<usize> {
    ids.iter().position(|id| *id == target)
}
fn display_number(identifier: Cf) -> Option<u32> {
    if equals_text(identifier, "Main") {
        return Some(unsafe { CGMainDisplayID() });
    }
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        for index in 0..count {
            let screen: Obj = msg_send![screens, objectAtIndex:index];
            let description: Obj = msg_send![screen, deviceDescription];
            let number: Obj =
                msg_send![description, objectForKey:&*NSString::from_str("NSScreenNumber")];
            let id: u32 = msg_send![number, unsignedIntValue];
            let uuid = Owned::new(CGDisplayCreateUUIDFromDisplayID(id))?;
            let text = Owned::new(CFUUIDCreateString(std::ptr::null(), uuid.0))?;
            if crate::accessibility::CFEqual(identifier, text.0) {
                return Some(id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_uses_full_space_order_and_survives_reordering() {
        // Space 77 represents fullscreen: desktop 2 is at position 2, not 1.
        assert_eq!(position_in_display(&[10, 77, 20], 20), Some(2));
        assert_eq!(position_in_display(&[20, 10, 77], 20), Some(0));
        assert_eq!(position_in_display(&[10, 77], 20), None);
    }
}

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
    pub desktops: Vec<u64>,
    pub active: Option<usize>,
}
pub fn snapshot() -> Snapshot {
    read().unwrap_or_else(|| Snapshot {
        title: "—".into(),
        desktops: vec![],
        active: None,
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
                desktops: desktops
                    .iter()
                    .filter_map(|s| number(get(*s, "ManagedSpaceID")).map(|id| id as u64))
                    .collect(),
                active: Some(i),
                detail: format!(
                    "Escritorio {} de {} · pantalla activa",
                    i + 1,
                    desktops.len()
                ),
            });
        }
        return Some(Snapshot {
            title: "▣".into(),
            desktops: desktops
                .iter()
                .filter_map(|s| number(get(*s, "ManagedSpaceID")).map(|id| id as u64))
                .collect(),
            active: None,
            detail: format!("Pantalla completa · {} escritorios", desktops.len()),
        });
    }
    None
}
