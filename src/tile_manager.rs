//! Main-thread macOS adapter. Layout calculations live in `tiling`.
use crate::{
    accessibility as ax,
    native::{Obj, color, rect},
    spaces,
    tiling::{Desktop, DropIntent, Layout, Zone, ax_screen_rect, contains, drop_intent},
    window_server as ws,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSPoint, NSRect, NSString};
use std::{
    collections::{HashMap, HashSet},
    ptr,
    time::{Duration, Instant},
};

pub type Workspace = (u32, u64); // Display identity + native Space identity.

#[derive(Clone)]
pub struct PickerContext {
    pub workspace: Workspace,
    pub area: NSRect,
    pub desktop: Desktop,
}
impl PickerContext {
    pub fn proposal(&self, layout: Layout) -> Option<Desktop> {
        let mut desktop = self.desktop.clone();
        desktop.reset_template(layout, self.area);
        let frames = desktop.frames(self.area, 10.);
        frames
            .iter()
            .all(|(_, r)| r.size.width >= 120. && r.size.height >= 90.)
            .then_some(desktop)
    }
}
pub fn pref_or(defaults: Obj, key: &str, fallback: isize) -> isize {
    let exists: Obj = unsafe { msg_send![defaults,objectForKey:&*NSString::from_str(key)] };
    if exists.is_null() {
        fallback
    } else {
        pref(defaults, key)
    }
}
fn layout_key(space: Workspace) -> String {
    format!("tile.layout.{}.{}", space.1, space.0)
}
fn layout_pref(defaults: Obj, space: Workspace) -> Layout {
    Layout::from_index(pref_or(
        defaults,
        &layout_key(space),
        pref(defaults, &format!("tile.layout.{}", space.1)),
    ))
}
fn pointer_display() -> Option<u32> {
    unsafe {
        let p: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        for i in 0..count {
            let screen: Obj = msg_send![screens,objectAtIndex:i];
            let frame: NSRect = msg_send![screen, frame];
            if contains(frame, p) {
                let desc: Obj = msg_send![screen, deviceDescription];
                let id: Obj = msg_send![desc,objectForKey:&*NSString::from_str("NSScreenNumber")];
                return Some(msg_send![id, unsignedIntValue]);
            }
        }
        None
    }
}

