//! AppKit and AX run on the main thread. Idle mode has no recurring timer.
use crate::{
    accessibility as ax, focus, model, native::*, space_style, space_switch, spaces,
    updater::Updater, window_server as ws,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
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
#[derive(Clone, Copy, PartialEq)]
struct BorderPaint {
    width: f64,
    rgb: [f64; 3],
    radius: f64,
    scale: f64,
}
struct PendingFocus {
    pid: i32,
    id: u32,
    previous: Option<u32>,
    point: NSPoint,
}
struct App {
    tile_manager: crate::tile_manager::Manager,
    tile_timer: Option<Retained<AnyObject>>,
    tile_interval: f64,
    tile_hotkey: Option<crate::tile_hotkey::HotKey>,
    picker_hotkey: Option<crate::tile_hotkey::HotKey>,
    layout_picker: Option<crate::tile_picker::Picker>,
    picker_settings: Option<Retained<AnyObject>>,
    picker_hint: Obj,
    picker_mods: Obj,
    picker_keys: Obj,
    picker_suggestions: Obj,
    picker_apply: Obj,
    suggested_shortcuts: Vec<(usize, usize)>,
    tile_space: Option<u64>,
    tile_controls: Vec<Obj>,
    tile_grid_window: Option<Retained<AnyObject>>,
    tile_grid_fields: Vec<Obj>,
    tile_grid_status: Obj,
    tile_grid_toggle: Obj,
    tile_rules_window: Option<Retained<AnyObject>>,
    tile_rules_scroll: Obj,
    tile_rule_ids: Vec<String>,
    target: Retained<AnyObject>,
    status: Retained<AnyObject>,
    status_buttons: Vec<Retained<AnyObject>>,
    status_ids: Vec<u64>,
    switching: Option<space_switch::Switch>,
    hover_resume: Instant,
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
    border_paint: Option<BorderPaint>,
    border_target: u32,
    border_needs_order: bool,
    border_retry_pending: bool,
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
unsafe fn primary_height() -> Option<f64> {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let screen: Obj = msg_send![screens, firstObject];
        if screen.is_null() {
            return None;
        }
        let frame: NSRect = msg_send![screen, frame];
        (frame.size.height.is_finite() && frame.size.height > 0.).then_some(frame.size.height)
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
            let _: () = msg_send![b, setAction:sel!(statusClick:)];
            let _: usize = msg_send![b, sendActionOn:((1usize << 2) | (1usize << 4))];
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
                border_paint: None,
                border_target: 0,
                border_needs_order: true,
                border_retry_pending: false,
                hover_enabled: value(Retained::as_ptr(&defaults) as Obj, "hover"),
                raise_enabled: value(Retained::as_ptr(&defaults) as Obj, "raise"),
                tile_manager: Default::default(),
                tile_timer: None,
                tile_interval: 0.,
                tile_hotkey: None,
                picker_hotkey: None,
                layout_picker: None,
                picker_settings: None,
                picker_hint: ptr::null_mut(),
                picker_mods: ptr::null_mut(),
                picker_keys: ptr::null_mut(),
                picker_suggestions: ptr::null_mut(),
                picker_apply: ptr::null_mut(),
                suggested_shortcuts: Vec::new(),
                tile_space: None,
                tile_controls: vec![],
                tile_grid_window: None,
                tile_grid_fields: vec![],
                tile_grid_status: ptr::null_mut(),
                tile_grid_toggle: ptr::null_mut(),
                tile_rules_window: None,
                tile_rules_scroll: ptr::null_mut(),
                tile_rule_ids: vec![],
                pending_focus: None,
                focus_target: None,
                raise_switch: ptr::null_mut(),
                defaults,
                target,
                status,
                status_buttons: vec![],
                status_ids: vec![],
                switching: None,
                hover_resume: Instant::now(),
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
            a.configure_tiling();
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
            self.refresh_status_buttons(&s);
            self.last_title = s.title;
            let _: () = msg_send![b, setToolTip:&*NSString::from_str(&s.detail)];
            let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(&format!("{} · clic derecho para ajustes", s.detail))];
            if !self.preview.is_null() {
                let _: () = msg_send![self.preview, setAttributedStringValue:&*title];
            }
            if !self.space_scroll.is_null() && self.space_ids != s.desktops {
                self.rebuild_space_rows();
            }
        }
    }
    fn refresh_status_buttons(&mut self, snapshot: &spaces::Snapshot) {
        unsafe {
            let parent: Obj = msg_send![&self.status, button];
            let compact = snapshot.desktops.len() > 9 || snapshot.desktops.is_empty();
            let count = if compact { 1 } else { snapshot.desktops.len() };
            let width = if compact { 70. } else { 24. };
            let frame: NSRect = msg_send![parent, bounds];
            if self.status_ids != snapshot.desktops || self.status_buttons.len() != count {
                for button in self.status_buttons.drain(..) {
                    let _: () = msg_send![&button, removeFromSuperview];
                }
                self.status_ids = snapshot.desktops.clone();
                let _: () = msg_send![parent, setTitle:&*NSString::from_str("")];
                let _: () = msg_send![&self.status, setLength:width * count as f64];
                for index in 0..count {
                    let button = alloc_view(
                        desktop_button_class(),
                        rect(index as f64 * width, 0., width, frame.size.height.max(22.)),
                    );
                    let _: () = msg_send![&button, setBordered:false];
                    let _: () = msg_send![&button, setRefusesFirstResponder:true];
                    let _: () = msg_send![&button, setFocusRingType:1usize];
                    let _: () = msg_send![&button, setAutoresizingMask:16usize];
                    let _: () = msg_send![&button, setTarget:&*self.target];
                    let _: () = msg_send![&button, setAction:if compact {sel!(desktopMenu:)} else {sel!(selectDesktop:)}];
                    let _: () = msg_send![&button, setTag:if compact {0isize} else {snapshot.desktops[index] as isize}];
                    let _: () = msg_send![parent, addSubview:&*button];
                    self.status_buttons.push(button);
                }
            }
            for (index, button) in self.status_buttons.iter().enumerate() {
                let title = if compact {
                    space_style::render(snapshot, Retained::as_ptr(&self.defaults) as Obj)
                } else {
                    space_style::render_desktop(
                        snapshot,
                        Retained::as_ptr(&self.defaults) as Obj,
                        index,
                    )
                };
                let _: () = msg_send![button, setAttributedTitle:&*title];
                let label = if compact {
                    "Elegir escritorio".into()
                } else {
                    format!(
                        "Ir al escritorio {}{}",
                        index + 1,
                        if snapshot.active == Some(index) {
                            ", activo"
                        } else {
                            ""
                        }
                    )
                };
                let _: () = msg_send![button, setAccessibilityLabel:&*NSString::from_str(&label)];
                let _: () = msg_send![button, setToolTip:&*NSString::from_str(&format!("{label} · clic derecho para ajustes"))];
            }
        }
    }
    fn desktop_menu(&self) -> Retained<AnyObject> {
        unsafe {
            let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
            let snapshot = spaces::snapshot();
            for (index, id) in snapshot.desktops.iter().enumerate() {
                let item: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
                let item: Retained<AnyObject> = msg_send![item, initWithTitle:&*NSString::from_str(&format!("Escritorio {}", index + 1)), action:sel!(selectDesktop:), keyEquivalent:&*NSString::from_str("")];
                let _: () = msg_send![&item, setTarget:&*self.target];
                let _: () = msg_send![&item, setTag:*id as isize];
                let _: () = msg_send![&item, setState:if snapshot.active == Some(index) {1isize} else {0isize}];
                let _: () = msg_send![&menu, addItem:&*item];
            }
            let settings: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
            let settings: Retained<AnyObject> = msg_send![settings, initWithTitle:&*NSString::from_str("Ajustes…"), action:sel!(settings:), keyEquivalent:&*NSString::from_str("")];
            let _: () = msg_send![&settings, setTarget:&*self.target];
            let _: () = msg_send![&menu, addItem:&*settings];
            menu
        }
    }
    fn select_desktop(&mut self, id: u64) {
        if std::env::var_os("AMSPACE_TRACE_SPACES").is_some() {
            eprintln!("spaces: click={id} pending={}", self.switching.is_some());
        }
        if let Some(switch) = self.switching.as_mut() {
            if let Err(error) = switch.retarget(id) {
                self.feedback(error);
            }
            return;
        }
        if spaces::destination(id).is_some_and(|d| d.current == id) {
            return;
        }
        match space_switch::Switch::start(id) {
            Ok(switch) => {
                self.switching = Some(switch);
                self.tile_manager.space_changed();
                self.pending_focus = None;
                self.focus_target = None;
                self.border_suppressed = true;
                self.hide_border();
                self.schedule_switch_step();
            }
            Err(error) => {
                self.show_settings();
                self.feedback(error);
            }
        }
    }
    fn schedule_switch_step(&self) {
        schedule_switch_timer(Retained::as_ptr(&self.target) as Obj);
    }
    fn step_switch(&mut self) {
        let Some(switch) = self.switching.as_mut() else {
            return;
        };
        match switch.step() {
            Ok(space_switch::Progress::Waiting) => self.schedule_switch_step(),
            result => {
                self.switching = None;
                self.tile_manager.space_changed();
                self.hover_resume = Instant::now() + Duration::from_millis(400);
                self.refresh_spaces();
                self.update_window();
                if let Err(error) = result {
                    if std::env::var_os("AMSPACE_TRACE_SPACES").is_some() {
                        eprintln!("spaces: {error}");
                    }
                    self.show_settings();
                    self.feedback(error);
                }
            }
        }
    }
    fn hide_border(&self) {
        unsafe {
            let _: () = msg_send![&self.border, orderOut:ptr::null::<AnyObject>()];
        }
    }
    fn update_window(&mut self) {
        if self.switching.is_some() {
            self.hide_border();
            return;
        }
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
                        && ax::focused_id(pid).is_some_and(|actual| actual != id))
            }) {
                self.focus_target = None;
            }
            let Some(candidates) = ws::focus_windows(pid, self.focus_target.map(|(_, id, _)| id))
            else {
                // One bounded retry for a failed compositor query, preserving
                // a same-process border instead of hiding/showing it immediately.
                if self.observed_pid == pid && self.tracked.is_some() && !self.border_retry_pending
                {
                    self.border_retry_pending = true;
                    self.settle_focus();
                } else {
                    self.border_suppressed = true;
                    self.hide_border();
                }
                return;
            };
            self.border_retry_pending = false;
            let mut selected = None;
            let mut selected_element = None;
            for mut candidate in candidates {
                if self.observed_pid == pid && self.tracked.is_some_and(|w| w.id == candidate.id) {
                    candidate.radius = self.tracked.unwrap().radius;
                    selected = Some(candidate);
                    break;
                }
                let element = ax::window_for_id(pid, candidate.id);
                // Gecko/Chromium publish temporary layer-zero popup windows.
                // A confirmed nonstandard AX window must not steal the border.
                if element
                    .as_ref()
                    .and_then(|w| ax::attr(w.0, "AXSubrole"))
                    .is_some_and(|role| !ax::equals_text(role.0, "AXStandardWindow"))
                {
                    continue;
                }
                candidate.radius = ws::corner_radius(candidate.id);
                selected = Some(candidate);
                selected_element = element;
                break;
            }
            ws::track_target(selected.map_or(0, |w| w.id));
            let changed =
                self.observed_pid != pid || self.tracked.map(|w| w.id) != selected.map(|w| w.id);
            self.tracked = selected;
            if changed || self.observer.is_none() {
                self.observer = None;
                self.observed_window = selected_element
                    .or_else(|| selected.and_then(|w| ax::window_for_id(pid, w.id)));
                self.observer = ax::Observer::new(
                    pid,
                    self.observed_window.as_ref().map(|w| w.0),
                    !ws::native_tracking(),
                );
                self.observed_pid = pid;
            }
            if let Some(target) = selected {
                let border: isize = msg_send![&self.border, windowNumber];
                self.border_needs_order = self.border_target != target.id
                    || ws::border_needs_ordering(border as u32, target.id).unwrap_or(false);
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
            let Some(height) = primary_height() else {
                self.hide_border();
                return;
            };
            let frame = rect(p.x, height - p.y - s.height, s.width, s.height);
            let id: isize = msg_send![&self.border, windowNumber];
            let geometry_changed = ws::bounds(id as u32) != Some(target.frame);
            if geometry_changed {
                let _: () = msg_send![class!(CATransaction), begin];
                let _: () = msg_send![class!(CATransaction), setDisableActions:true];
                let _: () = msg_send![&self.border, setFrame:frame, display:true];
            }
            let scale: f64 = msg_send![&self.border, backingScaleFactor];
            let paint = BorderPaint {
                width: self.width,
                rgb: self.rgb,
                radius: target.radius,
                scale,
            };
            let paint_changed = self.border_paint != Some(paint);
            if paint_changed {
                if !geometry_changed {
                    let _: () = msg_send![class!(CATransaction), begin];
                    let _: () = msg_send![class!(CATransaction), setDisableActions:true];
                }
                let view: Obj = msg_send![&self.border, contentView];
                let layer: Obj = msg_send![view, layer];
                let c = color(self.rgb[0], self.rgb[1], self.rgb[2], 0.95);
                let cg: *const std::ffi::c_void = msg_send![c, CGColor];
                let _: () = msg_send![layer, setBorderColor:cg];
                let _: () = msg_send![layer, setBorderWidth:self.width];
                let _: () = msg_send![layer, setCornerRadius:target.radius];
                let _: () = msg_send![layer, setCornerCurve:&*NSString::from_str("continuous")];
                let _: () = msg_send![layer, setContentsScale:scale];
                self.border_paint = Some(paint);
            }
            if geometry_changed || paint_changed {
                let _: () = msg_send![class!(CATransaction), commit];
                let _: () = msg_send![class!(CATransaction), flush];
            }
            let visible: bool = msg_send![&self.border, isVisible];
            if self.border_needs_order || self.border_target != target.id || !visible {
                let _: () =
                    msg_send![&self.border, orderWindow:1isize, relativeTo:target.id as isize];
                self.border_needs_order = false;
                self.border_target = target.id;
            }
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
        if self.layout_picker.is_some()
            || self.switching.is_some()
            || Instant::now() < self.hover_resume
        {
            return;
        }
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
            let Some(height) = primary_height() else {
                return;
            };
            let Some((w, pid)) = ax::window_at(point.x, height - point.y) else {
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
            if self.switching.is_some()
                || Instant::now() < self.hover_resume
                || !self.hover_enabled
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
            let _: () = msg_send![&segmented, setSegmentCount:4isize];
            for (i, title) in ["Ventanas", "Escritorios", "Tiling", "Aplicación"]
                .iter()
                .enumerate()
            {
                let _: () = msg_send![&segmented, setLabel:&*NSString::from_str(title), forSegment:i as isize];
                let _: () = msg_send![&segmented, setWidth:108f64, forSegment:i as isize];
            }
            let _: () = msg_send![&segmented, setSelectedSegment:0isize];
            let _: () = msg_send![&segmented, setTarget:target];
            let _: () = msg_send![&segmented, setAction:sel!(change:)];
            let _: () = msg_send![&segmented, setTag:13isize];
            let _: () = msg_send![root, addSubview:&*segmented];
            let tabs = alloc_view(class!(NSTabView), rect(24., 120., 432., 360.));
            let _: () = msg_send![&tabs, setTabViewType:6usize];
            for i in 0..4 {
                let view = alloc_view(class!(NSView), rect(0., 0., 432., 360.));
                let page = Retained::as_ptr(&view) as Obj;
                match i {
                    0 => self.build_window_page(page, target),
                    1 => self.build_space_page(page, target),
                    2 => self.build_tile_page(page, target),
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
    fn configure_tiling(&mut self) {
        unsafe {
            if let Some(t) = self.tile_timer.take() {
                let _: () = msg_send![&t, invalidate];
            }
            self.tile_hotkey = None;
            self.picker_hotkey = None;
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            if crate::tile_manager::pref(defaults, "tile.enabled") == 0
                || self.sleeping
                || !ax::trusted()
            {
                self.layout_picker = None;
                self.tile_manager.cancel_drag();
                return;
            }
            self.tile_hotkey = crate::tile_hotkey::HotKey::new(
                crate::tile_hotkey::key_index(crate::tile_manager::pref(defaults, "tile.key")),
                crate::tile_hotkey::modifier_index(crate::tile_manager::pref(
                    defaults,
                    "tile.mods",
                )),
                crate::tile_hotkey::Action::Float,
            );
            self.picker_hotkey = crate::tile_hotkey::HotKey::new(
                crate::tile_hotkey::key_index(crate::tile_manager::pref_or(
                    defaults,
                    "tile.picker.key",
                    2,
                )),
                crate::tile_hotkey::modifier_index(crate::tile_manager::pref(
                    defaults,
                    "tile.picker.mods",
                )),
                crate::tile_hotkey::Action::Layouts,
            );
            if self.tile_hotkey.is_none() {
                self.feedback("Atajo ocupado: elige otra combinación en Tiling.");
            }
            self.refresh_picker_hint();
            self.update_tile_timer();
            self.tile_manager.invalidate();
        }
    }
    fn update_tile_timer(&mut self) {
        let interval = self.tile_manager.tick_interval();
        if self.tile_timer.is_some() && self.tile_interval == interval {
            return;
        }
        unsafe {
            if let Some(timer) = self.tile_timer.take() {
                let _: () = msg_send![&timer, invalidate];
            }
            let timer: Retained<AnyObject> = msg_send![class!(NSTimer),timerWithTimeInterval:interval,target:&*self.target,selector:sel!(tileTick:),userInfo:ptr::null::<AnyObject>(),repeats:true];
            let _: () = msg_send![&timer,setTolerance:if interval>0.02 {0.01f64}else{0.001f64}];
            let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
            add_common_timer(run, &timer);
            self.tile_timer = Some(timer);
            self.tile_interval = interval;
        }
    }
    fn refresh_tile_controls(&mut self) {
        self.tile_space = self
            .tile_manager
            .current_workspace()
            .map(|key| key.1)
            .or_else(crate::tile_manager::current_space);
        if self.tile_controls.len() != 4 {
            return;
        }
        unsafe {
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            let title = if let Some(id) = self.tile_space {
                let snapshot = spaces::snapshot();
                format!(
                    "Escritorio {} · distribución independiente",
                    snapshot.desktops.iter().position(|s| *s == id).unwrap_or(0) + 1
                )
            } else {
                "Pantalla completa · selecciona un escritorio".into()
            };
            text(self.tile_controls[0], &title);
            let enabled = self.tile_space.is_some();
            let _: () = msg_send![self.tile_controls[1],setEnabled:enabled];
            let _: () = msg_send![self.tile_controls[2],setEnabled:enabled];
            if let Some(context) = self.tile_manager.picker_context() {
                for layout in [
                    crate::tiling::Layout::ThreeBands,
                    crate::tiling::Layout::FourBands,
                ] {
                    let item: Obj = msg_send![self.tile_controls[2],itemAtIndex:layout.index()];
                    let _: () = msg_send![item,setTitle:&*NSString::from_str(layout.screen_name(context.area))];
                }
            }
            if let Some(id) = self.tile_space {
                let _: () = msg_send![self.tile_controls[1],setState:1-crate::tile_manager::pref(defaults,&format!("tile.off.{id}")).clamp(0,1)];
                let _: () = msg_send![self.tile_controls[2],selectItemAtIndex:self.tile_manager.current_layout(defaults).index()];
            }
            text(
                self.tile_controls[3],
                if self.tile_hotkey.is_none()
                    && crate::tile_manager::pref(defaults, "tile.enabled") != 0
                {
                    "Atajo no disponible."
                } else {
                    "Atajo: flotante ↔ tile."
                },
            );
        }
    }
    unsafe fn build_tile_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            card(root, rect(0., 282., 432., 78.));
            label(
                root,
                rect(18., 326., 340., 22.),
                "Gestor de ventanas en mosaico",
                14.,
                0.3,
                false,
            );
            toggle(
                root,
                374.,
                326.,
                "Activar tiling",
                target,
                40,
                crate::tile_manager::pref(defaults, "tile.enabled") != 0,
            );
            label(
                root,
                rect(18., 299., 395., 20.),
                "Al desactivarlo, recuperas los tamaños y posiciones previos.",
                11.,
                0.,
                true,
            );
            let title = label(root, rect(18., 249., 395., 22.), "", 12., 0.3, false);
            label(
                root,
                rect(18., 215., 310., 22.),
                "Usar mosaico en este escritorio",
                13.,
                0.,
                false,
            );
            let enabled = toggle(
                root,
                374.,
                215.,
                "Mosaico en este escritorio",
                target,
                41,
                true,
            );
            label(
                root,
                rect(18., 179., 130., 22.),
                "Plantilla",
                13.,
                0.,
                false,
            );
            let layout = popup(
                root,
                rect(160., 173., 254., 30.),
                &[
                    "Dwindle · divisiones",
                    "Espiral",
                    "Cuadrícula simétrica",
                    "Principal y secundarias",
                    "3 divisiones · según pantalla",
                    "4 divisiones · según pantalla",
                ],
                target,
                42,
            );
            label(
                root,
                rect(18., 141., 340., 22.),
                "Colocación inteligente con ratón",
                13.,
                0.,
                false,
            );
            toggle(
                root,
                374.,
                141.,
                "Colocación inteligente",
                target,
                43,
                crate::tile_manager::pref(defaults, "tile.simple") == 0,
            );
            label(
                root,
                rect(18., 115., 400., 18.),
                "Arrastra el título. Bordes: divide; centro: intercambia.",
                11.,
                0.,
                true,
            );
            button(
                root,
                rect(15., 88., 196., 28.),
                "Rejilla y distribución…",
                target,
                49,
            );
            label(
                root,
                rect(18., 59., 150., 22.),
                "Flotante ↔ tile",
                13.,
                0.3,
                false,
            );
            let mods = popup(
                root,
                rect(178., 53., 120., 30.),
                &crate::tile_hotkey::MODIFIERS
                    .iter()
                    .map(|(s, _)| *s)
                    .collect::<Vec<_>>(),
                target,
                44,
            );
            let keys = popup(
                root,
                rect(304., 53., 110., 30.),
                &crate::tile_hotkey::KEYS
                    .iter()
                    .map(|(s, _)| *s)
                    .collect::<Vec<_>>(),
                target,
                45,
            );
            let _: () = msg_send![mods,selectItemAtIndex:crate::tile_hotkey::modifier_index(crate::tile_manager::pref(defaults,"tile.mods")) as isize];
            let _: () = msg_send![keys,selectItemAtIndex:crate::tile_hotkey::key_index(crate::tile_manager::pref(defaults,"tile.key")) as isize];
            button(
                root,
                rect(217., 88., 198., 28.),
                "Plantillas y atajo…",
                target,
                54,
            );
            let hint = label(root, rect(18., 17., 225., 24.), "", 11., 0., true);
            button(
                root,
                rect(251., 12., 165., 30.),
                "Apps flotantes…",
                target,
                46,
            );
            self.tile_controls = vec![title, enabled, layout, hint];
            self.refresh_tile_controls();
        }
    }
    fn refresh_picker_hint(&self) {
        if self.picker_hint.is_null() {
            return;
        }
        unsafe {
            text(
                self.picker_hint,
                if self.picker_hotkey.is_some() {
                    "Atajo listo · abre el selector en la pantalla del cursor."
                } else {
                    "Atajo no disponible: revisa tiling, Accesibilidad o la combinación."
                },
            );
        }
    }
    fn show_picker_settings(&mut self) {
        unsafe {
            if self.picker_settings.is_none() {
                let panel = window(rect(0., 0., 480., 340.), 1 | 2 | 4);
                let _: () =
                    msg_send![&panel,setTitle:&*NSString::from_str("Selector de distribuciones")];
                let _: () = msg_send![&panel, center];
                let root: Obj = msg_send![&panel, contentView];
                let target = Retained::as_ptr(&self.target) as Obj;
                let defaults = Retained::as_ptr(&self.defaults) as Obj;
                label(
                    root,
                    rect(20., 298., 440., 24.),
                    "Plantillas en la pantalla del cursor",
                    16.,
                    0.3,
                    false,
                );
                label(
                    root,
                    rect(20., 271., 440., 22.),
                    "Previsualiza con el ratón o las flechas. Intro aplica; Esc cancela.",
                    11.,
                    0.,
                    true,
                );
                label(
                    root,
                    rect(20., 227., 160., 24.),
                    "Abrir selector",
                    13.,
                    0.,
                    false,
                );
                let mods = popup(
                    root,
                    rect(200., 224., 125., 30.),
                    &crate::tile_hotkey::MODIFIERS
                        .iter()
                        .map(|v| v.0)
                        .collect::<Vec<_>>(),
                    target,
                    55,
                );
                let keys = popup(
                    root,
                    rect(330., 224., 130., 30.),
                    &crate::tile_hotkey::KEYS
                        .iter()
                        .map(|v| v.0)
                        .collect::<Vec<_>>(),
                    target,
                    56,
                );
                let _: () = msg_send![mods,selectItemAtIndex:crate::tile_hotkey::modifier_index(crate::tile_manager::pref(defaults,"tile.picker.mods")) as isize];
                let _: () = msg_send![keys,selectItemAtIndex:crate::tile_hotkey::key_index(crate::tile_manager::pref_or(defaults,"tile.picker.key",2)) as isize];
                self.picker_mods = mods;
                self.picker_keys = keys;
                button(
                    root,
                    rect(18., 142., 230., 30.),
                    "Buscar atajos libres",
                    target,
                    58,
                );
                self.picker_suggestions = popup(
                    root,
                    rect(20., 100., 290., 30.),
                    &["Busca sugerencias en este Mac"],
                    target,
                    59,
                );
                let _: () = msg_send![self.picker_suggestions,setEnabled:false];
                self.picker_apply = button(
                    root,
                    rect(320., 100., 140., 30.),
                    "Usar sugerido",
                    target,
                    60,
                );
                let _: () = msg_send![self.picker_apply,setEnabled:false];
                label(
                    root,
                    rect(20., 56., 440., 36.),
                    "Se comprueban reservas globales y atajos del sistema.
Otras apps pueden tener atajos propios no detectables.",
                    11.,
                    0.,
                    true,
                );
                self.picker_hint = label(root, rect(20., 186., 440., 30.), "", 11., 0., true);
                button(
                    root,
                    rect(270., 20., 190., 32.),
                    "Mostrar plantillas",
                    target,
                    57,
                );
                self.picker_settings = Some(panel);
            }
            self.refresh_picker_hint();
            let _: () = msg_send![self.picker_settings.as_ref().unwrap(),makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn find_picker_shortcuts(&mut self) {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let excluded = [(
            crate::tile_hotkey::key_index(crate::tile_manager::pref(defaults, "tile.key")),
            crate::tile_hotkey::modifier_index(crate::tile_manager::pref(defaults, "tile.mods")),
        )];
        let suggestions = crate::tile_hotkey::suggestions(&excluded);
        self.suggested_shortcuts = suggestions.clone().unwrap_or_default();
        unsafe {
            let _: () = msg_send![self.picker_suggestions, removeAllItems];
            for &(key, mods) in &self.suggested_shortcuts {
                let _: () = msg_send![self.picker_suggestions,addItemWithTitle:&*NSString::from_str(&crate::tile_hotkey::title(key,mods))];
            }
            let _: () =
                msg_send![self.picker_suggestions,setEnabled:!self.suggested_shortcuts.is_empty()];
            let _: () =
                msg_send![self.picker_apply,setEnabled:!self.suggested_shortcuts.is_empty()];
            text(
                self.picker_hint,
                if suggestions.is_none() {
                    "No se pudieron consultar los atajos del sistema."
                } else if self.suggested_shortcuts.is_empty() {
                    "No se encontraron combinaciones libres. Prueba otra tecla."
                } else {
                    "Sugerencias comprobadas en este Mac. Elige una y pulsa Usar sugerido."
                },
            );
        }
    }
    fn toggle_picker(&mut self) {
        if self.layout_picker.take().is_some() {
            return;
        }
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        if self.sleeping
            || self.switching.is_some()
            || !ax::trusted()
            || crate::tile_manager::pref(defaults, "tile.enabled") == 0
        {
            self.feedback("Activa tiling y Accesibilidad para usar las plantillas.");
            self.refresh_picker_hint();
            return;
        }
        self.pending_focus = None;
        self.tile_manager.refresh_for_picker(defaults);
        let Some(context) = self.tile_manager.picker_context() else {
            self.feedback("Activa el mosaico en el escritorio de esta pantalla.");
            return;
        };
        self.layout_picker =
            crate::tile_picker::Picker::new(context, Retained::as_ptr(&self.target) as Obj);
        let Some(picker) = &self.layout_picker else {
            return;
        };
        picker.show();
        self.update_tile_timer();
    }
    fn commit_picker(&mut self) {
        let Some(picker) = self.layout_picker.take() else {
            return;
        };
        let Some(layout) = picker.selected_layout() else {
            self.layout_picker = Some(picker);
            return;
        };
        let context = picker.context.clone();
        drop(picker);
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        self.tile_manager.refresh_for_picker(defaults);
        if self.tile_manager.apply_template(defaults, &context, layout) {
            self.tile_manager.tick(defaults);
        } else {
            self.feedback("La pantalla o sus ventanas han cambiado. Abre de nuevo el selector.");
        }
        self.hover_resume = Instant::now() + Duration::from_millis(300);
        self.update_tile_timer();
        self.refresh_tile_controls();
    }
    fn show_tile_grid(&mut self) {
        unsafe {
            if self.tile_grid_window.is_none() {
                let w = window(rect(0., 0., 480., 350.), 1 | 2 | 4);
                let _: () = msg_send![&w,setTitle:&*NSString::from_str("Rejilla y distribución")];
                let _: () = msg_send![&w, center];
                let root: Obj = msg_send![&w, contentView];
                let target = Retained::as_ptr(&self.target) as Obj;
                let defaults = Retained::as_ptr(&self.defaults) as Obj;
                label(
                    root,
                    rect(22., 302., 370., 24.),
                    "Ajustar tamaños a una rejilla virtual",
                    16.,
                    0.3,
                    false,
                );
                self.tile_grid_toggle = toggle(
                    root,
                    419.,
                    302.,
                    "Activar rejilla virtual",
                    target,
                    50,
                    crate::tile_manager::pref(defaults, "tile.grid.enabled") != 0,
                );
                label(
                    root,
                    rect(22., 270., 435., 20.),
                    "Las divisiones se alinean sin dejar huecos entre tiles.",
                    12.,
                    0.,
                    true,
                );
                for (i, (name, key, fallback, max)) in [
                    ("Columnas (2–240)", "tile.grid.columns", 120, 240),
                    ("Filas (2–60)", "tile.grid.rows", 6, 60),
                ]
                .iter()
                .enumerate()
                {
                    let x = 22. + i as f64 * 220.;
                    label(root, rect(x, 231., 210., 20.), name, 12., 0., false);
                    let field = alloc_view(class!(NSTextField), rect(x, 200., 190., 26.));
                    let _: () = msg_send![&field,setStringValue:&*NSString::from_str(&crate::tile_manager::grid_count(defaults,key,*fallback,*max).to_string())];
                    let _: () = msg_send![&field,setAccessibilityLabel:&*NSString::from_str(name)];
                    let _: () = msg_send![root,addSubview:&*field];
                    self.tile_grid_fields.push(Retained::as_ptr(&field) as Obj);
                }
                button(
                    root,
                    rect(326., 158., 132., 30.),
                    "Aplicar rejilla",
                    target,
                    51,
                );
                label(
                    root,
                    rect(22., 122., 435., 20.),
                    "Una distribución por escritorio · mismo número de tiles.",
                    12.,
                    0.,
                    true,
                );
                button(
                    root,
                    rect(18., 80., 210., 30.),
                    "Guardar distribución",
                    target,
                    52,
                );
                button(
                    root,
                    rect(242., 80., 216., 30.),
                    "Restaurar distribución",
                    target,
                    53,
                );
                self.tile_grid_status = label(
                    root,
                    rect(22., 20., 435., 45.),
                    "Se guardan proporciones y rejilla, no se abren aplicaciones.",
                    11.,
                    0.,
                    true,
                );
                self.tile_grid_window = Some(w);
            }
            let _: () = msg_send![self.tile_grid_window.as_ref().unwrap(),makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn show_tile_rules(&mut self) {
        unsafe {
            if self.tile_rules_window.is_none() {
                let w = window(rect(0., 0., 480., 430.), 1 | 2 | 4);
                let _: () = msg_send![&w,setTitle:&*NSString::from_str("Aplicaciones flotantes")];
                let _: () = msg_send![&w, center];
                let root: Obj = msg_send![&w, contentView];
                let target = Retained::as_ptr(&self.target) as Obj;
                label(
                    root,
                    rect(22., 385., 435., 24.),
                    "Abrir como flotantes",
                    18.,
                    0.3,
                    false,
                );
                label(
                    root,
                    rect(22., 356., 435., 20.),
                    "Se aplica a las nuevas ventanas de estas aplicaciones.",
                    12.,
                    0.,
                    true,
                );
                label(
                    root,
                    rect(22., 332., 435., 20.),
                    "Puedes devolver cada ventana al tile con tu atajo.",
                    12.,
                    0.,
                    true,
                );
                let scroll = alloc_view(class!(NSScrollView), rect(18., 65., 444., 250.));
                let _: () = msg_send![&scroll,setHasVerticalScroller:true];
                let _: () = msg_send![&scroll,setAutohidesScrollers:true];
                let _: () = msg_send![&scroll,setDrawsBackground:false];
                let _: () = msg_send![root,addSubview:&*scroll];
                self.tile_rules_scroll = Retained::as_ptr(&scroll) as Obj;
                button(
                    root,
                    rect(18., 18., 190., 30.),
                    "Añadir aplicación…",
                    target,
                    47,
                );
                button(root, rect(343., 18., 119., 30.), "Cerrar", target, 48);
                self.tile_rules_window = Some(w);
            }
            self.rebuild_tile_rules();
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app,activateIgnoringOtherApps:true];
            let _: () = msg_send![self.tile_rules_window.as_ref().unwrap(),makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn rebuild_tile_rules(&mut self) {
        if self.tile_rules_scroll.is_null() {
            return;
        }
        unsafe {
            let entries = crate::tile_rules::entries(Retained::as_ptr(&self.defaults) as Obj);
            self.tile_rule_ids = entries.iter().map(|(id, _)| id.clone()).collect();
            let height = (entries.len() as f64 * 60.).max(250.);
            let document = alloc_view(class!(NSView), rect(0., 0., 426., height));
            let root = Retained::as_ptr(&document) as Obj;
            if entries.is_empty() {
                label(
                    root,
                    rect(12., height - 45., 400., 24.),
                    "Ninguna aplicación: las ventanas usarán mosaico.",
                    12.,
                    0.,
                    true,
                );
            }
            for (i, (id, name)) in entries.iter().enumerate() {
                let y = height - (i + 1) as f64 * 60.;
                label(root, rect(12., y + 28., 298., 22.), name, 13., 0.3, false);
                label(root, rect(12., y + 8., 298., 18.), id, 10., 0., true);
                let remove = button(
                    root,
                    rect(326., y + 15., 88., 28.),
                    "Quitar",
                    Retained::as_ptr(&self.target) as Obj,
                    2000 + i as isize,
                );
                let _: () = msg_send![remove,setAccessibilityLabel:&*NSString::from_str(&format!("Quitar {name}"))];
            }
            let _: () = msg_send![self.tile_rules_scroll,setDocumentView:&*document];
        }
    }
    fn add_tile_rule(&mut self) {
        unsafe {
            let panel: Obj = msg_send![class!(NSOpenPanel), openPanel];
            let _: () = msg_send![panel,setTitle:&*NSString::from_str("Selecciona una aplicación")];
            let _: () = msg_send![panel,setCanChooseFiles:true];
            let _: () = msg_send![panel,setCanChooseDirectories:false];
            let _: () = msg_send![panel,setAllowsMultipleSelection:false];
            let _: () = msg_send![panel,setTreatsFilePackagesAsDirectories:false];
            let types: Obj = msg_send![class!(NSArray),arrayWithObject:&*NSString::from_str("app")];
            let _: () = msg_send![panel,setAllowedFileTypes:types];
            let directory: Obj =
                msg_send![class!(NSURL),fileURLWithPath:&*NSString::from_str("/Applications")];
            let _: () = msg_send![panel,setDirectoryURL:directory];
            let result: isize = msg_send![panel, runModal];
            if result == 1 {
                let url: Obj = msg_send![panel, URL];
                if let Some((id, name)) = crate::tile_rules::bundle(url) {
                    crate::tile_rules::set(
                        Retained::as_ptr(&self.defaults) as Obj,
                        &id,
                        Some(&name),
                    );
                    self.rebuild_tile_rules();
                } else {
                    self.feedback("La aplicación seleccionada no tiene identificador de bundle.");
                }
            }
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
                        "○ Borde, foco y tiling requieren Accesibilidad"
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
                40..=45 => {
                    let defaults = Retained::as_ptr(&self.defaults) as Obj;
                    let selected: isize = if tag == 40 || tag == 41 || tag == 43 {
                        msg_send![sender, state]
                    } else {
                        msg_send![sender, indexOfSelectedItem]
                    };
                    match tag {
                        40 => {
                            crate::tile_manager::save(defaults, "tile.enabled", selected);
                            if selected == 0 {
                                self.tile_manager.restore();
                            }
                            self.configure_tiling();
                        }
                        41 => {
                            if let Some(id) = self.tile_space {
                                crate::tile_manager::save(
                                    defaults,
                                    &format!("tile.off.{id}"),
                                    1 - selected,
                                );
                                if selected == 0 {
                                    self.tile_manager.restore_space(id);
                                }
                            }
                        }
                        42 => {
                            self.tile_manager.reset_template(
                                defaults,
                                crate::tiling::Layout::from_index(selected),
                            );
                        }
                        43 => crate::tile_manager::save(defaults, "tile.simple", 1 - selected),
                        44 | 45 => {
                            crate::tile_manager::save(
                                defaults,
                                if tag == 44 { "tile.mods" } else { "tile.key" },
                                selected,
                            );
                            self.configure_tiling();
                        }
                        _ => {}
                    }
                    self.tile_manager.invalidate();
                    self.refresh_tile_controls();
                }
                54 => self.show_picker_settings(),
                55 | 56 => {
                    let selected: isize = msg_send![sender, indexOfSelectedItem];
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        if tag == 55 {
                            "tile.picker.mods"
                        } else {
                            "tile.picker.key"
                        },
                        selected,
                    );
                    self.configure_tiling();
                    self.refresh_picker_hint();
                }
                58 => self.find_picker_shortcuts(),
                59 => {}
                60 => {
                    let index: isize = msg_send![self.picker_suggestions, indexOfSelectedItem];
                    if let Some(&(key, mods)) = self.suggested_shortcuts.get(index.max(0) as usize)
                    {
                        let defaults = Retained::as_ptr(&self.defaults) as Obj;
                        crate::tile_manager::save(defaults, "tile.picker.key", key as isize);
                        crate::tile_manager::save(defaults, "tile.picker.mods", mods as isize);
                        let _: () = msg_send![self.picker_keys,selectItemAtIndex:key as isize];
                        let _: () = msg_send![self.picker_mods,selectItemAtIndex:mods as isize];
                        self.configure_tiling(); // Reserve again: another app may have taken it since searching.
                    }
                }
                57 => self.toggle_picker(),
                49 => self.show_tile_grid(),
                50 => {
                    let state: isize = msg_send![sender, state];
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        "tile.grid.enabled",
                        state,
                    );
                    self.tile_manager.invalidate();
                }
                51 => {
                    let columns: isize = msg_send![self.tile_grid_fields[0], integerValue];
                    let rows: isize = msg_send![self.tile_grid_fields[1], integerValue];
                    if !(2..=240).contains(&columns) || !(2..=60).contains(&rows) {
                        text(
                            self.tile_grid_status,
                            "Introduce columnas entre 2 y 240 y filas entre 2 y 60.",
                        );
                    } else {
                        for (key, value) in
                            [("tile.grid.columns", columns), ("tile.grid.rows", rows)]
                        {
                            crate::tile_manager::save(
                                Retained::as_ptr(&self.defaults) as Obj,
                                key,
                                value,
                            );
                        }
                        self.tile_manager.invalidate();
                        text(
                            self.tile_grid_status,
                            "Rejilla actualizada. Activa la casilla para aplicarla.",
                        );
                    }
                }
                52 => {
                    let ok = self
                        .tile_manager
                        .save_distribution(Retained::as_ptr(&self.defaults) as Obj);
                    text(
                        self.tile_grid_status,
                        if ok {
                            "Distribución guardada para este escritorio."
                        } else {
                            "Activa tiling y abre ventanas en este escritorio primero."
                        },
                    );
                }
                53 => {
                    let ok = self
                        .tile_manager
                        .restore_distribution(Retained::as_ptr(&self.defaults) as Obj);
                    self.refresh_tile_controls();
                    let defaults = Retained::as_ptr(&self.defaults) as Obj;
                    let _: () = msg_send![self.tile_grid_toggle,setState:crate::tile_manager::pref(defaults,"tile.grid.enabled")];
                    for (field, key, fallback, max) in [
                        (self.tile_grid_fields[0], "tile.grid.columns", 120, 240),
                        (self.tile_grid_fields[1], "tile.grid.rows", 6, 60),
                    ] {
                        text(
                            field,
                            &crate::tile_manager::grid_count(defaults, key, fallback, max)
                                .to_string(),
                        );
                    }
                    text(
                        self.tile_grid_status,
                        if ok {
                            "Distribución restaurada."
                        } else {
                            "No hay una distribución compatible con este número de tiles."
                        },
                    );
                }
                46 => self.show_tile_rules(),
                47 => self.add_tile_rule(),
                48 => {
                    if let Some(w) = &self.tile_rules_window {
                        let _: () = msg_send![&**w,orderOut:ptr::null::<AnyObject>()];
                    }
                }
                2000.. => {
                    if let Some(id) = self.tile_rule_ids.get((tag - 2000) as usize) {
                        crate::tile_rules::set(Retained::as_ptr(&self.defaults) as Obj, id, None);
                        self.rebuild_tile_rules();
                    }
                }
                1000..=1999 => {
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
fn desktop_button_class() -> &'static AnyClass {
    if let Some(class) = AnyClass::get(c"AMSpaceDesktopButton") {
        return class;
    }
    unsafe {
        let mut builder = ClassBuilder::new(c"AMSpaceDesktopButton", class!(NSButton)).unwrap();
        // Let the real NSStatusBarButton own mouse tracking across Spaces.
        // Children keep their individual AXPress actions, but only draw for
        // physical input: nested NSButton cells can retain tracking state.
        builder.add_method(
            sel!(hitTest:),
            desktop_hit_test as extern "C-unwind" fn(Obj, Sel, NSPoint) -> Obj,
        );
        builder.register()
    }
}
extern "C-unwind" fn desktop_hit_test(_: Obj, _: Sel, _: NSPoint) -> Obj {
    ptr::null_mut()
}
extern "C-unwind" fn status_click(target: Obj, _: Sel, sender: Obj) {
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let event: Obj = msg_send![app, currentEvent];
        let kind: usize = msg_send![event, type];
        let flags: usize = msg_send![event, modifierFlags];
        if kind == 4 || flags & (1 << 18) != 0 {
            settings(target, sel!(settings:), sender);
            return;
        }
        if kind != 2 {
            // Keyboard/VoiceOver activation has no reliable pointer position.
            desktop_menu(target, sel!(desktopMenu:), sender);
            return;
        }
        // The menu-bar host may forward a click at the item's centre.
        // Use the actual pointer location to identify its desktop segment.
        let cursor: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let window: Obj = msg_send![sender, window];
        let location: NSPoint = msg_send![window, convertPointFromScreen:cursor];
        let point: NSPoint =
            msg_send![sender, convertPoint:location, fromView:ptr::null::<AnyObject>()];
        let views: Obj = msg_send![sender, subviews];
        let count: usize = msg_send![views, count];
        if std::env::var_os("AMSPACE_TRACE_SPACES").is_some() {
            eprintln!("spaces: status pointer x={} views={count}", point.x);
        }
        for index in 0..count {
            let view: Obj = msg_send![views, objectAtIndex:index];
            let desktop: bool = msg_send![view, isKindOfClass:desktop_button_class()];
            if !desktop {
                continue;
            }
            let frame: NSRect = msg_send![view, frame];
            if point.x < frame.origin.x || point.x >= frame.origin.x + frame.size.width {
                continue;
            }
            let id: isize = msg_send![view, tag];
            if std::env::var_os("AMSPACE_TRACE_SPACES").is_some() {
                eprintln!(
                    "spaces: status mouse-up x={} cursor={},{} id={id}",
                    point.x, cursor.x, cursor.y
                );
            }
            if id > 0 {
                schedule_selection(target, id);
            } else {
                desktop_menu(target, sel!(desktopMenu:), sender);
            }
            return;
        }
    }
}
fn schedule_selection(target: Obj, id: isize) {
    unsafe {
        let value: Obj = msg_send![class!(NSNumber), numberWithInteger:id];
        let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.01f64, target:target, selector:sel!(selectDesktopRequest:), userInfo:value, repeats:false];
        let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
        add_common_timer(run, &timer);
    }
}
extern "C-unwind" fn select_desktop_request(target: Obj, _: Sel, timer: Obj) {
    let id: isize = unsafe {
        let value: Obj = msg_send![timer, userInfo];
        msg_send![value, integerValue]
    };
    let delivered = APP.with(|cell| {
        let Ok(mut app) = cell.try_borrow_mut() else {
            return false;
        };
        if let Some(app) = app.as_mut() {
            app.select_desktop(id as u64);
        }
        true
    });
    if !delivered {
        schedule_selection(target, id);
    }
}
extern "C-unwind" fn select_desktop(target: Obj, _: Sel, sender: Obj) {
    let id: isize = unsafe { msg_send![sender, tag] };
    unsafe {
        // AXPress on a submenu item can leave AppKit tracking the parent menu.
        // End tracking before the deferred desktop switch is sent.
        let is_menu_item: bool = msg_send![sender, isKindOfClass:class!(NSMenuItem)];
        if is_menu_item {
            let mut menu: Obj = msg_send![sender, menu];
            while !menu.is_null() {
                let _: () = msg_send![menu, cancelTracking];
                menu = msg_send![menu, supermenu];
            }
        }
    }
    if id > 0 {
        schedule_selection(target, id);
    }
}
fn schedule_switch_timer(target: Obj) {
    unsafe {
        let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.05f64, target:target, selector:sel!(switchStep:), userInfo:ptr::null::<AnyObject>(), repeats:false];
        let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
        add_common_timer(run, &timer);
    }
}
extern "C-unwind" fn switch_step(target: Obj, _: Sel, _: Obj) {
    let delivered = APP.with(|cell| {
        let Ok(mut app) = cell.try_borrow_mut() else {
            return false;
        };
        if let Some(app) = app.as_mut() {
            app.step_switch();
        }
        true
    });
    if !delivered {
        schedule_switch_timer(target);
    }
}
extern "C-unwind" fn desktop_menu(target: Obj, _: Sel, _: Obj) {
    unsafe {
        let _: () = msg_send![target, performSelector:sel!(showDesktopMenu:), withObject:ptr::null::<AnyObject>(), afterDelay:0f64];
    }
}
extern "C-unwind" fn show_desktop_menu(_: Obj, _: Sel, _: Obj) {
    let mut menu = None;
    let mut parent: Obj = ptr::null_mut();
    with_app(|a| {
        menu = Some(a.desktop_menu());
        parent = unsafe { msg_send![&a.status, button] };
    });
    if let Some(menu) = menu {
        unsafe {
            let _: bool = msg_send![&menu, popUpMenuPositioningItem:ptr::null::<AnyObject>(), atLocation:NSPoint::new(0., 0.), inView:parent];
        }
    }
}
extern "C-unwind" fn settings(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.show_settings());
}
extern "C-unwind" fn desktop_menu_update(target: Obj, _: Sel, menu: Obj) {
    unsafe {
        let _: () = msg_send![menu, removeAllItems];
        let snapshot = spaces::snapshot();
        for (index, id) in snapshot.desktops.iter().enumerate() {
            let item: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
            let item: Retained<AnyObject> = msg_send![item, initWithTitle:&*NSString::from_str(&format!("Escritorio {}", index + 1)), action:sel!(selectDesktop:), keyEquivalent:&*NSString::from_str("")];
            let _: () = msg_send![&item, setTarget:target];
            let _: () = msg_send![&item, setTag:*id as isize];
            let _: () =
                msg_send![&item, setState:if snapshot.active == Some(index) {1isize} else {0isize}];
            let _: () = msg_send![menu, addItem:&*item];
        }
    }
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
pub fn toggle_layout_picker() {
    with_app(|a| a.toggle_picker());
}
pub fn cancel_layout_picker() {
    with_app(|a| {
        a.layout_picker = None;
        a.hover_resume = Instant::now() + Duration::from_millis(300);
    });
}
pub fn layout_picker_hover(index: usize) {
    with_app(|a| {
        if let Some(picker) = &mut a.layout_picker {
            picker.select(index);
        }
    });
}
pub fn layout_picker_key(key: u16, shift: bool) {
    with_app(|a| {
        let Some(picker) = &mut a.layout_picker else {
            return;
        };
        match crate::tile_picker::key_input(key, picker.columns, shift) {
            Some(crate::tile_picker::Input::Select(index)) => picker.select(index),
            Some(crate::tile_picker::Input::Move(offset)) => picker.move_selection(offset),
            Some(crate::tile_picker::Input::Confirm) => a.commit_picker(),
            Some(crate::tile_picker::Input::Cancel) => a.layout_picker = None,
            None => {}
        }
    });
}
extern "C-unwind" fn layout_choice(_: Obj, _: Sel, sender: Obj) {
    let index: isize = unsafe { msg_send![sender, tag] };
    with_app(|a| {
        if index < 0 {
            a.layout_picker = None;
        } else if let Some(picker) = &mut a.layout_picker {
            picker.select(index as usize);
            a.commit_picker();
        }
    });
}
pub fn toggle_tile_float() {
    with_app(|a| {
        if a.layout_picker.is_some() {
            return;
        }
        a.tile_manager
            .toggle_float(Retained::as_ptr(&a.defaults) as Obj);
        a.tile_manager.tick(Retained::as_ptr(&a.defaults) as Obj);
        a.update_tile_timer();
        a.update_window();
    });
}
extern "C-unwind" fn tile_tick(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| {
        with_app(|a| {
            if !ax::trusted() {
                a.configure_tiling();
                return;
            }
            if a.layout_picker.is_some() {
                return;
            }
            if a.switching.is_some() {
                a.tile_manager.cancel_drag();
                a.update_tile_timer();
                return;
            }
            a.tile_manager.tick(Retained::as_ptr(&a.defaults) as Obj);
            a.update_tile_timer();
            if let Some(error) = a.tile_manager.last_error.take() {
                a.feedback(&error);
            }
        })
    });
}
extern "C-unwind" fn hover(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| with_app(|a| a.hover()));
}
extern "C-unwind" fn space_changed(target: Obj, selector: Sel, notification: Obj) {
    with_app(|a| {
        a.layout_picker = None;
        a.tile_manager.space_changed();
    });
    workspace(target, selector, notification);
}
extern "C-unwind" fn screens_changed(target: Obj, selector: Sel, notification: Obj) {
    cancel_layout_picker();
    with_app(|a| a.tile_manager.screens_changed());
    workspace(target, selector, notification);
}
extern "C-unwind" fn workspace(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.tile_manager.invalidate();
        a.refresh_tile_controls();
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
        a.configure_tiling();
        a.update_window();
    });
}
extern "C-unwind" fn sleep(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.sleeping = true;
        a.configure_hover();
        a.configure_tiling();
        a.update_window();
    });
}
extern "C-unwind" fn wake(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.sleeping = false;
        a.configure_hover();
        a.configure_tiling();
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
            ("Ir al escritorio…", sel!(desktopMenu:), ""),
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
            if action == sel!(desktopMenu:) {
                let desktops: Retained<AnyObject> = msg_send![class!(NSMenu), new];
                let _: () = msg_send![&desktops, setDelegate:target];
                desktop_menu_update(
                    target,
                    sel!(menuNeedsUpdate:),
                    Retained::as_ptr(&desktops) as Obj,
                );
                let _: () = msg_send![&command, setSubmenu:&*desktops];
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
            (sel!(selectDesktop:), select_desktop),
            (sel!(selectDesktopRequest:), select_desktop_request),
            (sel!(statusClick:), status_click),
            (sel!(menuNeedsUpdate:), desktop_menu_update),
            (sel!(desktopMenu:), desktop_menu),
            (sel!(showDesktopMenu:), show_desktop_menu),
            (sel!(switchStep:), switch_step),
            (sel!(about:), about),
            (sel!(checkUpdates:), check_updates),
            (sel!(change:), change),
            (sel!(hover:), hover),
            (sel!(tileTick:), tile_tick),
            (sel!(layoutChoice:), layout_choice),
            (sel!(screensChanged:), screens_changed),
            (sel!(focusPending:), focus_pending),
            (sel!(update:), update),
            (sel!(settle:), settle),
            (sel!(workspace:), workspace),
            (sel!(spaceChanged:), space_changed),
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
                if name == "NSWorkspaceActiveSpaceDidChangeNotification" {
                    sel!(spaceChanged:)
                } else {
                    sel!(workspace:)
                },
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
            sel!(screensChanged:),
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
