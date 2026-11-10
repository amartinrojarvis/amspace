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

#[cfg(test)]
mod robustness_tests;

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
pub(crate) fn pointer_display() -> Option<u32> {
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
pub(crate) fn area(display: u32) -> Option<NSRect> {
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
    previews: DropPreviewCache,
}
#[derive(Clone, Copy)]
struct DropContext<'a> {
    workspace: Workspace,
    id: u32,
    area: NSRect,
    desktop: &'a Desktop,
    minimums: &'a HashMap<u32, objc2_foundation::NSSize>,
}
#[derive(Clone)]
struct PreviewContext {
    workspace: Workspace,
    id: u32,
    area: NSRect,
    desktop: Desktop,
    minimums: HashMap<u32, objc2_foundation::NSSize>,
    slots: Vec<(u32, NSRect)>,
    proposals: Vec<(DropIntent, Option<NSRect>)>,
}
#[derive(Clone, Default)]
struct DropPreviewCache {
    contexts: Vec<PreviewContext>,
}
impl DropPreviewCache {
    fn context(&mut self, input: DropContext<'_>) -> &mut PreviewContext {
        let found = self.contexts.iter().position(|c| {
            c.workspace == input.workspace
                && c.id == input.id
                && c.area == input.area
                && &c.desktop == input.desktop
                && &c.minimums == input.minimums
        });
        if let Some(index) = found {
            let last = self.contexts.len() - 1;
            self.contexts.swap(index, last);
        } else {
            self.contexts.retain(|c| c.workspace != input.workspace);
            // A bounded cache covers the current and previous monitor during
            // preview hysteresis. It never survives the native gesture.
            if self.contexts.len() == 2 {
                self.contexts.remove(0);
            }
            self.contexts.push(PreviewContext {
                workspace: input.workspace,
                id: input.id,
                area: input.area,
                desktop: input.desktop.clone(),
                minimums: input.minimums.clone(),
                slots: input.desktop.frames(input.area, 10.),
                proposals: Vec::new(),
            });
        }
        self.contexts.last_mut().unwrap()
    }
    fn slots(&mut self, input: DropContext<'_>) -> &[(u32, NSRect)] {
        &self.context(input).slots
    }
    fn preview(&mut self, input: DropContext<'_>, intent: DropIntent) -> Option<NSRect> {
        let context = self.context(input);
        if let Some((_, frame)) = context.proposals.iter().find(|(key, _)| *key == intent) {
            return *frame;
        }
        let frame = arrival_proposal(
            input.desktop,
            input.id,
            Some(intent),
            input.area,
            Some(input.minimums),
        )
        .and_then(|d| {
            d.frames(input.area, 10.)
                .into_iter()
                .find(|(id, _)| *id == input.id)
                .map(|(_, r)| r)
        });
        if context.proposals.len() == 32 {
            context.proposals.remove(0);
        }
        context.proposals.push((intent, frame));
        frame
    }
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
    if selected.as_ref().is_some_and(|d| d.moved || d.resized) {
        return;
    }
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
#[derive(Clone, PartialEq)]
struct ReentrySample {
    areas: Vec<(Workspace, NSRect)>,
    windows: Vec<(i32, u32, u64, NSRect, Option<NSRect>)>,
    absent: Vec<(i32, u32)>,
}
impl ReentrySample {
    fn agrees(&self, other: &Self) -> bool {
        self.areas == other.areas
            && self.absent == other.absent
            && self.windows.len() == other.windows.len()
            && self.windows.iter().zip(&other.windows).all(|(a, b)| {
                a.0 == b.0
                    && a.1 == b.1
                    && a.2 == b.2
                    && near(a.3, b.3)
                    && match (a.4, b.4) {
                        (Some(a), Some(b)) => near(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
    }
}
#[derive(Default)]
struct SpaceReentry {
    anchor: Option<(ReentrySample, Instant)>,
    incomplete: u8,
}
impl SpaceReentry {
    fn observe(&mut self, sample: Option<ReentrySample>, now: Instant) -> bool {
        let Some(sample) = sample else {
            // None is an incomplete read, not an empty desktop, so the anchor
            // restarts. A bounded burst of them still resumes the normal scan
            // from a fresh list instead of keeping the fast timer forever.
            self.anchor = None;
            self.incomplete += 1;
            return self.incomplete >= 8;
        };
        self.incomplete = 0;
        if let Some((previous, since)) = &self.anchor {
            if previous.agrees(&sample) {
                return now.duration_since(*since) >= Duration::from_millis(160);
            }
        }
        self.anchor = Some((sample, now));
        false
    }
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
    fn retained(
        &mut self,
        uncertain: &HashSet<u32>,
        native_confirmed: &HashSet<u32>,
        now: Instant,
    ) -> HashSet<u32> {
        self.since
            .retain(|id, _| uncertain.contains(id) && !native_confirmed.contains(id));
        uncertain
            .iter()
            .copied()
            .filter(|id| {
                if native_confirmed.contains(id) {
                    return true;
                }
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
    space_reentry: Option<SpaceReentry>,
    reentry_preserved: Vec<(Workspace, NSRect)>,
    pub last_error: Option<String>,
}
pub struct KeyboardTransfer {
    pub source: Workspace,
    pub destination: Workspace,
    pub target: NSRect,
    source_area: NSRect,
    destination_area: NSRect,
    desktop: Option<Desktop>,
    tracked: bool,
    floating: bool,
}
pub struct KeyboardPlacement {
    pub pid: i32,
    pub id: u32,
    pub element: ax::Owned,
    pub target: NSRect,
}
impl KeyboardTransfer {
    /// Only a validated tiled proposal may resize the hidden destination.
    pub fn background_frames(&self) -> Vec<(u32, NSRect)> {
        self.desktop
            .as_ref()
            .map(|desktop| desktop.frames(self.destination_area, 10.))
            .unwrap_or_default()
    }
}
impl Manager {
    pub fn diagnostic_report(&self) -> String {
        let mut report = format!(
            "\nMosaic paused={:?} reentry={} quiet={}\n",
            self.presentation,
            self.space_reentry.is_some(),
            self.space_quiet_until.is_some()
        );
        let mut keys: Vec<_> = self.desktops.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            if !self.active_areas.iter().any(|(active, _)| *active == key) {
                continue;
            }
            let desktop = &self.desktops[&key];
            report.push_str(&format!(
                "Workspace {key:?} layout={:?} grid={:?} order={:?}\n",
                desktop.layout, desktop.grid, desktop.order
            ));
            if let Some((_, area)) = self.active_areas.iter().find(|(active, _)| *active == key) {
                report.push_str(&format!(
                    "Area {area:?} tree={:?}\n",
                    desktop.save_distribution(*area)
                ));
            }
            for id in &desktop.order {
                if let Some(window) = self.windows.get(id) {
                    report.push_str(&format!("Window {id} pid={} native={:?} AX={:?} applied={:?} constrained={:?} minimum={:?} transition={} verification={}\n",window.pid,spaces::window_space(*id),ax::geometry(window.element.0),self.applied.get(id),self.constrained.get(id),self.minimum_sizes.get(id),self.transitions.contains_key(id),self.verifications.contains_key(id)));
                }
            }
        }
        report
    }
    pub fn keyboard_transfer_windows(&self, plan: &KeyboardTransfer) -> Vec<KeyboardPlacement> {
        let Some(identities) = ws::all_window_ids() else {
            return Vec::new();
        };
        plan.background_frames()
            .into_iter()
            .filter_map(|(id, target)| {
                let (pid, _) = *identities.iter().find(|(_, window)| *window == id)?;
                let element = self
                    .windows
                    .get(&id)
                    .filter(|window| window.pid == pid)
                    .map(|window| window.element.clone())
                    .or_else(|| ax::window_for_id(pid, id))?;
                Some(KeyboardPlacement {
                    pid,
                    id,
                    element,
                    target,
                })
            })
            .collect()
    }
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
        self.space_reentry = Some(SpaceReentry::default());
        self.reentry_preserved.clear();
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
        self.reentry_preserved = self
            .active_areas
            .iter()
            .copied()
            .filter(|(key, _)| visible.contains(key))
            .collect();
        self.space_reentry = Some(SpaceReentry::default());
        self.cancel_inactive_placements(visible);
        // A gesture baseline predates the presentation change, even when it
        // belongs to another monitor. Never finalize it from sliding bounds or
        // a delayed mouse-up. Keep pressed latched until the physical release.
        self.drag = None;
        self.gesture_candidates.clear();
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
    fn reentry_sample(
        &self,
        defaults: Obj,
        active: &[(Workspace, NSRect)],
    ) -> Option<ReentrySample> {
        let snapshot = ws::transition_windows()?;
        let candidates: Vec<_> = snapshot
            .iter()
            .filter(|w| w.layer == 0 && w.onscreen && w.pid != std::process::id() as i32)
            .filter_map(|w| Some((w.pid, w.id, w.frame?)))
            .collect();
        // The burst samples every window twice or thrice; one native read per
        // id keeps this frame's membership self-consistent and cheap.
        let mut space_of: HashMap<u32, Option<u64>> = HashMap::new();
        let visible: HashSet<_> = candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect();
        let expected: Vec<_> = self
            .desktops
            .iter()
            .filter(|(key, _)| active.iter().any(|(s, _)| s == *key))
            .flat_map(|(_, d)| d.order.iter())
            .filter_map(|id| self.windows.get(id).map(|w| (*id, w)))
            .filter(|(id, _)| !self.floating.contains(id) && !self.inactive_tabs.contains(id))
            .collect();
        let all: HashSet<_> = snapshot.iter().map(|w| (w.pid, w.id)).collect();
        let mut absent = Vec::new();
        let expected_ids: HashSet<_> = expected.iter().map(|(id, w)| (w.pid, *id)).collect();
        for (id, w) in expected {
            if visible.contains(&(w.pid, id)) {
                continue;
            }
            if snapshot
                .iter()
                .any(|entry| entry.pid == w.pid && entry.id == id && entry.layer != 0)
            {
                continue;
            }
            let ordered = ws::ordered_in(id);
            if ordered == Some(false) {
                continue;
            }
            let native = window_space_cached(id, &mut space_of);
            if native.is_some_and(|native| native != w.space.1) {
                continue;
            }
            // Switching an AppKit tab while inactive can replace the selected
            // NSWindow ID. Its shared group, not a missing old ID, owns the slot.
            if let Some(group) = self
                .tab_groups
                .get(&id)
                .cloned()
                .or_else(|| ax::native_tab_group(w.element.0))
            {
                let replaced = candidates.iter().any(|(pid, other, _)| {
                    if *pid != w.pid
                        || *other == id
                        || window_space_cached(*other, &mut space_of) != Some(w.space.1)
                    {
                        return false;
                    }
                    let other_group = self.tab_groups.get(other).cloned().or_else(|| {
                        let element = self
                            .windows
                            .get(other)
                            .map(|w| w.element.clone())
                            .or_else(|| ax::window_for_id(*pid, *other))?;
                        ax::native_tab_group(element.0)
                    });
                    other_group.is_some_and(|other| unsafe { ax::CFEqual(group.0, other.0) })
                });
                if replaced {
                    continue;
                }
            }
            // Existing windows cannot vanish from the model merely because the
            // compositor has not published them on screen yet. A genuine hide
            // or minimization is distinct from that incomplete arrival.
            let hidden = unsafe {
                let app: Obj = msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:w.pid];
                !app.is_null() && msg_send![app, isHidden]
            };
            if hidden || ax::flag(w.element.0, "AXMinimized") {
                continue;
            }
            if ordered == Some(true) || ws::bounds(id).is_some() || all.contains(&(w.pid, id)) {
                return None;
            }
            absent.push((w.pid, id));
        }
        let mut windows = Vec::new();
        let mut floating_apps = HashMap::new();
        // The first visit since launch has no cached desktop order. Ordered-in
        // offscreen windows on the destination still count as arrivals; do not
        // build an initial layout from just the first one macOS exposes.
        for entry in snapshot.iter().filter(|w| {
            w.layer == 0 && !w.onscreen && w.frame.is_some() && w.pid != std::process::id() as i32
        }) {
            if expected_ids.contains(&(entry.pid, entry.id))
                || self.floating.contains(&entry.id)
                || self.inactive_tabs.contains(&entry.id)
                || *floating_apps
                    .entry(entry.pid)
                    .or_insert_with(|| crate::tile_rules::floats(defaults, entry.pid))
                || ws::ordered_in(entry.id) != Some(true)
            {
                continue;
            }
            let Some(native) = window_space_cached(entry.id, &mut space_of) else {
                continue;
            };
            if !active.iter().any(|(key, _)| key.1 == native) {
                continue;
            }
            let hidden = unsafe {
                let app: Obj = msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:entry.pid];
                !app.is_null() && msg_send![app, isHidden]
            };
            if hidden {
                continue;
            }
            let element = ax::window_for_id(entry.pid, entry.id)?;
            if ax::tileable(element.0) {
                return None;
            }
        }
        for (pid, id, cg) in candidates {
            if self.floating.contains(&id)
                || *floating_apps
                    .entry(pid)
                    .or_insert_with(|| crate::tile_rules::floats(defaults, pid))
            {
                continue;
            }
            let native = match window_space_cached(id, &mut space_of) {
                Some(native) => native,
                None if expected_ids.contains(&(pid, id)) => return None,
                None => continue,
            };
            if !active.iter().any(|(key, _)| key.1 == native) && !expected_ids.contains(&(pid, id))
            {
                continue;
            }
            let ax = if let Some(w) = self.windows.get(&id).filter(|w| w.pid == pid) {
                // AX wrappers can expire while inactive. Use the same fresh-ID
                // recovery as reconciliation instead of waiting on a dead one.
                let (p, s) = ax::geometry(w.element.0).or_else(|| {
                    let fresh = ax::window_for_id(pid, id)?;
                    ax::tileable(fresh.0).then(|| ax::geometry(fresh.0))?
                })?;
                Some(NSRect::new(p, s))
            } else {
                None
            };
            windows.push((pid, id, native, cg, ax));
        }
        windows.sort_by_key(|w| (w.0, w.1));
        absent.sort_unstable();
        Some(ReentrySample {
            areas: active.to_vec(),
            windows,
            absent,
        })
    }
    fn observe_reentry(&mut self, sample: Option<ReentrySample>, now: Instant) -> bool {
        let preserved: Vec<_> = self
            .reentry_preserved
            .iter()
            .filter(|area| {
                sample
                    .as_ref()
                    .is_some_and(|sample| sample.areas.contains(area))
            })
            .map(|(key, _)| *key)
            .collect();
        let Some(reentry) = self.space_reentry.as_mut() else {
            return true;
        };
        if !reentry.observe(sample, now) {
            return false;
        }
        self.space_reentry = None;
        // Rebuild writes for contexts that changed. An independent monitor
        // retains valid targets and must finish (not restart) its bounded retry.
        self.cancel_inactive_placements(&preserved);
        self.reentry_preserved.clear();
        self.space_settle = Some(now + Duration::from_millis(600));
        self.invalidate();
        true
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
                let visible: bool = msg_send![&**w, isVisible];
                if visible {
                    let _: () = msg_send![&**w,orderOut:ptr::null::<AnyObject>()];
                }
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
                let _: () = msg_send![&p,setAnimationBehavior:2isize];
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
                    let visible: bool = msg_send![&**p, isVisible];
                    if visible {
                        let _: () = msg_send![&**p,orderOut:ptr::null::<AnyObject>()];
                    }
                }
            }
        }
    }
    pub fn prepare_keyboard_transfer(
        &mut self,
        defaults: Obj,
        pid: i32,
        id: u32,
        source: Workspace,
        destination: Workspace,
        frame: NSRect,
    ) -> Result<KeyboardTransfer, &'static str> {
        let tracked = pref(defaults, "tile.enabled") != 0;
        if self.presentation_blocks_input()
            || (tracked && (self.space_reentry.is_some() || self.waiting_for_space(Instant::now())))
        {
            return Err("Espera a que termine el cambio de escritorio.");
        }
        let source_area = area(source.0).ok_or("La pantalla de origen ya no está disponible.")?;
        let destination_area =
            area(destination.0).ok_or("La pantalla de destino ya no está disponible.")?;
        let floating = self.floating.contains(&id) || crate::tile_rules::floats(defaults, pid);
        let tiled =
            tracked && pref(defaults, &format!("tile.off.{}", destination.1)) == 0 && !floating;
        let desktop =
            if tiled {
                let mut base = self.desktops.get(&destination).cloned().unwrap_or_else(|| {
                    Desktop::configured(
                        layout_pref(defaults, destination),
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
                let snapshot = ws::transition_windows()
                    .ok_or("No se puede comprobar el escritorio de destino.")?;
                // Refresh known membership without treating an unavailable AX
                // listing as destruction of a customized hidden layout.
                let mut ids: Vec<_> = base
                    .order
                    .iter()
                    .copied()
                    .filter(|known| {
                        *known != id
                            && !self.floating.contains(known)
                            && spaces::window_space(*known)
                                .is_none_or(|space| space == destination.1)
                            && ws::ordered_in(*known) != Some(false)
                            && snapshot.iter().any(|window| {
                                window.id == *known
                                    && !crate::tile_rules::floats(defaults, window.pid)
                                    && window.frame.is_none_or(|frame| {
                                        overlap(frame, destination_area)
                                            > frame.size.width * frame.size.height * 0.5
                                    })
                            })
                    })
                    .collect();
                for window in snapshot
                    .into_iter()
                    .filter(|w| w.layer == 0 && w.pid != std::process::id() as i32 && w.id != id)
                {
                    if window.frame.is_none_or(|frame| {
                        overlap(frame, destination_area)
                            <= frame.size.width * frame.size.height * 0.5
                    }) {
                        continue;
                    }
                    if spaces::window_space(window.id) != Some(destination.1)
                        || ws::ordered_in(window.id) == Some(false)
                        || self.floating.contains(&window.id)
                        || crate::tile_rules::floats(defaults, window.pid)
                    {
                        continue;
                    }
                    // A native layer-0 entry is not proof of a tileable
                    // window: auxiliary overlays and hidden AX windows also
                    // appear here. Never let an unverified entry veto the
                    // entire transfer, or resize it without AX confirmation.
                    let element = self
                        .windows
                        .get(&window.id)
                        .filter(|managed| managed.pid == window.pid)
                        .map(|managed| managed.element.clone())
                        .or_else(|| ax::window_for_id(window.pid, window.id));
                    let eligible = match element {
                        Some(element) => {
                            ax::window_id(element.0) == Some(window.id)
                                && ax::tileable(element.0)
                                && !ax::flag(element.0, "AXMinimized")
                                && !ax::flag(element.0, "AXFullScreen")
                        }
                        // A known tile may temporarily disappear from AX.
                        // Preserve its slot while native membership holds.
                        None => base.order.contains(&window.id),
                    };
                    if eligible && !ids.contains(&window.id) {
                        ids.push(window.id);
                    }
                }
                ids.sort_unstable();
                base.reconcile_at(&ids, destination_area, 10., None);
                Some(arrival_proposal(&base,id,None,destination_area,Some(&self.minimum_sizes))
                .ok_or("La ventana no cabe en la distribución de destino; no se ha movido nada.")?)
            } else {
                None
            };
        let target = desktop
            .as_ref()
            .and_then(|d| {
                d.frames(destination_area, 10.)
                    .into_iter()
                    .find(|(window, _)| *window == id)
                    .map(|(_, frame)| frame)
            })
            .unwrap_or_else(|| restoration_frame(frame, source_area, destination_area));
        self.cancel_drag();
        let affected: Vec<_> = self
            .windows
            .iter()
            .filter(|(_, w)| w.space == source || w.space == destination)
            .map(|(id, _)| *id)
            .collect();
        for id in affected {
            self.cancel_window_placement(id);
        }
        Ok(KeyboardTransfer {
            source,
            destination,
            target,
            source_area,
            destination_area,
            desktop,
            tracked,
            floating,
        })
    }
    /// Commit only after native arrival and the bounded background placement.
    /// Normal reentry still reconciles windows that refused hidden AX writes.
    pub fn commit_keyboard_transfer(
        &mut self,
        plan: KeyboardTransfer,
        pid: i32,
        id: u32,
        element: ax::Owned,
        original: NSRect,
    ) {
        if area(plan.destination.0) != Some(plan.destination_area) {
            self.invalidate();
            return;
        }
        self.commit_keyboard_transfer_model(plan, pid, id, element, original);
    }
    fn commit_keyboard_transfer_model(
        &mut self,
        plan: KeyboardTransfer,
        pid: i32,
        id: u32,
        element: ax::Owned,
        original: NSRect,
    ) {
        self.cancel_window_placement(id);
        for (key, desktop) in &mut self.desktops {
            if *key != plan.destination {
                desktop.remove(id);
            }
        }
        let destination_area = plan.destination_area;
        if let Some(desktop) = plan.desktop {
            self.desktops.insert(plan.destination, desktop);
        }
        if plan.tracked {
            if plan.floating {
                self.floating.insert(id);
            }
            if let Some(window) = self.windows.get_mut(&id).filter(|w| w.pid == pid) {
                window.original =
                    restoration_frame(window.original, window.original_area, destination_area);
                window.original_area = destination_area;
                window.space = plan.destination;
                window.element = element;
            } else {
                self.windows.insert(
                    id,
                    Managed {
                        pid,
                        space: plan.destination,
                        original: restoration_frame(original, plan.source_area, destination_area),
                        original_area: destination_area,
                        element,
                    },
                );
            }
            self.handoffs.insert(
                id,
                MonitorHandoff {
                    destination: plan.destination,
                    expires: Instant::now() + Duration::from_millis(1200),
                },
            );
        }
        self.membership_grace.since.remove(&id);
        self.invalidate();
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
    fn needs_placement(
        &self,
        id: u32,
        target: NSRect,
        actual: Option<NSRect>,
        settling: bool,
    ) -> bool {
        let pending = self
            .transitions
            .get(&id)
            .map(|t| t.to)
            .or_else(|| self.verifications.get(&id).map(|v| v.target))
            // While a Space is still publishing its windows, the committed
            // destination is the pending one too: the compositor can report the
            // pre-slide frame twice, and re-writing an identical rect only adds
            // a second visible step. The bounded settle window keeps the repair
            // for real drift on the first scan after it expires.
            .or_else(|| settling.then(|| self.applied.get(&id).copied()).flatten());
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
    fn animate(
        &mut self,
        active: &[(Workspace, NSRect)],
        candidates: &[(i32, u32, NSRect)],
        space_of: &mut HashMap<u32, Option<u64>>,
    ) {
        if self.transitions.is_empty() {
            return;
        }
        // The tick's single compositor snapshot replaces synchronous AX flag
        // reads per frame/window.
        let visible: HashSet<_> = candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect();
        let mut done = vec![];
        let now = Instant::now();
        for (id, t) in &mut self.transitions {
            let Some(w) = self.windows.get(id) else {
                done.push(*id);
                continue;
            };
            if !active.iter().any(|(space, _)| *space == w.space)
                || window_space_cached(*id, space_of) != Some(w.space.1)
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
    fn verify_placements(
        &mut self,
        active: &[(Workspace, NSRect)],
        candidates: &[(i32, u32, NSRect)],
        settling: bool,
        space_of: &mut HashMap<u32, Option<u64>>,
    ) {
        if self.verifications.is_empty() {
            return;
        }
        let visible: HashSet<_> = candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect();
        let now = Instant::now();
        for id in self.verifications.keys().copied().collect::<Vec<_>>() {
            let Some(w) = self.windows.get(&id) else {
                self.verifications.remove(&id);
                continue;
            };
            if self.floating.contains(&id)
                || !active.iter().any(|(key, _)| *key == w.space)
                || window_space_cached(id, space_of) != Some(w.space.1)
                || !visible.contains(&(w.pid, id))
            {
                self.verifications.remove(&id);
                continue;
            }
            let Some((position, size)) = ax::geometry(w.element.0) else {
                continue;
            };
            let actual = NSRect::new(position, size);
            // During the bounded settle window the compositor can still report
            // the pre-slide frame. The destination already committed owns the
            // window, so neither the duplicate write nor its learned limit is
            // evidence yet; the verification resumes on the first scan after
            // the window expires, where a real drift still repairs itself.
            if committed_during_settling(
                settling,
                self.applied.get(&id).copied(),
                self.verifications[&id].target,
            ) {
                continue;
            }
            let verification = self.verifications.get_mut(&id).unwrap();
            match verification.step(actual, now) {
                VerificationStep::Wait => {}
                VerificationStep::Complete => {
                    self.verifications.remove(&id);
                    self.constrained.remove(&id);
                    // The settled frame is the proof of the smallest workable
                    // size. A larger minimum learned by an earlier Limited step
                    // must not keep nudging the tree on later round trips.
                    if let Some(minimum) = self.minimum_sizes.get_mut(&id) {
                        settle_minimum(minimum, actual.size);
                    }
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
    ) -> Option<Vec<NSRect>> {
        // Move and resize overlays have separate owners. In particular, the
        // disabled resize preview must never hide an active move destination.
        if !self.drag.as_ref().is_some_and(|drag| drag.resized) {
            return None;
        }
        if !final_sample && !preview {
            // No overlay means no per-frame layout/AX work during a native
            // resize. The gesture is still observed by the shared input path.
            return None;
        }
        let drag = self.drag.as_mut()?;
        let frame = ws::bounds(drag.id)?;
        if self.floating.contains(&drag.id) || spaces::window_space(drag.id) != Some(drag.space.1) {
            return None;
        }
        let layout = drag.resize_proposal(frame, point, pointer_workspace)?;
        if !final_sample {
            return Some(
                layout
                    .frames(drag.area, 10.)
                    .into_iter()
                    .map(|(_, r)| r)
                    .collect(),
            );
        }
        let space = drag.space;
        for id in &layout.order {
            self.cancel_window_placement(*id);
        }
        // Use the normal bounded placement/verification path after release;
        // never mark an unverified AX request as an applied native frame.
        self.desktops.insert(space, layout);
        self.invalidate();
        None
    }
    pub fn current_workspace(&self) -> Option<Workspace> {
        if self.space_quiet_until.is_some()
            || self.space_reentry.is_some()
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
        if self.space_quiet_until.is_some()
            || self.space_reentry.is_some()
            || self.presentation_blocks_input()
        {
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
    pub fn custom_context(
        &mut self,
        defaults: Obj,
        display: Option<u32>,
    ) -> Result<PickerContext, &'static str> {
        if !ax::trusted() || pref(defaults, "tile.enabled") == 0 {
            return Err("Activa Mosaico y Accesibilidad para guardar o aplicar plantillas.");
        }
        if self.presentation_blocks_input()
            || self.waiting_for_space(Instant::now())
            || self.space_reentry.is_some()
        {
            return Err("Espera a que termine el cambio de escritorio.");
        }
        self.refresh_for_picker(defaults);
        if self.space_reentry.is_some() || self.waiting_for_space(Instant::now()) {
            return Err("Espera a que termine el cambio de escritorio.");
        }
        let workspace = match display {
            Some(display) => spaces::visible_desktops()
                .into_iter()
                .find(|(id, _)| *id == display),
            None => self.current_workspace(),
        }
        .ok_or("La pantalla no está disponible.")?;
        if pref(defaults, &format!("tile.off.{}", workspace.1)) != 0 {
            return Err("Activa el mosaico en el escritorio de esta pantalla.");
        }
        let area = area(workspace.0).ok_or("La pantalla no está disponible.")?;
        let desktop = self
            .desktops
            .get(&workspace)
            .filter(|d| !d.order.is_empty())
            .cloned()
            .ok_or("Abre ventanas en mosaico en esta pantalla primero.")?;
        Ok(PickerContext {
            workspace,
            area,
            desktop,
        })
    }
    pub fn apply_custom_distribution(
        &mut self,
        defaults: Obj,
        context: &PickerContext,
        distribution: &str,
    ) -> bool {
        if self.presentation_blocks_input()
            || self.space_reentry.is_some()
            || self.waiting_for_space(Instant::now())
            || !spaces::visible_desktops().contains(&context.workspace)
            || area(context.workspace.0) != Some(context.area)
            || pref(defaults, "tile.enabled") == 0
            || pref(defaults, &format!("tile.off.{}", context.workspace.1)) != 0
            || !self
                .desktops
                .get(&context.workspace)
                .is_some_and(|d| d.order == context.desktop.order)
        {
            return false;
        }
        self.restore_distribution_with_grid(
            defaults,
            context.workspace,
            distribution,
            context.area,
            true,
        )
    }
    pub fn restore_distribution(&mut self, defaults: Obj) -> bool {
        let Some(space) = self.current_workspace() else {
            return false;
        };
        let Some((_, area)) = self.active_areas.iter().find(|(key, _)| *key == space) else {
            return false;
        };
        let area = *area;
        let text = unsafe {
            let mut text: *mut NSString = msg_send![defaults,stringForKey:&*NSString::from_str(&format!("tile.saved.{}.{}",space.1,space.0))];
            if text.is_null() {
                text = msg_send![defaults,stringForKey:&*NSString::from_str(&format!("tile.saved.{}",space.1))];
            }
            text.as_ref().map(|s| s.to_string())
        };
        let Some(text) = text else {
            return false;
        };
        self.restore_distribution_at(defaults, space, &text, area)
    }
    fn restore_distribution_at(
        &mut self,
        defaults: Obj,
        space: Workspace,
        text: &str,
        area: NSRect,
    ) -> bool {
        self.restore_distribution_with_grid(defaults, space, text, area, false)
    }
    fn restore_distribution_with_grid(
        &mut self,
        defaults: Obj,
        space: Workspace,
        text: &str,
        area: NSRect,
        preserve_grid: bool,
    ) -> bool {
        if ![
            area.origin.x,
            area.origin.y,
            area.size.width,
            area.size.height,
        ]
        .iter()
        .all(|n| n.is_finite())
            || area.size.width <= 0.
            || area.size.height <= 0.
        {
            return false;
        }
        let Some(current) = self.desktops.get(&space) else {
            return false;
        };
        let mut desktop = current.clone();
        if !desktop.restore_distribution(text) {
            return false;
        }
        // Named templates describe divisions, not a change to the global grid
        // setting used by other screens. The legacy quick copy keeps its grid.
        if preserve_grid {
            desktop.grid = current.grid;
        }
        let frames = desktop.frames(area, 10.);
        if frames.is_empty()
            || frames.len() != desktop.order.len()
            || frames.iter().any(|(_, r)| {
                ![r.origin.x, r.origin.y, r.size.width, r.size.height]
                    .iter()
                    .all(|n| n.is_finite())
                    || r.size.width < 80.
                    || r.size.height < 60.
                    || r.origin.x < area.origin.x - 0.001
                    || r.origin.y < area.origin.y - 0.001
                    || r.origin.x + r.size.width > area.origin.x + area.size.width + 0.001
                    || r.origin.y + r.size.height > area.origin.y + area.size.height + 0.001
            })
        {
            return false;
        }
        let ids = desktop.order.clone();
        if !preserve_grid {
            save(
                defaults,
                "tile.grid.enabled",
                isize::from(desktop.grid.is_some()),
            );
            if let Some((columns, rows)) = desktop.grid {
                save(defaults, "tile.grid.columns", columns as isize);
                save(defaults, "tile.grid.rows", rows as isize);
            }
        }
        save(defaults, &layout_key(space), desktop.layout.index());
        self.desktops.insert(space, desktop);
        for id in ids {
            self.cancel_window_placement(id);
        }
        self.invalidate();
        true
    }
    pub fn tick_interval(&self) -> f64 {
        // 0.08 while a bounded operation is in flight; 0.25 when the model is
        // idle. The internal 500 ms scan gate is unchanged, so the coarser idle
        // cadence cannot postpone a scan, and every notification, gesture or
        // dirty flag still resumes a fast timer on the very tick it is set.
        if self.space_reentry.is_some() && self.drag.is_none()
            || self.space_quiet_until.is_some()
            || self.dirty
            || !self.verifications.is_empty()
        {
            0.08
        } else if !self.transitions.is_empty()
            || self.drag.is_some()
            || !self.gesture_candidates.is_empty()
            || self.pending_drop.is_some()
        {
            1. / 60.
        } else {
            0.25
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
        self.prepare_read_only_refresh();
        self.invalidate();
        if ax::trusted() {
            self.tick_mode(defaults, false);
        }
    }
    fn prepare_read_only_refresh(&mut self) {
        // Reading/saving a layout must not restart another monitor's pending
        // placements. Only cancel windows participating in a pointer gesture.
        let mut affected: Vec<_> = self
            .drag
            .iter()
            .map(|d| d.id)
            .chain(self.gesture_candidates.iter().map(|d| d.id))
            .collect();
        affected.extend(self.pending_drop.iter().map(|d| d.id));
        for id in affected {
            self.cancel_window_placement(id);
        }
        self.drag = None;
        self.gesture_candidates.clear();
        self.pending_drop = None;
        self.hide_preview();
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
            self.drag = ws::window_at(p)
                .or_else(|| {
                    // Native resize handles extend a few points outside AX bounds.
                    self.windows
                        .iter()
                        .filter_map(|(id, w)| {
                            if !active.iter().any(|(key, _)| *key == w.space)
                                || !self.can_tile_drag(*id, w.space)
                            {
                                return None;
                            }
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
                            previews: DropPreviewCache::default(),
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
                    let area = active.iter().find(|(key, _)| *key == w.space)?.1;
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
                        previews: DropPreviewCache::default(),
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
                            let destination = self.desktops.get(key)?;
                            let context = DropContext {
                                workspace: *key,
                                id: drag.id,
                                area: *area,
                                desktop: destination,
                                minimums: &self.minimum_sizes,
                            };
                            let frames = drag.previews.slots(context);
                            let intent = drop_intent(
                                *area,
                                frames,
                                drag.id,
                                p,
                                pref(defaults, "tile.simple") == 0,
                            )
                            .or_else(|| {
                                (frames.is_empty() && contains(*area, p))
                                    .then_some(DropIntent::Screen(Zone::Left))
                            })?;
                            drag.previews.preview(context, intent)?;
                            Some((*key, intent))
                        });
                    drag.update_drop(candidate, Instant::now(), !down);
                    if let Some((key, intent)) = drag.drop {
                        if let Some((_, area)) = active.iter().find(|(k, _)| *k == key) {
                            if let Some(d) = self.desktops.get(&key) {
                                preview = drag.previews.preview(
                                    DropContext {
                                        workspace: key,
                                        id: drag.id,
                                        area: *area,
                                        desktop: d,
                                        minimums: &self.minimum_sizes,
                                    },
                                    intent,
                                );
                            }
                        }
                    }
                } else {
                    drag.drop = None;
                }
            }
            if down {
                let resize_preview = self.resize_drag(
                    false,
                    p,
                    pointer_workspace,
                    pref(defaults, "tile.resize.preview") != 0,
                );
                // One owner updates overlay visibility once per tick. Resize
                // preference changes cannot hide a move destination afterwards.
                if let Some(frames) = resize_preview {
                    self.show_previews(&frames);
                } else if let Some(r) = preview {
                    self.show_preview(r);
                } else {
                    self.hide_preview();
                }
                return; // Only previews change while macOS owns the native gesture.
            }
        }
        let _ = self.resize_drag(true, p, pointer_workspace, false);
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
        if self.space_reentry.is_some() {
            let sample = self.reentry_sample(defaults, &active);
            if !self.observe_reentry(sample, Instant::now()) {
                return;
            }
        }
        // One coherent compositor snapshot serves verification, animation and
        // membership for this tick: the same data three times over, instead of
        // three lists that can disagree during a native slide.
        let Some(candidates) = ws::try_tile_windows() else {
            return;
        };
        // One Space-membership memo per tick, so every reader of this frame
        // consults the same native answer instead of three mid-slide reads.
        let settling = self.settling_space();
        let mut space_of: HashMap<u32, Option<u64>> = HashMap::new();
        // Short, bounded reconciliation burst while macOS publishes the arriving windows.
        if place_windows {
            self.verify_placements(&active, &candidates, settling, &mut space_of);
            self.animate(&active, &candidates, &mut space_of);
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
        self.last_visible_windows =
            Some(candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect());
        self.last_scan = Some(Instant::now());
        let focused = std::cell::LazyCell::new(|| unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            ax::focused_id(pid)
        });
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
        let mut native_confirmed = HashSet::new();
        for (pid, id, compositor_frame) in candidates {
            if self.windows.get(&id).is_some_and(|w| w.pid != pid) {
                self.forget_window(id);
                transferred.remove(&id);
            }
            let Some(space) = window_space_cached(id, &mut space_of) else {
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
            if let Some(window) = self
                .windows
                .get(&id)
                .filter(|w| w.pid == pid && w.space.1 == space)
                && active.iter().any(|(key, area)| {
                    *key == window.space
                        && overlap(compositor_frame, *area)
                            > compositor_frame.size.width * compositor_frame.size.height * 0.5
                })
            {
                // Positive native identity, visible membership and physical
                // ownership outlive an AX timeout in a busy app. Preserve the
                // edited slot, but the uncertain guard still forbids writes.
                native_confirmed.insert(id);
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
                && window_space_cached(*id, &mut space_of).is_none_or(|native| native == w.space.1)
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
                    && ws::bounds(**id).is_none()
                    && ws::ordered_in(**id) != Some(true)
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
        for id in self
            .membership_grace
            .retained(&uncertain, &native_confirmed, Instant::now())
        {
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
        if settling {
            // During the native transition, onscreen visibility can lag Space
            // membership. Do not erase a live desktop's ratios in that gap.
            for (id, w) in &self.windows {
                if !visible_ids.contains(id)
                    && !self.inactive_tabs.contains(id)
                    && !self.floating.contains(id)
                    && active.iter().any(|(space, _)| *space == w.space)
                    && window_space_cached(*id, &mut space_of) == Some(w.space.1)
                {
                    members.entry(w.space).or_default().push(*id);
                }
            }
        }
        crate::desktop_mode::refresh();
        if crate::desktop_mode::active() {
            return;
        }
        // The settle flag was read once for this tick, above.
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
            // While macOS publishes the arriving windows, one id can still be
            // absent from the compositor list on the first scan. Reuse the
            // retained order so its leaf is neither collapsed nor re-expanded:
            // the same distribution is produced and no second write is needed.
            retain_settled_ids(ids, &d.order, settling);
            let insertion_focus = if d.needs_insertion_focus(ids) {
                *focused
            } else {
                None
            };
            d.reconcile_at(ids, r, 10., insertion_focus);
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
                if !self.needs_placement(id, frame, actual, settling) {
                    continue;
                }
                self.constrained.remove(&id);
                if transferred.contains(&id) || settling {
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

pub(crate) fn restoration_frame(original: NSRect, source: NSRect, destination: NSRect) -> NSRect {
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

/// Clamp a learned app minimum to the frame the app just proved it can hold,
/// never below the smallest usable tile. Idempotent, so repeated round trips
/// converge instead of growing monotonically.
fn settle_minimum(minimum: &mut objc2_foundation::NSSize, actual: objc2_foundation::NSSize) {
    minimum.width = minimum.width.min(actual.width).max(80.);
    minimum.height = minimum.height.min(actual.height).max(60.);
}

/// One native Space read per window per tick. The animation, the verification
/// and the membership scan must agree on the same mid-slide answer.
fn window_space_cached(id: u32, memo: &mut HashMap<u32, Option<u64>>) -> Option<u64> {
    if let Some(known) = memo.get(&id) {
        return *known;
    }
    let read = spaces::window_space(id);
    memo.insert(id, read);
    read
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

/// Keep the previous distribution while a Space is still publishing windows.
/// During the settle burst the compositor list can lag the retained order in
/// two ways: one id not yet published (a gap anywhere in the list) or one
/// transient extra id (a native tab's second `NSWindow`, an Electron helper).
/// Both are the same membership as the retained order, so that order wins and
/// each leaf is written once. An unrelated list is a real change and is kept.
fn retain_settled_ids(ids: &mut Vec<u32>, order: &[u32], settling: bool) {
    // An empty retained order is the first visit, not a lagging list.
    if !settling || ids.is_empty() || order.is_empty() || ids.len() == order.len() {
        return;
    }
    // Different lengths with either inclusion hold only for the two lag
    // shapes above, so those reuse the retained order.
    if ids.iter().all(|id| order.contains(id)) || order.iter().all(|id| ids.contains(id)) {
        *ids = order.to_vec();
    }
}

/// While a Space is still publishing its windows, the destination already in
/// `applied` is the authoritative one, so the verification must not rewrite it.
/// Outside the settle window, or for a genuinely different destination, the
/// observed frame still drives the repair.
fn committed_during_settling(settling: bool, applied: Option<NSRect>, target: NSRect) -> bool {
    settling && applied.is_some_and(|applied| near(applied, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_a_template_does_not_cancel_unrelated_screen_placements() {
        let mut manager = Manager::default();
        let drag = test_drag();
        let affected = drag.id;
        manager.drag = Some(drag);
        manager.pressed = true;
        for id in [affected, 99] {
            let target = rect(10., 41., 1000., 900.);
            manager.transition_from(id, rect(20., 50., 800., 600.), target);
            manager.verifications.insert(id, Verification::new(target));
            manager.applied.insert(id, target);
        }
        manager.prepare_read_only_refresh();
        assert!(manager.drag.is_none());
        assert!(manager.pressed);
        assert!(!manager.transitions.contains_key(&affected));
        assert!(!manager.verifications.contains_key(&affected));
        assert!(manager.transitions.contains_key(&99));
        assert!(manager.verifications.contains_key(&99));
        assert!(manager.applied.contains_key(&99));
    }
    #[test]
    fn busy_apps_cannot_lose_a_custom_master_slot_while_native_membership_is_confirmed() {
        let area = rect(10., 41., 2540., 1327.);
        let mut desktop = Desktop::configured(Layout::Master, None);
        desktop.reconcile(&[1, 2], area, 10.);
        let from = desktop.frames(area, 10.)[0].1;
        let mut to = from;
        to.size.width += 125.;
        desktop.resize(1, from, to, area, 10.);
        let saved = desktop.save_distribution(area);
        let uncertain = HashSet::from([2]);
        let native_confirmed = HashSet::from([2]);
        assert!(native_confirmed.contains(&2));
        let now = Instant::now();
        let mut grace = MembershipGrace::default();
        for millis in [0, 500, 1000, 2000, 5000] {
            let kept = grace.retained(
                &uncertain,
                &native_confirmed,
                now + Duration::from_millis(millis),
            );
            let mut members = vec![1];
            members.extend(kept);
            desktop.reconcile_at(&members, area, 10., Some(1));
            assert_eq!(
                desktop.save_distribution(area),
                saved,
                "A busy AX client must not be treated as a closed native window"
            );
        }
    }
    #[test]
    fn edited_layouts_and_constraints_survive_repeated_independent_space_round_trips() {
        let element = ax::application(std::process::id() as i32).unwrap();
        for layout in Layout::ALL {
            for grid in [None, Some((12, 8))] {
                for area in [
                    rect(2570., -187., 1060., 1870.),
                    rect(-1420., -500., 1400., 2400.),
                    rect(10., 41., 1660., 947.),
                ] {
                    for count in 2..=7 {
                        let key = (5, 1515);
                        let mut desktop = Desktop::configured(layout, grid);
                        let ids: Vec<_> = (1..=count).collect();
                        desktop.reconcile(&ids, area, 10.);
                        let (id, from) = desktop.frames(area, 10.)[0];
                        let mut to = from;
                        to.size.width *= 0.9;
                        to.size.height *= 0.92;
                        desktop.resize(id, from, to, area, 10.);
                        desktop.place(ids[0], ids[1], Zone::Center);
                        let saved = desktop.save_distribution(area);
                        let frames = desktop.frames(area, 10.);
                        let mut manager = Manager {
                            active_areas: vec![((2, 5), rect(10., 41., 2540., 1327.)), (key, area)],
                            ..Default::default()
                        };
                        manager.desktops.insert(key, desktop);
                        for (id, frame) in &frames {
                            manager.windows.insert(
                                *id,
                                Managed {
                                    pid: 42,
                                    space: key,
                                    original: *frame,
                                    original_area: area,
                                    element: element.clone(),
                                },
                            );
                            manager.applied.insert(*id, *frame);
                            manager.constrained.insert(*id, (*frame, *frame));
                        }
                        let now = Instant::now();
                        for (index, space) in [1, 5, 6, 5, 1].into_iter().enumerate() {
                            let start = now + Duration::from_secs(index as u64 * 2);
                            manager.space_changed_with_visible_at(start, &[(2, space), key]);
                            let sample = ReentrySample {
                                areas: vec![
                                    ((2, space), rect(10., 41., 2540., 1327.)),
                                    (key, area),
                                ],
                                windows: frames
                                    .iter()
                                    .map(|(id, r)| (42, *id, key.1, *r, Some(*r)))
                                    .collect(),
                                absent: vec![],
                            };
                            manager.active_areas = sample.areas.clone();
                            assert!(!manager.observe_reentry(
                                Some(sample.clone()),
                                start + Duration::from_millis(600)
                            ));
                            assert!(
                                manager.observe_reentry(
                                    Some(sample),
                                    start + Duration::from_millis(800)
                                )
                            );
                            assert_eq!(manager.desktops[&key].save_distribution(area), saved);
                            assert_eq!(manager.desktops[&key].frames(area, 10.), frames);
                            for (id, frame) in &frames {
                                assert_eq!(manager.applied.get(id), Some(frame));
                                assert_eq!(manager.constrained.get(id), Some(&(*frame, *frame)));
                                assert!(!manager.needs_placement(
                                    *id,
                                    *frame,
                                    Some(*frame),
                                    manager.settling_space()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn named_distribution_apply_is_atomic_and_cannot_change_another_screens_grid() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite =
                NSString::from_str(&format!("amspace.named-apply-test.{}", std::process::id()));
            let defaults: objc2::rc::Allocated<objc2::runtime::AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<objc2::runtime::AnyObject> =
                msg_send![defaults,initWithSuiteName:&*suite];
            let obj = Retained::as_ptr(&defaults) as Obj;
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            save(obj, "tile.grid.enabled", 0);
            save(obj, "tile.grid.columns", 19);
            let area = rect(10., 41., 1600., 1000.);
            let key = (1, 7);
            let other = (2, 9);
            for layout in Layout::ALL {
                let mut source = Desktop::configured(layout, Some((12, 8)));
                source.reconcile(&[1, 2, 3], area, 10.);
                let data = source.save_distribution(area).unwrap();
                let mut current = Desktop::configured(Layout::Grid, None);
                current.reconcile(&[101, 102, 103], area, 10.);
                let mut manager = Manager::default();
                manager.desktops.insert(key, current);
                manager.desktops.insert(other, source.clone());
                assert!(manager.restore_distribution_with_grid(obj, key, &data, area, true));
                assert!(manager.desktops[&key].grid.is_none());
                assert_eq!(manager.desktops[&key].order, vec![101, 102, 103]);
                assert!(manager.desktops[&other] == source);
                assert_eq!(pref(obj, "tile.grid.enabled"), 0);
                assert_eq!(pref(obj, "tile.grid.columns"), 19);
                let before = manager.desktops[&key].clone();
                source.reconcile(&[1, 2, 3, 4], area, 10.);
                assert!(!manager.restore_distribution_with_grid(
                    obj,
                    key,
                    &source.save_distribution(area).unwrap(),
                    area,
                    true
                ));
                assert!(manager.desktops[&key] == before);
            }
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
    #[test]
    fn edited_portrait_rows_cannot_finalize_a_stale_resize_after_another_screen_switches() {
        let key = (5, 1515);
        let area = rect(2570., -187., 1060., 1870.);
        let mut desktop = Desktop::configured(Layout::ThreeBands, None);
        desktop.reconcile(&[10, 20, 30], area, 10.);
        let before = desktop.frames(area, 10.);
        let mut resized = before[2].1;
        resized.origin.y -= 65.;
        resized.size.height += 65.;
        desktop.resize(30, before[2].1, resized, area, 10.);
        let expected = desktop.save_distribution(area);
        let frame = desktop.frames(area, 10.)[2].1;
        let drag = Drag {
            id: 30,
            space: key,
            start: frame,
            point: NSPoint::new(frame.origin.x + 50., frame.origin.y),
            drop: None,
            candidate: None,
            candidate_since: Instant::now(),
            moved: false,
            resized: true,
            resize_gesture: true,
            baseline: desktop.clone(),
            area,
            previews: Default::default(),
        };
        let mut manager = Manager {
            pressed: true,
            drag: Some(drag.clone()),
            gesture_candidates: vec![drag],
            ..Default::default()
        };
        manager.desktops.insert(key, desktop);
        manager.active_areas = vec![((2, 5), rect(10., 41., 2540., 1327.)), (key, area)];
        for space in [1, 5, 6, 5, 1] {
            manager.space_changed_with_visible_at(Instant::now(), &[(2, space), key]);
            assert!(
                manager.drag.is_none(),
                "A Space transition must invalidate an old resize baseline even on the unchanged monitor"
            );
            assert!(manager.gesture_candidates.is_empty());
            assert!(
                manager.pressed,
                "Do not recapture a held gesture until physical release"
            );
            assert_eq!(manager.desktops[&key].save_distribution(area), expected);
        }
    }
    #[test]
    fn space_reentry_does_not_restart_the_bottom_rows_constraint_retry_on_an_unchanged_monitor() {
        let key = (5, 1515);
        let area = rect(2570., -187., 1060., 1870.);
        let primary = ((2, 5), rect(10., 41., 2540., 1327.));
        let mut desktop = Desktop::configured(Layout::ThreeBands, None);
        desktop.reconcile(&[10, 20, 30], area, 10.);
        let target = desktop.frames(area, 10.)[2].1;
        let mut resized = target;
        resized.origin.y -= 40.;
        resized.size.height += 40.;
        desktop.resize(30, target, resized, area, 10.);
        let saved = desktop.save_distribution(area);
        let now = Instant::now();
        let mut manager = Manager {
            active_areas: vec![primary, (key, area)],
            ..Default::default()
        };
        manager.desktops.insert(key, desktop.clone());
        manager.windows.insert(
            30,
            Managed {
                pid: 42,
                space: key,
                original: target,
                original_area: area,
                element: ax::application(std::process::id() as i32).unwrap(),
            },
        );
        let mut verification = Verification::new(target);
        verification.retried = true;
        verification.accepted = true;
        manager.verifications.insert(30, verification);
        manager.applied.insert(30, target);
        manager.space_changed_with_visible_at(now, &[(2, 6), key]);
        let sample = ReentrySample {
            areas: vec![((2, 6), primary.1), (key, area)],
            windows: desktop
                .frames(area, 10.)
                .into_iter()
                .map(|(id, r)| (42, id, key.1, r, Some(r)))
                .collect(),
            absent: vec![],
        };
        assert!(!manager.observe_reentry(Some(sample.clone()), now + Duration::from_millis(600)));
        assert!(manager.observe_reentry(Some(sample), now + Duration::from_millis(800)));
        assert!(
            manager.verifications.contains_key(&30),
            "An unchanged monitor must finish its bounded verification rather than restart placement on each unrelated Space switch"
        );
        assert!(manager.verifications[&30].retried);
        assert_eq!(manager.applied.get(&30), Some(&target));
        assert_eq!(manager.desktops[&key].save_distribution(area), saved);
    }
    fn assert_keyboard_frames(desktop: &Desktop, area: NSRect) {
        let frames = desktop.frames(area, 10.);
        assert_eq!(frames.len(), desktop.order.len());
        for (index, (_, frame)) in frames.iter().enumerate() {
            assert!(frame.size.width >= 120. && frame.size.height >= 90.);
            assert!(frame.origin.x >= area.origin.x - 1. && frame.origin.y >= area.origin.y - 1.);
            assert!(frame.origin.x + frame.size.width <= area.origin.x + area.size.width + 1.);
            assert!(frame.origin.y + frame.size.height <= area.origin.y + area.size.height + 1.);
            for (_, other) in &frames[index + 1..] {
                assert!(overlap(*frame, *other) < 0.01);
            }
        }
    }
    #[test]
    fn keyboard_round_trips_cross_all_template_pairs_spaces_and_orientations() {
        let source_area = rect(10., 41., 1660., 947.);
        let destinations = [
            ((1, 5), source_area),
            ((2, 9), rect(-1420., -600., 1400., 2400.)),
            ((2, 1), rect(1680., -1100., 1900., 1050.)),
            ((2, 1), rect(-1100., 1050., 1080., 1900.)),
        ];
        let element = ax::application(std::process::id() as i32).unwrap();
        for source_layout in Layout::ALL {
            for target_layout in Layout::ALL {
                for grid in [None, Some((120, 6))] {
                    for (destination, destination_area) in destinations {
                        let source = (1, 1);
                        let untouched = (3, 9);
                        let mut manager = Manager::default();
                        let mut from = Desktop::configured(source_layout, grid);
                        from.reconcile(&[1, 2, 3], source_area, 10.);
                        let original = from.frames(source_area, 10.)[0].1;
                        let mut to = Desktop::configured(target_layout, grid);
                        to.reconcile(&[10, 11, 12], destination_area, 10.);
                        let (resized_id, frame) = to.frames(destination_area, 10.)[0];
                        let mut resized = frame;
                        resized.size.width *= 0.9;
                        to.resize(resized_id, frame, resized, destination_area, 10.);
                        let saved = to.save_distribution(destination_area);
                        manager.desktops.insert(source, from);
                        manager.desktops.insert(destination, to);
                        manager
                            .desktops
                            .insert(untouched, Desktop::configured(Layout::ThreeBands, None));
                        let unaffected = manager.desktops[&untouched].clone();
                        let proposed = arrival_proposal(
                            &manager.desktops[&destination],
                            1,
                            None,
                            destination_area,
                            None,
                        )
                        .unwrap();
                        assert_eq!(
                            manager.desktops[&destination].save_distribution(destination_area),
                            saved
                        );
                        assert_keyboard_frames(&proposed, destination_area);
                        let target = proposed
                            .frames(destination_area, 10.)
                            .into_iter()
                            .find(|(id, _)| *id == 1)
                            .unwrap()
                            .1;
                        let plan = KeyboardTransfer {
                            source,
                            destination,
                            target,
                            source_area,
                            destination_area,
                            desktop: Some(proposed),
                            tracked: true,
                            floating: false,
                        };
                        assert_eq!(plan.background_frames().len(), 4);
                        manager.commit_keyboard_transfer_model(
                            plan,
                            42,
                            1,
                            element.clone(),
                            original,
                        );
                        assert_eq!(manager.windows[&1].space, destination);
                        assert_eq!(manager.desktops[&destination].layout, target_layout);
                        assert!(manager.desktops[&untouched] == unaffected);
                        assert_eq!(
                            manager
                                .desktops
                                .values()
                                .filter(|d| d.order.contains(&1))
                                .count(),
                            1
                        );
                        assert_keyboard_frames(&manager.desktops[&source], source_area);
                        let proposed = arrival_proposal(
                            &manager.desktops[&source],
                            1,
                            None,
                            source_area,
                            None,
                        )
                        .unwrap();
                        let plan = KeyboardTransfer {
                            source: destination,
                            destination: source,
                            target: original,
                            source_area: destination_area,
                            destination_area: source_area,
                            desktop: Some(proposed),
                            tracked: true,
                            floating: false,
                        };
                        manager.commit_keyboard_transfer_model(
                            plan,
                            42,
                            1,
                            element.clone(),
                            target,
                        );
                        assert_eq!(manager.windows[&1].space, source);
                        assert_eq!(manager.desktops[&source].layout, source_layout);
                        assert_eq!(
                            manager
                                .desktops
                                .values()
                                .filter(|d| d.order.contains(&1))
                                .count(),
                            1
                        );
                        assert!(manager.desktops[&untouched] == unaffected);
                        assert_keyboard_frames(&manager.desktops[&source], source_area);
                        assert_keyboard_frames(&manager.desktops[&destination], destination_area);
                    }
                }
            }
        }
    }
    #[test]
    fn keyboard_floating_and_disabled_tiling_do_not_create_background_tiles() {
        let element = ax::application(std::process::id() as i32).unwrap();
        for tracked in [false, true] {
            for floating in [false, true] {
                let source = (1, 1);
                let destination = (2, 1); // same native Space on different monitors
                let source_area = rect(10., 41., 1660., 947.);
                let destination_area = rect(-900., -400., 880., 1580.);
                let original = rect(500., 100., 1000., 800.);
                let mut manager = Manager::default();
                let mut desktop = Desktop::configured(Layout::Spiral, None);
                desktop.reconcile(&[2, 3], destination_area, 10.);
                manager.desktops.insert(destination, desktop.clone());
                let target = restoration_frame(original, source_area, destination_area);
                let plan = KeyboardTransfer {
                    source,
                    destination,
                    target,
                    source_area,
                    destination_area,
                    desktop: None,
                    tracked,
                    floating,
                };
                assert!(plan.background_frames().is_empty());
                manager.commit_keyboard_transfer_model(plan, 42, 1, element.clone(), original);
                assert!(manager.desktops[&destination] == desktop);
                assert_eq!(manager.floating.contains(&1), tracked && floating);
                assert_eq!(manager.windows.contains_key(&1), tracked);
                if tracked {
                    assert_eq!(manager.windows[&1].space, destination);
                    assert_eq!(manager.windows[&1].original, target);
                }
            }
        }
    }
    fn reentry_observation(
        key: Workspace,
        area: NSRect,
        frames: &[(u32, NSRect)],
    ) -> ReentrySample {
        ReentrySample {
            areas: vec![(key, area)],
            windows: frames
                .iter()
                .map(|(id, r)| (42, *id, key.1, *r, Some(*r)))
                .collect(),
            absent: Vec::new(),
        }
    }
    #[test]
    fn keyboard_arrival_preflight_preserves_custom_destination_until_commit() {
        for layout in Layout::ALL {
            for area in [
                rect(-1500., 40., 1400., 900.),
                rect(100., -1100., 900., 1400.),
            ] {
                for grid in [None, Some((12, 8))] {
                    let mut destination = Desktop::configured(layout, grid);
                    destination.reconcile_at(&[10, 11, 12, 13], area, 10., None);
                    let initial = destination.frames(area, 10.);
                    if let Some((id, frame)) = initial.first() {
                        let mut adjusted = *frame;
                        adjusted.size.width *= 0.9;
                        destination.resize(*id, *frame, adjusted, area, 10.);
                    }
                    let before = destination.save_distribution(area);
                    let before_frames = destination.frames(area, 10.);
                    let result = arrival_proposal(&destination, 99, None, area, None);
                    assert_eq!(destination.save_distribution(area), before);
                    assert_eq!(destination.frames(area, 10.), before_frames);
                    if let Some(proposed) = result {
                        assert_eq!(proposed.order.len(), 5);
                        assert!(proposed.order.contains(&99));
                        for id in [10, 11, 12, 13] {
                            assert!(proposed.order.contains(&id));
                        }
                        let frames = proposed.frames(area, 10.);
                        assert_eq!(frames.len(), 5);
                        for (i, (_, a)) in frames.iter().enumerate() {
                            assert!(a.origin.x >= area.origin.x && a.origin.y >= area.origin.y);
                            assert!(
                                a.origin.x + a.size.width <= area.origin.x + area.size.width + 1.
                            );
                            assert!(
                                a.origin.y + a.size.height <= area.origin.y + area.size.height + 1.
                            );
                            for (_, b) in &frames[i + 1..] {
                                assert!(overlap(*a, *b) < 0.01);
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn impossible_keyboard_arrival_leaves_destination_unchanged() {
        let area = rect(0., 0., 1200., 800.);
        for layout in Layout::ALL {
            let mut destination = Desktop::configured(layout, None);
            destination.reconcile_at(&[1, 2, 3], area, 10., None);
            let before = destination.save_distribution(area);
            let minimums = HashMap::from([(99, objc2_foundation::NSSize::new(5000., 5000.))]);
            assert!(arrival_proposal(&destination, 99, None, area, Some(&minimums)).is_none());
            assert_eq!(destination.save_distribution(area), before);
        }
    }
    #[test]
    #[ignore = "read-only reentry diagnostic; requires a stable interactive desktop"]
    fn inspect_destination_reentry_sample() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let defaults: Obj = msg_send![class!(NSUserDefaults), standardUserDefaults];
            let active: Vec<_> = spaces::visible_desktops()
                .into_iter()
                .filter_map(|key| area(key.0).map(|r| (key, r)))
                .collect();
            assert!(!active.is_empty());
            let mut manager = Manager::default();
            let sample = manager
                .reentry_sample(defaults, &active)
                .expect("complete first-visit snapshot");
            println!(
                "first_visit windows={} absent={}",
                sample.windows.len(),
                sample.absent.len()
            );
            for (pid, id, native, cg, _) in sample.windows {
                let Some(element) = ax::window_for_id(pid, id) else {
                    continue;
                };
                if !ax::tileable(element.0) {
                    continue;
                }
                let Some(key) = workspace_at_frame(native, cg, &active) else {
                    continue;
                };
                let area = active.iter().find(|(k, _)| *k == key).unwrap().1;
                manager.windows.insert(
                    id,
                    Managed {
                        pid,
                        space: key,
                        original: cg,
                        original_area: area,
                        element,
                    },
                );
                manager
                    .desktops
                    .entry(key)
                    .or_insert_with(|| Desktop::configured(Layout::Spiral, None))
                    .order
                    .push(id);
            }
            let sample = manager
                .reentry_sample(defaults, &active)
                .expect("complete cached snapshot");
            assert!(
                sample
                    .windows
                    .iter()
                    .filter(|w| manager.windows.contains_key(&w.1))
                    .all(|w| w.4.is_some())
            );
            manager.space_reentry = Some(SpaceReentry::default());
            assert!(!manager.observe_reentry(Some(sample), Instant::now()));
            std::thread::sleep(Duration::from_millis(180));
            let sample = manager
                .reentry_sample(defaults, &active)
                .expect("stable cached snapshot");
            assert!(manager.observe_reentry(Some(sample), Instant::now()));
            println!(
                "cached reentry released without writing geometry; managed={}",
                manager.windows.len()
            );
        });
    }
    #[test]
    fn destination_reentry_never_reconciles_a_partial_arrival_after_the_old_deadline() {
        for layout in Layout::ALL {
            for area in [
                rect(10., 41., 2560., 1440.),
                rect(-1440., -500., 1440., 2560.),
            ] {
                for grid in [None, Some((17, 9))] {
                    let key = (2, 8);
                    let mut desktop = Desktop::configured(layout, grid);
                    desktop.reconcile(&[1, 2, 3, 4, 5], area, 10.);
                    let from = desktop.frames(area, 10.)[0].1;
                    desktop.resize(
                        1,
                        from,
                        rect(
                            from.origin.x,
                            from.origin.y,
                            from.size.width - 30.,
                            from.size.height - 20.,
                        ),
                        area,
                        10.,
                    );
                    let frames = desktop.frames(area, 10.);
                    let saved = desktop.save_distribution(area);
                    // Model the old late snapshot: dropping a surviving slot
                    // really does resize the other windows before it returns.
                    let mut old = desktop.clone();
                    old.reconcile(&[1, 2, 3, 4], area, 10.);
                    assert_ne!(old.frames(area, 10.), frames);
                    let mut manager = Manager::default();
                    manager.desktops.insert(key, desktop);
                    let now = Instant::now();
                    manager.space_changed_with_visible_at(now, &[key]);
                    assert!(!manager.waiting_for_space(now + Duration::from_secs(2)));
                    // None is an incomplete native observation, not an empty
                    // desktop. It must stay suspended beyond a fixed timeout.
                    for delay in [2000, 2100, 3000] {
                        assert!(!manager.observe_reentry(None, now + Duration::from_millis(delay)));
                        assert_eq!(manager.desktops[&key].frames(area, 10.), frames);
                        assert_eq!(manager.desktops[&key].save_distribution(area), saved);
                    }
                    let sample = reentry_observation(key, area, &frames);
                    assert!(
                        !manager.observe_reentry(
                            Some(sample.clone()),
                            now + Duration::from_millis(3100)
                        )
                    );
                    assert!(
                        !manager.observe_reentry(
                            Some(sample.clone()),
                            now + Duration::from_millis(3180)
                        )
                    );
                    assert!(
                        manager.observe_reentry(Some(sample), now + Duration::from_millis(3260))
                    );
                    assert_eq!(manager.desktops[&key].frames(area, 10.), frames);
                    for (id, r) in &frames {
                        assert!(!manager.needs_placement(
                            *id,
                            *r,
                            Some(*r),
                            manager.settling_space()
                        ));
                    }
                }
            }
        }
    }
    #[test]
    fn changing_frames_membership_and_failed_reads_restart_reentry_stability() {
        let now = Instant::now();
        let area = rect(10., 41., 1660., 947.);
        let key = (1, 7);
        let sample = reentry_observation(
            key,
            area,
            &[
                (1, rect(10., 41., 800., 947.)),
                (2, rect(820., 41., 850., 947.)),
            ],
        );
        let mut barrier = SpaceReentry::default();
        assert!(!barrier.observe(Some(sample.clone()), now));
        let mut resizing = sample.clone();
        resizing.windows[0].4.as_mut().unwrap().size.width += 80.;
        assert!(!barrier.observe(Some(resizing), now + Duration::from_millis(80)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(160)));
        assert!(!barrier.observe(None, now + Duration::from_millis(240)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(400)));
        let mut closing = sample.clone();
        closing.windows.pop();
        closing.absent.push((42, 2));
        assert!(!barrier.observe(Some(closing.clone()), now + Duration::from_millis(480)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(560)));
        assert!(barrier.observe(Some(sample), now + Duration::from_millis(720)));
    }
    #[test]
    fn a_burst_of_incomplete_reads_still_releases_the_fast_timer() {
        let now = Instant::now();
        let area = rect(10., 41., 1660., 947.);
        let sample = reentry_observation((1, 7), area, &[(1, area)]);
        let mut barrier = SpaceReentry::default();
        // An incomplete read restarts the agreement, but the burst is bounded.
        for _ in 0..7 {
            assert!(!barrier.observe(None, now));
        }
        assert!(barrier.observe(None, now));
        // A real observation restarts the counter and needs its own stability.
        assert!(!barrier.observe(Some(sample.clone()), now));
        assert!(!barrier.observe(None, now));
        assert!(!barrier.observe(Some(sample.clone()), now));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(159)));
        assert!(barrier.observe(Some(sample), now + Duration::from_millis(160)));
    }
    #[test]
    fn a_settled_frame_clamps_a_grown_minimum_without_losing_the_floor() {
        // A Limited step once observed a wider frame than the tile can hold.
        let mut minimum = objc2_foundation::NSSize::new(1200., 800.);
        settle_minimum(&mut minimum, objc2_foundation::NSSize::new(825., 947.));
        assert_eq!((minimum.width, minimum.height), (825., 800.));
        // Idempotent: the same settled frame cannot drift the tree again.
        settle_minimum(&mut minimum, objc2_foundation::NSSize::new(825., 947.));
        assert_eq!((minimum.width, minimum.height), (825., 800.));
        // Never below the smallest usable tile.
        settle_minimum(&mut minimum, objc2_foundation::NSSize::new(10., 10.));
        assert_eq!((minimum.width, minimum.height), (80., 60.));
    }
    #[test]
    fn reentry_requires_stability_after_cumulative_motion_and_display_changes() {
        let now = Instant::now();
        let mut sample = reentry_observation(
            (2, 8),
            rect(-1440., -500., 1440., 2560.),
            &[(1, rect(-1440., -500., 1440., 2560.))],
        );
        let mut barrier = SpaceReentry::default();
        assert!(!barrier.observe(Some(sample.clone()), now));
        // Consecutive steps are below tolerance, but their total is not.
        sample.windows[0].3.origin.x += 2.;
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(80)));
        sample.windows[0].3.origin.x += 2.;
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(160)));
        sample.areas[0].1.size.height -= 100.;
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(320)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(400)));
        assert!(barrier.observe(Some(sample), now + Duration::from_millis(480)));
        // A first visit to an empty desktop is a valid stable state too.
        let empty = ReentrySample {
            areas: Vec::new(),
            windows: Vec::new(),
            absent: Vec::new(),
        };
        assert!(!barrier.observe(Some(empty.clone()), now + Duration::from_millis(560)));
        assert!(barrier.observe(Some(empty), now + Duration::from_millis(720)));
    }
    #[test]
    fn reentry_refreshes_pending_writes_without_changing_other_workspace_layouts() {
        let mut manager = Manager::default();
        let now = Instant::now();
        let areas = [
            ((1, 7), rect(10., 41., 2560., 1440.)),
            ((2, 8), rect(-1440., -500., 1440., 2560.)),
        ];
        for (key, area) in areas {
            let mut d = Desktop::configured(
                if key.0 == 1 {
                    Layout::Spiral
                } else {
                    Layout::VerticalMaster
                },
                Some((17, 9)),
            );
            d.reconcile(&[key.0 * 10 + 1, key.0 * 10 + 2, key.0 * 10 + 3], area, 10.);
            manager.desktops.insert(key, d);
        }
        manager.space_changed_with_visible_at(now, &[(1, 7), (2, 8)]);
        let mut sample = ReentrySample {
            areas: areas.to_vec(),
            windows: Vec::new(),
            absent: Vec::new(),
        };
        let original = manager.desktops.clone();
        for (key, area) in areas {
            for (id, r) in manager.desktops[&key].frames(area, 10.) {
                sample.windows.push((42, id, key.1, r, Some(r)));
            }
        }
        manager
            .verifications
            .insert(11, Verification::new(sample.windows[0].3));
        manager.applied.insert(11, sample.windows[0].3);
        assert!(!manager.observe_reentry(Some(sample.clone()), now + Duration::from_millis(600)));
        assert_eq!(manager.verifications.len(), 1);
        assert_eq!(manager.tick_interval(), 0.08);
        assert!(manager.observe_reentry(Some(sample), now + Duration::from_millis(760)));
        assert!(manager.verifications.is_empty());
        assert!(!manager.applied.contains_key(&11));
        assert!(manager.desktops == original);
        // A rapid second switch cannot reuse the previous agreement window.
        manager.space_changed_with_visible_at(now + Duration::from_millis(780), &[(1, 9), (2, 8)]);
        assert!(!manager.observe_reentry(None, now + Duration::from_secs(4)));
    }
    #[test]
    fn failed_restore_keeps_the_model_preferences_and_pending_work_unchanged() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite =
                NSString::from_str(&format!("amspace.tests.restore.qa.{}", std::process::id()));
            let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            let portrait = rect(-1000., -300., 1000., 1600.);
            let landscape = rect(20., 41., 2000., 1000.);
            let key = (1, 7);
            let mut current = Desktop::configured(Layout::Spiral, None);
            current.reconcile(&[1, 2], portrait, 10.);
            current.set_minimum_sizes(
                &[
                    (1, objc2_foundation::NSSize::new(700., 200.)),
                    (2, objc2_foundation::NSSize::new(700., 200.)),
                ]
                .into(),
            );
            assert_eq!(current.frames(portrait, 10.).len(), 2);
            let mut saved = Desktop::configured(Layout::Grid, Some((17, 9)));
            saved.reconcile(&[1, 2], landscape, 10.);
            let text = saved.save_distribution(landscape).unwrap();
            let mut manager = Manager::default();
            manager.desktops.insert(key, current.clone());
            manager
                .applied
                .insert(1, current.frames(portrait, 10.)[0].1);
            manager
                .verifications
                .insert(1, Verification::new(current.frames(portrait, 10.)[0].1));
            save(raw, "tile.grid.enabled", 0);
            save(raw, &layout_key(key), Layout::Spiral.index());
            assert!(!manager.restore_distribution_at(raw, key, &text, portrait));
            assert!(manager.desktops[&key] == current);
            assert_eq!(manager.applied.len(), 1);
            assert_eq!(manager.verifications.len(), 1);
            assert_eq!(pref(raw, "tile.grid.enabled"), 0);
            assert_eq!(layout_pref(raw, key), Layout::Spiral);
            for area in [
                rect(0., 0., 0., 1000.),
                rect(f64::NAN, 0., 1000., 1000.),
                rect(0., 0., f64::INFINITY, 1000.),
            ] {
                assert!(!manager.restore_distribution_at(raw, key, &text, area));
                assert!(manager.desktops[&key] == current);
                assert_eq!(pref(raw, "tile.grid.enabled"), 0);
            }
            let mut seed = 0x1d40_be16_528a_11cdu64;
            let alphabet = [
                '0', '1', '2', '8', '9', 'v', 'h', 'w', 'e', '-', '.', ' ', '∞', '\n',
            ];
            for _ in 0..512 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let mut chars: Vec<_> = text.chars().collect();
                for turn in 0..=seed % 3 {
                    let index = (seed.rotate_left(turn as u32 * 7) as usize) % chars.len();
                    chars[index] =
                        alphabet[(seed.rotate_right(turn as u32 * 5) as usize) % alphabet.len()];
                }
                let corrupted: String = chars.into_iter().collect();
                manager.desktops.insert(key, current.clone());
                save(raw, "tile.grid.enabled", 0);
                save(raw, &layout_key(key), Layout::Spiral.index());
                if manager.restore_distribution_at(raw, key, &corrupted, portrait) {
                    let frames = manager.desktops[&key].frames(portrait, 10.);
                    assert_eq!(frames.len(), 2);
                    for (_, r) in frames {
                        assert!(r.size.width + 0.001 >= 700. && r.size.height + 0.001 >= 200.);
                        assert!(
                            r.origin.x >= portrait.origin.x - 0.001
                                && r.origin.y >= portrait.origin.y - 0.001
                        );
                        assert!(
                            r.origin.x + r.size.width
                                <= portrait.origin.x + portrait.size.width + 0.001
                        );
                        assert!(
                            r.origin.y + r.size.height
                                <= portrait.origin.y + portrait.size.height + 0.001
                        );
                    }
                } else {
                    assert!(manager.desktops[&key] == current);
                    assert_eq!(pref(raw, "tile.grid.enabled"), 0);
                    assert_eq!(layout_pref(raw, key), Layout::Spiral);
                }
            }
            manager.desktops.insert(key, current);
            assert!(manager.restore_distribution_at(raw, key, &text, landscape));
            assert_eq!(manager.desktops[&key].frames(landscape, 10.).len(), 2);
            assert_eq!(pref(raw, "tile.grid.enabled"), 1);
            assert_eq!(layout_pref(raw, key), Layout::Grid);
            assert!(manager.verifications.is_empty());
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
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
                assert!(!manager.needs_placement(
                    *id,
                    *frame,
                    Some(*frame),
                    manager.settling_space()
                ));
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
        assert!(!manager.needs_placement(3, area, Some(resized), manager.settling_space()));
        // A settled window needs no geometry write on repeated notifications.
        for offset in [100, 200, 300] {
            manager.space_changed_with_visible_at(
                now + Duration::from_millis(offset),
                &[(1, 9), portrait],
            );
            assert_eq!(manager.applied[&4], area);
            assert!(!manager.needs_placement(4, area, Some(area), manager.settling_space()));
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
            assert!(!manager.needs_placement(next, target, Some(target), manager.settling_space()));
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
        assert!(manager.drag.is_none());
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
        assert!(manager.drag.is_none());
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
            assert!(!manager.needs_placement(1, target, Some(actual), false));
        }
        assert!(manager.needs_placement(1, rect(-890., -390., 1200., 1580.), Some(actual), false));
        assert!(manager.needs_placement(1, target, Some(rect(-800., -390., 1100., 1580.)), false));
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
            let retained = grace.retained(
                &uncertain,
                &HashSet::new(),
                now + Duration::from_millis(millis),
            );
            let mut members = vec![2, 3];
            members.extend(retained);
            desktop.reconcile_at(&members, area, 10., Some(2));
            assert_eq!(desktop.frames(area, 10.), before);
        }
        assert!(
            grace
                .retained(
                    &uncertain,
                    &HashSet::new(),
                    now + Duration::from_millis(650)
                )
                .is_empty()
        );
        assert!(
            grace
                .retained(&uncertain, &HashSet::new(), now + Duration::from_secs(5))
                .is_empty()
        );
        // Recovery resets the grace period for a later, independent failure.
        grace.retained(
            &HashSet::new(),
            &HashSet::new(),
            now + Duration::from_secs(6),
        );
        assert_eq!(
            grace.retained(&uncertain, &HashSet::new(), now + Duration::from_secs(7)),
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
            previews: DropPreviewCache::default(),
        }
    }
    #[test]
    fn move_preview_is_not_owned_by_the_disabled_resize_path() {
        let mut drag = test_drag();
        drag.moved = true;
        let point = drag.point;
        let space = drag.space;
        let mut manager = Manager {
            drag: Some(drag),
            ..Default::default()
        };
        // Neither option may enter native geometry or manipulate the move
        // panel. The caller alone renders its destination once per sample.
        for preview in [false, true] {
            assert!(
                manager
                    .resize_drag(false, point, Some(space), preview)
                    .is_none()
            );
            assert!(manager.drag.as_ref().unwrap().moved);
        }
    }
    #[test]
    fn a_classified_gesture_no_longer_queries_native_bounds_on_each_tick() {
        let mut drag = test_drag();
        drag.moved = true;
        let space = drag.space;
        let point = drag.point;
        let mut selected = Some(drag);
        for _ in 0..600 {
            resolve_gesture(&mut selected, &mut [], point, Some(space), |_| {
                panic!("redundant native query")
            });
        }
        assert!(selected.unwrap().moved);
    }
    #[test]
    fn a_brief_gap_does_not_flash_preview_but_release_always_clears_it() {
        let mut drag = test_drag();
        let now = Instant::now();
        let target = Some((drag.space, DropIntent::Window(2, Zone::Center)));
        drag.update_drop(target, now, false);
        drag.update_drop(None, now + Duration::from_millis(10), false);
        assert_eq!(drag.drop, target);
        drag.update_drop(target, now + Duration::from_millis(20), false);
        assert_eq!(drag.drop, target);
        drag.update_drop(None, now + Duration::from_millis(30), false);
        drag.update_drop(None, now + Duration::from_millis(100), false);
        assert_eq!(drag.drop, None);
        drag.update_drop(target, now + Duration::from_millis(110), false);
        drag.update_drop(None, now + Duration::from_millis(111), true);
        assert_eq!(drag.drop, None);
    }
    #[test]
    fn preview_cache_matches_fresh_proposals_and_invalidates_all_geometry_inputs() {
        let area = rect(-1080., -500., 1080., 1880.);
        let mut desktop = Desktop::configured(Layout::ThreeBands, Some((17, 9)));
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let mut minimums = HashMap::new();
        let mut cache = DropPreviewCache::default();
        let intent = DropIntent::Window(2, Zone::Center);
        let check = |cache: &mut DropPreviewCache,
                     desktop: &Desktop,
                     minimums: &HashMap<_, _>,
                     space,
                     area,
                     id| {
            let context = DropContext {
                workspace: space,
                id,
                area,
                desktop,
                minimums,
            };
            assert_eq!(cache.slots(context), desktop.frames(area, 10.));
            let expected = arrival_proposal(desktop, id, Some(intent), area, Some(minimums))
                .and_then(|d| {
                    d.frames(area, 10.)
                        .into_iter()
                        .find(|(w, _)| *w == id)
                        .map(|(_, r)| r)
                });
            for _ in 0..120 {
                assert_eq!(cache.preview(context, intent), expected);
            }
            assert_eq!(cache.context(context).proposals.len(), 1);
        };
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        let from = desktop.frames(area, 10.)[1].1;
        desktop.resize(
            2,
            from,
            rect(
                from.origin.x,
                from.origin.y,
                from.size.width,
                from.size.height + 90.,
            ),
            area,
            10.,
        );
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        desktop.grid = None;
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        desktop.reset_template(Layout::VerticalMaster, area);
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        minimums.insert(1, objc2_foundation::NSSize::new(5000., 5000.));
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        minimums.clear();
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
        check(&mut cache, &desktop, &minimums, (1, 7), area, 2);
        let other = rect(20., 41., 1880., 1080.);
        for space in [(1, 8), (2, 8), (3, 9), (1, 7)] {
            check(&mut cache, &desktop, &minimums, space, other, 1);
            assert!(cache.contexts.len() <= 2);
        }
        desktop.reconcile(&[1, 2, 3, 4], area, 10.);
        check(&mut cache, &desktop, &minimums, (1, 7), area, 1);
    }
    #[test]
    #[ignore = "CPU-only drag-path benchmark; run with --release --ignored --nocapture"]
    fn benchmark_drop_previews() {
        let iterations = 2000;
        let mut before = Duration::ZERO;
        let mut after = Duration::ZERO;
        for layout in Layout::ALL {
            for grid in [None, Some((17, 9))] {
                for area in [
                    rect(20., 41., 2560., 1440.),
                    rect(-1440., -500., 1440., 2560.),
                ] {
                    let mut desktop = Desktop::configured(layout, grid);
                    desktop.reconcile(&[1, 2, 3, 4, 5, 6, 7], area, 10.);
                    let minimums = HashMap::new();
                    let intent = DropIntent::Window(2, Zone::Center);
                    let start = Instant::now();
                    for _ in 0..iterations {
                        let destination = std::hint::black_box(&desktop).clone();
                        std::hint::black_box(destination.frames(area, 10.));
                        std::hint::black_box(arrival_proposal(
                            &destination,
                            1,
                            Some(intent),
                            area,
                            Some(&minimums),
                        ));
                        let destination = std::hint::black_box(&desktop).clone();
                        std::hint::black_box(
                            arrival_proposal(&destination, 1, Some(intent), area, Some(&minimums))
                                .map(|d| d.frames(area, 10.)),
                        );
                    }
                    before += start.elapsed();
                    let mut cache = DropPreviewCache::default();
                    let context = DropContext {
                        workspace: (1, 7),
                        id: 1,
                        area,
                        desktop: &desktop,
                        minimums: &minimums,
                    };
                    let start = Instant::now();
                    for _ in 0..iterations {
                        std::hint::black_box(cache.slots(context));
                        std::hint::black_box(cache.preview(context, intent));
                        std::hint::black_box(cache.preview(context, intent));
                    }
                    after += start.elapsed();
                }
            }
        }
        let samples = iterations * Layout::ALL.len() * 4;
        println!(
            "preview_benchmark samples={samples} before_ns_per_sample={:.1} after_ns_per_sample={:.1} speedup={:.2}",
            before.as_nanos() as f64 / samples as f64,
            after.as_nanos() as f64 / samples as f64,
            before.as_secs_f64() / after.as_secs_f64()
        );
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
        assert_eq!(manager.tick_interval(), 0.25);
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
    #[test]
    fn a_single_spiral_tile_needs_no_write_after_leaving_and_returning() {
        let area = rect(10., 41., 1660., 947.);
        let key: Workspace = (1, 7);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[42], area, 10.);
        let original = desktop.frames(area, 10.);
        assert_eq!(original.len(), 1);
        let target = original[0].1;
        let saved = desktop.save_distribution(area);
        let mut manager = Manager {
            active_areas: vec![(key, area)],
            ..Default::default()
        };
        manager.desktops.insert(key, desktop.clone());
        manager.windows.insert(
            42,
            Managed {
                pid: 9,
                space: key,
                original: target,
                original_area: area,
                element: ax::application(std::process::id() as i32).unwrap(),
            },
        );
        manager.applied.insert(42, target);
        let now = Instant::now();
        manager.space_changed_with_visible_at(now, &[key]);
        assert!(manager.waiting_for_space(now + Duration::from_millis(599)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(600)));
        let sample = ReentrySample {
            areas: vec![(key, area)],
            windows: vec![(9, 42, key.1, target, Some(target))],
            absent: vec![],
        };
        assert!(!manager.observe_reentry(Some(sample.clone()), now + Duration::from_millis(600)));
        assert!(manager.observe_reentry(Some(sample), now + Duration::from_millis(800)));
        assert_eq!(manager.desktops[&key].frames(area, 10.), original);
        assert_eq!(manager.desktops[&key].save_distribution(area), saved);
        // The settled frame equals the destination, so no further AX write runs.
        assert!(!manager.needs_placement(42, target, Some(target), manager.settling_space()));
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
        // A committed write verifies complete on its first check: no retry pair.
        assert_eq!(
            Verification::new(target).step(target, Instant::now()),
            VerificationStep::Complete
        );
    }
    #[test]
    fn repeated_space_notifications_leave_one_tile_settled_without_pending_writes() {
        let area = rect(0., 30., 880., 1580.);
        let key: Workspace = (2, 8);
        let mut desktop = Desktop::configured(Layout::ThreeBands, None);
        desktop.reconcile(&[10, 20], area, 10.);
        let original = desktop.frames(area, 10.);
        assert_eq!(original.len(), 2);
        let mut manager = Manager {
            active_areas: vec![(key, area)],
            ..Default::default()
        };
        manager.desktops.insert(key, desktop.clone());
        for (id, frame) in &original {
            manager.windows.insert(
                *id,
                Managed {
                    pid: 42,
                    space: key,
                    original: *frame,
                    original_area: area,
                    element: ax::application(std::process::id() as i32).unwrap(),
                },
            );
            manager.applied.insert(*id, *frame);
        }
        let now = Instant::now();
        for index in 0..5 {
            manager.space_changed_with_visible_at(now + Duration::from_millis(index * 40), &[key]);
        }
        let sample = ReentrySample {
            areas: vec![(key, area)],
            windows: original
                .iter()
                .map(|(id, frame)| (42, *id, key.1, *frame, Some(*frame)))
                .collect(),
            absent: vec![],
        };
        assert!(!manager.observe_reentry(Some(sample.clone()), now + Duration::from_millis(400)));
        assert!(manager.observe_reentry(Some(sample), now + Duration::from_millis(600)));
        assert!(manager.space_reentry.is_none());
        assert!(manager.transitions.is_empty() && manager.verifications.is_empty());
        for (id, frame) in &original {
            assert_eq!(manager.desktops[&key].frames(area, 10.), original);
            assert!(!manager.needs_placement(*id, *frame, Some(*frame), manager.settling_space()));
        }
    }
    #[test]
    fn a_partial_membership_during_settling_keeps_one_distribution_per_window() {
        let area = rect(10., 41., 1660., 947.);
        for layout in Layout::ALL {
            let mut full = Desktop::configured(layout, None);
            full.reconcile(&[10, 20, 30], area, 10.);
            let original = full.frames(area, 10.);
            assert_eq!(original.len(), 3);
            // The compositor names only two windows on the first scan: the same
            // three destinations must result, so each window is written once.
            let mut partial = Desktop::configured(layout, None);
            partial.reconcile(&[10, 20, 30], area, 10.);
            let mut ids = vec![10, 20];
            retain_settled_ids(&mut ids, &partial.order, true);
            partial.reconcile_at(&ids, area, 10., None);
            partial.fit(area, 10.);
            assert_eq!(
                partial.frames(area, 10.),
                original,
                "{layout:?}: the trailing window must keep its leaf during the settling burst"
            );
            // Once settled, a genuinely closed window does collapse the tree.
            let mut closed = Desktop::configured(layout, None);
            closed.reconcile(&[10, 20, 30], area, 10.);
            let mut ids = vec![10, 20];
            retain_settled_ids(&mut ids, &closed.order, false);
            closed.reconcile_at(&ids, area, 10., None);
            assert_eq!(closed.frames(area, 10.).len(), 2);
            // A gap in the middle is the same membership during the burst.
            let mut gap = Desktop::configured(layout, None);
            gap.reconcile(&[10, 20, 30], area, 10.);
            let mut ids = vec![10, 30];
            retain_settled_ids(&mut ids, &gap.order, true);
            gap.reconcile_at(&ids, area, 10., None);
            gap.fit(area, 10.);
            assert_eq!(
                gap.frames(area, 10.),
                original,
                "{layout:?}: a mid-list gap must not collapse the published leaf"
            );
            // One transient extra id (native tab second NSWindow) keeps the tree.
            let mut extra = Desktop::configured(layout, None);
            extra.reconcile(&[10, 20, 30], area, 10.);
            let mut ids = vec![10, 20, 30, 40];
            retain_settled_ids(&mut ids, &extra.order, true);
            extra.reconcile_at(&ids, area, 10., None);
            extra.fit(area, 10.);
            assert_eq!(
                extra.frames(area, 10.),
                original,
                "{layout:?}: the transient extra id must not redistribute the tree"
            );
            // An unrelated membership is not treated as a settling prefix.
            let mut other = Desktop::configured(layout, None);
            other.reconcile(&[10, 20, 30], area, 10.);
            let mut ids = vec![10, 99];
            retain_settled_ids(&mut ids, &other.order, true);
            assert_eq!(ids, vec![10, 99]);
        }
    }
    #[test]
    fn a_committed_destination_is_not_rewritten_while_the_space_settles() {
        let area = rect(10., 41., 1660., 947.);
        let key: Workspace = (1, 4);
        let mut desktop = Desktop::configured(Layout::Grid, None);
        desktop.reconcile(&[7], area, 10.);
        let target = desktop.frames(area, 10.)[0].1;
        // The app's own frame, as the compositor reports it right after the slide.
        let native = rect(200., 120., 900., 600.);
        let mut manager = Manager {
            active_areas: vec![(key, area)],
            ..Default::default()
        };
        manager.desktops.insert(key, desktop.clone());
        manager.windows.insert(
            7,
            Managed {
                pid: 3,
                space: key,
                original: native,
                original_area: area,
                element: ax::application(std::process::id() as i32).unwrap(),
            },
        );
        let base = Instant::now();
        manager.space_changed_with_visible_at(base, &[key]);
        manager.applied.insert(7, target);
        assert!(
            !manager.needs_placement(7, target, Some(native), manager.settling_space()),
            "the committed destination is still pending during the settle window"
        );
        let other = rect(10., 41., 800., 947.);
        assert!(
            manager.needs_placement(7, other, Some(native), manager.settling_space()),
            "a genuinely different template must still be written"
        );
        let sample = ReentrySample {
            areas: vec![(key, area)],
            windows: vec![(3, 7, key.1, target, Some(target))],
            absent: vec![],
        };
        assert!(!manager.observe_reentry(Some(sample.clone()), base + Duration::from_millis(200)));
        assert!(manager.observe_reentry(Some(sample), base + Duration::from_millis(400)));
        assert!(
            !manager.needs_placement(7, target, Some(native), manager.settling_space()),
            "the bounded window keeps the single committed write"
        );
        std::thread::sleep(Duration::from_millis(1300));
        assert!(
            manager.needs_placement(7, target, Some(native), manager.settling_space()),
            "after the settle window, real drift is repaired again"
        );
    }
    #[test]
    fn the_verification_skips_only_the_destination_committed_during_settling() {
        let target = rect(10., 41., 1660., 947.);
        let native = rect(200., 120., 900., 600.);
        let other = rect(10., 41., 800., 947.);
        assert!(
            committed_during_settling(true, Some(target), target),
            "the committed destination owns the window while the Space settles"
        );
        assert!(
            !committed_during_settling(true, Some(native), target),
            "the pre-slide frame is not a destination, so the retry still runs"
        );
        assert!(
            !committed_during_settling(true, Some(other), target),
            "a different committed rect must not silence this window's retry"
        );
        assert!(
            !committed_during_settling(false, Some(target), target),
            "after the bounded window, a real drift is repaired again"
        );
        assert!(
            !committed_during_settling(true, None, target),
            "without a committed destination the verification keeps its own budget"
        );
    }
}