pub fn pref(defaults: Obj, key: &str) -> isize {
    unsafe { msg_send![defaults,integerForKey:&*NSString::from_str(key)] }
}
pub fn save(defaults: Obj, key: &str, value: isize) {
    unsafe {
        let _: () = msg_send![defaults,setInteger:value,forKey:&*NSString::from_str(key)];
    }
}
pub fn grid_count(defaults: Obj, key: &str, fallback: u32, max: u32) -> u32 {
    let value = pref(defaults, key);
    if value == 0 {
        fallback
    } else {
        value.clamp(2, max as isize) as u32
    }
}
pub fn current_space() -> Option<u64> {
    let s = spaces::snapshot();
    s.active.and_then(|i| s.desktops.get(i).copied())
}
fn area(display: u32) -> Option<NSRect> {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![screens, firstObject];
        let primary: NSRect = msg_send![first, frame];
        let count: usize = msg_send![screens, count];
        for i in 0..count {
            let screen: Obj = msg_send![screens,objectAtIndex:i];
            let desc: Obj = msg_send![screen, deviceDescription];
            let num: Obj = msg_send![desc,objectForKey:&*NSString::from_str("NSScreenNumber")];
            let id: u32 = msg_send![num, unsignedIntValue];
            if id == display {
                let r: NSRect = msg_send![screen, visibleFrame];
                let r = ax_screen_rect(r, primary.size.height);
                if r.size.width <= 40. || r.size.height <= 40. {
                    return None;
                }
                return Some(rect(
                    r.origin.x + 10.,
                    r.origin.y + 10.,
                    r.size.width - 20.,
                    r.size.height - 20.,
                ));
            }
        }
        None
    }
}
fn pointer() -> NSPoint {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![screens, firstObject];
        let r: NSRect = msg_send![first, frame];
        let p: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        NSPoint::new(p.x, r.size.height - p.y)
    }
}
fn near(a: NSRect, b: NSRect) -> bool {
    (a.origin.x - b.origin.x).abs() < 3.
        && (a.origin.y - b.origin.y).abs() < 3.
        && (a.size.width - b.size.width).abs() < 3.
        && (a.size.height - b.size.height).abs() < 3.
}
struct Managed {
    pid: i32,
    space: Workspace,
    original: NSRect,
    element: ax::Owned,
}
struct Drag {
    id: u32,
    space: Workspace,
    start: NSRect,
    point: NSPoint,
    drop: Option<(Workspace, DropIntent)>,
    candidate: Option<(Workspace, DropIntent)>,
    candidate_since: Instant,
    moved: bool,
    resized: bool,
    resize_gesture: bool,
    baseline: Desktop,
    area: NSRect,
}
struct PendingDrop {
    id: u32,
    source: Workspace,
    destination: Workspace,
    intent: Option<DropIntent>,
    expires: Instant,
}
#[derive(Clone, Copy)]
struct MonitorHandoff {
    destination: Workspace,
    expires: Instant,
}
fn arrival_proposal(
    desktop: &Desktop,
    id: u32,
    intent: Option<DropIntent>,
    area: NSRect,
) -> Option<Desktop> {
    if let Some(intent) = intent {
        return desktop.proposed_drop(id, intent, area, 10.);
    }
    let mut proposed = desktop.clone();
    let mut ids = proposed.order.clone();
    if !ids.contains(&id) {
        ids.push(id);
    }
    proposed.reconcile_at(&ids, area, 10., None);
    proposed.fit(area, 10.);
    proposed
        .frames(area, 10.)
        .iter()
        .all(|(_, r)| r.size.width >= 120. && r.size.height >= 90.)
        .then_some(proposed)
}
impl Drag {
    fn observe(&mut self, frame: NSRect, pointer_workspace: Option<Workspace>) {
        // Once a move is underway, a native size change on another screen is
        // still a move. Never reinterpret it as resizing the source layout.
        if self.moved || self.resized {
            return;
        }
        let size_changed = (frame.size.width - self.start.size.width).abs() > 3.
            || (frame.size.height - self.start.size.height).abs() > 3.;
        let moved = (frame.origin.x - self.start.origin.x).abs() > 5.
            || (frame.origin.y - self.start.origin.y).abs() > 5.;
        let anchored_x = (frame.origin.x - self.start.origin.x).abs() < 3.
            || (frame.origin.x + frame.size.width - self.start.origin.x - self.start.size.width)
                .abs()
                < 3.;
        let anchored_y = (frame.origin.y - self.start.origin.y).abs() < 3.
            || (frame.origin.y + frame.size.height - self.start.origin.y - self.start.size.height)
                .abs()
                < 3.;
        self.resized = self.resize_gesture
            && pointer_workspace == Some(self.space)
            && size_changed
            && anchored_x
            && anchored_y;
        self.moved = !self.resized && moved;
    }
}
#[derive(Clone, Copy)]
struct Verification {
    target: NSRect,
    due: Instant,
    retried: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerificationStep {
    Wait,
    Complete,
    Retry,
    Limited,
}
impl Verification {
    fn new(target: NSRect) -> Self {
        Self {
            target,
            due: Instant::now() + Duration::from_millis(120),
            retried: false,
        }
    }
    fn step(&mut self, actual: NSRect, now: Instant) -> VerificationStep {
        if near(actual, self.target) {
            return VerificationStep::Complete;
        }
        if now < self.due {
            return VerificationStep::Wait;
        }
        if self.retried {
            return VerificationStep::Limited;
        }
        self.retried = true;
        self.due = now + Duration::from_millis(120);
        VerificationStep::Retry
    }
}
struct Transition {
    from: NSRect,
    to: NSRect,
    started: Instant,
    last: NSRect,
}
#[derive(Default)]
pub struct Manager {
    desktops: HashMap<Workspace, Desktop>,
    windows: HashMap<u32, Managed>,
    floating: HashSet<u32>,
    applied: HashMap<u32, NSRect>,
    transitions: HashMap<u32, Transition>,
    verifications: HashMap<u32, Verification>,
    constrained: HashMap<u32, (NSRect, NSRect)>,
    drag: Option<Drag>,
    pending_drop: Option<PendingDrop>,
    handoffs: HashMap<u32, MonitorHandoff>,
    pressed: bool,
    preview: Option<Retained<AnyObject>>,
    last_scan: Option<Instant>,
    last_area_update: Option<Instant>,
    active_areas: Vec<(Workspace, NSRect)>,
    dirty: bool,
    space_settle: Option<Instant>,
    pub last_error: Option<String>,
}
impl Manager {
    pub fn invalidate(&mut self) {
        self.dirty = true;
        self.last_scan = None;
        self.last_area_update = None;
    }
    pub fn space_changed(&mut self) {
        if !self.pressed && self.pending_drop.is_none() {
            self.cancel_drag();
        }
        self.invalidate();
        self.space_settle = Some(Instant::now() + Duration::from_millis(600));
    }
    fn settling_space(&self) -> bool {
        self.space_settle
            .is_some_and(|until| Instant::now() < until)
    }
    pub fn cancel_drag(&mut self) {
        // Cached destinations are not evidence that cancelled animations arrived.
        for id in self.transitions.keys().chain(self.verifications.keys()) {
            self.applied.remove(id);
        }
        if !self.transitions.is_empty()
            || !self.verifications.is_empty()
            || self.drag.is_some()
            || self.pending_drop.is_some()
        {
            self.invalidate();
        }
        self.transitions.clear();
        self.verifications.clear();
        self.drag = None;
        self.pending_drop = None;
        self.pressed = false;
        self.hide_preview();
    }
    pub fn restore(&mut self) {
        self.cancel_drag();
        for (id, w) in &self.windows {
            if !self.floating.contains(id) && !ax::flag(w.element.0, "AXFullScreen") {
                ax::set_frame(w.element.0, w.original);
            }
        }
        self.applied.clear();
        self.constrained.clear();
        self.windows.clear();
        self.handoffs.clear();
        self.invalidate();
    }
    fn hide_preview(&self) {
        if let Some(w) = &self.preview {
            unsafe {
                let _: () = msg_send![&**w,orderOut:ptr::null::<AnyObject>()];
            }
        }
    }
    fn show_preview(&mut self, r: NSRect) {
        unsafe {
            if self.preview.is_none() {
                let p: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
                let p: Retained<AnyObject> = msg_send![p,initWithContentRect:r,styleMask:128usize,backing:2usize,defer:false];
                let _: () = msg_send![&p,setReleasedWhenClosed:false];
                let _: () = msg_send![&p,setOpaque:false];
                let _: () = msg_send![&p,setBackgroundColor:color(0.42,0.48,0.95,0.3)];
                let _: () = msg_send![&p,setIgnoresMouseEvents:true];
                let _: () = msg_send![&p,setHasShadow:false];
                let _: () = msg_send![&p,setLevel:3isize];
                let _: () = msg_send![&p,setCollectionBehavior:((1usize<<0)|(1<<3)|(1<<6)|(1<<8))];
                self.preview = Some(p);
            }
            let screens: Obj = msg_send![class!(NSScreen), screens];
            let first: Obj = msg_send![screens, firstObject];
            let primary: NSRect = msg_send![first, frame];
            let p = self.preview.as_ref().unwrap();
            let _: () = msg_send![&**p,setFrame:rect(r.origin.x,primary.size.height-r.origin.y-r.size.height,r.size.width,r.size.height),display:true];
            let _: () = msg_send![&**p, orderFrontRegardless];
        }
    }
    pub fn toggle_float(&mut self, defaults: Obj) {
        unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            let Some(id) = ax::focused_id(pid) else {
                return;
            };
            let Some(w) = self.windows.get(&id) else {
                return;
            };
            if pref(defaults, &format!("tile.off.{}", w.space.1)) != 0 {
                return;
            }
            self.transitions.remove(&id);
            self.verifications.remove(&id);
            self.constrained.remove(&id);
            if self.floating.remove(&id) {
                if let Some(w) = self.windows.get_mut(&id) {
                    if let Some((p, s)) = ax::geometry(w.element.0) {
                        w.original = NSRect::new(p, s);
                    }
                }
                self.invalidate();
                return;
            }
            self.floating.insert(id);
            self.applied.remove(&id);
            if let Some(d) = self.desktops.get_mut(&w.space) {
                d.remove(id);
            }
            let original = w.original;
            ax::raise(w.element.0);
            self.transition(id, original);
            self.invalidate();
        }
    }
    fn can_tile_drag(&self, id: u32, space: Workspace) -> bool {
        !self.floating.contains(&id)
            && self
                .desktops
                .get(&space)
                .is_some_and(|d| d.order.contains(&id))
    }
    #[cfg(test)]
    fn place_drag(&mut self, id: u32, space: Workspace, target: u32, zone: Zone) {
        if self.can_tile_drag(id, space) {
            if let Some(desktop) = self.desktops.get_mut(&space) {
                desktop.place(id, target, zone);
            }
        }
    }
    fn transition(&mut self, id: u32, to: NSRect) {
        let from = ws::bounds(id).unwrap_or(to);
        if near(from, to) {
            self.transitions.remove(&id);
            self.verifications.remove(&id);
            return;
        }
        self.transitions.insert(
            id,
            Transition {
                from,
                to,
                started: Instant::now(),
                last: from,
            },
        );
    }
    fn animate(&mut self, active: &[(Workspace, NSRect)]) {
        let mut done = vec![];
        let now = Instant::now();
        for (id, t) in &mut self.transitions {
            let Some(w) = self.windows.get(id) else {
                done.push(*id);
                continue;
            };
            if !active.iter().any(|(space, _)| *space == w.space)
                || spaces::window_space(*id) != Some(w.space.1)
                || ax::flag(w.element.0, "AXFullScreen")
                || ax::flag(w.element.0, "AXMinimized")
            {
                done.push(*id);
                continue;
            }
            let progress = (now.duration_since(t.started).as_secs_f64() / 0.12).min(1.);
            let mut frame = crate::tiling::interpolate(t.from, t.to, progress);
            if progress < 1. {
                frame.origin.x = frame.origin.x.round();
                frame.origin.y = frame.origin.y.round();
                frame.size.width = frame.size.width.round();
                frame.size.height = frame.size.height.round();
            }
            let ok = if progress >= 1. {
                ax::set_frame(w.element.0, frame)
            } else {
                ax::set_frame_step(w.element.0, t.last, frame)
            };
            t.last = frame;
            if !ok || progress >= 1. {
                self.verifications.insert(*id, Verification::new(t.to));
                done.push(*id);
            }
        }
        for id in done {
            self.transitions.remove(&id);
        }
    }
    fn verify_placements(&mut self, active: &[(Workspace, NSRect)]) {
        let now = Instant::now();
        for id in self.verifications.keys().copied().collect::<Vec<_>>() {
            let Some(w) = self.windows.get(&id) else {
                self.verifications.remove(&id);
                continue;
            };
            let Some(actual) = ws::bounds(id) else {
                self.verifications.remove(&id);
                continue;
            };
            if self.floating.contains(&id) || !active.iter().any(|(key, _)| *key == w.space) {
                self.verifications.remove(&id);
                continue;
            }
            let verification = self.verifications.get_mut(&id).unwrap();
            match verification.step(actual, now) {
                VerificationStep::Wait => {}
                VerificationStep::Complete => {
                    self.verifications.remove(&id);
                    self.constrained.remove(&id);
                }
                VerificationStep::Retry => {
                    if spaces::window_space(id) == Some(w.space.1) {
                        ax::set_frame(w.element.0, verification.target);
                    } else {
                        self.verifications.remove(&id);
                    }
                }
                VerificationStep::Limited => {
                    self.constrained.insert(id, (verification.target, actual));
                    self.verifications.remove(&id);
                    self.last_error=Some("Una ventana limita su tamaño o posición; prueba otra plantilla o déjala flotante.".into());
                }
            }
        }
    }
    fn resize_drag(&mut self, final_sample: bool) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(frame) = ws::bounds(drag.id) else {
            return;
        };
        let pointer_workspace = (pointer_display() == Some(drag.space.0)).then_some(drag.space);
        drag.observe(frame, pointer_workspace);
        if !drag.resized
            || self.floating.contains(&drag.id)
            || spaces::window_space(drag.id) != Some(drag.space.1)
        {
            return;
        }
        let mut layout = drag.baseline.clone();
        let from = layout
            .frames(drag.area, 10.)
            .iter()
            .find(|(id, _)| *id == drag.id)
            .map(|(_, r)| *r)
            .unwrap_or(drag.start);
        layout.resize(drag.id, from, frame, drag.area, 10.);
        for (id, target) in layout.frames(drag.area, 10.) {
            // macOS owns the user's window throughout the native resize gesture.
            if id == drag.id && !final_sample {
                continue;
            }
            if ws::bounds(id).is_some_and(|actual| near(actual, target)) {
                continue;
            }
            if let Some(w) = self.windows.get(&id) {
                ax::set_frame(w.element.0, target);
                self.applied.insert(id, target);
            }
        }
        self.desktops.insert(drag.space, layout);
    }
    pub fn current_workspace(&self) -> Option<Workspace> {
        let display = pointer_display()?;
        self.active_areas
            .iter()
            .find(|(key, _)| key.0 == display)
            .map(|(key, _)| *key)
    }
    pub fn current_layout(&self, defaults: Obj) -> Layout {
        self.current_workspace()
            .map(|key| layout_pref(defaults, key))
            .unwrap_or_default()
    }
    pub fn picker_context(&self) -> Option<PickerContext> {
        let workspace = self.current_workspace()?;
        Some(PickerContext {
            workspace,
            area: self
                .active_areas
                .iter()
                .find(|(key, _)| *key == workspace)?
                .1,
            desktop: self.desktops.get(&workspace)?.clone(),
        })
    }
    pub fn apply_template(
        &mut self,
        defaults: Obj,
        context: &PickerContext,
        layout: Layout,
    ) -> bool {
        // A picker is tied to one display/Space and one window set for its entire lifetime.
        if !spaces::visible_desktops().contains(&context.workspace)
            || area(context.workspace.0) != Some(context.area)
            || pref(defaults, &format!("tile.off.{}", context.workspace.1)) != 0
            || !self
                .desktops
                .get(&context.workspace)
                .is_some_and(|d| d.order == context.desktop.order)
        {
            return false;
        }
        let Some(proposed) = context.proposal(layout) else {
            return false;
        };
        save(defaults, &layout_key(context.workspace), layout.index());
        self.desktops.insert(context.workspace, proposed);
        self.invalidate();
        true
    }
    pub fn reset_template(&mut self, defaults: Obj, layout: Layout) {
        if let Some(context) = self.picker_context() {
            self.apply_template(defaults, &context, layout);
        }
    }
    pub fn save_distribution(&self, defaults: Obj) -> bool {
        let Some(space) = self.current_workspace() else {
            return false;
        };
        let Some((_, area)) = self.active_areas.iter().find(|(s, _)| *s == space) else {
            return false;
        };
        let Some(text) = self
            .desktops
            .get(&space)
            .and_then(|d| d.save_distribution(*area))
        else {
            return false;
        };
        unsafe {
            let _: () = msg_send![defaults,setObject:&*NSString::from_str(&text),forKey:&*NSString::from_str(&format!("tile.saved.{}.{}",space.1,space.0))];
        }
        true
    }
    pub fn restore_distribution(&mut self, defaults: Obj) -> bool {
        let Some(space) = self.current_workspace() else {
            return false;
        };
        let text = unsafe {
            let mut text: *mut NSString = msg_send![defaults,stringForKey:&*NSString::from_str(&format!("tile.saved.{}.{}",space.1,space.0))];
            if text.is_null() {
                text = msg_send![defaults,stringForKey:&*NSString::from_str(&format!("tile.saved.{}",space.1))];
            }
            text.as_ref().map(|s| s.to_string())
        };
        let (Some(text), Some(desktop)) = (text, self.desktops.get_mut(&space)) else {
            return false;
        };
        if !desktop.restore_distribution(&text) {
            return false;
        }
        save(
            defaults,
            "tile.grid.enabled",
            isize::from(desktop.grid.is_some()),
        );
        if let Some((columns, rows)) = desktop.grid {
            save(defaults, "tile.grid.columns", columns as isize);
            save(defaults, "tile.grid.rows", rows as isize);
        }
        save(defaults, &layout_key(space), desktop.layout.index());
        self.invalidate();
        true
    }
    pub fn tick_interval(&self) -> f64 {
        if !self.transitions.is_empty() || self.drag.is_some() || self.pending_drop.is_some() {
            1. / 60.
        } else {
            0.08
        }
    }
    pub fn tick(&mut self, defaults: Obj) {
        if !ax::trusted() {
            self.cancel_drag();
            return;
        }
        if self
            .last_area_update
            .is_none_or(|last| last.elapsed() >= Duration::from_millis(500))
        {
            self.active_areas = spaces::visible_desktops()
                .iter()
                .filter_map(|(display, space)| {
                    if pref(defaults, &format!("tile.off.{space}")) != 0 {
                        None
                    } else {
                        area(*display).map(|r| ((*display, *space), r))
                    }
                })
                .collect();
            self.last_area_update = Some(Instant::now());
        }
        let active = self.active_areas.clone();
        for (key, _) in &active {
            self.desktops.entry(*key).or_insert_with(|| {
                Desktop::configured(
                    layout_pref(defaults, *key),
                    if pref(defaults, "tile.grid.enabled") != 0 {
                        Some((
                            grid_count(defaults, "tile.grid.columns", 120, 240),
                            grid_count(defaults, "tile.grid.rows", 6, 60),
                        ))
                    } else {
                        None
                    },
                )
            });
        }
        let down: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
        let down = down & 1 != 0;
        let p = pointer();
        let pointer_workspace = pointer_display().and_then(|display| {
            active
                .iter()
                .find(|(key, _)| key.0 == display)
                .map(|(key, _)| *key)
        });
        self.handoffs.retain(|_, handoff| {
            handoff.expires > Instant::now()
                && active.iter().any(|(key, _)| *key == handoff.destination)
        });
        if down && !self.pressed {
            self.pending_drop = None;
            self.dirty |= !self.transitions.is_empty() || !self.verifications.is_empty();
            self.transitions.clear();
            self.verifications.clear();
            // A click inside content is not a drag until native window geometry moves.
            let handle = unsafe {
                let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
                let app: Obj = msg_send![workspace, frontmostApplication];
                let pid: i32 = msg_send![app, processIdentifier];
                ax::focused_id(pid).and_then(|id| {
                    let r = ws::bounds(id)?;
                    let outer = rect(
                        r.origin.x - 12.,
                        r.origin.y - 12.,
                        r.size.width + 24.,
                        r.size.height + 24.,
                    );
                    let inner = rect(
                        r.origin.x + 12.,
                        r.origin.y + 12.,
                        r.size.width - 24.,
                        r.size.height - 24.,
                    );
                    (contains(outer, p) && !contains(inner, p)).then_some((pid, id))
                })
            };
            self.drag = handle
                .or_else(|| ws::window_at(p))
                .or_else(|| {
                    // Native resize handles extend a few points outside AX bounds.
                    self.windows
                        .iter()
                        .filter_map(|(id, w)| {
                            let r = ws::bounds(*id)?;
                            let dx = (r.origin.x - p.x)
                                .max(0.)
                                .max(p.x - r.origin.x - r.size.width);
                            let dy = (r.origin.y - p.y)
                                .max(0.)
                                .max(p.y - r.origin.y - r.size.height);
                            (dx * dx + dy * dy <= 64.).then_some((dx * dx + dy * dy, w.pid, *id))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, pid, id)| (pid, id))
                })
                .and_then(|(_, id)| {
                    self.windows.get(&id).and_then(|w| {
                        if !self.can_tile_drag(id, w.space)
                            || !active.iter().any(|(s, _)| *s == w.space)
                        {
                            return None;
                        }
                        let frame = ws::bounds(id)?;
                        let edge = (p.x - frame.origin.x)
                            .abs()
                            .min((p.x - frame.origin.x - frame.size.width).abs())
                            .min((p.y - frame.origin.y).abs())
                            .min((p.y - frame.origin.y - frame.size.height).abs());
                        Some(Drag {
                            id,
                            space: w.space,
                            start: ws::bounds(id)?,
                            point: p,
                            drop: None,
                            candidate: None,
                            candidate_since: Instant::now(),
                            moved: false,
                            resized: false,
                            resize_gesture: edge <= 8.,
                            baseline: self.desktops.get(&w.space)?.clone(),
                            area: active.iter().find(|(s, _)| *s == w.space)?.1,
                        })
                    })
                });
        }
        if down && !self.pressed {
            if let Some(drag) = &self.drag {
                self.handoffs.remove(&drag.id);
            }
        }
        self.pressed = down;
        if down {
            let mut preview = None;
            if let Some(drag) = &mut self.drag {
                if !active
                    .iter()
                    .any(|(s, r)| *s == drag.space && *r == drag.area)
                {
                    self.cancel_drag();
                    self.pressed = true; // Wait for release; never recapture halfway through a gesture.
                    return;
                }
                if let Some(frame) = ws::bounds(drag.id) {
                    drag.observe(frame, pointer_workspace);
                }
                if drag.moved
                    && !drag.resized
                    && (p.x - drag.point.x).abs() + (p.y - drag.point.y).abs() > 12.
                {
                    let candidate = active
                        .iter()
                        .filter(|(key, _)| Some(*key) == pointer_workspace)
                        .find_map(|(key, area)| {
                            let destination = self.desktops.get(key).cloned().unwrap_or_default();
                            let frames = destination.frames(*area, 10.);
                            let intent = drop_intent(
                                *area,
                                &frames,
                                drag.id,
                                p,
                                pref(defaults, "tile.simple") == 0,
                            )
                            .or_else(|| {
                                (frames.is_empty() && contains(*area, p))
                                    .then_some(DropIntent::Screen(Zone::Left))
                            })?;
                            destination.proposed_drop(drag.id, intent, *area, 10.)?;
                            Some((*key, intent))
                        });
                    if candidate != drag.candidate {
                        drag.candidate = candidate;
                        drag.candidate_since = Instant::now();
                    }
                    if candidate.is_none()
                        || drag.drop.is_none()
                        || drag.candidate_since.elapsed() >= Duration::from_millis(70)
                    {
                        drag.drop = candidate;
                    }
                    if let Some((key, intent)) = drag.drop {
                        if let Some((_, area)) = active.iter().find(|(k, _)| *k == key) {
                            let d = self.desktops.get(&key).cloned().unwrap_or_default();
                            preview = d.proposed_drop(drag.id, intent, *area, 10.).and_then(|d| {
                                d.frames(*area, 10.)
                                    .into_iter()
                                    .find(|(id, _)| *id == drag.id)
                                    .map(|(_, r)| r)
                            });
                        }
                    }
                } else {
                    drag.drop = None;
                }
            }
            if let Some(r) = preview {
                self.show_preview(r)
            } else {
                self.hide_preview()
            }
            self.resize_drag(false);
            return; // The source stays under native mouse control; neighbors follow its shared edges.
        }
        self.resize_drag(true);
        if let Some(drag) = self.drag.take() {
            self.hide_preview();
            if drag.moved && !drag.resized && !self.floating.contains(&drag.id) {
                let selected = drag
                    .drop
                    .filter(|(key, _)| Some(*key) == pointer_workspace)
                    .map(|(key, intent)| (key, Some(intent)))
                    .or_else(|| {
                        pointer_workspace
                            .filter(|key| *key != drag.space)
                            .map(|key| (key, None))
                    });
                if let Some((destination, intent)) = selected {
                    self.pending_drop = Some(PendingDrop {
                        id: drag.id,
                        source: drag.space,
                        destination,
                        intent,
                        expires: Instant::now() + Duration::from_millis(700),
                    });
                }
            }
            if drag.moved || drag.resized {
                self.dirty = true;
                self.last_scan = None;
            }
        }
        if let Some(drop) = self.pending_drop.take() {
            if Instant::now() < drop.expires && !self.floating.contains(&drop.id) {
                if spaces::window_space(drop.id) == Some(drop.destination.1) {
                    if let Some((_, area)) = active.iter().find(|(key, _)| *key == drop.destination)
                    {
                        let d = self
                            .desktops
                            .get(&drop.destination)
                            .cloned()
                            .unwrap_or_default();
                        if let Some(proposed) = arrival_proposal(&d, drop.id, drop.intent, *area) {
                            if drop.source != drop.destination {
                                if let Some(source) = self.desktops.get_mut(&drop.source) {
                                    source.remove(drop.id);
                                }
                            }
                            if drop.source != drop.destination {
                                self.transitions.remove(&drop.id);
                                self.verifications.remove(&drop.id);
                                self.applied.remove(&drop.id);
                                self.constrained.remove(&drop.id);
                                // WindowServer can still publish the source frame after AX
                                // has accepted the destination. Keep ownership stable until
                                // that native handoff settles; do not animate across displays.
                                self.handoffs.insert(
                                    drop.id,
                                    MonitorHandoff {
                                        destination: drop.destination,
                                        expires: Instant::now() + Duration::from_millis(1200),
                                    },
                                );
                            }
                            self.desktops.insert(drop.destination, proposed);
                            if let Some(w) = self.windows.get_mut(&drop.id) {
                                w.space = drop.destination;
                            }
                            self.invalidate();
                        }
                    }
                } else {
                    self.pending_drop = Some(drop);
                }
            }
        }
        // Never snap the dragged window back while macOS is publishing its new Space.
        // The bounded pending handoff keeps the fast timer alive without AX polling.
        if self.pending_drop.is_some() {
            return;
        }
        // Short, bounded reconciliation burst while macOS publishes the arriving windows.
        self.verify_placements(&active);
        self.animate(&active);
        let scan_interval = if self.settling_space() { 100 } else { 500 };
        if self
            .last_scan
            .is_some_and(|t| t.elapsed() < Duration::from_millis(scan_interval))
            && !self.dirty
        {
            return;
        }
        self.last_scan = Some(Instant::now());
        let focused = unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            ax::focused_id(pid)
        };
        let candidates = ws::tile_windows();
        let visible_ids: HashSet<_> = candidates.iter().map(|(_, id, _)| *id).collect();
        let mut transferred: HashSet<u32> = self.handoffs.keys().copied().collect();
        // Offscreen Spaces retain their trees; closed/hidden/minimized windows on
        // visible Spaces leave the current tree and rejoin when visible again.
        let mut members: HashMap<Workspace, Vec<u32>> = HashMap::new();
        for (pid, id, frame) in candidates {
            let Some(space) = spaces::window_space(id) else {
                continue;
            };
            if self
                .handoffs
                .get(&id)
                .is_some_and(|h| h.destination.1 != space)
                || self.windows.get(&id).is_some_and(|w| w.pid != pid)
            {
                self.handoffs.remove(&id);
                transferred.remove(&id);
            }
            let previous = self
                .windows
                .get(&id)
                .filter(|w| w.pid == pid)
                .map(|w| w.space);
            let retained = self.handoffs.get(&id).map(|h| h.destination).or_else(|| {
                let expected = self.applied.get(&id).copied();
                let limited = self.constrained.get(&id).map(|(_, observed)| *observed);
                (expected.is_some_and(|r| near(r, frame))
                    || limited.is_some_and(|r| near(r, frame)))
                .then_some(previous)
                .flatten()
            });
            let Some(space) = retained_workspace(space, frame, &active, retained) else {
                continue;
            };
            if self.windows.get(&id).is_some_and(|w| w.pid != pid) {
                self.windows.remove(&id);
                self.floating.remove(&id);
                self.applied.remove(&id);
            }
            if let std::collections::hash_map::Entry::Vacant(entry) = self.windows.entry(id) {
                let Some(element) = ax::window_for_id(pid, id) else {
                    continue;
                };
                if !ax::tileable(element.0) {
                    continue;
                }
                if crate::tile_rules::floats(defaults, pid) {
                    self.floating.insert(id);
                }
                entry.insert(Managed {
                    pid,
                    space,
                    original: frame,
                    element,
                });
                self.dirty = true;
            }
            let w = self.windows.get_mut(&id).unwrap();
            if w.space != space {
                if let Some(previous) = self.desktops.get_mut(&w.space) {
                    previous.remove(id);
                }
                transferred.insert(id);
                self.transitions.remove(&id);
                self.verifications.remove(&id);
                self.constrained.remove(&id);
                self.applied.remove(&id);
            }
            w.space = space;
            if ax::geometry(w.element.0).is_none() {
                continue;
            }
            if !self.floating.contains(&id) {
                members.entry(space).or_default().push(id);
            } else if !self.transitions.contains_key(&id) {
                // Floating windows are user-owned: never snap, preview or reinsert.
                w.original = frame;
            }
        }
        // Offscreen apps can take the full AX timeout per call. Use one native
        // window list, and fail closed if the compositor query is unavailable.
        let all_ids = if self.windows.keys().any(|id| !visible_ids.contains(id)) {
            ws::all_window_ids()
        } else {
            None
        };
        let gone: Vec<_> = self
            .windows
            .iter()
            .filter(|(id, w)| {
                all_ids
                    .as_ref()
                    .is_some_and(|ids| !ids.contains(&(w.pid, **id)))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(w) = self.windows.remove(&id) {
                if let Some(d) = self.desktops.get_mut(&w.space) {
                    d.remove(id);
                }
            }
            self.handoffs.remove(&id);
            self.floating.remove(&id);
            self.applied.remove(&id);
            self.transitions.remove(&id);
            self.verifications.remove(&id);
            self.constrained.remove(&id);
        }
        for (id, handoff) in &self.handoffs {
            if self
                .windows
                .get(id)
                .is_some_and(|w| w.space == handoff.destination)
                && !self.floating.contains(id)
            {
                let ids = members.entry(handoff.destination).or_default();
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        if self.settling_space() {
            // During the native transition, onscreen visibility can lag Space
            // membership. Do not erase a live desktop's ratios in that gap.
            for (id, w) in &self.windows {
                if !visible_ids.contains(id)
                    && !self.floating.contains(id)
                    && active.iter().any(|(space, _)| *space == w.space)
                    && spaces::window_space(*id) == Some(w.space.1)
                {
                    members.entry(w.space).or_default().push(*id);
                }
            }
        }
        for (space, r) in active {
            let d = self.desktops.entry(space).or_default();
            d.grid = if pref(defaults, "tile.grid.enabled") != 0 {
                Some((
                    grid_count(defaults, "tile.grid.columns", 120, 240),
                    grid_count(defaults, "tile.grid.rows", 6, 60),
                ))
            } else {
                None
            };
            let layout = layout_pref(defaults, space);
            if d.layout != layout {
                d.layout = layout;
                self.dirty = true;
            }
            let old = d.order.clone();
            let ids = members.entry(space).or_default();
            ids.sort_unstable();
            d.reconcile_at(ids, r, 10., focused);
            let changed = old != d.order || self.dirty;
            d.fit(r, 10.);
            let frames = d.frames(r, 10.);
            // Do not create unusably small tiles; retain the layout until space is available.
            if frames
                .iter()
                .any(|(_, r)| r.size.width < 80. || r.size.height < 60.)
            {
                self.last_error =
                    Some("Demasiadas ventanas para esta plantilla; usa Cuadrícula.".into());
                continue;
            }
            for (id, frame) in frames {
                if !visible_ids.contains(&id) {
                    continue;
                }
                let actual = ws::bounds(id);
                let pending = self
                    .transitions
                    .get(&id)
                    .map(|t| t.to)
                    .or_else(|| self.verifications.get(&id).map(|v| v.target));
                let limited = self.constrained.get(&id).copied();
                if !needs_placement(frame, actual, pending, if changed { None } else { limited }) {
                    continue;
                }
                self.constrained.remove(&id);
                if transferred.contains(&id) || self.settling_space() {
                    // macOS has already animated the Space transition. Commit
                    // the destination once, rather than animating it a second time.
                    self.transitions.remove(&id);
                    self.verifications.remove(&id);
                    if let Some(w) = self.windows.get(&id) {
                        ax::set_frame(w.element.0, frame);
                        self.verifications.insert(id, Verification::new(frame));
                    }
                } else {
                    self.transition(id, frame);
                }
                // Cache destinations; animation owns intermediate frames.
                self.applied.insert(id, frame);
            }
        }
        self.dirty = false;
    }
    pub fn restore_space(&mut self, space: u64) {
        for (id, w) in &self.windows {
            if w.space.1 == space
                && !self.floating.contains(id)
                && !ax::flag(w.element.0, "AXFullScreen")
            {
                ax::set_frame(w.element.0, w.original);
                self.applied.remove(id);
                self.transitions.remove(id);
                self.verifications.remove(id);
            }
        }
        self.windows.retain(|_, w| w.space.1 != space);
        self.invalidate();
    }
}

fn retained_workspace(
    native: u64,
    frame: NSRect,
    areas: &[(Workspace, NSRect)],
    retained: Option<Workspace>,
) -> Option<Workspace> {
    retained
        .filter(|key| key.1 == native && areas.iter().any(|(candidate, _)| candidate == key))
        .or_else(|| workspace_at_frame(native, frame, areas))
}

fn workspace_at_frame(
    native: u64,
    frame: NSRect,
    areas: &[(Workspace, NSRect)],
) -> Option<Workspace> {
    areas
        .iter()
        .filter(|(key, _)| key.1 == native)
        .map(|(key, r)| {
            let w = (frame.origin.x + frame.size.width).min(r.origin.x + r.size.width)
                - frame.origin.x.max(r.origin.x);
            let h = (frame.origin.y + frame.size.height).min(r.origin.y + r.size.height)
                - frame.origin.y.max(r.origin.y);
            (*key, w.max(0.) * h.max(0.))
        })
        .filter(|(_, overlap)| *overlap > 0.)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(key, _)| key)
}

