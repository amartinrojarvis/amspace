//! Window identity and geometry come from the compositor, never from an app's
//! potentially stale AXFocusedWindow. Private symbols are optional and isolated.
use crate::accessibility::{Cf, Owned, array, get, number};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect, ns_string};
use std::{
    ffi::{CStr, c_void},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(options: u32, relative: u32) -> Cf;
    fn CGRectMakeWithDictionaryRepresentation(dict: Cf, result: *mut NSRect) -> bool;
    fn CGWindowLevelForKey(key: i32) -> i32;
}
unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void),
    );
}
struct Api {
    connection: unsafe extern "C" fn() -> i32,
    bounds: unsafe extern "C" fn(i32, u32, *mut NSRect) -> i32,
    move_window: Option<unsafe extern "C" fn(i32, u32, *const NSPoint) -> i32>,
    register: unsafe extern "C" fn(
        extern "C" fn(u32, *const c_void, usize, *mut c_void),
        u32,
        *mut c_void,
    ) -> i32,
    watch: unsafe extern "C" fn(i32, *const u32, i32) -> i32,
    query: unsafe extern "C" fn(i32, Cf, u32) -> Cf,
    iterator: unsafe extern "C" fn(Cf) -> Cf,
    advance: unsafe extern "C" fn(Cf) -> bool,
    radii: Option<unsafe extern "C" fn(Cf) -> Cf>,
}
static API: OnceLock<Option<Api>> = OnceLock::new();
static WATCHED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static TARGET: AtomicU32 = AtomicU32::new(0);
static BORDER: AtomicU32 = AtomicU32::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU32 = AtomicU32::new(0);
static QUEUED: AtomicBool = AtomicBool::new(false);
static REGISTERED: OnceLock<bool> = OnceLock::new();
fn api() -> Option<&'static Api> {
    API.get_or_init(|| unsafe {
        let h = libc::dlopen(
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
            libc::RTLD_LAZY,
        );
        if h.is_null() {
            return None;
        }
        let sym = |s: &CStr| {
            let p = libc::dlsym(h, s.as_ptr());
            (!p.is_null()).then_some(p)
        };
        macro_rules! load {
            ($name:literal,$ty:ty) => {
                std::mem::transmute::<*mut c_void, $ty>(sym($name)?)
            };
        }
        Some(Api {
            connection: load!(c"SLSMainConnectionID", unsafe extern "C" fn() -> i32),
            bounds: load!(
                c"SLSGetWindowBounds",
                unsafe extern "C" fn(i32, u32, *mut NSRect) -> i32
            ),
            move_window: sym(c"SLSMoveWindow").map(|p| {
                std::mem::transmute::<
                    *mut c_void,
                    unsafe extern "C" fn(i32, u32, *const NSPoint) -> i32,
                >(p)
            }),
            register: load!(
                c"SLSRegisterNotifyProc",
                unsafe extern "C" fn(
                    extern "C" fn(u32, *const c_void, usize, *mut c_void),
                    u32,
                    *mut c_void,
                ) -> i32
            ),
            watch: load!(
                c"SLSRequestNotificationsForWindows",
                unsafe extern "C" fn(i32, *const u32, i32) -> i32
            ),
            query: load!(
                c"SLSWindowQueryWindows",
                unsafe extern "C" fn(i32, Cf, u32) -> Cf
            ),
            iterator: load!(
                c"SLSWindowQueryResultCopyWindows",
                unsafe extern "C" fn(Cf) -> Cf
            ),
            advance: load!(
                c"SLSWindowIteratorAdvance",
                unsafe extern "C" fn(Cf) -> bool
            ),
            radii: sym(c"SLSWindowIteratorGetCornerRadii")
                .map(|p| std::mem::transmute::<*mut c_void, unsafe extern "C" fn(Cf) -> Cf>(p)),
        })
    })
    .as_ref()
}
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub id: u32,
    pub frame: NSRect,
    pub radius: f64,
}
fn valid(frame: NSRect) -> bool {
    [
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    ]
    .iter()
    .all(|n| n.is_finite())
        && frame.size.width > 40.
        && frame.size.height > 40.
}
/// The compositor returns front-to-back order. A normal window of the active
/// process wins even when AXFocus incorrectly continues reporting another one.
pub fn front_window(pid: i32) -> Option<Window> {
    let mut window = focus_windows(pid, None)?.first().copied()?;
    window.radius = corner_radius(window.id);
    Some(window)
}
/// Ordered candidates, so the UI can reject AX popups without losing the real
/// browser window underneath. None is query failure, Some(empty) is no window.
pub fn focus_windows(pid: i32, focused: Option<u32>) -> Option<Vec<Window>> {
    ENABLED.store(true, Ordering::Release);
    let list = Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) })?;
    observe_presentation(list.0);
    if crate::desktop_mode::active() {
        return Some(Vec::new());
    }
    let visible_spaces: Vec<_> = crate::spaces::visible_desktops()
        .into_iter()
        .map(|(_, space)| space)
        .collect();
    let mut ids = Vec::new();
    let mut windows = Vec::new();
    for info in array(list.0) {
        let Some(layer) = number(get(info, ns_string!("kCGWindowLayer"))) else {
            continue;
        };
        if !(0..=3).contains(&layer) {
            continue;
        }
        let Some(id) = number(get(info, ns_string!("kCGWindowNumber"))).map(|n| n as u32) else {
            continue;
        };
        let mut frame = NSRect::ZERO;
        let dictionary = get(info, ns_string!("kCGWindowBounds"));
        if dictionary.is_null()
            || !unsafe { CGRectMakeWithDictionaryRepresentation(dictionary, &mut frame) }
            || !valid(frame)
        {
            continue;
        }
        ids.push(id);
        if layer != 0 || number(get(info, ns_string!("kCGWindowOwnerPID"))) != Some(i64::from(pid))
        {
            continue;
        }
        if crate::accessibility::decimal(get(info, ns_string!("kCGWindowAlpha")))
            .is_some_and(|alpha| alpha <= 0.)
            || (!visible_spaces.is_empty()
                && !crate::focus::in_visible_space(
                    crate::spaces::window_space(id),
                    &visible_spaces,
                ))
        {
            continue;
        }
        windows.push(Window {
            id,
            frame,
            radius: 10.,
        });
    }
    if let Some(index) = windows.iter().position(|w| Some(w.id) == focused) {
        let preferred = windows.remove(index);
        windows.insert(0, preferred);
    }
    let previous = TARGET.load(Ordering::Acquire);
    let target = if ids.contains(&previous) { previous } else { 0 };
    watch(ids, target); // Only the UI promotes a candidate after AX validation.
    Some(windows)
}
pub fn track_target(id: u32) {
    TARGET.store(id, Ordering::Release);
}
/// Find the visible window at the pointer, independently of stale AX hit tests.
/// Our click-through border is ignored; menus and own settings block targeting.
pub fn window_at(point: NSPoint) -> Option<(i32, u32)> {
    hit_windows_at(point).first().map(|(pid, id)| (*pid, *id))
}
pub fn hit_windows_at(point: NSPoint) -> Vec<(i32, u32)> {
    let visible_spaces: Vec<_> = crate::spaces::visible_desktops()
        .into_iter()
        .map(|(_, space)| space)
        .collect();
    if visible_spaces.is_empty() {
        return Vec::new();
    }
    let Some(list) = Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) }) else {
        return Vec::new();
    };
    observe_presentation(list.0);
    if crate::desktop_mode::active() || contains_menu(list.0) {
        return Vec::new(); // Menus own keyboard tracking even outside their frame.
    }
    let mut hits = Vec::new();
    for info in array(list.0) {
        let Some(id) = number(get(info, ns_string!("kCGWindowNumber"))).map(|id| id as u32) else {
            continue;
        };
        if id == BORDER.load(Ordering::Acquire) {
            continue;
        }
        if number(get(info, ns_string!("kCGWindowLayer"))) != Some(0) {
            continue;
        }
        let mut frame = NSRect::ZERO;
        if !unsafe {
            CGRectMakeWithDictionaryRepresentation(
                get(info, ns_string!("kCGWindowBounds")),
                &mut frame,
            )
        } || point.x < frame.origin.x
            || point.y < frame.origin.y
            || point.x >= frame.origin.x + frame.size.width
            || point.y >= frame.origin.y + frame.size.height
        {
            continue;
        }
        if crate::accessibility::decimal(get(info, ns_string!("kCGWindowAlpha")))
            .is_some_and(|alpha| alpha <= 0.)
            || !crate::focus::in_visible_space(crate::spaces::window_space(id), &visible_spaces)
        {
            continue;
        }
        let Some(pid) = number(get(info, ns_string!("kCGWindowOwnerPID"))).map(|pid| pid as i32)
        else {
            continue;
        };
        if pid == std::process::id() as i32 {
            if hits.is_empty() {
                return Vec::new();
            }
            break; // Our window behind a real hit cannot block that foreground hit.
        }
        if valid(frame) {
            hits.push((pid, id));
        }
    }
    hits
}
fn contains_menu(list: Cf) -> bool {
    // kCGPopUpMenuWindowLevelKey. Menubars, tooltips and our preview are distinct.
    let menu_level = i64::from(unsafe { CGWindowLevelForKey(11) });
    array(list).any(|info| {
        number(get(info, ns_string!("kCGWindowLayer"))) == Some(menu_level)
            && !crate::accessibility::decimal(get(info, ns_string!("kCGWindowAlpha")))
                .is_some_and(|alpha| alpha <= 0.)
    })
}
#[cfg(test)]
pub fn inspect_menu_open() -> Option<bool> {
    Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) }).map(|list| contains_menu(list.0))
}
pub fn bounds(id: u32) -> Option<NSRect> {
    let api = api()?;
    let mut frame = NSRect::ZERO;
    (unsafe { (api.bounds)((api.connection)(), id, &mut frame) } == 0 && valid(frame))
        .then_some(frame)
}
pub fn corner_radius(id: u32) -> f64 {
    let read = || unsafe {
        let api = api()?;
        let radii = api.radii?;
        let id: Retained<AnyObject> = msg_send![class!(NSNumber),numberWithUnsignedInt:id];
        let ids: Retained<AnyObject> = msg_send![class!(NSArray),arrayWithObject:&*id];
        let query = Owned::new((api.query)(
            (api.connection)(),
            Retained::as_ptr(&ids).cast(),
            0,
        ))?;
        let iterator = Owned::new((api.iterator)(query.0))?;
        if !(api.advance)(iterator.0) {
            return None;
        }
        let values = Owned::new(radii(iterator.0))?;
        let n = number(array(values.0).next()?)?;
        (0..=80).contains(&n).then_some(n as f64)
    };
    read().unwrap_or(10.)
}
fn watch(mut ids: Vec<u32>, target: u32) {
    ids.sort_unstable();
    ids.dedup(); // Stacking changes do not change the notification subscription.
    TARGET.store(target, Ordering::Release);
    let Some(api) = api() else {
        return;
    };
    REGISTERED.get_or_init(|| unsafe {
        // Stacking updates affect clipping only, never focus or foreign ordering.
        let mut ok = true;
        for event in [804, 806, 807, 808, 815, 816, 723, 1325, 1326] {
            ok &= (api.register)(notification, event, ptr::null_mut()) == 0;
        }
        ok
    });
    let changed = WATCHED.lock().is_ok_and(|mut watched| {
        if *watched == ids {
            return false;
        }
        *watched = ids.clone();
        true
    });
    if changed {
        unsafe {
            (api.watch)((api.connection)(), ids.as_ptr(), ids.len() as i32);
        }
    }
}
pub fn set_border(id: u32) {
    BORDER.store(id, Ordering::Release);
}
pub fn native_tracking() -> bool {
    REGISTERED.get().copied().unwrap_or(false)
}
pub fn stop() {
    ENABLED.store(false, Ordering::Release);
    watch(Vec::new(), 0);
}
extern "C" fn notification(event: u32, data: *const c_void, len: usize, _: *mut c_void) {
    let target = TARGET.load(Ordering::Acquire);
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let id = if !data.is_null() && len >= 4 {
        Some(unsafe { ptr::read_unaligned(data.cast::<u32>()) })
    } else {
        None
    };
    let flag = classify_event(event, id, target, BORDER.load(Ordering::Acquire));
    if flag == 0 {
        return;
    }
    // Translation changes no pixels. No AppKit hop or CA commit is necessary.
    if flag == 1 && event == 806 {
        move_border(target); // Translation now also needs an updated clipping mask.
    }
    PENDING.fetch_or(flag, Ordering::AcqRel);
    if !QUEUED.swap(true, Ordering::AcqRel) {
        unsafe {
            dispatch_async_f(ptr::addr_of!(_dispatch_main_q), ptr::null_mut(), deliver);
        }
    }
}
fn classify_event(event: u32, id: Option<u32>, target: u32, border: u32) -> u32 {
    if border != 0 && id == Some(border) {
        return 0; // Own-panel lifecycle/stacking cannot feed back into the border.
    }
    match event {
        // Creation/closure must work even while no target exists yet.
        1325 | 1326 => 2 | 4,
        806 | 807 if target != 0 && id == Some(target) => 1,
        804 | 806 | 807 | 808 | 815 | 816 | 723 if target != 0 => 8,
        // Stacking notifications include the effect of ordering our own border.
        // They are output, not a focus source. Real focus/visibility changes are
        // delivered by AXObserver and NSWorkspace, with lifecycle events above.
        _ => 0,
    }
}

