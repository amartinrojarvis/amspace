//! Window-specific keyboard activation without changing stacking order.
//! Event layout follows yabai (MIT, Åsmund Vikane); see docs/YABAI-LICENSE.txt.
//! No event tap, injected code, synthetic click, or SIP change is needed.
use std::{ffi::c_void, sync::OnceLock};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}
/// Main thread only, as required by Carbon. Never compete with password entry.
pub fn secure_input() -> bool {
    unsafe { IsSecureEventInputEnabled() != 0 }
}

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
pub fn activation_needed(
    front_pid: i32,
    applied: Option<(i32, u32)>,
    pid: i32,
    window: u32,
) -> bool {
    front_pid != pid || applied != Some((pid, window))
}
pub fn in_visible_space(window_space: Option<u64>, visible: &[u64]) -> bool {
    !visible.is_empty() && window_space.is_none_or(|space| visible.contains(&space))
}
/// AX decides which surface receives input, but only a currently visible native
/// hit corroborates its window identity. A stale sibling falls back to geometry.
pub fn pointer_target(
    hits: &[(i32, u32)],
    ax_pid: i32,
    ax_window: Option<u32>,
) -> Option<(i32, u32)> {
    ax_window
        .and_then(|id| hits.iter().copied().find(|hit| *hit == (ax_pid, id)))
        .or_else(|| hits.first().copied())
}
#[derive(Default)]
pub struct ProbeBudget(u8);
impl ProbeBudget {
    pub fn reset(&mut self) {
        self.0 = 0;
    }
    pub fn attempt(&mut self) -> bool {
        if self.0 >= 3 {
            return false;
        }
        self.0 += 1;
        true
    }
    pub fn complete(&mut self) {
        self.0 = 3;
    }
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
    if secure_input() || crate::desktop_mode::active() {
        return false;
    }
    process(pid).is_some_and(|p| activation(&p, previous, false))
}
pub fn activate(pid: i32, window: u32, sibling: bool) -> bool {
    if secure_input() || crate::desktop_mode::active() {
        return false;
    }
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

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "read-only native AX diagnostic; requires Accessibility and an interactive desktop"]
    fn inspect_pointer_focus() {
        use crate::{accessibility as ax, window_server as ws};
        use objc2_foundation::NSPoint;
        use std::time::{Duration, Instant};
        unsafe extern "C" {
            fn CGEventCreate(source: ax::Cf) -> ax::Cf;
            fn CGEventGetLocation(event: ax::Cf) -> NSPoint;
        }
        assert!(
            ax::trusted(),
            "Accessibility is required for this diagnostic"
        );
        let start = Instant::now();
        let mut previous = String::new();
        while start.elapsed() < Duration::from_secs(30) {
            objc2::rc::autoreleasepool(|_| {
                let Some(event) = ax::Owned::new(unsafe { CGEventCreate(std::ptr::null()) }) else {
                    return;
                };
                let point = unsafe { CGEventGetLocation(event.0) };
                let native = ws::window_at(point);
                let selected = ax::window_at(point.x, point.y)
                    .and_then(|(window, pid)| ax::window_id(window.0).map(|id| (pid, id)));
                let reported_focus = selected.and_then(|(pid, _)| ax::focused_id(pid));
                let line = format!(
                    "point={point:?} native={native:?} selected={selected:?} reported_focus={reported_focus:?}"
                );
                if previous != line {
                    println!("t={:.3} {line}", start.elapsed().as_secs_f64());
                    previous = line;
                }
            });
            std::thread::sleep(Duration::from_millis(150));
        }
    }
    #[test]
    fn a_stale_window_from_another_space_cannot_receive_pointer_focus() {
        assert!(!super::in_visible_space(Some(7), &[8]));
        assert!(super::in_visible_space(Some(7), &[7, 8]));
        assert!(super::in_visible_space(None, &[8]));
        assert!(!super::in_visible_space(None, &[]));
    }
    #[test]
    fn floating_window_does_not_steal_a_corroborated_tile_hit() {
        let hits = [(7, 100), (7, 200)];
        assert_eq!(super::pointer_target(&hits, 7, Some(200)), Some((7, 200)));
        // The AX sibling is stale and not under the cursor: native geometry wins.
        assert_eq!(
            super::pointer_target(&[(7, 200)], 7, Some(100)),
            Some((7, 200))
        );
        assert_eq!(
            super::pointer_target(&[(8, 200)], 7, Some(100)),
            Some((8, 200))
        );
        assert_eq!(super::pointer_target(&[], 7, Some(100)), None);
    }
    #[test]
    fn transient_hit_failures_have_three_attempts_then_stop_until_pointer_moves() {
        let mut budget = super::ProbeBudget::default();
        assert!(budget.attempt());
        assert!(budget.attempt());
        assert!(budget.attempt());
        for _ in 0..100 {
            assert!(!budget.attempt());
        }
        budget.reset();
        assert!(budget.attempt());
        budget.complete();
        assert!(!budget.attempt());
    }
    #[test]
    fn same_application_is_not_evidence_that_the_requested_window_has_focus() {
        assert!(!super::activation_needed(7, Some((7, 20)), 7, 20));
        assert!(super::activation_needed(7, Some((7, 20)), 7, 21));
        assert!(super::activation_needed(7, Some((7, 20)), 8, 20));
        assert!(super::activation_needed(7, None, 7, 20));
    }
}
