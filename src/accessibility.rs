//! Small, checked CoreFoundation/Accessibility boundary. Create/Copy results own +1.
use objc2_foundation::{NSPoint, NSSize, NSString};
use std::{ffi::c_void, ptr};
pub type Cf = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Cf);
    fn CFRetain(value: Cf) -> Cf;
    fn CFGetTypeID(value: Cf) -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDictionaryGetValue(dict: Cf, key: Cf) -> Cf;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: Cf) -> isize;
    fn CFArrayGetValueAtIndex(array: Cf, index: isize) -> Cf;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(value: Cf, kind: isize, out: *mut i64) -> bool;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Cf) -> bool;
    pub fn CFEqual(a: Cf, b: Cf) -> bool;
    fn CFRunLoopGetMain() -> Cf;
    fn CFRunLoopAddSource(loop_: Cf, source: Cf, mode: Cf);
    fn CFRunLoopRemoveSource(loop_: Cf, source: Cf, mode: Cf);
    static kCFRunLoopCommonModes: Cf;
    static kCFBooleanTrue: Cf;
    static kCFBooleanFalse: Cf;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: Cf) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> Cf;
    fn AXUIElementCreateSystemWide() -> Cf;
    fn AXUIElementCopyElementAtPosition(element: Cf, x: f32, y: f32, result: *mut Cf) -> i32;
    fn AXUIElementCopyAttributeValue(element: Cf, name: Cf, result: *mut Cf) -> i32;
    fn AXUIElementGetPid(element: Cf, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Cf, seconds: f32) -> i32;
    fn AXUIElementSetAttributeValue(element: Cf, name: Cf, value: Cf) -> i32;
    fn AXUIElementPerformAction(element: Cf, action: Cf) -> i32;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetValue(value: Cf, kind: u32, out: *mut c_void) -> bool;
    fn AXObserverCreate(
        pid: i32,
        callback: extern "C" fn(Cf, Cf, Cf, *mut c_void),
        result: *mut Cf,
    ) -> i32;
    fn AXObserverAddNotification(
        observer: Cf,
        element: Cf,
        notification: Cf,
        context: *mut c_void,
    ) -> i32;
    fn AXObserverGetRunLoopSource(observer: Cf) -> Cf;
}
pub struct Owned(pub Cf);
impl Owned {
    pub fn new(p: Cf) -> Option<Self> {
        (!p.is_null()).then_some(Self(p))
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
fn cf(s: &NSString) -> Cf {
    (s as *const NSString).cast()
}
pub fn get(dict: Cf, key: &str) -> Cf {
    if dict.is_null() || unsafe { CFGetTypeID(dict) != CFDictionaryGetTypeID() } {
        return ptr::null();
    }
    unsafe { CFDictionaryGetValue(dict, cf(&NSString::from_str(key))) }
}
pub fn number(value: Cf) -> Option<i64> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut n = 0;
    unsafe { CFNumberGetValue(value, 4, &mut n) }.then_some(n)
}
pub fn decimal(value: Cf) -> Option<f64> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut value_out = 0_f64;
    // kCFNumberDoubleType; CFNumberGetValue's C output is untyped.
    let copied = unsafe { CFNumberGetValue(value, 13, (&mut value_out as *mut f64).cast()) };
    (copied && value_out.is_finite()).then_some(value_out)
}
pub fn array(value: Cf) -> Vec<Cf> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFArrayGetTypeID() } {
        return vec![];
    }
    unsafe {
        (0..CFArrayGetCount(value))
            .map(|i| CFArrayGetValueAtIndex(value, i))
            .collect()
    }
}
pub fn attr(element: Cf, key: &str) -> Option<Owned> {
    if element.is_null() {
        return None;
    }
    let mut result = ptr::null();
    let code = unsafe {
        AXUIElementCopyAttributeValue(element, cf(&NSString::from_str(key)), &mut result)
    };
    if code == 0 { Owned::new(result) } else { None }
}
pub fn equals_text(value: Cf, text: &str) -> bool {
    !value.is_null() && unsafe { CFEqual(value, cf(&NSString::from_str(text))) }
}
pub fn auxiliary_window_role(role: Cf) -> bool {
    ["AXMenu", "AXMenuBar", "AXPopover", "AXToolTip"]
        .iter()
        .any(|name| equals_text(role, name))
}
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}
/// Only invoked by the explicit Accessibility button; never prompts on launch.
pub fn request_permission() {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    unsafe {
        let options: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
        let _: () = msg_send![&options, setObject:kCFBooleanTrue as *const AnyObject, forKey:&*NSString::from_str("AXTrustedCheckOptionPrompt")];
        AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast());
    }
}
pub fn application(pid: i32) -> Option<Owned> {
    let app = Owned::new(unsafe { AXUIElementCreateApplication(pid) })?;
    unsafe {
        AXUIElementSetMessagingTimeout(app.0, 0.08);
    }
    Some(app)
}
pub fn window_id(window: Cf) -> Option<u32> {
    type GetId = unsafe extern "C" fn(Cf, *mut u32) -> i32;
    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr()) };
    if symbol.is_null() {
        return None;
    }
    let get_id = unsafe { std::mem::transmute::<*mut c_void, GetId>(symbol) };
    let mut id = 0;
    (unsafe { get_id(window, &mut id) } == 0 && id != 0).then_some(id)
}
pub fn focused_id(pid: i32) -> Option<u32> {
    window_id(attr(application(pid)?.0, "AXFocusedWindow")?.0)
}
pub fn raise(window: Cf) -> bool {
    unsafe { AXUIElementPerformAction(window, cf(&NSString::from_str("AXRaise"))) == 0 }
}
pub fn window_for_id(pid: i32, id: u32) -> Option<Owned> {
    let windows = attr(application(pid)?.0, "AXWindows")?;
    for window in array(windows.0) {
        if window_id(window) == Some(id) {
            return Owned::new(unsafe { CFRetain(window) });
        }
    }
    None
}
pub fn flag(element: Cf, name: &str) -> bool {
    attr(element, name).is_some_and(|v| unsafe {
        CFGetTypeID(v.0) == CFBooleanGetTypeID() && CFBooleanGetValue(v.0)
    })
}
pub fn geometry(window: Cf) -> Option<(NSPoint, NSSize)> {
    if flag(window, "AXMinimized") || flag(window, "AXFullScreen") {
        return None;
    }
    let p = attr(window, "AXPosition")?;
    let s = attr(window, "AXSize")?;
    let mut point = NSPoint::new(0., 0.);
    let mut size = NSSize::new(0., 0.);
    let valid = unsafe {
        CFGetTypeID(p.0) == AXValueGetTypeID()
            && CFGetTypeID(s.0) == AXValueGetTypeID()
            && AXValueGetValue(p.0, 1, (&mut point as *mut NSPoint).cast())
            && AXValueGetValue(s.0, 2, (&mut size as *mut NSSize).cast())
    };
    (valid
        && point.x.is_finite()
        && point.y.is_finite()
        && size.width.is_finite()
        && size.height.is_finite()
        && size.width > 40.
        && size.height > 40.)
        .then_some((point, size))
}
pub fn window_at(x: f64, y: f64) -> Option<(Owned, i32)> {
    // AX knows which overlays actually receive input (the Dock also publishes
    // a transparent full-screen window). Use it to reject menus/Mission Control,
    // then use compositor geometry to distinguish sibling application windows.
    let root = Owned::new(unsafe { AXUIElementCreateSystemWide() })?;
    unsafe {
        AXUIElementSetMessagingTimeout(root.0, 0.08);
    }
    let mut hit = ptr::null();
    if unsafe { AXUIElementCopyElementAtPosition(root.0, x as f32, y as f32, &mut hit) } != 0 {
        return None;
    }
    let hit = Owned::new(hit)?;
    let hit_window = attr(hit.0, "AXWindow").or_else(|| {
        let role = attr(hit.0, "AXRole")?;
        unsafe { CFEqual(role.0, cf(&NSString::from_str("AXWindow"))) }.then_some(hit)
    })?;
    let role = attr(hit_window.0, "AXSubrole")?;
    if !unsafe { CFEqual(role.0, cf(&NSString::from_str("AXStandardWindow"))) } {
        return None;
    }
    let mut hit_pid = 0;
    if unsafe { AXUIElementGetPid(hit_window.0, &mut hit_pid) } != 0 {
        return None;
    }
    let hit_id = window_id(hit_window.0);
    let hits = crate::window_server::hit_windows_at(NSPoint::new(x, y));
    #[cfg(test)]
    eprintln!("AX-pointer pid={hit_pid} window={hit_id:?} native-hits={hits:?}");
    let (owner, id) = crate::focus::pointer_target(&hits, hit_pid, hit_id)?;
    if hit_pid == owner && hit_id == Some(id) {
        return Some((hit_window, hit_pid));
    }
    let window = window_for_id(owner, id)?;
    let role = attr(window.0, "AXSubrole")?;
    if !unsafe { CFEqual(role.0, cf(&NSString::from_str("AXStandardWindow"))) } {
        return None;
    }
    let mut pid = 0;
    if unsafe { AXUIElementGetPid(window.0, &mut pid) } != 0 || pid == std::process::id() as i32 {
        return None;
    }
    Some((window, pid))
}
pub fn focus(window: Cf, pid: i32) -> bool {
    let Some(app) = application(pid) else {
        return false;
    };
    unsafe {
        // Raising a background window alone does not make its app active.
        AXUIElementSetAttributeValue(
            app.0,
            cf(&NSString::from_str("AXFrontmost")),
            kCFBooleanTrue,
        );
        AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXMain")), kCFBooleanTrue);
        AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXFocused")), kCFBooleanTrue);
        AXUIElementPerformAction(window, cf(&NSString::from_str("AXRaise"))) == 0
    }
}
pub struct Observer {
    handle: Owned,
}
impl Observer {
    pub fn new(pid: i32, window: Option<Cf>, geometry_fallback: bool) -> Option<Self> {
        let app = application(pid)?;
        let mut observer = ptr::null();
        if unsafe { AXObserverCreate(pid, changed, &mut observer) } != 0 {
            return None;
        }
        let this = Self {
            handle: Owned::new(observer)?,
        };
        for name in [
            "AXFocusedWindowChanged",
            "AXMainWindowChanged",
            "AXWindowCreated",
            "AXApplicationHidden",
            "AXApplicationShown",
        ] {
            this.subscribe(app.0, name);
        }
        if let Some(w) = window {
            if geometry_fallback {
                this.subscribe(w, "AXMoved");
                this.subscribe(w, "AXResized");
            }
            for name in [
                "AXUIElementDestroyed",
                "AXWindowMiniaturized",
                "AXWindowDeminiaturized",
            ] {
                this.subscribe(w, name);
            }
        }
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                AXObserverGetRunLoopSource(observer),
                kCFRunLoopCommonModes,
            );
        }
        Some(this)
    }
    fn subscribe(&self, element: Cf, name: &str) {
        unsafe {
            AXObserverAddNotification(
                self.handle.0,
                element,
                cf(&NSString::from_str(name)),
                ptr::null_mut(),
            );
        }
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopRemoveSource(
                CFRunLoopGetMain(),
                AXObserverGetRunLoopSource(self.handle.0),
                kCFRunLoopCommonModes,
            );
        }
    }
}
extern "C" fn changed(_: Cf, _: Cf, notification: Cf, _: *mut c_void) {
    crate::ui::queue_window_update(
        equals_text(notification, "AXFocusedWindowChanged")
            || equals_text(notification, "AXMainWindowChanged"),
    );
}