fn move_border(target: u32) -> bool {
    let border = BORDER.load(Ordering::Acquire);
    if border == 0 {
        return false;
    }
    let Some(api) = api() else {
        return false;
    };
    let Some(move_window) = api.move_window else {
        return false;
    };
    let Some(frame) = bounds(target) else {
        return false;
    };
    unsafe { move_window((api.connection)(), border, &frame.origin) == 0 }
}

pub fn visible_border_regions(target: u32, border: u32) -> Option<Vec<NSRect>> {
    let list = Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) })?;
    observe_presentation(list.0);
    if crate::desktop_mode::active() {
        return Some(Vec::new());
    }
    let mut occluders = Vec::new();
    let visible_spaces: Vec<_> = crate::spaces::visible_desktops()
        .into_iter()
        .map(|(_, space)| space)
        .collect();
    for info in array(list.0) {
        let Some(id) = number(get(info, ns_string!("kCGWindowNumber"))).map(|id| id as u32) else {
            continue;
        };
        if id == border {
            continue;
        }
        if id != target
            && !number(get(info, ns_string!("kCGWindowLayer")))
                .is_some_and(|layer| (0..=3).contains(&layer))
        {
            continue;
        }
        if !visible_spaces.is_empty()
            && !crate::focus::in_visible_space(crate::spaces::window_space(id), &visible_spaces)
        {
            continue;
        }
        if crate::accessibility::decimal(get(info, ns_string!("kCGWindowAlpha")))
            .is_some_and(|alpha| alpha <= 0.)
        {
            continue;
        }
        let mut frame = NSRect::ZERO;
        if !unsafe {
            CGRectMakeWithDictionaryRepresentation(
                get(info, ns_string!("kCGWindowBounds")),
                &mut frame,
            )
        } {
            continue;
        }
        if id == target {
            return Some(crate::border::visible_regions(frame, &occluders));
        }
        if number(get(info, ns_string!("kCGWindowLayer")))
            .is_some_and(|layer| (0..=3).contains(&layer))
            && frame.size.width > 0.
            && frame.size.height > 0.
        {
            occluders.push(frame);
        }
    }
    Some(Vec::new())
}
extern "C" fn deliver(_: *mut c_void) {
    QUEUED.store(false, Ordering::Release);
    let flags = PENDING.swap(0, Ordering::AcqRel);
    objc2::rc::autoreleasepool(|_| crate::ui::window_server_event(flags & 2 != 0, flags & 4 != 0));
}

