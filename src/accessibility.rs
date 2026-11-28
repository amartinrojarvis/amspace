//! Small, checked CoreFoundation/Accessibility boundary. Create/Copy results own +1.
use objc2_foundation::{NSPoint, NSSize, NSString, ns_string};
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
    fn AXUIElementCopyMultipleAttributeValues(
        element: Cf,
        attributes: Cf,
        options: u32,
        result: *mut Cf,
    ) -> i32;
    fn AXUIElementGetPid(element: Cf, pid: *mut i32) -> i32;
    fn AXUIElementSetMessagingTimeout(element: Cf, seconds: f32) -> i32;
    fn AXUIElementSetAttributeValue(element: Cf, name: Cf, value: Cf) -> i32;
    fn AXUIElementPerformAction(element: Cf, action: Cf) -> i32;
    fn AXUIElementGetTypeID() -> usize;
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
impl Clone for Owned {
    fn clone(&self) -> Self {
        Self(unsafe { CFRetain(self.0) })
    }
}
impl Owned {
    pub fn new(p: Cf) -> Option<Self> {
        if p.is_null() { None } else { Some(Self(p)) }
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
pub fn get(dict: Cf, key: &NSString) -> Cf {
    if dict.is_null() || unsafe { CFGetTypeID(dict) != CFDictionaryGetTypeID() } {
        return ptr::null();
    }
    unsafe { CFDictionaryGetValue(dict, cf(key)) }
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
/// Retain the source while traversing it, without allocating a temporary Vec.
#[derive(Clone)]
pub struct Array {
    owner: Option<Owned>,
    indices: std::ops::Range<isize>,
}
impl Iterator for Array {
    type Item = Cf;
    fn next(&mut self) -> Option<Cf> {
        let index = self.indices.next()?;
        Some(unsafe { CFArrayGetValueAtIndex(self.owner.as_ref()?.0, index) })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.indices.len();
        (len, Some(len))
    }
}
impl ExactSizeIterator for Array {}
pub fn array(value: Cf) -> Array {
    let valid = !value.is_null() && unsafe { CFGetTypeID(value) == CFArrayGetTypeID() };
    Array {
        owner: valid.then(|| Owned(unsafe { CFRetain(value) })),
        indices: 0..if valid {
            unsafe { CFArrayGetCount(value) }
        } else {
            0
        },
    }
}
pub fn attr(element: Cf, key: &str) -> Option<Owned> {
    if element.is_null() {
        return None;
    }
    let dynamic_key;
    let name = if let Some(name) = common_attribute_key(key) {
        name
    } else {
        dynamic_key = NSString::from_str(key);
        &dynamic_key
    };
    let mut result = ptr::null();
    let code = unsafe { AXUIElementCopyAttributeValue(element, cf(name), &mut result) };
    if code == 0 { Owned::new(result) } else { None }
}
fn common_attribute_key(key: &str) -> Option<&'static NSString> {
    Some(match key {
        "AXChildren" => ns_string!("AXChildren"),
        "AXEnhancedUserInterface" => ns_string!("AXEnhancedUserInterface"),
        "AXFocusedUIElement" => ns_string!("AXFocusedUIElement"),
        "AXFocusedWindow" => ns_string!("AXFocusedWindow"),
        "AXFullScreen" => ns_string!("AXFullScreen"),
        "AXIdentifier" => ns_string!("AXIdentifier"),
        "AXMinimized" => ns_string!("AXMinimized"),
        "AXModal" => ns_string!("AXModal"),
        "AXParent" => ns_string!("AXParent"),
        "AXPosition" => ns_string!("AXPosition"),
        "AXRole" => ns_string!("AXRole"),
        "AXSheets" => ns_string!("AXSheets"),
        "AXSize" => ns_string!("AXSize"),
        "AXSubrole" => ns_string!("AXSubrole"),
        "AXWindow" => ns_string!("AXWindow"),
        "AXWindows" => ns_string!("AXWindows"),
        _ => return None,
    })
}
pub fn equals_text(value: Cf, text: &str) -> bool {
    !value.is_null() && unsafe { CFEqual(value, cf(&NSString::from_str(text))) }
}
pub fn auxiliary_window_role(role: Cf) -> bool {
    !role.is_null()
        && [
            ns_string!("AXMenu"),
            ns_string!("AXMenuBar"),
            ns_string!("AXPopover"),
            ns_string!("AXToolTip"),
        ]
        .into_iter()
        .any(|name| unsafe { CFEqual(role, cf(name)) })
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
    static GET_ID: std::sync::OnceLock<Option<GetId>> = std::sync::OnceLock::new();
    let get_id = GET_ID
        .get_or_init(|| {
            let symbol =
                unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr()) };
            (!symbol.is_null())
                .then(|| unsafe { std::mem::transmute::<*mut c_void, GetId>(symbol) })
        })
        .as_ref()?;
    let mut id = 0;
    (unsafe { get_id(window, &mut id) } == 0 && id != 0).then_some(id)
}
pub fn focused_id(pid: i32) -> Option<u32> {
    window_id(attr(application(pid)?.0, "AXFocusedWindow")?.0)
}
fn protected_focus_role(role: Cf) -> bool {
    !role.is_null()
        && [
            ns_string!("AXDialog"),
            ns_string!("AXSystemDialog"),
            ns_string!("AXSheet"),
            ns_string!("AXSecureTextField"),
            ns_string!("AXMenu"),
            ns_string!("AXMenuItem"),
            ns_string!("AXMenuBarItem"),
        ]
        .into_iter()
        .any(|name| unsafe { CFEqual(role, cf(name)) })
}
fn pointer_focus_role_excluded(role: Cf) -> bool {
    auxiliary_window_role(role) || protected_focus_role(role) || equals_text(role, "AXDesktop")
}

