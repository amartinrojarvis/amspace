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

fn centered_resize_request(
    from: NSRect,
    start: NSRect,
    press: NSPoint,
    pointer: NSPoint,
    mut native: NSRect,
    horizontal: bool,
) -> NSRect {
    let (base, span, initial, initial_span, pressed, point) = if horizontal {
        (
            from.origin.x,
            from.size.width,
            start.origin.x,
            start.size.width,
            press.x,
            pointer.x,
        )
    } else {
        (
            from.origin.y,
            from.size.height,
            start.origin.y,
            start.size.height,
            press.y,
            pointer.y,
        )
    };
    let near_start = (pressed - initial).abs();
    let near_end = (pressed - initial - initial_span).abs();
    let (origin, length) = if near_start.min(near_end) > 12. {
        (base, span)
    } else if near_start < near_end {
        let delta = point - pressed + initial - base;
        (base + delta, span - delta)
    } else {
        let delta = point - pressed + initial + initial_span - base - span;
        (base, span + delta)
    };
    if horizontal {
        native.origin.x = origin;
        native.size.width = length;
    } else {
        native.origin.y = origin;
        native.size.height = length;
    }
    native
}
fn trace_tiling() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("AMSPACE_TRACE_TILING").is_some())
}
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
        (frames.len() == desktop.order.len()).then_some(())?;
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
    point_display(pointer()?)
}
fn point_display(point: NSPoint) -> Option<u32> {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![screens, firstObject];
        if first.is_null() {
            return None;
        }
        let primary: NSRect = msg_send![first, frame];
        let p = NSPoint::new(point.x, primary.size.height - point.y);
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
        if first.is_null() {
            return None;
        }
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
fn pointer() -> Option<NSPoint> {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![screens, firstObject];
        if first.is_null() {
            return None;
        }
        let r: NSRect = msg_send![first, frame];
        let p: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        Some(NSPoint::new(p.x, r.size.height - p.y))
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
    original_area: NSRect,
    element: ax::Owned,
}
#[derive(Clone)]
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
    minimums: Option<&HashMap<u32, objc2_foundation::NSSize>>,
) -> Option<Desktop> {
    let mut proposed = if let Some(intent) = intent {
        desktop.proposed_drop(id, intent, area, 10.)?
    } else {
        let mut proposed = desktop.clone();
        let mut ids = proposed.order.clone();
        if !ids.contains(&id) {
            ids.push(id);
        }
        proposed.reconcile_at(&ids, area, 10., None);
        proposed
    };
    if let Some(minimums) = minimums {
        proposed.set_minimum_sizes(minimums);
    }
    proposed.fit(area, 10.);
    let frames = proposed.frames(area, 10.);
    (frames.len() == proposed.order.len()
        && frames
            .iter()
            .all(|(_, r)| r.size.width >= 120. && r.size.height >= 90.))
    .then_some(proposed)
}
impl Drag {
    fn update_drop(
        &mut self,
        candidate: Option<(Workspace, DropIntent)>,
        now: Instant,
        released: bool,
    ) {
        if candidate != self.candidate {
            self.candidate = candidate;
            self.candidate_since = now;
        }
        if released
            || candidate.is_none()
            || self.drop.is_none()
            || now.duration_since(self.candidate_since) >= Duration::from_millis(70)
        {
            self.drop = candidate;
        }
    }
    fn resize_proposal(
        &mut self,
        frame: NSRect,
        point: NSPoint,
        pointer_workspace: Option<Workspace>,
    ) -> Option<Desktop> {
        self.observe(frame, point, pointer_workspace);
        if !self.resized || pointer_workspace != Some(self.space) {
            return None;
        }
        // Every sample is a proposal against the immutable gesture baseline.
        // Native resizing owns the real windows until mouse-up: writing AX
        // geometry during that gesture can change the next native delta.
        let mut layout = self.baseline.clone();
        let from = layout
            .frames(self.area, 10.)
            .into_iter()
            .find(|(id, _)| *id == self.id)?
            .1;
        let requested = if layout.order.first() == Some(&self.id) {
            layout.layout.primary_center_axis().map_or(frame, |axis| {
                centered_resize_request(from, self.start, self.point, point, frame, axis)
            })
        } else {
            frame
        };
        layout.resize(self.id, from, requested, self.area, 10.);
        (layout.frames(self.area, 10.).len() == layout.order.len()).then_some(layout)
    }
    fn observe(&mut self, frame: NSRect, point: NSPoint, pointer_workspace: Option<Workspace>) {
        // Once a move is underway, a native size change on another screen is
        // still a move. Never reinterpret it as resizing the source layout.
        if self.moved || self.resized {
            return;
        }
        // Activation and delayed AX writes can change geometry under a stationary
        // click. A native gesture requires actual pointer movement as well.
        if (point.x - self.point.x)
            .abs()
            .max((point.y - self.point.y).abs())
            <= 3.
        {
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
fn resolve_gesture(
    selected: &mut Option<Drag>,
    candidates: &mut [Drag],
    point: NSPoint,
    workspace: Option<Workspace>,
    bounds: impl Fn(u32) -> Option<NSRect>,
) {
    if let Some(drag) = selected.as_mut() {
        if let Some(frame) = bounds(drag.id) {
            drag.observe(frame, point, workspace);
        }
    }
    if selected.as_ref().is_some_and(|d| d.moved || d.resized) {
        return;
    }
    for candidate in candidates.iter_mut() {
        if let Some(frame) = bounds(candidate.id) {
            candidate.observe(frame, point, workspace);
        }
    }
    if let Some(candidate) = candidates.iter().find(|d| d.moved || d.resized) {
        *selected = Some(candidate.clone());
    }
}
#[derive(Clone, Copy)]
struct Verification {
    target: NSRect,
    due: Instant,
    retried: bool,
    accepted: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerificationStep {
    Wait,
    Complete,
    Retry,
    Limited,
    Rejected,
}
impl Verification {
    fn new(target: NSRect) -> Self {
        Self {
            target,
            due: Instant::now() + Duration::from_millis(120),
            retried: false,
            accepted: false,
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
            return if self.accepted {
                VerificationStep::Limited
            } else {
                VerificationStep::Rejected
            };
        }
        self.retried = true;
        self.due = now + Duration::from_millis(120);
        VerificationStep::Retry
    }
}
struct Transition {
    _geometry_session: Option<std::rc::Rc<ax::GeometrySession>>,
    from: NSRect,
    to: NSRect,
    started: Option<Instant>,
    last: NSRect,
}
impl Transition {
    fn sample(&mut self, now: Instant) -> (NSRect, bool) {
        // Preparing AX sessions for several apps can take time. Start the batch
        // on its first frame, so late enumeration cannot consume the animation.
        let started = *self.started.get_or_insert(now);
        let progress = (now.duration_since(started).as_secs_f64() / 0.16).min(1.);
        (
            ax::integral_frame(crate::tiling::interpolate(self.from, self.to, progress)),
            progress >= 1.,
        )
    }
    fn record_write(&mut self, frame: NSRect, accepted: bool) {
        // A failed intermediate write must be retried, not cached as delivered.
        if accepted {
            self.last = frame;
        }
    }
}
#[derive(Default)]
struct MembershipGrace {
    since: HashMap<u32, Instant>,
}
impl MembershipGrace {
    fn retained(&mut self, uncertain: &HashSet<u32>, now: Instant) -> HashSet<u32> {
        self.since.retain(|id, _| uncertain.contains(id));
        uncertain
            .iter()
            .copied()
            .filter(|id| {
                let since = self.since.entry(*id).or_insert(now);
                now.duration_since(*since) < Duration::from_millis(650)
            })
            .collect()
    }
}
#[derive(Default)]
pub struct Manager {
    desktops: HashMap<Workspace, Desktop>,
    windows: HashMap<u32, Managed>,
    floating: HashSet<u32>,
    tab_groups: HashMap<u32, ax::Owned>,
    inactive_tabs: HashSet<u32>,
    applied: HashMap<u32, NSRect>,
    transitions: HashMap<u32, Transition>,
    verifications: HashMap<u32, Verification>,
    constrained: HashMap<u32, (NSRect, NSRect)>,
    minimum_sizes: HashMap<u32, objc2_foundation::NSSize>,
    membership_grace: MembershipGrace,
    presentation: crate::desktop_mode::Mode,
    desktop_baseline: Option<HashSet<(i32, u32)>>,
    desktop_floating: HashSet<(i32, u32)>,
    last_visible_windows: Option<HashSet<(i32, u32)>>,
    restore_after_presentation: bool,
    restore_spaces_after_presentation: HashSet<u64>,
    drag: Option<Drag>,
    gesture_candidates: Vec<Drag>,
    pending_drop: Option<PendingDrop>,
    handoffs: HashMap<u32, MonitorHandoff>,
    pressed: bool,
    preview: Vec<Retained<AnyObject>>,
    last_scan: Option<Instant>,
    last_area_update: Option<Instant>,
    active_areas: Vec<(Workspace, NSRect)>,
    dirty: bool,
    space_settle: Option<Instant>,
    space_quiet_until: Option<Instant>,
    pub last_error: Option<String>,
}
impl Manager {
    fn presentation_blocks_input(&mut self) -> bool {
        if crate::desktop_mode::active() || self.presentation != crate::desktop_mode::Mode::Normal {
            return true;
        }
        crate::desktop_mode::refresh();
        if crate::desktop_mode::active() {
            self.set_presentation(crate::desktop_mode::current());
            return true;
        }
        false
    }
    pub fn finish_deferred_restores(&mut self, defaults: Obj) -> bool {
        self.set_presentation(crate::desktop_mode::current());
        if self.presentation != crate::desktop_mode::Mode::Normal {
            return false;
        }
        let all = std::mem::take(&mut self.restore_after_presentation);
        let spaces = std::mem::take(&mut self.restore_spaces_after_presentation);
        if all && pref(defaults, "tile.enabled") == 0 {
            self.restore();
        }
        for space in spaces {
            if pref(defaults, &format!("tile.off.{space}")) != 0 {
                self.restore_space(space);
            }
        }
        true
    }
    pub fn set_presentation(&mut self, mode: crate::desktop_mode::Mode) {
        if self.presentation == mode {
            return;
        }
        let snapshot = if mode == crate::desktop_mode::Mode::Desktop
            || self.presentation == crate::desktop_mode::Mode::Desktop
        {
            ws::all_window_ids()
        } else {
            None
        };
        self.set_presentation_at(mode, Instant::now(), snapshot);
        if mode == crate::desktop_mode::Mode::Desktop
            && let Some(visible) = ws::visible_normal_window_ids()
        {
            self.remember_late_desktop_windows(&visible);
        }
    }
    fn remember_late_desktop_windows(&mut self, visible: &HashSet<(i32, u32)>) {
        if let Some(before) = &self.last_visible_windows {
            self.desktop_floating.extend(
                visible.difference(before).copied().filter(|(pid, id)| {
                    self.windows.get(id).is_none_or(|window| window.pid != *pid)
                }),
            );
        }
    }
    fn set_presentation_at(
        &mut self,
        mode: crate::desktop_mode::Mode,
        now: Instant,
        snapshot: Option<HashSet<(i32, u32)>>,
    ) {
        use crate::desktop_mode::Mode;
        if self.presentation == mode {
            return;
        }
        if self.presentation == Mode::Desktop {
            let Some(after) = snapshot.as_ref() else {
                return;
            };
            let before = self.desktop_baseline.take().unwrap_or_else(|| {
                self.windows
                    .iter()
                    .map(|(id, w)| (w.pid, *id))
                    .chain(
                        self.last_visible_windows
                            .iter()
                            .flat_map(|ids| ids.iter().copied()),
                    )
                    .collect()
            });
            self.desktop_floating
                .extend(after.difference(&before).copied());
        }
        if mode == Mode::Desktop {
            if let Some(snapshot) = &snapshot {
                self.desktop_floating.retain(|key| snapshot.contains(key));
            }
            self.desktop_baseline = snapshot;
        }
        let was_paused = self.presentation != Mode::Normal;
        self.presentation = mode;
        if !was_paused {
            self.cancel_drag(); // Cancels writes/previews, preserving trees and originals.
        } else if mode == Mode::Normal {
            // Let macOS put the parked windows back before consulting geometry.
            self.space_changed_with_visible_at(now, &[]);
        }
    }
    pub fn invalidate(&mut self) {
        self.dirty = true;
        self.last_scan = None;
        self.last_area_update = None;
    }
    pub fn screens_changed(&mut self) {
        // Coordinates and app-imposed limits belong to the previous screen setup.
        // Do not let a deferred write run before the next membership scan.
        let pressed = self.pressed;
        self.cancel_drag();
        self.pressed = pressed; // Never recapture a gesture already in progress.
        self.handoffs.clear();
        self.applied.clear();
        self.constrained.clear();
        self.invalidate();
        self.space_settle = Some(Instant::now() + Duration::from_millis(600));
    }
    pub fn space_changed(&mut self) {
        self.space_changed_with_visible_at(Instant::now(), &spaces::visible_desktops());
    }
    #[cfg(test)]
    fn space_changed_at(&mut self, now: Instant) {
        self.space_changed_with_visible_at(now, &[]);
    }
    fn space_changed_with_visible_at(&mut self, now: Instant, visible: &[Workspace]) {
        self.cancel_inactive_placements(visible);
        if !self.pressed && self.pending_drop.is_none() {
            self.drag = None;
        }
        self.hide_preview();
        self.invalidate();
        // During the native slide both visibility and global bounds are transient.
        // Retain the trees without sampling/reconciling or writing any geometry.
        let until = now + Duration::from_millis(600);
        self.space_quiet_until = Some(until);
        self.space_settle = Some(until + Duration::from_millis(600));
        self.membership_grace.since.clear();
        if let Some(drop) = &mut self.pending_drop {
            drop.expires = drop.expires.max(until + Duration::from_millis(700));
        }
        for handoff in self.handoffs.values_mut() {
            handoff.expires = handoff.expires.max(until + Duration::from_millis(1200));
        }
    }
    fn cancel_inactive_placements(&mut self, visible: &[Workspace]) {
        let cancelled: Vec<_> = self
            .transitions
            .keys()
            .chain(self.verifications.keys())
            .copied()
            .filter(|id| {
                self.windows
                    .get(id)
                    .is_none_or(|w| !visible.contains(&w.space))
            })
            .collect();
        for id in cancelled {
            self.transitions.remove(&id);
            self.verifications.remove(&id);
            self.applied.remove(&id);
        }
    }
    fn waiting_for_space(&mut self, now: Instant) -> bool {
        if let Some(until) = self.space_quiet_until {
            if now < until {
                return true;
            }
            self.space_quiet_until = None;
            self.invalidate(); // Never resume from pre-transition display/Space data.
        }
        false
    }
    fn settling_space(&self) -> bool {
        self.space_settle
            .is_some_and(|until| Instant::now() < until)
    }
    fn cancel_placements(&mut self) {
        // Cached destinations are not evidence that cancelled animations arrived.
        for id in self.transitions.keys().chain(self.verifications.keys()) {
            self.applied.remove(id);
        }
        if !self.transitions.is_empty() || !self.verifications.is_empty() {
            self.invalidate();
        }
        self.transitions.clear();
        self.verifications.clear();
    }
    pub fn cancel_drag(&mut self) {
        self.cancel_placements();
        if self.drag.is_some() || self.pending_drop.is_some() {
            self.invalidate();
        }
        self.drag = None;
        self.gesture_candidates.clear();
        self.pending_drop = None;
        self.pressed = false;
        self.hide_preview();
    }
    pub fn restore(&mut self) {
        if self.presentation_blocks_input() {
            self.restore_after_presentation = true;
            return;
        }
        self.cancel_drag();
        for (id, w) in &self.windows {
            if !self.inactive_tabs.contains(id)
                && !self.floating.contains(id)
                && !ax::flag(w.element.0, "AXFullScreen")
            {
                if let Some(current) = area(w.space.0) {
                    ax::set_frame(
                        w.element.0,
                        restoration_frame(w.original, w.original_area, current),
                    );
                }
            }
        }
        self.applied.clear();
        self.constrained.clear();
        self.windows.clear();
        self.tab_groups.clear();
        self.inactive_tabs.clear();
        self.handoffs.clear();
        self.membership_grace.since.clear();
        self.invalidate();
    }
    fn hide_preview(&self) {
        for w in &self.preview {
            unsafe {
                let _: () = msg_send![&**w,orderOut:ptr::null::<AnyObject>()];
            }
        }
    }
    fn show_preview(&mut self, r: NSRect) {
        self.show_previews(&[r]);
    }
    fn show_previews(&mut self, frames: &[NSRect]) {
        unsafe {
            while self.preview.len() < frames.len() {
                let p: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
                let p: Retained<AnyObject> = msg_send![p,initWithContentRect:rect(0.,0.,1.,1.),styleMask:128usize,backing:2usize,defer:false];
                let _: () = msg_send![&p,setReleasedWhenClosed:false];
                let _: () = msg_send![&p,setOpaque:false];
                let _: () = msg_send![&p,setBackgroundColor:color(0.42,0.48,0.95,0.3)];
                let _: () = msg_send![&p,setIgnoresMouseEvents:true];
                let _: () = msg_send![&p,setHasShadow:false];
                let _: () = msg_send![&p,setLevel:3isize];
                let _: () = msg_send![&p,setCollectionBehavior:((1usize<<0)|(1<<3)|(1<<6)|(1<<8))];
                self.preview.push(p);
            }
            let screens: Obj = msg_send![class!(NSScreen), screens];
            let first: Obj = msg_send![screens, firstObject];
            if first.is_null() {
                self.hide_preview();
                return;
            }
            let primary: NSRect = msg_send![first, frame];
            for (index, p) in self.preview.iter().enumerate() {
                if let Some(r) = frames.get(index) {
                    let target = rect(
                        r.origin.x,
                        primary.size.height - r.origin.y - r.size.height,
                        r.size.width,
                        r.size.height,
                    );
                    let current: NSRect = msg_send![&**p, frame];
                    if current != target {
                        let _: () = msg_send![&**p,setFrame:target,display:true];
                    }
                    let visible: bool = msg_send![&**p, isVisible];
                    if !visible {
                        let _: () = msg_send![&**p, orderFrontRegardless];
                    }
                } else {
                    let _: () = msg_send![&**p,orderOut:ptr::null::<AnyObject>()];
                }
            }
        }
    }
    pub fn toggle_float(&mut self, defaults: Obj) {
        if self.presentation_blocks_input() {
            return;
        }
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
                        if let Some(current) = area(w.space.0) {
                            w.original_area = current;
                        }
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
            let Some(current) = area(w.space.0) else {
                return;
            };
            let original = restoration_frame(w.original, w.original_area, current);
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
    fn inherit_tab_slot(&mut self, old: u32, new: u32) {
        let Some(source) = self.windows.get(&old) else {
            return;
        };
        let (space, original, original_area) =
            (source.space, source.original, source.original_area);
        let floating = self.floating.contains(&old);
        if !floating
            && !self
                .desktops
                .get_mut(&space)
                .is_some_and(|d| d.replace_window(old, new))
        {
            return;
        }
        self.transitions.remove(&old);
        self.verifications.remove(&old);
        self.transitions.remove(&new);
        self.verifications.remove(&new);
        if let Some(target) = self.applied.remove(&old) {
            self.applied.insert(new, target);
        }
        if let Some(limit) = self.constrained.remove(&old) {
            self.constrained.insert(new, limit);
        }
        if let Some(minimum) = self.minimum_sizes.remove(&old) {
            self.minimum_sizes.insert(new, minimum);
        }
        if floating {
            self.floating.insert(new);
        } else {
            self.floating.remove(&new);
        }
        if let Some(destination) = self.windows.get_mut(&new) {
            destination.original = original;
            destination.original_area = original_area;
        }
        self.inactive_tabs.insert(old);
        self.inactive_tabs.remove(&new);
        self.membership_grace.since.remove(&old);
    }
    fn needs_placement(&self, id: u32, target: NSRect, actual: Option<NSRect>) -> bool {
        let pending = self
            .transitions
            .get(&id)
            .map(|t| t.to)
            .or_else(|| self.verifications.get(&id).map(|v| v.target));
        // A focus/application notification requests a scan, not a new placement.
        // Limits expire when the target or observed geometry changes, never just
        // because `dirty` was set by a workspace callback.
        needs_placement(target, actual, pending, self.constrained.get(&id).copied())
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
        self.transition_from(id, from, to);
    }
    fn transition_from(&mut self, id: u32, from: NSRect, to: NSRect) {
        // A retry for an older destination must not fight the new animation.
        self.verifications.remove(&id);
        if near(from, to) {
            self.transitions.remove(&id);
            return;
        }
        self.transitions.insert(
            id,
            Transition {
                _geometry_session: self
                    .windows
                    .get(&id)
                    .and_then(|w| ax::GeometrySession::shared(w.element.0)),
                from,
                to,
                started: None,
                last: from,
            },
        );
    }
    fn animate(&mut self, active: &[(Workspace, NSRect)]) {
        if self.transitions.is_empty() {
            return;
        }
        // One compositor snapshot replaces synchronous AX flag reads per frame/window.
        let Some(candidates) = ws::try_tile_windows() else {
            return;
        };
        let visible: HashSet<_> = candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect();
        let mut done = vec![];
        let now = Instant::now();
        for (id, t) in &mut self.transitions {
            let Some(w) = self.windows.get(id) else {
                done.push(*id);
                continue;
            };
            if !active.iter().any(|(space, _)| *space == w.space)
                || spaces::window_space(*id) != Some(w.space.1)
                || !visible.contains(&(w.pid, *id))
            {
                done.push(*id);
                continue;
            }
            let (frame, complete) = t.sample(now);
            let accepted = if complete {
                ax::set_frame(w.element.0, frame)
            } else {
                ax::set_frame_step(w.element.0, t.last, frame)
            };
            t.record_write(frame, accepted);
            // A rejected intermediate sample must not abandon the final destination.
            if complete {
                let mut verification = Verification::new(t.to);
                verification.accepted = accepted;
                self.verifications.insert(*id, verification);
                done.push(*id);
            }
        }
        for id in done {
            self.transitions.remove(&id);
        }
    }
    fn verify_placements(&mut self, active: &[(Workspace, NSRect)]) {
        if self.verifications.is_empty() {
            return;
        }
        let Some(candidates) = ws::try_tile_windows() else {
            return;
        };
        let visible: HashSet<_> = candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect();
        let now = Instant::now();
        for id in self.verifications.keys().copied().collect::<Vec<_>>() {
            let Some(w) = self.windows.get(&id) else {
                self.verifications.remove(&id);
                continue;
            };
            if self.floating.contains(&id)
                || !active.iter().any(|(key, _)| *key == w.space)
                || spaces::window_space(id) != Some(w.space.1)
                || !visible.contains(&(w.pid, id))
            {
                self.verifications.remove(&id);
                continue;
            }
            let Some((position, size)) = ax::geometry(w.element.0) else {
                continue;
            };
            let actual = NSRect::new(position, size);
            let verification = self.verifications.get_mut(&id).unwrap();
            match verification.step(actual, now) {
                VerificationStep::Wait => {}
                VerificationStep::Complete => {
                    self.verifications.remove(&id);
                    self.constrained.remove(&id);
                }
                VerificationStep::Retry => {
                    // Electron and permission changes can invalidate a retained
                    // AX proxy while the compositor identity stays unchanged.
                    let fresh = ax::window_for_id(w.pid, id);
                    let element = fresh.as_ref().map_or(w.element.0, |w| w.0);
                    verification.accepted = ax::set_frame(element, verification.target);
                    if let Some(fresh) = fresh {
                        self.windows.get_mut(&id).unwrap().element = fresh;
                    }
                }
                VerificationStep::Rejected => {
                    // A transport/permission failure is not an app-imposed
                    // size constraint. Let the next scan retry placement.
                    self.applied.remove(&id);
                    self.verifications.remove(&id);
                }
                VerificationStep::Limited => {
                    if trace_tiling() {
                        eprintln!(
                            "tiling limit: id={id} requested={:?} actual={actual:?}",
                            verification.target
                        );
                    }
                    let minimum = self
                        .minimum_sizes
                        .entry(id)
                        .or_insert(objc2_foundation::NSSize::new(80., 60.));
                    if actual.size.width > verification.target.size.width + 2. {
                        minimum.width = minimum.width.max(actual.size.width);
                    }
                    if actual.size.height > verification.target.size.height + 2. {
                        minimum.height = minimum.height.max(actual.size.height);
                    }
                    self.constrained.insert(id, (verification.target, actual));
                    self.verifications.remove(&id);
                    self.last_error=Some("Una ventana limita su tamaño o posición; prueba otra plantilla o déjala flotante.".into());
                }
            }
        }
    }
    fn resize_drag(
        &mut self,
        final_sample: bool,
        point: NSPoint,
        pointer_workspace: Option<Workspace>,
        preview: bool,
    ) {
        if !final_sample && !preview {
            // No overlay means no per-frame layout/AX work during a native
            // resize. The gesture is still observed by the shared input path.
            self.hide_preview();
            return;
        }
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(frame) = ws::bounds(drag.id) else {
            return;
        };
        if self.floating.contains(&drag.id) || spaces::window_space(drag.id) != Some(drag.space.1) {
            return;
        }
        let Some(layout) = drag.resize_proposal(frame, point, pointer_workspace) else {
            return;
        };
        if !final_sample {
            let frames = layout
                .frames(drag.area, 10.)
                .into_iter()
                .map(|(_, r)| r)
                .collect::<Vec<_>>();
            self.show_previews(&frames);
            return;
        }
        let space = drag.space;
        for id in &layout.order {
            self.cancel_window_placement(*id);
        }
        // Use the normal bounded placement/verification path after release;
        // never mark an unverified AX request as an applied native frame.
        self.desktops.insert(space, layout);
        self.invalidate();
    }
    pub fn current_workspace(&self) -> Option<Workspace> {
        if self.space_quiet_until.is_some()
            || crate::desktop_mode::active()
            || self.presentation != crate::desktop_mode::Mode::Normal
        {
            return None; // Cached display/Space data still belongs to the transition.
        }
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
        if self.space_quiet_until.is_some() || self.presentation_blocks_input() {
            return false;
        }
        // A picker is tied to one display/Space and one window set for its entire lifetime.
        if self.space_quiet_until.is_some()
            || !spaces::visible_desktops().contains(&context.workspace)
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
        for id in &context.desktop.order {
            self.cancel_window_placement(*id);
        }
        save(defaults, &layout_key(context.workspace), layout.index());
        if trace_tiling() {
            eprintln!(
                "tiling apply: workspace={:?} layout={layout:?} frames={:?}",
                context.workspace,
                proposed.frames(context.area, 10.)
            );
        }
        self.desktops.insert(context.workspace, proposed);
        self.invalidate();
        true
    }
    pub fn reset_template(&mut self, defaults: Obj, layout: Layout) -> bool {
        if let Some(context) = self.picker_context() {
            self.apply_template(defaults, &context, layout)
        } else {
            false
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
        let ids = desktop.order.clone();
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
        for id in ids {
            self.cancel_window_placement(id);
        }
        self.invalidate();
        true
    }
    pub fn tick_interval(&self) -> f64 {
        if self
            .space_quiet_until
            .is_some_and(|until| Instant::now() < until)
        {
            0.08
        } else if !self.transitions.is_empty()
            || self.drag.is_some()
            || !self.gesture_candidates.is_empty()
            || self.pending_drop.is_some()
        {
            1. / 60.
        } else {
            0.08
        }
    }
    /// False tells the UI to stop its timer after permission is revoked.
    pub fn tick(&mut self, defaults: Obj) -> bool {
        self.set_presentation(crate::desktop_mode::current());
        if self.presentation != crate::desktop_mode::Mode::Normal {
            return true;
        }
        if !ax::trusted() {
            self.cancel_drag();
            return false;
        }
        self.tick_mode(defaults, true);
        true
    }
    pub fn refresh_for_picker(&mut self, defaults: Obj) {
        if self.presentation_blocks_input() {
            return;
        }
        self.cancel_drag();
        self.invalidate();
        if ax::trusted() {
            self.tick_mode(defaults, false);
        }
    }
    fn tick_mode(&mut self, defaults: Obj, place_windows: bool) {
        self.tick_mode_input(defaults, place_windows, None);
    }
    pub fn mouse_event(&mut self, defaults: Obj, down: bool, point: NSPoint) {
        if self.presentation_blocks_input() || !ax::trusted() {
            return;
        }
        self.tick_mode_input(defaults, true, Some((down, point)));
    }
    fn tick_mode_input(
        &mut self,
        defaults: Obj,
        place_windows: bool,
        input: Option<(bool, NSPoint)>,
    ) {
        if crate::desktop_mode::active() || self.presentation != crate::desktop_mode::Mode::Normal {
            return;
        }
        let resuming_space = self.space_quiet_until.is_some();
        if self.waiting_for_space(Instant::now()) {
            return;
        }
        let down: usize = input.map_or_else(
            || unsafe { msg_send![class!(NSEvent), pressedMouseButtons] },
            |(down, _)| usize::from(down),
        );
        let scan_interval = if self.settling_space() { 100 } else { 500 };
        if down & 1 != 0
            || self.pressed
            || self.pending_drop.is_some()
            || !self.transitions.is_empty()
            || !self.verifications.is_empty()
            || self.dirty
            || self
                .last_area_update
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(500))
            || self
                .last_scan
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(scan_interval))
        {
            crate::desktop_mode::refresh();
            if crate::desktop_mode::active() {
                return;
            }
        }
        if self
            .last_area_update
            .is_none_or(|last| last.elapsed() >= Duration::from_millis(500))
        {
            let areas: Vec<_> = spaces::visible_desktops()
                .iter()
                .filter_map(|(display, space)| {
                    if pref(defaults, &format!("tile.off.{space}")) != 0 {
                        None
                    } else {
                        area(*display).map(|r| ((*display, *space), r))
                    }
                })
                .collect();
            // Notifications may arrive late or be skipped during native reentry.
            // A changed visible Space also starts the quiet period before writes.
            if !resuming_space && desktop_keys_changed(&self.active_areas, &areas) {
                self.space_changed();
                return;
            }
            // visibleFrame can change with the Dock/menu bar without a display
            // notification. Invalidate old destinations before animation/retries.
            if areas_changed(&self.active_areas, &areas) {
                // A Dock/menu bar change on one display does not invalidate
                // placements or learned constraints on the other displays.
                let changed: HashSet<_> = self
                    .active_areas
                    .iter()
                    .filter_map(|(key, old)| {
                        areas
                            .iter()
                            .any(|(candidate, updated)| candidate == key && updated != old)
                            .then_some(*key)
                    })
                    .collect();
                let keep: Vec<_> = areas
                    .iter()
                    .map(|(key, _)| *key)
                    .filter(|key| !changed.contains(key))
                    .collect();
                self.cancel_inactive_placements(&keep);
                self.applied.retain(|id, _| {
                    self.windows
                        .get(id)
                        .is_some_and(|w| !changed.contains(&w.space))
                });
                self.constrained.retain(|id, _| {
                    self.windows
                        .get(id)
                        .is_some_and(|w| !changed.contains(&w.space))
                });
                self.invalidate();
            }
            // The notification can precede the native Space update. Check again
            // after the quiet period, before resuming any deferred geometry.
            self.cancel_inactive_placements(&areas.iter().map(|(key, _)| *key).collect::<Vec<_>>());
            self.active_areas = areas;
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
        let down = place_windows && down & 1 != 0;
        let Some(p) = input.map(|(_, p)| p).or_else(pointer) else {
            self.cancel_drag();
            return;
        };
        // Display hit-testing matters only while capturing/releasing a gesture.
        let pointer_workspace =
            (down || self.drag.is_some() || !self.gesture_candidates.is_empty())
                .then(|| point_display(p))
                .flatten()
                .and_then(|display| {
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
            self.drag = ws::window_at(p)
                .or(handle)
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
                .and_then(|(pid, id)| {
                    self.windows.get(&id).and_then(|w| {
                        if w.pid != pid
                            || !self.can_tile_drag(id, w.space)
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
                            resize_gesture: edge <= 12.,
                            baseline: self.desktops.get(&w.space)?.clone(),
                            area: active.iter().find(|(s, _)| *s == w.space)?.1,
                        })
                    })
                });
            // Focus can still name the previous window at mouse-down, and
            // resize handles extend into the gap between neighboring tiles.
            // Keep nearby candidates until native geometry identifies the owner.
            self.gesture_candidates = self
                .windows
                .iter()
                .filter_map(|(id, w)| {
                    if self.drag.as_ref().is_some_and(|drag| drag.id == *id)
                        || !self.can_tile_drag(*id, w.space)
                    {
                        return None;
                    }
                    let frame = ws::bounds(*id)?;
                    if !contains(
                        rect(
                            frame.origin.x - 12.,
                            frame.origin.y - 12.,
                            frame.size.width + 24.,
                            frame.size.height + 24.,
                        ),
                        p,
                    ) {
                        return None;
                    }
                    let area = active.iter().find(|(key, _)| *key == w.space)?.1;
                    let edge = (p.x - frame.origin.x)
                        .abs()
                        .min((p.x - frame.origin.x - frame.size.width).abs())
                        .min((p.y - frame.origin.y).abs())
                        .min((p.y - frame.origin.y - frame.size.height).abs());
                    Some(Drag {
                        id: *id,
                        space: w.space,
                        start: frame,
                        point: p,
                        drop: None,
                        candidate: None,
                        candidate_since: Instant::now(),
                        moved: false,
                        resized: false,
                        resize_gesture: edge <= 12.,
                        baseline: self.desktops.get(&w.space)?.clone(),
                        area,
                    })
                })
                .collect();
        }
        if down && !self.pressed {
            if let Some(drag) = &self.drag {
                self.handoffs.remove(&drag.id);
            }
        }
        if trace_tiling() && down && !self.pressed {
            eprintln!(
                "tiling capture: point={p:?} drag={:?}",
                self.drag
                    .as_ref()
                    .map(|d| (d.id, d.space, d.start, d.resize_gesture))
            );
        }
        self.pressed = down;
        // Resolve on release too: a short gesture may have no held sample.
        resolve_gesture(
            &mut self.drag,
            &mut self.gesture_candidates,
            p,
            pointer_workspace,
            ws::bounds,
        );
        if down || self.drag.is_some() {
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
                    drag.observe(frame, p, pointer_workspace);
                }
                if drag.moved || drag.resized {
                    self.dirty |= !self.transitions.is_empty() || !self.verifications.is_empty();
                    self.transitions.clear();
                    self.verifications.clear();
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
                            arrival_proposal(
                                &destination,
                                drag.id,
                                Some(intent),
                                *area,
                                Some(&self.minimum_sizes),
                            )?;
                            Some((*key, intent))
                        });
                    drag.update_drop(candidate, Instant::now(), !down);
                    if let Some((key, intent)) = drag.drop {
                        if let Some((_, area)) = active.iter().find(|(k, _)| *k == key) {
                            let d = self.desktops.get(&key).cloned().unwrap_or_default();
                            preview = arrival_proposal(
                                &d,
                                drag.id,
                                Some(intent),
                                *area,
                                Some(&self.minimum_sizes),
                            )
                            .and_then(|d| {
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
            } else if !self.drag.as_ref().is_some_and(|drag| drag.resized) {
                self.hide_preview()
            }
            if down {
                self.resize_drag(
                    false,
                    p,
                    pointer_workspace,
                    pref(defaults, "tile.resize.preview") != 0,
                );
                return; // Only previews change while macOS owns the native gesture.
            }
        }
        self.resize_drag(true, p, pointer_workspace, false);
        self.gesture_candidates.clear();
        if let Some(drag) = self.drag.take() {
            if trace_tiling() {
                eprintln!(
                    "tiling release: id={} moved={} resized={} destination={:?} minimums={:?}",
                    drag.id, drag.moved, drag.resized, drag.drop, self.minimum_sizes
                );
            }
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
                        if let Some(proposed) = arrival_proposal(
                            &d,
                            drop.id,
                            drop.intent,
                            *area,
                            Some(&self.minimum_sizes),
                        ) {
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
                            if trace_tiling() {
                                eprintln!(
                                    "tiling commit: id={} workspace={:?} frames={:?}",
                                    drop.id,
                                    drop.destination,
                                    proposed.frames(*area, 10.)
                                );
                            }
                            self.desktops.insert(drop.destination, proposed);
                            if let Some(w) = self.windows.get_mut(&drop.id) {
                                if w.space != drop.destination {
                                    w.original =
                                        restoration_frame(w.original, w.original_area, *area);
                                    w.original_area = *area;
                                }
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
        if place_windows {
            self.verify_placements(&active);
            self.animate(&active);
        }
        if !self.transitions.is_empty() {
            // Do not interleave full AX enumeration with the 160 ms animation.
            // Space/screen changes and real gestures already cancel that batch.
            return;
        }
        if self
            .last_scan
            .is_some_and(|t| t.elapsed() < Duration::from_millis(scan_interval))
            && !self.dirty
        {
            return;
        }
        let Some(candidates) = ws::try_tile_windows() else {
            return;
        };
        self.last_visible_windows =
            Some(candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect());
        self.last_scan = Some(Instant::now());
        let focused = unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            ax::focused_id(pid)
        };
        let visible_ids: HashSet<_> = candidates.iter().map(|(_, id, _)| *id).collect();
        let all_ids = if self.windows.keys().any(|id| !visible_ids.contains(id)) {
            ws::all_window_ids()
        } else {
            None
        };
        let mut app_window_ids = HashMap::new();
        let mut transferred: HashSet<u32> = self.handoffs.keys().copied().collect();
        // Offscreen Spaces retain their trees; closed/hidden/minimized windows on
        // visible Spaces leave the current tree and rejoin when visible again.
        let mut members: HashMap<Workspace, Vec<u32>> = HashMap::new();
        let mut actual_frames = HashMap::new();
        let mut uncertain = HashSet::new();
        for (pid, id, _compositor_frame) in candidates {
            if self.windows.get(&id).is_some_and(|w| w.pid != pid) {
                self.forget_window(id);
                transferred.remove(&id);
            }
            let Some(space) = spaces::window_space(id) else {
                if self.windows.get(&id).is_some_and(|w| w.pid == pid) {
                    uncertain.insert(id);
                }
                continue;
            };
            if self
                .handoffs
                .get(&id)
                .is_some_and(|h| h.destination.1 != space)
            {
                self.handoffs.remove(&id);
                transferred.remove(&id);
            }
            if !active.iter().any(|(key, _)| key.1 == space) {
                continue; // Never query AX for a confirmed non-visible Space.
            }
            // The compositor can translate one window outside its display while
            // a Space slides. Read AX before deciding ownership, not after a
            // geometry-based rejection has already removed its layout slot.
            let known = self.windows.contains_key(&id);
            let retain_uncertain = self.windows.get(&id).is_some_and(|w| w.space.1 == space);
            let Some(mut element) = self
                .windows
                .get(&id)
                .map(|w| w.element.clone())
                .or_else(|| ax::window_for_id(pid, id))
            else {
                if retain_uncertain {
                    uncertain.insert(id);
                }
                continue;
            };
            if !known && !ax::tileable(element.0) {
                continue;
            }
            let mut geometry = ax::geometry(element.0);
            if geometry.is_none()
                && let Some(fresh) = ax::window_for_id(pid, id)
                && ax::tileable(fresh.0)
            {
                element = fresh;
                geometry = ax::geometry(element.0);
            }
            let Some((position, size)) = geometry else {
                if retain_uncertain {
                    uncertain.insert(id);
                }
                continue;
            };
            let frame = NSRect::new(position, size);
            let Some(space) = self.observed_workspace(id, space, frame, &active) else {
                // No overlap is uncertainty, not evidence of a closed window.
                if retain_uncertain {
                    uncertain.insert(id);
                }
                continue;
            };
            if let std::collections::hash_map::Entry::Vacant(entry) = self.windows.entry(id) {
                if self.desktop_floating.remove(&(pid, id))
                    || crate::tile_rules::floats(defaults, pid)
                {
                    self.floating.insert(id);
                }
                entry.insert(Managed {
                    pid,
                    space,
                    original: frame,
                    original_area: active.iter().find(|(key, _)| *key == space).unwrap().1,
                    element: element.clone(),
                });
                self.dirty = true;
            }
            let w = self.windows.get_mut(&id).unwrap();
            w.element = element;
            if w.space != space {
                if let Some((_, destination)) = active.iter().find(|(key, _)| *key == space) {
                    w.original = restoration_frame(w.original, w.original_area, *destination);
                    w.original_area = *destination;
                }
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
            actual_frames.insert(id, frame);
            self.inactive_tabs.remove(&id); // A detached tab can become an ordinary window.
            let owns_slot = self
                .desktops
                .get(&space)
                .is_some_and(|d| d.order.contains(&id));
            if !owns_slot {
                if let Some(group) =
                    ax::native_tab_group(w.element.0).or_else(|| self.tab_groups.get(&id).cloned())
                {
                    // AXTabGroup is shared by the tabs of ONE window. Matching
                    // its AX identity avoids merging unrelated windows, even if
                    // they have identical frames, titles, process and Space.
                    let possible: Vec<_> = self
                        .windows
                        .iter()
                        .filter_map(|(old, source)| {
                            (*old != id
                                && source.pid == pid
                                && source.space == space
                                && !visible_ids.contains(old)
                                && (self.floating.contains(old)
                                    || self
                                        .desktops
                                        .get(&space)
                                        .is_some_and(|d| d.order.contains(old))))
                            .then_some(*old)
                        })
                        .collect();
                    let mut matches = Vec::new();
                    let mut first_tabs = Vec::new();
                    for old in possible {
                        // The first Cmd+T can add the tab bar after the source's
                        // last scan; query its retained AX element once if needed.
                        if !self.tab_groups.contains_key(&old)
                            && let Some(previous) =
                                ax::native_tab_group(self.windows[&old].element.0)
                        {
                            self.tab_groups.insert(old, previous);
                        }
                        if spaces::window_space(old).is_some_and(|native| native != space.1) {
                            continue;
                        }
                        if let Some(previous) = self.tab_groups.get(&old) {
                            if unsafe { ax::CFEqual(previous.0, group.0) } {
                                matches.push(old);
                            }
                        } else if all_ids
                            .as_ref()
                            .is_some_and(|ids| ids.contains(&(pid, old)))
                        {
                            // The first tab creates a group after the outgoing
                            // AXWindow has become unreadable. Bootstrap only from
                            // one live slot at this frame, absent from AXWindows.
                            let ax_ids = app_window_ids
                                .entry(pid)
                                .or_insert_with(|| ax::application_window_ids(pid));
                            let previous = self
                                .constrained
                                .get(&old)
                                .map(|(_, actual)| *actual)
                                .or_else(|| self.applied.get(&old).copied())
                                .or_else(|| ws::bounds(old));
                            if ax_ids.as_ref().is_some_and(|ids| !ids.contains(&old))
                                && previous.is_some_and(|previous| same_tab_frame(previous, frame))
                            {
                                first_tabs.push(old);
                            }
                        }
                    }
                    if matches.is_empty() {
                        matches = first_tabs;
                    }
                    if let [old] = matches.as_slice() {
                        self.tab_groups.insert(*old, group.clone());
                        self.inherit_tab_slot(*old, id);
                        // A stale inactive tab can retain an older frame. Restore
                        // this slot directly, never animate it as a new tile.
                        transferred.insert(id);
                    }
                    self.tab_groups.insert(id, group);
                }
            }
            let w = self.windows.get_mut(&id).unwrap();
            if !self.floating.contains(&id) {
                members.entry(space).or_default().push(id);
            } else if !self.transitions.contains_key(&id) {
                // Floating windows are user-owned: never snap, preview or reinsert.
                w.original = frame;
                if let Some((_, current)) = active.iter().find(|(key, _)| *key == space) {
                    w.original_area = *current;
                }
            }
        }
        // Offscreen apps can take the full AX timeout per call. Use one native
        // window list, and fail closed if the compositor query is unavailable.
        for (id, w) in &self.windows {
            if !visible_ids.contains(id)
                && !self.inactive_tabs.contains(id)
                && !self.floating.contains(id)
                && active.iter().any(|(key, _)| *key == w.space)
                && all_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&(w.pid, *id)))
                && spaces::window_space(*id).is_none_or(|native| native == w.space.1)
            {
                // Being temporarily offscreen is not proof of destruction. A
                // confirmed native Space transfer never retains the old leaf.
                uncertain.insert(*id);
            }
        }
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
            self.forget_window(id);
        }
        for id in &uncertain {
            // Retaining a leaf preserves its layout, not permission to continue
            // writing through an AX/native-visibility uncertainty.
            let was_animating = self.transitions.remove(id).is_some();
            let was_verifying = self.verifications.remove(id).is_some();
            if was_animating || was_verifying {
                self.applied.remove(id);
            }
        }
        // A transient AX/Space read must not remove and reinsert a visible tile
        // around the focused window. Genuine ineligibility still expires promptly.
        for id in self.membership_grace.retained(&uncertain, Instant::now()) {
            if let Some(w) = self.windows.get(&id) {
                if !self.floating.contains(&id)
                    && !self.inactive_tabs.contains(&id)
                    && active.iter().any(|(key, _)| *key == w.space)
                {
                    members.entry(w.space).or_default().push(id);
                }
            }
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
                    && !self.inactive_tabs.contains(id)
                    && !self.floating.contains(id)
                    && active.iter().any(|(space, _)| *space == w.space)
                    && spaces::window_space(*id) == Some(w.space.1)
                {
                    members.entry(w.space).or_default().push(*id);
                }
            }
        }
        crate::desktop_mode::refresh();
        if crate::desktop_mode::active() {
            return;
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
            let ids = members.entry(space).or_default();
            ids.sort_unstable();
            d.reconcile_at(ids, r, 10., focused);
            d.set_minimum_sizes(&self.minimum_sizes);
            d.fit(r, 10.);
            let frames = d.frames(r, 10.);
            if frames.len() != d.order.len() {
                self.last_error = Some("Esta plantilla no admite los tamaños mínimos de estas ventanas; prueba otra o deja una ventana flotante.".into());
                continue;
            }
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
                if crate::desktop_mode::active() {
                    return;
                }
                if !place_windows || !visible_ids.contains(&id) || uncertain.contains(&id) {
                    continue;
                }
                // WindowServer bounds can include the native Space slide offset.
                // AX reports the app's frame, already read during membership scan.
                let actual = actual_frames.get(&id).copied();
                if !self.needs_placement(id, frame, actual) {
                    continue;
                }
                self.constrained.remove(&id);
                if transferred.contains(&id) || self.settling_space() {
                    // macOS has already animated the Space transition. Commit
                    // the destination once, rather than animating it a second time.
                    self.transitions.remove(&id);
                    self.verifications.remove(&id);
                    if let Some(w) = self.windows.get(&id) {
                        let mut verification = Verification::new(frame);
                        verification.accepted = ax::set_frame(w.element.0, frame);
                        self.verifications.insert(id, verification);
                    }
                } else {
                    self.transition_from(id, actual.unwrap(), frame);
                }
                // Cache destinations; animation owns intermediate frames.
                self.applied.insert(id, frame);
            }
        }
        self.dirty = false;
    }
    /// `frame` is the AX geometry used for placement, never a compositor slide.
    fn observed_workspace(
        &self,
        id: u32,
        native: u64,
        frame: NSRect,
        areas: &[(Workspace, NSRect)],
    ) -> Option<Workspace> {
        let previous = self.windows.get(&id).map(|w| w.space);
        let retained = self
            .handoffs
            .get(&id)
            .map(|h| h.destination)
            .or_else(|| {
                let expected = self.applied.get(&id).copied();
                let limited = self.constrained.get(&id).map(|(_, observed)| *observed);
                (expected.is_some_and(|r| near(r, frame))
                    || limited.is_some_and(|r| near(r, frame)))
                .then_some(previous)
                .flatten()
            })
            .or_else(|| stable_overlap_workspace(native, frame, areas, previous));
        retained_workspace(native, frame, areas, retained)
    }
    fn forget_window(&mut self, id: u32) {
        self.tab_groups.remove(&id);
        self.inactive_tabs.remove(&id);
        self.minimum_sizes.remove(&id);
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
        self.membership_grace.since.remove(&id);
    }
    fn cancel_window_placement(&mut self, id: u32) {
        self.transitions.remove(&id);
        self.verifications.remove(&id);
        self.applied.remove(&id);
        self.constrained.remove(&id);
    }
    pub fn restore_space(&mut self, space: u64) {
        if self.presentation_blocks_input() {
            self.restore_spaces_after_presentation.insert(space);
            return;
        }
        for (id, w) in &self.windows {
            if w.space.1 == space
                && !self.inactive_tabs.contains(id)
                && !self.floating.contains(id)
                && !ax::flag(w.element.0, "AXFullScreen")
            {
                if let Some(current) = area(w.space.0) {
                    ax::set_frame(
                        w.element.0,
                        restoration_frame(w.original, w.original_area, current),
                    );
                }
                self.applied.remove(id);
                self.transitions.remove(id);
                self.verifications.remove(id);
            }
        }
        self.windows.retain(|_, w| w.space.1 != space);
        self.invalidate();
    }
}

fn restoration_frame(original: NSRect, source: NSRect, destination: NSRect) -> NSRect {
    if source == destination {
        return original;
    }
    let width = original.size.width.min(destination.size.width);
    let height = original.size.height.min(destination.size.height);
    let relative = |position: f64, start: f64, span: f64, size: f64| {
        if span > size {
            ((position - start) / (span - size)).clamp(0., 1.)
        } else {
            0.
        }
    };
    rect(
        destination.origin.x
            + relative(
                original.origin.x,
                source.origin.x,
                source.size.width,
                original.size.width,
            ) * (destination.size.width - width),
        destination.origin.y
            + relative(
                original.origin.y,
                source.origin.y,
                source.size.height,
                original.size.height,
            ) * (destination.size.height - height),
        width,
        height,
    )
}

fn areas_changed(old: &[(Workspace, NSRect)], new: &[(Workspace, NSRect)]) -> bool {
    old.iter().any(|(key, area)| {
        new.iter()
            .any(|(candidate, updated)| candidate == key && updated != area)
    })
}

fn desktop_keys_changed(old: &[(Workspace, NSRect)], new: &[(Workspace, NSRect)]) -> bool {
    !old.is_empty()
        && (old.len() != new.len()
            || old
                .iter()
                .any(|(key, _)| !new.iter().any(|(candidate, _)| candidate == key)))
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
        .map(|(key, r)| (*key, overlap(frame, *r)))
        .filter(|(_, overlap)| *overlap > 0.)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(key, _)| key)
}

fn overlap(frame: NSRect, area: NSRect) -> f64 {
    let width = (frame.origin.x + frame.size.width).min(area.origin.x + area.size.width)
        - frame.origin.x.max(area.origin.x);
    let height = (frame.origin.y + frame.size.height).min(area.origin.y + area.size.height)
        - frame.origin.y.max(area.origin.y);
    width.max(0.) * height.max(0.)
}

fn same_tab_frame(a: NSRect, b: NSRect) -> bool {
    // The first native tab bar may extend the top edge by one titlebar height.
    (a.origin.x - b.origin.x).abs() < 3.
        && (a.size.width - b.size.width).abs() < 3.
        && (a.origin.y - b.origin.y).abs() <= 40.
        && ((a.origin.y + a.size.height) - (b.origin.y + b.size.height)).abs() < 3.
}

fn stable_overlap_workspace(
    native: u64,
    frame: NSRect,
    areas: &[(Workspace, NSRect)],
    previous: Option<Workspace>,
) -> Option<Workspace> {
    let previous = previous.filter(|key| key.1 == native)?;
    let (_, area) = areas.iter().find(|(key, _)| *key == previous)?;
    let retained = overlap(frame, *area);
    let best = areas
        .iter()
        .filter(|(key, _)| key.1 == native)
        .map(|(_, area)| overlap(frame, *area))
        .fold(0., f64::max);
    // Avoid ownership flapping around a shared edge from 1–2 point compositor
    // differences. A clear majority or native Space change still transfers.
    (retained > 0. && retained >= best * 0.95).then_some(previous)
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
    fn picker_disables_a_template_that_cannot_fit_app_minima_instead_of_previewing_columns_as_spiral()
     {
        let area = rect(10., 41., 1660., 947.);
        let mut desktop = Desktop::configured(Layout::Spiral, Some((120, 6)));
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop.set_minimum_sizes(
            &[(2, objc2_foundation::NSSize::new(120., 947.))]
                .into_iter()
                .collect(),
        );
        let context = PickerContext {
            workspace: (1, 7),
            area,
            desktop,
        };
        assert!(context.proposal(Layout::Spiral).is_none());
        assert!(context.proposal(Layout::ThreeBands).is_some());
    }
    #[test]
    fn show_desktop_preserves_tree_and_leaves_new_windows_floating_after_exit() {
        use crate::desktop_mode::Mode;
        let mut manager = Manager::default();
        let workspace = (2, 8);
        let area = rect(-900., -400., 880., 1580.);
        let mut desktop = Desktop::configured(Layout::ThreeBands, Some((120, 6)));
        desktop.reconcile(&[1, 2], area, 10.);
        let before = desktop.frames(area, 10.);
        manager.desktops.insert(workspace, desktop);
        manager.transition_from(1, area, before[0].1);
        manager
            .verifications
            .insert(2, Verification::new(before[1].1));
        let now = Instant::now();
        let existing = HashSet::from([(7, 1), (7, 2), (7, 30)]);
        manager.set_presentation_at(Mode::Desktop, now, Some(existing));
        assert!(manager.transitions.is_empty() && manager.verifications.is_empty());
        // A null defaults pointer is safe only if every operation returns before
        // touching AX, screen geometry, preferences or the desktop membership.
        manager.tick_mode(ptr::null_mut(), true);
        manager.refresh_for_picker(ptr::null_mut());
        manager.toggle_float(ptr::null_mut());
        assert_eq!(manager.desktops[&workspace].frames(area, 10.), before);
        let after = HashSet::from([(7, 1), (7, 2), (9, 3), (9, 4), (8, 30)]);
        // Repeated desktop notifications cannot reset the original baseline.
        manager.set_presentation_at(Mode::Desktop, now, Some(after.clone()));
        manager.set_presentation_at(Mode::Overview, now, Some(after));
        assert_eq!(
            manager.desktop_floating,
            HashSet::from([(9, 3), (9, 4), (8, 30)])
        );
        manager.tick_mode(ptr::null_mut(), true);
        manager.set_presentation_at(Mode::Normal, now, None);
        assert!(manager.waiting_for_space(now + Duration::from_millis(599)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(600)));
        assert_eq!(manager.desktops[&workspace].frames(area, 10.), before);
        assert_eq!(manager.desktop_floating.len(), 3);
        assert!(!manager.desktop_floating.contains(&(7, 30)));
    }
    #[test]
    fn failed_desktop_snapshot_preserves_known_windows_and_retries_before_resume() {
        use crate::desktop_mode::Mode;
        let mut manager = Manager {
            last_visible_windows: Some(HashSet::from([(7, 1)])),
            ..Default::default()
        };
        let now = Instant::now();
        manager.set_presentation_at(Mode::Desktop, now, None);
        manager.set_presentation_at(Mode::Normal, now, None);
        assert_eq!(manager.presentation, Mode::Desktop);
        manager.tick_mode(ptr::null_mut(), true);
        manager.set_presentation_at(Mode::Normal, now, Some(HashSet::from([(7, 1), (9, 2)])));
        assert_eq!(manager.presentation, Mode::Normal);
        assert_eq!(manager.desktop_floating, HashSet::from([(9, 2)]));
        manager.set_presentation_at(Mode::Overview, now, None);
        manager.set_presentation_at(
            Mode::Normal,
            now,
            Some(HashSet::from([(7, 1), (9, 2), (9, 3)])),
        );
        assert_eq!(manager.desktop_floating, HashSet::from([(9, 2)]));
    }
    #[test]
    fn folder_opened_before_desktop_entry_delivery_still_stays_floating() {
        use crate::desktop_mode::Mode;
        let mut manager = Manager {
            last_visible_windows: Some(HashSet::from([(7, 1)])),
            ..Default::default()
        };
        let now = Instant::now();
        let after = HashSet::from([(7, 1), (9, 2)]);
        manager.set_presentation_at(Mode::Desktop, now, Some(after.clone()));
        manager.remember_late_desktop_windows(&after);
        manager.set_presentation_at(Mode::Normal, now, Some(after));
        assert_eq!(manager.desktop_floating, HashSet::from([(9, 2)]));
    }
    #[test]
    fn show_desktop_round_trip_preserves_every_rect_and_saved_tree_across_displays_and_spaces() {
        use crate::desktop_mode::Mode;
        let contexts = [
            ((1, 7), rect(10., 41., 1660., 947.)),
            ((1, 9), rect(10., 41., 1660., 947.)),
            ((2, 8), rect(-900., -400., 880., 1580.)),
            ((2, 10), rect(-900., -400., 880., 1580.)),
        ];
        for layout in Layout::ALL {
            for grid in [None, Some((120, 6))] {
                let mut manager = Manager::default();
                let mut snapshots = HashMap::new();
                let mut before_ids = HashSet::new();
                for (index, (key, area)) in contexts.iter().enumerate() {
                    let ids = [
                        index as u32 * 3 + 1,
                        index as u32 * 3 + 2,
                        index as u32 * 3 + 3,
                    ];
                    let mixed_layout = Layout::ALL[(layout.selection_index() as usize
                        + [0, 1, 3, 4][index])
                        % Layout::ALL.len()];
                    let mut desktop = Desktop::configured(mixed_layout, grid);
                    desktop.reconcile(&ids, *area, 10.);
                    let frame = desktop
                        .frames(*area, 10.)
                        .iter()
                        .find(|(id, _)| *id == ids[0])
                        .unwrap()
                        .1;
                    let mut resized = frame;
                    resized.size.width -= 35.;
                    resized.size.height += 50.;
                    desktop.resize(ids[0], frame, resized, *area, 10.);
                    let second = desktop
                        .frames(*area, 10.)
                        .iter()
                        .find(|(id, _)| *id == ids[1])
                        .unwrap()
                        .1;
                    let minimum = if second.size.width + 20. < area.size.width {
                        objc2_foundation::NSSize::new(second.size.width + 20., 90.)
                    } else {
                        objc2_foundation::NSSize::new(120., second.size.height + 20.)
                    };
                    manager.minimum_sizes.insert(ids[1], minimum);
                    desktop.set_minimum_sizes(&manager.minimum_sizes);
                    assert_eq!(desktop.frames(*area, 10.).len(), 3);
                    snapshots.insert(
                        *key,
                        (desktop.frames(*area, 10.), desktop.save_distribution(*area)),
                    );
                    before_ids.extend(ids.map(|id| (7, id)));
                    manager.desktops.insert(*key, desktop);
                }
                let now = Instant::now();
                manager.set_presentation_at(Mode::Desktop, now, Some(before_ids.clone()));
                manager.space_changed_with_visible_at(now, &[(1, 9), (2, 8)]);
                manager.space_changed_with_visible_at(now, &[(1, 9), (2, 10)]);
                manager.tick_mode(ptr::null_mut(), true);
                let mut after_ids = before_ids;
                after_ids.extend([(8, 100), (9, 101)]);
                manager.set_presentation_at(Mode::Normal, now, Some(after_ids));
                assert!(manager.waiting_for_space(now + Duration::from_millis(599)));
                assert!(!manager.waiting_for_space(now + Duration::from_millis(600)));
                for (key, area) in contexts {
                    let desktop = manager.desktops.get_mut(&key).unwrap();
                    let ids = desktop.order.clone();
                    desktop.reconcile_at(&ids, area, 10., Some(ids[2]));
                    desktop.set_minimum_sizes(&manager.minimum_sizes);
                    desktop.fit(area, 10.);
                    assert_eq!(desktop.frames(area, 10.), snapshots[&key].0);
                    assert_eq!(desktop.save_distribution(area), snapshots[&key].1);
                }
                assert_eq!(
                    manager.desktop_floating,
                    HashSet::from([(8, 100), (9, 101)])
                );
            }
        }
    }
    #[test]
    fn ax_workspace_membership_preserves_real_shared_space_monitor_transfers() {
        let mut manager = Manager::default();
        let areas = [
            ((1, 7), rect(0., 30., 1600., 900.)),
            ((2, 7), rect(-900., -400., 880., 1580.)),
        ];
        let actual = rect(-900., -400., 880., 785.);
        manager.windows.insert(
            1,
            Managed {
                pid: std::process::id() as i32,
                space: (2, 7),
                original: actual,
                original_area: areas[1].1,
                element: ax::application(std::process::id() as i32).unwrap(),
            },
        );
        manager.applied.insert(1, actual);
        let compositor_slide = rect(100., 30., 880., 785.);
        assert_eq!(
            manager.observed_workspace(1, 7, compositor_slide, &areas),
            Some((1, 7))
        );
        assert_eq!(
            manager.observed_workspace(1, 7, actual, &areas),
            Some((2, 7))
        );
        // If AX actually moves to the other monitor, the cache must not pin it.
        assert_eq!(
            manager.observed_workspace(1, 7, areas[0].1, &areas),
            Some((1, 7))
        );
        // Disconnecting the old monitor cannot retain its former ownership.
        assert_eq!(
            manager.observed_workspace(1, 7, areas[0].1, &areas[..1]),
            Some((1, 7))
        );
    }
    #[test]
    fn space_slide_cannot_expand_one_of_two_windows_in_three_portrait_rows() {
        let mut manager = Manager::default();
        let primary = ((1, 7), rect(0., 30., 1600., 900.));
        let secondary = ((2, 8), rect(-900., -400., 880., 1580.));
        let mut desktop = Desktop::configured(Layout::ThreeBands, None);
        desktop.reconcile(&[10, 20], secondary.1, 10.);
        let original = desktop.frames(secondary.1, 10.);
        assert_eq!(original.len(), 2);
        manager.active_areas = vec![primary, secondary];
        manager.desktops.insert(secondary.0, desktop.clone());
        for (id, frame) in &original {
            manager.windows.insert(
                *id,
                Managed {
                    pid: std::process::id() as i32,
                    space: secondary.0,
                    original: *frame,
                    original_area: secondary.1,
                    element: ax::application(std::process::id() as i32).unwrap(),
                },
            );
            manager.applied.insert(*id, *frame);
        }
        let active = [((1, 9), primary.1), secondary];
        manager.space_changed_with_visible_at(
            Instant::now(),
            &active.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        );
        assert_eq!(
            manager.desktops[&secondary.0].frames(secondary.1, 10.),
            original
        );
        // Only the horizontal monitor changed Space (7 -> 9). The vertical
        // monitor stays on Space 8, with identical AX geometry and no gesture.
        // A transient compositor offset is metadata, not a physical user move.
        let mut sliding = original[1].1;
        sliding.origin.x -= 1700.;
        assert_eq!(manager.observed_workspace(20, 8, sliding, &active), None);
        let mut broken = desktop.clone();
        broken.reconcile(&[10], secondary.1, 10.);
        assert_ne!(broken.frames(secondary.1, 10.), original);
        // Production classifies membership from the same AX frame it places.
        for _ in 0..5 {
            let members: Vec<_> = original
                .iter()
                .filter_map(|(id, ax_frame)| {
                    (manager.observed_workspace(*id, 8, *ax_frame, &active) == Some(secondary.0))
                        .then_some(*id)
                })
                .collect();
            desktop.reconcile_at(&members, secondary.1, 10., Some(20));
            desktop.fit(secondary.1, 10.);
            assert_eq!(desktop.frames(secondary.1, 10.), original);
            for (id, frame) in &original {
                assert!(!manager.needs_placement(*id, *frame, Some(*frame)));
            }
        }
        // A real native Space change must still override cached ownership.
        assert_eq!(
            manager.observed_workspace(10, 9, primary.1, &active),
            Some((1, 9))
        );
        assert_eq!(
            manager.observed_workspace(10, 99, original[0].1, &active),
            None
        );
    }
    #[test]
    fn independent_space_switch_preserves_secondary_layout_and_pending_placement() {
        let mut manager = Manager::default();
        let primary = (1, 7);
        let portrait = (2, 8);
        let area = rect(-890., -390., 880., 1580.);
        let mut desktop = Desktop::default();
        desktop.reconcile(&[2, 3], area, 10.);
        let from = desktop.frames(area, 10.)[0].1;
        let mut resized = from;
        resized.size.width -= 100.;
        desktop.resize(2, from, resized, area, 10.);
        let before = desktop.frames(area, 10.);
        manager.desktops.insert(portrait, desktop);
        for (id, space) in [(1, primary), (2, portrait), (3, portrait), (4, portrait)] {
            manager.windows.insert(
                id,
                Managed {
                    pid: std::process::id() as i32,
                    space,
                    original: area,
                    original_area: area,
                    element: ax::application(std::process::id() as i32).unwrap(),
                },
            );
            manager.applied.insert(id, area);
        }
        manager.transition_from(1, from, area);
        manager.transition_from(2, from, area);
        manager.verifications.insert(3, Verification::new(area));
        manager.constrained.insert(3, (area, resized));
        let now = Instant::now();
        manager.space_changed_with_visible_at(now, &[(1, 9), portrait]);
        assert!(!manager.transitions.contains_key(&1));
        assert!(!manager.applied.contains_key(&1));
        assert!(manager.transitions.contains_key(&2));
        assert!(manager.verifications.contains_key(&3));
        assert_eq!(manager.applied[&2], area);
        assert_eq!(manager.constrained[&3], (area, resized));
        assert_eq!(manager.desktops[&portrait].frames(area, 10.), before);
        assert!(!manager.needs_placement(3, area, Some(resized)));
        // A settled window needs no geometry write on repeated notifications.
        for offset in [100, 200, 300] {
            manager.space_changed_with_visible_at(
                now + Duration::from_millis(offset),
                &[(1, 9), portrait],
            );
            assert_eq!(manager.applied[&4], area);
            assert!(!manager.needs_placement(4, area, Some(area)));
            assert_eq!(manager.desktops[&portrait].frames(area, 10.), before);
        }
        assert!(!manager.waiting_for_space(now + Duration::from_millis(900)));
        // A late native update is checked again before deferred writes resume.
        manager.cancel_inactive_placements(&[(1, 9), (2, 10)]);
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
    }

    #[test]
    fn selecting_native_tabs_keeps_one_slot_and_preserves_constraints_and_floating_state() {
        let mut manager = Manager::default();
        let space = (1, 7);
        let area = rect(10., 41., 1660., 947.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let frames = desktop.frames(area, 10.);
        let mut resized = frames[0].1;
        resized.size.width += 80.;
        desktop.resize(1, frames[0].1, resized, area, 10.);
        let before = desktop.frames(area, 10.);
        let target = before.iter().find(|(id, _)| *id == 2).unwrap().1;
        manager.desktops.insert(space, desktop);
        for id in [2, 4, 5] {
            manager.windows.insert(
                id,
                Managed {
                    pid: std::process::id() as i32,
                    space,
                    original: area,
                    original_area: area,
                    element: ax::application(std::process::id() as i32).unwrap(),
                },
            );
        }
        manager.applied.insert(2, target);
        manager.constrained.insert(2, (target, target));
        manager.minimum_sizes.insert(2, target.size);
        let mut selected = 2;
        for next in [4, 5, 2, 5, 4, 2] {
            manager.inherit_tab_slot(selected, next);
            assert!(manager.inactive_tabs.contains(&selected));
            assert!(!manager.inactive_tabs.contains(&next));
            let frames = manager.desktops[&space].frames(area, 10.);
            assert_eq!(
                frames.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                before.iter().map(|(_, r)| *r).collect::<Vec<_>>()
            );
            assert_eq!(manager.desktops[&space].order, vec![1, next, 3]);
            assert_eq!(manager.applied[&next], target);
            assert_eq!(manager.minimum_sizes[&next], target.size);
            assert!(!manager.needs_placement(next, target, Some(target)));
            selected = next;
        }
        manager.desktops.get_mut(&space).unwrap().remove(selected);
        manager.floating.insert(selected);
        manager.inherit_tab_slot(selected, 4);
        assert!(manager.floating.contains(&4));
        assert_eq!(manager.desktops[&space].order, vec![1, 3]);
    }

    #[test]
    fn tab_switches_preserve_adjusted_frames_in_every_template_and_orientation() {
        for area in [rect(10., 41., 1660., 947.), rect(-890., -390., 880., 1580.)] {
            for layout in Layout::ALL {
                let mut desktop = Desktop::configured(layout, Some((12, 6)));
                desktop.reconcile(&[1, 2, 3], area, 10.);
                let from = desktop.frames(area, 10.)[0].1;
                let mut resized = from;
                resized.size.width += 70.;
                desktop.resize(1, from, resized, area, 10.);
                let before = desktop.frames(area, 10.);
                for (old, new) in [(2, 10), (10, 11), (11, 2)] {
                    assert!(desktop.replace_window(old, new));
                    desktop.reconcile_at(&[1, new, 3], area, 10., Some(new));
                    desktop.fit(area, 10.);
                    let frames = desktop.frames(area, 10.);
                    assert_eq!(
                        frames.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                        before.iter().map(|(_, r)| *r).collect::<Vec<_>>()
                    );
                }
                assert!(!desktop.replace_window(2, 3));
                assert!(!desktop.replace_window(99, 4));
                assert_eq!(desktop.frames(area, 10.), before);
            }
        }
    }

    #[test]
    fn first_native_tab_matches_its_frame_but_not_other_tiles_or_monitors() {
        let frame = rect(10., 41., 825., 947.);
        assert!(same_tab_frame(frame, frame));
        assert!(same_tab_frame(frame, rect(10., 13., 825., 975.)));
        for other in [
            rect(845., 41., 825., 947.),
            rect(10., 41., 1660., 947.),
            rect(-890., -390., 880., 1580.),
            rect(10., 41., 825., 600.),
        ] {
            assert!(!same_tab_frame(frame, other));
        }
    }

    #[test]
    fn shared_space_monitor_ownership_does_not_flap_at_equal_overlap() {
        let areas = [
            ((1, 7), rect(0., 30., 1600., 900.)),
            ((2, 7), rect(-900., -400., 900., 1600.)),
        ];
        for x in [-302., -300., -298.] {
            assert_eq!(
                stable_overlap_workspace(7, rect(x, 40., 600., 500.), &areas, Some((2, 7))),
                Some((2, 7))
            );
        }
        assert_eq!(
            stable_overlap_workspace(7, rect(-200., 40., 600., 500.), &areas, Some((2, 7))),
            None
        );
        assert_eq!(
            stable_overlap_workspace(8, rect(-300., 40., 600., 500.), &areas, Some((2, 7))),
            None
        );
        assert_eq!(
            stable_overlap_workspace(7, rect(-300., 40., 600., 500.), &areas[..1], Some((2, 7))),
            None
        );
    }
    #[test]
    fn animation_batch_starts_together_after_slow_preparation_and_retries_failed_samples() {
        let mut manager = Manager::default();
        let from = rect(-900., -400., 880., 1580.);
        let target = rect(10., 41., 1660. / 3., 947.);
        manager.transition_from(1, from, target);
        manager.transition_from(2, from, target);
        let now = Instant::now() + Duration::from_secs(4);
        for transition in manager.transitions.values_mut() {
            assert_eq!(transition.sample(now), (from, false));
        }
        let halfway = now + Duration::from_millis(80);
        let (sample, complete) = manager.transitions.get_mut(&1).unwrap().sample(halfway);
        assert!(!complete);
        assert_eq!(
            manager.transitions.get_mut(&2).unwrap().sample(halfway).0,
            sample
        );
        let transition = manager.transitions.get_mut(&1).unwrap();
        transition.record_write(sample, false);
        assert_eq!(transition.last, from);
        transition.record_write(sample, true);
        assert_eq!(transition.last, sample);
        assert_eq!(
            transition.sample(now + Duration::from_millis(160)),
            (ax::integral_frame(target), true)
        );
    }

    #[test]
    fn replacing_a_template_cancels_only_its_windows_old_animation_and_retry() {
        let mut manager = Manager::default();
        for id in [1, 2] {
            let from = rect(10., 41., 1600., 900.);
            let target = rect(-890., -390., 880., 1580.);
            manager.transition_from(id, from, target);
            manager.verifications.insert(id, Verification::new(target));
            manager.applied.insert(id, target);
            manager.constrained.insert(id, (target, from));
        }
        manager.cancel_window_placement(1);
        assert!(!manager.transitions.contains_key(&1));
        assert!(!manager.verifications.contains_key(&1));
        assert!(!manager.applied.contains_key(&1));
        assert!(!manager.constrained.contains_key(&1));
        assert!(manager.transitions.contains_key(&2));
        assert!(manager.verifications.contains_key(&2));
        assert!(manager.applied.contains_key(&2));
        assert!(manager.constrained.contains_key(&2));
    }

    #[test]
    fn floating_restore_after_monitor_transfer_or_rotation_stays_on_destination() {
        let areas = [
            rect(10., 41., 1660., 947.),
            rect(-890., -390., 880., 1580.),
            rect(210., -1040., 1900., 1000.),
            rect(310., 1120., 1260., 760.),
        ];
        for source in areas {
            for destination in areas {
                for (x, y) in [(0., 0.), (0.5, 0.5), (1., 1.)] {
                    let original = rect(
                        source.origin.x + x * (source.size.width - 600.),
                        source.origin.y + y * (source.size.height - 500.),
                        600.,
                        500.,
                    );
                    let restored = restoration_frame(original, source, destination);
                    assert!(restored.origin.x >= destination.origin.x);
                    assert!(restored.origin.y >= destination.origin.y);
                    assert!(
                        restored.origin.x + restored.size.width
                            <= destination.origin.x + destination.size.width + 0.001
                    );
                    assert!(
                        restored.origin.y + restored.size.height
                            <= destination.origin.y + destination.size.height + 0.001
                    );
                    assert_eq!(restored.size, original.size);
                    if source == destination {
                        assert_eq!(restored, original);
                    }
                }
            }
        }
        let restored = restoration_frame(rect(10., 41., 1500., 900.), areas[0], areas[1]);
        assert_eq!(restored.size.width, areas[1].size.width);
        assert_eq!(restored.origin.x, areas[1].origin.x);
    }

    #[test]
    fn polling_detects_space_changes_without_confusing_order_or_geometry() {
        let a = ((1, 7), rect(0., 30., 1600., 900.));
        let b = ((2, 8), rect(-900., -400., 900., 1600.));
        assert!(!desktop_keys_changed(&[], &[a, b]));
        assert!(!desktop_keys_changed(&[a, b], &[b, a]));
        assert!(!desktop_keys_changed(&[a], &[(a.0, b.1)]));
        assert!(desktop_keys_changed(&[a, b], &[((1, 9), a.1), b]));
        assert!(desktop_keys_changed(&[a, b], &[a]));
    }

    #[test]
    fn space_change_freezes_layout_and_cancels_writes_even_with_mouse_held() {
        let mut manager = Manager::default();
        let drag = test_drag();
        let original = drag.baseline.frames(drag.area, 10.);
        manager.desktops.insert(drag.space, drag.baseline.clone());
        manager.transition_from(1, drag.start, original[0].1);
        manager.applied.insert(1, original[0].1);
        manager
            .verifications
            .insert(2, Verification::new(original[1].1));
        manager.applied.insert(2, original[1].1);
        manager.drag = Some(drag);
        manager.pressed = true;
        let now = Instant::now();
        manager.space_changed_at(now);
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
        assert!(manager.applied.is_empty());
        assert!(manager.drag.is_some());
        assert!(manager.pressed);
        assert_eq!(manager.tick_interval(), 0.08);
        assert!(manager.current_workspace().is_none());
        assert!(manager.picker_context().is_none());
        let context = PickerContext {
            workspace: (1, 7),
            area: rect(0., 30., 1600., 900.),
            desktop: manager.desktops[&(1, 7)].clone(),
        };
        assert!(!manager.apply_template(ptr::null_mut(), &context, Layout::Grid));
        assert!(manager.waiting_for_space(now + Duration::from_millis(100)));
        assert!(manager.drag.is_some());
        assert!(manager.pressed);
        assert_eq!(
            manager.desktops[&(1, 7)].frames(rect(0., 30., 1600., 900.), 10.),
            original
        );
        assert!(manager.waiting_for_space(now + Duration::from_millis(599)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(600)));
        assert!(manager.last_scan.is_none());
        assert!(manager.last_area_update.is_none());
    }

    #[test]
    fn consecutive_space_changes_restart_quiet_period_without_rebuilding_tree() {
        let mut manager = Manager::default();
        let drag = test_drag();
        manager.desktops.insert(drag.space, drag.baseline.clone());
        let before = drag.baseline.frames(drag.area, 10.);
        let now = Instant::now();
        manager.space_changed_at(now);
        manager.space_changed_at(now + Duration::from_millis(400));
        assert!(manager.waiting_for_space(now + Duration::from_millis(600)));
        assert!(manager.waiting_for_space(now + Duration::from_millis(999)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(1000)));
        assert_eq!(manager.desktops[&drag.space].frames(drag.area, 10.), before);
    }

    #[test]
    fn native_space_transition_does_not_consume_pending_drop_or_monitor_handoff_timeout() {
        let mut manager = Manager::default();
        let now = Instant::now();
        let source = (1, 7);
        let destination = (2, 8);
        manager.pending_drop = Some(PendingDrop {
            id: 1,
            source,
            destination,
            intent: None,
            expires: now + Duration::from_millis(700),
        });
        manager.handoffs.insert(
            1,
            MonitorHandoff {
                destination,
                expires: now + Duration::from_millis(1200),
            },
        );
        manager
            .verifications
            .insert(1, Verification::new(rect(0., 30., 800., 600.)));
        manager.space_changed_at(now);
        assert!(manager.verifications.is_empty());
        assert_eq!(
            manager.pending_drop.as_ref().unwrap().expires,
            now + Duration::from_millis(1300)
        );
        assert_eq!(
            manager.handoffs[&1].expires,
            now + Duration::from_millis(1800)
        );
        assert!(!manager.waiting_for_space(now + Duration::from_millis(600)));
        assert!(manager.pending_drop.is_some());
    }

    #[test]
    fn app_activation_does_not_retry_a_stable_constraint() {
        let mut manager = Manager::default();
        let target = rect(-890., -390., 880., 1580.);
        let actual = rect(-890., -390., 1100., 1580.);
        manager.constrained.insert(1, (target, actual));
        manager.applied.insert(1, target);
        for _ in 0..10 {
            // Same invalidate used by the NSWorkspace activation callback.
            manager.invalidate();
            assert!(!manager.needs_placement(1, target, Some(actual)));
        }
        assert!(manager.needs_placement(1, rect(-890., -390., 1200., 1580.), Some(actual)));
        assert!(manager.needs_placement(1, target, Some(rect(-800., -390., 1100., 1580.))));
    }

    #[test]
    fn stationary_click_and_small_pointer_jitter_cannot_resize_or_move_a_tile() {
        for frame in [rect(10., 40., 1200., 650.), rect(110., 140., 1400., 700.)] {
            let mut drag = test_drag();
            let before = drag.baseline.frames(drag.area, 10.);
            for point in [
                drag.point,
                NSPoint::new(drag.point.x + 2., drag.point.y - 2.),
            ] {
                drag.observe(frame, point, Some(drag.space));
                assert!(!drag.moved && !drag.resized);
                assert_eq!(drag.baseline.frames(drag.area, 10.), before);
            }
        }
        let mut drag = test_drag();
        drag.observe(drag.start, NSPoint::new(300., 100.), Some(drag.space));
        assert!(!drag.moved && !drag.resized); // Selection alone is not window motion.
    }

    #[test]
    fn brief_membership_read_failure_preserves_resized_tree_and_expires_if_persistent() {
        let mut grace = MembershipGrace::default();
        let now = Instant::now();
        let area = rect(0., 30., 1600., 900.);
        let mut desktop = Desktop::default();
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let from = desktop.frames(area, 10.)[0].1;
        let mut to = from;
        to.size.width += 100.;
        desktop.resize(1, from, to, area, 10.);
        let before = desktop.frames(area, 10.);
        let uncertain = HashSet::from([1]);
        for millis in [0, 100, 500, 649] {
            let retained = grace.retained(&uncertain, now + Duration::from_millis(millis));
            let mut members = vec![2, 3];
            members.extend(retained);
            desktop.reconcile_at(&members, area, 10., Some(2));
            assert_eq!(desktop.frames(area, 10.), before);
        }
        assert!(
            grace
                .retained(&uncertain, now + Duration::from_millis(650))
                .is_empty()
        );
        assert!(
            grace
                .retained(&uncertain, now + Duration::from_secs(5))
                .is_empty()
        );
        // Recovery resets the grace period for a later, independent failure.
        grace.retained(&HashSet::new(), now + Duration::from_secs(6));
        assert_eq!(
            grace.retained(&uncertain, now + Duration::from_secs(7)),
            uncertain
        );
    }

    #[test]
    fn forgotten_window_has_no_remaining_deferred_geometry_or_ownership() {
        let mut manager = Manager::default();
        let target = rect(0., 30., 800., 600.);
        manager.transition_from(1, rect(10., 50., 600., 400.), target);
        manager.verifications.insert(1, Verification::new(target));
        manager.constrained.insert(1, (target, target));
        manager.applied.insert(1, target);
        manager.floating.insert(1);
        manager.membership_grace.since.insert(1, Instant::now());
        manager.handoffs.insert(
            1,
            MonitorHandoff {
                destination: (1, 7),
                expires: Instant::now() + Duration::from_secs(1),
            },
        );
        manager.forget_window(1);
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
        assert!(manager.constrained.is_empty());
        assert!(manager.applied.is_empty());
        assert!(manager.floating.is_empty());
        assert!(manager.membership_grace.since.is_empty());
        assert!(manager.handoffs.is_empty());
    }

    #[test]
    fn new_destination_discards_retry_for_previous_monitor() {
        let mut manager = Manager::default();
        let old = rect(10., 30., 1500., 800.);
        let destination = rect(-890., -390., 880., 1580.);
        manager.verifications.insert(1, Verification::new(old));
        manager.transition_from(1, old, destination);
        assert!(!manager.verifications.contains_key(&1));
        assert_eq!(manager.transitions[&1].to, destination);
    }

    #[test]
    fn screen_changes_cancel_old_coordinates_and_keep_mouse_gesture_latched() {
        let mut manager = Manager::default();
        let old = rect(-890., -390., 880., 1580.);
        manager.transition_from(1, rect(10., 30., 1500., 800.), old);
        manager.verifications.insert(2, Verification::new(old));
        manager.applied.insert(1, old);
        manager.constrained.insert(2, (old, old));
        manager.handoffs.insert(
            1,
            MonitorHandoff {
                destination: (2, 7),
                expires: Instant::now() + Duration::from_secs(1),
            },
        );
        manager.drag = Some(test_drag());
        manager.pressed = true;
        manager.screens_changed();
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
        assert!(manager.applied.is_empty());
        assert!(manager.constrained.is_empty());
        assert!(manager.handoffs.is_empty());
        assert!(manager.drag.is_none());
        assert!(manager.pressed);
        assert!(manager.settling_space());
        assert!(manager.last_area_update.is_none());
    }

    #[test]
    fn work_area_changes_are_detected_without_confusing_space_switches_or_order() {
        let primary = ((1, 7), rect(10., 30., 1580., 860.));
        let portrait = ((2, 8), rect(-890., -390., 880., 1580.));
        assert!(!areas_changed(&[primary, portrait], &[portrait, primary]));
        assert!(!areas_changed(
            &[primary, portrait],
            &[((1, 9), primary.1), portrait]
        ));
        assert!(areas_changed(
            &[primary, portrait],
            &[primary, ((2, 8), rect(-890., -350., 880., 1540.))]
        ));
        assert!(areas_changed(
            &[primary],
            &[((1, 7), rect(10., 30., 1900., 1040.))]
        ));
    }

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
        verification.accepted = true;
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

    #[test]
    fn rejected_ax_writes_cannot_become_permanent_size_limits() {
        let mut verification = Verification::new(rect(10., 41., 825., 947.));
        let old = rect(10., 41., 1660., 947.);
        let due = verification.due;
        assert_eq!(verification.step(old, due), VerificationStep::Retry);
        assert_eq!(
            verification.step(old, due + Duration::from_secs(1)),
            VerificationStep::Rejected
        );
        verification.accepted = true; // A fresh AX proxy recovers transport.
        assert_eq!(
            verification.step(verification.target, due + Duration::from_secs(2)),
            VerificationStep::Complete
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
    fn native_geometry_resolves_neighbor_handle_and_latches_actual_owner() {
        let mut previous_focus = test_drag();
        previous_focus.start = rect(0., 30., 790., 900.);
        previous_focus.point = NSPoint::new(795., 400.);
        let mut actual = previous_focus.clone();
        actual.id = 2;
        actual.start = rect(800., 30., 800., 900.);
        let mut selected = Some(previous_focus.clone());
        let mut candidates = vec![actual.clone()];
        let point = NSPoint::new(840., 400.);
        resolve_gesture(
            &mut selected,
            &mut candidates,
            point,
            Some(actual.space),
            |id| {
                Some(if id == 1 {
                    previous_focus.start
                } else {
                    rect(845., 30., 755., 900.)
                })
            },
        );
        assert_eq!(selected.as_ref().unwrap().id, 2);
        assert!(selected.as_ref().unwrap().resized);
        resolve_gesture(
            &mut selected,
            &mut candidates,
            point,
            Some(actual.space),
            |_| None,
        );
        assert_eq!(selected.unwrap().id, 2);
        // The same resolution works when mouse-up is the first geometry sample.
        let mut selected = None;
        resolve_gesture(
            &mut selected,
            &mut [actual.clone()],
            point,
            Some(actual.space),
            |_| Some(rect(845., 30., 755., 900.)),
        );
        assert_eq!(selected.unwrap().id, 2);
    }
    #[test]
    fn release_uses_current_slot_even_before_preview_hysteresis_expires() {
        let mut drag = test_drag();
        let now = Instant::now();
        let first = Some((drag.space, DropIntent::Window(2, Zone::Center)));
        let last = Some((drag.space, DropIntent::Window(3, Zone::Center)));
        drag.update_drop(first, now, false);
        drag.update_drop(last, now + Duration::from_millis(10), false);
        assert_eq!(drag.drop, first);
        drag.update_drop(last, now + Duration::from_millis(11), true);
        assert_eq!(drag.drop, last);
        drag.update_drop(None, now + Duration::from_millis(12), true);
        assert_eq!(drag.drop, None);
    }
    #[test]
    fn arrival_checks_incoming_minimum_before_committing_a_monitor_handoff() {
        let area = rect(-1080., -500., 1080., 1920.);
        let mut destination = Desktop::configured(Layout::VerticalMaster, None);
        destination.reconcile(&[1, 2, 3, 4], area, 10.);
        let before = destination.frames(area, 10.);
        let minimums = [(99, objc2_foundation::NSSize::new(1200., 200.))]
            .into_iter()
            .collect();
        assert!(
            arrival_proposal(
                &destination,
                99,
                Some(DropIntent::Window(2, Zone::Center)),
                area,
                Some(&minimums)
            )
            .is_none()
        );
        assert_eq!(destination.frames(area, 10.), before);
        let minimums = [(99, objc2_foundation::NSSize::new(500., 200.))]
            .into_iter()
            .collect();
        let proposed = arrival_proposal(
            &destination,
            99,
            Some(DropIntent::Window(2, Zone::Center)),
            area,
            Some(&minimums),
        )
        .unwrap();
        assert_eq!(proposed.frames(area, 10.).len(), 5);
        assert_eq!(destination.frames(area, 10.), before);
    }
    #[test]
    fn native_resize_samples_are_transactional_and_do_not_accumulate_feedback() {
        for layout in Layout::ALL {
            for grid in [None, Some((17, 9))] {
                for area in [
                    rect(10., 41., 2560., 1440.),
                    rect(-1440., -600., 1440., 2560.),
                ] {
                    let mut baseline = Desktop::configured(layout, grid);
                    baseline.reconcile(&[1, 2, 3, 4, 5], area, 10.);
                    let original = baseline.frames(area, 10.);
                    for (id, from) in &original {
                        for edge in [Zone::Left, Zone::Right, Zone::Top, Zone::Bottom] {
                            let mut drag = test_drag();
                            drag.id = *id;
                            drag.baseline = baseline.clone();
                            drag.area = area;
                            drag.start = *from;
                            drag.point = match edge {
                                Zone::Left => NSPoint::new(
                                    from.origin.x,
                                    from.origin.y + from.size.height / 2.,
                                ),
                                Zone::Right => NSPoint::new(
                                    from.origin.x + from.size.width,
                                    from.origin.y + from.size.height / 2.,
                                ),
                                Zone::Top => NSPoint::new(
                                    from.origin.x + from.size.width / 2.,
                                    from.origin.y,
                                ),
                                _ => NSPoint::new(
                                    from.origin.x + from.size.width / 2.,
                                    from.origin.y + from.size.height,
                                ),
                            };
                            let mut fresh = drag.clone();
                            for delta in [8., 20., 45., 20., 45.] {
                                let mut frame = *from;
                                let mut point = drag.point;
                                match edge {
                                    Zone::Left => {
                                        frame.origin.x += delta;
                                        frame.size.width -= delta;
                                        point.x += delta;
                                    }
                                    Zone::Right => {
                                        frame.size.width += delta;
                                        point.x += delta;
                                    }
                                    Zone::Top => {
                                        frame.origin.y += delta;
                                        frame.size.height -= delta;
                                        point.y += delta;
                                    }
                                    _ => {
                                        frame.size.height += delta;
                                        point.y += delta;
                                    }
                                }
                                let proposed = drag
                                    .resize_proposal(frame, point, Some(drag.space))
                                    .unwrap();
                                let direct = fresh
                                    .resize_proposal(frame, point, Some(fresh.space))
                                    .unwrap();
                                let frames = proposed.frames(area, 10.);
                                assert_eq!(
                                    frames,
                                    direct.frames(area, 10.),
                                    "{layout:?}, {edge:?}"
                                );
                                assert_eq!(drag.baseline.frames(area, 10.), original);
                                let mut committed = proposed.clone();
                                committed.reconcile(&[5, 4, 3, 2, 1], area, 10.);
                                assert_eq!(committed.frames(area, 10.), frames);
                                let swapped = committed
                                    .proposed_drop(
                                        1,
                                        DropIntent::Window(2, Zone::Center),
                                        area,
                                        10.,
                                    )
                                    .unwrap();
                                assert_eq!(
                                    frames.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                    swapped
                                        .frames(area, 10.)
                                        .iter()
                                        .map(|(_, r)| *r)
                                        .collect::<Vec<_>>()
                                );
                            }
                            assert!(
                                drag.resize_proposal(*from, drag.point, Some((99, 99)))
                                    .is_none()
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn crossing_to_portrait_does_not_resize_the_source_tree() {
        let mut drag = test_drag();
        let original = drag.baseline.frames(drag.area, 10.);
        drag.observe(
            rect(40., 60., 1500., 800.),
            NSPoint::new(230., 65.),
            Some((1, 7)),
        );
        assert!(drag.moved && !drag.resized);
        drag.observe(
            rect(-850., -300., 850., 1300.),
            NSPoint::new(-650., -295.),
            Some((2, 8)),
        );
        assert!(drag.moved && !drag.resized);
        assert_eq!(drag.baseline.frames(drag.area, 10.), original);
        let mut fast = test_drag();
        fast.observe(
            rect(-850., -300., 850., 1300.),
            NSPoint::new(-650., -295.),
            Some((2, 8)),
        );
        assert!(fast.moved && !fast.resized);
    }
    #[test]
    fn genuine_resize_handles_remain_resizes() {
        for frame in [rect(10., 40., 1200., 650.), rect(110., 140., 1400., 700.)] {
            let mut drag = test_drag();
            drag.observe(frame, NSPoint::new(300., 145.), Some((1, 7)));
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
        let proposed = arrival_proposal(&dest, 1, None, area, None).unwrap();
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
            assert_eq!(layout_pref(raw, (1, 8)), Layout::Spiral);
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
    fn picker_template_then_drag_to_any_slot_preserves_preview_and_virtual_grid() {
        for area in [
            rect(0., 41., 1800., 1000.),
            rect(-1200., -400., 1200., 1800.),
        ] {
            for grid in [None, Some((120, 6))] {
                let mut initial = Desktop::configured(Layout::Spiral, grid);
                initial.reconcile(&[1, 2, 3, 4, 5], area, 10.);
                let context = PickerContext {
                    workspace: (1, 7),
                    area,
                    desktop: initial,
                };
                for layout in Layout::ALL {
                    let committed = context.proposal(layout).unwrap();
                    let slots = committed.frames(area, 10.);
                    let source = slots[0].0;
                    for (target, frame) in &slots {
                        if *target == source {
                            continue;
                        }
                        // Includes the region that previously requested a new split.
                        for point in [
                            NSPoint::new(frame.origin.x + 2., frame.origin.y + 2.),
                            NSPoint::new(
                                frame.origin.x + frame.size.width / 2.,
                                frame.origin.y + frame.size.height / 2.,
                            ),
                        ] {
                            let intent = drop_intent(area, &slots, source, point, true).unwrap();
                            let preview =
                                committed.proposed_drop(source, intent, area, 10.).unwrap();
                            assert_eq!(preview.layout, layout);
                            let after = preview.frames(area, 10.);
                            assert_eq!(
                                after.iter().find(|(id, _)| *id == source).unwrap().1,
                                *frame,
                                "{layout:?}: drag must use the target slot"
                            );
                            let mut released =
                                arrival_proposal(&committed, source, Some(intent), area, None)
                                    .unwrap();
                            released.reconcile(&[5, 4, 3, 2, 1], area, 10.);
                            released.fit(area, 10.);
                            assert_eq!(released.frames(area, 10.), after);
                            assert_eq!(
                                slots.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                after.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                "{layout:?}: drag changed the template geometry"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn mixed_templates_drag_and_save_are_isolated_across_monitors_and_spaces() {
        let areas = [
            ((1, 7), rect(10., 41., 3200., 1700.)),
            ((1, 9), rect(10., 41., 3200., 1700.)),
            ((2, 8), rect(-1440., -500., 1420., 2500.)),
            ((2, 10), rect(-1440., -500., 1420., 2500.)),
            ((3, 7), rect(3210., 200., 1900., 1100.)),
        ];
        for offset in 0..Layout::ALL.len() {
            let mut desktops = HashMap::new();
            for (index, (key, area)) in areas.iter().enumerate() {
                let layout = Layout::ALL[(offset + index) % Layout::ALL.len()];
                let ids: Vec<_> = (1..=layout.example_count().max(5))
                    .map(|id| id + index as u32 * 100)
                    .collect();
                let mut desktop =
                    Desktop::configured(layout, if index % 2 == 0 { Some((120, 6)) } else { None });
                desktop.reconcile(&ids, *area, 10.);
                desktops.insert(*key, desktop);
            }
            for (key, area) in areas {
                let unaffected: HashMap<_, _> = areas
                    .iter()
                    .filter(|(other, _)| *other != key)
                    .map(|(other, r)| (*other, desktops[other].save_distribution(*r)))
                    .collect();
                let context = PickerContext {
                    workspace: key,
                    area,
                    desktop: desktops[&key].clone(),
                };
                let committed = context.proposal(context.desktop.layout).unwrap();
                let frames = committed.frames(area, 10.);
                let source = frames[0].0;
                let (target, slot) = frames.last().copied().unwrap();
                let point = NSPoint::new(slot.origin.x + 2., slot.origin.y + 2.);
                let intent = drop_intent(area, &frames, source, point, true).unwrap();
                let proposed =
                    arrival_proposal(&committed, source, Some(intent), area, None).unwrap();
                assert_eq!(
                    proposed
                        .frames(area, 10.)
                        .iter()
                        .find(|(id, _)| *id == source)
                        .unwrap()
                        .1,
                    slot
                );
                assert_eq!(proposed.order.len(), committed.order.len());
                assert_ne!(source, target);
                let saved = proposed.save_distribution(area).unwrap();
                let mut restored = proposed.clone();
                restored.reset_template(proposed.layout, area);
                assert!(restored.restore_distribution(&saved));
                assert_eq!(restored.frames(area, 10.), proposed.frames(area, 10.));
                desktops.insert(key, restored);
                for (other, other_area) in areas {
                    if other != key {
                        assert_eq!(
                            desktops[&other].save_distribution(other_area),
                            unaffected[&other]
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn centered_live_resize_uses_pointer_not_its_own_mirrored_feedback() {
        let from = rect(400., 40., 1200., 1600.);
        let start = rect(400., 40., 1220., 1600.);
        let press = NSPoint::new(1620., 500.);
        let point = NSPoint::new(1650., 500.);
        let first = centered_resize_request(from, start, press, point, start, true);
        let mirrored = rect(350., 40., 1300., 1600.);
        assert_eq!(
            centered_resize_request(from, start, press, point, mirrored, true),
            first
        );
        assert_eq!(first.size.width, 1250.);
        let vertical = rect(-900., 400., 1200., 1200.);
        let requested = centered_resize_request(
            vertical,
            vertical,
            NSPoint::new(-500., 1600.),
            NSPoint::new(-500., 1640.),
            vertical,
            false,
        );
        assert_eq!(requested.size.height, 1240.);
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
                _geometry_session: None,
                from,
                to,
                last: from,
                started: None,
            },
        );
        manager.space_changed_at(Instant::now());
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
                _geometry_session: None,
                from,
                to,
                last: from,
                started: None,
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
                _geometry_session: None,
                from,
                to,
                started: None,
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