/// Only standard, movable/resizable windows participate. Dialogs stay floating.
pub fn tileable(window: Cf) -> bool {
    unsafe extern "C" {
        fn AXUIElementIsAttributeSettable(element: Cf, name: Cf, result: *mut bool) -> i32;
    }
    if !attr(window, "AXSubrole").is_some_and(|v| equals_text(v.0, "AXStandardWindow"))
        || geometry(window).is_none()
    {
        return false;
    }
    ["AXPosition", "AXSize"].iter().all(|name| {
        let mut settable = false;
        unsafe {
            AXUIElementIsAttributeSettable(window, cf(&NSString::from_str(name)), &mut settable)
                == 0
                && settable
        }
    })
}
/// Temporarily suppress AppKit's own AX geometry animation, restoring the exact
/// app attribute when this transaction ends (Rectangle PR #285).
pub struct GeometrySession(Option<Owned>);
impl GeometrySession {
    pub fn shared(window: Cf) -> Option<std::rc::Rc<Self>> {
        thread_local! {static SESSIONS: std::cell::RefCell<std::collections::HashMap<i32,std::rc::Weak<GeometrySession>>> = std::cell::RefCell::new(std::collections::HashMap::new());}
        let mut pid = 0;
        if unsafe { AXUIElementGetPid(window, &mut pid) } != 0 {
            return None;
        }
        SESSIONS.with(|sessions| {
            let mut sessions = sessions.borrow_mut();
            if let Some(session) = sessions.get(&pid).and_then(std::rc::Weak::upgrade) {
                return Some(session);
            }
            sessions.retain(|_, session| session.strong_count() > 0);
            let session = std::rc::Rc::new(Self::new(window));
            sessions.insert(pid, std::rc::Rc::downgrade(&session));
            Some(session)
        })
    }
    pub fn new(window: Cf) -> Self {
        let mut pid = 0;
        if unsafe { AXUIElementGetPid(window, &mut pid) } != 0 {
            return Self(None);
        }
        let Some(app) = application(pid) else {
            return Self(None);
        };
        if flag(app.0, "AXEnhancedUserInterface")
            && unsafe {
                AXUIElementSetAttributeValue(
                    app.0,
                    cf(&NSString::from_str("AXEnhancedUserInterface")),
                    kCFBooleanFalse,
                )
            } == 0
        {
            Self(Some(app))
        } else {
            Self(None)
        }
    }
}
impl Drop for GeometrySession {
    fn drop(&mut self) {
        if let Some(app) = &self.0 {
            unsafe {
                AXUIElementSetAttributeValue(
                    app.0,
                    cf(&NSString::from_str("AXEnhancedUserInterface")),
                    kCFBooleanTrue,
                );
            }
        }
    }
}
/// Keep shared edges aligned and never pass fractional AX geometry to apps.
pub(crate) fn integral_frame(frame: objc2_foundation::NSRect) -> objc2_foundation::NSRect {
    let x = frame.origin.x.round();
    let y = frame.origin.y.round();
    crate::native::rect(
        x,
        y,
        (frame.origin.x + frame.size.width).round() - x,
        (frame.origin.y + frame.size.height).round() - y,
    )
}
pub fn set_frame(window: Cf, frame: objc2_foundation::NSRect) -> bool {
    unsafe extern "C" {
        fn AXValueCreate(kind: u32, value: *const c_void) -> Cf;
    }
    if !frame.origin.x.is_finite()
        || !frame.origin.y.is_finite()
        || !frame.size.width.is_finite()
        || !frame.size.height.is_finite()
        || frame.size.width < 80.
        || frame.size.height < 60.
    {
        return false;
    }
    let _session = GeometrySession::shared(window);
    let frame = integral_frame(frame);
    let Some(p) = Owned::new(unsafe { AXValueCreate(1, (&frame.origin as *const NSPoint).cast()) })
    else {
        return false;
    };
    let Some(s) = Owned::new(unsafe { AXValueCreate(2, (&frame.size as *const NSSize).cast()) })
    else {
        return false;
    };
    unsafe {
        // Size first permits movement from a larger display to a smaller one.
        apply_frame(
            || AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXSize")), s.0) == 0,
            || {
                AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXPosition")), p.0)
                    == 0
            },
        )
    }
}