fn accepts_pointer_focus(window: Cf) -> bool {
    ["AXRole", "AXSubrole"]
        .iter()
        .all(|name| attr(window, name).is_none_or(|role| !pointer_focus_role_excluded(role.0)))
}

/// Modal dialogs and sheets keep keyboard ownership even when the pointer rests
/// on another app. Secure input is checked separately and needs no AX access.
pub fn focus_is_protected(pid: i32) -> bool {
    protected_focus(pid, false)
}

pub(crate) fn pointer_focus_is_protected(pid: i32) -> bool {
    protected_focus(pid, true)
}

fn protected_focus(pid: i32, preserve_fullscreen: bool) -> bool {
    let Some(app) = application(pid) else {
        return false;
    };
    let protected = |element| {
        ["AXRole", "AXSubrole"]
            .iter()
            .any(|key| attr(element, key).is_some_and(|role| protected_focus_role(role.0)))
    };
    if attr(app.0, "AXFocusedUIElement").is_some_and(|element| protected(element.0)) {
        return true;
    }
    attr(app.0, "AXFocusedWindow").is_some_and(|window| {
        flag(window.0, "AXModal")
            || (preserve_fullscreen && flag(window.0, "AXFullScreen"))
            || protected(window.0)
            || attr(window.0, "AXSheets").is_some_and(|sheets| array(sheets.0).next().is_some())
    })
}
pub fn raise(window: Cf) -> bool {
    unsafe { AXUIElementPerformAction(window, cf(ns_string!("AXRaise"))) == 0 }
}
/// Request the window's normal close action. Success is not confirmation:
/// the application may display a save dialog or cancel the close.
pub fn request_close(window: Cf) -> bool {
    if window.is_null() || unsafe { CFGetTypeID(window) != AXUIElementGetTypeID() } {
        return false;
    }
    let Some(button) = attr(window, "AXCloseButton") else {
        return false;
    };
    if unsafe { CFGetTypeID(button.0) != AXUIElementGetTypeID() } || !flag(button.0, "AXEnabled") {
        return false;
    }
    unsafe { AXUIElementPerformAction(button.0, cf(ns_string!("AXPress"))) == 0 }
}

