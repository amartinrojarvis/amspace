//! Main-thread macOS adapter. Layout calculations live in `tiling`.
use crate::model::{TAB_ICON_BACKING, TabAccent};
use crate::{
    accessibility as ax,
    native::{Obj, alloc_view, color, rect},
    spaces,
    tiling::{
        Desktop, DropIntent, GROUP_DROP_FRACTION, Group, Layout, TAB_BAR_GAP, TAB_BAR_HEIGHT, Zone,
        ax_screen_rect, contains, drop_intent,
    },
    window_server as ws,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::{NSPoint, NSRect, NSString};
use std::{
    collections::{HashMap, HashSet},
    ptr,
    sync::LazyLock,
    time::{Duration, Instant},
};

#[path = "tab_diagnostics.rs"]
pub mod diagnostics;

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
        let frames = desktop.window_frames(self.area, 10.);
        (frames.len() == desktop.order.len()).then_some(())?;
        frames
            .iter()
            .all(|(_, r)| r.size.width >= 120. && r.size.height >= 90.)
            .then_some(desktop)
    }
}
pub fn max_group_tabs(defaults: Obj) -> usize {
    pref_or(defaults, "tile.groups.max", 5).clamp(2, 20) as usize
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
impl Managed {
    fn restoration_target(
        &self,
        id: u32,
        identities: Option<&HashSet<(i32, u32)>>,
        native_space: Option<u64>,
        ax_id: Option<u32>,
        current_area: Option<NSRect>,
    ) -> Option<NSRect> {
        if !identities.is_some_and(|ids| ids.contains(&(self.pid, id)))
            || native_space != Some(self.space.1)
            || ax_id != Some(id)
        {
            return None;
        }
        Some(restoration_frame(
            self.original,
            self.original_area,
            current_area?,
        ))
    }
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
struct TabDrag {
    id: u32,
    pid: i32,
    space: Workspace,
    area: NSRect,
    panel_number: isize,
    baseline: Desktop,
    released_at: Option<Instant>,
    proposed: Option<Desktop>,
    reorder: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabDragOutcome {
    Rejected,
    Tracking,
    Reordered,
    Detached,
}

fn tab_detach_proposal(
    desktop: &Desktop,
    id: u32,
    area: NSRect,
    minimums: &HashMap<u32, objc2_foundation::NSSize>,
) -> Option<Desktop> {
    desktop
        .tab_strip(id)
        .filter(|(members, _)| members.len() > 1)?;
    // A screen edge resolves against visible leaves after detaching. Another
    // member's ID might remain hidden and cannot name an independent slot.
    arrival_proposal(
        desktop,
        id,
        Some(DropIntent::Screen(Zone::Right)),
        area,
        Some(minimums),
    )
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
        self.context(input).preview(intent)
    }

    fn hint(
        &mut self,
        input: DropContext<'_>,
        intent: DropIntent,
        grouped: bool,
        preview: Option<NSRect>,
    ) -> Option<DropHint> {
        let context = self.context(input);
        // Managed slots have exactly two actions. Arrivals can also split edges:
        // keep their actual proposed destination instead of advertising a swap.
        if grouped && context.slots.iter().any(|(id, _)| *id == input.id) {
            let target = match intent {
                DropIntent::Group(id) | DropIntent::Window(id, Zone::Center) => Some(id),
                _ => None,
            };
            if let Some((target, frame)) = target
                .and_then(|target| context.slots.iter().find(|(id, _)| *id == target).copied())
            {
                let available = [
                    context.preview(DropIntent::Group(target)).is_some(),
                    context
                        .preview(DropIntent::Window(target, Zone::Center))
                        .is_some(),
                ];
                return Some(DropHint {
                    frame,
                    kind: HintKind::Zones,
                    available,
                    active: preview.map(|_| usize::from(!matches!(intent, DropIntent::Group(_)))),
                });
            }
        }
        preview.map(|frame| DropHint::single(intent, frame))
    }
}

impl PreviewContext {
    fn preview(&mut self, intent: DropIntent) -> Option<NSRect> {
        if let Some((_, frame)) = self.proposals.iter().find(|(key, _)| *key == intent) {
            return *frame;
        }
        let frame = arrival_proposal(
            &self.desktop,
            self.id,
            Some(intent),
            self.area,
            Some(&self.minimums),
        )
        .and_then(|d| {
            d.window_frames(self.area, 10.)
                .into_iter()
                .find(|(id, _)| *id == self.id)
                .map(|(_, r)| r)
        });
        if self.proposals.len() == 32 {
            self.proposals.remove(0);
        }
        self.proposals.push((intent, frame));
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
    let frames = proposed.window_frames(area, 10.);
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
        let content_from = layout.window_frame(self.id, from);
        let requested = if layout.order.first() == Some(&self.id) {
            layout.layout.primary_center_axis().map_or(frame, |axis| {
                centered_resize_request(content_from, self.start, self.point, point, frame, axis)
            })
        } else {
            frame
        };
        layout.resize(
            self.id,
            from,
            layout.slot_frame(self.id, requested),
            self.area,
            10.,
        );
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
            due: Instant::now() + Duration::from_millis(60),
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
        self.due = now + Duration::from_millis(60);
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
                return now.duration_since(*since) >= Duration::from_millis(60);
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
#[derive(Clone, Copy)]
enum StripSpaceBinding {
    Pending(Instant),
    Confirmed,
    Fallback,
}

/// A reused strip panel and the content last painted into it.
struct TabStrip {
    panel: Retained<AnyObject>,
    space: Workspace,
    area: Option<NSRect>,
    binding: StripSpaceBinding,
    marker: Option<Retained<AnyObject>>,
    signature: Vec<(String, Option<Retained<AnyObject>>)>,
    members: Vec<u32>,
    active: usize,
    built: NSRect,
    frame: NSRect,
}
impl TabStrip {
    fn show_at(&mut self, frame: NSRect, primary: f64) {
        self.confirm_space();
        unsafe {
            if self.frame != frame {
                let _: () = msg_send![&*self.panel,setFrame:appkit(frame, primary),display:true];
                self.frame = frame;
            }
            self.show();
        }
    }
    fn show(&self) {
        unsafe {
            let visible: bool = msg_send![&*self.panel, isVisible];
            if !visible {
                let _: () = msg_send![&*self.panel, orderFrontRegardless];
                let root: Obj = msg_send![&*self.panel, contentView];
                sync_strip_hover(&*root);
            }
        }
    }

    fn confirm_space(&mut self) {
        let StripSpaceBinding::Pending(until) = self.binding else {
            return;
        };
        let number: isize = unsafe { msg_send![&*self.panel, windowNumber] };
        let membership = u32::try_from(number).ok().and_then(spaces::window_spaces);
        match membership.as_deref() {
            Some([space]) if *space == self.space.1 => {
                self.binding = StripSpaceBinding::Confirmed;
            }
            // WindowServer publishes a newly ordered panel after the run loop
            // returns. The existing tiling cadence supplies bounded retries.
            Some([]) if Instant::now() < until => {}
            _ => {
                self.binding = StripSpaceBinding::Fallback;
                unsafe {
                    let _: () = msg_send![&*self.panel, setCollectionBehavior:((1usize << 0) | (1 << 3) | (1 << 6))];
                }
            }
        }
    }

    fn accepts_input(&self) -> bool {
        unsafe { msg_send![&*self.panel, isVisible] && msg_send![&*self.panel, isOnActiveSpace] }
    }

    fn clear_marker(&mut self) {
        if let Some(marker) = self.marker.take() {
            unsafe {
                let _: () = msg_send![&*marker, removeFromSuperview];
            }
        }
    }

    /// Coordinates come from the recognizer's window, not the global cursor.
    /// The document conversion includes horizontal scrolling.
    fn reorder_target(&mut self, id: u32, location: NSPoint, scroll: bool) -> Option<usize> {
        unsafe {
            let viewport: Obj = msg_send![&*self.panel, contentView];
            let document: Obj = msg_send![viewport, documentView];
            let mut point: NSPoint =
                msg_send![document, convertPoint:location, fromView:ptr::null::<AnyObject>()];
            let mut visible: NSRect = msg_send![document, visibleRect];
            let corridor = rect(
                visible.origin.x - 24.,
                visible.origin.y - 12.,
                visible.size.width + 48.,
                visible.size.height + 24.,
            );
            if !point.x.is_finite() || !point.y.is_finite() || !contains(corridor, point) {
                self.clear_marker();
                return None;
            }
            if scroll {
                let step = if point.x < visible.origin.x + 16. {
                    -24.
                } else if point.x > visible.origin.x + visible.size.width - 16. {
                    24.
                } else {
                    0.
                };
                if step != 0. {
                    let bounds: NSRect = msg_send![document, bounds];
                    let x = (visible.origin.x + step)
                        .clamp(0., (bounds.size.width - visible.size.width).max(0.));
                    let clip: Obj = msg_send![viewport, contentView];
                    let _: () = msg_send![clip, scrollToPoint:NSPoint::new(x, visible.origin.y)];
                    let _: () = msg_send![viewport, reflectScrolledClipView:clip];
                    point = msg_send![document, convertPoint:location, fromView:ptr::null::<AnyObject>()];
                    visible = msg_send![document, visibleRect];
                }
            }
            let (index, x) = tab_reorder_target(
                &self.members,
                self.signature.iter().map(|(name, _)| tab_width(name)),
                id,
                point.x,
            )?;
            if self.members.get(index) == Some(&id) {
                self.clear_marker();
            } else {
                let marker = self.marker.get_or_insert_with(|| {
                    let marker = alloc_view(class!(NSView), NSRect::ZERO);
                    let _: () = msg_send![&marker, setWantsLayer:true];
                    let layer: Obj = msg_send![&marker, layer];
                    let cg: *const crate::native::CgColor =
                        msg_send![color(1., 1., 1., 0.95), CGColor];
                    let _: () = msg_send![layer, setBackgroundColor:cg];
                    let _: () = msg_send![layer, setCornerRadius:1f64];
                    let _: () = msg_send![document, addSubview:&*marker];
                    marker
                });
                let left = visible.origin.x + 1.;
                let x = x.clamp(left, (visible.origin.x + visible.size.width - 3.).max(left));
                let _: () = msg_send![&**marker, setFrame:rect(x, 3., 2., TAB_BAR_HEIGHT - 6.)];
            }
            Some(index)
        }
    }
}

impl Drop for TabStrip {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&*self.panel, orderOut:ptr::null::<AnyObject>()];
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HintKind {
    Zones,
    Group,
    Place,
}

#[derive(Clone, Copy)]
struct DropHint {
    frame: NSRect,
    kind: HintKind,
    available: [bool; 2],
    active: Option<usize>,
}

impl DropHint {
    fn single(intent: DropIntent, slot: NSRect) -> Self {
        let width = slot.size.width.min(280.);
        let height = slot.size.height.min(156.);
        Self {
            frame: rect(
                slot.origin.x + (slot.size.width - width) / 2.,
                slot.origin.y + (slot.size.height - height) / 2.,
                width,
                height,
            ),
            kind: if matches!(intent, DropIntent::Group(_)) {
                HintKind::Group
            } else {
                HintKind::Place
            },
            available: [true, false],
            active: Some(0),
        }
    }
}

/// Cached plates: moving the destination or crossing its divider reuses views.
struct HintBadge {
    panel: Retained<AnyObject>,
    hint: DropHint,
    plates: [Obj; 2],
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
    desktop_resume_until: Option<Instant>,
    desktop_floating: HashSet<(i32, u32)>,
    last_visible_windows: Option<HashSet<(i32, u32)>>,
    restore_after_presentation: bool,
    restore_spaces_after_presentation: HashSet<u64>,
    drag: Option<Drag>,
    tab_drag: Option<TabDrag>,
    gesture_candidates: Vec<Drag>,
    pending_drop: Option<PendingDrop>,
    handoffs: HashMap<u32, MonitorHandoff>,
    pressed: bool,
    preview: Vec<Retained<AnyObject>>,
    /// One rounded strip per tabbed group, reused across ticks.
    strips: Vec<TabStrip>,
    tab_target: Option<Retained<AnyObject>>,
    /// Transient SF Symbol hints over the resolved drop destination.
    badges: Vec<HintBadge>,
    last_scan: Option<Instant>,
    last_area_update: Option<Instant>,
    active_areas: Vec<(Workspace, NSRect)>,
    dirty: bool,
    space_settle: Option<Instant>,
    space_quiet_until: Option<Instant>,
    space_reentry: Option<SpaceReentry>,
    reentry_preserved: Vec<(Workspace, NSRect)>,
    /// `Some(false)` selects the single-write commit; `None`/`Some(true)` keeps
    /// the eased 160 ms batch. `tick_mode_input` refreshes it from preferences.
    animated: Option<bool>,
    pub last_error: Option<String>,
    /// One-shot notice for the gesture that just ended. The parent consumes it
    /// after each mouse event and tile tick; routine background fitting
    /// failures stay in `last_error` (Settings-only) so no frame repeats them.
    pub interaction_error: Option<String>,
    accent: TabAccent,
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
            .map(|desktop| desktop.window_frames(self.destination_area, 10.))
            .unwrap_or_default()
    }
}
impl Manager {
    pub fn with_tab_target(target: Retained<AnyObject>) -> Self {
        Self {
            tab_target: Some(target),
            ..Self::default()
        }
    }

    /// Retint existing layers without rebuilding controls or reading AX titles.
    pub fn set_tab_accent(&mut self, rgb: [f64; 3]) {
        if self.accent.rgb == rgb {
            return;
        }
        let next = TabAccent::new(rgb);
        if next == self.accent {
            return;
        }
        self.accent = next;
        if self.strips.is_empty() {
            return;
        }
        unsafe {
            let [r, g, b] = next.backing();
            let cg: *const crate::native::CgColor = msg_send![color(r, g, b, 1.), CGColor];
            for strip in &self.strips {
                let Some(id) = strip.members.get(strip.active) else {
                    continue;
                };
                let root: Obj = msg_send![&strip.panel, contentView];
                let button: Obj = msg_send![root, viewWithTag:*id as isize];
                if button.is_null() {
                    continue;
                }
                let container: Obj = msg_send![button, superview];
                let plate = (*container)
                    .class()
                    .instance_variable(c"accentPlate")
                    .unwrap()
                    .load_ptr::<Obj>(&*container)
                    .read();
                if !plate.is_null() {
                    let layer: Obj = msg_send![plate, layer];
                    let _: () = msg_send![layer, setBackgroundColor:cg];
                }
            }
        }
    }

    /// Only an explicit tab click activates/raises a foreign window.
    pub fn activate_tab(&mut self, id: u32) -> bool {
        if self.presentation_blocks_input()
            || !ax::trusted()
            || crate::focus::secure_input()
            || self.drag.is_some()
            || self.tab_drag.is_some()
            || self.pending_drop.is_some()
        {
            return false;
        }
        let Some(window) = self.windows.get(&id) else {
            return false;
        };
        if self.floating.contains(&id)
            || !self
                .active_areas
                .iter()
                .any(|(key, _)| *key == window.space)
            || spaces::window_space(id) != Some(window.space.1)
            || ax::window_id(window.element.0) != Some(id)
            || !ax::tileable(window.element.0)
            || !ws::all_window_ids().is_some_and(|ids| ids.contains(&(window.pid, id)))
            || !self
                .desktops
                .get(&window.space)
                .is_some_and(|d| d.tab_role(id).is_some())
        {
            return false;
        }
        if !ax::focus(window.element.0, window.pid) {
            return false;
        }
        self.select_tab_model(id);
        true
    }
    /// Close is a native request, never an optimistic removal from the tree.
    pub fn close_tab(&mut self, id: u32) -> bool {
        if self.presentation_blocks_input()
            || self.space_quiet_until.is_some()
            || self.space_reentry.is_some()
            || !ax::trusted()
            || crate::focus::secure_input()
            || self.drag.is_some()
            || self.tab_drag.is_some()
            || self.pending_drop.is_some()
        {
            return false;
        }
        let Some(window) = self.windows.get(&id) else {
            return false;
        };
        // Do not move focus away from a save dialog. Explicitly selecting a
        // hidden member before closing makes its own confirmation visible.
        if ax::focus_is_protected(window.pid) || !self.activate_tab(id) {
            return false;
        }
        if !ax::request_close(self.windows[&id].element.0) {
            self.last_error = Some("La aplicación no ha aceptado cerrar esta ventana.".into());
            return false;
        }
        self.cancel_window_placement(id);
        self.invalidate();
        true
    }

    fn select_tab_model(&mut self, id: u32) {
        let Some(space) = self.windows.get(&id).map(|w| w.space) else {
            return;
        };
        let Some(desktop) = self.desktops.get_mut(&space) else {
            return;
        };
        if desktop.tab_role(id) != Some(false) {
            return;
        }
        if let Some(previous) = desktop.select_tab(id) {
            self.cancel_window_placement(previous);
            self.cancel_window_placement(id);
            self.invalidate();
        }
    }

    /// AppKit owns the threshold/click suppression. Only Ended changes the model.
    pub fn tab_drag(&mut self, button: Obj, state: isize, location: NSPoint) -> TabDragOutcome {
        use TabDragOutcome::*;
        if matches!(state, 4 | 5)
            || button.is_null()
            || self.presentation_blocks_input()
            || self.space_quiet_until.is_some()
            || self.space_reentry.is_some()
            || !location.x.is_finite()
            || !location.y.is_finite()
            || !ax::trusted()
            || crate::focus::secure_input()
        {
            self.cancel_tab_drag();
            return Rejected;
        }
        let (id, panel_number) = unsafe {
            let tag: isize = msg_send![button, tag];
            let window: Option<Retained<AnyObject>> = msg_send![button, window];
            let Some(window) = window else {
                self.cancel_tab_drag();
                return Rejected;
            };
            let Ok(id) = u32::try_from(tag) else {
                return Rejected;
            };
            if !self.strips.iter().any(|strip| {
                Retained::as_ptr(&strip.panel) == Retained::as_ptr(&window)
                    && strip.members.contains(&id)
                    && strip.accepts_input()
            }) {
                self.cancel_tab_drag();
                return Rejected;
            }
            (id, msg_send![&*window, windowNumber])
        };
        match state {
            1 => {
                self.cancel_tab_drag();
                self.pressed = true;
                if self.drag.is_some() || self.pending_drop.is_some() {
                    return Rejected;
                }
                let Some(window) = self.windows.get(&id) else {
                    return Rejected;
                };
                let Some((_, area)) = self
                    .active_areas
                    .iter()
                    .find(|(key, _)| *key == window.space)
                else {
                    return Rejected;
                };
                let Some(desktop) = self.desktops.get(&window.space) else {
                    return Rejected;
                };
                let Some((members, _)) = desktop
                    .tab_strip(id)
                    .filter(|(members, _)| members.len() > 1)
                else {
                    return Rejected;
                };
                if self.floating.contains(&id)
                    || spaces::window_space(id) != Some(window.space.1)
                    || ax::window_id(window.element.0) != Some(id)
                    || !ax::tileable(window.element.0)
                    || !ws::all_window_ids().is_some_and(|ids| ids.contains(&(window.pid, id)))
                {
                    return Rejected;
                }
                // A strip may be reorderable even when no independent tile fits.
                let proposed = tab_detach_proposal(desktop, id, *area, &self.minimum_sizes);
                self.tab_drag = Some(TabDrag {
                    id,
                    pid: window.pid,
                    space: window.space,
                    area: *area,
                    panel_number,
                    released_at: None,
                    baseline: desktop.clone(),
                    proposed,
                    reorder: members.iter().position(|member| *member == id),
                });
                self.preview_tab_drag(location, false);
                Tracking
            }
            2 | 3 => {
                if !self.tab_drag.as_ref().is_some_and(|drag| {
                    drag.id == id
                        && drag.panel_number == panel_number
                        && self.tab_drag_matches(drag)
                }) {
                    self.cancel_tab_drag();
                    return Rejected;
                }
                self.preview_tab_drag(location, state == 2);
                if state == 2 {
                    return Tracking;
                }
                let drag = self.tab_drag.take().unwrap();
                self.hide_preview_panels();
                for strip in &mut self.strips {
                    strip.clear_marker();
                }
                if !spaces::visible_desktops().contains(&drag.space)
                    || area(drag.space.0) != Some(drag.area)
                {
                    return Rejected;
                }
                let Some(ids) = ws::all_window_ids() else {
                    return Rejected;
                };
                if drag
                    .baseline
                    .order
                    .iter()
                    .chain(drag.baseline.groups().iter().flat_map(|g| &g.members))
                    .any(|member| {
                        !self
                            .windows
                            .get(member)
                            .is_some_and(|w| ids.contains(&(w.pid, *member)))
                    })
                {
                    return Rejected;
                }
                let window = &self.windows[&id];
                if spaces::window_space(id) != Some(drag.space.1)
                    || ax::window_id(window.element.0) != Some(id)
                    || !ax::tileable(window.element.0)
                {
                    return Rejected;
                }
                if let Some(index) = drag.reorder {
                    self.commit_tab_reorder(drag.space, id, index);
                    return Reordered;
                }
                if drag.proposed.is_none() {
                    self.interaction_error = Some(DETACH_CAPACITY.to_string());
                    return Rejected;
                }
                if !ax::focus(window.element.0, window.pid) {
                    return Rejected;
                }
                self.commit_tab_detach(drag);
                Detached
            }
            _ => Rejected,
        }
    }

    fn preview_tab_drag(&mut self, location: NSPoint, scroll: bool) {
        let Some(drag) = &self.tab_drag else {
            return;
        };
        let Some(strip) = self.strips.iter_mut().find(|strip| {
            let number: isize = unsafe { msg_send![&*strip.panel, windowNumber] };
            number == drag.panel_number && strip.space == drag.space
        }) else {
            return;
        };
        let reorder = strip.reorder_target(drag.id, location, scroll);
        let changed = reorder != drag.reorder;
        let frame = if changed && reorder.is_none() {
            drag.proposed.as_ref().and_then(|desktop| {
                desktop
                    .window_frames(drag.area, 10.)
                    .into_iter()
                    .find_map(|(id, frame)| (id == drag.id).then_some(frame))
            })
        } else {
            None
        };
        self.tab_drag.as_mut().unwrap().reorder = reorder;
        if let Some(frame) = frame {
            self.show_preview(frame);
        } else if changed {
            self.hide_preview_panels();
        }
    }

    fn commit_tab_reorder(&mut self, space: Workspace, id: u32, index: usize) {
        if let Some(desktop) = self.desktops.get_mut(&space)
            && desktop.reorder_tab(id, index)
        {
            // No focus, AX writes or cancellation of unrelated placements.
            self.dirty = true;
        }
    }

    fn tab_drag_matches(&self, drag: &TabDrag) -> bool {
        self.active_areas.contains(&(drag.space, drag.area))
            && self.desktops.get(&drag.space) == Some(&drag.baseline)
            && self
                .windows
                .get(&drag.id)
                .is_some_and(|w| w.pid == drag.pid && w.space == drag.space)
            && !self.floating.contains(&drag.id)
    }

    fn commit_tab_detach(&mut self, drag: TabDrag) {
        if let Some(proposed) = drag.proposed {
            self.commit_tab_model(drag.space, drag.id, proposed);
        }
    }

    fn commit_tab_model(&mut self, space: Workspace, id: u32, proposed: Desktop) {
        if let Some(previous) = self.desktops.insert(space, proposed) {
            self.cancel_desktop_placements(&previous);
        }
        self.gesture_candidates.clear();
        self.handoffs.remove(&id);
        self.invalidate();
    }

    fn observe_tab_pointer(&mut self, down: bool, now: Instant) {
        let Some(drag) = &self.tab_drag else { return };
        if !self.tab_drag_matches(drag) {
            self.cancel_tab_drag();
        } else if !down {
            // The global mouse-up monitor can precede AppKit's Ended action.
            // Keep one bounded delivery window, not an indefinitely stuck preview.
            let released = self
                .tab_drag
                .as_mut()
                .unwrap()
                .released_at
                .get_or_insert(now);
            if now.duration_since(*released) >= Duration::from_millis(350) {
                self.cancel_tab_drag();
            }
        }
    }

    pub fn cancel_tab_drag(&mut self) {
        if self.tab_drag.take().is_some() {
            self.hide_preview_panels();
            for strip in &mut self.strips {
                strip.clear_marker();
            }
        }
    }
    /// Explicit user pause: keep every tree, membership and original frame,
    /// drop only the in-flight writes and the visible overlays.
    pub fn pause_user(&mut self) {
        self.cancel_drag();
        self.interaction_error = None;
    }

    pub fn set_max_group_tabs(&mut self, maximum: usize) {
        for desktop in self.desktops.values_mut() {
            desktop.max_group_tabs = maximum;
        }
    }

    /// Snapshot of the tab the strip is showing. Every later action revalidates
    /// it, so a menu opened before a Space switch or a window replacement can
    /// never act on stale identities.
    pub fn tab_context(&mut self, id: u32) -> Result<TabContext, &'static str> {
        self.tab_identity_guards()?;
        self.current_tab_context(id).map(|(context, _)| context)
    }

    /// Re-run the identity checks of a captured context. A changed PID, a
    /// recycled window id, another native Space, a new area or a modal owner
    /// makes the stored context stale.
    fn validate_tab_context(&mut self, context: &TabContext) -> Result<NSRect, &'static str> {
        self.tab_identity_guards()?;
        let (current, area) = self.current_tab_context(context.id)?;
        (current == *context)
            .then_some(area)
            .ok_or("La pestaña cambió; vuelve a abrir el menú.")
    }

    fn tab_identity_guards(&mut self) -> Result<(), &'static str> {
        if !ax::trusted() {
            return Err("Concede Accesibilidad a AMSpace para gestionar las pestañas.");
        }
        if crate::focus::secure_input() {
            return Err("Hay una entrada segura activa; termina ese cuadro de diálogo.");
        }
        if self.presentation_blocks_input() {
            return Err("Mosaico está en pausa o el Escritorio está activo.");
        }
        if self.space_quiet_until.is_some() || self.space_reentry.is_some() {
            return Err("Espera a que termine el cambio de escritorio.");
        }
        if self.drag.is_some() || self.tab_drag.is_some() || self.pending_drop.is_some() {
            return Err("Termina el arrastre en curso antes de usar el menú de pestañas.");
        }
        Ok(())
    }

    /// The live identity plus the area its Space currently publishes.
    fn current_tab_context(&self, id: u32) -> Result<(TabContext, NSRect), &'static str> {
        let window = self
            .windows
            .get(&id)
            .ok_or("Esta ventana ya no está gestionada por Mosaico.")?;
        let (pid, space) = (window.pid, window.space);
        let slot = self
            .active_areas
            .iter()
            .find(|(key, _)| *key == space)
            .map(|(_, rect)| *rect)
            .ok_or("Este escritorio ya no está visible.")?;
        let desktop = self
            .desktops
            .get(&space)
            .ok_or("Este escritorio ya no está gestionado.")?;
        if self.floating.contains(&id)
            || !desktop
                .tab_strip(id)
                .is_some_and(|(members, _)| members.len() > 1)
        {
            return Err("Esta ventana ya no pertenece a un grupo de pestañas.");
        }
        // One fresh area read is the authority: a Dock/menu-bar change or a
        // Space slide invalidates every frame the menu cached.
        if !spaces::visible_desktops().contains(&space)
            || spaces::window_space(id) != Some(space.1)
            || area(space.0) != Some(slot)
        {
            return Err("La ventana cambió de escritorio o de área; vuelve a abrir el menú.");
        }
        if ax::window_id(window.element.0) != Some(id) || !ax::tileable(window.element.0) {
            return Err("La identidad de la ventana cambió; vuelve a abrir el menú.");
        }
        let ids = ws::all_window_ids().ok_or("No se pudo comprobar la lista de ventanas.")?;
        let members = desktop
            .order
            .iter()
            .copied()
            .chain(desktop.groups().iter().flat_map(|group| {
                group
                    .members
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, member)| (index != group.active).then_some(*member))
            }));
        for member in members {
            if !self.windows.get(&member).is_some_and(|w| {
                w.space == space
                    && ids.contains(&(w.pid, member))
                    && spaces::window_space(member) == Some(space.1)
            }) {
                return Err("Las ventanas del escritorio cambiaron; vuelve a abrir el menú.");
            }
        }
        if ax::focus_is_protected(pid) {
            return Err("Un cuadro de diálogo de la aplicación tiene el foco.");
        }
        Ok((
            TabContext {
                id,
                pid,
                workspace: space,
            },
            slot,
        ))
    }

    /// Commit one menu action against the currently validated context.
    pub fn perform_tab_action(
        &mut self,
        context: &TabContext,
        action: TabAction,
    ) -> Result<(), &'static str> {
        let area = self.validate_tab_context(context)?;
        let id = context.id;
        match action {
            TabAction::Detach => self.detach_tab(id, area),
            TabAction::Float => self.float_tab(id, area),
            TabAction::MoveToGroup(target) => self.move_tab_to_group(context, target, area),
            TabAction::Close => {
                if self.close_tab(id) {
                    return Ok(());
                }
                let reason = if self.last_error.is_some() {
                    "La aplicación no ha aceptado cerrar esta ventana."
                } else {
                    "La ventana no está lista para cerrarse; inténtalo de nuevo."
                };
                self.last_error = None;
                Err(reason)
            }
        }
    }

    fn detach_tab(&mut self, id: u32, area: NSRect) -> Result<(), &'static str> {
        let space = self.windows[&id].space;
        let baseline = self
            .desktops
            .get(&space)
            .ok_or("Este escritorio ya no está visible.")?;
        let proposed =
            tab_detach_proposal(baseline, id, area, &self.minimum_sizes).ok_or(DETACH_CAPACITY)?;
        self.focus_member(id)?;
        self.commit_tab_model(space, id, proposed);
        Ok(())
    }

    fn float_tab(&mut self, id: u32, area: NSRect) -> Result<(), &'static str> {
        let window = &self.windows[&id];
        let space = window.space;
        let (position, size) =
            ax::geometry(window.element.0).ok_or("No se pudo leer la geometría de la ventana.")?;
        let from = NSRect::new(position, size);
        let target = crate::tiling::floating_frame(from, area)
            .ok_or("La geometría de la ventana o pantalla no es válida.")?;
        let mut desktop = self
            .desktops
            .get(&space)
            .cloned()
            .ok_or("Este escritorio ya no está visible.")?;
        desktop.remove(id);
        self.focus_member(id)?;
        let window = self.windows.get_mut(&id).unwrap();
        window.original = target;
        window.original_area = area;
        self.floating.insert(id);
        self.inactive_tabs.remove(&id);
        self.minimum_sizes.remove(&id);
        self.commit_tab_model(space, id, desktop);
        self.transition_from(id, from, target);
        Ok(())
    }

    fn move_tab_to_group(
        &mut self,
        context: &TabContext,
        target: u32,
        area: NSRect,
    ) -> Result<(), &'static str> {
        let space = context.workspace;
        let desktop = self
            .desktops
            .get(&space)
            .ok_or("Este escritorio ya no está visible.")?;
        // The menu stores the leaf it saw; follow the strip if its visible tab
        // changed while the menu was tracking.
        let leaf = desktop
            .groups()
            .iter()
            .find(|group| group.members.contains(&target))
            .and_then(Group::leaf)
            .ok_or("El grupo de destino ya no existe.")?;
        if desktop
            .tab_strip(leaf)
            .is_some_and(|(members, _)| members.contains(&context.id))
        {
            return Err("La pestaña ya pertenece a ese grupo.");
        }
        let proposed = tab_group_proposal(desktop, &self.minimum_sizes, context.id, leaf, area)?;
        self.focus_member(context.id)?;
        self.commit_tab_model(space, context.id, proposed);
        Ok(())
    }

    /// Other strips of the same Space, with the concrete reason a destination
    /// is disabled. Titles are read only here, i.e. on an explicit menu open.
    pub fn tab_group_targets(&self, context: &TabContext) -> Vec<TabGroupTarget> {
        let Some(desktop) = self.desktops.get(&context.workspace) else {
            return Vec::new();
        };
        let Some((_, area)) = self
            .active_areas
            .iter()
            .find(|(key, _)| *key == context.workspace)
        else {
            return Vec::new();
        };
        desktop
            .groups()
            .iter()
            .filter(|group| group.members.len() > 1 && !group.members.contains(&context.id))
            .filter_map(|group| {
                let leaf = group.leaf()?;
                Some(TabGroupTarget {
                    id: leaf,
                    title: self
                        .windows
                        .get(&leaf)
                        .map(|window| window_label(window.element.0 as Obj, window.pid))
                        .unwrap_or_else(|| "Grupo no disponible".into()),
                    unavailable: group_rejection(
                        desktop,
                        &self.minimum_sizes,
                        context.id,
                        leaf,
                        *area,
                    ),
                })
            })
            .collect()
    }

    fn focus_member(&self, id: u32) -> Result<(), &'static str> {
        let window = self
            .windows
            .get(&id)
            .ok_or("La ventana ya no está gestionada.")?;
        if ax::window_id(window.element.0) != Some(id) || !ax::tileable(window.element.0) {
            return Err("La identidad de la ventana cambió; vuelve a intentarlo.");
        }
        if ax::focus_is_protected(window.pid) {
            return Err("Un cuadro de diálogo de la aplicación tiene el foco.");
        }
        if !ax::focus(window.element.0, window.pid) {
            return Err("La aplicación no ha aceptado activar esta ventana.");
        }
        Ok(())
    }

    /// Move the visible tab of the front window's own strip, wrapping around.
    pub fn cycle_tab(&mut self, forward: bool) -> Result<(), &'static str> {
        self.tab_identity_guards()?;
        let (_, id) = self.front_tab_identity()?;
        self.current_tab_context(id)?;
        let space = self.windows[&id].space;
        let desktop = self
            .desktops
            .get(&space)
            .ok_or("Este escritorio ya no está visible.")?;
        let (members, _) = desktop
            .tab_strip(id)
            .ok_or("Esta ventana no forma un grupo de pestañas.")?;
        let active = members
            .iter()
            .position(|member| *member == id)
            .ok_or("El grupo de pestañas cambió.")?;
        let next = tab_cycle(members, active, forward).ok_or("El grupo de pestañas cambió.")?;
        self.focus_member(next)?;
        self.select_tab_model(next);
        Ok(())
    }

    /// Detach the visible tab of the front window's strip.
    pub fn detach_active_tab(&mut self) -> Result<(), &'static str> {
        self.tab_identity_guards()?;
        let (_, id) = self.front_tab_identity()?;
        let (_, area) = self.current_tab_context(id)?;
        self.detach_tab(id, area)
    }

    /// The real front window of the frontmost application, never the last AX
    /// focus the manager happened to cache.
    fn front_tab_identity(&self) -> Result<(i32, u32), &'static str> {
        unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            if workspace.is_null() {
                return Err("Sin sesión de espacio de trabajo.");
            }
            let app: Obj = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return Err("Ninguna aplicación está al frente.");
            }
            let pid: i32 = msg_send![app, processIdentifier];
            let id = ws::front_window(pid)
                .map(|window| window.id)
                .ok_or("La aplicación del frente no tiene ventana normal.")?;
            if !self.windows.contains_key(&id) {
                return Err("Esa ventana no está gestionada por Mosaico.");
            }
            if self.windows[&id].pid != pid {
                return Err("El identificador de ventana cambió de aplicación.");
            }
            Ok((pid, id))
        }
    }
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
                "Workspace {key:?} layout={:?} grid={:?} order={:?} strips={:?}\n",
                desktop.layout,
                desktop.grid,
                desktop.order,
                desktop
                    .groups()
                    .iter()
                    .map(|group| (group.members.clone(), group.active))
                    .collect::<Vec<_>>()
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
            let before = self.desktop_baseline.get_or_insert_with(|| {
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
                .extend(after.difference(before).copied());
        }
        if mode == Mode::Desktop {
            self.desktop_resume_until = None;
            if let Some(snapshot) = &snapshot {
                self.desktop_floating.retain(|key| snapshot.contains(key));
            }
            if self.desktop_baseline.is_none() {
                self.desktop_baseline = snapshot;
            }
        }
        let was_paused = self.presentation != Mode::Normal;
        self.presentation = mode;
        if !was_paused {
            self.cancel_drag(); // Cancels writes/previews, preserving trees and originals.
        } else if mode == Mode::Normal {
            if self.desktop_baseline.is_some() {
                self.desktop_resume_until = Some(now + Duration::from_secs(2));
            }
            // Let macOS put the parked windows back before consulting geometry.
            self.space_changed_with_visible_at(now, &[]);
        }
    }
    fn finish_desktop_reentry(&mut self, candidates: &[(i32, u32, NSRect)], now: Instant) {
        // Opening an app can dismiss Show Desktop before its window is published.
        // Capture the first settled scan, but never extend this to a later manual
        // resume after sleep, disabled tiling or loss of accessibility permission.
        let within_resume = self
            .desktop_resume_until
            .take()
            .is_some_and(|until| now <= until);
        if let Some(before) = self.desktop_baseline.take()
            && within_resume
        {
            self.desktop_floating.extend(
                candidates
                    .iter()
                    .map(|(pid, id, _)| (*pid, *id))
                    .filter(|key| !before.contains(key)),
            );
        }
    }
    fn apply_desktop_float(&mut self, pid: i32, id: u32) {
        if !self.desktop_floating.remove(&(pid, id)) {
            return;
        }
        // Also applies to a window already tracked before the deferred notification.
        self.floating.insert(id);
        self.inactive_tabs.remove(&id);
        self.cancel_window_placement(id);
        if let Some(window) = self.windows.get(&id)
            && let Some(desktop) = self.desktops.get_mut(&window.space)
        {
            desktop.remove(id);
        }
        self.dirty = true;
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
        self.strips.clear(); // Native ownership/coordinates may have changed.
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
    /// Presentation only: never wait for AX reconciliation to restore a cached
    /// native strip. The UI calls this only while tiling is enabled and unpaused.
    pub fn resume_space_strips(&mut self) {
        if !self.strips.is_empty() {
            self.restore_space_strips(&spaces::visible_desktops());
        }
    }
    fn restore_space_strips(&mut self, visible: &[Workspace]) {
        if self.presentation != crate::desktop_mode::Mode::Normal || crate::desktop_mode::active() {
            return;
        }
        for strip in &self.strips {
            if matches!(strip.binding, StripSpaceBinding::Confirmed)
                && visible.contains(&strip.space)
                && self.desktops.get(&strip.space).is_some_and(|desktop| {
                    desktop.groups().iter().any(|group| {
                        group.members.len() > 1
                            && group.members == strip.members
                            && group.active == strip.active
                    })
                })
                && strip
                    .area
                    .is_some_and(|saved| area(strip.space.0) == Some(saved))
            {
                strip.show();
            }
        }
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
        self.tab_drag = None;
        self.gesture_candidates.clear();
        self.hide_preview_panels();
        // Native per-Space panels travel with the desktop. Keep their controls
        // alive through the quiet/reentry barrier; fallback overlays cannot.
        for strip in &mut self.strips {
            strip.clear_marker();
            strip.confirm_space();
            if !matches!(strip.binding, StripSpaceBinding::Confirmed) {
                unsafe {
                    let _: () = msg_send![&*strip.panel, orderOut:ptr::null::<AnyObject>()];
                }
            }
        }
        for badge in &self.badges {
            unsafe {
                let _: () = msg_send![&*badge.panel, orderOut:ptr::null::<AnyObject>()];
            }
        }
        self.invalidate();
        // During the native slide both visibility and global bounds are transient.
        // Retain the trees without sampling/reconciling or writing any geometry.
        // One slide is ~300 ms; the reentry barrier still needs two agreeing
        // samples, so this floor only skips the pre-slide list.
        let until = now + Duration::from_millis(400);
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
        self.cancel_tab_drag();
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
        let identities = ws::all_window_ids();
        for (id, w) in &self.windows {
            if !self.inactive_tabs.contains(id)
                && !self.floating.contains(id)
                && !ax::flag(w.element.0, "AXFullScreen")
            {
                if let Some(target) = w.restoration_target(
                    *id,
                    identities.as_ref(),
                    spaces::window_space(*id),
                    ax::window_id(w.element.0),
                    area(w.space.0),
                ) {
                    ax::set_frame(w.element.0, target);
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
        self.hide_preview_panels();
        for strip in &self.strips {
            unsafe {
                let visible: bool = msg_send![&*strip.panel, isVisible];
                if visible {
                    let _: () = msg_send![&*strip.panel,orderOut:ptr::null::<AnyObject>()];
                }
            }
        }
        for badge in &self.badges {
            unsafe {
                let visible: bool = msg_send![&*badge.panel, isVisible];
                if visible {
                    let _: () = msg_send![&*badge.panel,orderOut:ptr::null::<AnyObject>()];
                }
            }
        }
    }
    /// Only the destination rectangles; strips and badges keep their own trim.
    fn hide_preview_panels(&self) {
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
    /// One rounded strip per tabbed group, above its slot: app icon plus title
    /// per tab, with the visible tab lit. Panels are reused; content is only
    /// rebuilt when the strip's identity or geometry actually changed.
    fn draw_strips(
        &mut self,
        space: Workspace,
        desktop: &Desktop,
        frames: &[(u32, NSRect)],
        primary: f64,
        start: usize,
        centered: bool,
    ) -> usize {
        let mut used = start;
        let workspace_area = self
            .active_areas
            .iter()
            .find_map(|(key, area)| (*key == space).then_some(*area));
        let groups: &[Group] = desktop.groups();
        for group in groups {
            if group.members.len() < 2 {
                continue;
            }
            let Some(leaf) = group.leaf() else { continue };
            let Some((_, slot)) = frames.iter().find(|(id, _)| *id == leaf) else {
                continue;
            };
            // Active panels occupy the prefix. Never reuse another Space's panel
            // just because its group happened to have the same ordinal.
            let existing = self
                .strips
                .iter()
                .enumerate()
                .skip(used)
                .find_map(|(index, strip)| {
                    (strip.space == space
                        && strip.members.iter().any(|id| group.members.contains(id)))
                    .then_some(index)
                });
            if let Some(index) = existing {
                self.strips.swap(used, index);
                self.strips[used].area = workspace_area;
            }
            // During a native gesture only geometry changes. Reuse captions and
            // icons instead of synchronous AX title reads at the gesture cadence.
            // The first reconciliation after release refreshes them normally.
            if existing.is_some()
                && self.pressed
                && let Some(strip) = self.strips.get_mut(used)
                && strip.members == group.members
                && strip.active == group.active
            {
                let total = strip_width(strip.signature.iter().map(|(name, _)| tab_width(name)));
                let panel = tab_bar_frame(*slot, total, centered);
                if strip.built.size == panel.size {
                    strip.show_at(panel, primary);
                    used += 1;
                    continue;
                }
            }
            let mut tabs: Vec<(String, Option<Retained<AnyObject>>)> =
                Vec::with_capacity(group.members.len());
            for id in &group.members {
                let (pid, element) = self
                    .windows
                    .get(id)
                    .map(|w| (w.pid, w.element.0))
                    .unwrap_or((0, ptr::null()));
                tabs.push((window_label(element as Obj, pid), app_icon(pid)));
            }
            let total = strip_width(tabs.iter().map(|(name, _)| tab_width(name)));
            let panel = tab_bar_frame(*slot, total, centered);
            match existing.map(|_| &mut self.strips[used]) {
                Some(strip) => {
                    let same = strip.signature.len() == tabs.len()
                        && strip
                            .signature
                            .iter()
                            .zip(tabs.iter())
                            .all(|((n, i), (m, j))| {
                                n == m
                                    && match (i, j) {
                                        (None, None) => true,
                                        (Some(a), Some(b)) => {
                                            Retained::as_ptr(a) == Retained::as_ptr(b)
                                        }
                                        _ => false,
                                    }
                            });
                    if !same
                        || strip.members != group.members
                        || strip.active != group.active
                        || strip.built.size != panel.size
                    {
                        let root = unsafe {
                            strip_content(
                                &tabs,
                                &group.members,
                                group.active,
                                panel.size.width,
                                self.tab_target.as_deref(),
                                self.accent,
                            )
                        };
                        unsafe {
                            let _: () = msg_send![&*strip.panel,setContentView:&*root];
                            sync_strip_hover(&root);
                        }
                        strip.signature = tabs;
                        strip.members.clone_from(&group.members);
                        strip.active = group.active;
                        strip.built = panel;
                    }
                    strip.show_at(panel, primary);
                }
                None => {
                    let panel_obj = unsafe { new_overlay(panel, primary) };
                    // No CanJoinAllSpaces or MoveToActiveSpace: AppKit owns the
                    // slide, rather than hiding/redrawing a sticky overlay.
                    unsafe {
                        let _: () =
                            msg_send![&panel_obj, setCollectionBehavior:((1usize << 3) | (1 << 6))];
                        let _: () = msg_send![&panel_obj, setHidesOnDeactivate:false];
                    }
                    let root = unsafe {
                        strip_content(
                            &tabs,
                            &group.members,
                            group.active,
                            panel.size.width,
                            self.tab_target.as_deref(),
                            self.accent,
                        )
                    };
                    unsafe {
                        let _: () = msg_send![&panel_obj,setIgnoresMouseEvents:false];
                        // The compositor supplies the strip's compact native shadow.
                        let _: () = msg_send![&panel_obj,setHasShadow:true];
                        let _: () = msg_send![&panel_obj,setContentView:&*root];
                        let _: () = msg_send![&panel_obj, orderFrontRegardless];
                        sync_strip_hover(&root);
                    }
                    let binding =
                        StripSpaceBinding::Pending(Instant::now() + Duration::from_secs(1));
                    self.strips.insert(
                        used,
                        TabStrip {
                            panel: panel_obj,
                            space,
                            area: workspace_area,
                            binding,
                            marker: None,
                            signature: tabs,
                            members: group.members.clone(),
                            active: group.active,
                            built: panel,
                            frame: panel,
                        },
                    );
                }
            }
            used += 1;
        }
        used
    }
    fn show_badge(&mut self, hint: DropHint, primary: f64) {
        unsafe {
            if self.badges.is_empty() {
                let panel = new_overlay(hint.frame, primary);
                let (root, plates) = badge_content(hint);
                let _: () = msg_send![&panel, setContentView:&*root];
                self.badges.push(HintBadge {
                    panel,
                    hint,
                    plates,
                });
            }
            let badge = &mut self.badges[0];
            let rebuilt = badge.hint.kind != hint.kind
                || badge.hint.available != hint.available
                || badge.hint.frame.size != hint.frame.size;
            if badge.hint.frame != hint.frame {
                let _: () =
                    msg_send![&badge.panel, setFrame:appkit(hint.frame, primary), display:true];
            }
            if rebuilt {
                let (root, plates) = badge_content(hint);
                let _: () = msg_send![&badge.panel, setContentView:&*root];
                badge.plates = plates;
            } else if badge.hint.active != hint.active {
                paint_hint(&badge.plates, hint);
            }
            badge.hint = hint;
            let visible: bool = msg_send![&badge.panel, isVisible];
            if !visible {
                let _: () = msg_send![&badge.panel, orderFrontRegardless];
            }
        }
    }
    /// Retain live inactive groups, not anonymous spare panels.
    fn trim_overlays(&mut self, strips: usize, badges: usize) {
        let mut index = 0;
        self.strips.retain(|strip| {
            let used = index < strips;
            index += 1;
            used || (!self.active_areas.iter().any(|(key, _)| *key == strip.space)
                && self.desktops.get(&strip.space).is_some_and(|desktop| {
                    desktop
                        .groups()
                        .iter()
                        .any(|group| group.members.len() > 1 && group.members == strip.members)
                }))
        });
        for badge in self.badges.iter().skip(badges) {
            unsafe {
                let visible: bool = msg_send![&*badge.panel, isVisible];
                if visible {
                    let _: () = msg_send![&*badge.panel,orderOut:ptr::null::<AnyObject>()];
                }
            }
        }
        self.badges.truncate(badges);
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
                d.window_frames(destination_area, 10.)
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
            let Some(current) = area(w.space.0) else {
                return;
            };
            let Some((position, size)) = ax::geometry(w.element.0) else {
                return;
            };
            let from = NSRect::new(position, size);
            let Some(target) = crate::tiling::floating_frame(from, current) else {
                return;
            };
            ax::raise(w.element.0);
            if let Some(d) = self.desktops.get_mut(&w.space) {
                d.remove(id);
            }
            let w = self.windows.get_mut(&id).unwrap();
            w.original = target;
            w.original_area = current;
            self.floating.insert(id);
            self.applied.remove(&id);
            self.minimum_sizes.remove(&id);
            self.transition_from(id, from, target);
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
        // An incoming independent floating window must not inherit a tiled slot.
        // Inactive tabs instead follow the currently selected tab, including an
        // explicit float-to-tile shortcut since they were last selected.
        let incoming_float = self.floating.contains(&new) && !self.inactive_tabs.contains(&new);
        let floating = self.floating.contains(&old) || incoming_float;
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
        if floating {
            if let Some(desktop) = self.desktops.get_mut(&space) {
                desktop.remove(old);
            }
            self.floating.insert(old);
            self.floating.insert(new);
            for id in [old, new] {
                self.cancel_window_placement(id);
                self.minimum_sizes.remove(&id);
            }
        } else {
            if let Some(target) = self.applied.remove(&old) {
                self.applied.insert(new, target);
            }
            if let Some(limit) = self.constrained.remove(&old) {
                self.constrained.insert(new, limit);
            }
            if let Some(minimum) = self.minimum_sizes.remove(&old) {
                self.minimum_sizes.insert(new, minimum);
            }
            self.floating.remove(&new);
        }
        if !incoming_float && let Some(destination) = self.windows.get_mut(&new) {
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
    /// Group members share their slot even when the active member is already
    /// there. Reuse bounded verification and learned native size constraints.
    fn place_hidden_tab(&mut self, id: u32, frame: NSRect) {
        let Some(w) = self.windows.get(&id) else {
            return;
        };
        if ax::window_id(w.element.0) != Some(id)
            || !self.active_areas.iter().any(|(key, _)| *key == w.space)
            || spaces::window_space(id) != Some(w.space.1)
        {
            return;
        }
        let actual = ax::geometry(w.element.0).map(|(p, s)| NSRect::new(p, s));
        if !self.needs_placement(id, frame, actual, self.settling_space()) {
            return;
        }
        self.transitions.remove(&id);
        self.constrained.remove(&id);
        let element = w.element.0;
        let mut verification = Verification::new(frame);
        verification.accepted = ax::set_frame(element, frame);
        self.verifications.insert(id, verification);
        self.applied.insert(id, frame);
    }
    #[cfg(test)]
    fn place_drag(&mut self, id: u32, space: Workspace, target: u32, zone: Zone) {
        if self.can_tile_drag(id, space) {
            if let Some(desktop) = self.desktops.get_mut(&space) {
                desktop.place(id, target, zone);
            }
        }
    }
    fn transition_from(&mut self, id: u32, from: NSRect, to: NSRect) {
        // A retry for an older destination must not fight the new animation.
        self.verifications.remove(&id);
        if near(from, to) {
            self.transitions.remove(&id);
            return;
        }
        if self.animated == Some(false) {
            // One write per window: the eased batch is off, so the destination
            // commits directly and is verified like any other final frame.
            self.transitions.remove(&id);
            if let Some(w) = self.windows.get(&id) {
                let element = w.element.0;
                let mut verification = Verification::new(to);
                verification.accepted = ax::set_frame(element, to);
                self.verifications.insert(id, verification);
            }
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
                    .window_frames(drag.area, 10.)
                    .into_iter()
                    .map(|(_, r)| r)
                    .collect(),
            );
        }
        let space = drag.space;
        self.cancel_desktop_placements(&layout);
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
        self.cancel_desktop_placements(&context.desktop);
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
        let frames = desktop.window_frames(area, 10.);
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
        self.cancel_desktop_placements(&desktop);
        self.desktops.insert(space, desktop);
        self.invalidate();
        true
    }
    pub fn tick_interval(&self) -> f64 {
        // 0.08 while a bounded operation is in flight; 0.25 when the model is
        // idle. The internal 500 ms scan gate is unchanged, so the coarser idle
        // cadence cannot postpone a scan, and every notification, gesture or
        // dirty flag still resumes a fast timer on the very tick it is set.
        let animated = self.animated.unwrap_or(true);
        if self.space_reentry.is_some() && self.drag.is_none()
            || self.space_quiet_until.is_some()
            || self.dirty
            || !self.verifications.is_empty()
            || self.tab_drag.is_some()
            || (!animated && !self.transitions.is_empty())
        {
            0.08
        } else if animated && !self.transitions.is_empty()
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
        self.cancel_tab_drag();
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
        // Read once per tick: absent keeps the eased batch, 0 commits directly.
        self.animated = Some(pref_or(defaults, "tile.animate", 1) != 0);
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
        let group_limit = max_group_tabs(defaults);
        for (key, _) in &active {
            let desktop = self.desktops.entry(*key).or_insert_with(|| {
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
            desktop.max_group_tabs = group_limit;
        }
        let down = place_windows && down & 1 != 0;
        let Some(p) = input.map(|(_, p)| p).or_else(pointer) else {
            self.cancel_drag();
            return;
        };
        if self.tab_drag.is_some() {
            if crate::focus::secure_input() {
                self.cancel_tab_drag();
            } else {
                self.observe_tab_pointer(down, Instant::now());
            }
            return;
        }
        // Native strip buttons own these clicks, not the titlebar below them.
        if down
            && self.drag.is_none()
            && self
                .strips
                .iter()
                .any(|strip| contains(strip.frame, p) && strip.accepts_input())
        {
            self.pressed = true;
            return;
        }
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
            // A grouped sibling may have been raised outside AMSpace since the
            // last scan. Adopt that real hit before deciding if it owns a slot.
            if let Some((pid, id)) = ws::window_at(p)
                && self.windows.get(&id).is_some_and(|w| w.pid == pid)
            {
                self.select_tab_model(id);
            }
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
            let mut hint: Option<DropHint> = None;
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
                                pref_or(defaults, "tile.groups", 1) != 0,
                            )
                            .or_else(|| {
                                (frames.is_empty() && contains(*area, p))
                                    .then_some(DropIntent::Screen(Zone::Left))
                            })?;
                            let proposed = drag.previews.preview(context, intent);
                            if proposed.is_none() {
                                hint = drag.previews.hint(
                                    context,
                                    intent,
                                    pref_or(defaults, "tile.groups", 1) != 0,
                                    None,
                                );
                            }
                            proposed.map(|_| (*key, intent))
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
                                if hint.is_none() {
                                    hint = drag.previews.hint(
                                        DropContext {
                                            workspace: key,
                                            id: drag.id,
                                            area: *area,
                                            desktop: d,
                                            minimums: &self.minimum_sizes,
                                        },
                                        intent,
                                        pref_or(defaults, "tile.groups", 1) != 0,
                                        preview,
                                    );
                                } else {
                                    // An unavailable hovered zone must not advertise
                                    // the previous destination during hysteresis.
                                    preview = None;
                                }
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
                let primary = primary_height();
                // One owner updates overlay visibility once per tick. Resize
                // preference changes cannot hide a move destination afterwards.
                // The alignment is read once per tick alongside the other prefs.
                let centered = pref(defaults, "tile.groups.centered") == 1;
                let mut strips = 0;
                for (key, area) in active.iter().map(|(k, r)| (*k, *r)) {
                    if let Some(desktop) = self
                        .desktops
                        .get(&key)
                        .filter(|desktop| !desktop.groups().is_empty())
                        .cloned()
                    {
                        let frames = desktop.window_frames(area, 10.);
                        strips =
                            self.draw_strips(key, &desktop, &frames, primary, strips, centered);
                    }
                }
                if let Some(frames) = resize_preview {
                    self.show_previews(&frames);
                } else if let Some(r) = preview {
                    self.show_preview(r);
                } else {
                    self.hide_preview_panels();
                }
                let badges = if let Some(hint) = hint {
                    self.show_badge(hint, primary);
                    1
                } else {
                    0
                };
                self.trim_overlays(strips, badges);
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
                        } else {
                            // A rejected gesture reports itself once; the next
                            // tick consumes it, so no frame repeats the notice.
                            self.interaction_error = Some(
                                drop_rejection(
                                    &d,
                                    &self.minimum_sizes,
                                    drop.id,
                                    drop.intent,
                                    *area,
                                )
                                .to_string(),
                            );
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
        let scan_due = self.dirty
            || self
                .last_scan
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(scan_interval));
        if !scan_due && self.transitions.is_empty() && self.verifications.is_empty() {
            return;
        }
        // One coherent compositor snapshot serves verification, animation and
        // membership for this tick: the same data three times over, instead of
        // three lists that can disagree during a native slide.
        let Some(candidates) = ws::try_tile_windows() else {
            return;
        };
        self.finish_desktop_reentry(&candidates, Instant::now());
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
        if !scan_due {
            return;
        }
        self.last_visible_windows =
            Some(candidates.iter().map(|(pid, id, _)| (*pid, *id)).collect());
        self.last_scan = Some(Instant::now());
        let focused = std::cell::LazyCell::new(|| unsafe {
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![app, processIdentifier];
            ws::front_window(pid)
                .map(|w| w.id)
                .or_else(|| ax::focused_id(pid))
        });
        let visible_ids: HashSet<_> = candidates.iter().map(|(_, id, _)| *id).collect();
        let all_ids = if self.windows.keys().any(|id| !visible_ids.contains(id)) {
            ws::all_window_ids()
        } else {
            None
        };
        // Enumerate each app once, including a failed read. Repeating AXWindows
        // for every newly discovered sibling makes cold start quadratic in AX
        // ID lookups and multiplies timeouts in an unresponsive application.
        let mut app_windows = HashMap::new();
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
            let Some(mut element) =
                self.windows
                    .get(&id)
                    .map(|w| w.element.clone())
                    .or_else(|| {
                        app_windows
                            .entry(pid)
                            .or_insert_with(|| ax::application_windows(pid))
                            .as_ref()?
                            .get(&id)
                            .cloned()
                    })
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
                && let Some(fresh) = app_windows
                    .entry(pid)
                    .or_insert_with(|| ax::application_windows(pid))
                    .as_ref()
                    .and_then(|windows| windows.get(&id))
                    .cloned()
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
            self.apply_desktop_float(pid, id);
            if let std::collections::hash_map::Entry::Vacant(entry) = self.windows.entry(id) {
                if !self.floating.contains(&id) && crate::tile_rules::floats(defaults, pid) {
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
                                && !self.inactive_tabs.contains(old)
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
                        if window_space_cached(old, &mut space_of)
                            .is_some_and(|native| native != space.1)
                        {
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
                            let ax_windows = app_windows
                                .entry(pid)
                                .or_insert_with(|| ax::application_windows(pid));
                            let previous = self
                                .constrained
                                .get(&old)
                                .map(|(_, actual)| *actual)
                                .or_else(|| self.applied.get(&old).copied())
                                .or_else(|| ws::bounds(old));
                            if ax_windows
                                .as_ref()
                                .is_some_and(|windows| !windows.contains_key(&old))
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
            self.inactive_tabs.remove(&id); // A detached tab can become an ordinary window.
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
                && !self
                    .desktops
                    .get(&w.space)
                    .is_some_and(|d| d.is_hidden_tab(*id))
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
        let primary = primary_height();
        // Absent or invalid is 0, i.e. the left inset; 1 is centered.
        let centered = pref(defaults, "tile.groups.centered") == 1;
        let mut strips_drawn = 0usize;
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
            // A native tab switch changes the visible id without a model event:
            // adopt it before geometry so the slot keeps a live leaf.
            d.sync_tabs(ids);
            if let Some(id) = *focused
                && ids.contains(&id)
            {
                d.select_tab(id);
            }
            let insertion_focus = if d.needs_insertion_focus(ids) {
                *focused
            } else {
                None
            };
            d.reconcile_at(ids, r, 10., insertion_focus);
            d.set_minimum_sizes(&self.minimum_sizes);
            d.fit(r, 10.);
            let frames = d.window_frames(r, 10.);
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
            let hidden = hidden_tab_frames(d, &frames);
            if !d.groups().is_empty() {
                let snapshot = d.clone();
                strips_drawn =
                    self.draw_strips(space, &snapshot, &frames, primary, strips_drawn, centered);
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
            for (id, frame) in hidden {
                if !place_windows || self.floating.contains(&id) || uncertain.contains(&id) {
                    continue;
                }
                self.place_hidden_tab(id, frame);
            }
        }
        self.trim_overlays(strips_drawn, 0);
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
        if self.tab_drag.as_ref().is_some_and(|drag| {
            drag.baseline.order.contains(&id) || drag.baseline.tab_role(id).is_some()
        }) {
            self.cancel_tab_drag();
        }
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
    fn cancel_desktop_placements(&mut self, desktop: &Desktop) {
        for id in &desktop.order {
            self.cancel_window_placement(*id);
        }
        for group in desktop.groups() {
            for id in group.members.iter().filter(|id| Some(**id) != group.leaf()) {
                self.cancel_window_placement(*id);
            }
        }
    }
    pub fn restore_space(&mut self, space: u64) {
        if self.presentation_blocks_input() {
            self.restore_spaces_after_presentation.insert(space);
            return;
        }
        let identities = ws::all_window_ids();
        for (id, w) in &self.windows {
            if w.space.1 == space
                && !self.inactive_tabs.contains(id)
                && !self.floating.contains(id)
                && !ax::flag(w.element.0, "AXFullScreen")
            {
                if let Some(target) = w.restoration_target(
                    *id,
                    identities.as_ref(),
                    spaces::window_space(*id),
                    ax::window_id(w.element.0),
                    area(w.space.0),
                ) {
                    ax::set_frame(w.element.0, target);
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

/// Everything a tab menu needs to revalidate its target: the compositor id,
/// its owning process and the Space/display it was listed on.
#[derive(Clone, PartialEq)]
pub struct TabContext {
    pub id: u32,
    pub pid: i32,
    pub workspace: Workspace,
}

/// The native requests a tab strip offers.
#[derive(Clone, Copy)]
pub enum TabAction {
    Detach,
    Float,
    MoveToGroup(u32),
    Close,
}

/// One receiving strip in the menu. `unavailable` is the concrete reason the
/// item is disabled, so the menu never has to guess.
pub struct TabGroupTarget {
    pub id: u32,
    pub title: String,
    pub unavailable: Option<&'static str>,
}

const DETACH_CAPACITY: &str =
    "No hay espacio suficiente para separar esta pestaña respetando los tamaños mínimos.";
const MINIMUM_CAPACITY: &str =
    "La distribución no permite respetar los tamaños mínimos de estas ventanas.";
const GROUP_GONE: &str = "El grupo ya no está en este escritorio.";
const GROUP_FULL: &str = "Grupo lleno: alcanza su máximo de pestañas.";
const GROUP_LIMIT: &str = "El máximo de pestañas por grupo es demasiado bajo.";
const SAME_TAB: &str = "Es la propia pestaña.";

/// Why `id` cannot join the strip led by `leaf` right now, or `None` when the
/// model accepts the join. Only literal, bounded reasons, so a disabled item
/// explains itself without a per-frame rebuild note. The minimum check reuses
/// the very solver the drag gesture commits with.
fn group_rejection(
    desktop: &Desktop,
    minimums: &HashMap<u32, objc2_foundation::NSSize>,
    id: u32,
    leaf: u32,
    area: NSRect,
) -> Option<&'static str> {
    tab_group_proposal(desktop, minimums, id, leaf, area).err()
}

fn tab_group_proposal(
    desktop: &Desktop,
    minimums: &HashMap<u32, objc2_foundation::NSSize>,
    id: u32,
    leaf: u32,
    area: NSRect,
) -> Result<Desktop, &'static str> {
    if leaf == id {
        return Err(SAME_TAB);
    }
    if !desktop.order.contains(&leaf) {
        return Err(GROUP_GONE);
    }
    if desktop.max_group_tabs < 2 {
        return Err(GROUP_LIMIT);
    }
    let members = desktop
        .tab_strip(leaf)
        .map(|(members, _)| members)
        .unwrap_or(&[]);
    if !members.contains(&id) && members.len().max(1) >= desktop.max_group_tabs {
        return Err(GROUP_FULL);
    }
    arrival_proposal(
        desktop,
        id,
        Some(DropIntent::Group(leaf)),
        area,
        Some(minimums),
    )
    .ok_or(MINIMUM_CAPACITY)
}

/// Why the released gesture did not change the tree. The same solver answered
/// `None`, so the message names the actual limit instead of a generic failure.
fn drop_rejection(
    desktop: &Desktop,
    minimums: &HashMap<u32, objc2_foundation::NSSize>,
    id: u32,
    intent: Option<DropIntent>,
    area: NSRect,
) -> &'static str {
    match intent {
        Some(DropIntent::Group(leaf)) => {
            group_rejection(desktop, minimums, id, leaf, area).unwrap_or(MINIMUM_CAPACITY)
        }
        Some(_) => MINIMUM_CAPACITY,
        None => DETACH_CAPACITY,
    }
}

/// Wrap inside one strip: `active` indexes `members`, so forward advances and
/// backward steps with a single modular addition.
fn tab_cycle(members: &[u32], active: usize, forward: bool) -> Option<u32> {
    let count = members.len();
    if count < 2 || active >= count {
        return None;
    }
    let next = if forward {
        (active + 1) % count
    } else {
        (active + count - 1) % count
    };
    members.get(next).copied()
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
/// Height of the primary screen, the reference for AppKit's y-flip.
fn primary_height() -> f64 {
    unsafe {
        let screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![screens, firstObject];
        if first.is_null() {
            return 0.;
        }
        let frame: NSRect = msg_send![first, frame];
        frame.size.height
    }
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

/// Strip geometry inside a logical slot: left-aligned with an 8 pt inset by
/// default, symmetric when `centered`. Both edges stay inside the slot, so a
/// narrow window clamps the strip instead of overflowing it. Left alignment
/// keeps a stable origin as the tab content changes.
fn tab_bar_frame(window: NSRect, content_width: f64, centered: bool) -> NSRect {
    let width = content_width.min((window.size.width - 12.).max(1.));
    let inset = if centered {
        (window.size.width - width) / 2.
    } else {
        8f64.min((window.size.width - width).max(0.))
    };
    rect(
        window.origin.x + inset,
        window.origin.y - TAB_BAR_HEIGHT - TAB_BAR_GAP,
        width,
        TAB_BAR_HEIGHT,
    )
}

/// Convert global AX (top-left) coordinates to AppKit's bottom-left system.
fn appkit(r: NSRect, primary_height: f64) -> NSRect {
    rect(
        r.origin.x,
        primary_height - r.origin.y - r.size.height,
        r.size.width,
        r.size.height,
    )
}

unsafe fn new_overlay(frame: NSRect, primary_height: f64) -> Retained<AnyObject> {
    unsafe {
        let panel: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
        let panel: Retained<AnyObject> = msg_send![panel,initWithContentRect:appkit(frame, primary_height),styleMask:128usize,backing:2usize,defer:false];
        let _: () = msg_send![&panel,setReleasedWhenClosed:false];
        let _: () = msg_send![&panel,setOpaque:false];
        let clear: Obj = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![&panel,setBackgroundColor:clear];
        let _: () = msg_send![&panel,setIgnoresMouseEvents:true];
        let _: () = msg_send![&panel,setHasShadow:false];
        let _: () = msg_send![&panel,setAnimationBehavior:2isize];
        let _: () = msg_send![&panel,setLevel:3isize];
        let _: () = msg_send![&panel,setCollectionBehavior:((1usize<<0)|(1<<3)|(1<<6)|(1<<8))];
        panel
    }
}

/// A dark, slightly translucent rounded card hosting the tab captions.
unsafe fn translucent_card(frame: NSRect) -> Retained<AnyObject> {
    unsafe {
        let root = alloc_view(class!(NSVisualEffectView), frame);
        let _: () = msg_send![&root,setMaterial:13isize];
        let _: () = msg_send![&root,setBlendingMode:0isize];
        let _: () = msg_send![&root,setState:1isize];
        let _: () = msg_send![&root,setWantsLayer:true];
        let layer: Obj = msg_send![&root, layer];
        let _: () = msg_send![layer,setCornerRadius:9f64];
        let _: () = msg_send![layer,setMasksToBounds:true];
        root
    }
}

unsafe fn rounded_plate(parent: Obj, frame: NSRect, rgb: [f64; 3], alpha: f64, radius: f64) -> Obj {
    unsafe {
        let view = alloc_view(class!(NSView), frame);
        let _: () = msg_send![&view,setWantsLayer:true];
        let layer: Obj = msg_send![&view, layer];
        let c = color(rgb[0], rgb[1], rgb[2], alpha);
        let cg: *const crate::native::CgColor = msg_send![c, CGColor];
        let _: () = msg_send![layer,setBackgroundColor:cg];
        let _: () = msg_send![layer,setCornerRadius:radius];
        let _: () = msg_send![parent,addSubview:&*view];
        Retained::as_ptr(&view) as Obj
    }
}

unsafe fn caption(parent: Obj, frame: NSRect, value: &str, bold: bool) {
    unsafe {
        let field: Allocated<AnyObject> = msg_send![class!(NSTextField), alloc];
        let field: Retained<AnyObject> = msg_send![field,initWithFrame:frame];
        let _: () = msg_send![&field,setStringValue:&*NSString::from_str(value)];
        let _: () = msg_send![&field,setEditable:false];
        let _: () = msg_send![&field,setBordered:false];
        let _: () = msg_send![&field,setDrawsBackground:false];
        let _: () = msg_send![&field,setUsesSingleLineMode:true];
        let _: () = msg_send![&field,setLineBreakMode:4isize];
        let font: Obj =
            msg_send![class!(NSFont),systemFontOfSize:11f64,weight:if bold {0.4f64} else {0.}];
        let _: () = msg_send![&field,setFont:font];
        let text = if bold {
            color(1., 1., 1., 1.)
        } else {
            color(0.96, 0.97, 1., 1.)
        };
        let _: () = msg_send![&field,setTextColor:text];
        let _: () = msg_send![&field,setAccessibilityLabel:&*NSString::from_str(value)];
        let _: () = msg_send![parent,addSubview:&*field];
    }
}

fn tab_button_class() -> &'static AnyClass {
    static CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| unsafe {
        let mut builder = ClassBuilder::new(c"AMSpaceTabButton", class!(NSButton)).unwrap();
        builder.add_method(
            sel!(mouseDown:),
            tab_button_down as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(rightMouseDown:),
            tab_button_right_down as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.register()
    });
    *CLASS
}
extern "C-unwind" fn tab_button_down(button: Obj, _: Sel, event: Obj) {
    unsafe {
        if !button.is_null() && !event.is_null() {
            let modifiers: usize = msg_send![event, modifierFlags];
            // A control-click is the keyboard route to the same context menu.
            if modifiers & (1 << 18) != 0 {
                request_tab_menu(button, event);
                return;
            }
        }
        let gestures: Obj = msg_send![button, gestureRecognizers];
        let gesture: Obj = msg_send![gestures, firstObject];
        if !gesture.is_null() {
            let ivar = (*gesture)
                .class()
                .instance_variable(c"recognizedPan")
                .unwrap();
            let recognized = ivar.load_ptr::<u8>(&*gesture);
            if recognized.read() != 0 {
                // Cancellation replays delayed mouse events on macOS. A drag
                // must not become a click; programmatic/AX presses still work.
                recognized.write(0);
                return;
            }
        }
        let _: () = msg_send![super(button, class!(NSButton)),mouseDown:event];
    }
}

/// Right-click on a tab: the parent owns the menu, its items and the dispatch.
extern "C-unwind" fn tab_button_right_down(button: Obj, _: Sel, event: Obj) {
    if button.is_null() || event.is_null() {
        return;
    }
    request_tab_menu(button, event);
}

fn request_tab_menu(button: Obj, event: Obj) {
    unsafe {
        let target: Obj = msg_send![button, target];
        if !target.is_null() {
            let accepts: bool = msg_send![target, respondsToSelector:sel!(showTileTabMenu:event:)];
            if accepts {
                let _: () = msg_send![target, showTileTabMenu:button, event:event];
            }
        }
    }
}

fn tab_pan_class() -> &'static AnyClass {
    static CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| unsafe {
        let mut builder =
            ClassBuilder::new(c"AMSpaceTabPan", class!(NSPanGestureRecognizer)).unwrap();
        builder.add_ivar::<NSPoint>(c"pressOrigin");
        builder.add_ivar::<u8>(c"recognizedPan");
        builder.add_method(
            sel!(mouseDown:),
            tab_pan_down as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(mouseDragged:),
            tab_pan_moved as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.register()
    });
    *CLASS
}
extern "C-unwind" fn tab_pan_down(gesture: Obj, _: Sel, event: Obj) {
    unsafe {
        let origin: NSPoint = msg_send![event, locationInWindow];
        let ivar = (*gesture)
            .class()
            .instance_variable(c"pressOrigin")
            .unwrap();
        ivar.load_ptr::<NSPoint>(&*gesture).write(origin);
        let recognized = (*gesture)
            .class()
            .instance_variable(c"recognizedPan")
            .unwrap();
        recognized.load_ptr::<u8>(&*gesture).write(0);
        let _: () = msg_send![super(gesture, class!(NSPanGestureRecognizer)),mouseDown:event];
    }
}
extern "C-unwind" fn tab_pan_moved(gesture: Obj, _: Sel, event: Obj) {
    unsafe {
        let state: isize = msg_send![gesture, state];
        if state == 0 {
            let ivar = (*gesture)
                .class()
                .instance_variable(c"pressOrigin")
                .unwrap();
            let origin = *ivar.load::<NSPoint>(&*gesture);
            let point: NSPoint = msg_send![event, locationInWindow];
            let dx = point.x - origin.x;
            let dy = point.y - origin.y;
            // NSPan recognizes a one-point movement on macOS. Keep small hand
            // jitter as a normal button click rather than detaching a window.
            if dx * dx + dy * dy < 36. {
                return;
            }
            let recognized = (*gesture)
                .class()
                .instance_variable(c"recognizedPan")
                .unwrap();
            recognized.load_ptr::<u8>(&*gesture).write(1);
        }
        let _: () = msg_send![super(gesture, class!(NSPanGestureRecognizer)),mouseDragged:event];
    }
}

/// One real button per member, clipped to the slot with horizontal scrolling.
unsafe fn strip_content(
    tabs: &[(String, Option<Retained<AnyObject>>)],
    members: &[u32],
    active: usize,
    width: f64,
    target: Option<&AnyObject>,
    accent: TabAccent,
) -> Retained<AnyObject> {
    unsafe {
        let total = strip_width(tabs.iter().map(|(name, _)| tab_width(name)));
        let root = translucent_card(rect(0., 0., total, TAB_BAR_HEIGHT));
        let appearance: Obj = msg_send![class!(NSAppearance), appearanceNamed:&*NSString::from_str("NSAppearanceNameVibrantDark")];
        let _: () = msg_send![&root,setAppearance:appearance];
        let parent = Retained::as_ptr(&root) as Obj;
        let mut x = 4.;
        let mut selected = NSRect::ZERO;
        for (index, ((name, icon), id)) in tabs.iter().zip(members).enumerate() {
            let width = tab_width(name);
            let plate = rect(x, 0., width, TAB_BAR_HEIGHT);
            let is_active = index == active;
            let active_plate: Obj = if is_active {
                selected = plate;
                // Opaque hue-preserving backing: the white caption and the
                // close glyph hold 4.5:1 over any wallpaper behind the card.
                rounded_plate(
                    parent,
                    rect(x, 3., width, TAB_BAR_HEIGHT - 6.),
                    accent.backing(),
                    1.,
                    6.,
                )
            } else {
                rounded_plate(
                    parent,
                    rect(x, 3., width, TAB_BAR_HEIGHT - 6.),
                    [1., 1., 1.],
                    0.05,
                    6.,
                );
                ptr::null_mut()
            };
            let inner = x + 6.;
            let mut text_x = inner;
            if let Some(icon) = icon {
                // A neutral compact backing keeps monochrome app icons legible
                // on the saturated active plate; originals keep their own colors.
                if is_active {
                    rounded_plate(
                        parent,
                        rect(inner - 1., 4., 14., 14.),
                        TAB_ICON_BACKING,
                        1.,
                        3.,
                    );
                }
                let image = alloc_view(class!(NSImageView), rect(inner, 5., 12., 12.));
                let _: () = msg_send![&image,setImage:&**icon];
                let _: () = msg_send![&image,setImageScaling:3isize];
                let _: () = msg_send![&root,addSubview:&*image];
                text_x = inner + 16.;
            }
            caption(
                parent,
                rect(text_x, 4., (x + width - 28. - text_x).max(8.), 14.),
                name,
                is_active,
            );
            // One container per tab owns the tracking area, so the glyph state
            // and both hit areas move together and never reflow on hover.
            let tab = alloc_view(tab_container_class(), plate);
            let button = alloc_view(
                tab_button_class(),
                rect(0., 0., width - 22., TAB_BAR_HEIGHT),
            );
            let _: () = msg_send![&button,setTransparent:true];
            let _: () = msg_send![&button,setTitle:&*NSString::from_str(name)];
            let _: () = msg_send![&button,setToolTip:&*NSString::from_str(name)];
            let _: () = msg_send![&button,setTag:*id as isize];
            let _: () = msg_send![&button,setTarget:target];
            let _: () = msg_send![&button,setAction:sel!(selectTileTab:)];
            let gesture: Allocated<AnyObject> = msg_send![tab_pan_class(), alloc];
            let gesture: Retained<AnyObject> =
                msg_send![gesture,initWithTarget:target,action:sel!(dragTileTab:)];
            let _: () = msg_send![&gesture,setButtonMask:1usize];
            let _: () = msg_send![&gesture,setDelaysPrimaryMouseButtonEvents:true];
            let _: () = msg_send![&gesture,setAllowedTouchTypes:0usize];
            let _: () = msg_send![&button,addGestureRecognizer:&*gesture];
            // A sibling control inside the tracked container, not a child of the
            // draggable/selectable button. The 22 pt column is always reserved.
            let close = alloc_view(class!(NSButton), rect(width - 22., 0., 22., TAB_BAR_HEIGHT));
            let _: () = msg_send![&close,setBordered:false];
            let _: () = msg_send![&close,setTitle:&*NSString::from_str("×")];
            let font: Obj = msg_send![class!(NSFont),systemFontOfSize:14f64];
            let _: () = msg_send![&close,setFont:font];
            let _: () = msg_send![&close, setContentTintColor:color(1., 1., 1., 1.)];
            let label = NSString::from_str(&format!("Cerrar ventana: {name}"));
            let _: () = msg_send![&close,setAccessibilityLabel:&*label];
            let _: () = msg_send![&close,setToolTip:&*label];
            // Reserve -1 (NSView's default tag); selection tags are positive.
            let _: () = msg_send![&close,setTag:-(*id as isize) - 2];
            let _: () = msg_send![&close,setTarget:target];
            let _: () = msg_send![&close,setAction:sel!(closeTileTab:)];
            let _: () = msg_send![&tab,addSubview:&*button];
            let _: () = msg_send![&tab,addSubview:&*close];
            let close_ivar = (*tab).class().instance_variable(c"close").unwrap();
            close_ivar
                .load_ptr::<*mut AnyObject>(&tab)
                .write(Retained::as_ptr(&close) as Obj);
            let active_ivar = (*tab).class().instance_variable(c"active").unwrap();
            active_ivar
                .load_ptr::<u8>(&tab)
                .write(if is_active { 1 } else { 0 });
            // The root retains this view; fetch its current AppKit layer on retint.
            (*tab)
                .class()
                .instance_variable(c"accentPlate")
                .unwrap()
                .load_ptr::<Obj>(&tab)
                .write(active_plate);
            let area: Allocated<AnyObject> = msg_send![class!(NSTrackingArea), alloc];
            // Enter/exit + active always + during drag + visible rect.
            // Do not assume the pointer starts inside newly created views.
            let area: Retained<AnyObject> = msg_send![area,initWithRect:NSRect::ZERO,options:(1usize | 128 | 256 | 512),owner:&*tab,userInfo:ptr::null::<AnyObject>()];
            let _: () = msg_send![&tab,addTrackingArea:&*area];
            set_close_transparent(&tab, !is_active);
            let _: () = msg_send![&root,addSubview:&*tab];
            x = plate.origin.x + plate.size.width + 4.;
        }
        let scroll = alloc_view(class!(NSScrollView), rect(0., 0., width, TAB_BAR_HEIGHT));
        let _: () = msg_send![&scroll,setDrawsBackground:false];
        let _: () = msg_send![&scroll,setWantsLayer:true];
        let viewport: Obj = msg_send![&scroll, layer];
        let _: () = msg_send![viewport,setCornerRadius:9f64];
        let _: () = msg_send![viewport,setMasksToBounds:true];
        let edge: Obj = msg_send![class!(NSColor), colorWithSRGBRed:1.,green:1.,blue:1.,alpha:0.10];
        let edge_cg: *const crate::native::CgColor = msg_send![edge, CGColor];
        let _: () = msg_send![viewport,setBorderColor:edge_cg];
        let _: () = msg_send![viewport,setBorderWidth:1f64];
        let _: () = msg_send![&scroll,setHasHorizontalScroller:total > width];
        let _: () = msg_send![&scroll,setAutohidesScrollers:true];
        let _: () = msg_send![&scroll,setScrollerStyle:1isize];
        let _: () = msg_send![&scroll,setDocumentView:&*root];
        let _: bool = msg_send![&root,scrollRectToVisible:selected];
        scroll
    }
}

/// The per-tab `NSView` that owns the tracking area and remembers whether it is
/// the selected tab, so leaving the hover restores the right glyph state.
fn tab_container_class() -> &'static AnyClass {
    static CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| unsafe {
        let mut builder = ClassBuilder::new(c"AMSpaceTabContainer", class!(NSView)).unwrap();
        builder.add_ivar::<*mut AnyObject>(c"close");
        builder.add_ivar::<u8>(c"active");
        builder.add_ivar::<Obj>(c"accentPlate");
        builder.add_method(
            sel!(mouseEntered:),
            tab_hover_entered as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(mouseExited:),
            tab_hover_exited as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.register()
    });
    *CLASS
}

unsafe fn set_close_transparent(tab: &AnyObject, transparent: bool) {
    unsafe {
        let close = (*tab)
            .class()
            .instance_variable(c"close")
            .unwrap()
            .load_ptr::<*mut AnyObject>(tab)
            .read();
        if close.is_null() {
            return;
        }
        // Only write when it differs, so repeated events stay free.
        let current: bool = msg_send![&*close, isTransparent];
        if current != transparent {
            let _: () = msg_send![&*close, setTransparent:transparent];
        }
    }
}

extern "C-unwind" fn tab_hover_entered(tab: Obj, _: Sel, _: Obj) {
    unsafe { set_close_transparent(&*tab, false) }
}

extern "C-unwind" fn tab_hover_exited(tab: Obj, _: Sel, _: Obj) {
    unsafe {
        let active = (*tab)
            .class()
            .instance_variable(c"active")
            .unwrap()
            .load_ptr::<u8>(&*tab)
            .read();
        set_close_transparent(&*tab, active == 0);
    }
}

/// Seed the glyph state right after a rebuild, so a strip created under the
/// pointer does not wait for the next mouse move. One native read per rebuild.
unsafe fn sync_strip_hover(scroll: &AnyObject) {
    unsafe {
        let window: Obj = msg_send![scroll, window];
        if window.is_null() {
            return;
        }
        let point: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let local: NSPoint = msg_send![window, convertPointFromScreen:point];
        let document: Obj = msg_send![scroll, documentView];
        let tabs: Obj = msg_send![document, subviews];
        let count: usize = msg_send![tabs, count];
        for index in 0..count {
            let tab: Obj = msg_send![tabs, objectAtIndex:index];
            let tracked: bool = msg_send![tab, isKindOfClass:tab_container_class()];
            if !tracked {
                continue;
            }
            let point: NSPoint =
                msg_send![tab,convertPoint:local,fromView:ptr::null::<AnyObject>()];
            let visible: NSRect = msg_send![tab, visibleRect];
            let inside = contains(visible, point);
            let active = (*tab)
                .class()
                .instance_variable(c"active")
                .unwrap()
                .load_ptr::<u8>(&*tab)
                .read();
            set_close_transparent(&*tab, !inside && active == 0);
        }
    }
}

fn hidden_tab_frames(desktop: &Desktop, frames: &[(u32, NSRect)]) -> Vec<(u32, NSRect)> {
    let mut hidden = Vec::new();
    for (id, frame) in frames {
        if let Some((members, _)) = desktop.tab_strip(*id) {
            hidden.extend(
                members
                    .iter()
                    .filter(|member| *member != id)
                    .map(|member| (*member, *frame)),
            );
        }
    }
    hidden
}

fn hint_regions(frame: NSRect) -> [NSRect; 2] {
    let top = frame.size.height * GROUP_DROP_FRACTION;
    [
        rect(frame.origin.x, frame.origin.y, frame.size.width, top),
        rect(
            frame.origin.x,
            frame.origin.y + top,
            frame.size.width,
            frame.size.height - top,
        ),
    ]
}

unsafe fn paint_hint(plates: &[Obj; 2], hint: DropHint) {
    unsafe {
        for (index, plate) in plates.iter().copied().enumerate() {
            if plate.is_null() {
                continue;
            }
            let active = hint.active == Some(index) && hint.available[index];
            let layer: Obj = msg_send![plate, layer];
            let fill = if active {
                color(0.12, 0.28, 0.52, 0.96)
            } else {
                color(0.08, 0.09, 0.12, 0.92)
            };
            let outline = if active {
                color(0.50, 0.77, 1., 1.)
            } else {
                color(1., 1., 1., 0.45)
            };
            let fill: *const crate::native::CgColor = msg_send![fill, CGColor];
            let outline: *const crate::native::CgColor = msg_send![outline, CGColor];
            let _: () = msg_send![layer, setBackgroundColor:fill];
            let _: () = msg_send![layer, setBorderColor:outline];
            let _: () = msg_send![layer, setBorderWidth:if active { 3f64 } else { 1f64 }];
        }
    }
}

unsafe fn badge_content(hint: DropHint) -> (Retained<AnyObject>, [Obj; 2]) {
    unsafe {
        let bounds = rect(0., 0., hint.frame.size.width, hint.frame.size.height);
        let root = alloc_view(class!(NSView), bounds);
        let mut plates = [ptr::null_mut(); 2];
        let regions = if hint.kind == HintKind::Zones {
            hint_regions(bounds)
        } else {
            [bounds, rect(0., 0., 0., 0.)]
        };
        for (index, region) in regions.into_iter().enumerate() {
            if region.size.width <= 0. || region.size.height <= 0. {
                continue;
            }
            let inset = 6f64
                .min(region.size.height / 8.)
                .min(region.size.width / 8.);
            let region = appkit(region, bounds.size.height);
            let plate = rounded_plate(
                Retained::as_ptr(&root) as Obj,
                rect(
                    region.origin.x + inset,
                    region.origin.y + inset,
                    region.size.width - inset * 2.,
                    region.size.height - inset * 2.,
                ),
                [0.08, 0.09, 0.12],
                0.92,
                12.,
            );
            plates[index] = plate;
            let width = region.size.width - inset * 2.;
            let height = region.size.height - inset * 2.;
            let group =
                hint.kind == HintKind::Group || (hint.kind == HintKind::Zones && index == 0);
            let title = if group {
                if width >= 240. {
                    "Apilar como pestaña"
                } else {
                    "Pestañas"
                }
            } else if hint.kind == HintKind::Zones {
                if width >= 240. {
                    "Intercambiar posiciones"
                } else if width >= 130. {
                    "Intercambiar"
                } else {
                    "Posición"
                }
            } else {
                "Colocar aquí"
            };
            let font = if width >= 240. { 18. } else { 12. };
            let title_height = 24f64.min(height);
            let status_height = if !hint.available[index] && height >= 100. {
                18.
            } else {
                0.
            };
            let title = if !hint.available[index] && status_height == 0. {
                if width >= 130. {
                    "No disponible"
                } else {
                    "No cabe"
                }
            } else {
                title
            };
            let icon_size = (height - title_height - status_height - 26.)
                .min(width - 24.)
                .clamp(0., 72.);
            let icon_size = if icon_size >= 12. { icon_size } else { 0. };
            let gap = if icon_size > 0. { 10. } else { 0. };
            let bottom = ((height - icon_size - gap - title_height - status_height) / 2.).max(0.);
            let tint = color(
                0.97,
                0.98,
                1.,
                if hint.available[index] { 1. } else { 0.65 },
            );
            let label = crate::native::label(
                plate,
                rect(
                    4.,
                    bottom + status_height,
                    (width - 8.).max(0.),
                    title_height,
                ),
                title,
                font,
                0.4,
                false,
            );
            let _: () = msg_send![label, setAlignment:1isize];
            let _: () = msg_send![label, setTextColor:tint];
            if status_height > 0. {
                let label = crate::native::label(
                    plate,
                    rect(4., bottom, (width - 8.).max(0.), status_height),
                    "No disponible",
                    12.,
                    0.,
                    false,
                );
                let _: () = msg_send![label, setAlignment:1isize];
                let _: () = msg_send![label, setTextColor:tint];
            }
            if icon_size > 0. {
                let symbol = if group {
                    "rectangle.stack"
                } else {
                    "arrow.left.arrow.right"
                };
                let image: Obj = msg_send![class!(NSImage), imageWithSystemSymbolName:&*NSString::from_str(symbol), accessibilityDescription:&*NSString::from_str(title)];
                if !image.is_null() {
                    let view = alloc_view(
                        class!(NSImageView),
                        rect(
                            (width - icon_size) / 2.,
                            bottom + status_height + title_height + gap,
                            icon_size,
                            icon_size,
                        ),
                    );
                    let _: () = msg_send![&view, setImage:image];
                    let _: () = msg_send![&view, setImageScaling:3isize];
                    let _: () = msg_send![&view, setContentTintColor:tint];
                    let _: () = msg_send![plate, addSubview:&*view];
                }
            }
        }
        if hint.kind == HintKind::Zones {
            rounded_plate(
                Retained::as_ptr(&root) as Obj,
                rect(
                    6.,
                    bounds.size.height * (1. - GROUP_DROP_FRACTION) - 1.,
                    (bounds.size.width - 12.).max(0.),
                    2.,
                ),
                [0.85, 0.90, 1.],
                0.85,
                0.,
            );
        }
        paint_hint(&plates, hint);
        (root, plates)
    }
}

/// Insertion gaps exclude the dragged tab, but use the unchanged painted widths.
fn tab_reorder_target(
    members: &[u32],
    widths: impl Iterator<Item = f64>,
    id: u32,
    point: f64,
) -> Option<(usize, f64)> {
    if !point.is_finite() || !members.contains(&id) {
        return None;
    }
    let mut x = 4.;
    let mut index = 0;
    let mut marker = 2.;
    for (member, width) in members.iter().zip(widths) {
        if *member != id {
            if point < x + width / 2. {
                return Some((index, x - 2.));
            }
            index += 1;
            marker = x + width + 2.;
        }
        x += width + 4.;
    }
    Some((index, marker))
}

fn tab_width(name: &str) -> f64 {
    (name.chars().count() as f64 * 6.4 + 56.).clamp(52., 242.)
}

fn strip_width(widths: impl Iterator<Item = f64>) -> f64 {
    let mut total = 0.;
    let mut count = 0usize;
    for width in widths {
        total += width;
        count += 1;
    }
    if count == 0 {
        return 0.;
    }
    total + 4. * (count - 1) as f64 + 8.
}

/// The window's own title when AX has one, otherwise its application name.
fn window_label(element: Obj, pid: i32) -> String {
    let title = ax::attr(element as ax::Cf, "AXTitle")
        .and_then(owned_title)
        .filter(|s| !s.is_empty());
    title.or_else(|| app_name(pid)).unwrap_or_default()
}

fn owned_title(value: ax::Owned) -> Option<String> {
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFGetTypeID(value: ax::Cf) -> usize;
        fn CFStringGetTypeID() -> usize;
    }
    // Convert while the Copy result still owns the CFString. Returning a borrowed
    // NSString from this scope would outlive `value` and message freed memory.
    unsafe {
        (CFGetTypeID(value.0) == CFStringGetTypeID())
            .then(|| (&*(value.0 as *const NSString)).to_string())
    }
}

fn app_name(pid: i32) -> Option<String> {
    unsafe {
        let app: *mut AnyObject =
            msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid];
        if app.is_null() {
            return None;
        }
        let name: *mut NSString = msg_send![app, localizedName];
        name.as_ref().map(|s| s.to_string())
    }
}

fn app_icon(pid: i32) -> Option<Retained<AnyObject>> {
    unsafe {
        let app: *mut AnyObject =
            msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid];
        if app.is_null() {
            return None;
        }
        // `icon` is a +0 getter. objc2 retains an optional owned return; adopting
        // its raw pointer with from_raw would release somebody else's reference.
        msg_send![app, icon]
    }
}
#[test]
fn strip_sits_above_window_without_overflowing_its_logical_slot() {
    let area = rect(0., 30., 1600., 900.);
    let mut base = Desktop::configured(Layout::Grid, None);
    base.reconcile(&[1, 2, 3], area, 10.);
    let desktop = base
        .proposed_drop(3, DropIntent::Group(1), area, 10.)
        .expect("join must fit");
    let frames = desktop.window_frames(area, 10.);
    let window = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
    let slot = desktop.slot_frame(3, window);
    let names: Vec<String> = desktop
        .tab_strip(3)
        .map(|(members, _)| members.iter().map(|id| format!("v{id}")).collect())
        .unwrap();
    let total = strip_width(names.iter().map(|n| tab_width(n)));
    // Two tabs: their own widths plus one inner gap and the two margins.
    assert_eq!(total, names.iter().map(|n| tab_width(n)).sum::<f64>() + 12.);
    let left = tab_bar_frame(window, total, false);
    for panel in [left, tab_bar_frame(window, total, true)] {
        assert!(panel.origin.x >= slot.origin.x);
        assert!(panel.origin.x + panel.size.width <= slot.origin.x + slot.size.width);
        assert!(panel.origin.y >= slot.origin.y);
        assert!(panel.origin.y + panel.size.height <= slot.origin.y + slot.size.height);
        assert!(panel.origin.y + panel.size.height + TAB_BAR_GAP <= window.origin.y);
    }
    // Default: 8 pt inset from the slot's left edge.
    assert_eq!(left.origin.x, slot.origin.x + 8.);
    // Centered: symmetric inside the same slot.
    let centered = tab_bar_frame(window, total, true);
    assert_eq!(
        centered.origin.x + centered.size.width / 2.,
        window.origin.x + window.size.width / 2.
    );
    let leading = centered.origin.x - slot.origin.x;
    let trailing = slot.origin.x + slot.size.width - (centered.origin.x + centered.size.width);
    assert!((leading - trailing).abs() < 1e-9);
    // The left inset is a function of the slot alone, so renaming a tab keeps
    // the strip at the same x while its width still tracks the content.
    let mut longer = names.clone();
    longer[0] = "una ventana con un titulo mucho mas largo".into();
    let wider = strip_width(longer.iter().map(|n| tab_width(n)));
    assert_eq!(tab_bar_frame(window, wider, false).origin.x, left.origin.x);
    assert!(wider > total);
    // A narrow window clamps both edges instead of overflowing the slot.
    let narrow = rect(window.origin.x, window.origin.y, 120., window.size.height);
    let tight_left = tab_bar_frame(narrow, wider, false);
    let tight_centered = tab_bar_frame(narrow, wider, true);
    for panel in [tight_left, tight_centered] {
        assert!(panel.origin.x >= narrow.origin.x);
        assert!(panel.origin.x + panel.size.width <= narrow.origin.x + narrow.size.width + 0.001);
    }
    assert_eq!(tight_left.origin.x, narrow.origin.x + 8.);
    assert_eq!(
        tight_centered.origin.x - narrow.origin.x,
        (narrow.size.width - tight_centered.size.width) / 2.
    );
    assert_eq!(strip_width(std::iter::empty()), 0.);
}

#[test]
fn drop_hint_divider_matches_hit_testing_and_keeps_the_original_slot() {
    for area in [rect(0., 30., 1600., 900.), rect(-950., -500., 900., 1400.)] {
        let mut desktop = Desktop::configured(Layout::Grid, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let frames = desktop.frames(area, 10.);
        let target = frames.iter().find(|(id, _)| *id == 2).unwrap().1;
        let minimums = HashMap::new();
        let input = DropContext {
            workspace: (1, 1),
            id: 1,
            area,
            desktop: &desktop,
            minimums: &minimums,
        };
        let mut cache = DropPreviewCache::default();
        let [top, bottom] = hint_regions(target);
        assert_eq!(top.origin, target.origin);
        assert_eq!(top.size.width, target.size.width);
        assert_eq!(bottom.origin.y, top.origin.y + top.size.height);
        assert!(
            (bottom.origin.y + bottom.size.height - target.origin.y - target.size.height).abs()
                < 1e-9
        );
        for (offset, expected, active) in [
            (-0.01, DropIntent::Group(2), 0),
            (0.01, DropIntent::Window(2, Zone::Center), 1),
        ] {
            let point = NSPoint::new(
                target.origin.x + target.size.width / 2.,
                bottom.origin.y + offset,
            );
            let intent = drop_intent(area, &frames, 1, point, true, true).unwrap();
            assert_eq!(intent, expected);
            let preview = cache.preview(input, intent);
            assert!(preview.is_some());
            let hint = cache.hint(input, intent, true, preview).unwrap();
            assert!(matches!(hint.kind, HintKind::Zones));
            assert_eq!(hint.frame, target);
            assert_eq!(hint.active, Some(active));
            assert_eq!(hint.available, [true, true]);
            // Native local coordinates place the same divider at the top third.
            let local = appkit(top, 1800.);
            assert!((local.origin.y - (1800. - bottom.origin.y)).abs() < 1e-9);
        }
    }
}

#[test]
fn drop_hints_do_not_offer_full_groups_or_invent_zones_for_arrivals() {
    let area = rect(0., 30., 1600., 900.);
    let mut desktop = Desktop::configured(Layout::Grid, None);
    desktop.reconcile(&[1, 2, 3], area, 10.);
    let mut desktop = desktop
        .proposed_drop(3, DropIntent::Group(2), area, 10.)
        .unwrap();
    desktop.max_group_tabs = 2;
    let minimums = HashMap::new();
    let input = DropContext {
        workspace: (1, 1),
        id: 1,
        area,
        desktop: &desktop,
        minimums: &minimums,
    };
    let mut cache = DropPreviewCache::default();
    let rejected = cache.preview(input, DropIntent::Group(3));
    assert!(rejected.is_none());
    let hint = cache
        .hint(input, DropIntent::Group(3), true, rejected)
        .unwrap();
    assert_eq!(hint.available, [false, true]);
    assert_eq!(hint.active, None);
    let swap = DropIntent::Window(3, Zone::Center);
    let preview = cache.preview(input, swap);
    let hint = cache.hint(input, swap, false, preview).unwrap();
    assert!(!matches!(hint.kind, HintKind::Zones));
    let arrival = DropContext { id: 4, ..input };
    let intent = DropIntent::Screen(Zone::Left);
    let preview = cache.preview(arrival, intent);
    let hint = cache.hint(arrival, intent, true, preview).unwrap();
    assert!(matches!(hint.kind, HintKind::Place));
    assert!(hint.frame.size.width <= preview.unwrap().size.width);
    assert!(hint.frame.size.height <= preview.unwrap().size.height);
    assert_eq!(desktop.tab_strip(3).unwrap().0, &[2, 3]);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_title_is_owned_before_its_cf_object_is_released_and_checks_type() {
        #[link(name = "CoreFoundation", kind = "framework")]
        unsafe extern "C" {
            fn CFStringCreateWithCString(
                allocator: ax::Cf,
                bytes: *const std::ffi::c_char,
                encoding: u32,
            ) -> ax::Cf;
        }
        let title = c"Título de una ventana suficientemente largo para no ser tagged pointer";
        let owned = ax::Owned::new(unsafe {
            CFStringCreateWithCString(ptr::null(), title.as_ptr(), 0x08000100)
        })
        .unwrap();
        let copied = owned_title(owned).unwrap();
        assert_eq!(copied, title.to_str().unwrap());
        assert!(owned_title(ax::application(std::process::id() as i32).unwrap()).is_none());
    }
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
                        manager.observe_reentry(Some(sample), now + Duration::from_millis(3160))
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
        // Agreement needs one bounded step, not the old 160 ms hold.
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(59)));
        assert!(barrier.observe(Some(sample.clone()), now + Duration::from_millis(60)));
        // A resized frame restarts the anchor from its own instant.
        let mut resizing = sample.clone();
        resizing.windows[0].4.as_mut().unwrap().size.width += 80.;
        assert!(!barrier.observe(Some(resizing), now + Duration::from_millis(80)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(150)));
        assert!(barrier.observe(Some(sample.clone()), now + Duration::from_millis(210)));
        // A closed window restarts it again, and a failed read cannot resume
        // the previous agreement.
        let mut closing = sample.clone();
        closing.windows.pop();
        closing.absent.push((42, 2));
        assert!(!barrier.observe(Some(closing), now + Duration::from_millis(280)));
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(350)));
        assert!(barrier.observe(Some(sample.clone()), now + Duration::from_millis(410)));
        assert!(!barrier.observe(None, now + Duration::from_millis(480)));
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
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(59)));
        assert!(barrier.observe(Some(sample), now + Duration::from_millis(60)));
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
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(40)));
        sample.windows[0].3.origin.x += 2.;
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(100)));
        sample.areas[0].1.size.height -= 100.;
        assert!(!barrier.observe(Some(sample.clone()), now + Duration::from_millis(160)));
        assert!(barrier.observe(Some(sample.clone()), now + Duration::from_millis(220)));
        // A first visit to an empty desktop is a valid stable state too.
        let empty = ReentrySample {
            areas: Vec::new(),
            windows: Vec::new(),
            absent: Vec::new(),
        };
        assert!(!barrier.observe(Some(empty.clone()), now + Duration::from_millis(280)));
        assert!(barrier.observe(Some(empty), now + Duration::from_millis(344)));
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
    fn desktop_reentry_keeps_old_and_late_floats_out_of_the_layout() {
        use crate::desktop_mode::Mode;
        let mut manager = Manager::default();
        let pid = std::process::id() as i32;
        let element = ax::application(pid).unwrap();
        let space = (2, 8);
        let area = rect(-900., -400., 880., 1580.);
        let quick = rect(-800., -100., 500., 600.);
        for id in [1, 2] {
            manager.windows.insert(
                id,
                Managed {
                    pid,
                    space,
                    original: quick,
                    original_area: area,
                    element: element.clone(),
                },
            );
        }
        manager.floating.insert(2);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1], area, 10.);
        let before = desktop.frames(area, 10.);
        manager.desktops.insert(space, desktop);
        let now = Instant::now();
        manager.set_presentation_at(
            Mode::Desktop,
            now,
            Some(HashSet::from([(pid, 1), (pid, 2)])),
        );
        manager.set_presentation_at(
            Mode::Normal,
            now,
            Some(HashSet::from([(pid, 1), (pid, 2), (pid, 3)])),
        );
        // A deferred entry may already be tracked. Window 4 is published only
        // after macOS dismisses Desktop, during the existing reentry barrier.
        for id in [3, 4] {
            manager.windows.insert(
                id,
                Managed {
                    pid,
                    space,
                    original: quick,
                    original_area: area,
                    element: element.clone(),
                },
            );
            manager.transition_from(id, quick, area);
            manager.verifications.insert(id, Verification::new(area));
        }
        manager
            .desktops
            .get_mut(&space)
            .unwrap()
            .reconcile(&[1, 3, 4], area, 10.);
        let candidates = [1, 2, 3, 4].map(|id| (pid, id, quick));
        manager.finish_desktop_reentry(&candidates, now + Duration::from_millis(600));
        manager.apply_desktop_float(pid + 1, 4);
        assert!(!manager.floating.contains(&4)); // A different PID cannot consume the classification.
        for id in [3, 4] {
            manager.apply_desktop_float(pid, id);
        }
        assert_eq!(manager.floating, HashSet::from([2, 3, 4]));
        assert_eq!(manager.desktops[&space].frames(area, 10.), before);
        assert!(manager.transitions.is_empty() && manager.verifications.is_empty());
        for id in [2, 3, 4] {
            assert!(!manager.can_tile_drag(id, space));
            assert_eq!(manager.windows[&id].original, quick);
        }
        // Ordinary subsequent arrivals are not captured by the completed cycle.
        manager.finish_desktop_reentry(&[(pid, 5, quick)], now + Duration::from_millis(700));
        manager.apply_desktop_float(pid, 5);
        assert!(!manager.floating.contains(&5));
        let identities = HashSet::from([(pid, 1), (pid, 2), (pid, 3), (pid, 4)]);
        manager.set_presentation_at(Mode::Desktop, now, Some(identities.clone()));
        manager.set_presentation_at(Mode::Normal, now, Some(identities));
        manager.finish_desktop_reentry(&candidates, now + Duration::from_millis(600));
        assert_eq!(manager.floating, HashSet::from([2, 3, 4]));
        assert_eq!(manager.desktops[&space].frames(area, 10.), before);
    }

    #[test]
    fn delayed_manual_resume_does_not_float_unrelated_normal_windows() {
        use crate::desktop_mode::Mode;
        let mut manager = Manager::default();
        let now = Instant::now();
        let frame = rect(0., 30., 800., 600.);
        manager.set_presentation_at(Mode::Desktop, now, Some(HashSet::from([(7, 1)])));
        manager.set_presentation_at(Mode::Normal, now, Some(HashSet::from([(7, 1), (7, 2)])));
        manager.finish_desktop_reentry(
            &[(7, 1, frame), (7, 2, frame), (7, 3, frame)],
            now + Duration::from_secs(60),
        );
        manager.apply_desktop_float(7, 2);
        manager.apply_desktop_float(7, 3);
        assert!(manager.floating.contains(&2)); // Observed during Desktop: no expiry of its state.
        assert!(!manager.floating.contains(&3)); // Opened during a later manual pause.
    }

    #[test]
    fn native_tab_inheritance_cannot_tile_an_incoming_floating_window() {
        use crate::desktop_mode::Mode;
        for opened_on_desktop in [false, true] {
            let mut manager = Manager::default();
            let pid = std::process::id() as i32;
            let element = ax::application(pid).unwrap();
            let space = (1, 7);
            let area = rect(10., 41., 1660., 947.);
            let quick = rect(300., 200., 600., 400.);
            let mut desktop = Desktop::configured(Layout::Spiral, None);
            desktop.reconcile(&[1, 3], area, 10.);
            manager.desktops.insert(space, desktop);
            for (id, original) in [(1, area), (2, quick), (4, quick)] {
                manager.windows.insert(
                    id,
                    Managed {
                        pid,
                        space,
                        original,
                        original_area: area,
                        element: element.clone(),
                    },
                );
            }
            if opened_on_desktop {
                let now = Instant::now();
                manager.set_presentation_at(Mode::Desktop, now, Some(HashSet::from([(pid, 1)])));
                manager.set_presentation_at(
                    Mode::Normal,
                    now,
                    Some(HashSet::from([(pid, 1), (pid, 2)])),
                );
                manager.apply_desktop_float(pid, 2);
            } else {
                manager.floating.insert(2);
            }
            for id in [1, 2] {
                manager.applied.insert(id, area);
                manager.constrained.insert(id, (area, area));
                manager.minimum_sizes.insert(id, area.size);
                manager.transition_from(id, quick, area);
                manager.verifications.insert(id, Verification::new(area));
            }
            manager.inherit_tab_slot(1, 2);
            assert!(manager.floating.contains(&2));
            assert_eq!(manager.windows[&2].original, quick);
            assert_eq!(manager.desktops[&space].order, vec![3]);
            assert!(manager.transitions.is_empty() && manager.verifications.is_empty());
            assert!(manager.applied.is_empty() && manager.constrained.is_empty());
            assert!(manager.minimum_sizes.is_empty());
            // Switching native tabs keeps the entire native window floating.
            manager.inherit_tab_slot(2, 1);
            manager.inherit_tab_slot(1, 2);
            manager.inherit_tab_slot(2, 4);
            assert!(manager.floating.contains(&4));
            assert!(manager.inactive_tabs.contains(&1) && manager.inactive_tabs.contains(&2));
            manager.inherit_tab_slot(4, 2);
            assert_eq!(manager.windows[&1].original, quick);
            assert_eq!(manager.desktops[&space].order, vec![3]);
            // An explicit shortcut back to tile must win over a stale inactive
            // tab's previous float classification on the next native selection.
            manager.floating.remove(&2);
            manager
                .desktops
                .get_mut(&space)
                .unwrap()
                .reconcile(&[2, 3], area, 10.);
            let expected: Vec<_> = manager.desktops[&space]
                .frames(area, 10.)
                .into_iter()
                .map(|(id, frame)| (if id == 2 { 1 } else { id }, frame))
                .collect();
            manager.inherit_tab_slot(2, 1);
            assert!(!manager.floating.contains(&1));
            assert_eq!(manager.desktops[&space].frames(area, 10.), expected);
        }
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
        assert!(manager.waiting_for_space(now + Duration::from_millis(399)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(400)));
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
                assert!(manager.waiting_for_space(now + Duration::from_millis(399)));
                assert!(!manager.waiting_for_space(now + Duration::from_millis(400)));
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
    fn hidden_members_get_destinations_even_when_the_active_frame_is_settled() {
        let area = rect(10., 41., 1660., 947.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let desktop = desktop
            .proposed_drop(3, DropIntent::Group(1), area, 10.)
            .unwrap();
        let frames = desktop.window_frames(area, 10.);
        let slot = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
        let manager = Manager::default();
        assert!(!manager.needs_placement(3, slot, Some(slot), false));
        assert_eq!(hidden_tab_frames(&desktop, &frames), vec![(1, slot)]);
        let mut selected = desktop;
        selected.select_tab(1);
        assert_eq!(
            hidden_tab_frames(&selected, &selected.window_frames(area, 10.)),
            vec![(3, slot)]
        );
    }

    #[test]
    fn hidden_placement_rejects_an_element_that_does_not_identify_the_member() {
        let mut manager = Manager::default();
        let space = (1, 7);
        let area = rect(10., 41., 1660., 947.);
        manager.active_areas.push((space, area));
        manager.windows.insert(
            1,
            Managed {
                pid: std::process::id() as i32,
                space,
                original: area,
                original_area: area,
                element: ax::application(std::process::id() as i32).unwrap(),
            },
        );
        manager.place_hidden_tab(1, area);
        assert!(manager.applied.is_empty());
        assert!(manager.verifications.is_empty());
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
    fn disabling_the_batch_switches_to_one_write_and_the_repair_cadence() {
        let from = rect(10., 41., 1600., 900.);
        let target = rect(-890., -390., 880., 1580.);
        let mut animated = Manager {
            animated: Some(true),
            ..Default::default()
        };
        animated.transition_from(1, from, target);
        assert_eq!(animated.tick_interval(), 1. / 60.);
        assert!(animated.transitions.contains_key(&1));
        assert!(animated.verifications.is_empty());
        let element = ax::application(std::process::id() as i32).unwrap();
        let mut instant = Manager {
            animated: Some(false),
            windows: [(
                1,
                Managed {
                    pid: 9,
                    space: (1, 7),
                    original: from,
                    original_area: from,
                    element: element.clone(),
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        instant.transition_from(1, from, target);
        // The eased batch is gone: the destination is the pending verification
        // instead, so no second frame is ever written for the same window.
        assert!(instant.transitions.is_empty());
        assert_eq!(instant.verifications[&1].target, target);
        assert_eq!(instant.tick_interval(), 0.08);
        // An identical destination still needs no work at all.
        instant.transition_from(2, target, target);
        assert!(!instant.verifications.contains_key(&2));
        assert!(instant.transitions.is_empty());
    }

    #[test]
    fn replacing_a_template_cancels_hidden_tab_writes_but_not_other_workspaces() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!(
                "amspace.tests.restore.tabs.{}",
                std::process::id()
            ));
            let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            let area = rect(-900., 40., 1800., 1200.);
            let key = (1, 7);
            let mut desktop = Desktop::configured(Layout::Spiral, None);
            desktop.reconcile(&[1, 2, 3], area, 10.);
            desktop = desktop
                .proposed_drop(4, DropIntent::Group(1), area, 10.)
                .unwrap();
            assert!(desktop.is_hidden_tab(1));
            let context = PickerContext {
                workspace: key,
                area,
                desktop: desktop.clone(),
            };
            let proposal = context.proposal(Layout::Grid).unwrap();
            let saved = proposal.save_distribution(area).unwrap();
            assert_ne!(
                desktop.window_frames(area, 10.),
                proposal.window_frames(area, 10.)
            );
            let mut manager = Manager::default();
            manager.desktops.insert(key, desktop);
            for id in [1, 2, 3, 4, 9] {
                let from = rect(10., 41., 1600., 900.);
                let target = rect(-890., -390., 880., 1580.);
                manager.transition_from(id, from, target);
                manager.verifications.insert(id, Verification::new(target));
                manager.applied.insert(id, target);
                manager.constrained.insert(id, (target, from));
            }
            assert!(manager.restore_distribution_at(raw, key, &saved, area));
            assert_eq!(
                manager.desktops[&key].window_frames(area, 10.),
                proposal.window_frames(area, 10.)
            );
            for id in [1, 2, 3, 4] {
                assert!(!manager.transitions.contains_key(&id));
                assert!(!manager.verifications.contains_key(&id));
                assert!(!manager.applied.contains_key(&id));
                assert!(!manager.constrained.contains_key(&id));
            }
            assert!(manager.transitions.contains_key(&9));
            assert!(manager.verifications.contains_key(&9));
            assert!(manager.applied.contains_key(&9));
            assert!(manager.constrained.contains_key(&9));
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
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
        assert!(manager.waiting_for_space(now + Duration::from_millis(399)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(400)));
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
        // The second notification restarts the shorter floor from its own instant.
        assert!(manager.waiting_for_space(now + Duration::from_millis(799)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(800)));
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
        // The shorter quiet floor shifts both bounded windows by the same 200 ms.
        assert_eq!(
            manager.pending_drop.as_ref().unwrap().expires,
            now + Duration::from_millis(1100)
        );
        assert_eq!(
            manager.handoffs[&1].expires,
            now + Duration::from_millis(1600)
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
            verification.step(old, due + Duration::from_millis(30)),
            VerificationStep::Wait
        );
        assert_eq!(
            verification.step(target, due + Duration::from_millis(60)),
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
                            let intent =
                                drop_intent(area, &slots, source, point, true, false).unwrap();
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
                let intent = drop_intent(area, &frames, source, point, true, false).unwrap();
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
        assert!(manager.waiting_for_space(now + Duration::from_millis(399)));
        assert!(!manager.waiting_for_space(now + Duration::from_millis(400)));
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

    fn pending_tab_detach(id: u32) -> Manager {
        let space = (1, 7);
        let area = rect(-900., 40., 1800., 1200.);
        let pid = std::process::id() as i32;
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2], area, 10.);
        desktop = desktop
            .proposed_drop(3, DropIntent::Group(1), area, 10.)
            .unwrap();
        desktop = desktop
            .proposed_drop(4, DropIntent::Group(3), area, 10.)
            .unwrap();
        let proposed = tab_detach_proposal(&desktop, id, area, &HashMap::new()).unwrap();
        let mut manager = Manager {
            active_areas: vec![(space, area)],
            pressed: true,
            ..Manager::default()
        };
        for member in 1..=4 {
            manager.windows.insert(
                member,
                Managed {
                    pid,
                    space,
                    original: area,
                    original_area: area,
                    element: ax::application(pid).unwrap(),
                },
            );
            manager.applied.insert(member, area);
            manager
                .verifications
                .insert(member, Verification::new(area));
        }
        manager.desktops.insert(space, desktop.clone());
        manager.tab_drag = Some(TabDrag {
            id,
            pid,
            space,
            area,
            panel_number: 10,
            baseline: desktop,
            proposed: Some(proposed),
            reorder: None,
            released_at: None,
        });
        manager
    }

    #[test]
    fn rejected_close_preserves_group_members_and_pending_geometry() {
        for id in [1, 4] {
            let mut manager = pending_tab_detach(id);
            let before = manager.desktops[&(1, 7)].clone();
            let applied = manager.applied.clone();
            // A recognized drag must never turn into a close action.
            assert!(!manager.close_tab(id));
            assert!(manager.desktops[&(1, 7)] == before);
            assert_eq!(manager.applied, applied);
            manager.cancel_tab_drag();
            // The fixture's AX element identifies an application, not this
            // window. No close can be sent even when AX is authorized.
            assert!(!manager.close_tab(id));
            assert!(manager.desktops[&(1, 7)] == before);
            assert_eq!(manager.applied, applied);
        }
    }

    #[test]
    fn reordering_without_detach_capacity_preserves_pending_geometry() {
        let mut manager = pending_tab_detach(1);
        let before = manager.desktops[&(1, 7)].window_frames(manager.active_areas[0].1, 10.);
        let applied = manager.applied.clone();
        let other = Desktop::configured(Layout::Grid, None);
        manager.desktops.insert((2, 9), other.clone());
        let drag = manager.tab_drag.as_mut().unwrap();
        drag.proposed = None;
        drag.reorder = Some(2);
        manager.observe_tab_pointer(false, Instant::now());
        let drag = manager.tab_drag.take().unwrap();
        manager.commit_tab_reorder(drag.space, drag.id, drag.reorder.unwrap());
        assert_eq!(
            manager.desktops[&drag.space].tab_strip(1),
            Some((&[3, 4, 1][..], 1))
        );
        assert_eq!(
            manager.desktops[&drag.space].window_frames(drag.area, 10.),
            before
        );
        assert_eq!(manager.applied, applied);
        for (id, frame) in &applied {
            assert_eq!(manager.verifications[id].target, *frame);
        }
        assert!(manager.desktops[&(2, 9)] == other);
    }

    #[test]
    fn reorder_hit_testing_uses_variable_width_centers_and_excludes_the_dragged_tab() {
        let members = [1, 2, 3];
        let target =
            |id, point| tab_reorder_target(&members, [60., 200., 80.].into_iter(), id, point);
        assert_eq!(target(2, 0.), Some((0, 2.)));
        assert_eq!(target(2, 33.), Some((0, 2.)));
        assert_eq!(target(2, 34.), Some((1, 270.)));
        assert_eq!(target(2, 312.), Some((2, 354.)));
        assert_eq!(target(1, 168.), Some((1, 270.)));
        assert_eq!(target(3, 400.), Some((2, 270.)));
        assert_eq!(target(9, 0.), None);
        assert_eq!(target(1, f64::NAN), None);
    }

    #[test]
    fn tab_release_before_appkit_ended_keeps_the_proposal_and_commits_only_its_workspace() {
        for id in [1, 4] {
            let mut manager = pending_tab_detach(id);
            let other = rect(0., 0., 400., 300.);
            manager.applied.insert(99, other);
            manager.verifications.insert(99, Verification::new(other));
            manager.observe_tab_pointer(false, Instant::now());
            let drag = manager
                .tab_drag
                .take()
                .expect("mouse-up must not beat the AppKit callback");
            assert_eq!(manager.desktops[&drag.space].order.len(), 2);
            manager.commit_tab_detach(drag);
            let desktop = &manager.desktops[&(1, 7)];
            assert_eq!(desktop.order.len(), 3);
            assert_eq!(desktop.tab_role(id), None);
            let group = desktop
                .groups()
                .iter()
                .find(|g| g.members.len() > 1)
                .unwrap();
            assert!(!group.members.contains(&id));
            assert_eq!(group.members.len(), 2);
            for member in 1..=4 {
                assert!(!manager.applied.contains_key(&member));
                assert!(!manager.verifications.contains_key(&member));
            }
            assert_eq!(manager.applied[&99], other);
            assert_eq!(manager.verifications[&99].target, other);
        }
    }

    #[test]
    fn cancelled_or_undelivered_tab_drag_never_detaches_the_group() {
        for cancellation in 0..8 {
            let mut manager = pending_tab_detach(1);
            if cancellation >= 4 {
                let drag = manager.tab_drag.as_mut().unwrap();
                drag.reorder = Some(2);
                drag.proposed = None;
            }
            let before = manager.desktops[&(1, 7)].clone();
            let now = Instant::now();
            match cancellation % 4 {
                0 => manager.cancel_tab_drag(),
                1 => manager.space_changed_at(now),
                2 => manager.screens_changed(),
                _ => {
                    manager.observe_tab_pointer(false, now);
                    manager.observe_tab_pointer(false, now + Duration::from_millis(350));
                }
            }
            assert!(manager.tab_drag.is_none());
            assert!(manager.desktops[&(1, 7)] == before);
            assert!(
                manager.pressed,
                "a cancelled held gesture must not capture the window below"
            );
        }
    }

    #[test]
    fn tab_drag_rejects_changed_identity_membership_area_and_selection() {
        for change in 0..4 {
            let mut manager = pending_tab_detach(1);
            match change {
                0 => manager.windows.get_mut(&1).unwrap().pid += 1,
                1 => manager.windows.get_mut(&1).unwrap().space = (1, 9),
                2 => manager.active_areas[0].1.origin.y += 20.,
                _ => {
                    manager.desktops.get_mut(&(1, 7)).unwrap().select_tab(3);
                }
            }
            let expected = manager.desktops[&(1, 7)].clone();
            manager.observe_tab_pointer(true, Instant::now());
            assert!(manager.tab_drag.is_none());
            assert!(manager.desktops[&(1, 7)] == expected);
            assert!(manager.desktops[&(1, 7)].is_hidden_tab(1));
        }
    }

    #[test]
    fn grouped_resize_maps_native_edges_back_to_logical_slots() {
        let manager = pending_tab_detach(4);
        let tab = manager.tab_drag.as_ref().unwrap();
        let from = tab
            .baseline
            .window_frames(tab.area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        let mut drag = test_drag();
        drag.id = 4;
        drag.space = tab.space;
        drag.area = tab.area;
        drag.baseline = tab.baseline.clone();
        drag.start = from;
        drag.point = NSPoint::new(
            from.origin.x + from.size.width,
            from.origin.y + from.size.height / 2.,
        );
        drag.resize_gesture = true;
        let mut requested = from;
        requested.size.width -= 100.;
        let point = NSPoint::new(drag.point.x - 100., drag.point.y);
        let proposed = drag
            .resize_proposal(requested, point, Some(tab.space))
            .unwrap();
        let actual = proposed
            .window_frames(tab.area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        assert!(near(actual, requested));
        assert_eq!(actual.origin.y, from.origin.y);
        assert_eq!(actual.size.height, from.size.height);
        assert!(drag.baseline == tab.baseline);
    }

    #[test]
    fn tab_cycle_wraps_in_both_directions_and_rejects_degenerate_strips() {
        let members = [10u32, 20, 30];
        assert_eq!(tab_cycle(&members, 0, true), Some(20));
        assert_eq!(tab_cycle(&members, 2, true), Some(10));
        assert_eq!(tab_cycle(&members, 0, false), Some(30));
        assert_eq!(tab_cycle(&members, 1, false), Some(10));
        assert_eq!(tab_cycle(&[7], 0, true), None);
        assert_eq!(tab_cycle(&members, 3, true), None);
    }

    /// Two independent leaves plus one two-tab strip.
    fn contract_desktop(area: NSRect) -> Desktop {
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile_at(&[1, 2, 3], area, 10., None);
        desktop
            .proposed_drop(4, DropIntent::Group(3), area, 10.)
            .unwrap()
    }

    #[test]
    fn group_rejection_names_the_limit_and_accepts_only_feasible_joins() {
        let area = rect(-900., 40., 1800., 1200.);
        let desktop = contract_desktop(area);
        let minimums = HashMap::new();
        assert_eq!(
            group_rejection(&desktop, &minimums, 1, 1, area),
            Some(SAME_TAB)
        );
        assert_eq!(
            group_rejection(&desktop, &minimums, 1, 99, area),
            Some(GROUP_GONE)
        );
        assert_eq!(group_rejection(&desktop, &minimums, 1, 4, area), None);
        // The capacity limit is reported before any solver work.
        let mut full = desktop.clone();
        full.max_group_tabs = 2;
        assert_eq!(
            group_rejection(&full, &minimums, 1, 4, area),
            Some(GROUP_FULL)
        );
        let mut narrow = desktop.clone();
        narrow.max_group_tabs = 1;
        assert_eq!(
            group_rejection(&narrow, &minimums, 1, 4, area),
            Some(GROUP_LIMIT)
        );
        // An app-imposed minimum the receiving slot cannot honor is concrete.
        let tight = HashMap::from([(1u32, objc2_foundation::NSSize::new(5000., 5000.))]);
        assert_eq!(
            group_rejection(&desktop, &tight, 1, 4, area),
            Some(MINIMUM_CAPACITY)
        );
        assert_eq!(
            drop_rejection(&desktop, &tight, 1, Some(DropIntent::Group(4)), area),
            MINIMUM_CAPACITY
        );
        assert_eq!(
            drop_rejection(&desktop, &minimums, 1, None, area),
            DETACH_CAPACITY
        );
    }

    #[test]
    fn tab_group_move_preserves_members_and_uses_the_live_limit() {
        let area = rect(-900., 40., 1800., 1200.);
        let desktop = contract_desktop(area)
            .proposed_drop(2, DropIntent::Group(1), area, 10.)
            .unwrap();
        let proposed = tab_group_proposal(&desktop, &HashMap::new(), 2, 4, area).unwrap();
        assert_eq!(proposed.tab_strip(2), Some((&[3, 4, 2][..], 2)));
        assert!(proposed.order.contains(&1));
        assert_eq!(
            proposed
                .window_frames(area, 10.)
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let mut manager = Manager::default();
        manager.desktops.insert((1, 7), proposed.clone());
        manager.set_max_group_tabs(2);
        let current = &manager.desktops[&(1, 7)];
        assert_eq!(
            group_rejection(current, &HashMap::new(), 1, 2, area),
            Some(GROUP_FULL)
        );
        assert_eq!(current.tab_strip(2), proposed.tab_strip(2));
        assert_eq!(current.order, proposed.order);
    }

    #[test]
    fn user_pause_cancels_pending_writes_without_restoring_or_detaching() {
        let mut manager = pending_tab_detach(1);
        let before = manager.desktops.clone();
        let original = manager.windows[&1].original;
        manager.interaction_error = Some(DETACH_CAPACITY.into());
        manager.pause_user();
        assert!(manager.desktops == before);
        assert_eq!(manager.windows[&1].original, original);
        assert!(manager.tab_drag.is_none());
        assert!(manager.pending_drop.is_none());
        assert!(manager.transitions.is_empty());
        assert!(manager.verifications.is_empty());
        assert!(manager.interaction_error.is_none());
    }
}

#[test]
fn restoration_rejects_reused_or_transferred_windows_and_missing_native_evidence() {
    let original = rect(100., 100., 800., 600.);
    let area = rect(0., 0., 1800., 1200.);
    let window = Managed {
        pid: 42,
        space: (1, 7),
        original,
        original_area: area,
        element: ax::application(std::process::id() as i32).unwrap(),
    };
    let identities = HashSet::from([(42, 9)]);
    assert_eq!(
        window.restoration_target(9, Some(&identities), Some(7), Some(9), Some(area)),
        Some(original)
    );
    let reused = HashSet::from([(43, 9)]);
    for (ids, space, ax_id, current) in [
        (None, Some(7), Some(9), Some(area)),
        (Some(&reused), Some(7), Some(9), Some(area)),
        (Some(&identities), None, Some(9), Some(area)),
        (Some(&identities), Some(8), Some(9), Some(area)),
        (Some(&identities), Some(7), None, Some(area)),
        (Some(&identities), Some(7), Some(10), Some(area)),
        (Some(&identities), Some(7), Some(9), None),
    ] {
        assert_eq!(
            window.restoration_target(9, ids, space, ax_id, current),
            None
        );
    }
}
