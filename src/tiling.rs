//! Pure layout state. Window identities are session-local; templates are preferences.
use crate::native::rect;
use objc2_foundation::{NSPoint, NSRect};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropIntent {
    Screen(Zone),
    Window(u32, Zone),
}
/// Global monitor coordinates in points; thresholds scale with the usable area.
pub fn drop_intent(
    area: NSRect,
    frames: &[(u32, NSRect)],
    dragged: u32,
    p: NSPoint,
    smart: bool,
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
        return frames
            .iter()
            .min_by(|(_, a), (_, b)| distance_to_rect(*a, p).total_cmp(&distance_to_rect(*b, p)))
            .map(|(id, _)| DropIntent::Window(*id, Zone::Center));
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
    Some(DropIntent::Window(
        target.0,
        if smart {
            zone(target.1, p)
        } else {
            Zone::Center
        },
    ))
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
#[derive(Clone, Debug)]
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
            if low <= high && available > 0. {
                *ratio = (*ratio * available).clamp(low, high) / available;
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
#[derive(Clone, Default)]
pub struct Desktop {
    pub layout: Layout,
    pub order: Vec<u32>,
    pub grid: Option<(u32, u32)>,
    resized: Option<(Layout, Vec<u32>, Node)>,
    minimums: std::collections::HashMap<u32, objc2_foundation::NSSize>,
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
    pub fn replace_window(&mut self, old: u32, new: u32) -> bool {
        if !self.order.contains(&old) || self.order.contains(&new) {
            return false;
        }
        if let Some(minimum) = self.minimums.remove(&old) {
            self.minimums.insert(new, minimum);
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
    pub fn remove(&mut self, id: u32) {
        self.minimums.remove(&id);
        if !self.order.contains(&id) {
            return;
        }
        let old = self.order.clone();
        self.order.retain(|w| *w != id);
        if self.layout.centered_axis().is_some() {
            // Keep the old outer ratios as a reference. A changed membership no
            // longer matches `resized`, so base_tree rebuilds both sidebars.
            return;
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
    }
    #[cfg(test)]
    pub fn reconcile(&mut self, ids: &[u32], area: NSRect, gap: f64) {
        self.reconcile_at(ids, area, gap, None);
    }
    pub fn reconcile_at(&mut self, ids: &[u32], area: NSRect, gap: f64, focused: Option<u32>) {
        for id in self.order.clone() {
            if !ids.contains(&id) {
                self.remove(id);
            }
        }
        for id in ids {
            if !self.order.contains(id) {
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
                if let (Some((layout, order, tree)), Some((target, r))) =
                    (&mut self.resized, target)
                {
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
        let mut result = self.clone();
        if self.order.contains(&id) {
            let frames = self.frames(area, gap);
            if frames.len() != self.order.len() {
                return None;
            }
            let target = match intent {
                DropIntent::Window(target, _) => target,
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
            // Reject a swap that cannot fit the receiving apps. Never accept an
            // empty proposal or change the other slots to accommodate the swap.
            for (old, frame) in &frames {
                let new = if *old == id {
                    target
                } else if *old == target {
                    id
                } else {
                    *old
                };
                if frame.size.width + 0.001 < self.window_minimum(new, true)
                    || frame.size.height + 0.001 < self.window_minimum(new, false)
                {
                    return None;
                }
            }
            let mut tree = self.effective_tree(area, gap)?;
            result.place(id, target, Zone::Center);
            tree.remap(&self.order, &result.order);
            result.resized = Some((result.layout, result.order.clone(), tree));
            return Some(result);
        }
        let old_tree = self.effective_tree(area, gap);
        if !self.order.is_empty() && old_tree.is_none() {
            return None;
        }
        let remaining = old_tree.and_then(|tree| tree.remove(id));
        let tree = match (remaining, intent) {
            (None, _) => Node::Leaf(id),
            (Some(tree), DropIntent::Screen(side)) => {
                let before = matches!(side, Zone::Left | Zone::Top);
                let vertical = matches!(side, Zone::Left | Zone::Right);
                if before {
                    Node::branch(vertical, 0.5, Node::Leaf(id), tree)
                } else {
                    Node::branch(vertical, 0.5, tree, Node::Leaf(id))
                }
            }
            (Some(mut tree), DropIntent::Window(target, side)) => {
                if !tree.insert(target, id, side) {
                    return None;
                }
                tree
            }
        };
        result.order.retain(|w| *w != id);
        match intent {
            DropIntent::Window(target, side) => {
                let index = result.order.iter().position(|w| *w == target)?
                    + usize::from(matches!(side, Zone::Right | Zone::Bottom | Zone::Center));
                result.order.insert(index, id);
            }
            DropIntent::Screen(side) => {
                if matches!(side, Zone::Left | Zone::Top) {
                    result.order.insert(0, id)
                } else {
                    result.order.push(id)
                }
            }
        }
        result.resized = Some((result.layout, result.order.clone(), tree));
        let frames = result.frames(area, gap);
        if frames.len() != result.order.len()
            || frames
                .iter()
                .any(|(_, r)| r.size.width < 120. || r.size.height < 90.)
        {
            return None;
        }
        Some(result)
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
        if !self.minimums.is_empty() {
            let minimum = |id, vertical| self.window_minimum(id, vertical);
            if tree.minimum_with(true, gap, &minimum) > area.size.width
                || tree.minimum_with(false, gap, &minimum) > area.size.height
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
        true
    }
    fn window_minimum(&self, id: u32, vertical: bool) -> f64 {
        let fallback: f64 = if vertical { 120. } else { 90. };
        self.minimums.get(&id).map_or(fallback, |size| {
            fallback.max(if vertical { size.width } else { size.height })
        })
    }
    pub fn set_minimum_sizes(
        &mut self,
        minimums: &std::collections::HashMap<u32, objc2_foundation::NSSize>,
    ) {
        self.minimums
            .retain(|id, _| self.order.contains(id) && minimums.contains_key(id));
        for id in &self.order {
            if let Some(size) = minimums.get(id) {
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
        if ![to.origin.x, to.origin.y, to.size.width, to.size.height]
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
        self.grid = grid;
        self.layout = Layout::from_index(index);
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
        let ids = self.order.clone();
        let grid = self.grid;
        let minimums = self.minimums.clone();
        *self = Self {
            layout,
            grid,
            minimums,
            ..Default::default()
        };
        self.reconcile_at(&ids, area, 10., None);
    }
    pub fn frames(&self, r: NSRect, gap: f64) -> Vec<(u32, NSRect)> {
        let mut out = vec![];
        if let Some(mut tree) = self.effective_tree(r, gap) {
            if let Some((cols, rows)) = self.grid {
                if self.minimums.is_empty() {
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
                return out;
            }
            tree.frames(r, gap.clamp(0., 24.), &mut out);
        }
        out
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
    fn slot_swap_rejects_incompatible_app_minimum_without_destroying_layout() {
        let area = rect(0., 0., 1600., 1000.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[1, 2, 3], area, 10.);
        desktop.set_minimum_sizes(&[(1, objc2_foundation::NSSize::new(600., 900.))].into());
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
                    drop_intent(area, &frames, 99, p, true),
                    Some(DropIntent::Screen(side))
                );
            }
            let target = frames.iter().find(|(id, _)| *id == 2).unwrap().1;
            let center = NSPoint::new(
                target.origin.x + target.size.width / 2.,
                target.origin.y + target.size.height / 2.,
            );
            assert_eq!(
                drop_intent(area, &frames, 99, center, true),
                Some(DropIntent::Window(2, Zone::Center))
            );
            assert_eq!(
                drop_intent(area, &frames, 2, center, true),
                Some(DropIntent::Window(2, Zone::Center))
            );
            assert_eq!(
                drop_intent(
                    area,
                    &frames,
                    1,
                    NSPoint::new(area.origin.x - 30., center.y),
                    true
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
            drop_intent(area, &frames, 99, p, true),
            Some(DropIntent::Window(2, Zone::Left))
        );
        assert_eq!(
            drop_intent(area, &frames, 99, p, false),
            Some(DropIntent::Window(2, Zone::Center))
        );
    }
    #[test]
    fn dropping_on_another_monitor_preserves_source_and_destination_splits() {
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
        assert_eq!(
            frames.iter().find(|(id, _)| *id == 1).unwrap().1.size.width,
            portrait.size.width
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
}
