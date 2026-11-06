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
pub struct HotKey {
    key: *mut c_void,
    handler: *mut c_void,
}
impl HotKey {
    pub fn new(key: usize, mods: usize) -> Option<Self> {
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
                ptr::null_mut(),
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
                    id: 1,
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
extern "C" fn pressed(_: *mut c_void, _: *mut c_void, _: *mut c_void) -> i32 {
    crate::ui::toggle_tile_float();
    0
}