/// Visible normal windows, including occluded windows. Excludes our UI/overlays.
pub fn tile_windows() -> Vec<(i32, u32, NSRect)> {
    try_tile_windows().unwrap_or_default()
}

/// Preserve query failure separately from a successfully observed empty desktop.
pub fn try_tile_windows() -> Option<Vec<(i32, u32, NSRect)>> {
    let list = Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) })?;
    observe_presentation(list.0);
    if crate::desktop_mode::active() {
        return None;
    }
    Some(
        array(list.0)
            .filter_map(|info| {
                let pid = number(get(info, ns_string!("kCGWindowOwnerPID")))? as i32;
                if pid == std::process::id() as i32
                    || number(get(info, ns_string!("kCGWindowLayer"))) != Some(0)
                {
                    return None;
                }
                let id = number(get(info, ns_string!("kCGWindowNumber")))? as u32;
                let mut frame = NSRect::ZERO;
                (unsafe {
                    CGRectMakeWithDictionaryRepresentation(
                        get(info, ns_string!("kCGWindowBounds")),
                        &mut frame,
                    )
                } && valid(frame))
                .then_some((pid, id, frame))
            })
            .collect(),
    )
}

/// Compositor identities include offscreen/minimized windows. Avoid synchronous
/// AX calls into background apps just to discover that their windows still exist.
pub fn all_window_ids() -> Option<std::collections::HashSet<(i32, u32)>> {
    let list = Owned::new(unsafe { CGWindowListCopyWindowInfo(16, 0) })?;
    Some(
        array(list.0)
            .filter_map(|info| {
                Some((
                    number(get(info, ns_string!("kCGWindowOwnerPID")))? as i32,
                    number(get(info, ns_string!("kCGWindowNumber")))? as u32,
                ))
            })
            .collect(),
    )
}