/// Intermediate animation frames only write attributes that changed. The final
/// frame uses set_frame's full transaction and is verified separately.
pub fn set_frame_step(
    window: Cf,
    previous: objc2_foundation::NSRect,
    next: objc2_foundation::NSRect,
) -> bool {
    unsafe extern "C" {
        fn AXValueCreate(kind: u32, value: *const c_void) -> Cf;
    }
    let previous = integral_frame(previous);
    let next = integral_frame(next);
    let (size, position) = frame_changes(previous, next);
    let mut ok = true;
    if size {
        let Some(value) =
            Owned::new(unsafe { AXValueCreate(2, (&next.size as *const NSSize).cast()) })
        else {
            return false;
        };
        ok &= unsafe {
            AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXSize")), value.0) == 0
        };
    }
    if position {
        let Some(value) =
            Owned::new(unsafe { AXValueCreate(1, (&next.origin as *const NSPoint).cast()) })
        else {
            return false;
        };
        ok &= unsafe {
            AXUIElementSetAttributeValue(window, cf(&NSString::from_str("AXPosition")), value.0)
                == 0
        };
    }
    ok
}
fn frame_changes(a: objc2_foundation::NSRect, b: objc2_foundation::NSRect) -> (bool, bool) {
    (
        (a.size.width - b.size.width).abs() >= 0.5 || (a.size.height - b.size.height).abs() >= 0.5,
        (a.origin.x - b.origin.x).abs() >= 0.5 || (a.origin.y - b.origin.y).abs() >= 0.5,
    )
}

