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

type Workspace = (u32, u64); // Display identity + native Space identity.

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
    intent: DropIntent,
    expires: Instant,
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
    constrained: HashMap<u32, (NSRect, NSRect)>,
    drag: Option<Drag>,
    pending_drop: Option<PendingDrop>,
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
        for id in self.transitions.keys() {
            self.applied.remove(id);
        }
        if !self.transitions.is_empty() || self.drag.is_some() || self.pending_drop.is_some() {
            self.invalidate();
        }
        self.transitions.clear();
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
            if !ok {
                if let Some(actual) = ws::bounds(*id) {
                    self.constrained.insert(*id, (t.to, actual));
                }
                done.push(*id);
                self.last_error = Some("Una aplicación no permite ajustar su ventana.".into());
                continue;
            }
            t.last = frame;
            if progress >= 1. {
                if let Some((p, s)) = ax::geometry(w.element.0) {
                    let actual = NSRect::new(p, s);
                    if !near(actual, t.to) {
                        self.constrained.insert(*id, (t.to, actual));
                        self.last_error=Some("Una ventana limita su tamaño; prueba otra plantilla o déjala flotante.".into());
                    } else {
                        self.constrained.remove(id);
                    }
                }
                done.push(*id);
            }
        }
        for id in done {
            self.transitions.remove(&id);
        }
    }
    fn resize_drag(&mut self, final_sample: bool) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(frame) = ws::bounds(drag.id) else {
            return;
        };
        drag.resized |= drag.resize_gesture
            && ((frame.size.width - drag.start.size.width).abs() > 3.
                || (frame.size.height - drag.start.size.height).abs() > 3.);
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
    fn current_workspace(&self) -> Option<Workspace> {
        let native = current_space()?;
        let p = pointer();
        self.active_areas
            .iter()
            .find(|(key, r)| key.1 == native && contains(*r, p))
            .or_else(|| self.active_areas.iter().find(|(key, _)| key.1 == native))
            .map(|(key, _)| *key)
    }
    pub fn reset_template(&mut self, native_space: u64, layout: Layout) {
        let Some(space) = self.current_workspace().filter(|key| key.1 == native_space) else {
            return;
        };
        if let (Some(d), Some((_, area))) = (
            self.desktops.get_mut(&space),
            self.active_areas.iter().find(|(id, _)| *id == space),
        ) {
            d.reset_template(layout, *area);
        }
        self.invalidate();
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
        save(
            defaults,
            &format!("tile.layout.{}", space.1),
            desktop.layout.index(),
        );
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
                    Layout::from_index(pref(defaults, &format!("tile.layout.{}", key.1))),
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
        if down && !self.pressed {
            self.pending_drop = None;
            self.dirty |= !self.transitions.is_empty();
            self.transitions.clear();
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
        self.pressed = down;
        if down {
            let mut preview = None;
            if let Some(drag) = &mut self.drag {
                if !active.iter().any(|(s, _)| *s == drag.space) {
                    self.cancel_drag();
                    return;
                }
                if let Some(frame) = ws::bounds(drag.id) {
                    drag.resized |= drag.resize_gesture
                        && ((frame.size.width - drag.start.size.width).abs() > 3.
                            || (frame.size.height - drag.start.size.height).abs() > 3.);
                    drag.moved |= (frame.origin.x - drag.start.origin.x).abs() > 5.
                        || (frame.origin.y - drag.start.origin.y).abs() > 5.;
                }
                if drag.moved
                    && !drag.resized
                    && (p.x - drag.point.x).abs() + (p.y - drag.point.y).abs() > 12.
                {
                    let candidate = active.iter().find_map(|(key, area)| {
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
                if let Some((destination, intent)) = drag.drop {
                    if active.iter().any(|(key, area)| {
                        *key == destination
                            && contains(
                                rect(
                                    area.origin.x - 12.,
                                    area.origin.y - 12.,
                                    area.size.width + 24.,
                                    area.size.height + 24.,
                                ),
                                p,
                            )
                    }) {
                        self.pending_drop = Some(PendingDrop {
                            id: drag.id,
                            source: drag.space,
                            destination,
                            intent,
                            expires: Instant::now() + Duration::from_millis(700),
                        });
                    }
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
                        if let Some(proposed) = d.proposed_drop(drop.id, drop.intent, *area, 10.) {
                            if drop.source != drop.destination {
                                if let Some(source) = self.desktops.get_mut(&drop.source) {
                                    source.remove(drop.id);
                                }
                            }
                            if drop.source != drop.destination {
                                if let (Some(w), Some((_, frame))) = (
                                    self.windows.get(&drop.id),
                                    proposed
                                        .frames(*area, 10.)
                                        .iter()
                                        .find(|(id, _)| *id == drop.id),
                                ) {
                                    ax::set_frame(w.element.0, *frame);
                                }
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
        let mut transferred = HashSet::new();
        // Offscreen Spaces retain their trees; closed/hidden/minimized windows on
        // visible Spaces leave the current tree and rejoin when visible again.
        let mut members: HashMap<Workspace, Vec<u32>> = HashMap::new();
        for (pid, id, frame) in candidates {
            let Some(space) = spaces::window_space(id) else {
                continue;
            };
            let Some(space) = workspace_at_frame(space, frame, &active) else {
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
            self.floating.remove(&id);
            self.applied.remove(&id);
            self.transitions.remove(&id);
            self.constrained.remove(&id);
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
            let layout = Layout::from_index(pref(defaults, &format!("tile.layout.{}", space.1)));
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
                let pending = self.transitions.get(&id).map(|t| t.to);
                let limited = self.constrained.get(&id).copied();
                if !needs_placement(frame, actual, pending, if changed { None } else { limited }) {
                    continue;
                }
                self.constrained.remove(&id);
                if transferred.contains(&id) || self.settling_space() {
                    // macOS has already animated the Space transition. Commit
                    // the destination once, rather than animating it a second time.
                    self.transitions.remove(&id);
                    if let Some(w) = self.windows.get(&id) {
                        if !ax::set_frame(w.element.0, frame) {
                            self.last_error = Some(
                                "No se pudo ajustar la ventana al escritorio de destino.".into(),
                            );
                        }
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
            }
        }
        self.windows.retain(|_, w| w.space.1 != space);
        self.invalidate();
    }
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
                intent: DropIntent::Screen(Zone::Top),
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