/// Only the native invalid-element error confirms destruction. A timeout,
/// missing attribute or unavailable permission is not evidence of a close.
pub fn destroyed(window: Cf) -> bool {
    if window.is_null() || unsafe { CFGetTypeID(window) != AXUIElementGetTypeID() } {
        return false;
    }
    let mut value = ptr::null();
    let status =
        unsafe { AXUIElementCopyAttributeValue(window, cf(ns_string!("AXRole")), &mut value) };
    let _value = Owned::new(value);
    status == -25202 // kAXErrorInvalidUIElement
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
/// One per-process snapshot for a reconciliation pass. A failed read is not an
/// empty application; callers cache that failure only until the next pass.
pub fn application_windows(pid: i32) -> Option<std::collections::HashMap<u32, Owned>> {
    let windows = attr(application(pid)?.0, "AXWindows")?;
    if unsafe { CFGetTypeID(windows.0) != CFArrayGetTypeID() } {
        return None;
    }
    Some(
        array(windows.0)
            .filter_map(|window| {
                Some((window_id(window)?, Owned::new(unsafe { CFRetain(window) })?))
            })
            .collect(),
    )
}
/// The AXTabGroup element identifies the native tabbed window across selected
/// NSWindow IDs. Retain that identity; tab buttons and titles are not stable.
pub fn native_tab_group(window: Cf) -> Option<Owned> {
    let children = attr(window, "AXChildren")?;
    array(children.0).find_map(|child| {
        attr(child, "AXRole")
            .is_some_and(|role| equals_text(role.0, "AXTabGroup"))
            .then(|| Owned::new(unsafe { CFRetain(child) }))
            .flatten()
    })
}
pub fn flag(element: Cf, name: &str) -> bool {
    attr(element, name)
        .and_then(|v| boolean(v.0))
        .unwrap_or(false)
}

pub(crate) fn restore_minimized_window(window: Cf) -> bool {
    !window.is_null()
        && attr(window, "AXSubrole").is_some_and(|v| equals_text(v.0, "AXStandardWindow"))
        && !flag(window, "AXModal")
        && !flag(window, "AXFullScreen")
        && !system_utility_window(window)
        && flag(window, "AXMinimized")
        && unsafe {
            AXUIElementSetAttributeValue(window, cf(ns_string!("AXMinimized")), kCFBooleanFalse)
                == 0
        }
}
pub fn boolean(value: Cf) -> Option<bool> {
    (!value.is_null() && unsafe { CFGetTypeID(value) == CFBooleanGetTypeID() })
        .then(|| unsafe { CFBooleanGetValue(value) })
}
thread_local! {
    static GEOMETRY_KEYS: objc2::rc::Retained<objc2::runtime::AnyObject> = {
        let keys = [
            cf(ns_string!("AXMinimized")),
            cf(ns_string!("AXFullScreen")),
            cf(ns_string!("AXPosition")),
            cf(ns_string!("AXSize")),
        ];
        let objects = keys.as_ptr().cast::<*const objc2::runtime::AnyObject>();
        unsafe { objc2::msg_send![objc2::class!(NSArray), arrayWithObjects:objects, count:keys.len()] }
    };
}

/// Read a fresh snapshot; only the attribute names survive between calls.
pub fn geometry(window: Cf) -> Option<(NSPoint, NSSize)> {
    if window.is_null() {
        return None;
    }
    let mut values = ptr::null();
    let status = GEOMETRY_KEYS.with(|keys| unsafe {
        AXUIElementCopyMultipleAttributeValues(
            window,
            objc2::rc::Retained::as_ptr(keys).cast(),
            0,
            &mut values,
        )
    });
    if status == 0
        && let Some(values) = Owned::new(values)
    {
        let mut entries = array(values.0);
        if entries.len() == 4 {
            let minimized = boolean(entries.next()?);
            let fullscreen = boolean(entries.next()?);
            if minimized == Some(true) || fullscreen == Some(true) {
                return None;
            }
            // Some providers expose these optional flags only individually.
            if (minimized.is_none() && flag(window, "AXMinimized"))
                || (fullscreen.is_none() && flag(window, "AXFullScreen"))
            {
                return None;
            }
            if let Some(frame) = decode_geometry(entries.next()?, entries.next()?) {
                return Some(frame);
            }
        }
    }
    // Preserve individual reads for providers that cannot return a full batch.
    geometry_individual(window)
}

fn geometry_individual(window: Cf) -> Option<(NSPoint, NSSize)> {
    if flag(window, "AXMinimized") || flag(window, "AXFullScreen") {
        return None;
    }
    let p = attr(window, "AXPosition")?;
    let s = attr(window, "AXSize")?;
    decode_geometry(p.0, s.0)
}

fn decode_geometry(p: Cf, s: Cf) -> Option<(NSPoint, NSSize)> {
    let mut point = NSPoint::new(0., 0.);
    let mut size = NSSize::new(0., 0.);
    let valid = unsafe {
        !p.is_null()
            && !s.is_null()
            && CFGetTypeID(p) == AXValueGetTypeID()
            && CFGetTypeID(s) == AXValueGetTypeID()
            && AXValueGetValue(p, 1, (&mut point as *mut NSPoint).cast())
            && AXValueGetValue(s, 2, (&mut size as *mut NSSize).cast())
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
pub fn window_at(point: NSPoint, front_pid: i32) -> Option<(Owned, i32)> {
    let hits = crate::window_server::hit_windows_at(point, Some(front_pid));
    if hits.is_empty() {
        return None;
    }
    let NSPoint { x, y } = point;
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
    // Do not follow AXWindow from the desktop surface to a Finder window.
    if attr(hit.0, "AXRole").is_some_and(|role| equals_text(role.0, "AXDesktop")) {
        return None;
    }
    let hit_window = attr(hit.0, "AXWindow").or_else(|| {
        let role = attr(hit.0, "AXRole")?;
        unsafe { CFEqual(role.0, cf(ns_string!("AXWindow"))) }.then_some(hit)
    })?;
    // A compositor-corroborated real window may omit its subrole or report
    // AXUnknown (including browser windows). Reject known auxiliary/modal
    // surfaces, not every value other than AXStandardWindow.
    if !accepts_pointer_focus(hit_window.0) || flag(hit_window.0, "AXFullScreen") {
        return None;
    }
    let mut hit_pid = 0;
    if unsafe { AXUIElementGetPid(hit_window.0, &mut hit_pid) } != 0
        || hit_pid == std::process::id() as i32
    {
        return None;
    }
    let hit_id = window_id(hit_window.0);
    #[cfg(test)]
    eprintln!("AX-pointer pid={hit_pid} window={hit_id:?} native-hits={hits:?}");
    let (owner, id) = crate::focus::pointer_target(&hits, hit_pid, hit_id)?;
    if hit_pid == owner && hit_id == Some(id) {
        return Some((hit_window, hit_pid));
    }
    let window = window_for_id(owner, id)?;
    if !accepts_pointer_focus(window.0) || flag(window.0, "AXFullScreen") {
        return None;
    }
    let mut pid = 0;
    if unsafe { AXUIElementGetPid(window.0, &mut pid) } != 0 || pid == std::process::id() as i32 {
        return None;
    }
    Some((window, pid))
}
pub fn focus(window: Cf, pid: i32) -> bool {
    if crate::desktop_mode::active() {
        return false;
    }
    let Some(app) = application(pid) else {
        return false;
    };
    // Raising a background window alone does not make its app active.
    unsafe {
        AXUIElementSetAttributeValue(app.0, cf(ns_string!("AXFrontmost")), kCFBooleanTrue);
    }
    focus_only_window(window)
}

pub(crate) fn focus_only_window(window: Cf) -> bool {
    unsafe {
        AXUIElementSetAttributeValue(window, cf(ns_string!("AXMain")), kCFBooleanTrue);
        AXUIElementSetAttributeValue(window, cf(ns_string!("AXFocused")), kCFBooleanTrue);
        AXUIElementPerformAction(window, cf(ns_string!("AXRaise"))) == 0
    }
}
pub struct Observer {
    handle: Owned,
    pub geometry_fallback: bool,
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
            geometry_fallback,
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
    crate::window_server::queue_accessibility_event(
        equals_text(notification, "AXFocusedWindowChanged")
            || equals_text(notification, "AXMainWindowChanged"),
    );
}

/// Dock publishes presentation transitions independently of the frontmost app.
/// Notification names are documented by yabai's mission_control.c (MIT).
pub struct DesktopObserver {
    handle: Owned,
    pub pid: i32,
}
impl DesktopObserver {
    pub fn dock_pid() -> Option<i32> {
        use objc2::{class, msg_send};
        unsafe {
            let apps: crate::native::Obj = msg_send![class!(NSRunningApplication), runningApplicationsWithBundleIdentifier:ns_string!("com.apple.dock")];
            let app: crate::native::Obj = msg_send![apps, firstObject];
            if app.is_null() {
                None
            } else {
                Some(msg_send![app, processIdentifier])
            }
        }
    }
    pub fn new(pid: i32) -> Option<Self> {
        let app = application(pid)?;
        let mut raw = ptr::null();
        if unsafe { AXObserverCreate(pid, desktop_changed, &mut raw) } != 0 {
            return None;
        }
        let this = Self {
            handle: Owned::new(raw)?,
            pid,
        };
        for (name, _) in crate::desktop_mode::NOTIFICATIONS {
            if unsafe {
                AXObserverAddNotification(
                    this.handle.0,
                    app.0,
                    cf(&NSString::from_str(name)),
                    ptr::null_mut(),
                )
            } != 0
            {
                return None;
            }
        }
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                AXObserverGetRunLoopSource(this.handle.0),
                kCFRunLoopCommonModes,
            );
        }
        Some(this)
    }
}
impl Drop for DesktopObserver {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopRemoveSource(
                CFRunLoopGetMain(),
                AXObserverGetRunLoopSource(self.handle.0),
                kCFRunLoopCommonModes,
            );
        }
        crate::desktop_mode::update(crate::desktop_mode::Mode::Normal);
    }
}
extern "C" fn desktop_changed(_: Cf, _: Cf, notification: Cf, _: *mut c_void) {
    if let Some((_, mode)) = crate::desktop_mode::NOTIFICATIONS
        .iter()
        .find(|(name, _)| equals_text(notification, name))
    {
        crate::desktop_mode::update(*mode);
    }
}