fn presentation_window(
    owner: Cf,
    layer: i64,
    alpha: Option<f64>,
    frame: NSRect,
    screens: &[NSRect],
) -> crate::desktop_mode::Mode {
    use crate::desktop_mode::Mode;
    if !crate::accessibility::equals_text(owner, "WindowManager") || alpha.is_some_and(|a| a <= 0.)
    {
        return Mode::Normal;
    }
    let fullscreen = screens.iter().any(|screen| {
        let width = (frame.origin.x + frame.size.width).min(screen.origin.x + screen.size.width)
            - frame.origin.x.max(screen.origin.x);
        let height = (frame.origin.y + frame.size.height).min(screen.origin.y + screen.size.height)
            - frame.origin.y.max(screen.origin.y);
        width.max(0.) * height.max(0.) >= screen.size.width * screen.size.height * 0.9
    });
    if !fullscreen {
        return Mode::Normal;
    }
    match layer {
        18 => Mode::Desktop,
        19 | 1500 => Mode::Overview,
        _ => Mode::Normal,
    }
}
fn presentation_from_list(list: Cf) -> crate::desktop_mode::Mode {
    use crate::desktop_mode::Mode;
    let mut screens = None;
    let mut mode = Mode::Normal;
    for info in array(list) {
        let Some(layer) = number(get(info, ns_string!("kCGWindowLayer"))) else {
            continue;
        };
        if ![18, 19, 1500].contains(&layer) {
            continue;
        }
        let owner = get(info, ns_string!("kCGWindowOwnerName"));
        if !crate::accessibility::equals_text(owner, "WindowManager") {
            continue;
        }
        let mut frame = NSRect::ZERO;
        if !unsafe {
            CGRectMakeWithDictionaryRepresentation(
                get(info, ns_string!("kCGWindowBounds")),
                &mut frame,
            )
        } {
            continue;
        }
        let screens = screens.get_or_insert_with(|| {
            let configuration = crate::native::screen_configuration();
            let height = configuration.first().map_or(0., |s| s.frame.size.height);
            configuration
                .iter()
                .map(|s| crate::tiling::ax_screen_rect(s.frame, height))
                .collect::<Vec<_>>()
        });
        let candidate = presentation_window(
            owner,
            layer,
            crate::accessibility::decimal(get(info, ns_string!("kCGWindowAlpha"))),
            frame,
            screens,
        );
        if candidate == Mode::Desktop {
            return candidate;
        }
        if candidate == Mode::Overview {
            mode = candidate;
        }
    }
    mode
}
fn observe_presentation(list: Cf) {
    crate::desktop_mode::observe_compositor(presentation_from_list(list));
}
pub fn presentation_mode() -> Option<crate::desktop_mode::Mode> {
    Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) })
        .map(|list| presentation_from_list(list.0))
}
pub fn visible_normal_window_ids() -> Option<std::collections::HashSet<(i32, u32)>> {
    let list = Owned::new(unsafe { CGWindowListCopyWindowInfo(1 | 16, 0) })?;
    Some(
        array(list.0)
            .filter_map(|info| {
                if number(get(info, ns_string!("kCGWindowLayer"))) != Some(0) {
                    return None;
                }
                Some((
                    number(get(info, ns_string!("kCGWindowOwnerPID")))? as i32,
                    number(get(info, ns_string!("kCGWindowNumber")))? as u32,
                ))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::classify_event;
    #[test]
    fn native_windowmanager_desktop_overlay_is_distinct_from_dock_and_small_panels() {
        use super::*;
        use crate::{desktop_mode::Mode, native::rect};
        let screens = [rect(0., 0., 1680., 1050.), rect(-900., -400., 880., 1580.)];
        let wm = ns_string!("WindowManager") as *const _ as Cf;
        let dock = ns_string!("Dock") as *const _ as Cf;
        assert_eq!(
            presentation_window(wm, 18, Some(1.), screens[0], &screens),
            Mode::Desktop
        );
        assert_eq!(
            presentation_window(wm, 18, Some(1.), screens[1], &screens),
            Mode::Desktop
        );
        assert_eq!(
            presentation_window(wm, 1500, Some(1.), screens[0], &screens),
            Mode::Overview
        );
        assert_eq!(
            presentation_window(wm, 19, Some(1.), screens[0], &screens),
            Mode::Overview
        );
        assert_eq!(
            presentation_window(dock, 20, Some(1.), screens[0], &screens),
            Mode::Normal
        );
        assert_eq!(
            presentation_window(dock, 18, Some(1.), screens[0], &screens),
            Mode::Normal
        );
        assert_eq!(
            presentation_window(wm, 18, Some(0.), screens[0], &screens),
            Mode::Normal
        );
        assert_eq!(
            presentation_window(wm, 18, Some(1.), rect(0., 0., 80., 1050.), &screens),
            Mode::Normal
        );
    }
    #[test]
    fn only_visible_popup_menus_block_focus_not_menubars_or_preview_overlays() {
        use super::*;
        objc2::rc::autoreleasepool(|_| unsafe {
            let info: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            let list: Retained<AnyObject> = msg_send![class!(NSArray), arrayWithObject:&*info];
            for (key, expected) in [(8, false), (9, false), (11, true), (15, false)] {
                let level = CGWindowLevelForKey(key);
                let level: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithInt:level];
                let _: () =
                    msg_send![&info, setObject:&*level, forKey:ns_string!("kCGWindowLayer")];
                assert_eq!(contains_menu(Retained::as_ptr(&list).cast()), expected);
            }
            let level: Retained<AnyObject> =
                msg_send![class!(NSNumber), numberWithInt:CGWindowLevelForKey(11)];
            let alpha: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithDouble:0_f64];
            let _: () = msg_send![&info, setObject:&*level, forKey:ns_string!("kCGWindowLayer")];
            let _: () = msg_send![&info, setObject:&*alpha, forKey:ns_string!("kCGWindowAlpha")];
            assert!(!contains_menu(Retained::as_ptr(&list).cast()));
        });
    }
    #[test]
    fn ordering_feedback_on_target_or_sibling_is_not_a_focus_change() {
        for event in [804, 808, 815, 816, 723] {
            for id in [None, Some(10), Some(20), Some(40)] {
                let flags = classify_event(event, id, 10, 40);
                assert_eq!(flags & 2, 0); // Never selects/activates another window.
                assert_eq!(flags, if id == Some(40) { 0 } else { 8 });
            }
        }
        assert_eq!(classify_event(1508, None, 10, 40), 0);
        assert_eq!(classify_event(806, Some(10), 10, 40), 1);
        assert_eq!(classify_event(807, Some(10), 10, 40), 1);
    }
    #[test]
    fn own_border_events_cannot_create_a_lifecycle_feedback_loop() {
        for event in [804, 806, 807, 808, 815, 816, 723, 1325, 1326, 1508] {
            assert_eq!(classify_event(event, Some(40), 10, 40), 0);
        }
        assert_eq!(classify_event(1325, Some(41), 10, 40), 6);
        assert_eq!(classify_event(1326, Some(10), 10, 40), 6);
    }
    #[test]
    fn new_windows_are_detected_without_an_existing_border() {
        assert_eq!(classify_event(1325, None, 0, 0), 6);
        assert_eq!(classify_event(1508, None, 0, 0), 0);
        assert_eq!(classify_event(1326, None, 42, 0), 6);
    }
    #[test]
    fn focus_and_geometry_do_not_confuse_sibling_windows() {
        assert_eq!(classify_event(806, Some(10), 20, 0), 8);
        assert_eq!(classify_event(807, Some(20), 20, 0), 1);
        assert_eq!(classify_event(808, Some(10), 20, 0), 8);
        assert_eq!(classify_event(808, Some(99), 20, 0), 8);
        assert_eq!(classify_event(806, None, 20, 0), 8);
    }
}
