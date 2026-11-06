//! Window-specific keyboard activation without changing stacking order.
//! Event layout follows yabai (MIT, Åsmund Vikane); see docs/YABAI-LICENSE.txt.
//! No event tap, injected code, synthetic click, or SIP change is needed.
use std::{ffi::c_void, sync::OnceLock};

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Process {
    high: u32,
    low: u32,
}
struct Api {
    process: unsafe extern "C" fn(i32, *mut Process) -> i32,
    activate: unsafe extern "C" fn(*const Process, u32, u32) -> i32,
    post: unsafe extern "C" fn(*const Process, *const u8) -> i32,
}
static API: OnceLock<Option<Api>> = OnceLock::new();
fn api() -> Option<&'static Api> {
    API.get_or_init(|| unsafe {
        // Carbon provides GetProcessForPID; SkyLight provides window activation.
        libc::dlopen(
            c"/System/Library/Frameworks/Carbon.framework/Carbon".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_GLOBAL,
        );
        libc::dlopen(
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_GLOBAL,
        );
        let process = libc::dlsym(libc::RTLD_DEFAULT, c"GetProcessForPID".as_ptr());
        let activate = libc::dlsym(
            libc::RTLD_DEFAULT,
            c"_SLPSSetFrontProcessWithOptions".as_ptr(),
        );
        let post = libc::dlsym(libc::RTLD_DEFAULT, c"SLPSPostEventRecordTo".as_ptr());
        if process.is_null() || activate.is_null() || post.is_null() {
            return None;
        }
        Some(Api {
            process: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(i32, *mut Process) -> i32,
            >(process),
            activate: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*const Process, u32, u32) -> i32,
            >(activate),
            post: std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*const Process, *const u8) -> i32,
            >(post),
        })
    })
    .as_ref()
}
pub fn available() -> bool {
    api().is_some()
}
fn process(pid: i32) -> Option<Process> {
    let mut process = Process::default();
    (unsafe { (api()?.process)(pid, &mut process) } == 0).then_some(process)
}
fn activation(process: &Process, window: u32, active: bool) -> bool {
    let Some(api) = api() else { return false };
    let mut event = [0u8; 0xf8];
    event[4] = 0xf8;
    event[8] = 0x0d;
    event[0x8a] = if active { 1 } else { 2 };
    event[0x3c..0x40].copy_from_slice(&window.to_ne_bytes());
    unsafe { (api.post)(process, event.as_ptr()) == 0 }
}
/// Deactivate the previous sibling before a delayed activation. AppKit needs a
/// separate run-loop turn (40 ms for some apps) to process the two transitions.
pub fn deactivate(pid: i32, previous: u32) -> bool {
    process(pid).is_some_and(|p| activation(&p, previous, false))
}
pub fn activate(pid: i32, window: u32, sibling: bool) -> bool {
    let (Some(api), Some(process)) = (api(), process(pid)) else {
        return false;
    };
    if sibling && !activation(&process, window, true) {
        return false;
    }
    if unsafe { (api.activate)(&process, window, 0x200) } != 0 {
        return false;
    }
    let mut event = [0u8; 0xf8];
    event[4] = 0xf8;
    event[0x3a] = 0x10;
    event[0x3c..0x40].copy_from_slice(&window.to_ne_bytes());
    event[0x20..0x30].fill(0xff);
    for kind in [1, 2] {
        event[8] = kind;
        if unsafe { (api.post)(&process, event.as_ptr()) } != 0 {
            return false;
        }
    }
    true
}
