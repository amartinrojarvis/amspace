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
    let (owner, id) = crate::window_server::window_at(NSPoint::new(x, y))?;
    if hit_pid != owner {
        return None;
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
extern "C" fn changed(_: Cf, _: Cf, _: Cf, _: *mut c_void) {
    crate::ui::queue_window_update();
}
