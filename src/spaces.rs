//! Read-only private SkyLight symbols, resolved at runtime. No injection or SIP changes.
use crate::accessibility::{Cf, Owned, array, equals_text, get, number};
use crate::model::indicator;
use crate::native::Obj;
use objc2::{class, msg_send};
use objc2_foundation::{NSString, ns_string};
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
        let spaces = array(get(display, ns_string!("Spaces")));
        let ids: Vec<_> = spaces
            .map(|s| number(get(s, ns_string!("ManagedSpaceID"))).map(|id| id as u64))
            .collect::<Option<_>>()?;
        if let Some(position) = position_in_display(&ids, id) {
            return Some(Destination {
                display: display_number(get(display, ns_string!("Display Identifier")))?,
                position,
                count: ids.len(),
                current: number(get(
                    get(display, ns_string!("Current Space")),
                    ns_string!("ManagedSpaceID"),
                ))? as u64,
            });
        }
    }
    None
}
pub fn neighbor(id: u64, right: bool) -> Option<u64> {
    let api = api()?;
    let displays = Owned::new(unsafe { (api.displays)((api.connection)()) })?;
    for display in array(displays.0) {
        let ids: Vec<_> = array(get(display, ns_string!("Spaces")))
            .map(|s| number(get(s, ns_string!("ManagedSpaceID"))).map(|n| n as u64))
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
        let current = number(get(
            get(display, ns_string!("Current Space")),
            ns_string!("ManagedSpaceID"),
        ))? as u64;
        if current != active {
            continue;
        }
        let spaces = array(get(display, ns_string!("Spaces")));
        let desktops: Vec<_> = spaces
            .filter(|s| number(get(*s, ns_string!("type"))) == Some(0))
            .collect();
        if let Some(i) = desktops
            .iter()
            .position(|s| number(get(*s, ns_string!("ManagedSpaceID"))) == Some(active as i64))
        {
            return Some(Snapshot {
                title: indicator(i + 1, desktops.len()),
                desktops: desktops
                    .iter()
                    .filter_map(|s| {
                        number(get(*s, ns_string!("ManagedSpaceID"))).map(|id| id as u64)
                    })
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
                .filter_map(|s| number(get(*s, ns_string!("ManagedSpaceID"))).map(|id| id as u64))
                .collect(),
            active: None,
            detail: format!("Pantalla completa · {} escritorios", desktops.len()),
        });
    }
    None
}

/// Fail closed for sticky/all-desktop windows and unavailable private API.
pub fn window_space(id: u32) -> Option<u64> {
    let spaces = window_spaces(id)?;
    match spaces.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}
/// Read all memberships for previews, including windows pinned to several Spaces.
pub fn window_spaces(id: u32) -> Option<Vec<u64>> {
    type CopySpaces = unsafe extern "C" fn(i32, u32, Cf) -> Cf;
    static COPY: OnceLock<Option<CopySpaces>> = OnceLock::new();
    let api = api()?;
    let copy = COPY
        .get_or_init(|| unsafe {
            let p = libc::dlsym(libc::RTLD_DEFAULT, c"SLSCopySpacesForWindows".as_ptr());
            if p.is_null() {
                None
            } else {
                Some(std::mem::transmute::<*mut c_void, CopySpaces>(p))
            }
        })
        .as_ref()?;
    unsafe {
        let number: objc2::rc::Retained<objc2::runtime::AnyObject> =
            msg_send![class!(NSNumber),numberWithUnsignedInt:id];
        let ids: objc2::rc::Retained<objc2::runtime::AnyObject> =
            msg_send![class!(NSArray),arrayWithObject:&*number];
        let values = Owned::new(copy(
            (api.connection)(),
            7,
            objc2::rc::Retained::as_ptr(&ids).cast(),
        ))?;
        array(values.0)
            .map(|value| crate::accessibility::number(value).map(|n| n as u64))
            .collect()
    }
}
/// Each display's currently visible, non-fullscreen Space.
pub fn visible_desktops() -> Vec<(u32, u64)> {
    let Some(api) = api() else { return vec![] };
    let Some(displays) = Owned::new(unsafe { (api.displays)((api.connection)()) }) else {
        return vec![];
    };
    let mut visible = Vec::new();
    for display in array(displays.0) {
        let current = get(display, ns_string!("Current Space"));
        if number(get(current, ns_string!("type"))) != Some(0) {
            continue;
        }
        let Some(space) = number(get(current, ns_string!("ManagedSpaceID"))) else {
            continue;
        };
        let identifier = get(display, ns_string!("Display Identifier"));
        if equals_text(identifier, "Main") {
            // With separate Spaces disabled, SkyLight describes one shared Space.
            // Keep an independent tiling area for every connected physical screen.
            unsafe {
                let screens: Obj = msg_send![class!(NSScreen), screens];
                let count: usize = msg_send![screens, count];
                for index in 0..count {
                    let screen: Obj = msg_send![screens, objectAtIndex:index];
                    let desc: Obj = msg_send![screen, deviceDescription];
                    let n: Obj =
                        msg_send![desc, objectForKey:&*NSString::from_str("NSScreenNumber")];
                    let id: u32 = msg_send![n, unsignedIntValue];
                    visible.push((id, space as u64));
                }
            }
        } else if let Some(id) = display_number(identifier) {
            visible.push((id, space as u64));
        }
    }
    visible.sort_unstable();
    visible.dedup();
    visible
}

/// Every real normal desktop, including displays other than the active one.
pub fn settings_desktops() -> Vec<(u64, String)> {
    let Some(api) = api() else {
        return vec![];
    };
    let Some(displays) = Owned::new(unsafe { (api.displays)((api.connection)()) }) else {
        return vec![];
    };
    let groups: Vec<Vec<u64>> = array(displays.0)
        .map(|display| {
            array(get(display, ns_string!("Spaces")))
                .filter(|s| number(get(*s, ns_string!("type"))) == Some(0))
                .filter_map(|s| number(get(s, ns_string!("ManagedSpaceID"))).map(|id| id as u64))
                .collect()
        })
        .collect();
    let multiple = groups.len() > 1;
    let mut seen = std::collections::HashSet::new();
    groups
        .into_iter()
        .enumerate()
        .flat_map(|(display, ids)| {
            ids.into_iter().enumerate().map(move |(i, id)| {
                (
                    id,
                    if multiple {
                        format!("Escritorio {} · Pantalla {}", i + 1, display + 1)
                    } else {
                        format!("Escritorio {}", i + 1)
                    },
                )
            })
        })
        .filter(|(id, _)| seen.insert(*id))
        .collect()
}