// An app may clamp the first expansion against the old screen edge. Apply the
// size again at the final position so an expanded sibling fills its entire tile.
// Resizing can itself shift the origin (screen constraints/anchored edges), so
// commit the position once more after the final size.
fn apply_frame(mut size: impl FnMut() -> bool, mut position: impl FnMut() -> bool) -> bool {
    size();
    position();
    let sized = size();
    let moved = position();
    moved && sized
}
#[cfg(test)]
mod placement_tests {
    use super::*;
    #[test]
    fn unknown_or_missing_accessibility_role_does_not_hide_a_real_window() {
        for role in ["AXWindow", "AXUnknown", "AXDialog"] {
            assert!(!auxiliary_window_role(cf(&NSString::from_str(role))));
        }
        assert!(!auxiliary_window_role(ptr::null()));
        for role in ["AXMenu", "AXPopover", "AXToolTip"] {
            assert!(auxiliary_window_role(cf(&NSString::from_str(role))));
        }
    }
    #[test]
    fn fractional_window_opacity_is_not_truncated_to_transparent() {
        objc2::rc::autoreleasepool(|_| unsafe {
            use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
            for alpha in [0., 0.25, 0.95, 1.] {
                let number: Retained<AnyObject> =
                    msg_send![class!(NSNumber), numberWithDouble:alpha];
                assert_eq!(decimal(Retained::as_ptr(&number).cast()), Some(alpha));
            }
            assert_eq!(decimal(ptr::null()), None);
        });
    }
    #[test]
    fn final_resize_cannot_displace_the_requested_position() {
        use std::cell::Cell;
        let x = Cell::new(700.);
        let writes = Cell::new(0);
        assert!(apply_frame(
            || {
                writes.set(writes.get() + 1);
                // An app anchors its right edge when its final size changes.
                x.set(x.get() - 100.);
                true
            },
            || {
                x.set(-890.);
                true
            },
        ));
        assert_eq!(writes.get(), 2);
        assert_eq!(x.get(), -890.);
    }

