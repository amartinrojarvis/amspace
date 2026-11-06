//! AppKit and AX run on the main thread. Idle mode has no recurring timer.
use crate::{accessibility as ax, model, native::*, spaces, window_server as ws};
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
struct App {
    target: Retained<AnyObject>,
    status: Retained<AnyObject>,
    border: Retained<AnyObject>,
    defaults: Retained<AnyObject>,
    settings: Option<Retained<AnyObject>>,
    border_enabled: bool,
    hover_enabled: bool,
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
            let _: () = msg_send![&border, setCollectionBehavior:(1usize|16|64|256)];
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
                defaults,
                target,
                status,
                border,
                settings: None,
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
            if self.last_title != s.title {
                let _: () = msg_send![b, setTitle:&*NSString::from_str(&s.title)];
                self.last_title = s.title;
            }
            let _: () = msg_send![b, setToolTip:&*NSString::from_str(&s.detail)];
            if !self.preview.is_null() {
                text(self.preview, &self.last_title);
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
            let selected = ws::front_window(pid);
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
                let _: () = msg_send![run, addTimer:&*timer, forMode:&*NSString::from_str("NSRunLoopCommonModes")];
                self.hover_timer = Some(timer);
            }
        }
    }
    fn hover(&mut self) {
        unsafe {
            let point: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            let flags: usize = msg_send![class!(NSEvent), modifierFlags];
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
            // No focus stealing while interacting with settings, a color panel, or a menu.
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let key: Obj = msg_send![app, keyWindow];
            if !key.is_null() {
                return;
            }
            let Some((w, pid)) = ax::window_at(point.x, primary_height() - point.y) else {
                return;
            };
            let ws: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let front: Obj = msg_send![ws, frontmostApplication];
            let front_pid: i32 = msg_send![front, processIdentifier];
            if front_pid == pid && ax::focused(pid).is_some_and(|f| ax::CFEqual(f.0, w.0)) {
                return;
            }
            if ax::focus(w.0, pid) {
                let running: Obj = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier:pid];
                let _: bool = msg_send![running, activateWithOptions:0usize];
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
            let w = window(rect(0., 0., 460., 658.), 1 | 2 | 4 | 32768);
            let _: () = msg_send![&w, setTitle:&*NSString::from_str("AMSpace")];
            let _: () = msg_send![&w, setTitleVisibility:1isize];
            let _: () = msg_send![&w, setTitlebarAppearsTransparent:true];
            let _: () = msg_send![&w, setHidesOnDeactivate:false];
            let _: () = msg_send![&w, center];
            let effect = alloc_view(class!(NSVisualEffectView), rect(0., 0., 460., 658.));
            let _: () = msg_send![&effect, setMaterial:12isize];
            let _: () = msg_send![&effect, setBlendingMode:0isize];
            let _: () = msg_send![&effect, setState:1isize];
            let _: () = msg_send![&w, setContentView:&*effect];
            let root = Retained::as_ptr(&effect) as Obj;
            let target = Retained::as_ptr(&self.target) as Obj;
            label(root, rect(28., 565., 250., 42.), "amspace", 32., 0.5, false);
            label(
                root,
                rect(29., 539., 350., 22.),
                "Tu espacio. Tu foco.",
                14.,
                0.,
                true,
            );
            self.preview = label(
                root,
                rect(314., 570., 118., 28.),
                &self.last_title,
                20.,
                0.3,
                false,
            );
            let _: () = msg_send![self.preview, setAlignment:2isize];
            card(root, rect(24., 303., 412., 215.));
            label(
                root,
                rect(42., 477., 310., 22.),
                "Borde de la ventana activa",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(42., 451., 320., 20.),
                "Un contorno sutil para situarte al instante.",
                12.,
                0.,
                true,
            );
            self.border_switch = toggle(
                root,
                378.,
                478.,
                "Borde de la ventana activa",
                target,
                1,
                self.border_enabled,
            );
            label(root, rect(42., 405., 100., 22.), "Color", 13., 0., false);
            let well = alloc_view(class!(NSColorWell), rect(366., 399., 48., 30.));
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
                    rect(134. + i as f64 * 53., 401., 50., 28.),
                    "●",
                    target,
                    20 + i as isize,
                );
                let _: () = msg_send![b, setContentTintColor:color(rgb[0],rgb[1],rgb[2],1.)];
                let attributes: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
                let _: () = msg_send![&attributes, setObject:color(rgb[0],rgb[1],rgb[2],1.), forKey:&*NSString::from_str("NSColor")];
                let title: Allocated<AnyObject> = msg_send![class!(NSAttributedString), alloc];
                let title: Retained<AnyObject> = msg_send![title, initWithString:&*NSString::from_str("●"), attributes:&*attributes];
                let _: () = msg_send![b, setAttributedTitle:&*title];
                let _: () = msg_send![b, setAccessibilityLabel:&*NSString::from_str(name)];
            }
            label(root, rect(42., 353., 90., 22.), "Grosor", 13., 0., false);
            self.width_label = label(
                root,
                rect(358., 353., 58., 22.),
                &format!("{:.1} pt", self.width),
                12.,
                0.,
                true,
            );
            let slider = alloc_view(class!(NSSlider), rect(131., 352., 215., 24.));
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
                rect(42., 318., 350., 18.),
                "Esquinas suaves · sin interceptar tus clics",
                11.,
                0.,
                true,
            );
            card(root, rect(24., 200., 412., 91.));
            label(
                root,
                rect(42., 254., 310., 22.),
                "El foco sigue al cursor",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(42., 226., 350., 20.),
                "Activa una ventana al detenerte sobre ella.",
                12.,
                0.,
                true,
            );
            label(
                root,
                rect(42., 207., 350., 17.),
                "Mantén ⌥ para pausarlo. No actúa al arrastrar.",
                11.,
                0.,
                true,
            );
            self.hover_switch = toggle(
                root,
                378.,
                254.,
                "El foco sigue al cursor",
                target,
                2,
                self.hover_enabled,
            );
            card(root, rect(24., 121., 412., 67.));
            label(
                root,
                rect(42., 151., 310., 22.),
                "Abrir al iniciar sesión",
                14.,
                0.3,
                false,
            );
            label(
                root,
                rect(42., 130., 350., 18.),
                "Disponible en cuanto empieza tu día.",
                12.,
                0.,
                true,
            );
            self.login = toggle(
                root,
                378.,
                150.,
                "Abrir al iniciar sesión",
                target,
                5,
                false,
            );
            self.permission = label(root, rect(29., 89., 276., 18.), "", 11., 0., true);
            button(
                root,
                rect(312., 82., 125., 28.),
                "Accesibilidad…",
                target,
                6,
            );
            self.feedback = label(root, rect(29., 64., 405., 18.), "", 10., 0., true);
            label(
                root,
                rect(29., 25., 270., 20.),
                "AMSPACE  /  0.1.0",
                10.,
                0.3,
                true,
            );
            button(root, rect(335., 19., 103., 28.), "Salir", target, 7);
            self.settings = Some(w);
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
extern "C-unwind" fn change(_: Obj, _: Sel, sender: Obj) {
    with_app(|a| a.change(sender));
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
            (sel!(change:), change),
            (sel!(hover:), hover),
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
