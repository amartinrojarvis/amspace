//! Presentation state from WindowManager metadata, with legacy Dock signals.
use std::{
    ffi::c_void,
    sync::atomic::{AtomicU8, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    #[default]
    Normal = 0,
    Overview = 1,
    Desktop = 2,
}
pub const NOTIFICATIONS: &[(&str, Mode)] = &[
    ("AXExposeShowAllWindows", Mode::Overview),
    ("AXExposeShowFrontWindows", Mode::Overview),
    ("AXExposeShowDesktop", Mode::Desktop),
    ("AXExposeExit", Mode::Normal),
];
pub const DISTRIBUTED_NOTIFICATIONS: &[(&str, Mode)] = &[
    ("com.apple.showdesktop.start", Mode::Desktop),
    ("com.apple.showdesktop.stop", Mode::Normal),
    ("com.apple.expose.start", Mode::Overview),
    ("com.apple.expose.stop", Mode::Normal),
    ("com.apple.expose.front.start", Mode::Overview),
    ("com.apple.expose.front.stop", Mode::Normal),
    ("com.apple.MissionControl.start", Mode::Overview),
    ("com.apple.MissionControl.stop", Mode::Normal),
];
pub fn distributed_mode(name: &str) -> Option<Mode> {
    DISTRIBUTED_NOTIFICATIONS
        .iter()
        .find_map(|(event, mode)| (*event == name).then_some(*mode))
}
static MODE: AtomicU8 = AtomicU8::new(0);
static SIGNAL_MODE: AtomicU8 = AtomicU8::new(0);
static COMPOSITOR_MODE: AtomicU8 = AtomicU8::new(0);
unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void),
    );
}
pub fn active() -> bool {
    MODE.load(Ordering::Acquire) != 0
}
pub fn current() -> Mode {
    match MODE.load(Ordering::Acquire) {
        1 => Mode::Overview,
        2 => Mode::Desktop,
        _ => Mode::Normal,
    }
}
pub fn update(mode: Mode) {
    SIGNAL_MODE.store(mode as u8, Ordering::Release);
    publish(merged(current_signal(), current_compositor()));
}
fn decoded(value: u8) -> Mode {
    match value {
        1 => Mode::Overview,
        2 => Mode::Desktop,
        _ => Mode::Normal,
    }
}
fn current_signal() -> Mode {
    decoded(SIGNAL_MODE.load(Ordering::Acquire))
}
fn current_compositor() -> Mode {
    decoded(COMPOSITOR_MODE.load(Ordering::Acquire))
}
fn merged(signal: Mode, compositor: Mode) -> Mode {
    decoded((signal as u8).max(compositor as u8))
}
pub fn observe_compositor(mode: Mode) {
    let previous = COMPOSITOR_MODE.swap(mode as u8, Ordering::AcqRel);
    if mode == Mode::Normal && previous != 0 {
        SIGNAL_MODE.store(0, Ordering::Release);
    }
    publish(merged(current_signal(), mode));
}
pub fn refresh() {
    if let Some(mode) = crate::window_server::presentation_mode() {
        observe_compositor(mode);
    }
}
fn publish(mode: Mode) {
    if MODE.swap(mode as u8, Ordering::AcqRel) == mode as u8 {
        return;
    }
    if std::env::var_os("AMSPACE_TRACE_DESKTOP").is_some() {
        eprintln!("desktop-mode: {mode:?}");
    }
    // Publish the gate before hopping to AppKit: a reentrant AX call cannot
    // resume placement/focus while the UI's RefCell is temporarily borrowed.
    unsafe {
        dispatch_async_f(
            std::ptr::addr_of!(_dispatch_main_q),
            mode as usize as *mut c_void,
            deliver,
        );
    }
}
extern "C" fn deliver(context: *mut c_void) {
    let mode = match context as usize {
        1 => Mode::Overview,
        2 => Mode::Desktop,
        _ => Mode::Normal,
    };
    objc2::rc::autoreleasepool(|_| crate::ui::desktop_presentation_changed(mode));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn showing_desktop_and_overview_have_distinct_enter_and_exit_signals() {
        assert_eq!(
            distributed_mode("com.apple.showdesktop.start"),
            Some(Mode::Desktop)
        );
        assert_eq!(
            distributed_mode("com.apple.showdesktop.stop"),
            Some(Mode::Normal)
        );
        assert_eq!(
            distributed_mode("com.apple.expose.start"),
            Some(Mode::Overview)
        );
        assert_eq!(
            distributed_mode("com.apple.expose.front.stop"),
            Some(Mode::Normal)
        );
        assert_eq!(distributed_mode("com.apple.unrelated.start"), None);
        let names: std::collections::HashSet<_> = DISTRIBUTED_NOTIFICATIONS
            .iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names.len(), DISTRIBUTED_NOTIFICATIONS.len());
    }
}
