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
pub const KEYS: &[(&str, u32)] = &[("T", 17), ("F", 3), ("Espacio", 49), ("Retorno", 36)];
pub const MODIFIERS: &[(&str, u32)] = &[
    ("⌃⌥", 4096 | 2048),
    ("⌘⌥", 256 | 2048),
    ("⌃⌥⇧", 4096 | 2048 | 512),
    ("⌘⇧", 256 | 512),
];
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
                0,
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
