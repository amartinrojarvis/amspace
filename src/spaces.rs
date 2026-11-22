//! Runtime-resolved native Space metadata. No injection or SIP changes.
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
pub(crate) fn display_uuid(id: u32) -> Option<String> {
    unsafe {
        let uuid = Owned::new(CGDisplayCreateUUIDFromDisplayID(id))?;
        let value = Owned::new(CFUUIDCreateString(std::ptr::null(), uuid.0))?;
        Some((&*value.0.cast::<NSString>()).to_string())
    }
}

fn display_number(identifier: Cf) -> Option<u32> {
    if identifier.is_null() {
        return None;
    }
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
    #[test]
    fn indicator_follows_the_pointer_screen_and_falls_back_to_the_active_one() {
        // Secondary portrait (id 2) keeps its own Spaces next to the main one.
        let two = [(Some(1), Some(10u64)), (Some(2), Some(20u64))];
        // Pointer on the portrait screen: its Space, not the main one.
        assert_eq!(chosen_display(&two, Some(2), || unreachable!()), 1);
        // Pointer on the main screen.
        assert_eq!(chosen_display(&two, Some(1), || unreachable!()), 0);
        // No pointer match: the screen owning the WindowServer active Space.
        assert_eq!(chosen_display(&two, Some(9), || 20), 1);
        // Neither source resolves: deterministic first entry.
        assert_eq!(chosen_display(&two, None, || 0), 0);
        // A display without a resolved number still participates by position.
        assert_eq!(chosen_display(&[(None, Some(5))], Some(1), || 5), 0);
    }
    fn fixture(json: &str) -> Owned {
        unsafe {
            let text = NSString::from_str(json);
            let data: Obj = msg_send![&*text, dataUsingEncoding:4usize];
            let value: Obj = msg_send![class!(NSJSONSerialization), JSONObjectWithData:data, options:0usize, error:std::ptr::null_mut::<Obj>()];
            assert!(!value.is_null());
            let retained: Obj = msg_send![value, retain];
            Owned(retained.cast())
        }
    }
    #[test]
    fn desktop_numbering_isolated_from_secondary_and_fullscreen_spaces() {
        objc2::rc::autoreleasepool(|_| {
            let displays = fixture(
                r#"[
                  {"Display Identifier":2,"Spaces":[{"type":0,"ManagedSpaceID":834}]},
                  {"Display Identifier":5,"Spaces":[
                    {"type":0,"ManagedSpaceID":10},{"type":4,"ManagedSpaceID":77},
                    {"type":0,"ManagedSpaceID":20}]}
                ]"#,
            );
            let resolve = |identifier| number(identifier).map(|id| id as u32);
            let primary = || desktops_from_snapshot(displays.0, 5, resolve);
            assert_eq!(primary().next(), Some(10));
            assert_eq!(primary().nth(1), Some(20));
            assert_eq!(primary().nth(2), None);
            assert_eq!(
                desktops_from_snapshot(displays.0, 2, resolve).collect::<Vec<_>>(),
                [834]
            );
            // A disconnected display must not silently select another monitor.
            assert_eq!(desktops_from_snapshot(displays.0, 9, resolve).next(), None);
        });
    }
    #[test]
    fn sticky_or_malformed_membership_is_not_a_single_desktop() {
        objc2::rc::autoreleasepool(|_| {
            assert_eq!(single_window_space(fixture("[42]").0), Some(42));
            for data in ["[]", "[42,43]", "[42,null]", "[\"42\"]", "{}"] {
                assert_eq!(single_window_space(fixture(data).0), None);
            }
        });
    }
    #[test]
    fn settings_rows_keep_first_membership_and_local_desktop_numbers() {
        objc2::rc::autoreleasepool(|_| {
            let displays = fixture(
                r#"[{"Spaces":[{"type":4,"ManagedSpaceID":7},{"type":0,"ManagedSpaceID":8}]},
                    {"Spaces":[{"type":0,"ManagedSpaceID":8},{"type":0,"ManagedSpaceID":9}]}]"#,
            );
            let rows = settings_rows(displays.0);
            let labels: Vec<_> = rows.iter().map(|r| (r.id, r.number, r.display)).collect();
            assert_eq!(labels, [(8, 1, Some(1)), (9, 2, Some(2))]);
        });
    }
    #[test]
    fn missing_display_identifier_cannot_supply_a_shortcut_destination() {
        objc2::rc::autoreleasepool(|_| {
            let displays = fixture(
                r#"[{"Spaces":[{"type":0,"ManagedSpaceID":8}]},
                    {"Display Identifier":"Main","Spaces":[{"type":0,"ManagedSpaceID":9}]}]"#,
            );
            assert_eq!(
                desktops_from_snapshot(displays.0, 2, display_number).collect::<Vec<_>>(),
                [9]
            );
        });
    }
    #[test]
    fn shared_desktops_keep_their_native_order_on_every_display() {
        objc2::rc::autoreleasepool(|_| {
            let displays = fixture(
                r#"[{"Display Identifier":"Main","Spaces":[
                  {"type":0,"ManagedSpaceID":20},{"type":4,"ManagedSpaceID":77},
                  {"type":0,"ManagedSpaceID":10}]}]"#,
            );
            for display in [2, 5] {
                assert_eq!(
                    desktops_from_snapshot(displays.0, display, |_| None).collect::<Vec<_>>(),
                    [20, 10]
                );
            }
        });
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
        detail: crate::i18n::tr("Escritorios no disponibles en esta versión de macOS").into(),
    })
}
fn read() -> Option<Snapshot> {
    let api = api()?;
    let cid = unsafe { (api.connection)() };
    let displays = Owned::new(unsafe { (api.displays)(cid) })?;
    let entries: Vec<_> = array(displays.0)
        .map(|entry| {
            (
                display_number(get(entry, ns_string!("Display Identifier"))),
                number(get(
                    get(entry, ns_string!("Current Space")),
                    ns_string!("ManagedSpaceID"),
                ))
                .map(|id| id as u64),
            )
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    // Every physical screen keeps its own Spaces, so the indicator follows the
    // screen holding the pointer; `SLSGetActiveSpace` only names the main one.
    let pointer = crate::tile_manager::pointer_display();
    let index = chosen_display(&entries, pointer, || unsafe { (api.active)(cid) });
    let display = array(displays.0).nth(index)?;
    let desktops: Vec<_> = array(get(display, ns_string!("Spaces")))
        .filter(|space| number(get(*space, ns_string!("type"))) == Some(0))
        .filter_map(|space| number(get(space, ns_string!("ManagedSpaceID"))).map(|id| id as u64))
        .collect();
    if desktops.is_empty() {
        return None;
    }
    let current = entries[index].1?;
    let count = desktops.len();
    if let Some(i) = position_in_display(&desktops, current) {
        return Some(Snapshot {
            title: indicator(i + 1, count),
            desktops,
            active: Some(i),
            detail: crate::tr_format!(
                "Escritorio {} de {} · pantalla {}",
                "Desktop {} of {} · display {}",
                i + 1,
                count,
                index + 1
            ),
        });
    }
    Some(Snapshot {
        title: "▣".into(),
        desktops,
        active: None,
        detail: crate::tr_format!(
            "Pantalla completa · {count} {}",
            "Full screen · {count} {}",
            crate::i18n::tr(if count == 1 {
                "escritorio"
            } else {
                "escritorios"
            })
        ),
    })
}

/// Screen whose Spaces the indicator reports: the pointer's screen first, then
/// the one owning the WindowServer active Space, then the first entry.
fn chosen_display(
    entries: &[(Option<u32>, Option<u64>)],
    pointer: Option<u32>,
    active: impl FnOnce() -> u64,
) -> usize {
    pointer
        .and_then(|target| entries.iter().position(|(id, _)| *id == Some(target)))
        .or_else(|| {
            let active = active();
            entries
                .iter()
                .position(|(_, current)| *current == Some(active))
        })
        .unwrap_or(0)
}

/// Fail closed for sticky/all-desktop windows and unavailable private API.
pub fn window_space(id: u32) -> Option<u64> {
    single_window_space(copy_window_spaces(id)?.0)
}
fn single_window_space(values: Cf) -> Option<u64> {
    let mut memberships = array(values);
    if memberships.len() != 1 {
        return None;
    }
    number(memberships.next()?).map(|id| id as u64)
}
/// Read all memberships for previews, including windows pinned to several Spaces.
pub fn window_spaces(id: u32) -> Option<Vec<u64>> {
    let values = copy_window_spaces(id)?;
    array(values.0)
        .map(|value| number(value).map(|id| id as u64))
        .collect()
}
fn copy_window_spaces(id: u32) -> Option<Owned> {
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
        Owned::new(copy(
            (api.connection)(),
            7,
            objc2::rc::Retained::as_ptr(&ids).cast(),
        ))
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

#[derive(Debug, PartialEq, Eq)]
pub struct SettingsDesktop {
    pub id: u64,
    number: usize,
    display: Option<usize>,
}
impl SettingsDesktop {
    pub fn title(&self) -> String {
        if let Some(display) = self.display {
            crate::tr_format!(
                "Escritorio {} · Pantalla {}",
                "Desktop {} · Display {}",
                self.number,
                display
            )
        } else {
            crate::tr_format!("Escritorio {}", "Desktop {}", self.number)
        }
    }
}

/// Every real normal desktop, including displays other than the active one.
pub fn settings_desktops() -> Vec<SettingsDesktop> {
    let Some(api) = api() else {
        return vec![];
    };
    let Some(displays) = Owned::new(unsafe { (api.displays)((api.connection)()) }) else {
        return vec![];
    };
    settings_rows(displays.0)
}
fn settings_rows(displays: Cf) -> Vec<SettingsDesktop> {
    let displays = array(displays);
    let multiple = displays.len() > 1;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (display, entry) in displays.enumerate() {
        let ids = array(get(entry, ns_string!("Spaces")))
            .filter(|s| number(get(*s, ns_string!("type"))) == Some(0))
            .filter_map(|s| number(get(s, ns_string!("ManagedSpaceID"))));
        for (index, id) in ids.enumerate() {
            let id = id as u64;
            if seen.insert(id) {
                rows.push(SettingsDesktop {
                    id,
                    number: index + 1,
                    display: multiple.then_some(display + 1),
                });
            }
        }
    }
    rows
}

/// Desktop numbering follows Mission Control on the requested physical screen.
pub fn desktop_ids_for_display(display: u32) -> Vec<u64> {
    desktops_for_display(display).collect()
}

/// Keyboard navigation always addresses the primary display, never the pointer
/// or focused window. Resolve again on every press, including after hotplug.
pub fn shortcut_desktop(index: usize) -> Option<u64> {
    desktops_for_display(unsafe { CGMainDisplayID() }).nth(index)
}

fn desktops_for_display(display: u32) -> impl Iterator<Item = u64> {
    let displays = api().and_then(|api| Owned::new(unsafe { (api.displays)((api.connection)()) }));
    desktops_from_snapshot(
        displays.as_ref().map_or(std::ptr::null(), |value| value.0),
        display,
        display_number,
    )
}

fn desktops_from_snapshot(
    displays: Cf,
    display: u32,
    resolve: impl Fn(Cf) -> Option<u32>,
) -> impl Iterator<Item = u64> {
    let spaces = array(displays).find_map(|entry| {
        let identifier = get(entry, ns_string!("Display Identifier"));
        (equals_text(identifier, "Main") || resolve(identifier) == Some(display))
            .then(|| array(get(entry, ns_string!("Spaces"))))
    });
    // Array retains the selected native list; no temporary desktop Vec or
    // indicator strings are needed for a single keyboard destination.
    spaces
        .into_iter()
        .flatten()
        .filter(|space| number(get(*space, ns_string!("type"))) == Some(0))
        .filter_map(|space| number(get(space, ns_string!("ManagedSpaceID"))).map(|id| id as u64))
}
