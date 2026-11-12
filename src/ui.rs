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
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::{
    cell::RefCell,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {}
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}
thread_local! { static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }
static SEND_QUEUED: AtomicU64 = AtomicU64::new(0);
static SEND_DELIVERED: AtomicU64 = AtomicU64::new(0);
static SEND_EXPIRED: AtomicU64 = AtomicU64::new(0);
static SEND_RETRIED: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, PartialEq)]
enum ShortcutDelivery {
    Done,
    Busy,
    Expired,
    Changed,
    Inactive,
}
fn deliver_shortcut<T>(
    state: &RefCell<Option<T>>,
    queued: Instant,
    generation: u64,
    current: u64,
    action: crate::tile_hotkey::Action,
    run: impl FnOnce(&mut T, crate::tile_hotkey::Action),
) -> ShortcutDelivery {
    if generation != current {
        return ShortcutDelivery::Changed;
    }
    if queued.elapsed() > Duration::from_millis(350) {
        return ShortcutDelivery::Expired;
    }
    let Ok(mut state) = state.try_borrow_mut() else {
        return ShortcutDelivery::Busy;
    };
    let Some(app) = state.as_mut() else {
        return ShortcutDelivery::Inactive;
    };
    run(app, action);
    ShortcutDelivery::Done
}
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
unsafe fn apply_border_clip(layer: Obj, target: NSRect, regions: &[NSRect]) {
    unsafe extern "C" {
        fn CGPathCreateMutable() -> *mut std::ffi::c_void;
        fn CGPathAddRect(
            path: *mut std::ffi::c_void,
            transform: *const std::ffi::c_void,
            rect: NSRect,
        );
        fn CGPathRelease(path: *mut std::ffi::c_void);
    }
    unsafe {
        if regions == [target] {
            let _: () = msg_send![layer, setMask:ptr::null::<AnyObject>()];
            return;
        }
        let path = CGPathCreateMutable();
        if path.is_null() {
            return;
        }
        for r in regions {
            CGPathAddRect(
                path,
                ptr::null(),
                rect(
                    r.origin.x - target.origin.x,
                    target.size.height - (r.origin.y - target.origin.y) - r.size.height,
                    r.size.width,
                    r.size.height,
                ),
            );
        }
        let mask: Obj = msg_send![class!(CAShapeLayer), layer];
        let _: () = msg_send![mask, setFrame:rect(0.,0.,target.size.width,target.size.height)];
        let _: () = msg_send![mask, setPath:path];
        let black = color(0., 0., 0., 1.);
        let cg: *const crate::native::CgColor = msg_send![black, CGColor];
        let _: () = msg_send![mask, setFillColor:cg];
        let _: () = msg_send![layer, setMask:mask];
        CGPathRelease(path);
    }
}
struct PendingFocus {
    front_pid: i32,
    pid: i32,
    id: u32,
    point: NSPoint,
    verify: bool,
    retries: u8,
}
struct App {
    tile_manager: crate::tile_manager::Manager,
    tile_timer: Option<Retained<AnyObject>>,
    tile_mouse: Option<crate::tile_mouse::Monitor>,
    tile_interval: f64,
    shortcuts: crate::shortcuts::Service,
    shortcut_config: Option<(Vec<crate::shortcuts::Binding>, bool)>,
    shortcut_status: Obj,
    shortcut_fields: Vec<(Obj, String, bool, isize)>,
    transfer: Option<crate::window_move::Pending>,
    transfer_history: std::collections::VecDeque<String>,
    transfer_feedback_at: Option<Instant>,
    last_tiling_feedback: String,
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
    tile_choices: Vec<Obj>,
    tile_preview_portrait: Option<bool>,
    tile_grid_window: Option<Retained<AnyObject>>,
    tile_grid_fields: Vec<Obj>,
    tile_grid_status: Obj,
    tile_grid_toggle: Obj,
    custom_name: Obj,
    custom_menu: Obj,
    custom_preview: Obj,
    custom_info: Obj,
    custom_apply: Obj,
    custom_delete: Obj,
    custom_display: Option<u32>,
    custom_templates: Vec<crate::custom_templates::Template>,
    tile_rules_window: Option<Retained<AnyObject>>,
    tile_rules_scroll: Obj,
    tile_rule_ids: Vec<String>,
    target: Retained<AnyObject>,
    status: Retained<AnyObject>,
    status_buttons: Vec<Retained<AnyObject>>,
    status_ids: Vec<u64>,
    desktop_preview: crate::desktop_preview::Preview,
    switching: Option<space_switch::Switch>,
    hover_resume: Instant,
    border: Retained<AnyObject>,
    defaults: Retained<AnyObject>,
    settings: Option<Retained<AnyObject>>,
    settings_timer: Option<Retained<AnyObject>>,
    about: Option<Retained<AnyObject>>,
    updater: Option<Updater>,
    updates_switch: Obj,
    tabs: Obj,
    page_title: Obj,
    bar_colors: Vec<Obj>,
    space_page: Obj,
    space_scroll: Obj,
    space_ids: Vec<u64>,
    space_controls: Vec<Obj>,
    border_enabled: bool,
    flash_enabled: bool,
    flash: crate::border::Flash,
    flash_panel: Option<Retained<AnyObject>>,
    flash_switch: Obj,
    animate_switch: Obj,
    hover_enabled: bool,
    raise_enabled: bool,
    pending_focus: Option<PendingFocus>,
    focus_target: Option<(i32, u32, Instant)>,
    raise_switch: Obj,
    width: f64,
    rgb: [f64; 3],
    observer: Option<ax::Observer>,
    desktop_observer: Option<ax::DesktopObserver>,
    presentation_timer: Option<Retained<AnyObject>>,
    observed_window: Option<ax::Owned>,
    observed_pid: i32,
    tracked: Option<ws::Window>,
    border_suppressed: bool,
    border_space_pause: crate::border::SpacePause,
    border_paint: Option<BorderPaint>,
    border_clip: Option<(u32, NSRect, Vec<NSRect>)>,
    border_retry_pending: bool,
    settle_queued: bool,
    hover_timer: Option<Retained<AnyObject>>,
    last_mouse: NSPoint,
    mouse_since: Instant,
    hover_probe: focus::ProbeBudget,
    sleeping: bool,
    update_queued: bool,
    last_title: String,
    status_detail: String,
    screen_configuration: Vec<ScreenGeometry>,
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
        let _: () = msg_send![&b, setCornerRadius:10f64];
        let c: Obj = msg_send![class!(NSColor), controlBackgroundColor];
        let _: () = msg_send![&b, setFillColor:c];
        let _: () = msg_send![parent, addSubview:&*b];
    }
}
unsafe fn paint_swatch(button: Obj, name: &str, rgb: [f64; 3]) {
    unsafe {
        let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
        let image: Retained<AnyObject> = msg_send![image, initWithSize:NSSize::new(24.,24.)];
        let _: () = msg_send![&image, lockFocus];
        let fill = color(rgb[0], rgb[1], rgb[2], 1.);
        let _: () = msg_send![fill, setFill];
        let circle: Obj =
            msg_send![class!(NSBezierPath), bezierPathWithOvalInRect:rect(3.,3.,18.,18.)];
        let _: () = msg_send![circle, fill];
        let _: () = msg_send![&image, unlockFocus];
        let _: () = msg_send![button, setTitle:&*NSString::from_str("")];
        let _: () = msg_send![button, setImage:&*image];
        let _: () = msg_send![button, setImagePosition:1usize];
        let _: () = msg_send![button, setAccessibilityLabel:&*NSString::from_str(name)];
        let _: () = msg_send![button, setToolTip:&*NSString::from_str(name)];
    }
}
const SETTINGS_SECTIONS: [(&str, &str); 7] = [
    ("Ventanas", "macwindow"),
    ("Barra de menús", "menubar.rectangle"),
    ("Mosaico", "rectangle.split.2x2"),
    ("Atajos", "keyboard"),
    ("General", "gearshape"),
    ("Acerca de", "info.circle"),
    ("Salir", "power"),
];
unsafe fn build_navigation(parent: Obj, frame: NSRect, target: Obj, footer: bool) -> Obj {
    unsafe {
        let table = alloc_view(
            class!(NSTableView),
            rect(0., 0., frame.size.width, frame.size.height),
        );
        let column: Allocated<AnyObject> = msg_send![class!(NSTableColumn), alloc];
        let column: Retained<AnyObject> =
            msg_send![column, initWithIdentifier:&*NSString::from_str("section")];
        let _: () = msg_send![&column, setWidth:frame.size.width];
        let _: () = msg_send![&table, addTableColumn:&*column];
        let _: () = msg_send![&table, setHeaderView:ptr::null::<AnyObject>()];
        let _: () = msg_send![&table, setStyle:3isize]; // NSTableViewStyleSourceList
        let _: () = msg_send![&table, setRowHeight:36f64];
        let _: () = msg_send![&table, setIntercellSpacing:NSSize::new(0., 4.)];
        let clear: Obj = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![&table, setBackgroundColor:clear];
        let _: () = msg_send![&table, setTag:if footer {1isize} else {0isize}];
        let _: () = msg_send![&table, setAllowsEmptySelection:footer];
        let _: () = msg_send![&table, setDataSource:target];
        let _: () = msg_send![&table, setDelegate:target];
        let _: () = msg_send![&table, setAccessibilityLabel:&*NSString::from_str(if footer {"Aplicación"} else {"Secciones de ajustes"})];
        let scroll = alloc_view(class!(NSScrollView), frame);
        let _: () = msg_send![&scroll, setDrawsBackground:false];
        let _: () = msg_send![&scroll, setHasVerticalScroller:false];
        let _: () = msg_send![&scroll, setHasHorizontalScroller:false];
        let _: () = msg_send![&scroll, setDocumentView:&*table];
        let _: () = msg_send![parent, addSubview:&*scroll];
        if !footer {
            let indices: Obj = msg_send![class!(NSIndexSet), indexSetWithIndex:0usize];
            let _: () = msg_send![&table, selectRowIndexes:indices, byExtendingSelection:false];
        }
        Retained::as_ptr(&table) as Obj
    }
}
extern "C-unwind" fn navigation_rows(_: Obj, _: Sel, table: Obj) -> isize {
    unsafe {
        let footer: isize = msg_send![table, tag];
        if footer == 1 { 2 } else { 5 }
    }
}
extern "C-unwind" fn navigation_view(_: Obj, _: Sel, table: Obj, _: Obj, row: isize) -> Obj {
    unsafe {
        let footer: isize = msg_send![table, tag];
        let (title, symbol) = SETTINGS_SECTIONS[row as usize + if footer == 1 { 5 } else { 0 }];
        let cell = alloc_view(class!(NSTableCellView), rect(0., 0., 160., 36.));
        let field = label(
            Retained::as_ptr(&cell) as Obj,
            rect(34., 8., 124., 20.),
            title,
            12.,
            0.,
            false,
        );
        let _: () = msg_send![&cell, setTextField:field];
        let image = alloc_view(class!(NSImageView), rect(7., 9., 18., 18.));
        let icon: Obj = msg_send![class!(NSImage), imageWithSystemSymbolName:&*NSString::from_str(symbol), accessibilityDescription:&*NSString::from_str(title)];
        let _: () = msg_send![&image, setImage:icon];
        let tint: Obj = msg_send![class!(NSColor), secondaryLabelColor];
        let _: () = msg_send![&image, setContentTintColor:tint];
        let _: () = msg_send![&cell, addSubview:&*image];
        let _: () = msg_send![&cell, setImageView:&*image];
        msg_send![Retained::into_raw(cell), autorelease]
    }
}
extern "C-unwind" fn navigation_selected(_: Obj, _: Sel, notification: Obj) {
    unsafe {
        let table: Obj = msg_send![notification, object];
        let row: isize = msg_send![table, selectedRow];
        if row < 0 {
            return;
        }
        let footer: isize = msg_send![table, tag];
        if footer == 1 {
            let _: () = msg_send![table, deselectAll:ptr::null::<AnyObject>()];
            if row == 0 {
                with_app(|a| a.show_about());
            } else {
                quit(ptr::null_mut(), sel!(quit:), ptr::null_mut());
            }
        } else {
            with_app(|a| {
                if a.tabs.is_null() {
                    return;
                }
                let _: () = msg_send![a.tabs, selectTabViewItemAtIndex:row];
                text(a.page_title, SETTINGS_SECTIONS[row as usize].0);
                if row == 1 {
                    a.rebuild_space_rows();
                }
            });
        }
    }
}
fn settings_tile_geometry(width: f64) -> (f64, Vec<NSRect>) {
    let columns = if width >= 560. { 4 } else { 3 };
    let rows = crate::tiling::Layout::ALL.len().div_ceil(columns);
    let height = 570. + rows.saturating_sub(2) as f64 * 108.;
    let card_width = (width - 16. - (columns - 1) as f64 * 16.) / columns as f64;
    let cards = (0..crate::tiling::Layout::ALL.len())
        .map(|i| {
            rect(
                8. + (i % columns) as f64 * (card_width + 16.),
                height - 217. - (i / columns) as f64 * 108.,
                card_width,
                90.,
            )
        })
        .collect();
    (height, cards)
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
            let _: () = msg_send![&border, setLevel:3isize];
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
                status_detail: String::new(),
                screen_configuration: screen_configuration(),
                border_clip: None,
                border_retry_pending: false,
                flash_enabled: crate::tile_manager::pref_or(
                    Retained::as_ptr(&defaults) as Obj,
                    "flash",
                    1,
                ) != 0,
                flash_switch: ptr::null_mut(),
                animate_switch: ptr::null_mut(),
                flash: Default::default(),
                flash_panel: None,
                hover_enabled: value(Retained::as_ptr(&defaults) as Obj, "hover"),
                raise_enabled: value(Retained::as_ptr(&defaults) as Obj, "raise"),
                tile_manager: crate::tile_manager::Manager::with_tab_target(target.clone()),
                tile_timer: None,
                tile_mouse: None,
                tile_interval: 0.,
                shortcuts: Default::default(),
                shortcut_config: None,
                shortcut_status: ptr::null_mut(),
                shortcut_fields: vec![],
                transfer: None,
                transfer_history: Default::default(),
                transfer_feedback_at: None,
                last_tiling_feedback: String::new(),
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
                tile_choices: vec![],
                tile_preview_portrait: None,
                tile_grid_window: None,
                tile_grid_fields: vec![],
                tile_grid_status: ptr::null_mut(),
                tile_grid_toggle: ptr::null_mut(),
                custom_name: ptr::null_mut(),
                custom_menu: ptr::null_mut(),
                custom_preview: ptr::null_mut(),
                custom_info: ptr::null_mut(),
                custom_apply: ptr::null_mut(),
                custom_delete: ptr::null_mut(),
                custom_display: None,
                custom_templates: vec![],
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
                desktop_preview: Default::default(),
                switching: None,
                hover_resume: Instant::now(),
                border,
                settings: None,
                settings_timer: None,
                about: None,
                updater: Updater::new(),
                updates_switch: ptr::null_mut(),
                tabs: ptr::null_mut(),
                page_title: ptr::null_mut(),
                bar_colors: vec![],
                space_page: ptr::null_mut(),
                space_scroll: ptr::null_mut(),
                space_ids: vec![],
                space_controls: vec![],
                width,
                rgb,
                observer: None,
                desktop_observer: None,
                presentation_timer: None,
                observed_window: None,
                observed_pid: 0,
                tracked: None,
                border_suppressed: false,
                border_space_pause: Default::default(),
                settle_queued: false,
                hover_timer: None,
                last_mouse: NSPoint::new(-1., -1.),
                mouse_since: Instant::now(),
                hover_probe: Default::default(),
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
            a.ensure_desktop_observer();
            crate::desktop_mode::refresh();
            a.tile_manager
                .set_presentation(crate::desktop_mode::current());
            a.apply_bar_background();
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
                ("flash", self.flash_enabled),
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
    fn ensure_desktop_observer(&mut self) {
        if !ax::trusted() {
            return;
        }
        let pid = ax::DesktopObserver::dock_pid();
        if self
            .desktop_observer
            .as_ref()
            .is_some_and(|observer| Some(observer.pid) == pid)
        {
            return;
        }
        self.desktop_observer = None;
        self.desktop_observer = pid.and_then(ax::DesktopObserver::new);
    }
    fn update_presentation_timer(&mut self) {
        unsafe {
            if !crate::desktop_mode::active() || self.sleeping {
                if let Some(timer) = self.presentation_timer.take() {
                    let _: () = msg_send![&timer, invalidate];
                }
                return;
            }
            if self.presentation_timer.is_some() {
                return;
            }
            // Only while presentation mode is active. Ordinary tiling/focus
            // reuses its existing compositor snapshots to detect entry.
            let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.2f64, target:&*self.target, selector:sel!(presentationTick:), userInfo:ptr::null::<AnyObject>(), repeats:true];
            let _: () = msg_send![&timer, setTolerance:0.03f64];
            let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
            add_common_timer(run, &timer);
            self.presentation_timer = Some(timer);
        }
    }
    fn refresh_spaces(&mut self) {
        unsafe {
            let s = spaces::snapshot();
            let b: Obj = msg_send![&self.status, button];
            let title = space_style::render(&s, Retained::as_ptr(&self.defaults) as Obj);
            self.refresh_status_buttons(&s);
            self.last_title = s.title;
            if self.status_detail != s.detail {
                let detail = NSString::from_str(&s.detail);
                let tip: Obj = if value(
                    Retained::as_ptr(&self.defaults) as Obj,
                    "desktopPreview.disabled",
                ) {
                    (&*detail as *const NSString).cast_mut().cast()
                } else {
                    ptr::null_mut()
                };
                let _: () = msg_send![b, setToolTip:tip];
                let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(&format!("{} · clic derecho para ajustes", s.detail))];
                self.status_detail = s.detail;
            }
            if !self.preview.is_null() {
                let current: Obj = msg_send![self.preview, attributedStringValue];
                let equal: bool = msg_send![current, isEqualToAttributedString:&*title];
                if !equal {
                    let _: () = msg_send![self.preview, setAttributedStringValue:&*title];
                }
            }
            if !self.space_scroll.is_null()
                && self.space_ids
                    != spaces::settings_desktops()
                        .iter()
                        .map(|(id, _)| *id)
                        .collect::<Vec<_>>()
            {
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
                self.desktop_preview.hide();
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
                    let bounds: NSRect = msg_send![&button, bounds];
                    let info: Obj = msg_send![class!(NSNumber), numberWithUnsignedLongLong:if compact {0u64} else {snapshot.desktops[index]}];
                    let tracking: Allocated<AnyObject> = msg_send![class!(NSTrackingArea), alloc];
                    let tracking: Retained<AnyObject> = msg_send![tracking, initWithRect:bounds, options:(1usize|128usize|512usize), owner:&*self.target, userInfo:info];
                    let _: () = msg_send![&button, addTrackingArea:&*tracking];
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
                let current: Obj = msg_send![button, attributedTitle];
                let equal: bool = msg_send![current, isEqualToAttributedString:&*title];
                if !equal {
                    let _: () = msg_send![button, setAttributedTitle:&*title];
                }
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
                let tip = NSString::from_str(&format!("{label} · clic derecho para ajustes"));
                let tip: Obj = if value(
                    Retained::as_ptr(&self.defaults) as Obj,
                    "desktopPreview.disabled",
                ) {
                    (&*tip as *const NSString).cast_mut().cast()
                } else {
                    ptr::null_mut()
                };
                let _: () = msg_send![button, setToolTip:tip];
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
            // No native slide happens, so the indicator is the only feedback.
            self.flash_desktop();
            return;
        }
        match space_switch::Switch::start(id) {
            Ok(switch) => {
                self.switching = Some(switch);
                self.tile_manager.space_changed();
                self.pause_border_for_space();
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
                self.pause_border_for_space();
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
    fn pause_border_for_space(&mut self) {
        self.border_space_pause.begin(Instant::now());
        self.pending_focus = None;
        self.focus_target = None;
        self.tracked = None;
        self.observed_window = None;
        self.observer = None;
        self.observed_pid = 0;
        self.border_suppressed = true;
        ws::stop();
        self.hide_border();
        unsafe {
            // One replaceable delivery, independent of tiling being enabled.
            let target = Retained::as_ptr(&self.target) as Obj;
            let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:sel!(resumeSpaceBorder:), object:ptr::null::<AnyObject>()];
            let _: () = msg_send![target, performSelector:sel!(resumeSpaceBorder:), withObject:ptr::null::<AnyObject>(), afterDelay:crate::border::SpacePause::DELAY.as_secs_f64()];
        }
    }
    fn hide_border(&self) {
        unsafe {
            let _: () = msg_send![&self.border, orderOut:ptr::null::<AnyObject>()];
        }
    }
    /// Soft full-screen frame for a reselected desktop. One lazily created
    /// panel, two replaceable one-shot deliveries and no polling timer.
    fn flash_desktop(&mut self) {
        if !self.flash_enabled {
            return;
        }
        let now = Instant::now();
        let fresh = !self.flash.pending(now);
        self.flash.begin(now);
        unsafe {
            let screen: Obj = msg_send![class!(NSScreen), mainScreen];
            // visibleFrame leaves the stroke below the system menu bar, so all
            // four sides are visible instead of the top one being clipped.
            let frame: NSRect = if screen.is_null() {
                rect(0., 0., 1440., 900.)
            } else {
                msg_send![screen, visibleFrame]
            };
            let panel = self.flash_panel.get_or_insert_with(|| {
                let panel: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
                let panel: Retained<AnyObject> = msg_send![panel, initWithContentRect:frame, styleMask:128usize, backing:2usize, defer:false];
                let _: () = msg_send![&panel, setReleasedWhenClosed:false];
                let _: () = msg_send![&panel, setOpaque:false];
                let clear: Obj = msg_send![class!(NSColor), clearColor];
                let _: () = msg_send![&panel, setBackgroundColor:clear];
                let _: () = msg_send![&panel, setLevel:3isize];
                let _: () = msg_send![&panel, setCollectionBehavior:BORDER_COLLECTION_BEHAVIOR];
                let _: () = msg_send![&panel, setIgnoresMouseEvents:true];
                let _: () = msg_send![&panel, setHasShadow:false];
                let _: () = msg_send![&panel, setHidesOnDeactivate:false];
                let v: Obj = msg_send![&panel, contentView];
                let _: () = msg_send![v, setWantsLayer:true];
                let layer: Obj = msg_send![v, layer];
                let rgb = crate::border::Flash::RGB;
                let c = color(rgb[0], rgb[1], rgb[2], 0.9);
                let cg: *const crate::native::CgColor = msg_send![c, CGColor];
                let _: () = msg_send![layer, setBorderColor:cg];
                let _: () = msg_send![layer, setBorderWidth:crate::border::Flash::WIDTH];
                let _: () = msg_send![layer, setCornerRadius:crate::border::Flash::CORNER];
                panel
            });
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions:true];
            let _: () = msg_send![&**panel, setFrame:frame, display:true];
            let _: () = msg_send![class!(CATransaction), commit];
            let _: () = msg_send![class!(CATransaction), flush];
            let target = Retained::as_ptr(&self.target) as Obj;
            for selector in [sel!(fadeFlash:), sel!(hideFlash:)] {
                let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:selector, object:ptr::null::<AnyObject>()];
            }
            if fresh {
                let _: () = msg_send![&**panel, setAlphaValue:0f64];
                let _: () = msg_send![&**panel, orderFrontRegardless];
                let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
                let context: Obj = msg_send![class!(NSAnimationContext), currentContext];
                let _: () = msg_send![context, setDuration:crate::border::Flash::FADE_IN];
                let animator: Obj = msg_send![&**panel, animator];
                let _: () = msg_send![animator, setAlphaValue:1f64];
                let _: () = msg_send![class!(NSAnimationContext), endGrouping];
            }
            let fade_at = crate::border::Flash::HOLD.as_secs_f64() - crate::border::Flash::FADE_OUT;
            let _: () = msg_send![target, performSelector:sel!(fadeFlash:), withObject:ptr::null::<AnyObject>(), afterDelay:fade_at];
            let _: () = msg_send![target, performSelector:sel!(hideFlash:), withObject:ptr::null::<AnyObject>(), afterDelay:crate::border::Flash::HOLD.as_secs_f64()];
        }
    }
    fn fade_flash(&self) {
        let Some(panel) = &self.flash_panel else {
            return;
        };
        unsafe {
            let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
            let context: Obj = msg_send![class!(NSAnimationContext), currentContext];
            let _: () = msg_send![context, setDuration:crate::border::Flash::FADE_OUT];
            let animator: Obj = msg_send![&**panel, animator];
            let _: () = msg_send![animator, setAlphaValue:0f64];
            let _: () = msg_send![class!(NSAnimationContext), endGrouping];
        }
    }
    fn hide_flash(&mut self) {
        if let Some(panel) = &self.flash_panel {
            unsafe {
                let _: () = msg_send![&**panel, setAlphaValue:0f64];
                let _: () = msg_send![&**panel, orderOut:ptr::null::<AnyObject>()];
            }
        }
    }
    fn update_window(&mut self) {
        self.update_queued = false;
        if self.border_space_pause.pending(Instant::now()) {
            self.hide_border();
            return;
        }
        if crate::desktop_mode::active() {
            self.hide_border();
            return;
        }
        if self.switching.is_some() {
            self.hide_border();
            return;
        }
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
            // Application activation can temporarily report its former key
            // window. Keep the existing border until the bounded focus delivery
            // finishes, rather than flashing a border on that intermediate one.
            if self
                .pending_focus
                .as_ref()
                .is_some_and(|pending| pending.pid == pid)
            {
                self.update_border_geometry();
                return;
            }
            if pid == std::process::id() as i32 {
                self.tracked = None;
                ws::stop();
                self.hide_border();
                return;
            }
            if self.focus_target.is_some_and(|(owner, _, _)| owner != pid) {
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
                // Missing/unknown subroles occur on real browser windows. Only
                // a positively identified auxiliary role excludes a candidate.
                if element
                    .as_ref()
                    .and_then(|w| ax::attr(w.0, "AXRole"))
                    .is_some_and(|role| ax::auxiliary_window_role(role.0))
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
        if crate::desktop_mode::active() {
            self.hide_border();
            return;
        }
        if self.border_suppressed || self.border_space_pause.pending(Instant::now()) {
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
            ws::set_border(id as u32);
            let geometry_changed = ws::bounds(id as u32) != Some(target.frame);
            let regions = ws::visible_border_regions(target.id, id as u32);
            let clip_changed = regions.as_ref().is_some_and(|regions| {
                self.border_clip
                    .as_ref()
                    .is_none_or(|(window, frame, old)| {
                        *window != target.id || *frame != target.frame || old != regions
                    })
            });
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
                let cg: *const crate::native::CgColor = msg_send![c, CGColor];
                let _: () = msg_send![layer, setBorderColor:cg];
                let _: () = msg_send![layer, setBorderWidth:self.width];
                let _: () = msg_send![layer, setCornerRadius:target.radius];
                let _: () = msg_send![layer, setCornerCurve:&*NSString::from_str("continuous")];
                let _: () = msg_send![layer, setContentsScale:scale];
                self.border_paint = Some(paint);
            }
            if clip_changed {
                if !geometry_changed && !paint_changed {
                    let _: () = msg_send![class!(CATransaction), begin];
                    let _: () = msg_send![class!(CATransaction), setDisableActions:true];
                }
                let view: Obj = msg_send![&self.border, contentView];
                let layer: Obj = msg_send![view, layer];
                let regions = regions.unwrap();
                apply_border_clip(layer, target.frame, &regions);
                self.border_clip = Some((target.id, target.frame, regions));
            }
            if geometry_changed || paint_changed || clip_changed {
                let _: () = msg_send![class!(CATransaction), commit];
                let _: () = msg_send![class!(CATransaction), flush];
            }
            let visible: bool = msg_send![&self.border, isVisible];
            if !visible {
                let _: () = msg_send![&self.border, orderFrontRegardless];
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
        self.ensure_desktop_observer();
        unsafe {
            if let Some(t) = self.hover_timer.take() {
                let _: () = msg_send![&t, invalidate];
            }
            self.hover_probe.reset();
            if self.hover_enabled
                && ax::trusted()
                && !self.sleeping
                && !crate::desktop_mode::active()
            {
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
            || self.transfer.is_some()
            || self.switching.is_some()
            || Instant::now() < self.hover_resume
            || focus::secure_input()
            || crate::desktop_mode::active()
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
                self.hover_probe.reset();
                return;
            }
            if point != self.last_mouse {
                self.last_mouse = point;
                self.mouse_since = Instant::now();
                self.hover_probe.reset();
                return;
            }
            if self.pending_focus.is_some()
                || self.mouse_since.elapsed() < Duration::from_millis(240)
                || !self.hover_probe.attempt()
            {
                return;
            }
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
            self.hover_probe.complete();
            let ws: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: Obj = msg_send![ws, frontmostApplication];
            let front_pid: i32 = msg_send![front, processIdentifier];
            if ax::focus_is_protected(front_pid) || focus::secure_input() {
                return;
            }
            let Some(id) = ax::window_id(w.0) else {
                return;
            };
            if !focus::available() {
                if self.raise_enabled {
                    ax::focus(w.0, pid);
                }
                return;
            }
            let focused = (front_pid == pid).then(|| ax::focused_id(pid)).flatten();
            let applied = self.focus_target.map(|(owner, window, _)| (owner, window));
            if !focus::activation_needed(front_pid, applied, pid, id) {
                self.focus_target = Some((pid, id, Instant::now()));
                self.update_window();
                return;
            }
            let previous = focused.filter(|old| *old != id);
            if let Some(previous) = previous {
                focus::deactivate(pid, previous);
            }
            self.pending_focus = Some(PendingFocus {
                front_pid,
                pid,
                id,
                point,
                verify: false,
                retries: 0,
            });
            let delay = if front_pid == pid { 0.04f64 } else { 0.0f64 };
            let _: () = msg_send![&self.target, performSelector:sel!(focusPending:), withObject:ptr::null::<AnyObject>(), afterDelay:delay];
        }
    }
    fn finish_focus(&mut self) {
        let Some(mut pending) = self.pending_focus.take() else {
            return;
        };
        // Reconcile the border after success, cancellation or exhausted retries.
        self.settle_focus();
        unsafe {
            let point: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            let flags: usize = msg_send![class!(NSEvent), modifierFlags];
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: Obj = msg_send![workspace, frontmostApplication];
            let front_pid: i32 = msg_send![front, processIdentifier];
            if self.switching.is_some()
                || self.transfer.is_some()
                || Instant::now() < self.hover_resume
                || !self.hover_enabled
                || self.sleeping
                || front_pid != pending.front_pid
                || point != pending.point
                || buttons != 0
                || flags & (1usize << 19) != 0
                || focus::secure_input()
                || crate::desktop_mode::active()
                || ax::focus_is_protected(front_pid)
            {
                // Cancellation cannot activate or reorder the previous window.
                return;
            }
            let Some(height) = primary_height() else {
                return;
            };
            let Some((window, pid)) = ax::window_at(point.x, height - point.y) else {
                return;
            };
            if pid != pending.pid || ax::window_id(window.0) != Some(pending.id) {
                // A floating window or layout may have moved under a stationary
                // pointer during the delay. Never activate the obsolete surface.
                self.hover_probe.reset();
                self.mouse_since = Instant::now();
                return;
            }
            if pending.verify {
                let reported = ax::focused_id(pending.pid);
                if reported == Some(pending.id) {
                    self.focus_target = Some((pending.pid, pending.id, Instant::now()));
                    self.update_window();
                    return;
                }
                // Activating a background application may restore its previous
                // key window after the first request. Repair that sibling switch
                // only while the original pointer target remains valid.
                if pending.retries >= 2 {
                    return;
                }
                if let Some(previous) = reported {
                    focus::deactivate(pending.pid, previous);
                }
                pending.verify = false;
                pending.retries += 1;
                self.pending_focus = Some(pending);
                let _: () = msg_send![&self.target, performSelector:sel!(focusPending:), withObject:ptr::null::<AnyObject>(), afterDelay:0.04f64];
                return;
            }
            let activated =
                focus::activate(pending.pid, pending.id, pending.front_pid == pending.pid);
            if activated {
                if self.raise_enabled {
                    ax::raise(window.0);
                }
                // A successful event post is not confirmation of keyboard focus.
                // Wait for application activation to settle before checking it.
                pending.front_pid = pending.pid;
                pending.verify = true;
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
            if activated {
                self.pending_focus = Some(pending);
                let _: () = msg_send![&self.target, performSelector:sel!(focusPending:), withObject:ptr::null::<AnyObject>(), afterDelay:0.08f64];
            }
        }
    }
    fn show_settings(&mut self) {
        self.desktop_preview.hide();
        unsafe {
            if self.settings.is_none() {
                self.build_settings();
            }
            self.refresh_permission();
            self.refresh_login();
            self.refresh_spaces();
            self.start_settings_refresh();
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps:true];
            let _: () = msg_send![self.settings.as_ref().unwrap(), makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn start_settings_refresh(&mut self) {
        if self.settings_timer.is_some() {
            return;
        }
        unsafe {
            let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:1f64, target:&*self.target, selector:sel!(settingsRefresh:), userInfo:ptr::null::<AnyObject>(), repeats:true];
            let _: () = msg_send![&timer, setTolerance:0.2f64];
            let run: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
            add_common_timer(run, &timer);
            self.settings_timer = Some(timer);
        }
    }
    unsafe fn build_settings(&mut self) {
        unsafe {
            let screen: Obj = msg_send![class!(NSScreen), mainScreen];
            let visible: NSRect = if screen.is_null() {
                rect(0., 0., 1440., 900.)
            } else {
                msg_send![screen, visibleFrame]
            };
            let width = (visible.size.width - 48.).clamp(700., 960.);
            let content_width = width - 268.;
            let height = (settings_tile_geometry(content_width.max(560.)).0 + 156.)
                .min(visible.size.height - 52.);
            let tab_height = height - 156.;
            let w = window(rect(0., 0., width, height), 1 | 2 | 4 | 32768);
            let _: () = msg_send![&w, setTitle:&*NSString::from_str("Ajustes de AMSpace")];
            let _: () = msg_send![&w, setTitleVisibility:1isize];
            let _: () = msg_send![&w, setTitlebarAppearsTransparent:true];
            let _: () = msg_send![&w, setHidesOnDeactivate:false];
            let _: () = msg_send![&w, center];
            let root: Obj = msg_send![&w, contentView];
            let target = Retained::as_ptr(&self.target) as Obj;
            let sidebar = alloc_view(class!(NSVisualEffectView), rect(0., 0., 184., height));
            let _: () = msg_send![&sidebar, setMaterial:7isize];
            let _: () = msg_send![&sidebar, setBlendingMode:0isize];
            let _: () = msg_send![root, addSubview:&*sidebar];
            let side = Retained::as_ptr(&sidebar) as Obj;
            label(
                side,
                rect(20., height - 82., 150., 32.),
                "amspace",
                23.,
                0.5,
                false,
            );
            build_navigation(side, rect(12., height - 328., 160., 228.), target, false);
            self.preview = label(
                side,
                rect(20., 144., 145., 28.),
                &self.last_title,
                18.,
                0.3,
                false,
            );
            build_navigation(side, rect(12., 25., 160., 80.), target, true);
            self.page_title = label(
                root,
                rect(220., height - 82., 432., 32.),
                "Ventanas",
                24.,
                0.4,
                false,
            );
            let tabs = alloc_view(
                class!(NSTabView),
                rect(220., 78., content_width, tab_height),
            );
            let _: () = msg_send![&tabs, setTabViewType:6usize];
            for i in 0..5 {
                let view = alloc_view(class!(NSView), rect(0., 0., content_width, tab_height));
                // Keep all Mosaic controls visible, including on smaller displays.
                // Matching frame and bounds aspect ratios scales the whole page uniformly.
                let tile_height = settings_tile_geometry(content_width.max(560.)).0;
                let scale = (tab_height / tile_height).min(content_width / 560.).min(1.);
                let page_width = if i == 2 || i == 3 {
                    content_width / scale
                } else {
                    432.
                };
                let page_height = if i == 2 || i == 3 {
                    tile_height
                } else if i == 1 {
                    430.
                } else {
                    360.
                };
                let inner = alloc_view(
                    class!(NSView),
                    if i == 2 || i == 3 {
                        rect(
                            0.,
                            tab_height - page_height * scale,
                            content_width,
                            page_height * scale,
                        )
                    } else {
                        rect(
                            (content_width - 432.) / 2.,
                            tab_height - page_height,
                            432.,
                            page_height,
                        )
                    },
                );
                let _: () = msg_send![&inner, setBoundsSize:NSSize::new(page_width, page_height)];
                let _: () = msg_send![&view, addSubview:&*inner];
                let page = Retained::as_ptr(&inner) as Obj;
                match i {
                    0 => self.build_window_page(page, target),
                    1 => self.build_space_page(page, target),
                    2 => self.build_tile_page(page, target),
                    3 => self.build_shortcuts_page(page, target),
                    _ => self.build_application_page(page, target),
                }
                let item: Retained<AnyObject> = msg_send![class!(NSTabViewItem), new];
                let _: () = msg_send![&item, setView:&*view];
                let _: () = msg_send![&tabs, addTabViewItem:&*item];
            }
            let _: () = msg_send![root, addSubview:&*tabs];
            self.tabs = Retained::as_ptr(&tabs) as Obj;
            self.permission = label(
                root,
                rect(238., 46., content_width - 165., 18.),
                "",
                11.,
                0.,
                true,
            );
            button(
                root,
                rect(width - 180., 38., 134., 30.),
                "Accesibilidad…",
                target,
                6,
            );
            self.feedback = label(
                root,
                rect(238., 5., content_width - 18., 30.),
                "",
                11.,
                0.,
                true,
            );
            let _: () = msg_send![self.feedback, setMaximumNumberOfLines:2isize];
            let _: () = msg_send![self.feedback,setSelectable:true];
            self.settings = Some(w);
        }
    }
    unsafe fn build_window_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            card(root, rect(0., 180., 432., 180.));
            label(
                root,
                rect(18., 319., 310., 24.),
                "Borde de la ventana activa",
                14.,
                0.3,
                false,
            );
            self.border_switch = toggle(
                root,
                374.,
                319.,
                "Borde de la ventana activa",
                target,
                1,
                self.border_enabled,
            );
            label(root, rect(18., 265., 90., 22.), "Color", 13., 0., false);
            let well = alloc_view(class!(NSColorWell), rect(365., 260., 48., 30.));
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
                    rect(105. + i as f64 * 56., 260., 50., 28.),
                    "●",
                    target,
                    20 + i as isize,
                );
                paint_swatch(b, name, *rgb);
            }
            label(root, rect(18., 212., 90., 22.), "Grosor", 13., 0., false);
            self.width_label = label(
                root,
                rect(360., 212., 58., 22.),
                &format!("{:.1} pt", self.width),
                12.,
                0.,
                true,
            );
            let slider = alloc_view(class!(NSSlider), rect(105., 211., 230., 24.));
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
            card(root, rect(0., 20., 432., 144.));
            label(
                root,
                rect(18., 122., 310., 24.),
                "Enfocar al pasar el cursor",
                14.,
                0.3,
                false,
            );
            self.hover_switch = toggle(
                root,
                374.,
                122.,
                "Enfocar al pasar el cursor",
                target,
                2,
                self.hover_enabled,
            );
            label(
                root,
                rect(18., 77., 310., 22.),
                "Traer al primer plano",
                13.,
                0.,
                false,
            );
            self.raise_switch = toggle(
                root,
                374.,
                77.,
                "Traer al primer plano",
                target,
                11,
                self.raise_enabled,
            );
            let _: () = msg_send![self.raise_switch, setEnabled:self.hover_enabled];
            label(
                root,
                rect(18., 36., 385., 18.),
                "Mantén ⌥ para pausar el foco automático.",
                11.,
                0.,
                true,
            );
        }
    }
    fn shortcut_action(&mut self, action: crate::tile_hotkey::Action) {
        use crate::tile_hotkey::Action;
        if matches!(
            action,
            Action::SendSpace(_) | Action::MonitorLeft | Action::MonitorRight
        ) {
            self.record_transfer(&format!("Atajo {action:?}; disponible={} pausa={} traslado={} entrada_segura={} presentación={}",self.shortcuts.available(action),self.sleeping,self.transfer.is_some(),focus::secure_input(),crate::desktop_mode::active()),false);
        }
        if !self.shortcuts.available(action)
            || self.sleeping
            || self.transfer.is_some()
            || focus::secure_input()
            || crate::desktop_mode::active()
        {
            return;
        }
        self.desktop_preview.hide();
        match action {
            Action::Desktop(index) => {
                let snapshot = spaces::snapshot();
                if let Some(id) = snapshot.desktops.get(index as usize) {
                    self.select_desktop(*id);
                } else {
                    self.feedback("Ese escritorio no existe en la pantalla activa.");
                }
            }
            Action::Float => {
                if self.switching.is_none() && self.layout_picker.is_none() {
                    self.tile_manager
                        .toggle_float(Retained::as_ptr(&self.defaults) as Obj);
                    self.update_tile_timer();
                }
            }
            Action::Layouts => self.toggle_picker(),
            Action::SendSpace(index) => {
                self.start_transfer(crate::window_move::Destination::Space(index as usize))
            }
            Action::MonitorLeft => {
                self.start_transfer(crate::window_move::Destination::Monitor(false))
            }
            Action::MonitorRight => {
                self.start_transfer(crate::window_move::Destination::Monitor(true))
            }
        }
    }
    fn start_transfer(&mut self, destination: crate::window_move::Destination) {
        if self.switching.is_some() || self.layout_picker.is_some() || self.transfer.is_some() {
            self.record_transfer(
                "Espera a que termine el cambio de escritorio o el traslado anterior.",
                true,
            );
            return;
        }
        self.record_transfer("Preparando traslado de ventana…", true);
        match crate::window_move::Pending::start(
            &mut self.tile_manager,
            Retained::as_ptr(&self.defaults) as Obj,
            destination,
        ) {
            Ok(pending) => {
                self.record_transfer(&format!("Solicitud nativa: pid={} ventana={} origen={:?} destino={:?} objetivo={:?}",pending.pid,pending.id,pending.plan.source,pending.plan.destination,pending.plan.target),false);
                self.pending_focus = None;
                self.focus_target = None;
                self.hide_border();
                self.transfer = Some(pending);
                self.schedule_transfer_tick();
            }
            Err(message) => {
                self.record_transfer(message, true);
                // A reselection of the current desktop produces no native
                // slide, so the soft frame is the only visible confirmation.
                if transfer_flashes(message) {
                    self.flash_desktop();
                }
            }
        }
    }
    fn schedule_transfer_tick(&self) {
        unsafe {
            let _: () = msg_send![&self.target,performSelector:sel!(transferTick:),withObject:ptr::null::<AnyObject>(),afterDelay:0.05f64];
        }
    }
    fn step_transfer(&mut self) {
        if self.sleeping {
            self.transfer = None;
            self.tile_manager.invalidate();
            return;
        }
        let Some(mut pending) = self.transfer.take() else {
            return;
        };
        match pending.poll() {
            crate::window_move::Progress::Waiting => {
                self.transfer = Some(pending);
                self.schedule_transfer_tick();
            }
            result => {
                let arrived =
                    matches!(result, crate::window_move::Progress::Arrived) && pending.arrived();
                let follow = pending.follow && arrived;
                let destination = pending.plan.destination.1;
                self.record_transfer(&format!("Confirmación {result:?}; llegada={arrived}; ventana={} native={:?} AX={:?}",pending.id,spaces::window_space(pending.id),ax::geometry(pending.element.0)),false);
                if arrived {
                    self.tile_manager.commit_keyboard_transfer(
                        pending.plan,
                        pending.pid,
                        pending.id,
                        pending.element,
                        pending.original,
                    );
                } else {
                    self.tile_manager.invalidate();
                }
                self.hover_resume = Instant::now() + Duration::from_millis(400);
                self.update_tile_timer();
                self.update_window();
                if let crate::window_move::Progress::Failed(message) = result {
                    self.record_transfer(message, true);
                } else if arrived {
                    self.record_transfer("Ventana enviada al destino.", true);
                } else {
                    self.record_transfer(
                        "La ventana cambió de ubicación antes de confirmar el traslado.",
                        true,
                    );
                }
                if follow {
                    self.select_desktop(destination);
                }
            }
        }
    }
    fn edit_shortcuts(&mut self, edits: &[(&str, isize)]) {
        if crate::shortcuts::edit(Retained::as_ptr(&self.defaults) as Obj, edits) {
            self.configure_shortcuts();
        } else {
            self.feedback("Esa combinación coincide con otro atajo de AMSpace. Se conservan los ajustes anteriores.");
        }
        self.refresh_shortcut_fields();
    }
    fn configure_shortcuts(&mut self) {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let tiling = crate::tile_manager::pref(defaults, "tile.enabled") != 0
            && ax::trusted()
            && !crate::desktop_mode::active();
        let mut bindings = if self.sleeping {
            vec![]
        } else {
            crate::shortcuts::bindings(defaults, tiling)
        };
        if !crate::window_move::space_moves_available() {
            bindings.retain(|b| !matches!(b.action, crate::tile_hotkey::Action::SendSpace(_)));
        }
        let priority = crate::tile_manager::pref(defaults, "shortcuts.priority") != 0;
        let config = (bindings.clone(), priority);
        let retry = priority && !self.shortcuts.priority && ax::trusted();
        if self.shortcut_config.as_ref() != Some(&config) || retry {
            self.shortcuts = Default::default(); // Release old Carbon reservations before replacing them.
            self.shortcuts = crate::shortcuts::Service::new(bindings, priority);
            self.shortcut_config = Some(config);
        }
        self.refresh_shortcut_status();
        self.refresh_picker_hint();
    }
    fn refresh_shortcut_fields(&self) {
        unsafe {
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            if !self.picker_mods.is_null() {
                let _: () = msg_send![self.picker_mods,selectItemAtIndex:crate::tile_hotkey::modifier_index(crate::tile_manager::pref(defaults,"tile.picker.mods")) as isize];
                let _: () = msg_send![self.picker_keys,selectItemAtIndex:crate::tile_hotkey::key_index(crate::tile_manager::pref_or(defaults,"tile.picker.key",2)) as isize];
            }
            for (field, key, is_key, fallback) in &self.shortcut_fields {
                let value = crate::tile_manager::pref_or(
                    Retained::as_ptr(&self.defaults) as Obj,
                    key,
                    *fallback,
                );
                let index = if *is_key {
                    crate::tile_hotkey::key_index(value)
                } else {
                    crate::tile_hotkey::modifier_index(value)
                };
                let _: () = msg_send![*field,selectItemAtIndex:index as isize];
            }
        }
    }
    fn refresh_shortcut_status(&self) {
        if self.shortcut_status.is_null() {
            return;
        }
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let enabled = crate::tile_manager::pref(defaults, "shortcuts.enabled") != 0;
        let priority = crate::tile_manager::pref(defaults, "shortcuts.priority") != 0;
        let mut status = if priority && self.shortcuts.priority_active() {
            "Prioridad activa".to_string()
        } else if priority && self.shortcuts.priority {
            "Prioridad suspendida; vuelve a activar el check para reintentar".to_string()
        } else if priority {
            "Prioridad no disponible; se usan reservas normales".to_string()
        } else {
            "Prioridad desactivada".to_string()
        };
        if !enabled {
            status.push_str(" · navegación desactivada");
        }
        if !self.shortcuts.unavailable.is_empty() {
            status.push_str(&format!(
                " · {} combinaciones no disponibles",
                self.shortcuts.unavailable.len()
            ));
        }
        if !crate::window_move::space_moves_available() {
            status.push_str("\nEl envío entre escritorios no está disponible en este macOS.");
        }
        unsafe {
            text(self.shortcut_status, &status);
        }
    }
    unsafe fn build_shortcuts_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            let bounds: NSRect = msg_send![root, bounds];
            let width = bounds.size.width;
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            card(root, rect(0., 480., width, 90.));
            label(
                root,
                rect(18., 530., width - 90., 24.),
                "Navegación y traslado de ventanas",
                14.,
                0.3,
                false,
            );
            toggle(
                root,
                width - 58.,
                530.,
                "Activar atajos de navegación y traslado",
                target,
                90,
                crate::tile_manager::pref(defaults, "shortcuts.enabled") != 0,
            );
            label(
                root,
                rect(18., 492., width - 36., 28.),
                "Activa los atajos nuevos cuando hayas elegido tus combinaciones.",
                11.,
                0.,
                true,
            );
            let priority = button(
                root,
                rect(18., 444., width - 36., 26.),
                "Dar prioridad a los atajos de AMSpace",
                target,
                91,
            );
            let _: () = msg_send![priority,setButtonType:3usize];
            let _: () = msg_send![priority,setState:crate::tile_manager::pref(defaults,"shortcuts.priority").clamp(0,1)];
            label(
                root,
                rect(18., 416., width - 36., 22.),
                "macOS, otras herramientas de teclado y la entrada segura pueden limitarla.",
                11.,
                0.,
                true,
            );
            self.shortcut_status =
                label(root, rect(18., 365., width - 155., 44.), "", 10., 0., true);
            let _: () = msg_send![self.shortcut_status,setMaximumNumberOfLines:2isize];
            button(
                root,
                rect(width - 130., 380., 112., 26.),
                "Sugerencias…",
                target,
                54,
            );
            let modifiers: Vec<_> = crate::tile_hotkey::MODIFIERS
                .iter()
                .map(|(name, _)| *name)
                .collect();
            let keys: Vec<_> = crate::tile_hotkey::KEYS
                .iter()
                .map(|(name, _)| *name)
                .collect();
            for (row, (title, prefix, mods_fallback, key_fallback)) in [
                ("Cambiar a escritorio", "shortcuts.desktop", 11, None),
                ("Enviar ventana a escritorio", "shortcuts.send", 4, None),
                ("Enviar a pantalla izquierda", "shortcuts.left", 9, Some(50)),
                ("Enviar a pantalla derecha", "shortcuts.right", 9, Some(51)),
                ("Ventana flotante / mosaico", "tile", 0, Some(0)),
                ("Selector de plantillas", "tile.picker", 0, Some(2)),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 326. - row as f64 * 43.;
                label(
                    root,
                    rect(18., y + 3., width - 300., 22.),
                    title,
                    13.,
                    0.,
                    false,
                );
                let mods = popup(
                    root,
                    rect(width - 264., y - 2., 122., 30.),
                    &modifiers,
                    target,
                    300 + self.shortcut_fields.len() as isize,
                );
                self.shortcut_fields
                    .push((mods, format!("{prefix}.mods"), false, mods_fallback));
                if let Some(fallback) = key_fallback {
                    let key = popup(
                        root,
                        rect(width - 134., y - 2., 116., 30.),
                        &keys,
                        target,
                        300 + self.shortcut_fields.len() as isize,
                    );
                    self.shortcut_fields
                        .push((key, format!("{prefix}.key"), true, fallback));
                } else {
                    label(
                        root,
                        rect(width - 118., y + 3., 96., 22.),
                        "1 – 9",
                        13.,
                        0.3,
                        false,
                    );
                }
            }
            let stay = button(
                root,
                rect(18., 59., width - 36., 26.),
                "Al enviar a otro escritorio, permanecer en el origen",
                target,
                93,
            );
            let _: () = msg_send![stay,setButtonType:3usize];
            let _: () = msg_send![stay,setState:crate::tile_manager::pref_or(defaults,"shortcuts.stay",1).clamp(0,1)];
            label(
                root,
                rect(18., 15., width - 36., 32.),
                "Los escritorios se numeran por pantalla. Flotante y selector requieren Mosaico activo.",
                11.,
                0.,
                true,
            );
            self.refresh_shortcut_fields();
            self.refresh_shortcut_status();
        }
    }
    fn configure_tiling(&mut self) {
        self.ensure_desktop_observer();
        unsafe {
            if let Some(t) = self.tile_timer.take() {
                let _: () = msg_send![&t, invalidate];
            }
            self.configure_shortcuts();
            self.tile_mouse = None;
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            if crate::tile_manager::pref(defaults, "tile.enabled") == 0
                || self.sleeping
                || crate::desktop_mode::active()
                || !ax::trusted()
            {
                self.layout_picker = None;
                self.tile_manager.cancel_drag();
                return;
            }
            self.tile_mouse = crate::tile_mouse::Monitor::new(|down, point| {
                autoreleasepool(|_| {
                    with_app(|a| {
                        if a.layout_picker.is_some()
                            || a.transfer.is_some()
                            || a.switching.is_some()
                            || a.sleeping
                        {
                            return;
                        }
                        a.tile_manager.mouse_event(
                            Retained::as_ptr(&a.defaults) as Obj,
                            down,
                            point,
                        );
                        a.update_tile_timer();
                    })
                });
            });
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
        if self.tile_controls.len() != 3 {
            return;
        }
        unsafe {
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            let title = if let Some(id) = self.tile_space {
                let snapshot = spaces::snapshot();
                format!(
                    "Escritorio {}",
                    snapshot.desktops.iter().position(|s| *s == id).unwrap_or(0) + 1
                )
            } else {
                "Selecciona un escritorio".into()
            };
            text(self.tile_controls[0], &title);
            let enabled = self.tile_space.is_some();
            let _: () = msg_send![self.tile_controls[1],setEnabled:enabled];
            let context = self.tile_manager.picker_context();
            let area = context.as_ref().map(|c| c.area).unwrap_or_else(|| {
                let screen: Obj = msg_send![class!(NSScreen), mainScreen];
                msg_send![screen, visibleFrame]
            });
            let portrait = area.size.height > area.size.width;
            let redraw = self.tile_preview_portrait != Some(portrait);
            let current = self.tile_manager.current_layout(defaults);
            for (button, layout) in self.tile_choices.iter().zip(crate::tiling::Layout::ALL) {
                let valid = enabled
                    && context
                        .as_ref()
                        .is_none_or(|c| c.proposal(layout).is_some());
                let _: () = msg_send![*button,setEnabled:valid];
                let _: () = msg_send![*button,setState:if current == layout {1isize} else {0isize}];
                let layer: Obj = msg_send![*button, layer];
                let accent: Obj = msg_send![class!(NSColor), controlAccentColor];
                let background: Obj = if current == layout {
                    msg_send![accent, colorWithAlphaComponent:0.18f64]
                } else {
                    msg_send![class!(NSColor), controlBackgroundColor]
                };
                let stroke: Obj = if current == layout {
                    accent
                } else {
                    msg_send![class!(NSColor), separatorColor]
                };
                let cg: *const crate::native::CgColor = msg_send![background, CGColor];
                let border: *const crate::native::CgColor = msg_send![stroke, CGColor];
                let _: () = msg_send![layer, setBackgroundColor:cg];
                let _: () = msg_send![layer, setBorderColor:border];
                let _: () =
                    msg_send![*button,setTitle:&*NSString::from_str(layout.short_name(area))];
                if redraw {
                    let image = crate::layout_thumbnail::image(layout, portrait);
                    let _: () = msg_send![*button,setImage:&*image];
                }
            }
            self.tile_preview_portrait = Some(portrait);
            if let Some(id) = self.tile_space {
                let _: () = msg_send![self.tile_controls[1],setState:1-crate::tile_manager::pref(defaults,&format!("tile.off.{id}")).clamp(0,1)];
            }
            text(
                self.tile_controls[2],
                "Configura los atajos en la sección Atajos.",
            );
        }
    }
    unsafe fn build_tile_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            let bounds: NSRect = msg_send![root, bounds];
            let width = bounds.size.width;
            let (height, cards) = settings_tile_geometry(width);
            card(root, rect(0., height - 64., width, 64.));
            label(
                root,
                rect(18., height - 43., width - 90., 24.),
                "Activar mosaico",
                14.,
                0.3,
                false,
            );
            toggle(
                root,
                width - 58.,
                height - 43.,
                "Activar mosaico",
                target,
                40,
                crate::tile_manager::pref(defaults, "tile.enabled") != 0,
            );
            let title = label(
                root,
                rect(18., height - 104., width - 90., 24.),
                "",
                14.,
                0.3,
                false,
            );
            let enabled = toggle(
                root,
                width - 58.,
                height - 104.,
                "Mosaico en este escritorio",
                target,
                41,
                true,
            );
            for (i, layout) in crate::tiling::Layout::ALL.into_iter().enumerate() {
                let b = button(root, cards[i], layout.name(), target, 80 + i as isize);
                let _: () = msg_send![b, setButtonType:1usize];
                let _: () = msg_send![b, setBezelStyle:6usize];
                let _: () = msg_send![b, setBordered:false];
                let _: () = msg_send![b, setWantsLayer:true];
                let layer: Obj = msg_send![b, layer];
                let _: () = msg_send![layer, setCornerRadius:10f64];
                let _: () = msg_send![layer, setBorderWidth:1f64];
                let _: () = msg_send![b, setImagePosition:5usize];
                let font: Obj = msg_send![class!(NSFont), systemFontOfSize:11f64,weight:0.2f64];
                let _: () = msg_send![b, setFont:font];
                let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(&format!("Plantilla: {}",layout.name()))];
                self.tile_choices.push(b);
            }
            // The lower rows start under the last template row, whatever its
            // position for the current column count, so nothing overlaps.
            let last_row = cards.last().map_or(245., |card| card.origin.y);
            let animation_row = last_row - 37.;
            let simple_row = animation_row - 40.;
            let preview_row = simple_row - 40.;
            let groups_row = preview_row - 40.;
            let actions_row = groups_row - 44.;
            label(
                root,
                rect(18., animation_row, width - 90., 22.),
                "Animar colocaciones",
                13.,
                0.,
                false,
            );
            self.animate_switch = toggle(
                root,
                width - 58.,
                animation_row,
                "Animar colocaciones",
                target,
                63,
                crate::tile_manager::pref_or(defaults, "tile.animate", 1) != 0,
            );
            label(
                root,
                rect(18., simple_row, width - 90., 22.),
                "Colocación inteligente",
                13.,
                0.,
                false,
            );
            toggle(
                root,
                width - 58.,
                simple_row,
                "Colocación inteligente",
                target,
                43,
                crate::tile_manager::pref(defaults, "tile.simple") == 0,
            );
            label(
                root,
                rect(18., preview_row, width - 90., 22.),
                "Previsualizar al redimensionar",
                13.,
                0.,
                false,
            );
            toggle(
                root,
                width - 58.,
                preview_row,
                "Previsualizar al redimensionar",
                target,
                61,
                crate::tile_manager::pref(defaults, "tile.resize.preview") != 0,
            );
            label(
                root,
                rect(18., groups_row, width - 90., 22.),
                "Agrupar como pestañas",
                13.,
                0.,
                false,
            );
            toggle(
                root,
                width - 58.,
                groups_row,
                "Agrupar como pestañas",
                target,
                65,
                crate::tile_manager::pref_or(defaults, "tile.groups", 1) != 0,
            );
            button(
                root,
                rect(15., actions_row, (width - 36.) / 2., 28.),
                "Rejilla y plantillas…",
                target,
                49,
            );
            button(
                root,
                rect(width / 2. + 3., actions_row, (width - 36.) / 2., 28.),
                "Selector de plantillas",
                target,
                57,
            );
            let hint = label(root, rect(18., 17., width - 200., 24.), "", 11., 0., true);
            button(
                root,
                rect(width - 181., 12., 165., 30.),
                "Apps flotantes…",
                target,
                46,
            );
            self.tile_controls = vec![title, enabled, hint];
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
                if self
                    .shortcuts
                    .available(crate::tile_hotkey::Action::Layouts)
                {
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
            || crate::desktop_mode::active()
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
        self.layout_picker = crate::tile_picker::Picker::new(
            context,
            Retained::as_ptr(&self.target) as Obj,
            defaults,
        );
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
        let Some(selection) = picker.selection() else {
            self.layout_picker = Some(picker);
            return;
        };
        let context = picker.context.clone();
        drop(picker);
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        self.tile_manager.refresh_for_picker(defaults);
        let applied = match selection {
            crate::tile_picker::Selection::Layout(layout) => {
                self.tile_manager.apply_template(defaults, &context, layout)
            }
            crate::tile_picker::Selection::Custom(distribution) => self
                .tile_manager
                .apply_custom_distribution(defaults, &context, &distribution),
        };
        if applied {
            self.tile_manager.tick(defaults);
        } else {
            self.feedback("La pantalla o sus ventanas han cambiado. Abre de nuevo el selector.");
        }
        self.hover_resume = Instant::now() + Duration::from_millis(300);
        self.update_tile_timer();
        self.refresh_tile_controls();
    }
    fn show_tile_grid(&mut self) {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        self.custom_display = self
            .tile_manager
            .custom_context(defaults, None)
            .ok()
            .map(|c| c.workspace.0);
        unsafe {
            if self.tile_grid_window.is_none() {
                let w = window(rect(0., 0., 520., 620.), 1 | 2 | 4);
                let _: () = msg_send![&w,setTitle:&*NSString::from_str("Rejilla y plantillas")];
                let _: () = msg_send![&w, center];
                let root: Obj = msg_send![&w, contentView];
                let target = Retained::as_ptr(&self.target) as Obj;
                label(
                    root,
                    rect(22., 574., 476., 26.),
                    "Rejilla y plantillas",
                    18.,
                    0.3,
                    false,
                );
                label(
                    root,
                    rect(22., 533., 400., 22.),
                    "Ajustar tamaños a una rejilla virtual",
                    14.,
                    0.,
                    false,
                );
                self.tile_grid_toggle = toggle(
                    root,
                    455.,
                    533.,
                    "Activar rejilla virtual",
                    target,
                    50,
                    crate::tile_manager::pref(defaults, "tile.grid.enabled") != 0,
                );
                label(
                    root,
                    rect(22., 504., 476., 20.),
                    "Las divisiones se alinean sin dejar huecos entre ventanas.",
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
                    let x = 22. + i as f64 * 256.;
                    label(root, rect(x, 475., 220., 20.), name, 12., 0., false);
                    let field = alloc_view(class!(NSTextField), rect(x, 443., 220., 26.));
                    let _: () = msg_send![&field,setStringValue:&*NSString::from_str(&crate::tile_manager::grid_count(defaults,key,*fallback,*max).to_string())];
                    let _: () = msg_send![&field,setAccessibilityLabel:&*NSString::from_str(name)];
                    let _: () = msg_send![root,addSubview:&*field];
                    self.tile_grid_fields.push(Retained::as_ptr(&field) as Obj);
                }
                button(
                    root,
                    rect(350., 400., 148., 30.),
                    "Aplicar rejilla",
                    target,
                    51,
                );
                card(root, rect(14., 104., 492., 280.));
                label(
                    root,
                    rect(28., 346., 460., 24.),
                    "Plantillas personalizadas",
                    15.,
                    0.3,
                    false,
                );
                let name = alloc_view(class!(NSTextField), rect(28., 303., 320., 27.));
                let _: () = msg_send![&name,setPlaceholderString:&*NSString::from_str("Nombre de la plantilla")];
                let _: () = msg_send![&name,setAccessibilityLabel:&*NSString::from_str("Nombre de la plantilla")];
                let _: () = msg_send![root,addSubview:&*name];
                self.custom_name = Retained::as_ptr(&name) as Obj;
                button(
                    root,
                    rect(360., 301., 132., 30.),
                    "Guardar actual",
                    target,
                    95,
                );
                self.custom_menu = popup(root, rect(26., 254., 324., 30.), &[], target, 96);
                let _: () = msg_send![self.custom_menu,setAccessibilityLabel:&*NSString::from_str("Plantillas guardadas")];
                self.custom_delete =
                    button(root, rect(360., 254., 132., 30.), "Eliminar", target, 98);
                let preview = alloc_view(class!(NSImageView), rect(26., 124., 220., 116.));
                let _: () = msg_send![root,addSubview:&*preview];
                self.custom_preview = Retained::as_ptr(&preview) as Obj;
                self.custom_info = wrapping_label(root, rect(260., 175., 232., 64.), "", 12.);
                self.custom_apply = button(
                    root,
                    rect(360., 128., 132., 30.),
                    "Aplicar plantilla",
                    target,
                    97,
                );
                label(
                    root,
                    rect(22., 82., 476., 18.),
                    "Copia rápida del escritorio actual",
                    11.,
                    0.,
                    true,
                );
                button(
                    root,
                    rect(18., 48., 230., 30.),
                    "Guardar copia rápida",
                    target,
                    52,
                );
                button(
                    root,
                    rect(272., 48., 230., 30.),
                    "Restaurar copia rápida",
                    target,
                    53,
                );
                self.tile_grid_status = wrapping_label(
                    root,
                    rect(22., 3., 476., 42.),
                    "Se guardan divisiones y proporciones; no se abren aplicaciones.",
                    11.,
                );
                self.tile_grid_window = Some(w);
            }
            self.refresh_custom_templates(None);
            let _: () = msg_send![self.tile_grid_window.as_ref().unwrap(),makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    fn refresh_custom_templates(&mut self, selected: Option<&str>) {
        if self.custom_menu.is_null() {
            return;
        }
        unsafe {
            let index: isize = msg_send![self.custom_menu, indexOfSelectedItem];
            let previous = self
                .custom_templates
                .get(index.max(0) as usize)
                .map(|t| t.id.clone());
            let selected = selected.map(str::to_owned).or(previous);
            self.custom_templates =
                crate::custom_templates::entries(Retained::as_ptr(&self.defaults) as Obj);
            let _: () = msg_send![self.custom_menu, removeAllItems];
            for template in &self.custom_templates {
                let _: () = msg_send![self.custom_menu,addItemWithTitle:&*NSString::from_str(&template.name)];
            }
            if self.custom_templates.is_empty() {
                let _: () = msg_send![self.custom_menu,addItemWithTitle:&*NSString::from_str("Ninguna plantilla guardada")];
            } else if let Some(index) =
                selected.and_then(|id| self.custom_templates.iter().position(|t| t.id == id))
            {
                let _: () = msg_send![self.custom_menu,selectItemAtIndex:index as isize];
            }
            self.refresh_custom_preview();
        }
    }
    fn refresh_custom_preview(&self) {
        if self.custom_menu.is_null() {
            return;
        }
        unsafe {
            let index: isize = msg_send![self.custom_menu, indexOfSelectedItem];
            let template = usize::try_from(index)
                .ok()
                .and_then(|i| self.custom_templates.get(i));
            let _: () = msg_send![self.custom_apply,setEnabled:template.is_some()];
            let _: () = msg_send![self.custom_delete,setEnabled:template.is_some()];
            let size = self
                .custom_display
                .and_then(crate::tile_manager::area)
                .map_or(objc2_foundation::NSSize::new(1600., 1000.), |r| r.size);
            if let Some(template) = template {
                if let Some(desktop) = template.preview() {
                    let image = crate::layout_thumbnail::custom_image(&desktop, size);
                    let _: () = msg_send![self.custom_preview,setImage:&*image];
                }
                text(
                    self.custom_info,
                    &format!(
                        "{} {}\nDivisiones y proporciones guardadas.\nSe conserva la rejilla actual.",
                        template.count,
                        if template.count == 1 {
                            "ventana"
                        } else {
                            "ventanas"
                        }
                    ),
                );
            } else {
                let _: () = msg_send![self.custom_preview,setImage:ptr::null::<AnyObject>()];
                text(
                    self.custom_info,
                    "Guarda el mosaico actual.\nReutilízalo con otras ventanas.",
                );
            }
        }
    }
    fn custom_action(&mut self, tag: isize) {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        if tag == 96 {
            self.refresh_custom_preview();
            return;
        }
        if self.switching.is_some()
            || self.transfer.is_some()
            || self.layout_picker.is_some()
            || self.sleeping
        {
            unsafe {
                text(
                    self.tile_grid_status,
                    "Espera a que termine la operación actual antes de cambiar una plantilla.",
                );
            }
            return;
        }
        unsafe {
            let index: isize = msg_send![self.custom_menu, indexOfSelectedItem];
            let selected = usize::try_from(index)
                .ok()
                .and_then(|i| self.custom_templates.get(i))
                .cloned();
            if tag == 98 {
                if let Some(template) = selected {
                    crate::custom_templates::remove(defaults, &template.id);
                    self.refresh_custom_templates(None);
                    text(
                        self.tile_grid_status,
                        "Plantilla eliminada. El mosaico actual se conserva.",
                    );
                }
                return;
            }
            let context = match self
                .tile_manager
                .custom_context(defaults, self.custom_display)
            {
                Ok(context) => context,
                Err(message) => {
                    text(self.tile_grid_status, message);
                    return;
                }
            };
            self.custom_display = Some(context.workspace.0);
            if tag == 95 {
                let name: *mut NSString = msg_send![self.custom_name, stringValue];
                let name = name.as_ref().map(ToString::to_string).unwrap_or_default();
                let result = context
                    .desktop
                    .save_distribution(context.area)
                    .ok_or("Esta distribución no se puede guardar.")
                    .and_then(|data| crate::custom_templates::save(defaults, &name, data));
                match result {
                    Ok(id) => {
                        self.refresh_custom_templates(Some(&id));
                        text(
                            self.tile_grid_status,
                            "Plantilla guardada. Puedes aplicarla en otros escritorios con el mismo número de ventanas.",
                        );
                    }
                    Err(message) => text(self.tile_grid_status, message),
                }
            } else if let Some(template) = selected {
                if template.count != context.desktop.order.len() {
                    text(
                        self.tile_grid_status,
                        &format!(
                            "Esta plantilla tiene {} huecos. Necesitas {} ventanas en mosaico en esta pantalla; ahora hay {}.",
                            template.count,
                            template.count,
                            context.desktop.order.len()
                        ),
                    );
                } else if self.tile_manager.apply_custom_distribution(
                    defaults,
                    &context,
                    &template.distribution,
                ) {
                    text(
                        self.tile_grid_status,
                        "Plantilla aplicada conservando sus divisiones y proporciones.",
                    );
                    self.refresh_tile_controls();
                    self.update_tile_timer();
                } else {
                    text(
                        self.tile_grid_status,
                        "Las ventanas o la pantalla han cambiado, o sus tamaños mínimos no caben en esta distribución.",
                    );
                }
            }
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
            card(root, rect(0., 284., 432., 76.));
            label(
                root,
                rect(18., 311., 310., 24.),
                "Abrir al iniciar sesión",
                14.,
                0.3,
                false,
            );
            self.login = toggle(
                root,
                374.,
                311.,
                "Abrir al iniciar sesión",
                target,
                5,
                false,
            );
            card(root, rect(0., 132., 432., 136.));
            label(
                root,
                rect(18., 227., 240., 24.),
                "Actualizaciones",
                14.,
                0.3,
                false,
            );
            let check = button(root, rect(299., 222., 115., 30.), "Buscar ahora", target, 8);
            let available = self.updater.is_some();
            let _: () = msg_send![check, setEnabled:available];
            label(
                root,
                rect(18., 176., 340., 22.),
                "Buscar automáticamente",
                13.,
                0.,
                false,
            );
            self.updates_switch = toggle(
                root,
                374.,
                176.,
                "Buscar actualizaciones automáticamente",
                target,
                9,
                self.updater.as_ref().is_some_and(Updater::automatic_checks),
            );
            let _: () = msg_send![self.updates_switch, setEnabled:available];
            label(
                root,
                rect(18., 77., 245., 24.),
                concat!("AMSpace ", env!("CARGO_PKG_VERSION")),
                14.,
                0.3,
                false,
            );
            button(
                root,
                rect(240., 72., 176., 30.),
                "Historial de versiones",
                target,
                32,
            );
            label(
                root,
                rect(18., 44., 200., 20.),
                "Alberto Martín",
                12.,
                0.,
                true,
            );
            button(
                root,
                rect(240., 28., 176., 30.),
                "Copiar diagnóstico",
                target,
                99,
            );
        }
    }
    fn apply_bar_background(&self) {
        unsafe {
            let index = crate::bar_background::normalize(crate::tile_manager::pref(
                Retained::as_ptr(&self.defaults) as Obj,
                "barBackground",
            ));
            let parent: Obj = msg_send![&self.status, button];
            crate::bar_background::apply(parent, index);
            for (i, button) in self.bar_colors.iter().enumerate() {
                let _: () = msg_send![*button, setState:if i == index {1isize} else {0isize}];
                let layer: Obj = msg_send![*button, layer];
                let accent: Obj = msg_send![class!(NSColor), controlAccentColor];
                let background: Obj = if i == index {
                    msg_send![accent, colorWithAlphaComponent:0.12f64]
                } else {
                    msg_send![class!(NSColor), controlBackgroundColor]
                };
                let border: Obj = if i == index {
                    accent
                } else {
                    msg_send![class!(NSColor), separatorColor]
                };
                let bg: *const crate::native::CgColor = msg_send![background, CGColor];
                let stroke: *const crate::native::CgColor = msg_send![border, CGColor];
                let _: () = msg_send![layer, setBackgroundColor:bg];
                let _: () = msg_send![layer, setBorderColor:stroke];
                let _: () = msg_send![*button, setAccessibilityValue:&*NSString::from_str(if i == index {"Seleccionado"} else {""})];
            }
        }
    }
    unsafe fn build_space_page(&mut self, root: Obj, target: Obj) {
        unsafe {
            self.space_page = root;
            card(root, rect(0., 216., 432., 144.));
            label(
                root,
                rect(18., 322., 300., 22.),
                "Fondo del widget",
                14.,
                0.3,
                false,
            );
            for (i, (name, rgb)) in crate::bar_background::OPTIONS.iter().enumerate() {
                let x = 16. + i as f64 * 57.;
                let b = button(
                    root,
                    rect(x, 265., 54., 40.),
                    if rgb.is_none() { "∅" } else { "●" },
                    target,
                    70 + i as isize,
                );
                let _: () = msg_send![b, setButtonType:1usize];
                let _: () = msg_send![b, setBordered:false];
                let _: () = msg_send![b, setWantsLayer:true];
                let layer: Obj = msg_send![b, layer];
                let _: () = msg_send![layer, setCornerRadius:10f64];
                let _: () = msg_send![layer, setBorderWidth:1f64];
                let accessibility = format!("Fondo del widget: {name}");
                if let Some(rgb) = rgb {
                    paint_swatch(b, &accessibility, rgb.map(|n| n as f64 / 255.));
                } else {
                    let _: () = msg_send![b, setTitle:&*NSString::from_str("")];
                    let image: Obj = msg_send![class!(NSImage), imageWithSystemSymbolName:&*NSString::from_str("nosign"), accessibilityDescription:&*NSString::from_str(&accessibility)];
                    let _: () = msg_send![b, setImage:image];
                    let _: () = msg_send![b, setImagePosition:1usize];
                    let _: () =
                        msg_send![b, setAccessibilityLabel:&*NSString::from_str(&accessibility)];
                }
                let caption = label(root, rect(x - 1., 239., 56., 18.), name, 10., 0., true);
                let _: () = msg_send![caption, setAlignment:1isize];
                self.bar_colors.push(b);
            }
            self.apply_bar_background();
            label(
                root,
                rect(18., 171., 210., 22.),
                "Escritorios",
                14.,
                0.3,
                false,
            );
            let menu = popup(
                root,
                rect(248., 165., 168., 30.),
                &["Puntos", "Números", "Iconos"],
                target,
                12,
            );
            let _: () = msg_send![menu, selectItemAtIndex:space_style::mode(Retained::as_ptr(&self.defaults) as Obj)];
            let children: Obj = msg_send![root, subviews];
            let count: usize = msg_send![children, count];
            for i in 0..count {
                let child: Obj = msg_send![children, objectAtIndex:i];
                let mut frame: NSRect = msg_send![child, frame];
                frame.origin.y += 70.;
                let _: () = msg_send![child, setFrame:frame];
            }
            let scroll = alloc_view(class!(NSScrollView), rect(0., 68., 432., 148.));
            let _: () = msg_send![&scroll, setHasVerticalScroller:true];
            let _: () = msg_send![&scroll, setAutohidesScrollers:true];
            let _: () = msg_send![&scroll, setDrawsBackground:false];
            let _: () = msg_send![root, addSubview:&*scroll];
            self.space_scroll = Retained::as_ptr(&scroll) as Obj;
            label(
                root,
                rect(18., 40., 330., 22.),
                "Preview al pasar por un escritorio",
                12.,
                0.,
                false,
            );
            let disabled = value(
                Retained::as_ptr(&self.defaults) as Obj,
                "desktopPreview.disabled",
            );
            toggle(
                root,
                374.,
                40.,
                "Preview al pasar por un escritorio",
                target,
                62,
                !disabled,
            );
            label(
                root,
                rect(18., 12., 330., 22.),
                "Borde al reelegir el escritorio",
                12.,
                0.,
                false,
            );
            self.flash_switch = toggle(
                root,
                374.,
                12.,
                "Borde al reelegir el escritorio",
                target,
                64,
                self.flash_enabled,
            );
            self.rebuild_space_rows();
        }
    }
    fn rebuild_space_rows(&mut self) {
        if self.space_scroll.is_null() {
            return;
        }
        unsafe {
            let desktops = spaces::settings_desktops();
            self.space_ids = desktops.iter().map(|(id, _)| *id).collect();
            self.space_controls.clear();
            let viewport: NSRect = msg_send![self.space_scroll, bounds];
            let height = (desktops.len() as f64 * 52.).max(viewport.size.height);
            let document = alloc_view(class!(NSView), rect(0., 0., 415., height));
            let root = Retained::as_ptr(&document) as Obj;
            let target = Retained::as_ptr(&self.target) as Obj;
            let defaults = Retained::as_ptr(&self.defaults) as Obj;
            let names: Vec<_> = space_style::ICONS.iter().map(|(name, _)| *name).collect();
            for (index, (id, title)) in desktops.iter().enumerate() {
                let y = height - (index + 1) as f64 * 52.;
                label(root, rect(18., y + 18., 180., 22.), title, 13., 0., false);
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
            if desktops.is_empty() {
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
            let _: () = msg_send![root, scrollPoint:NSPoint::new(0., (height - viewport.size.height).max(0.))];
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
    fn record_transfer(&mut self, message: &str, feedback: bool) {
        if self.transfer_history.len() == 16 {
            self.transfer_history.pop_front();
        }
        self.transfer_history.push_back(message.to_string());
        if feedback {
            self.transfer_feedback_at = Some(Instant::now());
            self.feedback(message);
        }
    }
    fn copy_diagnostic(&self) {
        unsafe {
            let process: Obj = msg_send![class!(NSProcessInfo), processInfo];
            let os: *mut NSString = msg_send![process, operatingSystemVersionString];
            let mut report = format!(
                "AMSpace {} · {}\nmacOS: {}\nAccessibility (app): {}\nNative Space transfer: {}\nPriority requested={} active={} unavailable={:?}\nSend queued={} delivered={} expired={} busy_retries={}\nSpaces: {:?}\nVisible: {:?}\nScreens: {:?}\n",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH,
                os.as_ref().map(ToString::to_string).unwrap_or_default(),
                ax::trusted(),
                crate::window_move::space_moves_available(),
                crate::tile_manager::pref(
                    Retained::as_ptr(&self.defaults) as Obj,
                    "shortcuts.priority"
                ) != 0,
                self.shortcuts.priority_active(),
                self.shortcuts.unavailable,
                SEND_QUEUED.load(Ordering::Relaxed),
                SEND_DELIVERED.load(Ordering::Relaxed),
                SEND_EXPIRED.load(Ordering::Relaxed),
                SEND_RETRIED.load(Ordering::Relaxed),
                spaces::snapshot(),
                spaces::visible_desktops(),
                screen_configuration()
            );
            report.push_str(&format!(
                "Navigation enabled={} Mosaic enabled={}\n",
                crate::tile_manager::pref(
                    Retained::as_ptr(&self.defaults) as Obj,
                    "shortcuts.enabled"
                ) != 0,
                crate::tile_manager::pref(Retained::as_ptr(&self.defaults) as Obj, "tile.enabled")
                    != 0
            ));
            for binding in crate::shortcuts::bindings(Retained::as_ptr(&self.defaults) as Obj, true)
            {
                report.push_str(&format!(
                    "Binding {:?}: {} available={}\n",
                    binding.action,
                    crate::tile_hotkey::title(binding.key, binding.mods),
                    self.shortcuts.available(binding.action)
                ));
            }
            report.push_str("\nÚltimos traslados (sin teclas ajenas ni títulos):\n");
            for event in &self.transfer_history {
                report.push_str(event);
                report.push('\n');
            }
            report.push_str(&format!(
                "Último aviso de mosaico: {}\n",
                self.last_tiling_feedback
            ));
            report.push_str(&self.tile_manager.diagnostic_report());
            let pasteboard: Obj = msg_send![class!(NSPasteboard), generalPasteboard];
            let _: isize = msg_send![pasteboard, clearContents];
            let ok: bool = msg_send![pasteboard,setString:&*NSString::from_str(&report),forType:&*NSString::from_str("public.utf8-plain-text")];
            self.feedback(if ok {"Diagnóstico copiado. Incluye estado y geometría, sin títulos ni contenido de ventanas."} else {"No se pudo copiar el diagnóstico."});
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
            if self.transfer.is_some()
                && ((40..=87).contains(&tag) || (95..=98).contains(&tag) || tag >= 2000)
            {
                self.feedback("Espera a que termine el traslado antes de cambiar Mosaico.");
                return;
            }
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
                70..=76 => {
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        "barBackground",
                        tag - 70,
                    );
                    self.apply_bar_background();
                }
                80..=87 => {
                    let defaults = Retained::as_ptr(&self.defaults) as Obj;
                    let layout = crate::tiling::Layout::from_selection(tag - 80);
                    if !self.tile_manager.reset_template(defaults, layout) {
                        self.feedback(if crate::desktop_mode::active() {
                            "Mostrar escritorio: el mosaico está en pausa."
                        } else {
                            "La plantilla no admite estas ventanas o el escritorio ha cambiado."
                        });
                    }
                    self.refresh_tile_controls();
                }
                40 | 41 | 43..=45 => {
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
                63 => {
                    let state: isize = msg_send![sender, state];
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        "tile.animate",
                        state,
                    );
                    // The next tick re-reads the flag and picks its own cadence.
                    self.tile_manager.invalidate();
                    self.update_tile_timer();
                }
                65 => {
                    let state: isize = msg_send![sender, state];
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        "tile.groups",
                        state,
                    );
                    // Only future drops consult the flag; live trees keep their strips.
                    self.tile_manager.invalidate();
                }
                64 => {
                    let state: isize = msg_send![sender, state];
                    self.flash_enabled = state == 1;
                    if !self.flash_enabled {
                        self.hide_flash();
                    }
                }
                95..=98 => self.custom_action(tag),
                99 => self.copy_diagnostic(),
                90..=93 => {
                    let state: isize = msg_send![sender, state];
                    let key = match tag {
                        90 => "shortcuts.enabled",
                        91 => "shortcuts.priority",
                        93 => "shortcuts.stay",
                        _ => "shortcuts.enabled",
                    };
                    crate::tile_manager::save(Retained::as_ptr(&self.defaults) as Obj, key, state);
                    self.configure_shortcuts();
                }
                300..=311 => {
                    if let Some((_, key, _, _)) = self.shortcut_fields.get((tag - 300) as usize) {
                        let key = key.clone();
                        let selected: isize = msg_send![sender, indexOfSelectedItem];
                        self.edit_shortcuts(&[(&key, selected)]);
                    }
                }
                54 => self.show_picker_settings(),
                62 => {
                    let state: isize = msg_send![sender, state];
                    let _: () = msg_send![&self.defaults, setBool:state == 0, forKey:&*NSString::from_str("desktopPreview.disabled")];
                    self.desktop_preview.hide();
                    self.status_detail.clear();
                    self.refresh_spaces();
                }
                61 => {
                    let selected: isize = msg_send![sender, state];
                    crate::tile_manager::save(
                        Retained::as_ptr(&self.defaults) as Obj,
                        "tile.resize.preview",
                        selected,
                    );
                }
                55 | 56 => {
                    let selected: isize = msg_send![sender, indexOfSelectedItem];
                    self.edit_shortcuts(&[(
                        if tag == 55 {
                            "tile.picker.mods"
                        } else {
                            "tile.picker.key"
                        },
                        selected,
                    )]);
                }
                58 => self.find_picker_shortcuts(),
                59 => {}
                60 => {
                    let index: isize = msg_send![self.picker_suggestions, indexOfSelectedItem];
                    if let Some(&(key, mods)) = self.suggested_shortcuts.get(index.max(0) as usize)
                    {
                        self.edit_shortcuts(&[
                            ("tile.picker.key", key as isize),
                            ("tile.picker.mods", mods as isize),
                        ]);
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
                            "No hay una distribución compatible con estas ventanas y pantalla."
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
            self.refresh_shortcut_fields();
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
pub fn queue_window_update(focus_changed: bool) {
    with_app(|a| {
        if focus_changed
            && a.focus_target
                .is_some_and(|(_, _, since)| since.elapsed() > Duration::from_millis(250))
        {
            a.focus_target = None;
        }
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
        // NSButtonCell's modern appearance invalidates itself while rendering
        // a status-item snapshot, scheduling another snapshot indefinitely.
        // Keep the native button's AX actions, but draw the attributed title
        // directly: these segments have no bezel, hover or pressed state.
        builder.add_method(
            sel!(drawRect:),
            desktop_draw as extern "C-unwind" fn(Obj, Sel, NSRect),
        );
        builder.register()
    }
}
pub fn queue_shortcut(action: crate::tile_hotkey::Action) {
    queue_shortcut_generation(action, crate::shortcuts::generation());
}
pub fn queue_shortcut_generation(action: crate::tile_hotkey::Action, generation: u64) {
    unsafe extern "C" {
        static _dispatch_main_q: std::ffi::c_void;
        fn dispatch_async_f(
            queue: *const std::ffi::c_void,
            context: *mut std::ffi::c_void,
            callback: extern "C" fn(*mut std::ffi::c_void),
        );
        fn dispatch_after_f(
            when: u64,
            queue: *const std::ffi::c_void,
            context: *mut std::ffi::c_void,
            callback: extern "C" fn(*mut std::ffi::c_void),
        );
        fn dispatch_time(when: u64, delta: i64) -> u64;
    }
    type Delivery = (crate::tile_hotkey::Action, Instant, u64);
    extern "C" fn deliver(context: *mut std::ffi::c_void) {
        let delivery = unsafe { Box::from_raw(context.cast::<Delivery>()) };
        let (action, queued, generation) = *delivery;
        let send = matches!(action, crate::tile_hotkey::Action::SendSpace(_));
        let result = autoreleasepool(|_| {
            APP.with(|state| {
                deliver_shortcut(
                    state,
                    queued,
                    generation,
                    crate::shortcuts::generation(),
                    action,
                    |app, action| {
                        if send {
                            SEND_DELIVERED.fetch_add(1, Ordering::Relaxed);
                        }
                        app.shortcut_action(action);
                    },
                )
            })
        });
        match result {
            ShortcutDelivery::Busy => unsafe {
                if send {
                    SEND_RETRIED.fetch_add(1, Ordering::Relaxed);
                }
                // AX can run a nested loop with App already borrowed. Defer
                // rather than losing the action or spinning inside that loop.
                dispatch_after_f(
                    dispatch_time(0, 5_000_000),
                    std::ptr::addr_of!(_dispatch_main_q),
                    Box::into_raw(delivery).cast(),
                    deliver,
                );
            },
            ShortcutDelivery::Expired if send => {
                SEND_EXPIRED.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    if matches!(action, crate::tile_hotkey::Action::SendSpace(_)) {
        SEND_QUEUED.fetch_add(1, Ordering::Relaxed);
    }
    unsafe {
        dispatch_async_f(
            std::ptr::addr_of!(_dispatch_main_q),
            Box::into_raw(Box::new((action, Instant::now(), generation))).cast(),
            deliver,
        );
    }
}
extern "C-unwind" fn transfer_tick(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.step_transfer());
}
extern "C-unwind" fn settings_refresh(_: Obj, _: Sel, _: Obj) {
    with_app(|a| unsafe {
        if !a.settings.as_ref().is_some_and(|w| msg_send![w, isVisible]) {
            if let Some(timer) = a.settings_timer.take() {
                let _: () = msg_send![&timer, invalidate];
            }
            return;
        }
        let page: Obj = msg_send![a.tabs, selectedTabViewItem];
        let index: isize = msg_send![a.tabs, indexOfTabViewItem:page];
        a.refresh_permission();
        if index == 1 {
            a.refresh_spaces();
        }
        if index == 3 {
            a.refresh_shortcut_status();
        }
    });
}
extern "C-unwind" fn preview_enter(_: Obj, _: Sel, event: Obj) {
    with_app(|a| unsafe {
        a.desktop_preview.hide();
        let target = Retained::as_ptr(&a.target) as Obj;
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:sel!(showDesktopPreview:), object:ptr::null::<AnyObject>()];
        if a.sleeping
            || a.switching.is_some()
            || value(
                Retained::as_ptr(&a.defaults) as Obj,
                "desktopPreview.disabled",
            )
        {
            return;
        }
        let area: Obj = msg_send![event, trackingArea];
        let info: Obj = msg_send![area, userInfo];
        let id: u64 = msg_send![info, unsignedLongLongValue];
        let Some(index) = a.status_ids.iter().position(|s| *s == id) else {
            return;
        };
        let Some(button) = a.status_buttons.get(index) else {
            return;
        };
        let bounds: NSRect = msg_send![button, bounds];
        let local: NSRect = msg_send![button, convertRect:bounds, toView:ptr::null::<AnyObject>()];
        let window: Obj = msg_send![button, window];
        let anchor: NSRect = msg_send![window, convertRectToScreen:local];
        a.desktop_preview.pending = Some((id, anchor));
        let _: () = msg_send![target, performSelector:sel!(showDesktopPreview:), withObject:ptr::null::<AnyObject>(), afterDelay:0.3f64];
    });
}
extern "C-unwind" fn preview_exit(_: Obj, _: Sel, event: Obj) {
    with_app(|a| unsafe {
        let area: Obj = msg_send![event, trackingArea];
        let info: Obj = msg_send![area, userInfo];
        let id: u64 = msg_send![info, unsignedLongLongValue];
        if a.desktop_preview.pending.is_some_and(|p| p.0 == id) {
            a.desktop_preview.hide();
        }
    });
}
extern "C-unwind" fn preview_show(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        if a.sleeping || a.switching.is_some() || crate::desktop_mode::active() {
            a.desktop_preview.hide();
        } else {
            a.desktop_preview.show();
        }
    });
}
extern "C-unwind" fn desktop_hit_test(_: Obj, _: Sel, _: NSPoint) -> Obj {
    ptr::null_mut()
}
extern "C-unwind" fn desktop_draw(button: Obj, _: Sel, _: NSRect) {
    unsafe {
        let title: Obj = msg_send![button, attributedTitle];
        let size: NSSize = msg_send![title, size];
        let bounds: NSRect = msg_send![button, bounds];
        let origin = NSPoint::new(
            bounds.origin.x + (bounds.size.width - size.width) / 2.,
            bounds.origin.y + (bounds.size.height - size.height) / 2.,
        );
        let _: () = msg_send![title, drawAtPoint:origin];
    }
}
extern "C-unwind" fn status_click(target: Obj, _: Sel, sender: Obj) {
    with_app(|a| a.desktop_preview.hide());
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
pub fn desktop_presentation_changed(mode: crate::desktop_mode::Mode) {
    with_app(|a| {
        a.pending_focus = None;
        a.focus_target = None;
        a.layout_picker = None;
        a.hover_probe.reset();
        a.tile_manager.set_presentation(mode);
        a.update_presentation_timer();
        if mode != crate::desktop_mode::Mode::Normal {
            a.tile_mouse = None;
            unsafe {
                if let Some(timer) = a.tile_timer.take() {
                    let _: () = msg_send![&timer, invalidate];
                }
                if let Some(timer) = a.hover_timer.take() {
                    let _: () = msg_send![&timer, invalidate];
                }
            }
            ws::stop();
            a.hide_border();
        } else {
            a.hover_resume = Instant::now() + Duration::from_millis(600);
            a.configure_hover();
            a.configure_tiling();
            a.settle_focus();
            unsafe {
                let _: () = msg_send![&a.target, performSelector:sel!(resumeDesktop:), withObject:ptr::null::<AnyObject>(), afterDelay:0.65f64];
            }
        }
    });
}
extern "C-unwind" fn desktop_lifecycle(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.ensure_desktop_observer());
}
extern "C-unwind" fn presentation_tick(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| crate::desktop_mode::refresh());
}
extern "C-unwind" fn resume_desktop(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        if crate::desktop_mode::active() {
            return;
        }
        if !a
            .tile_manager
            .finish_deferred_restores(Retained::as_ptr(&a.defaults) as Obj)
        {
            unsafe {
                let _: () = msg_send![&a.target, performSelector:sel!(resumeDesktop:), withObject:ptr::null::<AnyObject>(), afterDelay:0.2f64];
            }
        }
    });
}
extern "C-unwind" fn desktop_distributed(_: Obj, _: Sel, notification: Obj) {
    unsafe {
        let name: *mut NSString = msg_send![notification, name];
        if let Some(name) = name.as_ref() {
            if let Some(mode) = crate::desktop_mode::distributed_mode(&name.to_string()) {
                crate::desktop_mode::update(mode);
            }
        }
    }
}
extern "C-unwind" fn select_tile_tab(_: Obj, _: Sel, sender: Obj) {
    let id: isize = unsafe { msg_send![sender, tag] };
    with_app(|a| {
        if a.layout_picker.is_some() || a.transfer.is_some() || a.switching.is_some() {
            return;
        }
        if let Ok(id) = u32::try_from(id)
            && a.tile_manager.activate_tab(id)
        {
            a.hover_resume = Instant::now() + Duration::from_millis(600);
            a.update_tile_timer();
            a.settle_focus();
        }
    });
}
extern "C-unwind" fn tile_tick(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| {
        with_app(|a| {
            if a.layout_picker.is_some() || a.transfer.is_some() {
                return;
            }
            if a.switching.is_some() {
                a.tile_manager.cancel_drag();
                a.update_tile_timer();
                return;
            }
            if !a.tile_manager.tick(Retained::as_ptr(&a.defaults) as Obj) {
                a.configure_tiling();
                return;
            }
            a.update_tile_timer();
            if let Some(error) = a.tile_manager.last_error.take() {
                a.last_tiling_feedback = error.clone();
                if a.transfer_feedback_at
                    .is_none_or(|at| at.elapsed() > Duration::from_secs(30))
                {
                    a.feedback(&error);
                }
            }
        })
    });
}
extern "C-unwind" fn hover(_: Obj, _: Sel, _: Obj) {
    autoreleasepool(|_| with_app(|a| a.hover()));
}
extern "C-unwind" fn resume_space_border(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        if !a.border_space_pause.pending(Instant::now()) {
            a.update_window();
        }
    });
}
extern "C-unwind" fn fade_flash(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.fade_flash());
}
extern "C-unwind" fn hide_flash(_: Obj, _: Sel, _: Obj) {
    with_app(|a| a.hide_flash());
}
extern "C-unwind" fn space_changed(target: Obj, selector: Sel, notification: Obj) {
    with_app(|a| {
        a.desktop_preview.hide();
        a.pause_border_for_space();
        a.focus_target = None;
        a.pending_focus = None;
        a.layout_picker = None;
        a.tile_manager.space_changed();
    });
    workspace(target, selector, notification);
}
extern "C-unwind" fn screens_changed(target: Obj, selector: Sel, notification: Obj) {
    let mut changed = false;
    let mut physical_change = false;
    with_app(|a| {
        let configuration = screen_configuration();
        if configuration != a.screen_configuration {
            physical_change = configuration.len() != a.screen_configuration.len()
                || configuration
                    .iter()
                    .zip(&a.screen_configuration)
                    .any(|(new, old)| {
                        new.id != old.id || new.frame != old.frame || new.scale != old.scale
                    });
            a.screen_configuration = configuration;
            changed = true;
        }
    });
    if !changed {
        return;
    }
    cancel_layout_picker();
    with_app(|a| {
        if physical_change {
            a.tile_manager.screens_changed();
        } else {
            // Dock/menu bar changes are reconciled per display by the manager.
            a.tile_manager.invalidate();
        }
    });
    workspace(target, selector, notification);
}
extern "C-unwind" fn workspace(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.ensure_desktop_observer();
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
        a.desktop_preview.hide();
        a.update_presentation_timer();
        a.configure_hover();
        a.configure_tiling();
        a.update_window();
    });
}
extern "C-unwind" fn wake(_: Obj, _: Sel, _: Obj) {
    with_app(|a| {
        a.sleeping = false;
        crate::desktop_mode::refresh();
        a.update_presentation_timer();
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
            (sel!(selectTileTab:), select_tile_tab),
            (sel!(layoutChoice:), layout_choice),
            (sel!(screensChanged:), screens_changed),
            (sel!(desktopLifecycle:), desktop_lifecycle),
            (sel!(desktopDistributed:), desktop_distributed),
            (sel!(presentationTick:), presentation_tick),
            (sel!(resumeDesktop:), resume_desktop),
            (sel!(focusPending:), focus_pending),
            (sel!(update:), update),
            (sel!(settle:), settle),
            (sel!(workspace:), workspace),
            (sel!(spaceChanged:), space_changed),
            (sel!(resumeSpaceBorder:), resume_space_border),
            (sel!(becameActive:), became_active),
            (sel!(sleep:), sleep),
            (sel!(wake:), wake),
            (sel!(quit:), quit),
            (sel!(settingsRefresh:), settings_refresh),
            (sel!(transferTick:), transfer_tick),
            (sel!(mouseEntered:), preview_enter),
            (sel!(mouseExited:), preview_exit),
            (sel!(showDesktopPreview:), preview_show),
            (sel!(fadeFlash:), fade_flash),
            (sel!(hideFlash:), hide_flash),
        ] {
            builder.add_method(s, f);
        }
        builder.add_method(
            sel!(numberOfRowsInTableView:),
            navigation_rows as extern "C-unwind" fn(Obj, Sel, Obj) -> isize,
        );
        builder.add_method(
            sel!(tableView:viewForTableColumn:row:),
            navigation_view as extern "C-unwind" fn(Obj, Sel, Obj, Obj, isize) -> Obj,
        );
        builder.add_method(
            sel!(tableViewSelectionDidChange:),
            navigation_selected as extern "C-unwind" fn(Obj, Sel, Obj),
        );
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
            "NSWorkspaceDidLaunchApplicationNotification",
            "NSWorkspaceDidTerminateApplicationNotification",
        ] {
            observe(
                center,
                Retained::as_ptr(&target) as Obj,
                name,
                sel!(desktopLifecycle:),
            );
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
        let distributed: Obj = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
        for (name, _) in crate::desktop_mode::DISTRIBUTED_NOTIFICATIONS {
            let _: () = msg_send![distributed, addObserver:&*target, selector:sel!(desktopDistributed:), name:&*NSString::from_str(name), object:ptr::null::<AnyObject>(), suspensionBehavior:4usize];
        }
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

/// Only the no-op outcome gets the soft frame: the other failures already
/// report their own text and left the layout untouched.
fn transfer_flashes(message: &str) -> bool {
    message == crate::window_move::SAME_DESTINATION
}
#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::tile_hotkey::Action;
    #[test]
    fn only_the_no_op_transfer_outcome_confirms_with_the_soft_frame() {
        assert!(transfer_flashes(crate::window_move::SAME_DESTINATION));
        assert!(!transfer_flashes("No hay una ventana activa."));
        assert!(!transfer_flashes("Esta ventana no admite traslados."));
    }
    #[test]
    fn nested_ax_borrow_defers_the_command_and_releases_it_once_available() {
        let state = RefCell::new(Some(0u32));
        let queued = Instant::now();
        let nested = state.borrow_mut();
        assert_eq!(
            deliver_shortcut(&state, queued, 7, 7, Action::SendSpace(1), |count, _| {
                *count += 1
            }),
            ShortcutDelivery::Busy
        );
        drop(nested);
        assert_eq!(
            deliver_shortcut(&state, queued, 7, 7, Action::SendSpace(1), |count, _| {
                *count += 1
            }),
            ShortcutDelivery::Done
        );
        assert_eq!(*state.borrow(), Some(1));
    }
    #[test]
    fn deferred_commands_cannot_survive_expiry_or_a_configuration_change() {
        let state = RefCell::new(Some(0u32));
        let old = Instant::now() - Duration::from_millis(400);
        assert_eq!(
            deliver_shortcut(&state, old, 7, 7, Action::SendSpace(1), |count, _| {
                *count += 1
            }),
            ShortcutDelivery::Expired
        );
        assert_eq!(
            deliver_shortcut(
                &state,
                Instant::now(),
                7,
                8,
                Action::SendSpace(1),
                |count, _| *count += 1
            ),
            ShortcutDelivery::Changed
        );
        assert_eq!(*state.borrow(), Some(0));
        assert_eq!(
            deliver_shortcut(
                &RefCell::<Option<u32>>::new(None),
                Instant::now(),
                7,
                7,
                Action::SendSpace(1),
                |count, _| *count += 1
            ),
            ShortcutDelivery::Inactive
        );
    }
}