    #[test]
    fn placement_reports_final_writes_even_when_initial_writes_fail() {
        use std::cell::Cell;
        let sizes = Cell::new(0);
        let positions = Cell::new(0);
        assert!(apply_frame(
            || {
                sizes.set(sizes.get() + 1);
                sizes.get() > 1
            },
            || {
                positions.set(positions.get() + 1);
                positions.get() > 1
            },
        ));
        assert!(!apply_frame(|| true, || false));
        assert!(!apply_frame(|| false, || true));
    }

    #[test]
    fn fractional_tiles_have_integer_final_geometry_and_shared_edges() {
        let a = integral_frame(crate::native::rect(-900., 41., 1660. / 3., 947.));
        let b = integral_frame(crate::native::rect(
            -900. + 1660. / 3. + 10.,
            41.,
            1660. / 3.,
            947.,
        ));
        assert_eq!(a.origin.x + a.size.width + 10., b.origin.x);
        for r in [a, b] {
            for value in [r.origin.x, r.origin.y, r.size.width, r.size.height] {
                assert_eq!(value, value.round());
            }
        }
    }

    #[test]
    fn animation_writes_only_changed_attributes() {
        let a = crate::native::rect(0., 0., 600., 400.);
        assert_eq!(frame_changes(a, a), (false, false));
        assert_eq!(
            frame_changes(a, crate::native::rect(20., 0., 600., 400.)),
            (false, true)
        );
        assert_eq!(
            frame_changes(a, crate::native::rect(0., 0., 700., 400.)),
            (true, false)
        );
        assert_eq!(
            frame_changes(a, crate::native::rect(0.1, 0.1, 600.1, 400.1)),
            (false, false)
        );
    }
    #[test]
    fn expansion_is_retried_after_moving_away_from_screen_edge() {
        use std::cell::Cell;
        let x = Cell::new(700.0_f64);
        let width = Cell::new(500.0_f64);
        assert!(apply_frame(
            || {
                width.set(1200.0_f64.min(1200. - x.get()));
                true
            },
            || {
                x.set(0.);
                true
            }
        ));
        assert_eq!(x.get() + width.get(), 1200.);
        assert_eq!(width.get(), 1200.);
    }
}