/// Only standard, movable/resizable windows participate. Dialogs stay floating.
pub fn tileable(window: Cf) -> bool {
    unsafe extern "C" {
        fn AXUIElementIsAttributeSettable(element: Cf, name: Cf, result: *mut bool) -> i32;
    }
    if !attr(window, "AXSubrole").is_some_and(|v| equals_text(v.0, "AXStandardWindow"))
        || flag(window, "AXModal")
        || system_utility_window(window)
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
fn finder_utility_identifier(identifier: Cf) -> bool {
    // Finder exposes its copy panel as a resizable AXStandardWindow.
    unsafe {
        CFEqual(identifier, cf(ns_string!("Info")))
            || CFEqual(identifier, cf(ns_string!("Inspector")))
            || CFEqual(identifier, cf(ns_string!("Progress")))
    }
}
fn excluded_system_window(
    bundle: Option<&str>,
    identifier: impl FnOnce() -> Option<Owned>,
) -> bool {
    always_floating_bundle(bundle)
        || (bundle == Some("com.apple.finder")
            && identifier().is_some_and(|value| finder_utility_identifier(value.0)))
}
fn always_floating_bundle(bundle: Option<&str>) -> bool {
    // System-owned progress and panel services keep their own geometry: the
    // mosaic must never claim their slots.
    [
        "com.apple.archiveutility",
        "com.apple.CoreServicesUIAgent",
        "com.apple.appkit.xpc.openAndSavePanelService",
        "com.apple.SafariViewService",
    ]
    .iter()
    .any(|name| bundle.is_some_and(|id| id == *name))
}
fn system_utility_window(window: Cf) -> bool {
    let mut pid = 0;
    if unsafe { AXUIElementGetPid(window, &mut pid) } != 0 {
        return true;
    }
    let bundle = crate::tile_rules::application_id(pid);
    excluded_system_window(bundle.as_deref(), || attr(window, "AXIdentifier"))
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
                    cf(ns_string!("AXEnhancedUserInterface")),
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
                    cf(ns_string!("AXEnhancedUserInterface")),
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
    crate::desktop_mode::refresh();
    if crate::desktop_mode::active() {
        return false;
    }
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
            || {
                !crate::desktop_mode::active()
                    && AXUIElementSetAttributeValue(window, cf(ns_string!("AXSize")), s.0) == 0
            },
            || {
                !crate::desktop_mode::active()
                    && AXUIElementSetAttributeValue(window, cf(ns_string!("AXPosition")), p.0) == 0
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
    let next = integral_frame(next);
    let changed = frame_changes(integral_frame(previous), next);
    if changed == (false, false) {
        // The first sample and rounded easing endpoints often coincide. No
        // native write means no need to enumerate WindowServer for permission.
        return true;
    }
    crate::desktop_mode::refresh();
    if crate::desktop_mode::active() {
        return false;
    }
    unsafe extern "C" {
        fn AXValueCreate(kind: u32, value: *const c_void) -> Cf;
    }
    apply_changed_frame(
        changed,
        || !crate::desktop_mode::active(),
        || {
            let Some(value) =
                Owned::new(unsafe { AXValueCreate(2, (&next.size as *const NSSize).cast()) })
            else {
                return false;
            };
            unsafe { AXUIElementSetAttributeValue(window, cf(ns_string!("AXSize")), value.0) == 0 }
        },
        || {
            let Some(value) =
                Owned::new(unsafe { AXValueCreate(1, (&next.origin as *const NSPoint).cast()) })
            else {
                return false;
            };
            unsafe {
                AXUIElementSetAttributeValue(window, cf(ns_string!("AXPosition")), value.0) == 0
            }
        },
    )
}
fn frame_changes(a: objc2_foundation::NSRect, b: objc2_foundation::NSRect) -> (bool, bool) {
    (
        (a.size.width - b.size.width).abs() >= 0.5 || (a.size.height - b.size.height).abs() >= 0.5,
        (a.origin.x - b.origin.x).abs() >= 0.5 || (a.origin.y - b.origin.y).abs() >= 0.5,
    )
}

fn apply_changed_frame(
    changed: (bool, bool),
    mut allowed: impl FnMut() -> bool,
    size: impl FnOnce() -> bool,
    position: impl FnOnce() -> bool,
) -> bool {
    let mut ok = true;
    if changed.0 {
        if !allowed() {
            return false;
        }
        ok &= size();
    }
    if changed.1 {
        // An AX size write can run a nested loop and enter Mission Control.
        // Authorization for that write does not authorize the next position.
        if !allowed() {
            return false;
        }
        ok &= position();
    }
    ok
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
    fn null_create_copy_results_do_not_construct_an_owner() {
        assert!(Owned::new(ptr::null()).is_none());
    }
    #[test]
    fn close_rejects_missing_and_non_ax_objects() {
        assert!(!request_close(ptr::null()));
        assert!(!request_close(cf(ns_string!("not an AX window"))));
    }
    #[test]
    #[ignore = "CPU-only AX key preparation benchmark; run with --release --ignored --nocapture"]
    fn benchmark_attribute_keys() {
        let keys = [
            "AXPosition",
            "AXSize",
            "AXRole",
            "AXSubrole",
            "AXFullScreen",
            "AXMinimized",
            "AXEnhancedUserInterface",
        ];
        let iterations = 50_000;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            for key in keys {
                let value = NSString::from_str(std::hint::black_box(key));
                assert!(unsafe { CFEqual(cf(&value), cf(common_attribute_key(key).unwrap())) });
            }
        }
        let old = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            for key in keys {
                let value = common_attribute_key(std::hint::black_box(key)).unwrap();
                assert!(unsafe { CFEqual(cf(value), cf(common_attribute_key(key).unwrap())) });
            }
        }
        let new = start.elapsed();
        let count = iterations * keys.len();
        println!(
            "attribute_keys samples={count} before_ns={:.1} after_ns={:.1} speedup={:.2}",
            old.as_nanos() as f64 / count as f64,
            new.as_nanos() as f64 / count as f64,
            old.as_secs_f64() / new.as_secs_f64()
        );
    }
    #[test]
    fn static_attribute_keys_preserve_names_and_unknown_attributes_use_the_fallback() {
        for key in [
            "AXChildren",
            "AXEnhancedUserInterface",
            "AXFocusedUIElement",
            "AXFocusedWindow",
            "AXFullScreen",
            "AXIdentifier",
            "AXMinimized",
            "AXModal",
            "AXParent",
            "AXPosition",
            "AXRole",
            "AXSheets",
            "AXSize",
            "AXSubrole",
            "AXWindow",
            "AXWindows",
        ] {
            let expected = NSString::from_str(key);
            assert!(unsafe { CFEqual(cf(common_attribute_key(key).unwrap()), cf(&expected)) });
        }
        for key in ["", "AXCustomAttribute", "AXRôle", "AXWindow "] {
            assert!(common_attribute_key(key).is_none());
        }
    }
    #[test]
    #[ignore = "read-only focused AX roles diagnostic; requires an interactive desktop"]
    fn inspect_native_focus_roles() {
        use objc2::{class, msg_send, runtime::AnyObject};
        objc2::rc::autoreleasepool(|_| unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: *mut AnyObject = msg_send![ws, frontmostApplication];
            assert!(!front.is_null(), "An interactive session is required");
            let pid: i32 = msg_send![front, processIdentifier];
            println!("front_pid={pid} protected={}", focus_is_protected(pid));
            println!(
                "native_menu_open={:?}",
                crate::window_server::inspect_menu_open()
            );
            let root = Owned::new(AXUIElementCreateSystemWide()).unwrap();
            for (label, root) in [("system", root), ("front", application(pid).unwrap())] {
                let mut element = attr(root.0, "AXFocusedUIElement");
                for depth in 0..6 {
                    let Some(current) = element else { break };
                    for key in ["AXRole", "AXSubrole"] {
                        let value = attr(current.0, key);
                        let roles: Vec<_> = [
                            "AXMenu",
                            "AXMenuItem",
                            "AXMenuBar",
                            "AXMenuBarItem",
                            "AXWindow",
                            "AXApplication",
                            "AXTextField",
                            "AXSecureTextField",
                        ]
                        .into_iter()
                        .filter(|role| value.as_ref().is_some_and(|v| equals_text(v.0, role)))
                        .collect();
                        println!("{label} depth={depth} {key}={roles:?}");
                    }
                    element = attr(current.0, "AXParent");
                }
            }
        });
    }
    #[test]
    #[ignore = "read-only AX benchmark; set AMSPACE_BENCH_PID to a process with a stable window"]
    fn benchmark_window_geometry() {
        use std::time::Instant;
        let pid = std::env::var("AMSPACE_BENCH_PID").unwrap().parse().unwrap();
        let windows = application_windows(pid).unwrap();
        let (&id, window) = windows
            .iter()
            .filter(|(_, w)| geometry_individual(w.0).is_some())
            .min_by_key(|(id, _)| **id)
            .expect("benchmark needs a non-minimized, non-fullscreen window");
        let expected = geometry_individual(window.0).unwrap();
        let mut samples = [Vec::new(), Vec::new()];
        for round in 0..10 {
            for variant in [round % 2, 1 - round % 2] {
                let read = [geometry_individual, geometry][variant];
                let start = Instant::now();
                for _ in 0..64 {
                    assert_eq!(read(window.0), Some(expected));
                }
                samples[variant].push(start.elapsed().as_secs_f64() * 1e6 / 64.);
            }
        }
        for (label, mut times) in ["individual", "batch"].into_iter().zip(samples) {
            times.sort_by(f64::total_cmp);
            println!(
                "window={id} {label}: {:.1} µs/read (median of 10 rounds)",
                (times[4] + times[5]) / 2.
            );
        }
    }
    #[test]
    fn array_traversal_retains_source_and_checks_types() {
        use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
        objc2::rc::autoreleasepool(|_| unsafe {
            let value = NSString::from_str("retained item");
            let source: Retained<AnyObject> = msg_send![class!(NSArray), arrayWithObject:&*value];
            let mut entries = array(Retained::as_ptr(&source).cast());
            drop(source);
            drop(value);
            assert_eq!(entries.len(), 1);
            assert!(equals_text(entries.next().unwrap(), "retained item"));
            assert_eq!(entries.len(), 0);
            assert_eq!(entries.next(), None);
            assert_eq!(array(ptr::null()).len(), 0);
            assert_eq!(array(cf(ns_string!("not an array"))).next(), None);
        });
    }
    #[test]
    fn dialogs_sheets_and_password_fields_protect_keyboard_focus() {
        for role in ["AXDialog", "AXSystemDialog", "AXSheet", "AXSecureTextField"] {
            assert!(protected_focus_role(cf(&NSString::from_str(role))));
        }
        for role in ["AXWindow", "AXStandardWindow", "AXTextField", "AXUnknown"] {
            assert!(!protected_focus_role(cf(&NSString::from_str(role))));
        }
        assert!(!protected_focus_role(ptr::null()));
        assert!(pointer_focus_role_excluded(cf(ns_string!("AXDesktop"))));
        for role in ["AXMenu", "AXMenuItem", "AXMenuBarItem"] {
            assert!(protected_focus_role(cf(&NSString::from_str(role))));
        }
        assert!(!protected_focus_role(cf(ns_string!("AXMenuBar"))));
    }
    #[test]
    fn finder_utility_windows_stay_out_of_the_grid_without_excluding_folders() {
        fn identifier(name: &NSString) -> Owned {
            Owned(unsafe { CFRetain(cf(name)) })
        }
        for name in [
            ns_string!("Info"),
            ns_string!("Inspector"),
            ns_string!("Progress"),
        ] {
            assert!(excluded_system_window(Some("com.apple.finder"), || {
                Some(identifier(name))
            }));
        }
        for name in [ns_string!("FinderWindow"), ns_string!("Progress report")] {
            assert!(!excluded_system_window(Some("com.apple.finder"), || {
                Some(identifier(name))
            }));
        }
        assert!(!excluded_system_window(Some("com.apple.finder"), || None));
        assert!(!excluded_system_window(Some("org.example.editor"), || {
            Some(identifier(ns_string!("Progress")))
        }));
        assert!(excluded_system_window(
            Some("com.apple.archiveutility"),
            || None
        ));
        assert!(!excluded_system_window(None, || None));
    }
    #[test]
    #[ignore = "metadata microbenchmark; run with --release --ignored --nocapture"]
    fn benchmark_window_metadata() {
        objc2::rc::autoreleasepool(|_| unsafe {
            use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
            let entry: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            let value: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithInt:42i32];
            let keys = [
                ns_string!("kCGWindowNumber"),
                ns_string!("kCGWindowLayer"),
                ns_string!("kCGWindowOwnerPID"),
                ns_string!("kCGWindowAlpha"),
            ];
            for key in keys {
                let _: () = msg_send![&entry, setObject:&*value, forKey:key];
            }
            let entries: Retained<AnyObject> = msg_send![class!(NSMutableArray), new];
            for _ in 0..64 {
                let _: () = msg_send![&entries, addObject:&*entry];
            }
            let start = std::time::Instant::now();
            let mut sum = 0;
            for _ in 0..10_000 {
                for entry in array(std::hint::black_box(Retained::as_ptr(&entries).cast())) {
                    for key in keys {
                        sum += number(get(entry, key)).unwrap();
                    }
                }
            }
            assert_eq!(std::hint::black_box(sum), 64 * 10_000 * 4 * 42);
            println!(
                "metadata_ns_per_window={:.1}",
                start.elapsed().as_nanos() as f64 / 640_000.
            );
        });
    }
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
    fn native_pointer_focus_accepts_unknown_roles_but_not_auxiliary_or_modal_surfaces() {
        for role in ["AXWindow", "AXStandardWindow", "AXUnknown"] {
            assert!(!pointer_focus_role_excluded(cf(&NSString::from_str(role))));
        }
        assert!(!pointer_focus_role_excluded(ptr::null()));
        for role in [
            "AXMenu",
            "AXMenuBar",
            "AXMenuItem",
            "AXPopover",
            "AXToolTip",
            "AXDialog",
            "AXSystemDialog",
            "AXSheet",
            "AXSecureTextField",
        ] {
            assert!(pointer_focus_role_excluded(cf(&NSString::from_str(role))));
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
    fn presentation_during_an_animation_size_write_cancels_the_remaining_position() {
        use std::cell::Cell;
        let allowed = Cell::new(true);
        let position = Cell::new(700.);
        assert!(!apply_changed_frame(
            (true, true),
            || allowed.get(),
            || {
                allowed.set(false);
                true
            },
            || {
                position.set(0.);
                true
            },
        ));
        assert_eq!(position.get(), 700.);
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
