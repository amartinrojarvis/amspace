//! Carbon reserves the shortcut, so it is not also delivered to the focused app.
use std::{ffi::c_void, ptr};
#[repr(C)]
struct HotKeyId {
    signature: u32,
    id: u32,
}
#[repr(C)]
struct EventType {
    class: u32,
    kind: u32,
}
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetEventParameter(
        event: *mut c_void,
        name: u32,
        kind: u32,
        actual: *mut u32,
        size: u32,
        actual_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn CopySymbolicHotKeys(out: *mut crate::accessibility::Cf) -> i32;
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
        count: u32,
        types: *const EventType,
        data: *mut c_void,
        out: *mut *mut c_void,
    ) -> i32;
    fn RegisterEventHotKey(
        key: u32,
        modifiers: u32,
        id: HotKeyId,
        target: *mut c_void,
        options: u32,
        out: *mut *mut c_void,
    ) -> i32;
    fn UnregisterEventHotKey(key: *mut c_void) -> i32;
    fn RemoveEventHandler(handler: *mut c_void) -> i32;
}
// Keep the original first four indices: existing preferences remain valid.
pub const KEYS: &[(&str, u32)] = &[
    ("T", 17),
    ("F", 3),
    ("Espacio", 49),
    ("Retorno", 36),
    ("A", 0),
    ("B", 11),
    ("C", 8),
    ("D", 2),
    ("E", 14),
    ("G", 5),
    ("H", 4),
    ("I", 34),
    ("J", 38),
    ("K", 40),
    ("L", 37),
    ("M", 46),
    ("N", 45),
    ("O", 31),
    ("P", 35),
    ("Q", 12),
    ("R", 15),
    ("S", 1),
    ("U", 32),
    ("V", 9),
    ("W", 13),
    ("X", 7),
    ("Y", 16),
    ("Z", 6),
    ("0", 29),
    ("1", 18),
    ("2", 19),
    ("3", 20),
    ("4", 21),
    ("5", 23),
    ("6", 22),
    ("7", 26),
    ("8", 28),
    ("9", 25),
    ("F1", 122),
    ("F2", 120),
    ("F3", 99),
    ("F4", 118),
    ("F5", 96),
    ("F6", 97),
    ("F7", 98),
    ("F8", 100),
    ("F9", 101),
    ("F10", 109),
    ("F11", 103),
    ("F12", 111),
    ("←", 123),
    ("→", 124),
    ("↓", 125),
    ("↑", 126),
];
pub const MODIFIERS: &[(&str, u32)] = &[
    ("⌃⌥", 4096 | 2048),
    ("⌘⌥", 256 | 2048),
    ("⌃⌥⇧", 4096 | 2048 | 512),
    ("⌘⇧", 256 | 512),
    ("⌃⌘", 4096 | 256),
    ("⌃⇧", 4096 | 512),
    ("⌥⇧", 2048 | 512),
    ("⌘⌥⇧", 256 | 2048 | 512),
    ("⌃⌘⇧", 4096 | 256 | 512),
    ("⌃⌘⌥", 4096 | 256 | 2048),
    ("⌃⌘⌥⇧", 4096 | 256 | 2048 | 512),
];
pub fn key_index(value: isize) -> usize {
    value.clamp(0, KEYS.len() as isize - 1) as usize
}
pub fn modifier_index(value: isize) -> usize {
    value.clamp(0, MODIFIERS.len() as isize - 1) as usize
}
pub fn title(key: usize, mods: usize) -> String {
    format!("{}{}", MODIFIERS[mods].0, KEYS[key].0)
}
fn system_shortcuts() -> Option<std::collections::HashSet<(u32, u32)>> {
    use crate::accessibility::{Owned, array, get, number};
    let mut raw = ptr::null();
    if unsafe { CopySymbolicHotKeys(&mut raw) } != 0 {
        return None;
    }
    let list = Owned::new(raw)?;
    Some(
        array(list.0)
            .into_iter()
            .filter_map(|entry| {
                let enabled = get(entry, "kHISymbolicHotKeyEnabled");
                if enabled.is_null() {
                    return None;
                }
                let on: bool =
                    unsafe { objc2::msg_send![enabled as crate::native::Obj, boolValue] };
                if !on {
                    return None;
                }
                Some((
                    number(get(entry, "kHISymbolicHotKeyCode"))? as u32,
                    number(get(entry, "kHISymbolicHotKeyModifiers"))? as u32,
                ))
            })
            .collect(),
    )
}
fn probe(key: usize, mods: usize) -> bool {
    unsafe {
        let mut handle = ptr::null_mut();
        let status = RegisterEventHotKey(
            KEYS[key].1,
            MODIFIERS[mods].1,
            HotKeyId {
                signature: u32::from_be_bytes(*b"AMSp"),
                id: 1,
            },
            GetApplicationEventTarget(),
            1, // kEventHotKeyExclusive
            &mut handle,
        );
        if status == 0 {
            UnregisterEventHotKey(handle);
            true
        } else {
            false
        }
    }
}
/// Only returns candidates successfully reserved and immediately released on this Mac.
/// Local shortcuts inside other apps cannot be enumerated by macOS.
pub fn suggestions(excluded: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
    let system = system_shortcuts()?;
    let mut result = Vec::new();
    for mods in [2, 7, 8, 9, 10, 0, 1, 4, 5, 6, 3] {
        for key in [2, 14, 13, 3, 0, 41, 42, 43, 44, 45, 46, 47, 48, 49] {
            if excluded.contains(&(key, mods)) || system.contains(&(KEYS[key].1, MODIFIERS[mods].1))
            {
                continue;
            }
            if probe(key, mods) {
                result.push((key, mods));
                if result.len() == 6 {
                    return Some(result);
                }
            }
        }
    }
    Some(result)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Action {
    Float = 1,
    Layouts = 2,
}
fn action(signature: u32, id: u32, handler: usize) -> Option<Action> {
    if signature != u32::from_be_bytes(*b"AMSt") || id as usize != handler {
        return None;
    }
    match id {
        1 => Some(Action::Float),
        2 => Some(Action::Layouts),
        _ => None,
    }
}
pub struct HotKey {
    key: *mut c_void,
    handler: *mut c_void,
}
impl HotKey {
    pub fn new(key: usize, mods: usize, action: Action) -> Option<Self> {
        let physical = (KEYS.get(key)?.1, MODIFIERS.get(mods)?.1);
        if system_shortcuts().is_some_and(|system| system.contains(&physical)) {
            return None;
        }
        unsafe {
            let mut this = Self {
                key: ptr::null_mut(),
                handler: ptr::null_mut(),
            };
            let target = GetApplicationEventTarget();
            let event = EventType {
                class: u32::from_be_bytes(*b"keyb"),
                kind: 6,
            };
            if InstallEventHandler(
                target,
                pressed,
                1,
                &event,
                action as usize as *mut c_void,
                &mut this.handler,
            ) != 0
            {
                return None;
            }
            if RegisterEventHotKey(
                KEYS.get(key)?.1,
                MODIFIERS.get(mods)?.1,
                HotKeyId {
                    signature: u32::from_be_bytes(*b"AMSt"),
                    id: action as u32,
                },
                target,
                1, // kEventHotKeyExclusive
                &mut this.key,
            ) != 0
            {
                return None;
            }
            Some(this)
        }
    }
}
impl Drop for HotKey {
    fn drop(&mut self) {
        unsafe {
            if !self.key.is_null() {
                UnregisterEventHotKey(self.key);
            }
            if !self.handler.is_null() {
                RemoveEventHandler(self.handler);
            }
        }
    }
}
extern "C" fn pressed(_: *mut c_void, event: *mut c_void, data: *mut c_void) -> i32 {
    let mut id = HotKeyId {
        signature: 0,
        id: 0,
    };
    let status = unsafe {
        GetEventParameter(
            event,
            u32::from_be_bytes(*b"----"),
            u32::from_be_bytes(*b"hkid"),
            ptr::null_mut(),
            std::mem::size_of::<HotKeyId>() as u32,
            ptr::null_mut(),
            (&mut id as *mut HotKeyId).cast(),
        )
    };
    if status != 0 {
        return -9874;
    }
    match action(id.signature, id.id, data as usize) {
        Some(Action::Float) => crate::ui::toggle_tile_float(),
        Some(Action::Layouts) => crate::ui::toggle_layout_picker(),
        None => return -9874,
    }
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_catalog_preserves_old_preferences_and_has_no_duplicates() {
        assert_eq!(
            &KEYS[..4],
            &[("T", 17), ("F", 3), ("Espacio", 49), ("Retorno", 36)]
        );
        assert_eq!(MODIFIERS[0].1, 4096 | 2048);
        assert_eq!(MODIFIERS[3].1, 256 | 512);
        assert_eq!(
            KEYS.iter()
                .map(|v| v.1)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            KEYS.len()
        );
        assert_eq!(
            MODIFIERS
                .iter()
                .map(|v| v.1)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            MODIFIERS.len()
        );
        assert!(KEYS.len() > 50 && MODIFIERS.len() > 10);
        assert_eq!(key_index(500), KEYS.len() - 1);
        assert_eq!(modifier_index(-1), 0);
    }
    #[test]
    fn suggested_shortcut_is_exclusive_and_probe_releases_its_reservation() {
        objc2::rc::autoreleasepool(|_| {
            let Some(candidates) = suggestions(&[(2, 2)]) else {
                return;
            };
            assert!(!candidates.contains(&(2, 2)));
            let Some(&(key, mods)) = candidates.first() else {
                return;
            };
            let held =
                HotKey::new(key, mods, Action::Layouts).expect("suggested shortcut must register");
            assert!(!probe(key, mods));
            drop(held);
            assert!(probe(key, mods));
            assert!(probe(key, mods));
        });
    }

    #[test]
    fn shortcuts_dispatch_only_to_the_matching_handler() {
        let signature = u32::from_be_bytes(*b"AMSt");
        assert_eq!(action(signature, 1, 1), Some(Action::Float));
        assert_eq!(action(signature, 2, 2), Some(Action::Layouts));
        assert_eq!(action(signature, 1, 2), None);
        assert_eq!(action(signature, 2, 1), None);
        assert_eq!(action(0, 2, 2), None);
        assert_eq!(action(signature, 3, 3), None);
    }
}
