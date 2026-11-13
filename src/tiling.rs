//! Pure layout state. Window identities are session-local; templates are preferences.
use crate::native::rect;
use objc2_foundation::{NSPoint, NSRect};

pub const TAB_BAR_HEIGHT: f64 = 22.;
pub const TAB_BAR_GAP: f64 = 4.;
pub const GROUP_DROP_FRACTION: f64 = 1. / 3.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Spiral = 1,
    Grid = 2,
    Master = 3,
    ThreeBands = 4,
    FourBands = 5,
    Centered = 6,
    CenteredWide = 7,
    VerticalMaster = 8,
}
impl Layout {
    pub const ALL: [Self; 8] = [
        Self::Spiral,
        Self::Grid,
        Self::Master,
        Self::ThreeBands,
        Self::FourBands,
        Self::Centered,
        Self::CenteredWide,
        Self::VerticalMaster,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Spiral => "Espiral",
            Self::Grid => "Cuadrícula",
            Self::Master => "Principal",
            Self::ThreeBands => "División en 3",
            Self::FourBands => "División en 4",
            Self::Centered => "Principal centrada",
            Self::CenteredWide => "Centrada XL · 3 por lado",
            Self::VerticalMaster => "Principal vertical",
        }
    }
    pub fn screen_name(self, area: NSRect) -> &'static str {
        match (self, area.size.width >= area.size.height) {
            (Self::ThreeBands, true) => "3 columnas",
            (Self::ThreeBands, false) => "3 filas",
            (Self::FourBands, true) => "4 columnas",
            (Self::FourBands, false) => "4 filas",
            _ => self.name(),
        }
    }
    /// Stable preference identifiers; the retired Dwindle (0) migrates to Spiral.
    pub fn from_index(i: isize) -> Self {
        match i {
            2 => Self::Grid,
            3 => Self::Master,
            4 => Self::ThreeBands,
            5 => Self::FourBands,
            6 => Self::Centered,
            7 => Self::CenteredWide,
            8 => Self::VerticalMaster,
            _ => Self::Spiral,
        }
    }
    pub fn example_count(self) -> u32 {
        match self {
            Self::ThreeBands => 3,
            Self::Centered | Self::VerticalMaster => 5,
            Self::CenteredWide => 7,
            _ => 4,
        }
    }
    pub fn short_name(self, area: NSRect) -> &'static str {
        match self {
            Self::Centered => "Centrada",
            Self::CenteredWide => "Centrada XL",
            _ => self.screen_name(area),
        }
    }
    pub fn index(self) -> isize {
        self as isize
    }
    fn centered_axis(self) -> Option<bool> {
        match self {
            Self::Centered | Self::CenteredWide => Some(true),
            Self::VerticalMaster => Some(false),
            _ => None,
        }
    }
    pub fn primary_center_axis(self) -> Option<bool> {
        self.centered_axis()
    }
    pub fn selection_index(self) -> isize {
        Self::ALL.iter().position(|layout| *layout == self).unwrap() as isize
    }
    pub fn from_selection(index: isize) -> Self {
        usize::try_from(index)
            .ok()
            .and_then(|i| Self::ALL.get(i))
            .copied()
            .unwrap_or_default()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}