/// Compare actual geometry, not just the last requested destination. A stable
/// app-imposed size limit is not retried until the target or observed frame changes.
fn needs_placement(
    target: NSRect,
    actual: Option<NSRect>,
    pending: Option<NSRect>,
    limited: Option<(NSRect, NSRect)>,
) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    !near(actual, target)
        && !pending.is_some_and(|p| near(p, target))
        && !limited
            .is_some_and(|(requested, observed)| near(requested, target) && near(observed, actual))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelling_does_not_leave_a_deferred_write_to_an_old_monitor() {
        let mut manager = Manager::default();
        let target = rect(-890., -390., 880., 1580.);
        manager.applied.insert(1, target);
        manager.verifications.insert(1, Verification::new(target));
        manager.cancel_drag();
        assert!(manager.verifications.is_empty());
        assert!(!manager.applied.contains_key(&1));
        assert!(manager.dirty);
    }

    #[test]
    fn asynchronous_geometry_is_not_mistaken_for_a_permanent_limit() {
        let target = rect(-890., -390., 880., 1580.);
        let old = rect(10., 30., 1500., 800.);
        let mut verification = Verification::new(target);
        let due = verification.due;
        assert_eq!(
            verification.step(old, due - Duration::from_millis(1)),
            VerificationStep::Wait
        );
        assert_eq!(verification.step(old, due), VerificationStep::Retry);
        assert_eq!(
            verification.step(old, due + Duration::from_millis(80)),
            VerificationStep::Wait
        );
        assert_eq!(
            verification.step(target, due + Duration::from_millis(120)),
            VerificationStep::Complete
        );
    }
    #[test]
    fn application_constraints_have_only_one_bounded_retry() {
        let mut verification = Verification::new(rect(0., 0., 400., 300.));
        let actual = rect(0., 0., 600., 500.);
        let due = verification.due;
        assert_eq!(verification.step(actual, due), VerificationStep::Retry);
        assert_eq!(
            verification.step(actual, due + Duration::from_millis(120)),
            VerificationStep::Limited
        );
        assert_eq!(
            verification.step(actual, due + Duration::from_secs(1)),
            VerificationStep::Limited
        );
    }

    fn test_drag() -> Drag {
        let area = rect(0., 30., 1600., 900.);
        let mut baseline = Desktop::default();
        baseline.reconcile(&[1, 2], area, 10.);
        Drag {
            id: 1,
            space: (1, 7),
            start: rect(10., 40., 1500., 800.),
            point: NSPoint::new(200., 45.),
            drop: None,
            candidate: None,
            candidate_since: Instant::now(),
            moved: false,
            resized: false,
            resize_gesture: true,
            baseline,
            area,
        }
    }
    #[test]
    fn crossing_to_portrait_does_not_resize_the_source_tree() {
        let mut drag = test_drag();
        let original = drag.baseline.frames(drag.area, 10.);
        drag.observe(rect(40., 60., 1500., 800.), Some((1, 7)));
        assert!(drag.moved && !drag.resized);
        drag.observe(rect(-850., -300., 850., 1300.), Some((2, 8)));
        assert!(drag.moved && !drag.resized);
        assert_eq!(drag.baseline.frames(drag.area, 10.), original);
        let mut fast = test_drag();
        fast.observe(rect(-850., -300., 850., 1300.), Some((2, 8)));
        assert!(fast.moved && !fast.resized);
    }
    #[test]
    fn genuine_resize_handles_remain_resizes() {
        for frame in [rect(10., 40., 1200., 650.), rect(110., 140., 1400., 700.)] {
            let mut drag = test_drag();
            drag.observe(frame, Some((1, 7)));
            assert!(drag.resized && !drag.moved);
        }
    }
    #[test]
    fn delayed_source_frame_cannot_reverse_a_committed_monitor_handoff() {
        let areas = vec![
            ((1, 7), rect(0., 0., 1600., 900.)),
            ((2, 7), rect(-900., -400., 900., 1600.)),
        ];
        let stale = rect(10., 30., 1500., 800.);
        assert_eq!(workspace_at_frame(7, stale, &areas), Some((1, 7)));
        assert_eq!(
            retained_workspace(7, stale, &areas, Some((2, 7))),
            Some((2, 7))
        );
        let arrived = rect(-890., -390., 880., 1580.);
        assert_eq!(retained_workspace(7, arrived, &areas, None), Some((2, 7)));
        // An app minimum wider than the portrait monitor must not change ownership.
        let limited = rect(-890., -390., 2400., 1000.);
        assert_eq!(workspace_at_frame(7, limited, &areas), Some((1, 7)));
        assert_eq!(
            retained_workspace(7, limited, &areas, Some((2, 7))),
            Some((2, 7))
        );
    }
    #[test]
    fn handoff_ownership_never_overrides_a_different_native_space_or_removed_monitor() {
        let areas = vec![
            ((1, 7), rect(0., 0., 1600., 900.)),
            ((2, 8), rect(-900., -400., 900., 1600.)),
        ];
        assert_eq!(
            retained_workspace(7, rect(10., 30., 1500., 800.), &areas, Some((2, 8))),
            Some((1, 7))
        );
        assert_eq!(
            retained_workspace(7, rect(10., 30., 1500., 800.), &areas, Some((99, 7))),
            Some((1, 7))
        );
    }
    #[test]
    fn transfer_without_a_drop_zone_uses_destination_template_and_unique_membership() {
        let area = rect(-900., -400., 900., 1600.);
        let mut dest = Desktop::configured(Layout::ThreeBands, None);
        dest.reconcile(&[2, 3], area, 10.);
        let proposed = arrival_proposal(&dest, 1, None, area).unwrap();
        assert_eq!(proposed.layout, Layout::ThreeBands);
        assert_eq!(proposed.order, vec![2, 3, 1]);
        for (_, r) in proposed.frames(area, 10.) {
            assert_eq!(r.size.width, area.size.width);
        }
        assert_eq!(dest.order, vec![2, 3]);
    }

    #[test]
    fn display_preferences_override_legacy_without_affecting_other_monitors() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!("amspace.tests.layout.{}", std::process::id()));
            let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            save(raw, "tile.layout.7", 1);
            assert_eq!(layout_pref(raw, (1, 7)), Layout::Spiral);
            assert_eq!(layout_pref(raw, (2, 7)), Layout::Spiral);
            save(raw, &layout_key((1, 7)), 4);
            assert_eq!(layout_pref(raw, (1, 7)), Layout::ThreeBands);
            assert_eq!(layout_pref(raw, (2, 7)), Layout::Spiral);
            assert_eq!(layout_pref(raw, (1, 8)), Layout::Dwindle);
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }

    #[test]
    fn picker_proposals_do_not_mutate_original_and_match_committed_model() {
        for area in [rect(0., 30., 1600., 900.), rect(-900., -400., 900., 1600.)] {
            let mut desktop = Desktop::configured(Layout::Spiral, Some((120, 6)));
            desktop.reconcile(&[1, 2, 3], area, 10.);
            let context = PickerContext {
                workspace: (1, 7),
                area,
                desktop,
            };
            let original = context.desktop.frames(area, 10.);
            for layout in Layout::ALL {
                let mut proposed = context.proposal(layout).unwrap();
                let preview = proposed.frames(area, 10.);
                proposed.reconcile(&[3, 2, 1], area, 10.);
                proposed.fit(area, 10.);
                assert_eq!(proposed.frames(area, 10.), preview);
                assert_eq!(context.desktop.frames(area, 10.), original);
            }
            assert_ne!(layout_key((1, 7)), layout_key((2, 7)));
        }
    }
    #[test]
    fn picker_rejects_templates_that_do_not_fit() {
        let area = rect(0., 0., 500., 400.);
        let mut desktop = Desktop::configured(Layout::Grid, None);
        desktop.reconcile(&[1, 2, 3, 4], area, 10.);
        let context = PickerContext {
            workspace: (1, 7),
            area,
            desktop,
        };
        assert!(context.proposal(Layout::ThreeBands).is_some());
        assert!(context.proposal(Layout::FourBands).is_none());
        assert!(context.proposal(Layout::Grid).is_some());
    }

    #[test]
    fn monitor_membership_uses_overlap_and_native_space() {
        let areas = vec![
            ((1, 7), rect(0., 30., 1600., 900.)),
            ((2, 7), rect(-900., -400., 900., 1600.)),
            ((3, 8), rect(1600., 0., 1200., 800.)),
        ];
        assert_eq!(
            workspace_at_frame(7, rect(-800., 0., 600., 500.), &areas),
            Some((2, 7))
        );
        assert_eq!(
            workspace_at_frame(7, rect(-100., 30., 600., 500.), &areas),
            Some((1, 7))
        );
        assert_eq!(
            workspace_at_frame(8, rect(1700., 30., 600., 500.), &areas),
            Some((3, 8))
        );
        assert_eq!(
            workspace_at_frame(7, rect(1700., 30., 600., 500.), &areas),
            None
        );
        assert_eq!(
            workspace_at_frame(9, rect(0., 30., 600., 500.), &areas),
            None
        );
    }
    #[test]
    fn cancelling_pending_handoff_stops_fast_ticks_and_clears_intent() {
        let mut manager = Manager {
            pending_drop: Some(PendingDrop {
                id: 1,
                source: (1, 7),
                destination: (2, 8),
                intent: Some(DropIntent::Screen(Zone::Top)),
                expires: Instant::now() + Duration::from_millis(700),
            }),
            ..Default::default()
        };
        assert_eq!(manager.tick_interval(), 1. / 60.);
        manager.cancel_drag();
        assert!(manager.pending_drop.is_none());
        assert_eq!(manager.tick_interval(), 0.08);
    }

    #[test]
    fn space_handoff_cancels_old_animation_and_has_a_bounded_fast_scan() {
        let mut manager = Manager::default();
        let from = rect(0., 0., 400., 300.);
        let to = rect(0., 0., 1000., 800.);
        manager.transitions.insert(
            1,
            Transition {
                from,
                to,
                last: from,
                started: Instant::now(),
            },
        );
        manager.space_changed();
        assert!(manager.transitions.is_empty());
        assert!(manager.settling_space());
        assert!(manager.dirty);
        manager.space_settle = Some(Instant::now() - Duration::from_millis(1));
        assert!(!manager.settling_space());
        assert_eq!(manager.tick_interval(), 0.08);
    }
    #[test]
    fn high_frequency_ticks_exist_only_during_motion() {
        let mut manager = Manager::default();
        assert_eq!(manager.tick_interval(), 0.08);
        let from = rect(0., 0., 400., 300.);
        let to = rect(0., 0., 1000., 800.);
        manager.transitions.insert(
            1,
            Transition {
                from,
                to,
                last: from,
                started: Instant::now(),
            },
        );
        assert_eq!(manager.tick_interval(), 1. / 60.);
        manager.cancel_drag();
        assert_eq!(manager.tick_interval(), 0.08);
    }
    #[test]
    fn floating_drag_never_rejoins_a_tile() {
        let mut manager = Manager::default();
        let area = rect(0., 0., 1200., 800.);
        let mut desktop = Desktop::default();
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop.remove(2);
        manager.desktops.insert((1, 7), desktop);
        manager.floating.insert(2);
        let before = manager.desktops[&(1, 7)].frames(area, 10.);
        for zone in [
            Zone::Left,
            Zone::Right,
            Zone::Top,
            Zone::Bottom,
            Zone::Center,
        ] {
            assert!(!manager.can_tile_drag(2, (1, 7)));
            manager.place_drag(2, (1, 7), 1, zone);
            assert_eq!(manager.desktops[&(1, 7)].frames(area, 10.), before);
            assert!(manager.floating.contains(&2));
        }
        manager.floating.remove(&2); // Explicit shortcut transition.
        manager
            .desktops
            .get_mut(&(1, 7))
            .unwrap()
            .reconcile(&[1, 2, 3], area, 10.);
        assert!(manager.can_tile_drag(2, (1, 7)));
    }
    #[test]
    fn interrupted_animation_invalidates_unfinished_destinations() {
        let mut manager = Manager::default();
        let from = rect(500., 50., 400., 500.);
        let to = rect(10., 50., 1000., 700.);
        manager.applied.insert(1, to);
        manager.transitions.insert(
            1,
            Transition {
                from,
                to,
                started: Instant::now(),
                last: from,
            },
        );
        manager.cancel_drag();
        assert!(!manager.applied.contains_key(&1));
        assert!(manager.dirty);
        assert!(needs_placement(to, Some(from), None, None));
        assert!(!needs_placement(to, Some(to), None, None));
        assert!(!needs_placement(to, Some(from), Some(to), None));
    }
    #[test]
    fn geometry_drift_is_repaired_but_app_limits_do_not_loop() {
        let target = rect(0., 0., 1200., 800.);
        let smaller = rect(0., 0., 800., 600.);
        assert!(needs_placement(target, Some(smaller), None, None));
        assert!(!needs_placement(
            target,
            Some(smaller),
            None,
            Some((target, smaller))
        ));
        let moved = rect(20., 20., 800., 600.);
        assert!(needs_placement(
            target,
            Some(moved),
            None,
            Some((target, smaller))
        ));
    }
}
