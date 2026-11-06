//! AppKit and AX run on the main thread. Idle mode has no recurring timer.
use crate::{
    accessibility as ax, focus, model, native::*, space_style, spaces, updater::Updater,
    window_server as ws,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained, autoreleasepool},
    runtime::{AnyObject, Bool, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::{NSPoint, NSRect, NSString};
use std::{
    cell::RefCell,
    ptr,
    time::{Duration, Instant},
};
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}
thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }
// NSWindowCollectionBehavior: transient panels are hidden by Mission Control.
// Stationary panels remain visible like the desktop and must not be used here.
const BORDER_COLLECTION_BEHAVIOR: usize = (1 << 0) | (1 << 3) | (1 << 6) | (1 << 8);
struct PendingFocus {
    pid: i32,
    id: u32,
    previous: Option<u32>,
    point: NSPoint,
}
struct App {
    target: Retained<AnyObject>,
    status: Retained<AnyObject>,
    border: Retained<AnyObject>,
    defaults: Retained<AnyObject>,
    settings: Option<Retained<AnyObject>>,
    about: Option<Retained<AnyObject>>,
    updater: Option<Updater>,
    updates_switch: Obj,
    tabs: Obj,
    space_page: Obj,
    space_scroll: Obj,
    space_ids: Vec<u64>,
    space_controls: Vec<Obj>,
    border_enabled: bool,
    hover_enabled: bool,
    raise_enabled: bool,
    pending_focus: Option<PendingFocus>,
    focus_target: Option<(i32, u32, Instant)>,
    raise_switch: Obj,
    width: f64,
    rgb: [f64; 3],
    observer: Option<ax::Observer>,
    observed_window: Option<ax::Owned>,
    observed_pid: i32,
    tracked: Option<ws::Window>,
    border_suppressed: bool,
    settle_queued: bool,
    hover_timer: Option<Retained<AnyObject>>,
    last_mouse: NSPoint,
    mouse_since: Instant,
    hover_done: bool,
    sleeping: bool,
    update_queued: bool,
    last_title: String,
    permission: Obj,
    width_label: Obj,
    color_well: Obj,
    login: Obj,
    preview: Obj,
    feedback: Obj,
    border_switch: Obj,
    hover_switch: Obj,
    slider: Obj,
}
fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut a) = cell.try_borrow_mut() {
            if let Some(a) = a.as_mut() {
                f(a);
            }
        }
    });
}
unsafe fn value(defaults: Obj, key: &str) -> bool {
    unsafe { msg_send![defaults, boolForKey:&*NSString::from_str(key)] }
}
unsafe fn number(defaults: Obj, key: &str) -> f64 {
    unsafe { msg_send![defaults, doubleForKey:&*NSString::from_str(key)] }
}
unsafe fn window(frame: NSRect, style: usize) -> Retained<AnyObject> {
    unsafe {
        let w: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
        let w: Retained<AnyObject> =
            msg_send![w, initWithContentRect:frame, styleMask:style, backing:2usize, defer:false];
        let _: () = msg_send![&w, setReleasedWhenClosed:false];
        w
    }
}
unsafe fn button(parent: Obj, frame: NSRect, title: &str, target: Obj, tag: isize) -> Obj {
    unsafe {
        let b = alloc_view(class!(NSButton), frame);
        let _: () = msg_send![&b, setTitle:&*NSString::from_str(title)];
        let _: () = msg_send![&b, setBezelStyle:1usize];
        let _: () = msg_send![&b, setTarget:target];
        let _: () = msg_send![&b, setAction:sel!(change:)];
        let _: () = msg_send![&b, setTag:tag];
        let _: () = msg_send![parent, addSubview:&*b];
        Retained::as_ptr(&b) as Obj
    }
}
unsafe fn popup(parent: Obj, frame: NSRect, items: &[&str], target: Obj, tag: isize) -> Obj {
    unsafe {
        let b: Allocated<AnyObject> = msg_send![class!(NSPopUpButton), alloc];
        let b: Retained<AnyObject> = msg_send![b, initWithFrame:frame, pullsDown:false];
        for item in items {
            let _: () = msg_send![&b, addItemWithTitle:&*NSString::from_str(item)];
        }
        let _: () = msg_send![&b, setTarget:target];
        let _: () = msg_send![&b, setAction:sel!(change:)];
        let _: () = msg_send![&b, setTag:tag];
        let _: () = msg_send![parent, addSubview:&*b];
        Retained::as_ptr(&b) as Obj
    }
}
unsafe fn toggle(
    parent: Obj,
    x: f64,
    y: f64,
    title: &str,
    target: Obj,
    tag: isize,
    on: bool,
) -> Obj {
    unsafe {
        let s = alloc_view(class!(NSSwitch), rect(x, y, 38., 24.));
        let _: () = msg_send![&s, setTarget:target];
        let _: () = msg_send![&s, setAction:sel!(change:)];
        let _: () = msg_send![&s, setTag:tag];
        let _: () = msg_send![&s, setState:if on {1isize} else {0isize}];
        let _: () = msg_send![&s, setAccessibilityLabel:&*NSString::from_str(title)];
        let _: () = msg_send![parent, addSubview:&*s];
        Retained::as_ptr(&s) as Obj
    }
}
unsafe fn card(parent: Obj, frame: NSRect) {
    unsafe {
        let b = alloc_view(class!(NSBox), frame);
        let _: () = msg_send![&b, setBoxType:4usize]; // NSBoxCustom
        let _: () = msg_send![&b, setTransparent:false];
        let _: () = msg_send![&b, setTitlePosition:0usize];
        let _: () = msg_send![&b, setBorderWidth:0f64];
        let _: () = msg_send![&b, setCornerRadius:14f64];
        let c: Obj = msg_send![class!(NSColor), controlBackgroundColor];
        let _: () = msg_send![&b, setFillColor:c];
        let _: () = msg_send![parent, addSubview:&*b];
    }
}
unsafe fn primary_height() -> f64 {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let screen: Obj = msg_send![screens, firstObject];
        let frame: NSRect = msg_send![screen, frame];
        frame.size.height
    }
}
impl App {
    unsafe fn new(target: Retained<AnyObject>) -> Self {
        unsafe {
            let defaults: Retained<AnyObject> =
                msg_send![class!(NSUserDefaults), standardUserDefaults];
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let status: Retained<AnyObject> = msg_send![bar, statusItemWithLength:-1f64];
            let b: Obj = msg_send![&status, button];
            let font: Obj =
                msg_send![class!(NSFont), monospacedDigitSystemFontOfSize:13f64, weight:0.23f64];
            let _: () = msg_send![b, setFont:font];
            let _: () = msg_send![b, setTarget:&*target];
            let _: () = msg_send![b, setAction:sel!(settings:)];
            let _: () =
                msg_send![b, setAccessibilityLabel:&*NSString::from_str("AMSpace · abrir ajustes")];
            let border = window(rect(0., 0., 100., 100.), 128);
            let clear: Obj = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![&border, setBackgroundColor:clear];
            let _: () = msg_send![&border, setOpaque:false];
            let _: () = msg_send![&border, setHasShadow:false];
            let _: () = msg_send![&border, setIgnoresMouseEvents:true];
            let _: () = msg_send![&border, setLevel:0isize];
            let _: () = msg_send![&border, setCollectionBehavior:BORDER_COLLECTION_BEHAVIOR];
            let _: () = msg_send![&border, setHidesOnDeactivate:false];
            let _: () = msg_send![&border, setAnimationBehavior:2isize];
            let v: Obj = msg_send![&border, contentView];
            let _: () = msg_send![v, setWantsLayer:true];
            let saved: Obj = msg_send![&defaults, objectForKey:&*NSString::from_str("width")];
            let width = if saved.is_null() {
                2.
            } else {
                model::thickness(number(Retained::as_ptr(&defaults) as Obj, "width"))
            };
            let saved_color: Obj = msg_send![&defaults, objectForKey:&*NSString::from_str("red")];
            let rgb = if saved_color.is_null() {
                [0.42, 0.48, 0.95]
            } else {
                ["red", "green", "blue"].map(|k| {
                    let n = number(Retained::as_ptr(&defaults) as Obj, k);
                    if n.is_finite() { n.clamp(0., 1.) } else { 0.5 }
                })
            };
            let mut a = Self {
                border_enabled: value(Retained::as_ptr(&defaults) as Obj, "border"),
                hover_enabled: value(Retained::as_ptr(&defaults) as Obj, "hover"),
                raise_enabled: value(Retained::as_ptr(&defaults) as Obj, "raise"),
                pending_focus: None,
                focus_target: None,
                raise_switch: ptr::null_mut(),
                defaults,
                target,
                status,
                border,
                settings: None,
                about: None,
                updater: Updater::new(),
                updates_switch: ptr::null_mut(),
                tabs: ptr::null_mut(),
                space_page: ptr::null_mut(),
                space_scroll: ptr::null_mut(),
                space_ids: vec![],
                space_controls: vec![],
                width,
                rgb,
                observer: None,
                observed_window: None,
                observed_pid: 0,
                tracked: None,
                border_suppressed: false,
                settle_queued: false,
                hover_timer: None,
                last_mouse: NSPoint::new(-1., -1.),
                mouse_since: Instant::now(),
                hover_done: false,
                sleeping: false,
                update_queued: false,
                last_title: String::new(),
                permission: ptr::null_mut(),
                width_label: ptr::null_mut(),
                color_well: ptr::null_mut(),
                login: ptr::null_mut(),
                preview: ptr::null_mut(),
                feedback: ptr::null_mut(),
                border_switch: ptr::null_mut(),
                hover_switch: ptr::null_mut(),
                slider: ptr::null_mut(),
            };
            a.refresh_spaces();
            a.update_window();
            a.configure_hover();
            a
        }
    }
    fn save(&self) {
        unsafe {
            for (k, v) in [
                ("border", self.border_enabled),
                ("hover", self.hover_enabled),
                ("raise", self.raise_enabled),
            ] {
                let _: () = msg_send![&self.defaults, setBool:v, forKey:&*NSString::from_str(k)];
            }
            for (k, v) in [
                ("width", self.width),
                ("red", self.rgb[0]),
                ("green", self.rgb[1]),
                ("blue", self.rgb[2]),
            ] {
                let _: () = msg_send![&self.defaults, setDouble:v, forKey:&*NSString::from_str(k)];
            }
        }
    }
    fn refresh_spaces(&mut self) {
        unsafe {
            let s = spaces::snapshot();
            let b: Obj = msg_send![&self.status, button];
            let title = space_style::render(&s, Retained::as_ptr(&self.defaults) as Obj);
            let _: () = msg_send![b, setAttributedTitle:&*title];
            self.last_title = s.title;
            let _: () = msg_send![b, setToolTip:&*NSString::from_str(&s.detail)];
            let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(&format!("{} · abrir ajustes", s.detail))];
            if !self.preview.is_null() {
                let _: () = msg_send![self.preview, setAttributedStringValue:&*title];
            }
            if !self.space_scroll.is_null() && self.space_ids != s.desktops {
                self.rebuild_space_rows();
            }
        }
    }
    fn hide_border(&self) {
        unsafe {
            let _: () = msg_send![&self.border, orderOut:ptr::null::<AnyObject>()];
        }
    }
    fn update_window(&mut self) {
        self.update_queued = false;
        self.border_suppressed = false;
        if self.sleeping || !self.border_enabled || !ax::trusted() {
            self.observer = None;
            self.observed_window = None;
            self.observed_pid = 0;
            self.tracked = None;
            ws::stop();
            self.hide_border();
            return;
        }
        unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            if pid == std::process::id() as i32 {
                self.tracked = None;
                ws::stop();
                self.hide_border();
                return;
            }
            if self.focus_target.is_some_and(|(owner, id, since)| {
                owner != pid
                    || (since.elapsed() > Duration::from_millis(250)
                        && ax::focused_id(pid) != Some(id))
            }) {
                self.focus_target = None;
            }
            let selected = ws::window_for_focus(pid, self.focus_target.map(|(_, id, _)| id));
            let changed =
                self.observed_pid != pid || self.tracked.map(|w| w.id) != selected.map(|w| w.id);
            self.tracked = selected;
            if changed || self.observer.is_none() {
                self.observer = None;
                self.observed_window = selected.and_then(|w| ax::window_for_id(pid, w.id));
                self.observer = ax::Observer::new(
                    pid,
                    self.observed_window.as_ref().map(|w| w.0),
                    !ws::native_tracking(),
                );
                self.observed_pid = pid;
            }
            if self
                .observed_window
                .as_ref()
                .is_some_and(|w| ax::flag(w.0, "AXFullScreen") || ax::flag(w.0, "AXMinimized"))
            {
                self.border_suppressed = true;
                self.hide_border();
                return;
            }
            self.update_border_geometry();
        }
    }
    // Hot path: compositor geometry only, no app IPC, no fixed delay or animation.
    fn update_border_geometry(&mut self) {
        if self.border_suppressed {
            self.hide_border();
            return;
        }
        let Some(mut target) = self.tracked else {
            self.hide_border();
            return;
        };
        target.frame = ws::bounds(target.id)
            .or_else(|| {
                self.observed_window
                    .as_ref()
                    .and_then(|w| ax::geometry(w.0))
                    .map(|(p, s)| NSRect::new(p, s))
            })
            .unwrap_or(target.frame);
        self.tracked = Some(target);
        unsafe {
            let p = target.frame.origin;
            let s = target.frame.size;
            // Stroke is entirely INSIDE the real window boundary: no outer gap.
            let frame = rect(p.x, primary_height() - p.y - s.height, s.width, s.height);
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions:true];
            let _: () = msg_send![&self.border, setFrame:frame, display:true];
            let view: Obj = msg_send![&self.border, contentView];
            let layer: Obj = msg_send![view, layer];
            let c = color(self.rgb[0], self.rgb[1], self.rgb[2], 0.95);
            let cg: *const std::ffi::c_void = msg_send![c, CGColor];
            let _: () = msg_send![layer, setBorderColor:cg];
            let _: () = msg_send![layer, setBorderWidth:self.width];
            let _: () = msg_send![layer, setCornerRadius:target.radius];
            let _: () = msg_send![layer, setCornerCurve:&*NSString::from_str("continuous")];
            let _: () = msg_send![class!(CATransaction), commit];
            let _: () = msg_send![class!(CATransaction), flush];
            let _: () = msg_send![&self.border, orderWindow:1isize, relativeTo:target.id as isize];
            let id: isize = msg_send![&self.border, windowNumber];
            ws::set_border(id as u32);
        }
    }
    fn settle_focus(&mut self) {
        if self.settle_queued || !self.border_enabled || self.sleeping {
            return;
        }
        self.settle_queued = true;
        unsafe {
            // One bounded reconciliation after creation/activation, not a poll.
            let _: () = msg_send![&self.target,performSelector:sel!(settle:),withObject:ptr::null::<AnyObject>(),afterDelay:0.08f64];
        }
    }
    fn configure_hover(&mut self) {
        unsafe {
            if let Some(t) = self.hover_timer.take() {
                let _: () = msg_send![&t, invalidate];
            }
            self.hover_done = false;
            if self.hover_enabled && ax::trusted() && !self.sleeping {
                let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.08f64, target:&*self.target, selector:sel!(hover:), userInfo:ptr::null::<AnyObject>(), repeats:true];
                let _: () = msg_send![&timer, setTolerance:0.03f64];
                let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
                add_common_timer(run, &timer);
                self.hover_timer = Some(timer);
            }
        }
    }
    fn hover(&mut self) {
        unsafe {
            let point: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            let flags: usize = msg_send![class!(NSEvent), modifierFlags];
            if buttons != 0 {
                self.focus_target = None;
            }
            if buttons != 0 || flags & (1usize << 19) != 0 {
                self.mouse_since = Instant::now();
                self.hover_done = false;
                return;
            }
            if point != self.last_mouse {
                self.last_mouse = point;
                self.mouse_since = Instant::now();
                self.hover_done = false;
                return;
            }
            if self.hover_done || self.mouse_since.elapsed() < Duration::from_millis(240) {
                return;
            }
            self.hover_done = true;
            if !ax::trusted() {
                self.configure_hover();
                self.hide_border();
                return;
            }
            // Hit-testing excludes our own windows. Having settings active must
            // not block focusing another app after the pointer leaves settings.
            let Some((w, pid)) = ax::window_at(point.x, primary_height() - point.y) else {
                return;
            };
            let ws: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: Obj = msg_send![ws, frontmostApplication];
            let front_pid: i32 = msg_send![front, processIdentifier];
            let Some(id) = ax::window_id(w.0) else {
                return;
            };
            if !focus::available() {
                if self.raise_enabled {
                    ax::focus(w.0, pid);
                }
                return;
            }
            let previous = if front_pid == pid {
                ax::focused_id(pid).filter(|old| *old != id)
            } else {
                None
            };
            if let Some(previous) = previous {
                focus::deactivate(pid, previous);
            }
            self.pending_focus = Some(PendingFocus {
                pid,
                id,
                previous,
                point,
            });
            let delay = if previous.is_some() { 0.04f64 } else { 0.0f64 };
            let _: () = msg_send![&self.target, performSelector:sel!(focusPending:), withObject:ptr::null::<AnyObject>(), afterDelay:delay];
        }
    }
    fn finish_focus(&mut self) {
        let Some(pending) = self.pending_focus.take() else {
            return;
        };
        unsafe {
            let point: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            let flags: usize = msg_send![class!(NSEvent), modifierFlags];
            if !self.hover_enabled
                || self.sleeping
                || point != pending.point
                || buttons != 0
                || flags & (1usize << 19) != 0
            {
                // Do not override a click or an application switch during the delay.
                let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
                let front: Obj = msg_send![workspace, frontmostApplication];
                let pid: i32 = msg_send![front, processIdentifier];
                if buttons == 0 && pid == pending.pid {
                    if let Some(previous) = pending.previous {
                        focus::activate(pid, previous, true);
                    }
                }
                return;
            }
            let Some(window) = ax::window_for_id(pending.pid, pending.id) else {
                return;
            };
            let activated = focus::activate(pending.pid, pending.id, pending.previous.is_some());
            if activated {
                if self.raise_enabled {
                    ax::raise(window.0);
                }
                self.focus_target = Some((pending.pid, pending.id, Instant::now()));
                self.update_window();
                self.settle_focus();
            }
            if std::env::var_os("AMSPACE_TRACE_FOCUS").is_some() {
                eprintln!(
                    "hover: pid={} target={} keyboard={} raise={} AXFocused={:?}",
                    pending.pid,
                    pending.id,
                    activated,
                    self.raise_enabled,
                    ax::focused_id(pending.pid)
                );
            }
        }
    }
    fn show_settings(&mut self) {
        unsafe {
            if self.settings.is_none() {
                self.build_settings();
            }
            self.refresh_permission();
            self.refresh_login();
            self.refresh_spaces();
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps:true];
            let _: () = msg_send![self.settings.as_ref().unwrap(), makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    unsafe fn build_settings(&mut self) {
        unsafe {
            let w = window(rect(0., 0., 480., 660.), 1 | 2 | 4 | 32768);
            let _: () = msg_send![&w, setTitle:&*NSString::from_str("AMSpace")];
            let _: () = msg_send![&w, setTitleVisibility:1isize];
            let _: () = msg_send![&w, setTitlebarAppearsTransparent:true];
            let _: () = msg_send![&w, setHidesOnDeactivate:false];
            let _: () = msg_send![&w, center];
            let effect = alloc_view(class!(NSVisualEffectView), rect(0., 0., 480., 660.));
            let _: () = msg_send![&effect, setMaterial:12isize];
            let _: () = msg_send![&effect, setBlendingMode:0isize];
            let _: () = msg_send![&effect, setState:1isize];
            let _: () = msg_send![&w, setContentView:&*effect];
            let root = Retained::as_ptr(&effect) as Obj;
            let target = Retained::as_ptr(&self.target) as Obj;
            label(root, rect(28., 571., 250., 42.), "amspace", 30., 0.5, false);
            label(
                root,
                rect(29., 543., 350., 22.),
                "Tu espacio. Tu foco.",
                13.,
                0.,
                true,
            );
            self.preview = label(
                root,
                rect(324., 578., 128., 28.),
                &self.last_title,
                18.,
                0.3,
                false,
            );
            let _: () = msg_send![self.preview, setAlignment:2isize];
            let segmented = alloc_view(class!(NSSegmentedControl), rect(24., 496., 432., 32.));
            let _: () = msg_send![&segmented, setSegmentCount:3isize];
            for (i, title) in ["Ventanas", "Escritorios", "Aplicación"].iter().enumerate() {
                let _: () = msg_send![&segmented, setLabel:&*NSString::from_str(title), forSegment:i as isize];
                let _: () = msg_send![&segmented, setWidth:144f64, forSegment:i as isize];
            }
            let _: () = msg_send![&segmented, setSelectedSegment:0isize];
            let _: () = msg_send![&segmented, setTarget:target];
            let _: () = msg_send![&segmented, setAction:sel!(change:)];
            let _: () = msg_send![&segmented, setTag:13isize];
            let _: () = msg_send![root, addSubview:&*segmented];
            let tabs = alloc_view(class!(NSTabView), rect(24., 120., 432., 360.));
            let _: () = msg_send![&tabs, setTabViewType:6usize];
            for i in 0..3 {
                let view = alloc_view(class!(NSView), rect(0., 0., 432., 360.));
                let page = Retained::as_ptr(&view) as Obj;
                match i {
                    0 => self.build_window_page(page, target),
                    1 => self.build_space_page(page, target),
                    _ => self.build_application_page(page, target),
                }
                let item: Retained<AnyObject> = msg_send![class!(NSTabViewItem), new];
                let _: () = msg_send![&item, setView:&*view];
                let _: () = msg_send![&tabs, addTabViewItem:&*item];
            }
            let _: () = msg_send![root, addSubview:&*tabs];
            self.tabs = Retained::as_ptr(&tabs) as Obj;
            self.permission = label(root, rect(29., 89., 290., 18.), "", 11., 0., true);
            button(
                root,
                rect(332., 82., 125., 28.),
                "Accesibilidad…",
                target,
                6,
            );
            self.feedback = label(root, rect(29., 64., 425., 18.), "", 10., 0., true);
            label(root, rect(29., 25., 110., 20.), "AMSPACE", 10., 0.3, true);
            button(root, rect(234., 19., 116., 28.), "Acerca de…", target, 10);
            button(root, rect(355., 19., 103., 28.), "Salir", target, 7);
            self.settings = Some(w);
        }
    }
    unsafe fn build_window_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            card(root, rect(0., 164., 432., 196.));
            label(
                root,
                rect(18., 324., 310., 22.),
                "Borde de la ventana activa",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(18., 299., 350., 20.),
                "Un contorno sutil para situarte al instante.",
                12.,
                0.,
                true,
            );
            self.border_switch = toggle(
                root,
                374.,
                324.,
                "Borde de la ventana activa",
                target,
                1,
                self.border_enabled,
            );
            label(root, rect(18., 263., 90., 22.), "Color", 13., 0., false);
            let well = alloc_view(class!(NSColorWell), rect(365., 257., 48., 30.));
            let _: () = msg_send![&well, setColor:color(self.rgb[0],self.rgb[1],self.rgb[2],1.)];
            let _: () = msg_send![&well, setTarget:target];
            let _: () = msg_send![&well, setAction:sel!(change:)];
            let _: () = msg_send![&well, setTag:3isize];
            let _: () =
                msg_send![&well, setAccessibilityLabel:&*NSString::from_str("Color del borde")];
            let _: () = msg_send![root, addSubview:&*well];
            self.color_well = Retained::as_ptr(&well) as Obj;
            for (i, (name, rgb)) in [
                ("Índigo", [0.42, 0.48, 0.95]),
                ("Menta", [0.25, 0.72, 0.62]),
                ("Ámbar", [0.94, 0.65, 0.27]),
                ("Rosa", [0.88, 0.42, 0.60]),
            ]
            .iter()
            .enumerate()
            {
                let b = button(
                    root,
                    rect(105. + i as f64 * 56., 259., 50., 28.),
                    "●",
                    target,
                    20 + i as isize,
                );
                let attrs: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
                let _: () = msg_send![&attrs, setObject:color(rgb[0],rgb[1],rgb[2],1.), forKey:&*NSString::from_str("NSColor")];
                let title: Allocated<AnyObject> = msg_send![class!(NSAttributedString), alloc];
                let title: Retained<AnyObject> =
                    msg_send![title, initWithString:&*NSString::from_str("●"), attributes:&*attrs];
                let _: () = msg_send![b, setAttributedTitle:&*title];
                let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(name)];
            }
            label(root, rect(18., 215., 90., 22.), "Grosor", 13., 0., false);
            self.width_label = label(
                root,
                rect(360., 215., 58., 22.),
                &format!("{:.1} pt", self.width),
                12.,
                0.,
                true,
            );
            let slider = alloc_view(class!(NSSlider), rect(105., 214., 230., 24.));
            let _: () = msg_send![&slider, setMinValue:1f64];
            let _: () = msg_send![&slider, setMaxValue:6f64];
            let _: () = msg_send![&slider, setDoubleValue:self.width];
            let _: () = msg_send![&slider, setContinuous:true];
            let _: () = msg_send![&slider, setTarget:target];
            let _: () = msg_send![&slider, setAction:sel!(change:)];
            let _: () = msg_send![&slider, setTag:4isize];
            let _: () = msg_send![&slider, setAccessibilityLabel:&*NSString::from_str("Grosor del borde en puntos")];
            let _: () = msg_send![root, addSubview:&*slider];
            self.slider = Retained::as_ptr(&slider) as Obj;
            label(
                root,
                rect(18., 181., 350., 18.),
                "Esquinas suaves · sin interceptar tus clics",
                11.,
                0.,
                true,
            );
            card(root, rect(0., 0., 432., 152.));
            label(
                root,
                rect(18., 118., 310., 22.),
                "El foco sigue al cursor",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(18., 94., 350., 19.),
                "Activa la ventana al detenerte sobre ella.",
                12.,
                0.,
                true,
            );
            label(
                root,
                rect(18., 74., 350., 17.),
                "Mantén ⌥ para pausar. No actúa al arrastrar.",
                11.,
                0.,
                true,
            );
            self.hover_switch = toggle(
                root,
                374.,
                118.,
                "El foco sigue al cursor",
                target,
                2,
                self.hover_enabled,
            );
            label(
                root,
                rect(18., 39., 310., 22.),
                "Traer al primer plano",
                13.,
                0.3,
                false,
            );
            label(
                root,
                rect(18., 15., 350., 18.),
                "Al enfocar, sitúa la ventana sobre las demás.",
                11.,
                0.,
                true,
            );
            self.raise_switch = toggle(
                root,
                374.,
                39.,
                "Traer al primer plano",
                target,
                11,
                self.raise_enabled,
            );
            let _: () = msg_send![self.raise_switch, setEnabled:self.hover_enabled];
        }
    }
    unsafe fn build_application_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            card(root, rect(0., 282., 432., 78.));
            label(
                root,
                rect(18., 328., 310., 22.),
                "Abrir al iniciar sesión",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(18., 301., 350., 20.),
                "Disponible en cuanto empieza tu día.",
                12.,
                0.,
                true,
            );
            self.login = toggle(
                root,
                374.,
                326.,
                "Abrir al iniciar sesión",
                target,
                5,
                false,
            );
            card(root, rect(0., 137., 432., 133.));
            label(
                root,
                rect(18., 231., 240., 22.),
                concat!("Versión ", env!("CARGO_PKG_VERSION")),
                14.,
                0.3,
                false,
            );
            let check = button(root, rect(300., 225., 113., 30.), "Buscar…", target, 8);
            let available = self.updater.is_some();
            let _: () = msg_send![check, setEnabled:available];
            label(
                root,
                rect(18., 195., 345., 20.),
                "Buscar actualizaciones automáticamente",
                12.,
                0.,
                false,
            );
            self.updates_switch = toggle(
                root,
                374.,
                190.,
                "Buscar actualizaciones automáticamente",
                target,
                9,
                self.updater.as_ref().is_some_and(Updater::automatic_checks),
            );
            let _: () = msg_send![self.updates_switch, setEnabled:available];
            label(
                root,
                rect(18., 164., 360., 18.),
                "Avisos diarios · tú decides cuándo instalar.",
                11.,
                0.,
                true,
            );
            label(
                root,
                rect(18., 88., 390., 24.),
                "Creado por Alberto Martín",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(18., 62., 390., 20.),
                "Rust y macOS. Sin cuentas ni telemetría.",
                12.,
                0.,
                true,
            );
            button(
                root,
                rect(15., 19., 185., 28.),
                "Historial de versiones",
                target,
                32,
            );
        }
    }
    unsafe fn build_space_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            self.space_page = root;
            label(
                root,
                rect(18., 327., 220., 22.),
                "Mostrar escritorios como",
                13.,
                0.3,
                false,
            );
            let menu = popup(
                root,
                rect(248., 321., 168., 30.),
                &["Puntos", "Números", "Iconos"],
                target,
                12,
            );
            let _: () = msg_send![menu, selectItemAtIndex:space_style::mode(Retained::as_ptr(&self.defaults) as Obj)];
            label(
                root,
                rect(18., 291., 400., 20.),
                "El activo destaca; los demás quedan atenuados.",
                12.,
                0.,
                true,
            );
            label(
                root,
                rect(18., 265., 400., 18.),
                "Elige un icono para cada escritorio de esta pantalla.",
                11.,
                0.,
                true,
            );
            let scroll = alloc_view(class!(NSScrollView), rect(0., 10., 432., 242.));
            let _: () = msg_send![&scroll, setHasVerticalScroller:true];
            let _: () = msg_send![&scroll, setAutohidesScrollers:true];
            let _: () = msg_send![&scroll, setDrawsBackground:false];
            let _: () = msg_send![root, addSubview:&*scroll];
            self.space_scroll = Retained::as_ptr(&scroll) as Obj;
            self.rebuild_space_rows();
        }
    }
    fn rebuild_space_rows(&mut self) {
        if self.space_scroll.is_null() {
            return;
        }
        unsafe {
            let snapshot = spaces::snapshot();
            self.space_ids = snapshot.desktops.clone();
            self.space_controls.clear();
            let height = (snapshot.desktops.len() as f64 * 58.).max(242.);
            let document = alloc_view(class!(NSView), rect(0., 0., 415., height));
            let root = Retained::as_ptr(&document) as Obj;
            let target = Retained::as_ptr(&self.target) as Obj;
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            let names: Vec<_> = space_style::ICONS.iter().map(|(name, _)| *name).collect();
            for (index, id) in snapshot.desktops.iter().enumerate() {
                let y = height - (index + 1) as f64 * 58.;
                label(
                    root,
                    rect(18., y + 18., 180., 22.),
                    &format!("Escritorio {}", index + 1),
                    13.,
                    0.,
                    false,
                );
                let menu = popup(
                    root,
                    rect(211., y + 13., 190., 30.),
                    &names,
                    target,
                    1000 + index as isize,
                );
                let _: () = msg_send![menu, selectItemAtIndex:space_style::icon(defaults, *id, index) as isize];
                for (i, (name, symbol)) in space_style::ICONS.iter().enumerate().skip(1) {
                    let item: Obj = msg_send![menu, itemAtIndex:i as isize];
                    let image: Obj = msg_send![class!(NSImage), imageWithSystemSymbolName:&*NSString::from_str(symbol), accessibilityDescription:&*NSString::from_str(name)];
                    if !image.is_null() {
                        let _: () = msg_send![item, setImage:image];
                    }
                }
                let _: () = msg_send![menu, setEnabled:space_style::mode(defaults) == 2];
                self.space_controls.push(menu);
            }
            if snapshot.desktops.is_empty() {
                label(
                    root,
                    rect(18., height - 48., 390., 24.),
                    "No hay escritorios disponibles en esta pantalla.",
                    12.,
                    0.,
                    true,
                );
            }
            let _: () = msg_send![self.space_scroll, setDocumentView:&*document];
            let _: () = msg_send![root, scrollPoint:NSPoint::new(0., (height - 242.).max(0.))];
        }
    }
    fn check_updates(&self) {
        if let Some(updater) = &self.updater {
            updater.check();
        } else {
            self.feedback("Actualizaciones disponibles en el paquete instalado.");
        }
    }
    fn show_about(&mut self) {
        unsafe {
            if self.about.is_none() {
                let w = window(rect(0., 0., 400., 380.), 1 | 2 | 32768);
                let _: () = msg_send![&w, setTitle:&*NSString::from_str("Acerca de AMSpace")];
                let _: () = msg_send![&w, setTitlebarAppearsTransparent:true];
                let _: () = msg_send![&w, setHidesOnDeactivate:false];
                let _: () = msg_send![&w, center];
                let root: Obj = msg_send![&w, contentView];
                let target = Retained::as_ptr(&self.target) as Obj;
                let image = alloc_view(class!(NSImageView), rect(158., 254., 84., 84.));
                let app: Obj = msg_send![class!(NSApplication), sharedApplication];
                let icon: Obj = msg_send![app, applicationIconImage];
                let _: () = msg_send![&image, setImage:icon];
                let _: () = msg_send![root, addSubview:&*image];
                for (y, title, size, weight, secondary) in [
                    (213., "amspace", 27., 0.5, false),
                    (
                        183.,
                        concat!("Versión ", env!("CARGO_PKG_VERSION")),
                        12.,
                        0.,
                        true,
                    ),
                    (145., "Creado por Alberto Martín", 15., 0.3, false),
                    (118., "Tu espacio. Tu foco.", 12., 0., true),
                    (5., "© 2026 Alberto Martín · Licencia MIT", 10., 0., true),
                ] {
                    let label = label(
                        root,
                        rect(20., y, 360., 34.),
                        title,
                        size,
                        weight,
                        secondary,
                    );
                    let _: () = msg_send![label, setAlignment:1isize];
                }
                button(
                    root,
                    rect(33., 82., 165., 30.),
                    "Instagram · @amartinro",
                    target,
                    30,
                );
                button(
                    root,
                    rect(202., 82., 165., 30.),
                    "X · @amartinro",
                    target,
                    31,
                );
                button(
                    root,
                    rect(113., 48., 174., 28.),
                    "Historial de versiones",
                    target,
                    32,
                );
                self.about = Some(w);
            }
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps:true];
            let _: () = msg_send![self.about.as_ref().unwrap(), makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn refresh_permission(&self) {
        unsafe {
            if !self.permission.is_null() {
                text(
                    self.permission,
                    if ax::trusted() {
                        "● Accesibilidad concedida"
                    } else {
                        "○ Borde y foco requieren Accesibilidad"
                    },
                );
            }
        }
    }
    fn feedback(&self, message: &str) {
        unsafe {
            if !self.feedback.is_null() {
                text(self.feedback, message);
            }
        }
    }
    fn refresh_login(&self) {
        unsafe {
            if self.login.is_null() {
                return;
            }
            let service: Obj = msg_send![class!(SMAppService), mainAppService];
            let status: isize = msg_send![service, status];
            let _: () =
                msg_send![self.login,setState:if status==1 || status==2 {1isize} else {0isize}];
            if status == 2 {
                self.feedback("Autoriza AMSpace en Ajustes del Sistema → Ítems de inicio.");
            }
        }
    }
    fn change(&mut self, sender: Obj) {
        unsafe {
            let tag: isize = msg_send![sender, tag];
            self.feedback("");
            match tag {
                1 => {
                    let state: isize = msg_send![sender, state];
                    self.border_enabled = state == 1;
                }
                2 => {
                    let state: isize = msg_send![sender, state];
                    self.hover_enabled = state == 1;
                    self.configure_hover();
                    let _: () = msg_send![self.raise_switch, setEnabled:self.hover_enabled];
                }
                3 => {
                    let c: Obj = msg_send![sender, color];
                    let space: Obj = msg_send![class!(NSColorSpace), sRGBColorSpace];
                    let c: Obj = msg_send![c,colorUsingColorSpace:space];
                    if !c.is_null() {
                        self.rgb = [
                            msg_send![c, redComponent],
                            msg_send![c, greenComponent],
                            msg_send![c, blueComponent],
                        ];
                    }
                }
                4 => {
                    let n: f64 = msg_send![sender, doubleValue];
                    self.width = model::thickness((n * 2.).round() / 2.);
                    text(self.width_label, &format!("{:.1} pt", self.width));
                }
                5 => {
                    let service: Obj = msg_send![class!(SMAppService), mainAppService];
                    let state: isize = msg_send![sender, state];
                    let mut error: Obj = ptr::null_mut();
                    let ok: bool = if state == 1 {
                        msg_send![service,registerAndReturnError:&mut error]
                    } else {
                        msg_send![service,unregisterAndReturnError:&mut error]
                    };
                    if !ok {
                        self.feedback("No se pudo cambiar. Instala AMSpace.app en Aplicaciones.");
                    }
                    self.refresh_login();
                }
                6 => {
                    ax::request_permission();
                    open_url(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                    );
                }
                7 => {
                    let app: Obj = msg_send![class!(NSApplication), sharedApplication];
                    let _: () = msg_send![app,terminate:sender];
                }
                11 => {
                    let state: isize = msg_send![sender, state];
                    self.raise_enabled = state == 1;
                }
                12 => {
                    let selected: isize = msg_send![sender, indexOfSelectedItem];
                    let _: () = msg_send![&self.defaults, setInteger:space_style::normalize_mode(selected), forKey:&*NSString::from_str("spaceStyle")];
                    for control in &self.space_controls {
                        let _: () = msg_send![*control, setEnabled:selected == 2];
                    }
                    self.refresh_spaces();
                }
                13 => {
                    let selected: isize = msg_send![sender, selectedSegment];
                    let _: () = msg_send![self.tabs, selectTabViewItemAtIndex:selected];
                    if selected == 1 {
                        self.rebuild_space_rows();
                    }
                }
                1000.. => {
                    if let Some(id) = self.space_ids.get((tag - 1000) as usize) {
                        let selected: isize = msg_send![sender, indexOfSelectedItem];
                        let _: () = msg_send![&self.defaults, setInteger:space_style::normalize_icon(selected) as isize, forKey:&*NSString::from_str(&format!("spaceIcon.{id}"))];
                        self.refresh_spaces();
                    }
                }
                8 => self.check_updates(),
                9 => {
                    let enabled: isize = msg_send![sender, state];
                    if let Some(updater) = &self.updater {
                        updater.set_automatic_checks(enabled == 1);
                    }
                }
                10 => self.show_about(),
                30 => open_url("https://www.instagram.com/amartinro/"),
                31 => open_url("https://x.com/amartinro"),
                32 => open_url("https://github.com/amartinrojarvis/amspace/releases"),
                20..=23 => {
                    self.rgb = [
                        [0.42, 0.48, 0.95],
                        [0.25, 0.72, 0.62],
                        [0.94, 0.65, 0.27],
                        [0.88, 0.42, 0.60],
                    ][(tag - 20) as usize];
                    let _: () = msg_send![self.color_well,setColor:color(self.rgb[0],self.rgb[1],self.rgb[2],1.)];
                }
                _ => {}
            }
            self.save();
            self.update_window();
            self.refresh_permission();
            if (tag == 1 || tag == 2) && !ax::trusted() {
                self.feedback("Concede el permiso y vuelve a esta ventana para activarlo.");
            }
        }
    }
}
unsafe fn open_url(url: &str) {
    unsafe {
        let ws: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        let url: Obj = msg_send![class!(NSURL),URLWithString:&*NSString::from_str(url)];
        let _: bool = msg_send![ws,openURL:url];
    }
}
// AX focus callbacks are coalesced onto the next turn, with no artificial delay.
pub fn window_server_event(focus_changed: bool, lifecycle: bool) {
    with_app(|a| {
        if a.sleeping || !a.border_enabled {
            return;
        }
        if focus_changed {
            a.update_window();
        } else {
            a.update_border_geometry();
        }
        if lifecycle {
            a.settle_focus();
        }
    });
}
pub fn queue_window_update() {
    with_app(|a| {
        if !a.update_queued {
            a.update_queued = true;
            unsafe {
                let _: () = msg_send![&a.target,performSelector:sel!(update:),withObject:ptr::null::<AnyObject>(),afterDelay:0f64];
            }
        }
    });
}
extern "C-unwind" fn update(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.update_window());
}
extern "C-unwind" fn settle(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.settle_queued = false;
        a.update_window();
    });
}
extern "C-unwind" fn settings(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.show_settings());
}
extern "C-unwind" fn about(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.show_about());
}
extern "C-unwind" fn check_updates(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.check_updates());
}
extern "C-unwind" fn change(_: Obj, _: Sel, sender: Obj) {
    with_app(|a| a.change(sender));
}
extern "C-unwind" fn focus_pending(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.finish_focus());
}
extern "C-unwind" fn hover(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| with_app(|a| a.hover()));
}
extern "C-unwind" fn workspace(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.refresh_spaces();
        a.refresh_permission();
        a.update_window();
        a.settle_focus();
    });
}
extern "C-unwind" fn became_active(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.refresh_permission();
        a.refresh_login();
        a.configure_hover();
        a.update_window();
    });
}
extern "C-unwind" fn sleep(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.sleeping = true;
        a.configure_hover();
        a.update_window();
    });
}
extern "C-unwind" fn wake(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.sleeping = false;
        a.configure_hover();
        a.refresh_spaces();
        a.update_window();
    });
}
extern "C-unwind" fn reopen(_: Obj, _: Sel, _: Obj, _: Bool) -> Bool {
    with_app(|a| a.show_settings());
    Bool::YES
}
unsafe fn observe(center: Obj, target: Obj, name: &str, selector: Sel) {
    unsafe {
        let _: () = msg_send![center,addObserver:target,selector:selector,name:&*NSString::from_str(name),object:ptr::null::<AnyObject>()];
    }
}
extern "C-unwind" fn quit(_: Obj, _: Sel, _: Obj) {
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, terminate:ptr::null::<AnyObject>()];
    }
}
unsafe fn make_menu(app: Obj, target: Obj) {
    unsafe {
        let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
        let item: Retained<AnyObject> = msg_send![class!(NSMenuItem), new];
        let submenu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
        for (title, action, key) in [
            ("Acerca de AMSpace", sel!(about:), ""),
            ("Buscar actualizaciones…", sel!(checkUpdates:), ""),
            ("Ajustes…", sel!(settings:), ","),
            ("Cerrar ventana", sel!(performClose:), "w"),
            ("Salir de AMSpace", sel!(quit:), "q"),
        ] {
            let command: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
            let command: Retained<AnyObject> = msg_send![command, initWithTitle:&*NSString::from_str(title), action:action, keyEquivalent:&*NSString::from_str(key)];
            if key != "w" {
                let _: () = msg_send![&command, setTarget:target];
            }
            let _: () = msg_send![&submenu, addItem:&*command];
        }
        let _: () = msg_send![&item, setSubmenu:&*submenu];
        let _: () = msg_send![&menu, addItem:&*item];
        let _: () = msg_send![app, setMainMenu:&*menu];
    }
}
pub fn run() {
    let app: Retained<AnyObject> = autoreleasepool(|_| unsafe {
        let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![&app,setActivationPolicy:1isize];
        let mut builder = ClassBuilder::new(c"AMSpaceController", class!(NSObject)).unwrap();
        for (s, f) in [
            (
                sel!(settings:),
                settings as extern "C-unwind" fn(Obj, Sel, Obj),
            ),
            (sel!(about:), about),
            (sel!(checkUpdates:), check_updates),
            (sel!(change:), change),
            (sel!(hover:), hover),
            (sel!(focusPending:), focus_pending),
            (sel!(update:), update),
            (sel!(settle:), settle),
            (sel!(workspace:), workspace),
            (sel!(becameActive:), became_active),
            (sel!(sleep:), sleep),
            (sel!(wake:), wake),
            (sel!(quit:), quit),
        ] {
            builder.add_method(s, f);
        }
        builder.add_method(
            sel!(applicationShouldHandleReopen:hasVisibleWindows:),
            reopen as extern "C-unwind" fn(Obj, Sel, Obj, Bool) -> Bool,
        );
        let target: Retained<AnyObject> = msg_send![builder.register(), new];
        let _: () = msg_send![&app,setDelegate:&*target];
        make_menu(
            Retained::as_ptr(&app) as Obj,
            Retained::as_ptr(&target) as Obj,
        );
        let ws: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        let center: Obj = msg_send![ws, notificationCenter];
        for name in [
            "NSWorkspaceActiveSpaceDidChangeNotification",
            "NSWorkspaceDidActivateApplicationNotification",
            "NSWorkspaceDidUnhideApplicationNotification",
        ] {
            observe(
                center,
                Retained::as_ptr(&target) as Obj,
                name,
                sel!(workspace:),
            );
        }
        for name in [
            "NSWorkspaceWillSleepNotification",
            "NSWorkspaceScreensDidSleepNotification",
            "NSWorkspaceSessionDidResignActiveNotification",
        ] {
            observe(center, Retained::as_ptr(&target) as Obj, name, sel!(sleep:));
        }
        for name in [
            "NSWorkspaceDidWakeNotification",
            "NSWorkspaceScreensDidWakeNotification",
            "NSWorkspaceSessionDidBecomeActiveNotification",
        ] {
            observe(center, Retained::as_ptr(&target) as Obj, name, sel!(wake:));
        }
        let center: Obj = msg_send![class!(NSNotificationCenter), defaultCenter];
        observe(
            center,
            Retained::as_ptr(&target) as Obj,
            "NSApplicationDidChangeScreenParametersNotification",
            sel!(workspace:),
        );
        observe(
            center,
            Retained::as_ptr(&target) as Obj,
            "NSApplicationDidBecomeActiveNotification",
            sel!(becameActive:),
        );
        APP.with(|cell| *cell.borrow_mut() = Some(App::new(target)));
        app
    });
    autoreleasepool(|_| unsafe {
        let _: () = msg_send![&app, run];
    });
    APP.with(|cell| {
        cell.borrow_mut().take();
    });
}