/// A tabbed group of windows sharing one template slot. `members` is the tab
/// order; `members[active]` is the id owning that slot in `Desktop::order`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub members: Vec<u32>,
    pub active: usize,
}
impl Group {
    pub fn leaf(&self) -> Option<u32> {
        self.members.get(self.active).copied()
    }
    /// Remove `id` and return the tab that now owns the strip's leaf.
    fn take(&mut self, id: u32) -> Option<u32> {
        let index = self.members.iter().position(|w| *w == id)?;
        self.members.remove(index);
        if self.active >= index && self.active > 0 {
            self.active -= 1;
        }
        self.active = self.active.min(self.members.len().saturating_sub(1));
        self.leaf()
    }
    /// An AppKit tab keeps its position in the strip when its window id is
    /// replaced; only the leaf identity in the tree follows. A fresh id that
    /// repeats its own sibling collapses to the one tab owning the slot.
    fn rename(&mut self, old: u32, new: u32) {
        if !self.members.contains(&old) {
            return;
        }
        let mut members: Vec<u32> = Vec::with_capacity(self.members.len());
        let mut active = self.active.min(self.members.len() - 1);
        for (index, id) in self.members.iter().copied().enumerate() {
            let id = if id == old { new } else { id };
            if let Some(existing) = members.iter().position(|w| *w == id) {
                if self.active == index {
                    active = existing;
                }
                continue;
            }
            if self.active == index {
                active = members.len();
            }
            members.push(id);
        }
        self.members = members;
        self.active = active.min(self.members.len().saturating_sub(1));
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropIntent {
    Screen(Zone),
    Window(u32, Zone),
    /// Released in the top third of a slot: join it as a tab.
    Group(u32),
}
/// Global monitor coordinates in points; thresholds scale with the usable area.
pub fn drop_intent(
    area: NSRect,
    frames: &[(u32, NSRect)],
    dragged: u32,
    p: NSPoint,
    smart: bool,
    grouped: bool,
) -> Option<DropIntent> {
    let padded = rect(
        area.origin.x - 12.,
        area.origin.y - 12.,
        area.size.width + 24.,
        area.size.height + 24.,
    );
    if !contains(padded, p) {
        return None;
    }
    // A managed window moves between the template's virtual slots. The native
    // window is under the pointer while dragging, so hit-test the model instead.
    if frames.iter().any(|(id, _)| *id == dragged) {
        let (id, slot) = frames
            .iter()
            .min_by(|(_, a), (_, b)| distance_to_rect(*a, p).total_cmp(&distance_to_rect(*b, p)))?;
        return Some(if grouped && in_top_third(*slot, p) {
            DropIntent::Group(*id)
        } else {
            DropIntent::Window(*id, Zone::Center)
        });
    }
    if smart {
        let band = (area.size.width.min(area.size.height) * 0.08).clamp(28., 80.);
        let distances = [
            p.x - area.origin.x,
            area.origin.x + area.size.width - p.x,
            p.y - area.origin.y,
            area.origin.y + area.size.height - p.y,
        ];
        let index = (0..4).min_by(|a, b| distances[*a].total_cmp(&distances[*b]))?;
        if distances[index] <= band {
            return Some(DropIntent::Screen(
                [Zone::Left, Zone::Right, Zone::Top, Zone::Bottom][index],
            ));
        }
    }
    let target = frames
        .iter()
        .filter(|(id, _)| *id != dragged)
        .find(|(_, r)| contains(*r, p))?;
    Some(if !smart {
        DropIntent::Window(target.0, Zone::Center)
    } else if grouped && in_top_third(target.1, p) {
        DropIntent::Group(target.0)
    } else {
        DropIntent::Window(target.0, zone(target.1, p))
    })
}
/// The top third of a slot groups; the lower two thirds take its place.
fn in_top_third(r: NSRect, p: NSPoint) -> bool {
    contains(r, p) && (p.y - r.origin.y) / r.size.height < GROUP_DROP_FRACTION
}
fn distance_to_rect(r: NSRect, p: NSPoint) -> f64 {
    let dx = (r.origin.x - p.x)
        .max(0.)
        .max(p.x - r.origin.x - r.size.width);
    let dy = (r.origin.y - p.y)
        .max(0.)
        .max(p.y - r.origin.y - r.size.height);
    dx * dx + dy * dy
}
/// Convert AppKit's bottom-left coordinates to AX's global top-left points.
/// The primary screen height is the origin reference, not the target height.
pub fn ax_screen_rect(screen: NSRect, primary_height: f64) -> NSRect {
    rect(
        screen.origin.x,
        primary_height - screen.origin.y - screen.size.height,
        screen.size.width,
        screen.size.height,
    )
}
pub fn zone(r: NSRect, p: NSPoint) -> Zone {
    let x = (p.x - r.origin.x) / r.size.width;
    let y = (p.y - r.origin.y) / r.size.height;
    if (0.28..=0.72).contains(&x) && (0.28..=0.72).contains(&y) {
        return Zone::Center;
    }
    let distances = [x, 1. - x, y, 1. - y];
    let i = (0..4)
        .min_by(|a, b| distances[*a].total_cmp(&distances[*b]))
        .unwrap();
    [Zone::Left, Zone::Right, Zone::Top, Zone::Bottom][i]
}
pub fn contains(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x
        && p.y >= r.origin.y
        && p.x < r.origin.x + r.size.width
        && p.y < r.origin.y + r.size.height
}
fn split(r: NSRect, vertical: bool, ratio: f64, gap: f64) -> (NSRect, NSRect) {
    if vertical {
        let w = (r.size.width - gap) * ratio;
        (
            rect(r.origin.x, r.origin.y, w, r.size.height),
            rect(
                r.origin.x + w + gap,
                r.origin.y,
                r.size.width - w - gap,
                r.size.height,
            ),
        )
    } else {
        let h = (r.size.height - gap) * ratio;
        (
            rect(r.origin.x, r.origin.y, r.size.width, h),
            rect(
                r.origin.x,
                r.origin.y + h + gap,
                r.size.width,
                r.size.height - h - gap,
            ),
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Empty,
    Leaf(u32),
    Split {
        vertical: bool,
        ratio: f64,
        first: Box<Node>,
        second: Box<Node>,
    },
}
impl Node {
    fn empty_count(&self) -> usize {
        match self {
            Self::Empty => 1,
            Self::Leaf(_) => 0,
            Self::Split { first, second, .. } => first.empty_count() + second.empty_count(),
        }
    }
    fn centered_ratios(&self, axis: bool, primary: u32) -> Option<(f64, f64)> {
        let Self::Split {
            vertical,
            ratio,
            second,
            ..
        } = self
        else {
            return None;
        };
        let Self::Split {
            vertical: inner_axis,
            ratio: inner_ratio,
            first,
            ..
        } = second.as_ref()
        else {
            return None;
        };
        (*vertical == axis
            && *inner_axis == axis
            && matches!(first.as_ref(),Self::Leaf(id) if *id == primary))
        .then_some((*ratio, *inner_ratio))
    }
    fn has(&self, id: u32) -> bool {
        match self {
            Self::Empty => false,
            Self::Leaf(w) => *w == id,
            Self::Split { first, second, .. } => first.has(id) || second.has(id),
        }
    }
    fn insert(&mut self, target: u32, id: u32, z: Zone) -> bool {
        match self {
            Self::Leaf(w) if *w == target => {
                let old = self.clone();
                let new = Self::Leaf(id);
                let before = matches!(z, Zone::Left | Zone::Top);
                *self = Self::Split {
                    vertical: matches!(z, Zone::Left | Zone::Right | Zone::Center),
                    ratio: 0.5,
                    first: Box::new(if before { new.clone() } else { old.clone() }),
                    second: Box::new(if before { old } else { new }),
                };
                true
            }
            Self::Split { first, second, .. } => {
                first.insert(target, id, z) || second.insert(target, id, z)
            }
            _ => false,
        }
    }
    fn insert_spiral(&mut self, target: u32, id: u32, next: Zone) -> bool {
        match self {
            Self::Leaf(w) if *w == target => self.insert(target, id, next),
            Self::Split {
                vertical,
                first,
                second,
                ..
            } => {
                if first.has(target) {
                    first.insert_spiral(target, id, if *vertical { Zone::Top } else { Zone::Right })
                } else {
                    second.insert_spiral(
                        target,
                        id,
                        if *vertical { Zone::Bottom } else { Zone::Left },
                    )
                }
            }
            _ => false,
        }
    }
    fn fit_divisions(&mut self, area: NSRect, gap: f64) {
        self.fit_divisions_with(area, gap, &|_, vertical| if vertical { 120. } else { 90. });
    }
    fn fit_divisions_with(&mut self, area: NSRect, gap: f64, minimum: &impl Fn(u32, bool) -> f64) {
        if let Self::Split {
            vertical,
            ratio,
            first,
            second,
        } = self
        {
            let span = if *vertical {
                area.size.width
            } else {
                area.size.height
            };
            let available = span - gap;
            let low = first.minimum_with(*vertical, gap, minimum);
            let high = available - second.minimum_with(*vertical, gap, minimum);
            // Ancestor divisions can leave a sub-point rounding difference
            // at an exactly fitting minimum. Do not skip the clamp and retain
            // an arbitrary old ratio when high is microscopically below low.
            if low <= high + 0.001 && available > 0. {
                *ratio = (*ratio * available).clamp(low, high.max(low)) / available;
            }
            let (a, b) = split(area, *vertical, *ratio, gap);
            first.fit_divisions_with(a, gap, minimum);
            second.fit_divisions_with(b, gap, minimum);
        }
    }
    fn remove(self, id: u32) -> Option<Self> {
        match self {
            Self::Empty => Some(Self::Empty),
            Self::Leaf(w) => (w != id).then_some(Self::Leaf(w)),
            Self::Split {
                vertical,
                ratio,
                first,
                second,
            } => match (first.remove(id), second.remove(id)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    vertical,
                    ratio,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (a, b) => a.or(b),
            },
        }
    }
    /// Point the leaf of `target` at `id`, leaving every ratio untouched.
    fn rename(&mut self, target: u32, id: u32) {
        match self {
            Self::Empty => {}
            Self::Leaf(w) => {
                if *w == target {
                    *w = id;
                }
            }
            Self::Split { first, second, .. } => {
                first.rename(target, id);
                second.rename(target, id);
            }
        }
    }

    fn remap(&mut self, old: &[u32], new: &[u32]) {
        match self {
            Self::Empty => {}
            Self::Leaf(id) => {
                if let Some(i) = old.iter().position(|w| w == id) {
                    *id = new[i]
                }
            }
            Self::Split { first, second, .. } => {
                first.remap(old, new);
                second.remap(old, new);
            }
        }
    }
    fn snap(&mut self, area: NSRect, screen: NSRect, columns: u32, rows: u32, gap: f64) {
        self.snap_with(area, screen, columns, rows, gap, &|_, vertical| {
            if vertical { 120. } else { 90. }
        });
    }
    fn snap_with(
        &mut self,
        area: NSRect,
        screen: NSRect,
        columns: u32,
        rows: u32,
        gap: f64,
        minimum: &impl Fn(u32, bool) -> f64,
    ) {
        if let Self::Split {
            vertical,
            ratio,
            first,
            second,
        } = self
        {
            let (origin, base, span, step) = if *vertical {
                (
                    area.origin.x,
                    screen.origin.x,
                    area.size.width,
                    screen.size.width / columns as f64,
                )
            } else {
                (
                    area.origin.y,
                    screen.origin.y,
                    area.size.height,
                    screen.size.height / rows as f64,
                )
            };
            let min = first.minimum_with(*vertical, gap, minimum);
            let max = span - gap - second.minimum_with(*vertical, gap, minimum);
            // An ancestor may have snapped to a different span. Re-fit this
            // division in its new container before looking for a grid line;
            // if no line fits, retaining the old ratio can undersize a leaf.
            let available = span - gap;
            if available > 0. && min <= max + 0.001 {
                let current = *ratio * available;
                let fitted = current.clamp(min, max.max(min));
                if (current - fitted).abs() > 0.001 {
                    *ratio = fitted / available;
                }
            }
            if min <= max && step > 0. {
                let center = origin + (span - gap) * *ratio + gap / 2.;
                let low = ((origin + min + gap / 2. - base) / step).ceil();
                let high = ((origin + max + gap / 2. - base) / step).floor();
                if low <= high {
                    let line = ((center - base) / step).round().clamp(low, high);
                    *ratio = (base + line * step - origin - gap / 2.) / (span - gap);
                }
            }
            let (a, b) = split(area, *vertical, *ratio, gap);
            first.snap_with(a, screen, columns, rows, gap, minimum);
            second.snap_with(b, screen, columns, rows, gap, minimum);
        }
    }
    fn encode(&self, order: &[u32], out: &mut Vec<String>) {
        match self {
            Self::Empty => out.push("e".into()),
            Self::Leaf(id) => out.push(format!("w{}", order.iter().position(|w| w == id).unwrap())),
            Self::Split {
                vertical,
                ratio,
                first,
                second,
            } => {
                out.push(format!("{}{}", if *vertical { "v" } else { "h" }, ratio));
                first.encode(order, out);
                second.encode(order, out);
            }
        }
    }
    fn decode(
        tokens: &mut std::str::SplitWhitespace<'_>,
        order: &[u32],
        depth: usize,
    ) -> Option<Self> {
        if depth > 128 {
            return None;
        }
        let token = tokens.next()?;
        if token == "e" {
            return Some(Self::Empty);
        }
        let (kind, value) = token.split_at_checked(1)?;
        if kind == "w" {
            return Some(Self::Leaf(*order.get(value.parse::<usize>().ok()?)?));
        }
        if kind != "v" && kind != "h" {
            return None;
        }
        let ratio = value.parse::<f64>().ok()?;
        if !ratio.is_finite() || !(0.01..=0.99).contains(&ratio) {
            return None;
        }
        Some(Self::branch(
            kind == "v",
            ratio,
            Self::decode(tokens, order, depth + 1)?,
            Self::decode(tokens, order, depth + 1)?,
        ))
    }
    fn branch(vertical: bool, ratio: f64, first: Self, second: Self) -> Self {
        Self::Split {
            vertical,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }
    fn minimum(&self, vertical: bool, gap: f64) -> f64 {
        self.minimum_with(vertical, gap, &|_, axis| if axis { 120. } else { 90. })
    }
    fn minimum_with(&self, vertical: bool, gap: f64, minimum: &impl Fn(u32, bool) -> f64) -> f64 {
        match self {
            Self::Empty => 0.,
            Self::Leaf(id) => minimum(*id, vertical),
            Self::Split {
                vertical: axis,
                first,
                second,
                ..
            } => {
                let (a, b) = (
                    first.minimum_with(vertical, gap, minimum),
                    second.minimum_with(vertical, gap, minimum),
                );
                if *axis == vertical {
                    a + b + gap
                } else {
                    a.max(b)
                }
            }
        }
    }
    fn resize_edge(&mut self, id: u32, edge: Zone, old: f64, desired: f64, area: NSRect, gap: f64) {
        let Self::Split {
            vertical,
            ratio,
            first,
            second,
        } = self
        else {
            return;
        };
        let (a, b) = split(area, *vertical, *ratio, gap);
        let in_first = first.has(id);
        let axis = matches!(edge, Zone::Left | Zone::Right);
        let boundary = if *vertical {
            if in_first {
                a.origin.x + a.size.width
            } else {
                b.origin.x
            }
        } else if in_first {
            a.origin.y + a.size.height
        } else {
            b.origin.y
        };
        let shared = matches!(edge, Zone::Right | Zone::Bottom) == in_first;
        if axis == *vertical && shared && (boundary - old).abs() < 3. {
            let total = if *vertical {
                area.size.width - gap
            } else {
                area.size.height - gap
            };
            let origin = if *vertical {
                area.origin.x
            } else {
                area.origin.y
            };
            let min = first.minimum(*vertical, gap);
            let max = total - second.minimum(*vertical, gap);
            if total > 0. && min <= max {
                *ratio = ((desired - origin - if in_first { 0. } else { gap }).clamp(min, max)
                    / total)
                    .clamp(0.01, 0.99);
            }
        } else if in_first {
            first.resize_edge(id, edge, old, desired, a, gap)
        } else if second.has(id) {
            second.resize_edge(id, edge, old, desired, b, gap)
        }
    }
    fn frames(&self, r: NSRect, gap: f64, out: &mut Vec<(u32, NSRect)>) {
        match self {
            Self::Empty => {}
            Self::Leaf(w) => out.push((*w, r)),
            Self::Split {
                vertical,
                ratio,
                first,
                second,
            } => {
                let (a, b) = split(r, *vertical, *ratio, gap);
                first.frames(a, gap, out);
                second.frames(b, gap, out);
            }
        }
    }
}
#[derive(Clone, PartialEq)]
pub struct Desktop {
    pub layout: Layout,
    pub order: Vec<u32>,
    pub grid: Option<(u32, u32)>,
    pub max_group_tabs: usize,
    resized: Option<(Layout, Vec<u32>, Node)>,
    minimums: std::collections::HashMap<u32, objc2_foundation::NSSize>,
    /// Tabbed groups, in creation order. Only `members[active]` owns a slot in
    /// `order`, so the tree keeps exactly one leaf per group.
    groups: Vec<Group>,
}
impl Default for Desktop {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            order: Vec::new(),
            grid: None,
            max_group_tabs: 5,
            resized: None,
            minimums: Default::default(),
            groups: Vec::new(),
        }
    }
}
impl Desktop {
    pub fn configured(layout: Layout, grid: Option<(u32, u32)>) -> Self {
        Self {
            layout,
            grid,
            ..Default::default()
        }
    }
    /// Selecting an AppKit tab changes its NSWindow ID, not its layout slot.
    /// Both the strip entry and, when the id owned the slot, the tree leaf
    /// follow the new id, so a native tab switch never leaves a freed id.
    pub fn replace_window(&mut self, old: u32, new: u32) -> bool {
        let owns_slot = self.order.contains(&old);
        let grouped = self.groups.iter().any(|group| group.members.contains(&old));
        if !owns_slot && !grouped {
            return false;
        }
        // Strict identity: any other owner of `new` is another window. Only
        // the id being replaced may repeat, and then inside its own strip.
        let clash = new != old
            && (self.order.contains(&new)
                || self
                    .groups
                    .iter()
                    .any(|group| group.members.contains(&new) && !group.members.contains(&old)));
        if clash {
            return false;
        }
        if let Some(minimum) = self.minimums.remove(&old) {
            self.minimums.insert(new, minimum);
        }
        for group in &mut self.groups {
            group.rename(old, new);
        }
        self.groups.retain(|group| !group.members.is_empty());
        if !owns_slot {
            // A hidden tab never owned a leaf: the tree is untouched.
            return true;
        }
        for id in &mut self.order {
            if *id == old {
                *id = new;
            }
        }
        if let Some((_, order, tree)) = &mut self.resized {
            let before = order.clone();
            for id in order.iter_mut() {
                if *id == old {
                    *id = new;
                }
            }
            tree.remap(&before, order);
        }
        true
    }
    /// Drop a window. When it was the visible tab of a group, its sibling is
    /// promoted into the very same slot and returned, so the tree is renamed
    /// instead of collapsed and the other proportions survive.
    pub fn remove(&mut self, id: u32) -> Option<u32> {
        self.minimums.remove(&id);
        let old = self.order.clone();
        let Some(position) = self.order.iter().position(|w| *w == id) else {
            self.prune_member(id);
            return None;
        };
        self.order.retain(|w| *w != id);
        let promoted = self
            .groups
            .iter_mut()
            .find(|group| group.members.contains(&id))
            .and_then(|group| group.take(id));
        // A strip that lost its last tab is inert; drop it so no stale index
        // or phantom member survives the closure.
        self.groups.retain(|group| !group.members.is_empty());
        if let Some(next) = promoted {
            self.order.insert(position, next);
            self.remap_resized(&old);
            return Some(next);
        }
        if self.layout.centered_axis().is_some() {
            // Keep the old outer ratios as a reference. A changed membership no
            // longer matches `resized`, so base_tree rebuilds both sidebars.
            return None;
        }
        // Collapse only the vanished leaf's parent. Unrelated divisions retain
        // their user-defined proportions, across all templates.
        self.resized = self.resized.take().and_then(|(layout, order, tree)| {
            if order != old {
                return None;
            }
            tree.remove(id)
                .map(|tree| (layout, self.order.clone(), tree))
        });
        None
    }
    fn prune_member(&mut self, id: u32) {
        for group in &mut self.groups {
            if group.members.contains(&id) {
                // Same index arithmetic as `Group::take`: removing a member
                // before the visible one shifts `active` down, so the leaf
                // keeps naming the id that really owns the slot.
                group.take(id);
                break;
            }
        }
        self.groups.retain(|group| !group.members.is_empty());
    }
    /// Tabbed groups on this desktop, in creation order.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }
    /// Point each strip's leaf at the member the compositor really shows, so a
    /// native tab switch never leaves an offscreen id owning the slot.
    pub fn sync_tabs(&mut self, visible: &[u32]) {
        for index in 0..self.groups.len() {
            let Some(active) = self.groups[index].leaf() else {
                continue;
            };
            if visible.contains(&active) {
                continue;
            }
            if let Some(next) = self.groups[index]
                .members
                .iter()
                .copied()
                .find(|w| *w != active && visible.contains(w))
            {
                self.select_tab(next);
            }
        }
    }
    /// `Some(true)` for the visible tab, `Some(false)` for a hidden one.
    pub fn tab_role(&self, id: u32) -> Option<bool> {
        let group = self
            .groups
            .iter()
            .find(|group| group.members.contains(&id))?;
        let index = group.members.iter().position(|w| *w == id)?;
        Some(index == group.active)
    }
    /// True when `id` shares another window's slot without owning it.
    pub fn is_hidden_tab(&self, id: u32) -> bool {
        self.tab_role(id) == Some(false)
    }
    /// The strip to draw above `id`'s slot: tab order plus visible index.
    pub fn tab_strip(&self, id: u32) -> Option<(&[u32], usize)> {
        let group = self
            .groups
            .iter()
            .find(|group| group.members.contains(&id))?;
        Some((&group.members, group.active))
    }
    /// Reorder one strip without changing its active window or layout leaf.
    /// `index` is the final position, not an insertion gap in the old order.
    pub fn reorder_tab(&mut self, id: u32, index: usize) -> bool {
        let Some(group) = self.groups.iter_mut().find(|g| g.members.contains(&id)) else {
            return false;
        };
        if index >= group.members.len() {
            return false;
        }
        let from = group
            .members
            .iter()
            .position(|member| *member == id)
            .unwrap();
        if from < index {
            group.members[from..=index].rotate_left(1);
            if group.active > from && group.active <= index {
                group.active -= 1;
            } else if group.active == from {
                group.active = index;
            }
        } else if from > index {
            group.members[index..=from].rotate_right(1);
            if group.active >= index && group.active < from {
                group.active += 1;
            } else if group.active == from {
                group.active = index;
            }
        }
        true
    }
    /// Make `id` the visible tab. Returns the previously visible id so the
    /// caller can retire its cached geometry.
    pub fn select_tab(&mut self, id: u32) -> Option<u32> {
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.members.contains(&id))?;
        let index = group.members.iter().position(|w| *w == id)?;
        if index == group.active {
            return Some(id);
        }
        let previous = group.leaf()?;
        let position = self.order.iter().position(|w| *w == previous)?;
        group.active = index;
        let old = self.order.clone();
        self.order[position] = id;
        self.remap_resized(&old);
        Some(previous)
    }
    /// Attach `id` to `target`'s strip, or start a new two-tab group. The
    /// dropped window becomes the visible tab and owns the single leaf.
    fn join_group(&mut self, id: u32, target: u32) {
        if id == target || !self.order.contains(&target) {
            return;
        }
        let old = self.order.clone();
        let valid = self
            .resized
            .as_ref()
            .is_some_and(|(layout, order, _)| *layout == self.layout && *order == old);
        if self
            .groups
            .iter()
            .any(|group| group.members.contains(&id) && group.members.contains(&target))
        {
            self.select_tab(id);
            return;
        }
        let owned_slot = self.order.contains(&id);
        let mut sibling = None;
        if let Some(from) = self
            .groups
            .iter()
            .position(|group| group.members.contains(&id))
        {
            sibling = self.groups[from].take(id);
            if self.groups[from].members.is_empty() {
                self.groups.remove(from);
            }
        }
        match self
            .groups
            .iter()
            .position(|group| group.members.contains(&target))
        {
            Some(index) => {
                let group = &mut self.groups[index];
                group.members.push(id);
                group.active = group.members.len() - 1;
            }
            None => self.groups.push(Group {
                members: vec![target, id],
                active: 1,
            }),
        }
        let mut order = old;
        if let Some(next) = sibling {
            // The promoted sibling keeps its own strip's slot.
            if let Some(position) = order.iter().position(|w| *w == id) {
                order[position] = next;
                if valid {
                    if let Some((_, _, tree)) = &mut self.resized {
                        tree.rename(id, next);
                    }
                }
            }
        } else if owned_slot {
            // A lone member of its own strip: free that slot like a closure.
            if let Some(position) = order.iter().position(|w| *w == id) {
                order.remove(position);
                if valid {
                    if let Some((_, _, tree)) = &mut self.resized {
                        let previous = std::mem::replace(tree, Node::Empty);
                        *tree = previous.remove(id).unwrap_or(Node::Empty);
                    }
                }
            }
        }
        if let Some(position) = order.iter().position(|w| *w == target) {
            order[position] = id;
            if valid {
                if let Some((_, _, tree)) = &mut self.resized {
                    tree.rename(target, id);
                }
            }
        }
        self.order = order;
        if valid && let Some((_, order, _)) = &mut self.resized {
            order.clone_from(&self.order);
        }
    }
    fn remap_resized(&mut self, old: &[u32]) {
        if let Some((layout, order, tree)) = &mut self.resized
            && *layout == self.layout
            && *order == old
        {
            tree.remap(old, &self.order);
            *order = self.order.clone();
        }
    }
    #[cfg(test)]
    pub fn reconcile(&mut self, ids: &[u32], area: NSRect, gap: f64) {
        self.reconcile_at(ids, area, gap, None);
    }
    /// Focus only affects insertion into a customized, non-centered template.
    /// Avoid synchronous AX focus reads when reconciling an unchanged desktop.
    pub fn needs_insertion_focus(&self, ids: &[u32]) -> bool {
        self.layout != Layout::Spiral
            && self.layout.primary_center_axis().is_none()
            && self.resized.is_some()
            && ids
                .iter()
                .any(|id| !self.order.contains(id) && self.tab_role(*id).is_none())
    }
    pub fn reconcile_at(&mut self, ids: &[u32], area: NSRect, gap: f64, focused: Option<u32>) {
        self.sync_tabs(ids);
        let mut index = 0;
        while index < self.order.len() {
            let id = self.order[index];
            if ids.contains(&id) {
                index += 1;
            } else {
                // Removal can promote another absent member into this index.
                // Recheck it instead of leaving a phantom leaf for another scan.
                self.remove(id);
            }
        }
        for id in ids {
            if self.order.contains(id) {
                continue;
            }
            // A hidden tab already owns its group's leaf; it is not an arrival.
            if self.groups.iter().any(|group| group.members.contains(id)) {
                continue;
            }
            if self.layout.centered_axis().is_some() {
                self.order.push(*id);
                continue;
            }
            let frames = self.frames(area, gap);
            let target = frames
                .iter()
                .find(|(id, _)| {
                    Some(*id)
                        == if self.layout == Layout::Spiral {
                            self.order.last().copied()
                        } else {
                            focused
                        }
                })
                .or_else(|| frames.last())
                .copied();
            if let (Some((layout, order, tree)), Some((target, r))) = (&mut self.resized, target) {
                if *layout == self.layout && *order == self.order {
                    if *layout == Layout::Spiral {
                        tree.insert_spiral(target, *id, Zone::Right);
                    } else {
                        tree.insert(
                            target,
                            *id,
                            if r.size.width >= r.size.height {
                                Zone::Right
                            } else {
                                Zone::Bottom
                            },
                        );
                    }

                    order.push(*id);
                }
            }
            self.order.push(*id);
        }
    }
    pub fn place(&mut self, id: u32, target: u32, z: Zone) {
        if id == target || !self.order.contains(&target) {
            return;
        }
        if self.order.contains(&id) {
            let old = self.order.clone();
            let a = self.order.iter().position(|w| *w == id).unwrap();
            if z == Zone::Center {
                let b = self.order.iter().position(|w| *w == target).unwrap();
                self.order.swap(a, b);
            } else {
                self.order.remove(a);
                let b = self.order.iter().position(|w| *w == target).unwrap()
                    + usize::from(matches!(z, Zone::Right | Zone::Bottom));
                self.order.insert(b, id);
            }
            if let Some((layout, order, tree)) = &mut self.resized {
                if *layout == self.layout && *order == old {
                    tree.remap(&old, &self.order);
                    *order = self.order.clone();
                }
            }
            return;
        }
        self.remove(id);
        let i = self.order.iter().position(|w| *w == target).unwrap()
            + usize::from(matches!(z, Zone::Right | Zone::Bottom | Zone::Center));
        self.order.insert(i, id);
    }
    pub fn proposed_drop(
        &self,
        id: u32,
        intent: DropIntent,
        area: NSRect,
        gap: f64,
    ) -> Option<Self> {
        if let DropIntent::Group(target) = intent {
            if !self.order.contains(&target) {
                return None;
            }
            if let Some((members, _)) = self.tab_strip(target) {
                // Selecting an existing member is not admission, even after
                // the preference has been lowered below the group's size.
                if members.contains(&id) {
                    let mut result = self.clone();
                    result.select_tab(id)?;
                    return Some(result);
                }
                if members.len() >= self.max_group_tabs {
                    return None;
                }
            } else if self.max_group_tabs < 2 {
                return None;
            }
        }
        let mut result = self.clone();
        if !matches!(intent, DropIntent::Group(_)) && self.tab_role(id).is_some() {
            let minimum = result.minimums.get(&id).copied();
            if result
                .tab_strip(id)
                .is_some_and(|(members, _)| members.len() > 1)
            {
                result.remove(id);
            } else {
                result.prune_member(id);
            }
            if let Some(minimum) = minimum {
                result.minimums.insert(id, minimum);
            }
            // This member is no longer grouped. Treat its new independent slot
            // exactly like an arrival; reconciliation must not skip it forever.
            return result.proposed_drop(id, intent, area, gap);
        }
        if self.order.contains(&id) {
            let frames = self.frames(area, gap);
            if frames.len() != self.order.len() {
                return None;
            }
            let target = match intent {
                DropIntent::Window(target, _) => target,
                DropIntent::Group(target) => {
                    if !self.order.contains(&target) || target == id {
                        return None;
                    }
                    result.resized =
                        Some((self.layout, self.order.clone(), self.frame_tree(area, gap)?));
                    result.join_group(id, target);
                    return (result.frames(area, gap).len() == result.order.len())
                        .then_some(result);
                }
                DropIntent::Screen(side) => {
                    let p = match side {
                        Zone::Left => {
                            NSPoint::new(area.origin.x, area.origin.y + area.size.height / 2.)
                        }
                        Zone::Right => NSPoint::new(
                            area.origin.x + area.size.width,
                            area.origin.y + area.size.height / 2.,
                        ),
                        Zone::Top => {
                            NSPoint::new(area.origin.x + area.size.width / 2., area.origin.y)
                        }
                        Zone::Bottom | Zone::Center => NSPoint::new(
                            area.origin.x + area.size.width / 2.,
                            area.origin.y + area.size.height,
                        ),
                    };
                    frames
                        .iter()
                        .min_by(|(_, a), (_, b)| {
                            distance_to_rect(*a, p).total_cmp(&distance_to_rect(*b, p))
                        })?
                        .0
                }
            };
            if !self.order.contains(&target) {
                return None;
            }
            // Start with the actual virtual slots, including snapped divisions.
            // Keep them exact whenever they fit. If a receiving app needs more
            // space, the existing minimum-size solver can move shared divisions
            // instead of silently disabling an otherwise feasible destination.
            let mut tree = self.frame_tree(area, gap)?;
            result.place(id, target, Zone::Center);
            tree.remap(&self.order, &result.order);
            result.resized = Some((result.layout, result.order.clone(), tree));
            return (result.frames(area, gap).len() == result.order.len()).then_some(result);
        }
        if !self.order.is_empty() && self.effective_tree(area, gap).is_none() {
            return None;
        }
        if let DropIntent::Group(target) = intent {
            if !self.order.contains(&target) || target == id {
                return None;
            }
            result.resized = Some((self.layout, self.order.clone(), self.frame_tree(area, gap)?));
            result.join_group(id, target);
            return (result.frames(area, gap).len() == result.order.len()).then_some(result);
        }
        // A cross-display arrival uses the destination's membership policy,
        // then occupies a template slot. Free-form edge insertion could turn
        // a centered/vertical template into an unrelated split tree.
        let mut ids = result.order.clone();
        ids.push(id);
        result.reconcile_at(&ids, area, gap, None);
        let frames = result.frames(area, gap);
        if frames.len() != result.order.len()
            || frames
                .iter()
                .any(|(_, r)| r.size.width < 120. || r.size.height < 90.)
        {
            return None;
        }
        if self.order.is_empty() {
            Some(result)
        } else {
            result.proposed_drop(id, intent, area, gap)
        }
    }
    fn equal_chain(nodes: &[Node], vertical: bool, span: f64, gap: f64) -> Node {
        if nodes.len() == 1 {
            return nodes[0].clone();
        }
        let size = (span - gap * (nodes.len() - 1) as f64) / nodes.len() as f64;
        Node::branch(
            vertical,
            size / (span - gap),
            nodes[0].clone(),
            Self::equal_chain(&nodes[1..], vertical, span - size - gap, gap),
        )
    }
    fn spiral(ids: &[u32], depth: usize) -> Node {
        if ids.len() == 1 {
            return Node::Leaf(ids[0]);
        }
        let leaf = Node::Leaf(ids[0]);
        let rest = Self::spiral(&ids[1..], depth + 1);
        if depth % 4 >= 2 {
            Node::branch(depth % 2 == 0, 0.5, rest, leaf)
        } else {
            Node::branch(depth % 2 == 0, 0.5, leaf, rest)
        }
    }
    fn effective_tree(&self, area: NSRect, gap: f64) -> Option<Node> {
        let mut tree = self.base_tree(area, gap)?;
        if !self.center_tree(&mut tree, area, gap, None, None) {
            return None;
        }
        if !self.minimums.is_empty() || self.groups.iter().any(|group| group.members.len() > 1) {
            let minimum = |id, vertical| self.window_minimum(id, vertical);
            if tree.minimum_with(true, gap, &minimum) > area.size.width + 0.001
                || tree.minimum_with(false, gap, &minimum) > area.size.height + 0.001
            {
                return None;
            }
            tree.fit_divisions_with(area, gap, &minimum);
        }
        Some(tree)
    }
    fn center_tree(
        &self,
        tree: &mut Node,
        area: NSRect,
        gap: f64,
        desired: Option<f64>,
        grid: Option<(u32, u32)>,
    ) -> bool {
        let Some(axis) = self.layout.centered_axis() else {
            return true;
        };
        if self.order.len() < 2 {
            return true;
        }
        let Node::Split {
            vertical,
            ratio,
            first,
            second,
        } = tree
        else {
            return false;
        };
        let Node::Split {
            vertical: inner_axis,
            ratio: inner_ratio,
            first: main,
            second: after,
        } = second.as_mut()
        else {
            return false;
        };
        if *vertical != axis || *inner_axis != axis {
            return false;
        }
        let span = if axis {
            area.size.width
        } else {
            area.size.height
        };
        let minimum = |id, vertical| self.window_minimum(id, vertical);
        let side_min = first
            .minimum_with(axis, gap, &minimum)
            .max(after.minimum_with(axis, gap, &minimum));
        let main_min = main.minimum_with(axis, gap, &minimum);
        let main_max = span - 2. * gap - 2. * side_min;
        if main_max + 0.001 < main_min {
            return false;
        }
        let rest = span - (span - gap) * *ratio - gap;
        let width = ((desired.unwrap_or((rest - gap) * *inner_ratio) * 1_000_000.).round()
            / 1_000_000.)
            .clamp(main_min, main_max.max(main_min));
        let mut side = (span - width - 2. * gap) / 2.;
        if let Some((cols, rows)) = grid {
            let step = span / if axis { cols } else { rows } as f64;
            let low = ((side_min + gap / 2.) / step).ceil();
            let high = ((span - main_min - gap) / (2. * step)).floor();
            if low <= high {
                side = ((side + gap / 2.) / step).round().clamp(low, high) * step - gap / 2.;
            }
        }
        *ratio = side / (span - gap);
        let rest = span - side - gap;
        *inner_ratio = (span - 2. * side - 2. * gap) / (rest - gap);
        // Re-centering changes the sidebar containers. Re-fit their internal
        // divisions in those final rectangles; ratios fitted before symmetry
        // can otherwise leave a leaf below its minimum after a slot exchange.
        let (before_area, remaining) = split(area, axis, *ratio, gap);
        let (_, after_area) = split(remaining, axis, *inner_ratio, gap);
        first.fit_divisions_with(before_area, gap, &minimum);
        after.fit_divisions_with(after_area, gap, &minimum);
        if let Some((cols, rows)) = grid {
            first.snap_with(
                before_area,
                area,
                cols.clamp(2, 240),
                rows.clamp(2, 60),
                gap,
                &minimum,
            );
            after.snap_with(
                after_area,
                area,
                cols.clamp(2, 240),
                rows.clamp(2, 60),
                gap,
                &minimum,
            );
        }
        true
    }
    fn window_minimum(&self, id: u32, vertical: bool) -> f64 {
        let fallback: f64 = if vertical { 120. } else { 90. };
        let minimum = |member| {
            self.minimums.get(&member).map_or(fallback, |size| {
                fallback.max(if vertical { size.width } else { size.height })
            })
        };
        self.tab_strip(id).map_or_else(
            || minimum(id),
            |(members, _)| {
                members
                    .iter()
                    .map(|id| minimum(*id))
                    .fold(fallback, f64::max)
                    + if !vertical && members.len() > 1 {
                        TAB_BAR_HEIGHT + TAB_BAR_GAP
                    } else {
                        0.
                    }
            },
        )
    }
    pub fn set_minimum_sizes(
        &mut self,
        minimums: &std::collections::HashMap<u32, objc2_foundation::NSSize>,
    ) {
        let tracked = |id: &u32| {
            self.order.contains(id) || self.groups.iter().any(|group| group.members.contains(id))
        };
        self.minimums
            .retain(|id, _| tracked(id) && minimums.contains_key(id));
        for (id, size) in minimums {
            if tracked(id) {
                self.minimums.insert(*id, *size);
            }
        }
    }
    fn base_tree(&self, area: NSRect, gap: f64) -> Option<Node> {
        if self.order.is_empty() {
            return None;
        }
        if let Some((layout, order, tree)) = &self.resized {
            if *layout == self.layout
                && *order == self.order
                && self.layout.centered_axis().is_none_or(|axis| {
                    self.order.len() < 2 || tree.centered_ratios(axis, self.order[0]).is_some()
                })
            {
                return Some(tree.clone());
            }
        }
        let leaves: Vec<_> = self.order.iter().map(|id| Node::Leaf(*id)).collect();
        Some(match self.layout {
            Layout::Centered | Layout::CenteredWide | Layout::VerticalMaster
                if leaves.len() > 1 =>
            {
                let horizontal = self.layout != Layout::VerticalMaster;
                let before: Vec<_> = leaves.iter().skip(1).step_by(2).cloned().collect();
                let after: Vec<_> = leaves.iter().skip(2).step_by(2).cloned().collect();
                let span = if horizontal {
                    area.size.width
                } else {
                    area.size.height
                };
                let cross_span = if horizontal {
                    area.size.height
                } else {
                    area.size.width
                };
                let side_span = (span / 6. - gap).max(1.);
                let side = |nodes: &[Node], side_span: f64| {
                    if nodes.is_empty() {
                        return Node::Empty;
                    }
                    if !horizontal {
                        return Self::equal_chain(nodes, true, cross_span, gap);
                    }
                    let rows = nodes.len().min(if self.layout == Layout::CenteredWide {
                        3
                    } else {
                        2
                    });
                    let mut bands = Vec::with_capacity(rows);
                    let mut start = 0;
                    for row in 0..rows {
                        let count = nodes.len() / rows + usize::from(row < nodes.len() % rows);
                        bands.push(Self::equal_chain(
                            &nodes[start..start + count],
                            true,
                            side_span,
                            gap,
                        ));
                        start += count;
                    }
                    Self::equal_chain(&bands, false, cross_span, gap)
                };
                {
                    let rest_span = span - side_span - gap;
                    let defaults = (
                        side_span / (span - gap),
                        (rest_span - side_span - gap) / (rest_span - gap),
                    );
                    let (outer, inner) = self
                        .resized
                        .as_ref()
                        .filter(|(layout, order, _)| *layout == self.layout && order.len() >= 2)
                        .and_then(|(_, order, tree)| tree.centered_ratios(horizontal, order[0]))
                        .unwrap_or(defaults);
                    let before_span = (span - gap) * outer;
                    let remaining = span - before_span - gap;
                    let after_span = (remaining - gap) * (1. - inner);
                    Node::branch(
                        horizontal,
                        outer,
                        side(&before, before_span),
                        Node::branch(
                            horizontal,
                            inner,
                            leaves[0].clone(),
                            side(&after, after_span),
                        ),
                    )
                }
            }
            Layout::Spiral => {
                Self::spiral(&self.order, usize::from(area.size.height > area.size.width))
            }
            Layout::Master if leaves.len() > 1 => Node::branch(
                area.size.width >= area.size.height,
                0.6,
                leaves[0].clone(),
                Self::equal_chain(
                    &leaves[1..],
                    area.size.height > area.size.width,
                    if area.size.width >= area.size.height {
                        area.size.height
                    } else {
                        area.size.width
                    },
                    gap,
                ),
            ),
            Layout::ThreeBands | Layout::FourBands => {
                let count = leaves.len().min(if self.layout == Layout::ThreeBands {
                    3
                } else {
                    4
                });
                let vertical = area.size.width >= area.size.height;
                let mut bands = Vec::with_capacity(count);
                let mut start = 0;
                for index in 0..count {
                    let length = leaves.len() / count + usize::from(index < leaves.len() % count);
                    bands.push(Self::equal_chain(
                        &leaves[start..start + length],
                        !vertical,
                        if vertical {
                            area.size.height
                        } else {
                            area.size.width
                        },
                        gap,
                    ));
                    start += length;
                }
                Self::equal_chain(
                    &bands,
                    vertical,
                    if vertical {
                        area.size.width
                    } else {
                        area.size.height
                    },
                    gap,
                )
            }
            Layout::Grid => {
                let cols = (leaves.len() as f64).sqrt().ceil() as usize;
                let rows: Vec<_> = leaves
                    .chunks(cols)
                    .map(|row| Self::equal_chain(row, true, area.size.width, gap))
                    .collect();
                Self::equal_chain(&rows, false, area.size.height, gap)
            }
            _ => leaves[0].clone(),
        })
    }
    /// Move shared boundaries, preserving the outer screen rectangle and gaps.
    /// `from` and `to` must belong to the same gesture baseline to avoid drift.
    pub fn resize(&mut self, id: u32, from: NSRect, to: NSRect, area: NSRect, gap: f64) {
        if ![
            from.origin.x,
            from.origin.y,
            from.size.width,
            from.size.height,
            to.origin.x,
            to.origin.y,
            to.size.width,
            to.size.height,
        ]
        .iter()
        .all(|n| n.is_finite())
        {
            return;
        }
        let Some(mut tree) = self.effective_tree(area, gap) else {
            return;
        };
        if !tree.has(id) {
            return;
        }
        if let Some((cols, rows)) = self.grid {
            tree.snap(area, area, cols.clamp(2, 240), rows.clamp(2, 60), gap);
        }
        if !self.center_tree(&mut tree, area, gap, None, self.grid) {
            return;
        }
        let centered_span = self.layout.centered_axis().and_then(|axis| {
            let mut slots = vec![];
            tree.frames(area, gap, &mut slots);
            let primary = slots
                .iter()
                .find(|(window, _)| Some(*window) == self.order.first().copied())?
                .1;
            let (start, end, old_start, old_end, new_start, new_end, main_start, main_end) = if axis
            {
                (
                    area.origin.x,
                    area.origin.x + area.size.width,
                    from.origin.x,
                    from.origin.x + from.size.width,
                    to.origin.x,
                    to.origin.x + to.size.width,
                    primary.origin.x,
                    primary.origin.x + primary.size.width,
                )
            } else {
                (
                    area.origin.y,
                    area.origin.y + area.size.height,
                    from.origin.y,
                    from.origin.y + from.size.height,
                    to.origin.y,
                    to.origin.y + to.size.height,
                    primary.origin.y,
                    primary.origin.y + primary.size.height,
                )
            };
            let (left, right) = (new_start - old_start, new_end - old_end);
            let width = main_end - main_start;
            if self.order.first() == Some(&id) {
                Some(if left.abs() > right.abs() {
                    width - 2. * left
                } else {
                    width + 2. * right
                })
            } else if old_end <= main_start + 0.001 {
                Some(
                    width
                        - 2. * if right.abs() > 0.001 {
                            right
                        } else if (old_start - start).abs() < 3. {
                            -left
                        } else {
                            0.
                        },
                )
            } else if old_start >= main_end - 0.001 {
                Some(
                    width
                        + 2. * if left.abs() > 0.001 {
                            left
                        } else if (old_end - end).abs() < 3. {
                            -right
                        } else {
                            0.
                        },
                )
            } else {
                None
            }
        });
        for (edge, old, new) in [
            (Zone::Left, from.origin.x, to.origin.x),
            (
                Zone::Right,
                from.origin.x + from.size.width,
                to.origin.x + to.size.width,
            ),
            (Zone::Top, from.origin.y, to.origin.y),
            (
                Zone::Bottom,
                from.origin.y + from.size.height,
                to.origin.y + to.size.height,
            ),
        ] {
            if (old - new).abs() > 2. {
                let outer = match edge {
                    Zone::Left => area.origin.x,
                    Zone::Right => area.origin.x + area.size.width,
                    Zone::Top => area.origin.y,
                    Zone::Bottom => area.origin.y + area.size.height,
                    Zone::Center => old,
                };
                if (old - outer).abs() < 3. {
                    // A screen-edge handle still resizes its tile: move the
                    // opposite shared division while anchoring the screen edge.
                    let (opposite, boundary) = match edge {
                        Zone::Left => (Zone::Right, from.origin.x + from.size.width),
                        Zone::Right => (Zone::Left, from.origin.x),
                        Zone::Top => (Zone::Bottom, from.origin.y + from.size.height),
                        Zone::Bottom => (Zone::Top, from.origin.y),
                        Zone::Center => continue,
                    };
                    tree.resize_edge(id, opposite, boundary, boundary - (new - old), area, gap);
                } else {
                    tree.resize_edge(id, edge, old, new, area, gap);
                }
            }
        }
        if !self.center_tree(&mut tree, area, gap, centered_span, self.grid) {
            return;
        }
        self.resized = Some((self.layout, self.order.clone(), tree));
    }
    pub fn save_distribution(&self, area: NSRect) -> Option<String> {
        let tree = self.effective_tree(area, 10.)?;
        let mut tokens = vec![
            "2".into(),
            self.layout.index().to_string(),
            self.order.len().to_string(),
            self.grid.map_or(0, |g| g.0).to_string(),
            self.grid.map_or(0, |g| g.1).to_string(),
        ];
        tree.encode(&self.order, &mut tokens);
        Some(tokens.join(" "))
    }
    pub fn restore_distribution(&mut self, text: &str) -> bool {
        let mut tokens = text.split_whitespace();
        if tokens.next() != Some("2") {
            return false;
        }
        let Some(index) = tokens
            .next()
            .and_then(|s| s.parse::<isize>().ok())
            .filter(|i| (1..=8).contains(i))
        else {
            return false;
        };
        if tokens.next().and_then(|s| s.parse::<usize>().ok()) != Some(self.order.len()) {
            return false;
        }
        let grid = match (
            tokens.next().and_then(|s| s.parse::<u32>().ok()),
            tokens.next().and_then(|s| s.parse::<u32>().ok()),
        ) {
            (Some(0), Some(0)) => None,
            (Some(c), Some(r)) if (2..=240).contains(&c) && (2..=60).contains(&r) => Some((c, r)),
            _ => return false,
        };
        let Some(tree) = Node::decode(&mut tokens, &self.order, 0) else {
            return false;
        };
        if tokens.next().is_some() {
            return false;
        }
        let mut leaves = vec![];
        tree.frames(rect(0., 0., 1000., 1000.), 0., &mut leaves);
        let mut ids: Vec<_> = leaves.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        let mut expected = self.order.clone();
        expected.sort_unstable();
        if ids != expected {
            return false;
        }
        let layout = Layout::from_index(index);
        if layout.primary_center_axis().is_some_and(|axis| {
            self.order.len() >= 2 && tree.centered_ratios(axis, self.order[0]).is_none()
        }) || tree.empty_count()
            != usize::from(layout.primary_center_axis().is_some() && self.order.len() == 2)
        {
            return false;
        }
        self.grid = grid;
        self.layout = layout;
        self.resized = Some((self.layout, self.order.clone(), tree));
        true
    }
    /// Preserve the chosen topology. Only clamp divisions that became too small;
    /// never silently replace a spiral with a grid.
    pub fn fit(&mut self, area: NSRect, gap: f64) {
        if !self
            .frames(area, gap)
            .iter()
            .any(|(_, r)| r.size.width < 80. || r.size.height < 60.)
        {
            return;
        }
        let Some(mut tree) = self.effective_tree(area, gap) else {
            return;
        };
        if tree.minimum(true, gap) > area.size.width || tree.minimum(false, gap) > area.size.height
        {
            return;
        }
        tree.fit_divisions(area, gap);
        self.resized = Some((self.layout, self.order.clone(), tree));
    }
    pub fn reset_template(&mut self, layout: Layout, area: NSRect) {
        // Reset geometry, not membership: grouped leaves must keep their owner,
        // and the configured admission limit is independent of the template.
        self.layout = layout;
        self.resized = None;
        self.fit(area, 10.);
    }
    pub fn frames(&self, r: NSRect, gap: f64) -> Vec<(u32, NSRect)> {
        let mut out = vec![];
        if let Some(tree) = self.frame_tree(r, gap) {
            tree.frames(r, gap.clamp(0., 24.), &mut out);
        }
        out
    }
    fn header_height(&self, id: u32) -> f64 {
        if self
            .tab_strip(id)
            .is_some_and(|(members, _)| members.len() > 1)
        {
            TAB_BAR_HEIGHT + TAB_BAR_GAP
        } else {
            0.
        }
    }
    /// Physical AX rectangle; the logical slot also contains the tab header.
    pub fn window_frame(&self, id: u32, mut slot: NSRect) -> NSRect {
        let header = self.header_height(id);
        slot.origin.y += header;
        slot.size.height -= header;
        slot
    }
    /// Convert a native resize back to the tree's logical coordinate space.
    pub fn slot_frame(&self, id: u32, mut window: NSRect) -> NSRect {
        let header = self.header_height(id);
        window.origin.y -= header;
        window.size.height += header;
        window
    }
    pub fn window_frames(&self, area: NSRect, gap: f64) -> Vec<(u32, NSRect)> {
        let mut frames = self.frames(area, gap);
        for (id, frame) in &mut frames {
            *frame = self.window_frame(*id, *frame);
        }
        frames
    }
    fn frame_tree(&self, r: NSRect, gap: f64) -> Option<Node> {
        let mut tree = self.effective_tree(r, gap)?;
        if let Some((cols, rows)) = self.grid {
            if self.minimums.is_empty() && !self.groups.iter().any(|group| group.members.len() > 1)
            {
                tree.snap(r, r, cols.clamp(2, 240), rows.clamp(2, 60), gap);
            } else {
                tree.snap_with(
                    r,
                    r,
                    cols.clamp(2, 240),
                    rows.clamp(2, 60),
                    gap,
                    &|id, vertical| self.window_minimum(id, vertical),
                );
            }
        }
        if !self.center_tree(&mut tree, r, gap, None, self.grid) {
            return None;
        }
        Some(tree)
    }
}

/// Quintic easing has zero velocity AND acceleration at both endpoints.
pub fn interpolate(from: NSRect, to: NSRect, progress: f64) -> NSRect {
    let t = progress.clamp(0., 1.);
    let t = t * t * t * (t * (t * 6. - 15.) + 10.);
    let mix = |a: f64, b: f64| a + (b - a) * t;
    rect(
        mix(from.origin.x, to.origin.x),
        mix(from.origin.y, to.origin.y),
        mix(from.size.width, to.size.width),
        mix(from.size.height, to.size.height),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapping_a_parent_refits_children_when_no_inner_grid_line_is_available() {
        let area = rect(0., 0., 1000., 1000.);
        for minimum in [90., 250.] {
            let mut desktop = Desktop::configured(Layout::ThreeBands, Some((2, 2)));
            desktop.order = vec![1, 2, 3];
            desktop.resized = Some((
                desktop.layout,
                desktop.order.clone(),
                Node::branch(
                    false,
                    0.2,
                    Node::Leaf(1),
                    Node::branch(false, minimum / 782., Node::Leaf(2), Node::Leaf(3)),
                ),
            ));
            desktop.set_minimum_sizes(&[(2, objc2_foundation::NSSize::new(120., minimum))].into());
            desktop.fit(area, 10.);
            let frames = desktop.frames(area, 10.);
            assert_eq!(frames.len(), 3);
            assert!(
                (frames
                    .iter()
                    .find(|(id, _)| *id == 2)
                    .unwrap()
                    .1
                    .size
                    .height
                    - minimum)
                    .abs()
                    < 0.001
            );
            assert_layout_invariants(&desktop, area);
        }
    }
    #[test]
    fn malformed_saved_shapes_do_not_replace_the_current_distribution() {
        let area = rect(0., 41., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::ThreeBands, Some((17, 9)));
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let before = desktop.clone();
        for text in [
            "2 6 3 0 0 v0.5 w0 h0.5 w1 w2", // main is not in the centered core
            "2 1 3 0 0 v0.5 e h0.5 w0 h0.5 w1 w2", // unoccupied half-screen
            "2 1 3 0 0 vNaN w0 h0.5 w1 w2",
            "2 1 3 0 0 v0.5 w0 h0.5 w1 w1", // duplicate/missing identity
        ] {
            assert!(
                !desktop.restore_distribution(text),
                "accepted malformed shape: {text}"
            );
            assert!(desktop == before);
        }
    }
    #[test]
    fn a_legitimate_extreme_resize_can_be_saved_and_restored() {
        let area = rect(-10000., 41., 16000., 2000.);
        let mut desktop = Desktop::configured(Layout::ThreeBands, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let from = desktop.frames(area, 10.)[0].1;
        desktop.resize(
            1,
            from,
            rect(from.origin.x, from.origin.y, 120., from.size.height),
            area,
            10.,
        );
        let before = desktop.frames(area, 10.);
        let saved = desktop.save_distribution(area).unwrap();
        assert!(
            desktop.restore_distribution(&saved),
            "cannot restore generated distribution: {saved}"
        );
        assert_eq!(desktop.frames(area, 10.), before);
    }
    #[test]
    fn randomized_lifecycle_keeps_complete_valid_geometry_and_round_trips() {
        for mut seed in [
            0x7dbd_5042_10ac_11efu64,
            0x81ca_6cf0_779a_42b1,
            0xab0e_7149_3085_7e0a,
            0x9bdf_56ca_17ad_b871,
        ] {
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            for initial in Layout::ALL {
                let mut desktop = Desktop::configured(initial, None);
                let mut area = rect(-1440., -300., 1440., 2560.);
                let mut ids = vec![1, 2, 3, 4, 5];
                let mut serial = 6;
                desktop.reconcile(&ids, area, 10.);
                for step in 0..1200 {
                    match next() % 7 {
                        0 if ids.len() < 10 => {
                            ids.push(serial);
                            serial += 1;
                            desktop.reconcile(&ids, area, 10.);
                        }
                        1 if ids.len() > 1 => {
                            ids.remove(next() as usize % ids.len());
                            desktop.reconcile(&ids, area, 10.);
                        }
                        2 => {
                            let frames = desktop.frames(area, 10.);
                            if !frames.is_empty() {
                                let (id, from) = frames[next() as usize % frames.len()];
                                let dx = next() as f64 / u64::MAX as f64 * 1200. - 600.;
                                let dy = next() as f64 / u64::MAX as f64 * 1200. - 600.;
                                desktop.resize(
                                    id,
                                    from,
                                    rect(
                                        from.origin.x + dx,
                                        from.origin.y + dy,
                                        from.size.width - dx,
                                        from.size.height - dy,
                                    ),
                                    area,
                                    10.,
                                );
                            }
                        }
                        3 => {
                            let source = ids[next() as usize % ids.len()];
                            let target = ids[next() as usize % ids.len()];
                            if let Some(d) = desktop.proposed_drop(
                                source,
                                DropIntent::Window(target, Zone::Center),
                                area,
                                10.,
                            ) {
                                desktop = d;
                            }
                        }
                        4 => {
                            desktop.grid = [None, Some((2, 2)), Some((17, 9)), Some((240, 60))]
                                [next() as usize % 4];
                        }
                        5 => {
                            area = if area.size.width > area.size.height {
                                rect(-1440., -300., 1440., 2560.)
                            } else {
                                rect(30.5, 41.5, 2560., 1440.)
                            };
                        }
                        _ => {
                            desktop.reset_template(Layout::ALL[next() as usize % 8], area);
                        }
                    }
                    desktop.fit(area, 10.);
                    let frames = desktop.frames(area, 10.);
                    assert_eq!(
                        frames.len(),
                        ids.len(),
                        "incomplete: {initial:?}, step={step}"
                    );
                    assert_layout_invariants(&desktop, area);
                    for (id, r) in &frames {
                        assert!(
                            r.size.width + 0.001 >= 80.,
                            "width: {initial:?} step={step} {id} {r:?} current={:?} grid={:?} saved={:?}",
                            desktop.layout,
                            desktop.grid,
                            desktop.save_distribution(area)
                        );
                        assert!(
                            r.size.height + 0.001 >= 60.,
                            "height: {initial:?} step={step} {id} {r:?}"
                        );
                    }
                    let saved = desktop.save_distribution(area).unwrap();
                    assert!(
                        desktop.restore_distribution(&saved),
                        "round trip: {initial:?} step={step} {saved}"
                    );
                    assert_eq!(desktop.frames(area, 10.), frames);
                }
            }
        }
    }
    #[test]
    fn lazy_focus_preserves_insertion_and_closure_behavior_in_every_template() {
        for layout in Layout::ALL {
            for area in [
                rect(10., 41., 2560., 1440.),
                rect(-1440., -500., 1440., 2560.),
            ] {
                for customized in [false, true] {
                    let mut original = Desktop::configured(layout, Some((17, 9)));
                    original.reconcile(&[1, 2, 3, 4], area, 10.);
                    if customized {
                        let from = original.frames(area, 10.)[0].1;
                        original.resize(
                            1,
                            from,
                            rect(
                                from.origin.x,
                                from.origin.y,
                                from.size.width - 30.,
                                from.size.height - 30.,
                            ),
                            area,
                            10.,
                        );
                    }
                    for ids in [
                        vec![1, 2, 3, 4],
                        vec![4, 3, 2, 1],
                        vec![1, 2, 3, 4, 5],
                        vec![2, 3, 4, 5],
                        vec![2, 3],
                        vec![],
                        vec![5, 6],
                    ] {
                        for focused in [Some(1), Some(2), Some(4), Some(99), None] {
                            let mut eager = original.clone();
                            let mut lazy = original.clone();
                            eager.reconcile_at(&ids, area, 10., focused);
                            let needed = lazy.needs_insertion_focus(&ids);
                            lazy.reconcile_at(&ids, area, 10., if needed { focused } else { None });
                            assert_eq!(lazy.order, eager.order);
                            assert_eq!(
                                lazy.frames(area, 10.),
                                eager.frames(area, 10.),
                                "{layout:?}, {customized}, {ids:?}, {focused:?}"
                            );
                            if ids.iter().all(|id| original.order.contains(id)) {
                                assert!(!needed);
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn centered_primary_has_two_thirds_and_two_slots_per_side() {
        for area in [
            rect(0., 41., 1800., 1000.),
            rect(-1200., -300., 1200., 1800.),
        ] {
            for gap in [0., 10.] {
                for count in 3..=5 {
                    let mut desktop = Desktop::configured(Layout::Centered, None);
                    desktop.reconcile(&(1..=count).collect::<Vec<_>>(), area, gap);
                    let frames = desktop.frames(area, gap);
                    let primary = frames.iter().find(|(id, _)| *id == 1).unwrap().1;
                    assert!((primary.size.width - area.size.width * 2. / 3.).abs() < 0.001);
                    assert!(
                        (primary.origin.x + primary.size.width / 2.
                            - area.origin.x
                            - area.size.width / 2.)
                            .abs()
                            < 0.001
                    );
                    assert_eq!(primary.size.height, area.size.height);
                    for (id, frame) in &frames {
                        if *id == 1 {
                            continue;
                        }
                        assert_eq!(frame.origin.x < primary.origin.x, id % 2 == 0);
                        assert!(frame.origin.y >= area.origin.y);
                        assert!(
                            frame.origin.y + frame.size.height
                                <= area.origin.y + area.size.height + 0.001
                        );
                    }
                    let saved = desktop.save_distribution(area).unwrap();
                    let mut restored = Desktop::default();
                    restored.reconcile(&(1..=count).collect::<Vec<_>>(), area, gap);
                    assert!(restored.restore_distribution(&saved));
                    assert_eq!(restored.frames(area, 10.), desktop.frames(area, 10.));
                }
            }
        }
    }
    #[test]
    fn centered_xl_and_vertical_have_the_requested_seven_and_five_slots() {
        for (layout, area, count) in [
            (Layout::CenteredWide, rect(10., 40., 3000., 1600.), 7),
            (Layout::VerticalMaster, rect(-1200., -400., 1200., 2400.), 5),
        ] {
            let mut desktop = Desktop::configured(layout, None);
            desktop.reconcile(&(1..=count).collect::<Vec<_>>(), area, 10.);
            let frames = desktop.frames(area, 10.);
            assert_eq!(frames.len(), count as usize);
            let main = frames.iter().find(|(id, _)| *id == 1).unwrap().1;
            let horizontal = layout == Layout::CenteredWide;
            let main_span = if horizontal {
                main.size.width
            } else {
                main.size.height
            };
            let span = if horizontal {
                area.size.width
            } else {
                area.size.height
            };
            assert!((main_span - span * 2. / 3.).abs() < 0.001);
            for (id, r) in &frames {
                if *id == 1 {
                    continue;
                }
                if horizontal {
                    assert_eq!(r.origin.x < main.origin.x, id % 2 == 0);
                    assert!((r.size.height - (area.size.height - 20.) / 3.).abs() < 0.001);
                } else {
                    assert_eq!(r.origin.y < main.origin.y, id % 2 == 0);
                    assert!((r.size.width - (area.size.width - 10.) / 2.).abs() < 0.001);
                }
            }
            assert_layout_invariants(&desktop, area);
        }
    }
    #[test]
    fn centered_sidebars_rebalance_new_windows_after_a_slot_swap() {
        for (layout, count, area) in [
            (Layout::Centered, 5, rect(10., 41., 1660., 947.)),
            (Layout::CenteredWide, 7, rect(0., 30., 3000., 1600.)),
            (Layout::VerticalMaster, 5, rect(-1200., -400., 1200., 2400.)),
        ] {
            let mut desktop = Desktop::configured(layout, None);
            desktop.reconcile(&[1, 2, 3], area, 10.);
            let original = desktop
                .frames(area, 10.)
                .iter()
                .find(|(id, _)| *id == 1)
                .unwrap()
                .1;
            let adjusted = if layout == Layout::VerticalMaster {
                rect(
                    original.origin.x,
                    original.origin.y - 20.,
                    original.size.width,
                    original.size.height + 40.,
                )
            } else {
                rect(
                    original.origin.x - 20.,
                    original.origin.y,
                    original.size.width + 40.,
                    original.size.height,
                )
            };
            desktop.resize(1, original, adjusted, area, 10.);
            let main_before = desktop
                .frames(area, 10.)
                .iter()
                .find(|(id, _)| *id == 1)
                .unwrap()
                .1;
            desktop = desktop
                .proposed_drop(2, DropIntent::Window(3, Zone::Center), area, 10.)
                .unwrap();
            for n in 4..=count {
                desktop.reconcile_at(&(1..=n).collect::<Vec<_>>(), area, 10., Some(2));
            }
            let frames = desktop.frames(area, 10.);
            assert_eq!(frames.len(), count as usize);
            let main = frames.iter().find(|(id, _)| *id == 1).unwrap().1;
            assert_eq!(main, main_before);
            let horizontal = layout != Layout::VerticalMaster;
            let before = frames
                .iter()
                .filter(|(id, r)| {
                    *id != 1
                        && if horizontal {
                            r.origin.x < main.origin.x
                        } else {
                            r.origin.y < main.origin.y
                        }
                })
                .count();
            assert_eq!(
                before,
                (count as usize - 1) / 2,
                "{layout:?} must rebalance both sides after adding windows"
            );
            for (_, r) in frames.iter().filter(|(id, _)| *id != 1) {
                if horizontal {
                    let expected = if r.origin.x < main.origin.x {
                        main.origin.x - area.origin.x - 10.
                    } else {
                        area.origin.x + area.size.width - main.origin.x - main.size.width - 10.
                    };
                    assert!((r.size.width - expected).abs() < 0.001);
                } else {
                    let expected = if r.origin.y < main.origin.y {
                        main.origin.y - area.origin.y - 10.
                    } else {
                        area.origin.y + area.size.height - main.origin.y - main.size.height - 10.
                    };
                    assert!((r.size.height - expected).abs() < 0.001);
                }
            }
            assert_layout_invariants(&desktop, area);
        }
    }
    #[test]
    fn centered_sidebar_growth_and_closure_preserve_learned_primary_size() {
        let area = rect(10., 41., 1660., 947.);
        for grid in [None, Some((120, 6))] {
            let minima: std::collections::HashMap<_, _> = (1..=7)
                .map(|id| {
                    (
                        id,
                        if id == 1 {
                            objc2_foundation::NSSize::new(618., 700.)
                        } else {
                            objc2_foundation::NSSize::new(511., 300.)
                        },
                    )
                })
                .collect();
            let mut desktop = Desktop::configured(Layout::Centered, grid);
            desktop.reconcile(&[1, 2, 3], area, 10.);
            desktop.set_minimum_sizes(&minima);
            let primary = desktop
                .frames(area, 10.)
                .iter()
                .find(|(id, _)| *id == 1)
                .unwrap()
                .1;
            desktop = desktop
                .proposed_drop(2, DropIntent::Window(3, Zone::Center), area, 10.)
                .unwrap();
            for ids in [
                vec![1, 2, 3, 4],
                vec![1, 2, 3, 4, 5],
                vec![1, 2, 3, 5],
                vec![1, 2, 3],
                vec![1, 2, 3, 6],
                vec![1, 2, 3, 6, 7],
            ] {
                desktop.reconcile_at(&ids, area, 10., Some(2));
                desktop.set_minimum_sizes(&minima);
                let frames = desktop.frames(area, 10.);
                assert_eq!(frames.len(), ids.len());
                assert_eq!(frames.iter().find(|(id, _)| *id == 1).unwrap().1, primary);
                assert_layout_invariants(&desktop, area);
                if ids.len() == 5 {
                    assert_eq!(
                        frames
                            .iter()
                            .filter(|(_, r)| r.origin.x < primary.origin.x)
                            .count(),
                        2
                    );
                    assert_eq!(
                        frames
                            .iter()
                            .filter(|(_, r)| r.origin.x > primary.origin.x)
                            .count(),
                        2
                    );
                }
            }
        }
    }
    #[test]
    fn every_template_resizes_shared_edges_and_preserves_slot_sizes_when_swapped() {
        for layout in Layout::ALL {
            for area in [
                rect(200., -900., 3000., 1800.),
                rect(-1440., -500., 1440., 2700.),
            ] {
                for grid in [None, Some((120, 6)), Some((17, 9))] {
                    let mut baseline = Desktop::configured(layout, grid);
                    let count = layout.example_count().max(5);
                    baseline.reconcile(&(1..=count).collect::<Vec<_>>(), area, 10.);
                    for id in &baseline.order {
                        let mut desktop = baseline.clone();
                        let before = desktop.frames(area, 10.);
                        let from = before.iter().find(|(window, _)| window == id).unwrap().1;
                        let to = rect(
                            from.origin.x,
                            from.origin.y,
                            from.size.width + area.size.width * 0.15,
                            from.size.height + area.size.height * 0.25,
                        );
                        desktop.resize(*id, from, to, area, 10.);
                        if desktop.frames(area, 10.) == before {
                            desktop = baseline.clone();
                            desktop.resize(
                                *id,
                                from,
                                rect(
                                    from.origin.x,
                                    from.origin.y,
                                    from.size.width - area.size.width * 0.15,
                                    from.size.height - area.size.height * 0.25,
                                ),
                                area,
                                10.,
                            );
                        }
                        let resized = desktop.frames(area, 10.);
                        assert_ne!(resized, before, "{layout:?} id={id} cannot resize");
                        assert_layout_invariants(&desktop, area);
                        if let Some(axis) = layout.centered_axis() {
                            let main = resized
                                .iter()
                                .find(|(window, _)| Some(*window) == desktop.order.first().copied())
                                .unwrap()
                                .1;
                            let offset = if axis {
                                main.origin.x + main.size.width / 2.
                                    - area.origin.x
                                    - area.size.width / 2.
                            } else {
                                main.origin.y + main.size.height / 2.
                                    - area.origin.y
                                    - area.size.height / 2.
                            };
                            assert!(offset.abs() < 0.001, "{layout:?}: primary drifted {offset}");
                        }
                        let target = *desktop.order.iter().find(|window| *window != id).unwrap();
                        let swapped = desktop
                            .proposed_drop(*id, DropIntent::Window(target, Zone::Center), area, 10.)
                            .unwrap();
                        let after = swapped.frames(area, 10.);
                        assert_eq!(
                            resized.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                            after.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                            "{layout:?}: moving a window changed slot sizes"
                        );
                        let saved = swapped.save_distribution(area).unwrap();
                        let mut restored = swapped.clone();
                        restored.reset_template(layout, area);
                        assert!(restored.restore_distribution(&saved));
                        assert_eq!(restored.frames(area, 10.), after);
                    }
                }
            }
        }
    }
    #[test]
    fn centered_resize_stays_symmetric_with_two_windows_unequal_minima_and_odd_grid() {
        for layout in [
            Layout::Centered,
            Layout::CenteredWide,
            Layout::VerticalMaster,
        ] {
            let axis = layout != Layout::VerticalMaster;
            let area = if axis {
                rect(10., 41., 3000., 1800.)
            } else {
                rect(-1440., -500., 1440., 2700.)
            };
            for grid in [None, Some((17, 9))] {
                for count in [2, 5] {
                    let mut desktop = Desktop::configured(layout, grid);
                    desktop.reconcile(&(1..=count).collect::<Vec<_>>(), area, 10.);
                    desktop.set_minimum_sizes(
                        &[
                            (1, objc2_foundation::NSSize::new(500., 500.)),
                            (
                                2,
                                if axis {
                                    objc2_foundation::NSSize::new(600., 90.)
                                } else {
                                    objc2_foundation::NSSize::new(120., 600.)
                                },
                            ),
                        ]
                        .into(),
                    );
                    let before = desktop.frames(area, 10.);
                    assert_eq!(before.len(), count as usize);
                    let from = before.iter().find(|(id, _)| *id == 1).unwrap().1;
                    let to = if axis {
                        rect(
                            from.origin.x,
                            from.origin.y,
                            from.size.width - 100.,
                            from.size.height,
                        )
                    } else {
                        rect(
                            from.origin.x,
                            from.origin.y,
                            from.size.width,
                            from.size.height - 100.,
                        )
                    };
                    desktop.resize(1, from, to, area, 10.);
                    let after = desktop.frames(area, 10.);
                    let main = after.iter().find(|(id, _)| *id == 1).unwrap().1;
                    let offset = if axis {
                        main.origin.x + main.size.width / 2. - area.origin.x - area.size.width / 2.
                    } else {
                        main.origin.y + main.size.height / 2.
                            - area.origin.y
                            - area.size.height / 2.
                    };
                    assert!(offset.abs() < 0.001);
                    if grid.is_none() {
                        assert_ne!(main, from);
                    }
                    let saved = desktop.save_distribution(area).unwrap();
                    let mut restored = desktop.clone();
                    restored.reset_template(layout, area);
                    assert!(restored.restore_distribution(&saved));
                    assert_eq!(restored.frames(area, 10.), after);
                    assert_layout_invariants(&desktop, area);
                }
            }
        }
    }
    #[test]
    fn retired_layout_migrates_without_reinterpreting_other_preferences_or_saved_trees() {
        assert_eq!(Layout::default(), Layout::Spiral);
        assert_eq!(Layout::from_index(0), Layout::Spiral);
        for layout in Layout::ALL {
            assert_eq!(Layout::from_index(layout.index()), layout);
            assert_eq!(Layout::from_selection(layout.selection_index()), layout);
        }
        let mut desktop = Desktop::default();
        let area = rect(0., 0., 1600., 1000.);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let before = desktop.frames(area, 10.);
        // An old Dwindle distribution must not override the new default with its old topology.
        assert!(!desktop.restore_distribution("2 0 3 0 0 v 0.5 l 0 h 0.5 l 1 l 2"));
        assert_eq!(desktop.frames(area, 10.), before);
    }
    #[test]
    fn spark_minimum_height_adjusts_spiral_without_replacing_it_with_columns() {
        let area = rect(10., 41., 1660., 947.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop.set_minimum_sizes(
            &[(2, objc2_foundation::NSSize::new(720., 700.))]
                .into_iter()
                .collect(),
        );
        for grid in [None, Some((12, 6)), Some((120, 6))] {
            desktop.grid = grid;
            let result = desktop.frames(area, 10.);
            assert_eq!(result.len(), 3);
            assert!(result[1].1.size.height >= 700.);
            assert!(result[1].1.size.width >= 720.);
            assert_eq!(result[0].1.size.height, area.size.height);
            assert!(result[1].1.size.height < area.size.height);
            assert_eq!(result[1].1.origin.x, result[2].1.origin.x);
            assert_layout_invariants(&desktop, area);
            assert_eq!(desktop.frames(area, 10.), result);
        }
    }
    #[test]
    fn impossible_spiral_minimum_does_not_silently_become_three_columns() {
        let area = rect(10., 41., 1660., 947.);
        let mut desktop = Desktop::configured(Layout::Spiral, Some((120, 6)));
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let original = desktop.frames(area, 10.);
        desktop.set_minimum_sizes(
            &[(2, objc2_foundation::NSSize::new(120., 947.))]
                .into_iter()
                .collect(),
        );
        assert!(desktop.frames(area, 10.).is_empty());
        assert_eq!(desktop.layout, Layout::Spiral);
        assert_eq!(desktop.order, vec![1, 2, 3]);
        let mut columns = desktop.clone();
        columns.reset_template(Layout::ThreeBands, area);
        assert_eq!(columns.frames(area, 10.).len(), 3);
        desktop.set_minimum_sizes(&Default::default());
        assert_eq!(desktop.frames(area, 10.), original);
    }
    fn assert_layout_invariants(desktop: &Desktop, area: NSRect) {
        let frames = desktop.frames(area, 10.);
        let mut ids: Vec<_> = frames.iter().map(|(id, _)| *id).collect();
        let mut expected = desktop.order.clone();
        ids.sort_unstable();
        expected.sort_unstable();
        assert_eq!(ids, expected);
        assert!(ids.windows(2).all(|pair| pair[0] != pair[1]));
        for (index, (_, a)) in frames.iter().enumerate() {
            assert!(
                [a.origin.x, a.origin.y, a.size.width, a.size.height]
                    .iter()
                    .all(|n| n.is_finite())
            );
            assert!(
                a.size.width > 0. && a.size.height > 0.,
                "invalid tile {a:?} in {area:?}"
            );
            assert!(a.origin.x >= area.origin.x - 0.001 && a.origin.y >= area.origin.y - 0.001);
            assert!(a.origin.x + a.size.width <= area.origin.x + area.size.width + 0.001);
            assert!(a.origin.y + a.size.height <= area.origin.y + area.size.height + 0.001);
            for (_, b) in &frames[index + 1..] {
                let width = (a.origin.x + a.size.width).min(b.origin.x + b.size.width)
                    - a.origin.x.max(b.origin.x);
                let height = (a.origin.y + a.size.height).min(b.origin.y + b.size.height)
                    - a.origin.y.max(b.origin.y);
                assert!(width <= 0.001 || height <= 0.001, "overlap {a:?} / {b:?}");
            }
        }
        if !frames.is_empty()
            && !(desktop.layout.centered_axis().is_some() && desktop.order.len() == 2)
        {
            let left = frames
                .iter()
                .map(|(_, r)| r.origin.x)
                .fold(f64::INFINITY, f64::min);
            let top = frames
                .iter()
                .map(|(_, r)| r.origin.y)
                .fold(f64::INFINITY, f64::min);
            let right = frames
                .iter()
                .map(|(_, r)| r.origin.x + r.size.width)
                .fold(f64::NEG_INFINITY, f64::max);
            let bottom = frames
                .iter()
                .map(|(_, r)| r.origin.y + r.size.height)
                .fold(f64::NEG_INFINITY, f64::max);
            assert!((left - area.origin.x).abs() < 0.001 && (top - area.origin.y).abs() < 0.001);
            assert!((right - area.origin.x - area.size.width).abs() < 0.001);
            assert!((bottom - area.origin.y - area.size.height).abs() < 0.001);
        }
    }

    #[test]
    fn lifecycle_matrix_preserves_identity_bounds_and_nonoverlap_in_all_templates() {
        let areas = [
            rect(0., 41., 1660., 947.),
            rect(-900., -400., 880., 1580.),
            rect(200., -1080., 1900., 1040.),
            rect(300., 1080., 1260., 760.),
            rect(-1500.5, 30.5, 1480.5, 929.5),
            rect(0., 0., 640., 480.),
        ];
        for (layout_index, layout) in Layout::ALL.into_iter().enumerate() {
            for area in areas {
                for grid in [None, Some((120, 6)), Some((2, 2)), Some((240, 60))] {
                    for count in 1..=8 {
                        let ids: Vec<u32> = (1..=count).collect();
                        let mut desktop = Desktop::configured(layout, grid);
                        desktop.reconcile(&ids, area, 10.);
                        for step in 0..24 {
                            let id = ids[step % ids.len()];
                            let target = ids[(step + 1) % ids.len()];
                            match step % 6 {
                                0 => desktop.reset_template(
                                    Layout::ALL[(layout_index + step / 6) % Layout::ALL.len()],
                                    area,
                                ),
                                1 => {
                                    let from = desktop
                                        .frames(area, 10.)
                                        .into_iter()
                                        .find(|(w, _)| *w == id)
                                        .unwrap()
                                        .1;
                                    desktop.resize(
                                        id,
                                        from,
                                        rect(
                                            from.origin.x - 35.,
                                            from.origin.y + 20.,
                                            from.size.width + 70.,
                                            from.size.height - 40.,
                                        ),
                                        area,
                                        10.,
                                    );
                                }
                                2 | 3 => {
                                    let side = [Zone::Left, Zone::Top, Zone::Right, Zone::Bottom]
                                        [step / 6];
                                    let intent = if step % 6 == 2 {
                                        DropIntent::Screen(side)
                                    } else {
                                        DropIntent::Window(target, side)
                                    };
                                    if let Some(proposed) =
                                        desktop.proposed_drop(id, intent, area, 10.)
                                    {
                                        desktop = proposed;
                                    }
                                }
                                4 => {
                                    desktop.remove(id);
                                    desktop.reconcile_at(&ids, area, 10., Some(target));
                                }
                                _ => {
                                    let saved = desktop.save_distribution(area).unwrap();
                                    let mut restored = desktop.clone();
                                    assert!(restored.restore_distribution(&saved));
                                    assert_eq!(
                                        restored.frames(area, 10.),
                                        desktop.frames(area, 10.)
                                    );
                                }
                            }
                            desktop.fit(area, 10.);
                            assert_layout_invariants(&desktop, area);
                            let before = desktop.frames(area, 10.);
                            let mut reordered = ids.clone();
                            reordered.reverse();
                            desktop.reconcile_at(&reordered, area, 10., Some(target));
                            assert_eq!(desktop.frames(area, 10.), before);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn adaptive_three_and_four_bands_cover_and_round_trip() {
        for (layout, count) in [(Layout::ThreeBands, 3), (Layout::FourBands, 4)] {
            for area in [rect(0., 30., 1600., 900.), rect(-900., -400., 900., 1600.)] {
                for n in [1, count, 8] {
                    let mut d = Desktop::configured(layout, None);
                    let ids: Vec<_> = (1..=n).collect();
                    d.reconcile(&ids, area, 0.);
                    let frames = d.frames(area, 0.);
                    assert_eq!(frames.len(), n as usize);
                    let mut bands: Vec<_> = frames
                        .iter()
                        .map(|(_, r)| {
                            if area.size.width >= area.size.height {
                                r.origin.x as i64
                            } else {
                                r.origin.y as i64
                            }
                        })
                        .collect();
                    bands.sort();
                    bands.dedup();
                    assert_eq!(bands.len(), n.min(count) as usize);
                    let covered: f64 = frames
                        .iter()
                        .map(|(_, r)| r.size.width * r.size.height)
                        .sum();
                    assert!((covered - area.size.width * area.size.height).abs() < 0.01);
                    assert_eq!(
                        layout.screen_name(area),
                        match (count, area.size.width >= area.size.height) {
                            (3, true) => "3 columnas",
                            (3, false) => "3 filas",
                            (_, true) => "4 columnas",
                            (_, false) => "4 filas",
                        }
                    );
                    let saved = d.save_distribution(area).unwrap();
                    let mut restored = Desktop::default();
                    restored.reconcile(&ids, area, 10.);
                    assert!(restored.restore_distribution(&saved));
                    assert_eq!(restored.frames(area, 10.), d.frames(area, 10.));
                }
            }
        }
    }
    #[test]
    fn all_drop_zones_preserve_existing_template_slots_and_reconciliation() {
        for layout in Layout::ALL {
            for grid in [None, Some((120, 6)), Some((12, 6))] {
                for area in [rect(0., 30., 1600., 900.), rect(-900., -400., 900., 1600.)] {
                    let mut original = Desktop::configured(layout, grid);
                    original.reconcile(&[1, 2, 3, 4, 5], area, 10.);
                    let before = original.frames(area, 10.);
                    for side in [
                        Zone::Top,
                        Zone::Bottom,
                        Zone::Left,
                        Zone::Right,
                        Zone::Center,
                    ] {
                        for intent in [DropIntent::Screen(side), DropIntent::Window(2, side)] {
                            let mut proposed =
                                original.proposed_drop(1, intent, area, 10.).unwrap();
                            let preview = proposed.frames(area, 10.);
                            assert_eq!(proposed.layout, layout);
                            assert_eq!(
                                preview.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                before.iter().map(|(_, r)| *r).collect::<Vec<_>>()
                            );
                            proposed.reconcile(&[5, 4, 3, 2, 1], area, 10.);
                            proposed.fit(area, 10.);
                            assert_eq!(proposed.frames(area, 10.), preview);
                            assert_eq!(original.frames(area, 10.), before);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn three_bands_allow_every_pair_with_a_smaller_middle_and_different_minima() {
        for horizontal in [false, true] {
            let area = if horizontal {
                rect(40., -200., 1880., 1080.)
            } else {
                rect(-1080., -500., 1080., 1880.)
            };
            for grid in [None, Some((12, 6)), Some((17, 9))] {
                let mut desktop = Desktop::configured(Layout::ThreeBands, grid);
                desktop.order = vec![1, 2, 3];
                desktop.resized = Some((
                    Layout::ThreeBands,
                    desktop.order.clone(),
                    Node::branch(
                        horizontal,
                        740. / 1870.,
                        Node::Leaf(1),
                        Node::branch(horizontal, 380. / 1120., Node::Leaf(2), Node::Leaf(3)),
                    ),
                ));
                desktop.set_minimum_sizes(
                    &(1..=3)
                        .map(|id| {
                            let min = if id == 2 { 240. } else { 690. };
                            (
                                id,
                                if horizontal {
                                    objc2_foundation::NSSize::new(min, 400.)
                                } else {
                                    objc2_foundation::NSSize::new(400., min)
                                },
                            )
                        })
                        .collect(),
                );
                let original = desktop.frames(area, 10.);
                assert_layout_invariants(&desktop, area);
                for source in 1..=3 {
                    for target in 1..=3 {
                        let target_rect = original.iter().find(|(id, _)| *id == target).unwrap().1;
                        let intent = drop_intent(
                            area,
                            &original,
                            source,
                            NSPoint::new(
                                target_rect.origin.x + target_rect.size.width / 2.,
                                target_rect.origin.y + target_rect.size.height / 2.,
                            ),
                            true,
                            true,
                        )
                        .unwrap();
                        let mut proposed = desktop.proposed_drop(source, intent, area, 10.)
                            .unwrap_or_else(|| panic!("no preview for {source} → {target}, horizontal={horizontal}, grid={grid:?}"));
                        assert_layout_invariants(&proposed, area);
                        let frames = proposed.frames(area, 10.);
                        for (index, (old_id, _)) in original.iter().enumerate() {
                            let expected = if *old_id == source {
                                target
                            } else if *old_id == target {
                                source
                            } else {
                                *old_id
                            };
                            assert_eq!(frames[index].0, expected);
                            assert!(
                                frames[index].1.size.width + 0.001
                                    >= proposed.window_minimum(expected, true)
                            );
                            assert!(
                                frames[index].1.size.height + 0.001
                                    >= proposed.window_minimum(expected, false)
                            );
                        }
                        proposed.reconcile(&[3, 1, 2], area, 10.);
                        assert_eq!(proposed.frames(area, 10.), frames);
                        assert_eq!(desktop.frames(area, 10.), original);
                    }
                }
                let permutations = [
                    [1, 2, 3],
                    [1, 3, 2],
                    [2, 1, 3],
                    [2, 3, 1],
                    [3, 1, 2],
                    [3, 2, 1],
                ];
                for first in permutations {
                    for last in permutations {
                        let mut current = desktop.clone();
                        for expected in [first, last] {
                            for (index, source) in expected.iter().enumerate() {
                                let target = current.frames(area, 10.)[index].0;
                                current = current
                                    .proposed_drop(
                                        *source,
                                        DropIntent::Window(target, Zone::Center),
                                        area,
                                        10.,
                                    )
                                    .unwrap();
                                assert_layout_invariants(&current, area);
                            }
                            assert_eq!(
                                current
                                    .frames(area, 10.)
                                    .iter()
                                    .map(|(id, _)| *id)
                                    .collect::<Vec<_>>(),
                                expected
                            );
                        }
                        let saved = current.save_distribution(area).unwrap();
                        let before = current.frames(area, 10.);
                        assert!(current.restore_distribution(&saved));
                        assert_eq!(current.frames(area, 10.), before);
                    }
                }
            }
        }
    }
    #[test]
    fn every_template_pair_handles_different_minima_without_losing_feasible_slots() {
        for layout in Layout::ALL {
            for area in [
                rect(20., 41., 3840., 2160.),
                rect(-2160., -900., 2160., 3840.),
            ] {
                for grid in [None, Some((12, 6)), Some((17, 9))] {
                    for count in 2..=7 {
                        let ids: Vec<_> = (1..=count).collect();
                        let mut desktop = Desktop::configured(layout, grid);
                        desktop.reconcile(&ids, area, 10.);
                        let minimums = desktop
                            .frames(area, 10.)
                            .iter()
                            .map(|(id, r)| {
                                let fraction = [0.4, 0.7, 0.9][(*id as usize) % 3];
                                (
                                    *id,
                                    objc2_foundation::NSSize::new(
                                        (r.size.width * fraction).floor(),
                                        (r.size.height * fraction).floor(),
                                    ),
                                )
                            })
                            .collect();
                        desktop.set_minimum_sizes(&minimums);
                        let original = desktop.frames(area, 10.);
                        assert_layout_invariants(&desktop, area);
                        for source in &ids {
                            for target in &ids {
                                let fits = original.iter().all(|(old, frame)| {
                                    let new = if old == source {
                                        *target
                                    } else if old == target {
                                        *source
                                    } else {
                                        *old
                                    };
                                    frame.size.width + 0.001 >= desktop.window_minimum(new, true)
                                        && frame.size.height + 0.001
                                            >= desktop.window_minimum(new, false)
                                });
                                let proposed = desktop.proposed_drop(
                                    *source,
                                    DropIntent::Window(*target, Zone::Center),
                                    area,
                                    10.,
                                );
                                assert!(
                                    !fits || proposed.is_some(),
                                    "{layout:?}, {grid:?}, {count}, {source} → {target}"
                                );
                                if let Some(mut proposed) = proposed {
                                    assert_layout_invariants(&proposed, area);
                                    let preview = proposed.frames(area, 10.);
                                    if fits {
                                        assert_eq!(
                                            preview.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                            original.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
                                            "compatible slots moved: {layout:?}, {grid:?}, {source} → {target}"
                                        );
                                    }
                                    for (id, frame) in &preview {
                                        assert!(
                                            frame.size.width + 0.001
                                                >= proposed.window_minimum(*id, true),
                                            "width below minimum: {layout:?} {grid:?} {count} {source} → {target}, id={id}, {frame:?}, minimum={:?}",
                                            proposed.window_minimum(*id, true)
                                        );
                                        assert!(
                                            frame.size.height + 0.001
                                                >= proposed.window_minimum(*id, false),
                                            "height below minimum: {layout:?} {grid:?} {count} {source} → {target}, id={id}, {frame:?}, minimum={:?}",
                                            proposed.window_minimum(*id, false)
                                        );
                                    }
                                    proposed.reconcile(&ids, area, 10.);
                                    assert_eq!(proposed.frames(area, 10.), preview);
                                }
                                assert_eq!(desktop.frames(area, 10.), original);
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn slot_swap_rejects_incompatible_app_minimum_without_destroying_layout() {
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop.set_minimum_sizes(&[(1, objc2_foundation::NSSize::new(600., 950.))].into());
        let before = desktop.frames(area, 10.);
        assert!(
            desktop
                .proposed_drop(1, DropIntent::Window(2, Zone::Center), area, 10.)
                .is_none()
        );
        assert!(
            desktop
                .proposed_drop(1, DropIntent::Window(2, Zone::Top), area, 10.)
                .is_none()
        );
        assert_eq!(desktop.frames(area, 10.), before);
        assert!(
            desktop
                .proposed_drop(2, DropIntent::Window(3, Zone::Left), area, 10.)
                .is_some()
        );
    }
    #[test]
    fn intent_distinguishes_screen_edges_local_edges_and_centers() {
        for area in [rect(0., 30., 1600., 900.), rect(-900., -400., 900., 1600.)] {
            let mut d = Desktop::configured(Layout::Spiral, None);
            d.reconcile(&[1, 2], area, 10.);
            let frames = d.frames(area, 10.);
            for (p, side) in [
                (
                    NSPoint::new(area.origin.x + area.size.width / 2., area.origin.y + 5.),
                    Zone::Top,
                ),
                (
                    NSPoint::new(
                        area.origin.x + area.size.width / 2.,
                        area.origin.y + area.size.height - 5.,
                    ),
                    Zone::Bottom,
                ),
                (
                    NSPoint::new(area.origin.x + 5., area.origin.y + area.size.height / 2.),
                    Zone::Left,
                ),
                (
                    NSPoint::new(
                        area.origin.x + area.size.width - 5.,
                        area.origin.y + area.size.height / 2.,
                    ),
                    Zone::Right,
                ),
            ] {
                assert_eq!(
                    drop_intent(area, &frames, 99, p, true, true),
                    Some(DropIntent::Screen(side))
                );
            }
            let target = frames.iter().find(|(id, _)| *id == 2).unwrap().1;
            let center = NSPoint::new(
                target.origin.x + target.size.width / 2.,
                target.origin.y + target.size.height / 2.,
            );
            assert_eq!(
                drop_intent(area, &frames, 99, center, true, true),
                Some(DropIntent::Window(2, Zone::Center))
            );
            assert_eq!(
                drop_intent(area, &frames, 2, center, true, true),
                Some(DropIntent::Window(2, Zone::Center))
            );
            assert_eq!(
                drop_intent(
                    area,
                    &frames,
                    1,
                    NSPoint::new(area.origin.x - 30., center.y),
                    true,
                    true,
                ),
                None
            );
        }
        let area = rect(0., 0., 1600., 1000.);
        let frames = vec![
            (1, rect(0., 0., 800., 1000.)),
            (2, rect(810., 0., 790., 1000.)),
        ];
        let p = NSPoint::new(820., 500.);
        assert_eq!(
            drop_intent(area, &frames, 99, p, true, true),
            Some(DropIntent::Window(2, Zone::Left))
        );
        assert_eq!(
            drop_intent(area, &frames, 99, p, false, true),
            Some(DropIntent::Window(2, Zone::Center))
        );
    }
    #[test]
    fn dropping_on_another_monitor_uses_destination_template_slots() {
        let landscape = rect(0., 30., 1600., 900.);
        let portrait = rect(-900., -400., 900., 1600.);
        let mut source = Desktop::configured(Layout::Spiral, None);
        source.reconcile(&[1, 2], landscape, 10.);
        let mut destination = Desktop::configured(Layout::Spiral, None);
        destination.reconcile(&[3, 4], portrait, 10.);
        let preview = destination
            .proposed_drop(1, DropIntent::Screen(Zone::Bottom), portrait, 10.)
            .unwrap();
        assert_eq!(source.order.len(), 2);
        assert_eq!(destination.order.len(), 2);
        source.remove(1);
        destination = preview;
        destination.reconcile(&[1, 3, 4], portrait, 10.);
        assert_eq!(source.frames(landscape, 10.), vec![(2, landscape)]);
        let frames = destination.frames(portrait, 10.);
        assert_eq!(frames.len(), 3);
        let mut expected = Desktop::configured(Layout::Spiral, None);
        expected.reconcile(&[3, 4, 1], portrait, 10.);
        assert_eq!(
            frames.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
            expected
                .frames(portrait, 10.)
                .iter()
                .map(|(_, r)| *r)
                .collect::<Vec<_>>()
        );
        let empty = Desktop::configured(Layout::Grid, Some((120, 6)));
        assert_eq!(
            empty
                .proposed_drop(1, DropIntent::Screen(Zone::Top), portrait, 10.)
                .unwrap()
                .frames(portrait, 10.),
            vec![(1, portrait)]
        );
    }
    #[test]
    fn monitor_arrivals_keep_every_destination_template_and_resize_ratios() {
        for layout in Layout::ALL {
            for grid in [None, Some((17, 9))] {
                for area in [
                    rect(0., 30., 2560., 1440.),
                    rect(-1440., -600., 1440., 2560.),
                ] {
                    for count in 0..7 {
                        let ids: Vec<_> = (1..=count).collect();
                        let mut destination = Desktop::configured(layout, grid);
                        destination.reconcile(&ids, area, 10.);
                        if let Some((id, from)) = destination.frames(area, 10.).first().copied() {
                            destination.resize(
                                id,
                                from,
                                rect(
                                    from.origin.x,
                                    from.origin.y,
                                    from.size.width - 50.,
                                    from.size.height - 50.,
                                ),
                                area,
                                10.,
                            );
                        }
                        let unchanged = destination.save_distribution(area);
                        let mut expected = destination.clone();
                        let mut arriving = ids.clone();
                        arriving.push(99);
                        expected.reconcile(&arriving, area, 10.);
                        let mut slots: Vec<_> = expected
                            .frames(area, 10.)
                            .into_iter()
                            .map(|(_, r)| format!("{r:?}"))
                            .collect();
                        slots.sort();
                        for side in [
                            Zone::Left,
                            Zone::Right,
                            Zone::Top,
                            Zone::Bottom,
                            Zone::Center,
                        ] {
                            let intents = [DropIntent::Screen(side), DropIntent::Window(1, side)];
                            for intent in intents.into_iter().take(if count == 0 { 1 } else { 2 }) {
                                let mut proposed =
                                    destination.proposed_drop(99, intent, area, 10.).unwrap();
                                assert_layout_invariants(&proposed, area);
                                let mut actual: Vec<_> = proposed
                                    .frames(area, 10.)
                                    .into_iter()
                                    .map(|(_, r)| format!("{r:?}"))
                                    .collect();
                                actual.sort();
                                assert_eq!(
                                    actual, slots,
                                    "{layout:?}, {grid:?}, {count}, {intent:?}"
                                );
                                let before = proposed.frames(area, 10.);
                                proposed.reconcile(&arriving, area, 10.);
                                assert_eq!(proposed.frames(area, 10.), before);
                                assert_eq!(destination.save_distribution(area), unchanged);
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn portrait_defaults_stack_and_coordinate_conversion_preserves_global_points() {
        let portrait = rect(-900., -400., 900., 1600.);
        for layout in [Layout::Spiral, Layout::Master] {
            let mut d = Desktop::configured(layout, None);
            d.reconcile(&[1, 2], portrait, 10.);
            for (_, frame) in d.frames(portrait, 10.) {
                assert_eq!(frame.size.width, portrait.size.width);
                assert_eq!(frame.origin.x, portrait.origin.x);
            }
        }
        assert_eq!(
            ax_screen_rect(rect(0., 0., 1600., 900.), 900.),
            rect(0., 0., 1600., 900.)
        );
        assert_eq!(
            ax_screen_rect(rect(-900., -300., 900., 1600.), 900.),
            portrait
        );
        assert_eq!(
            ax_screen_rect(rect(300., 900., 1920., 1080.), 900.),
            rect(300., -1080., 1920., 1080.)
        );
        assert_eq!(
            ax_screen_rect(rect(0., -1080., 1920., 1080.), 900.),
            rect(0., 900., 1920., 1080.)
        );
    }

    #[test]
    fn templates_cover_without_overlap() {
        for layout in [
            Layout::Centered,
            Layout::Spiral,
            Layout::Grid,
            Layout::Master,
        ] {
            for n in 1..9 {
                let mut d = Desktop {
                    layout,
                    ..Default::default()
                };
                let r = rect(-1200., 30., 1200., 900.);
                d.reconcile(&(1..=n).collect::<Vec<_>>(), r, 0.);
                let frames = d.frames(r, 0.);
                assert_eq!(frames.len(), n as usize);
                let area: f64 = frames
                    .iter()
                    .map(|(_, r)| r.size.width * r.size.height)
                    .sum();
                let expected = if layout.centered_axis().is_some() && n == 2 {
                    900000.
                } else {
                    1080000.
                };
                assert!((area - expected).abs() < 0.01);
                for (i, (_, a)) in frames.iter().enumerate() {
                    assert!(a.size.width > 0. && a.size.height > 0.);
                    for (_, b) in &frames[i + 1..] {
                        assert!(
                            a.origin.x + a.size.width <= b.origin.x + 0.01
                                || b.origin.x + b.size.width <= a.origin.x + 0.01
                                || a.origin.y + a.size.height <= b.origin.y + 0.01
                                || b.origin.y + b.size.height <= a.origin.y + 0.01
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn drop_and_remove_preserve_membership() {
        let mut d = Desktop::default();
        let r = rect(0., 0., 1000., 800.);
        d.reconcile(&[1, 2, 3], r, 10.);
        d.place(1, 3, Zone::Top);
        assert_eq!(d.order, vec![2, 1, 3]);
        d.place(2, 3, Zone::Center);
        assert_eq!(d.order, vec![3, 1, 2]);
        d.remove(1);
        assert_eq!(d.frames(r, 10.).len(), 2);
        d.reconcile(&[2, 4], r, 10.);
        assert_eq!(d.order, vec![2, 4]);
    }
    #[test]
    fn default_spiral_splits_last_tile_and_spaces_are_independent() {
        let area = rect(0., 0., 1200., 800.);
        let mut first = Desktop::default();
        first.reconcile(&[1, 2], area, 10.);
        let before = first.frames(area, 10.);
        let second = first.clone();
        first.reconcile_at(&[1, 2, 3], area, 10., Some(1));
        assert_eq!(
            first.frames(area, 10.).iter().find(|(id, _)| *id == 1),
            before.iter().find(|(id, _)| *id == 1)
        );
        assert_eq!(second.order, vec![1, 2]);
        assert_eq!(second.frames(area, 10.), before);
    }
    #[test]
    fn insertion_preserves_unique_membership_at_every_edge() {
        let area = rect(0., 0., 1200., 800.);
        for zone in [
            Zone::Left,
            Zone::Right,
            Zone::Top,
            Zone::Bottom,
            Zone::Center,
        ] {
            let mut desktop = Desktop::default();
            desktop.reconcile(&[1, 2], area, 10.);
            desktop.place(3, 1, zone);
            desktop.reconcile(&[1, 2, 3], area, 10.);
            assert_eq!(desktop.order.len(), 3);
            let mut ids: Vec<_> = desktop
                .frames(area, 10.)
                .iter()
                .map(|(id, _)| *id)
                .collect();
            ids.sort();
            assert_eq!(ids, vec![1, 2, 3]);
            desktop.remove(3);
            desktop.remove(1);
            desktop.remove(2);
            assert!(desktop.frames(area, 10.).is_empty());
            desktop.reconcile(&[9], area, 10.);
            assert_eq!(desktop.frames(area, 10.), vec![(9, area)]);
        }
    }
    #[test]
    fn resize_shared_edges_in_every_template_without_overlap() {
        let area = rect(-1400., 40., 1400., 1000.);
        for layout in Layout::ALL {
            let mut desktop = Desktop {
                layout,
                ..Default::default()
            };
            desktop.reconcile(&[1, 2, 3, 4], area, 10.);
            let frames = desktop.frames(area, 10.);
            let (id, from) = frames
                .iter()
                .copied()
                .find(|(_, r)| r.origin.x + r.size.width < area.origin.x + area.size.width - 10.)
                .unwrap();
            let to = rect(
                from.origin.x,
                from.origin.y,
                from.size.width + 45.,
                from.size.height,
            );
            desktop.resize(id, from, to, area, 10.);
            let resized = desktop.frames(area, 10.);
            let target = resized.iter().find(|(w, _)| *w == id).unwrap().1;
            assert!(
                (target.size.width - to.size.width).abs() < 0.01,
                "{layout:?}: {target:?} != {to:?}"
            );
            for (i, (_, a)) in resized.iter().enumerate() {
                assert!(a.size.width >= 120. && a.size.height >= 90.);
                assert!(a.origin.x >= area.origin.x && a.origin.y >= area.origin.y);
                assert!(a.origin.x + a.size.width <= area.origin.x + area.size.width + 0.01);
                for (_, b) in &resized[i + 1..] {
                    assert!(
                        a.origin.x + a.size.width <= b.origin.x
                            || b.origin.x + b.size.width <= a.origin.x
                            || a.origin.y + a.size.height <= b.origin.y
                            || b.origin.y + b.size.height <= a.origin.y
                    );
                }
            }
            desktop.reconcile(&[1, 2, 3, 4], area, 10.);
            assert_eq!(
                desktop.frames(area, 10.),
                resized,
                "reconciliation discarded user proportions"
            );
        }
    }
    #[test]
    fn nonfinite_resize_baselines_and_targets_preserve_the_distribution() {
        let area = rect(-900., 40., 1800., 1200.);
        let mut baseline = Desktop::configured(Layout::Grid, Some((12, 8)));
        baseline.reconcile(&[1, 2, 3, 4], area, 10.);
        let from = baseline.frames(area, 10.)[0].1;
        baseline.resize(
            1,
            from,
            rect(
                from.origin.x,
                from.origin.y,
                from.size.width + 100.,
                from.size.height + 80.,
            ),
            area,
            10.,
        );
        let frames = baseline.frames(area, 10.);
        let saved = baseline.save_distribution(area);
        let from = frames.iter().find(|(id, _)| *id == 1).unwrap().1;
        let to = rect(
            from.origin.x,
            from.origin.y,
            from.size.width + 100.,
            from.size.height + 80.,
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for coordinate in 0..8 {
                let (mut start, mut end) = (from, to);
                *match coordinate {
                    0 => &mut start.origin.x,
                    1 => &mut start.origin.y,
                    2 => &mut start.size.width,
                    3 => &mut start.size.height,
                    4 => &mut end.origin.x,
                    5 => &mut end.origin.y,
                    6 => &mut end.size.width,
                    _ => &mut end.size.height,
                } = value;
                let mut desktop = baseline.clone();
                desktop.resize(1, start, end, area, 10.);
                assert_eq!(
                    desktop.frames(area, 10.),
                    frames,
                    "coordinate {coordinate}: {value}"
                );
                assert_eq!(desktop.save_distribution(area), saved);
            }
        }
    }
    #[test]
    fn resize_left_and_top_edges_limits_and_outer_screen() {
        let area = rect(0., 0., 1200., 900.);
        let mut d = Desktop::default();
        d.reconcile(&[1, 2, 3], area, 10.);
        let from = d
            .frames(area, 10.)
            .iter()
            .find(|(id, _)| *id == 3)
            .unwrap()
            .1;
        let to = rect(
            from.origin.x - 80.,
            from.origin.y - 60.,
            from.size.width + 80.,
            from.size.height + 60.,
        );
        d.resize(3, from, to, area, 10.);
        let actual = d
            .frames(area, 10.)
            .iter()
            .find(|(id, _)| *id == 3)
            .unwrap()
            .1;
        assert_eq!(actual, to);
        let too_large = rect(-500., -500., 2500., 2000.);
        d.resize(3, actual, too_large, area, 10.);
        for (_, r) in d.frames(area, 10.) {
            assert!(r.size.width >= 120. - 0.01 && r.size.height >= 90. - 0.01);
            assert!(r.origin.x >= 0. && r.origin.y >= 0.);
            assert!(r.origin.x + r.size.width <= 1200.01 && r.origin.y + r.size.height <= 900.01);
        }
    }
    #[test]
    fn grid_is_symmetric_with_gaps_and_resize_does_not_accumulate_drift() {
        let area = rect(0., 0., 1600., 1200.);
        let mut d = Desktop {
            layout: Layout::Grid,
            ..Default::default()
        };
        d.reconcile(&(1..=9).collect::<Vec<_>>(), area, 10.);
        let frames = d.frames(area, 10.);
        for (_, r) in &frames {
            assert!((r.size.width - frames[0].1.size.width).abs() < 0.01);
            assert!((r.size.height - frames[0].1.size.height).abs() < 0.01);
        }
        let from = frames[0].1;
        let to = rect(
            from.origin.x,
            from.origin.y,
            from.size.width + 35.,
            from.size.height + 25.,
        );
        let baseline = d.clone();
        for _ in 0..100 {
            d = baseline.clone();
            d.resize(1, from, to, area, 10.);
        }
        let result = d.frames(area, 10.);
        assert!((result[0].1.size.width - to.size.width).abs() < 0.01);
        assert!((result[0].1.size.height - to.size.height).abs() < 0.01);
    }
    fn assert_full_area(frames: &[(u32, NSRect)], area: NSRect, gap: f64) {
        // The union of tiles expanded by half a gap must cover the whole usable
        // area. Unlike summing areas, this also detects holes masked by overlap.
        for x in (0..area.size.width as usize).step_by(7) {
            for y in (0..area.size.height as usize).step_by(7) {
                let p = NSPoint::new(area.origin.x + x as f64, area.origin.y + y as f64);
                assert!(
                    frames.iter().any(|(_, r)| contains(
                        rect(
                            r.origin.x - gap / 2.,
                            r.origin.y - gap / 2.,
                            r.size.width + gap,
                            r.size.height + gap
                        ),
                        p
                    )),
                    "uncovered point {p:?}"
                );
            }
        }
    }
    #[test]
    fn removing_tiles_and_resizing_always_reclaims_the_entire_area() {
        let area = rect(-1200., 40., 1200., 900.);
        for layout in [
            Layout::Centered,
            Layout::Spiral,
            Layout::Grid,
            Layout::Master,
        ] {
            let mut d = Desktop {
                layout,
                ..Default::default()
            };
            d.reconcile(&(1..=12).collect::<Vec<_>>(), area, 10.);
            d.fit(area, 10.);
            assert!(
                d.frames(area, 10.)
                    .iter()
                    .all(|(_, r)| r.size.width >= 80. && r.size.height >= 60.)
            );
            let (id, from) = d.frames(area, 10.)[0];
            d.resize(
                id,
                from,
                rect(
                    from.origin.x,
                    from.origin.y,
                    from.size.width + 20.,
                    from.size.height + 20.,
                ),
                area,
                10.,
            );
            assert_full_area(&d.frames(area, 10.), area, 10.);
            for id in 1..12 {
                d.remove(id);
                d.fit(area, 10.);
                if layout.centered_axis().is_some() && d.order.len() == 2 {
                    assert_layout_invariants(&d, area);
                } else {
                    assert_full_area(&d.frames(area, 10.), area, 10.);
                }
            }
            assert_eq!(d.frames(area, 10.), vec![(12, area)]);
        }
    }
    #[test]
    fn transition_has_exact_endpoints_and_monotonic_geometry() {
        let from = rect(-600., 30., 500., 400.);
        let to = rect(20., 60., 800., 900.);
        assert_eq!(interpolate(from, to, 0.), from);
        assert_eq!(interpolate(from, to, 1.), to);
        let mut previous = from;
        for step in 1..=100 {
            let r = interpolate(from, to, step as f64 / 100.);
            assert!(r.origin.x >= previous.origin.x && r.size.width >= previous.size.width);
            previous = r;
        }
    }
    #[test]
    fn easing_accelerates_then_decelerates_symmetrically() {
        let a = rect(0., 0., 100., 100.);
        let b = rect(100., 0., 100., 100.);
        let x = |t| interpolate(a, b, t).origin.x;
        assert!((x(0.5) - 50.).abs() < 1e-9);
        assert!(x(0.1) - x(0.) < x(0.2) - x(0.1));
        assert!(x(0.2) - x(0.1) < x(0.5) - x(0.4));
        assert!((x(0.1) + x(0.9) - 100.).abs() < 1e-9);
        assert!(x(0.001) < 0.00001);
    }
    #[test]
    fn spiral_continues_from_surviving_turn_after_closing_first_window() {
        let area = rect(0., 0., 1600., 1100.);
        let mut d = Desktop {
            layout: Layout::Spiral,
            ..Default::default()
        };
        d.reconcile(&[1, 2, 3], area, 10.);
        let from = d.frames(area, 10.)[0].1;
        d.resize(1, from, rect(0., 0., 900., 1100.), area, 10.);
        d.reconcile(&[2, 3], area, 10.);
        d.reconcile_at(&[2, 3, 4], area, 10., Some(2));
        let frames = d.frames(area, 10.);
        let third = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
        let fourth = frames.iter().find(|(id, _)| *id == 4).unwrap().1;
        assert_eq!(third.origin.y, fourth.origin.y);
        assert!(fourth.origin.x < third.origin.x);
        assert_full_area(&frames, area, 10.);
    }
    #[test]
    fn adjusted_spiral_keeps_turning_even_when_first_window_is_focused() {
        let area = rect(0., 0., 1600., 1100.);
        let mut d = Desktop {
            layout: Layout::Spiral,
            ..Default::default()
        };
        d.reconcile(&[1, 2], area, 10.);
        let from = d.frames(area, 10.)[0].1;
        d.resize(1, from, rect(0., 0., 950., 1100.), area, 10.);
        d.reconcile_at(&[1, 2, 3], area, 10., Some(1));
        let frames = d.frames(area, 10.);
        assert_eq!(
            frames.iter().find(|(id, _)| *id == 1).unwrap().1.size.width,
            950.
        );
        let upper = frames.iter().find(|(id, _)| *id == 2).unwrap().1;
        let lower = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
        assert_eq!(upper.origin.x, lower.origin.x);
        assert!(lower.origin.y > upper.origin.y);
        d.reconcile_at(&[1, 2, 3, 4], area, 10., Some(1));
        let frames = d.frames(area, 10.);
        let third = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
        let fourth = frames.iter().find(|(id, _)| *id == 4).unwrap().1;
        assert!(fourth.origin.x < third.origin.x);
        assert_eq!(fourth.origin.y, third.origin.y);
        d.reset_template(Layout::Spiral, area);
        let mut fresh = Desktop {
            layout: Layout::Spiral,
            ..Default::default()
        };
        fresh.reconcile(&[1, 2, 3, 4], area, 10.);
        assert_eq!(d.frames(area, 10.), fresh.frames(area, 10.));
    }
    #[test]
    fn closing_a_stacked_neighbor_preserves_the_adjusted_column_width() {
        let area = rect(0., 0., 1600., 1000.);
        for layout in [Layout::Spiral, Layout::Master] {
            for closed in [2, 3] {
                let mut d = Desktop {
                    layout,
                    grid: Some((120, 6)),
                    ..Default::default()
                };
                d.reconcile(&[1, 2, 3], area, 10.);
                let from = d
                    .frames(area, 10.)
                    .iter()
                    .find(|(id, _)| *id == 1)
                    .unwrap()
                    .1;
                d.resize(
                    1,
                    from,
                    rect(
                        from.origin.x,
                        from.origin.y,
                        from.size.width + 130.,
                        from.size.height,
                    ),
                    area,
                    10.,
                );
                let before = d
                    .frames(area, 10.)
                    .iter()
                    .find(|(id, _)| *id == 1)
                    .unwrap()
                    .1;
                d.reconcile(&[1, if closed == 2 { 3 } else { 2 }], area, 10.);
                let frames = d.frames(area, 10.);
                assert_eq!(
                    frames.iter().find(|(id, _)| *id == 1).unwrap().1,
                    before,
                    "{layout:?} lost the adjusted width"
                );
                assert_full_area(&frames, area, 10.);
                let survivor = if closed == 2 { 3 } else { 2 };
                assert_eq!(
                    frames
                        .iter()
                        .find(|(id, _)| *id == survivor)
                        .unwrap()
                        .1
                        .size
                        .height,
                    area.size.height
                );
            }
        }
    }
    #[test]
    fn adding_after_closure_preserves_remaining_adjusted_divisions() {
        let area = rect(0., 0., 1600., 1000.);
        let mut d = Desktop {
            layout: Layout::Master,
            ..Default::default()
        };
        d.reconcile(&[1, 2, 3], area, 10.);
        let from = d.frames(area, 10.)[0].1;
        d.resize(1, from, rect(0., 0., 1100., 1000.), area, 10.);
        let before = d.frames(area, 10.)[0].1;
        d.reconcile(&[1, 2], area, 10.);
        d.reconcile_at(&[1, 2, 4], area, 10., Some(2));
        assert_eq!(d.frames(area, 10.)[0].1, before);
    }
    #[test]
    fn finder_lower_right_in_three_window_layout_resizes_from_all_handles() {
        let area = rect(0., 0., 1600., 1100.);
        let mut baseline = Desktop::default();
        baseline.reconcile(&[1, 2, 3], area, 10.);
        let frames = baseline.frames(area, 10.);
        assert_eq!(frames[0].1.size.height, area.size.height);
        let from = frames.iter().find(|(id, _)| *id == 3).unwrap().1;
        assert_eq!(from.origin.x + from.size.width, area.size.width);
        assert_eq!(from.origin.y + from.size.height, area.size.height);
        for desired in [
            rect(
                from.origin.x,
                from.origin.y,
                from.size.width - 80.,
                from.size.height - 60.,
            ),
            rect(
                from.origin.x + 80.,
                from.origin.y + 60.,
                from.size.width - 80.,
                from.size.height - 60.,
            ),
        ] {
            let mut d = baseline.clone();
            d.resize(3, from, desired, area, 10.);
            let result = d.frames(area, 10.);
            let resized = result.iter().find(|(id, _)| *id == 3).unwrap().1;
            assert!((resized.size.width - (from.size.width - 80.)).abs() < 0.01);
            assert!((resized.size.height - (from.size.height - 60.)).abs() < 0.01);
            assert!(
                result
                    .iter()
                    .find(|(id, _)| *id == 2)
                    .unwrap()
                    .1
                    .size
                    .height
                    > frames
                        .iter()
                        .find(|(id, _)| *id == 2)
                        .unwrap()
                        .1
                        .size
                        .height
            );
            assert_full_area(&result, area, 10.);
        }
    }
    #[test]
    fn bottom_right_screen_corner_can_resize_its_tile() {
        let area = rect(0., 0., 1400., 1000.);
        let mut d = Desktop {
            layout: Layout::Grid,
            ..Default::default()
        };
        d.reconcile(&[1, 2, 3, 4], area, 10.);
        let from = d
            .frames(area, 10.)
            .iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        d.resize(
            4,
            from,
            rect(
                from.origin.x,
                from.origin.y,
                from.size.width - 90.,
                from.size.height - 60.,
            ),
            area,
            10.,
        );
        let frame = d
            .frames(area, 10.)
            .iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        assert!((frame.size.width - (from.size.width - 90.)).abs() < 0.01);
        assert!((frame.size.height - (from.size.height - 60.)).abs() < 0.01);
        assert_eq!(frame.origin.x + frame.size.width, 1400.);
        assert_eq!(frame.origin.y + frame.size.height, 1000.);
        assert_full_area(&d.frames(area, 10.), area, 10.);
    }
    #[test]
    fn rearranging_resized_grid_preserves_every_slot() {
        let area = rect(0., 0., 1440., 900.);
        for zone in [
            Zone::Center,
            Zone::Left,
            Zone::Right,
            Zone::Top,
            Zone::Bottom,
        ] {
            let mut d = Desktop {
                layout: Layout::Grid,
                ..Default::default()
            };
            d.reconcile(&[1, 2, 3, 4], area, 10.);
            let from = d.frames(area, 10.)[0].1;
            d.resize(
                1,
                from,
                rect(0., 0., from.size.width + 100., from.size.height + 80.),
                area,
                10.,
            );
            let slots: Vec<_> = d.frames(area, 10.).iter().map(|(_, r)| *r).collect();
            d.place(1, 4, zone);
            assert_eq!(
                d.frames(area, 10.)
                    .iter()
                    .map(|(_, r)| *r)
                    .collect::<Vec<_>>(),
                slots
            );
            d.reconcile(&[1, 2, 3, 4], area, 10.);
            assert_eq!(
                d.frames(area, 10.)
                    .iter()
                    .map(|(_, r)| *r)
                    .collect::<Vec<_>>(),
                slots
            );
        }
    }
    #[test]
    fn grid_snap_covers_area_and_round_trips_across_window_ids() {
        let area = rect(-1440., 30., 1440., 900.);
        let mut d = Desktop {
            layout: Layout::Grid,
            grid: Some((120, 6)),
            ..Default::default()
        };
        d.reconcile(&[1, 2, 3, 4], area, 10.);
        let from = d.frames(area, 10.)[0].1;
        d.resize(
            1,
            from,
            rect(
                from.origin.x,
                from.origin.y,
                from.size.width + 47.,
                from.size.height + 65.,
            ),
            area,
            10.,
        );
        let frames = d.frames(area, 10.);
        assert_full_area(&frames, area, 10.);
        let edge = (frames[0].1.origin.x + frames[0].1.size.width + 5. - area.origin.x) / 12.;
        assert!((edge - edge.round()).abs() < 1e-9);
        let saved = d.save_distribution(area).unwrap();
        let mut other = Desktop::default();
        other.reconcile(&[10, 20, 30, 40], area, 10.);
        assert!(other.restore_distribution(&saved));
        assert_eq!(other.grid, Some((120, 6)));
        assert_eq!(
            other
                .frames(area, 10.)
                .iter()
                .map(|(_, r)| *r)
                .collect::<Vec<_>>(),
            frames.iter().map(|(_, r)| *r).collect::<Vec<_>>()
        );
        assert!(!other.restore_distribution("2 2 4 120 6 vNaN w0 w1"));
        assert!(!other.restore_distribution("2 2 4 120 6 v0.5 w0 w0"));
        other.remove(10);
        assert!(!other.restore_distribution(&saved));
    }
    #[test]
    fn intent_uses_nearest_edge_and_center() {
        let r = rect(-500., 20., 400., 300.);
        assert_eq!(zone(r, NSPoint::new(-490., 170.)), Zone::Left);
        assert_eq!(zone(r, NSPoint::new(-300., 25.)), Zone::Top);
        assert_eq!(zone(r, NSPoint::new(-300., 170.)), Zone::Center);
    }
    #[test]
    fn top_third_drop_joins_strip_and_keeps_one_leaf_per_slot() {
        let area = rect(0., 30., 1600., 900.);
        let mut d = Desktop::configured(Layout::Spiral, None);
        d.reconcile(&[1, 2, 3], area, 10.);
        let before = d.frames(area, 10.);
        let slot = before.iter().find(|(id, _)| *id == 1).unwrap().1;
        let p = NSPoint::new(
            slot.origin.x + slot.size.width / 2.,
            slot.origin.y + slot.size.height / 4.,
        );
        assert_eq!(
            drop_intent(area, &before, 3, p, true, true),
            Some(DropIntent::Group(1))
        );
        assert_eq!(
            drop_intent(area, &before, 3, p, true, false),
            Some(DropIntent::Window(1, Zone::Center))
        );
        let joined = d
            .proposed_drop(3, DropIntent::Group(1), area, 10.)
            .expect("join must fit");
        assert_layout_invariants(&joined, area);
        // Only the visible tab owns a slot: the strip takes the target's and the
        // dropped window's own leaf collapses.
        assert_eq!(joined.order, vec![3, 2]);
        let frames = joined.frames(area, 10.);
        assert_eq!(
            frames.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert_full_area(&frames, area, 10.);
        assert_eq!(
            joined.tab_strip(3).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![1, 3], 1))
        );
        assert_eq!(
            joined.tab_strip(1).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![1, 3], 1))
        );
        assert!(joined.is_hidden_tab(1));
        assert!(!joined.is_hidden_tab(3));
        assert_eq!(joined.tab_role(2), None);
        // A hidden tab is not an arrival: reconciling keeps two slots.
        let mut stable = joined.clone();
        stable.reconcile(&[1, 2, 3], area, 10.);
        assert_eq!(stable.frames(area, 10.), frames);
        // Selecting the hidden tab moves the same leaf, not a new split.
        assert_eq!(stable.select_tab(1), Some(3));
        assert_eq!(stable.order, vec![1, 2]);
        let selected = stable.frames(area, 10.);
        assert_eq!(
            selected.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            selected.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
            frames.iter().map(|(_, r)| *r).collect::<Vec<_>>()
        );
    }
    #[test]
    fn joining_strips_promotes_the_sibling_and_closing_the_tab_frees_no_slot() {
        let area = rect(0., 0., 1600., 1000.);
        let mut d = Desktop::configured(Layout::Grid, None);
        d.reconcile(&[1, 2, 3, 4], area, 10.);
        let first = d
            .proposed_drop(2, DropIntent::Group(1), area, 10.)
            .expect("first join");
        assert_eq!(first.order, vec![2, 3, 4]);
        let arrival = first
            .proposed_drop(5, DropIntent::Group(3), area, 10.)
            .expect("arrival joins");
        assert_eq!(arrival.order, vec![2, 5, 4]);
        assert_eq!(
            arrival.tab_strip(5).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![3, 5], 1))
        );
        // Merging a whole strip into another keeps both existing slots: the
        // promoted sibling inherits the leaf of the strip that lost its tab.
        let merged = arrival
            .proposed_drop(5, DropIntent::Group(2), area, 10.)
            .expect("merge strips");
        assert_layout_invariants(&merged, area);
        // Merging keeps both slots: the dropped tab takes the target's leaf and
        // the promoted sibling keeps the leaf of the strip that lost it.
        assert_eq!(merged.order, vec![5, 3, 4]);
        assert_eq!(
            merged.tab_strip(5).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![1, 2, 5], 2))
        );
        assert_eq!(
            merged.tab_strip(3).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![3], 0))
        );
        assert_eq!(merged.tab_strip(4), None);
        // Closing the visible tab promotes its sibling into the very same rect.
        let mut closed = merged.clone();
        let target = merged.frames(area, 10.)[0].1;
        assert_eq!(closed.remove(5), Some(2));
        assert_eq!(closed.order, vec![2, 3, 4]);
        assert_eq!(closed.frames(area, 10.)[0].1, target);
        assert_eq!(
            closed.tab_strip(2).map(|(m, a)| (m.to_vec(), a)),
            Some((vec![1, 2], 1))
        );
    }
    #[test]
    fn group_intent_rejects_unknown_targets_and_adopts_visible_tabs() {
        let area = rect(0., 0., 1200., 800.);
        let empty = Desktop::configured(Layout::Grid, None);
        assert!(
            empty
                .proposed_drop(1, DropIntent::Group(9), area, 10.)
                .is_none()
        );
        let mut d = Desktop::configured(Layout::VerticalMaster, None);
        d.reconcile(&[1, 2, 3], area, 10.);
        let joined = d
            .proposed_drop(4, DropIntent::Group(2), area, 10.)
            .expect("arrival joins a slot");
        assert_eq!(joined.order, vec![1, 4, 3]);
        assert!(
            joined
                .proposed_drop(4, DropIntent::Group(77), area, 10.)
                .is_none()
        );
        // The compositor can show the hidden tab before the next model event.
        let mut adopted = joined.clone();
        adopted.sync_tabs(&[1, 2, 3]);
        assert_eq!(adopted.order, vec![1, 2, 3]);
        assert_eq!(
            adopted
                .frames(area, 10.)
                .iter()
                .map(|(_, r)| *r)
                .collect::<Vec<_>>(),
            joined
                .frames(area, 10.)
                .iter()
                .map(|(_, r)| *r)
                .collect::<Vec<_>>()
        );
        // An already-visible leaf is left untouched.
        adopted.sync_tabs(&[1, 2, 3]);
        assert_eq!(adopted.order, vec![1, 2, 3]);
    }
    #[test]
    fn reordering_tabs_preserves_active_identity_geometry_and_other_groups() {
        let area = rect(-900., 30., 2000., 1200.);
        for layout in Layout::ALL {
            let mut desktop = Desktop::configured(layout, Some((12, 6)));
            desktop.reconcile(&[1, 2, 3], area, 10.);
            for (id, target) in [(4, 2), (5, 4), (6, 3)] {
                desktop = desktop
                    .proposed_drop(id, DropIntent::Group(target), area, 10.)
                    .unwrap();
            }
            let before = desktop.clone();
            desktop.max_group_tabs = 1; // Admission limits do not forbid reordering.
            for (id, index, members, active) in [
                (2, 2, [4, 5, 2], 1),
                (5, 0, [5, 4, 2], 0),
                (2, 0, [2, 5, 4], 1),
                (5, 2, [2, 4, 5], 2),
                (4, 1, [2, 4, 5], 2),
            ] {
                assert!(desktop.reorder_tab(id, index));
                assert_eq!(desktop.tab_strip(id), Some((&members[..], active)));
                assert_eq!(desktop.order, before.order);
                assert_eq!(
                    desktop.window_frames(area, 10.),
                    before.window_frames(area, 10.)
                );
                assert_eq!(desktop.groups()[1], before.groups()[1]);
                assert_layout_invariants(&desktop, area);
            }
            let unchanged = desktop.clone();
            assert!(!desktop.reorder_tab(99, 0));
            assert!(!desktop.reorder_tab(1, 0)); // Ungrouped window.
            assert!(!desktop.reorder_tab(2, 3));
            assert!(desktop == unchanged);
        }
    }
    #[test]
    fn closing_hidden_members_preserves_active_identity_and_slot_in_every_layout() {
        let area = rect(0., 30., 2000., 1200.);
        for layout in Layout::ALL {
            let mut desktop = Desktop::configured(layout, None);
            desktop.reconcile(&[1, 2, 3], area, 10.);
            desktop = desktop
                .proposed_drop(4, DropIntent::Group(2), area, 10.)
                .unwrap();
            desktop = desktop
                .proposed_drop(5, DropIntent::Group(4), area, 10.)
                .unwrap();
            let before = desktop.frames(area, 10.);
            for hidden in [2, 4] {
                assert_eq!(desktop.remove(hidden), None);
                assert_eq!(desktop.groups()[0].leaf(), Some(5));
                assert_eq!(desktop.tab_role(5), Some(true));
                assert_eq!(desktop.frames(area, 10.), before);
                assert_layout_invariants(&desktop, area);
            }
            desktop.remove(5);
            assert!(desktop.groups().is_empty());
            assert_eq!(desktop.order, vec![1, 3]);
            assert_layout_invariants(&desktop, area);
        }
    }

    #[test]
    fn native_tab_replacement_renames_the_mosaic_member_without_duplicate_ownership() {
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop = desktop
            .proposed_drop(4, DropIntent::Group(2), area, 10.)
            .unwrap();
        let slot = desktop
            .frames(area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        assert!(desktop.replace_window(4, 7));
        assert_eq!(desktop.tab_strip(7), Some((&[2, 7][..], 1)));
        assert_eq!(desktop.tab_role(4), None);
        assert!(!desktop.replace_window(7, 1));
        assert_eq!(desktop.select_tab(2), Some(7));
        assert_eq!(
            desktop
                .frames(area, 10.)
                .into_iter()
                .find(|(id, _)| *id == 2)
                .unwrap()
                .1,
            slot
        );
        assert_eq!(desktop.select_tab(7), Some(2));
        assert_layout_invariants(&desktop, area);
    }

    #[test]
    fn group_arrivals_and_merges_preserve_customized_slots() {
        for area in [
            rect(0., 30., 2200., 1300.),
            rect(-900., -400., 1000., 1900.),
        ] {
            for layout in Layout::ALL {
                let mut desktop = Desktop::configured(layout, None);
                desktop.reconcile(&[1, 2, 3, 4], area, 10.);
                let from = desktop.frames(area, 10.)[0].1;
                let mut to = from;
                to.size.width *= 0.9;
                to.size.height *= 0.9;
                desktop.resize(1, from, to, area, 10.);
                let before = desktop.frames(area, 10.);
                for (id, target) in [(5, 1), (6, 2), (6, 5)] {
                    desktop = desktop
                        .proposed_drop(id, DropIntent::Group(target), area, 10.)
                        .unwrap();
                    let frames = desktop.frames(area, 10.);
                    assert_eq!(frames.len(), before.len());
                    for ((_, actual), (_, expected)) in frames.iter().zip(&before) {
                        assert!(
                            (actual.origin.x - expected.origin.x).abs() < 0.001,
                            "{layout:?}"
                        );
                        assert!(
                            (actual.origin.y - expected.origin.y).abs() < 0.001,
                            "{layout:?}"
                        );
                        assert!(
                            (actual.size.width - expected.size.width).abs() < 0.001,
                            "{layout:?}"
                        );
                        assert!(
                            (actual.size.height - expected.size.height).abs() < 0.001,
                            "{layout:?}"
                        );
                    }
                    assert_layout_invariants(&desktop, area);
                }
                assert_eq!(desktop.order, vec![6, 2, 3, 4]);
                assert_eq!(desktop.select_tab(1), Some(6));
                assert_layout_invariants(&desktop, area);
            }
        }
    }

    #[test]
    fn dragging_active_or_hidden_group_members_out_creates_an_independent_slot() {
        for area in [
            rect(0., 30., 2000., 1200.),
            rect(-900., -400., 1000., 1900.),
        ] {
            for layout in Layout::ALL {
                let mut desktop = Desktop::configured(layout, None);
                desktop.reconcile(&[1, 2, 3], area, 10.);
                let grouped = desktop
                    .proposed_drop(4, DropIntent::Group(2), area, 10.)
                    .unwrap();
                for id in [2, 4] {
                    for intent in [
                        DropIntent::Window(3, Zone::Center),
                        DropIntent::Screen(Zone::Left),
                        DropIntent::Screen(Zone::Bottom),
                    ] {
                        let detached = grouped.proposed_drop(id, intent, area, 10.).unwrap();
                        assert_eq!(detached.tab_role(id), None);
                        let mut members = detached.order.clone();
                        members.sort_unstable();
                        assert_eq!(members, vec![1, 2, 3, 4]);
                        assert_layout_invariants(&detached, area);
                    }
                }
            }
        }
    }

    #[test]
    fn reconciliation_removes_all_absent_promotions_but_keeps_a_visible_sibling() {
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2], area, 10.);
        desktop = desktop
            .proposed_drop(3, DropIntent::Group(1), area, 10.)
            .unwrap();
        let mut visible = desktop.clone();
        visible.reconcile(&[1, 2], area, 10.);
        assert_eq!(visible.order, vec![1, 2]);
        assert_eq!(visible.tab_role(3), Some(false));
        desktop.reconcile(&[2], area, 10.);
        assert_eq!(desktop.order, vec![2]);
        assert!(desktop.groups().is_empty());
        assert_eq!(desktop.frames(area, 10.), vec![(2, area)]);
    }

    #[test]
    fn tab_limit_rejects_admission_atomically_but_keeps_existing_members() {
        let area = rect(0., 30., 2200., 1400.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 6], area, 10.);
        for id in 2..=5 {
            desktop = desktop
                .proposed_drop(id, DropIntent::Group(id - 1), area, 10.)
                .unwrap();
        }
        desktop = desktop
            .proposed_drop(7, DropIntent::Group(6), area, 10.)
            .unwrap();
        let before = desktop.clone();
        for id in [6, 7, 8] {
            assert!(
                desktop
                    .proposed_drop(id, DropIntent::Group(5), area, 10.)
                    .is_none()
            );
            assert!(desktop == before);
        }
        desktop.max_group_tabs = 2;
        let selected = desktop
            .proposed_drop(1, DropIntent::Group(5), area, 10.)
            .unwrap();
        assert_eq!(selected.tab_strip(1), Some((&[1, 2, 3, 4, 5][..], 0)));
        assert_eq!(selected.tab_strip(7), Some((&[6, 7][..], 1)));
        assert!(
            selected
                .proposed_drop(6, DropIntent::Group(1), area, 10.)
                .is_none()
        );
        let detached = selected
            .proposed_drop(3, DropIntent::Screen(Zone::Right), area, 10.)
            .unwrap();
        assert_eq!(detached.tab_role(3), None);
        assert_eq!(detached.tab_strip(1).unwrap().0, &[1, 2, 4, 5]);
        desktop.max_group_tabs = 6;
        let extended = desktop
            .proposed_drop(8, DropIntent::Group(5), area, 10.)
            .unwrap();
        assert_eq!(extended.tab_strip(8).unwrap().0, &[1, 2, 3, 4, 5, 8]);
        desktop.reset_template(Layout::Grid, area);
        assert!(
            desktop
                .proposed_drop(8, DropIntent::Group(5), area, 10.)
                .is_some()
        );
    }

    #[test]
    fn tab_headers_reserve_physical_space_and_singletons_reclaim_it() {
        for area in [
            rect(0., 30., 2200., 1400.),
            rect(-1000., -400., 1200., 2000.),
        ] {
            for layout in Layout::ALL {
                for grid in [None, Some((18, 12))] {
                    let mut desktop = Desktop::configured(layout, grid);
                    desktop.reconcile(&[1, 2, 3], area, 10.);
                    desktop = desktop
                        .proposed_drop(4, DropIntent::Group(2), area, 10.)
                        .unwrap();
                    let logical = desktop.frames(area, 10.);
                    let physical = desktop.window_frames(area, 10.);
                    for ((id, slot), (physical_id, window)) in logical.iter().zip(&physical) {
                        assert_eq!(id, physical_id);
                        assert_eq!(desktop.slot_frame(*id, *window), *slot);
                        assert_eq!(window.origin.x, slot.origin.x);
                        assert_eq!(window.size.width, slot.size.width);
                        assert!(window.size.height >= 90.);
                        if *id == 4 {
                            assert_eq!(
                                window.origin.y,
                                slot.origin.y + TAB_BAR_HEIGHT + TAB_BAR_GAP
                            );
                            assert_eq!(desktop.window_frame(2, *slot), *window);
                        } else {
                            assert_eq!(window, slot);
                        }
                    }
                    desktop.remove(4);
                    assert_eq!(desktop.window_frames(area, 10.), desktop.frames(area, 10.));
                    assert_layout_invariants(&desktop, area);
                }
            }
        }
    }

    #[test]
    fn tab_headers_are_included_in_minimum_height_even_without_app_constraints() {
        for grid in [None, Some((12, 12))] {
            let mut desktop = Desktop::configured(Layout::Spiral, grid);
            let short = rect(-400., 30., 400., 100.);
            desktop.reconcile(&[1], short, 10.);
            assert!(
                desktop
                    .proposed_drop(2, DropIntent::Group(1), short, 10.)
                    .is_none()
            );
            let exact = rect(-400., 30., 400., 90. + TAB_BAR_HEIGHT + TAB_BAR_GAP);
            let grouped = desktop
                .proposed_drop(2, DropIntent::Group(1), exact, 10.)
                .unwrap();
            assert_eq!(grouped.window_frames(exact, 10.)[0].1.size.height, 90.);
        }
    }

    #[test]
    fn a_group_respects_hidden_minimums_without_reflow_on_selection() {
        use objc2_foundation::NSSize;
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop = desktop
            .proposed_drop(4, DropIntent::Group(2), area, 10.)
            .unwrap();
        let minimums = std::collections::HashMap::from([
            (2, NSSize::new(900., 300.)),
            (4, NSSize::new(180., 100.)),
        ]);
        desktop.set_minimum_sizes(&minimums);
        desktop.fit(area, 10.);
        let before = desktop.window_frames(area, 10.);
        let slot = before.iter().find(|(id, _)| *id == 4).unwrap().1;
        assert!(slot.size.width >= 900. && slot.size.height >= 300.);
        desktop.select_tab(2);
        desktop.set_minimum_sizes(&minimums);
        desktop.fit(area, 10.);
        let selected = desktop.window_frames(area, 10.);
        assert_eq!(
            selected.iter().map(|(_, frame)| frame).collect::<Vec<_>>(),
            before.iter().map(|(_, frame)| frame).collect::<Vec<_>>()
        );
        assert_eq!(desktop.minimums[&4], minimums[&4]);
        assert_layout_invariants(&desktop, area);
    }

    #[test]
    fn replacing_a_hidden_tab_id_renames_only_its_strip_entry() {
        let area = rect(0., 30., 1800., 1100.);
        let mut desktop = Desktop::configured(Layout::Grid, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let joined = desktop
            .proposed_drop(4, DropIntent::Group(2), area, 10.)
            .unwrap();
        let before = joined.frames(area, 10.);
        let mut desktop = joined.clone();
        // The id of a hidden tab is replaced by AppKit without touching the
        // slot owner: the strip follows and no new leaf appears.
        assert!(desktop.replace_window(2, 9));
        assert_eq!(desktop.tab_strip(9), Some((&[9, 4][..], 1)));
        assert_eq!(desktop.tab_role(2), None);
        assert_eq!(desktop.tab_role(9), Some(false));
        assert_eq!(desktop.tab_role(4), Some(true));
        assert_eq!(desktop.order, vec![1, 4, 3]);
        let frames = desktop.frames(area, 10.);
        assert_eq!(
            frames.iter().map(|(_, r)| *r).collect::<Vec<_>>(),
            before.iter().map(|(_, r)| *r).collect::<Vec<_>>()
        );
        // The renamed entry still swaps into the very same rect.
        assert_eq!(desktop.select_tab(9), Some(4));
        assert_eq!(
            desktop
                .frames(area, 10.)
                .iter()
                .map(|(_, r)| *r)
                .collect::<Vec<_>>(),
            before.iter().map(|(_, r)| *r).collect::<Vec<_>>()
        );
        assert_layout_invariants(&desktop, area);
        // A second native id change follows the same strip entry again.
        assert!(desktop.replace_window(9, 77));
        assert_eq!(desktop.tab_strip(4), Some((&[77, 4][..], 0)));
        assert_eq!(desktop.tab_role(9), None);
        assert_eq!(desktop.order, vec![1, 77, 3]);
        // An id unknown to the model changes nothing.
        assert!(!desktop.replace_window(98, 4));
        assert_eq!(desktop.order, vec![1, 77, 3]);
    }

    #[test]
    fn same_strip_id_swap_collapses_the_duplicate_and_other_strips_stay_exact() {
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        let mut desktop = desktop
            .proposed_drop(4, DropIntent::Group(2), area, 10.)
            .unwrap();
        let slot = desktop
            .frames(area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 4)
            .unwrap()
            .1;
        assert!(desktop.replace_window(4, 7));
        assert_eq!(desktop.tab_strip(7), Some((&[2, 7][..], 1)));
        // The compositor reuses the sibling's id for the same tab: the strip
        // collapses to one entry and the leaf keeps the same rectangle.
        assert!(desktop.replace_window(7, 2));
        assert_eq!(desktop.tab_strip(2), Some((&[2][..], 0)));
        assert_eq!(desktop.tab_role(7), None);
        assert_eq!(desktop.tab_role(2), Some(true));
        assert_eq!(desktop.order, vec![1, 2, 3]);
        assert_eq!(
            desktop
                .frames(area, 10.)
                .into_iter()
                .find(|(id, _)| *id == 2)
                .unwrap()
                .1,
            slot
        );
        assert_layout_invariants(&desktop, area);
        // A fresh id owned by another strip belongs to a different window.
        let mut other = desktop
            .proposed_drop(5, DropIntent::Group(3), area, 10.)
            .unwrap();
        let order_before = other.order.clone();
        let groups_before: Vec<_> = other
            .groups()
            .iter()
            .map(|g| (g.members.clone(), g.active))
            .collect();
        assert!(!other.replace_window(2, 5));
        assert_eq!(other.order, order_before);
        let groups_after: Vec<_> = other
            .groups()
            .iter()
            .map(|g| (g.members.clone(), g.active))
            .collect();
        assert_eq!(groups_after, groups_before);
    }

    #[test]
    fn template_reset_keeps_strips_one_leaf_and_no_reinsertion() {
        let area = rect(200., -900., 2400., 1400.);
        for (from_layout, to_layout) in [
            (Layout::Grid, Layout::Centered),
            (Layout::CenteredWide, Layout::Spiral),
            (Layout::Spiral, Layout::VerticalMaster),
        ] {
            let mut desktop = Desktop::configured(from_layout, None);
            desktop.reconcile(&[1, 2, 3, 4], area, 10.);
            let mut desktop = desktop
                .proposed_drop(5, DropIntent::Group(3), area, 10.)
                .unwrap();
            let order_before = desktop.order.clone();
            let strip = desktop
                .tab_strip(5)
                .map(|(members, active)| (members.to_vec(), active))
                .unwrap();
            assert_eq!(strip, (vec![3, 5], 1));
            desktop.reset_template(to_layout, area);
            // Same slots: the hidden member was neither reinserted nor lost.
            assert_eq!(desktop.order, order_before);
            assert_eq!(
                desktop
                    .tab_strip(5)
                    .map(|(members, active)| (members.to_vec(), active)),
                Some(strip)
            );
            assert_eq!(desktop.tab_role(3), Some(false));
            assert_eq!(desktop.tab_role(5), Some(true));
            let after = desktop.frames(area, 10.);
            assert_layout_invariants(&desktop, area);
            // The strip's hidden tab still swaps into the rebuilt leaf.
            let leaf = after.iter().find(|(id, _)| *id == 5).unwrap().1;
            assert_eq!(desktop.select_tab(3), Some(5));
            assert_eq!(
                desktop
                    .frames(area, 10.)
                    .into_iter()
                    .find(|(id, _)| *id == 3)
                    .unwrap()
                    .1,
                leaf
            );
            // Closing both tabs frees exactly one slot for the whole strip.
            assert_eq!(desktop.remove(3), Some(5));
            assert_eq!(desktop.remove(5), None);
            assert!(desktop.groups().is_empty());
            assert_eq!(desktop.order, vec![1, 2, 4]);
            assert_layout_invariants(&desktop, area);
        }
    }
}
