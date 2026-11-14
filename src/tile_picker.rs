//! Native, transient template gallery. Preview uses the same pure model as commit.
use crate::{
    native::{Obj, alloc_view, color, label, rect},
    tile_manager::PickerContext,
    tiling::{Desktop, Layout},
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::{NSRect, NSString};
use std::{ptr, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Select(usize),
    Move(isize),
    Confirm,
    Cancel,
}

/// A gallery card: one of the eight built-in templates or one saved distribution.
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    Layout(Layout),
    Custom(String),
}
/// Mirror of the acceptance test used by `apply_custom_distribution`.
fn fits_area(desktop: &Desktop, area: NSRect) -> bool {
    let frames = desktop.window_frames(area, 10.);
    !frames.is_empty()
        && frames.len() == desktop.order.len()
        && frames.iter().all(|(_, r)| {
            [r.origin.x, r.origin.y, r.size.width, r.size.height]
                .iter()
                .all(|n| n.is_finite())
                && r.size.width >= 80.
                && r.size.height >= 60.
                && r.origin.x >= area.origin.x - 0.001
                && r.origin.y >= area.origin.y - 0.001
                && r.origin.x + r.size.width <= area.origin.x + area.size.width + 0.001
                && r.origin.y + r.size.height <= area.origin.y + area.size.height + 0.001
        })
}
fn custom_proposal(
    context: &PickerContext,
    template: &crate::custom_templates::Template,
) -> Option<Desktop> {
    if template.count != context.desktop.order.len() {
        return None;
    }
    let mut desktop = context.desktop.clone();
    if !desktop.restore_distribution(&template.distribution) {
        return None;
    }
    // Named templates preserve the current grid, app minimums and tab groups,
    // exactly like Manager::restore_distribution_with_grid.
    desktop.grid = context.desktop.grid;
    fits_area(&desktop, context.area).then_some(desktop)
}
pub fn key_input(key: u16, columns: usize, shift: bool) -> Option<Input> {
    Some(match key {
        18 => Input::Select(0),
        19 => Input::Select(1),
        20 => Input::Select(2),
        21 => Input::Select(3),
        23 => Input::Select(4),
        22 => Input::Select(5),
        26 => Input::Select(6),
        28 => Input::Select(7),
        25 => Input::Select(8),
        29 => Input::Select(9),
        123 => Input::Move(-1),
        125 => Input::Move(columns as isize),
        126 => Input::Move(-(columns as isize)),
        48 => Input::Move(if shift { -1 } else { 1 }),
        36 | 76 => Input::Confirm,
        53 => Input::Cancel,
        _ => return None,
    })
}
struct Gallery {
    frame: NSRect,
    viewport: NSRect,
    document: NSRect,
    cards: Vec<NSRect>,
    columns: usize,
}

fn gallery_geometry(area: NSRect, count: usize) -> Gallery {
    let portrait = area.size.height > area.size.width;
    let columns = if portrait { 2 } else { 4 };
    let rows = count.div_ceil(columns);
    let (cw, ch) = if portrait { (180., 244.) } else { (210., 158.) };
    let natural_width = columns as f64 * (cw + 12.) + 36.;
    // A large library scrolls instead of shrinking cards until their thumbnails
    // have negative dimensions. Width still adapts to the physical screen.
    let scale = ((area.size.width - 48.) / natural_width).clamp(0.1, 1.);
    let (cw, ch, gap, margin) = (cw * scale, ch * scale, 12. * scale, 24. * scale);
    // Keep a gutter even when macOS uses non-overlay scrollbars.
    let width = natural_width * scale + 16.;
    let content_height = rows as f64 * ch + rows.saturating_sub(1) as f64 * gap;
    let height = (content_height + 130.).min(area.size.height - 32.);
    let frame = rect(
        (area.size.width - width) / 2.,
        (area.size.height - height) / 2.,
        width,
        height,
    );
    let viewport = rect(
        frame.origin.x + margin,
        frame.origin.y + 48.,
        width - 2. * margin,
        height - 130.,
    );
    let document = rect(0., 0., viewport.size.width, content_height);
    let cards = (0..count)
        .map(|index| {
            rect(
                (index % columns) as f64 * (cw + gap),
                content_height - ch - (index / columns) as f64 * (ch + gap),
                cw,
                ch,
            )
        })
        .collect();
    Gallery {
        frame,
        viewport,
        document,
        cards,
        columns,
    }
}
fn thumbnail_area(card: NSRect, screen: NSRect) -> NSRect {
    let room = rect(
        card.origin.x + 14.,
        card.origin.y + 35.,
        card.size.width - 28.,
        card.size.height - 49.,
    );
    let scale = (room.size.width / screen.size.width).min(room.size.height / screen.size.height);
    let w = screen.size.width * scale;
    let h = screen.size.height * scale;
    rect(
        room.origin.x + (room.size.width - w) / 2.,
        room.origin.y + (room.size.height - h) / 2.,
        w,
        h,
    )
}
unsafe fn plate(parent: Obj, frame: NSRect, alpha: f64, accent: bool, radius: f64) -> Obj {
    unsafe {
        let view = alloc_view(class!(NSView), frame);
        let _: () = msg_send![&view, setWantsLayer:true];
        let layer: Obj = msg_send![&view, layer];
        let c = if accent {
            color(0.48, 0.60, 1., alpha)
        } else {
            color(0.10, 0.12, 0.18, alpha)
        };
        let cg: *const crate::native::CgColor = msg_send![c, CGColor];
        let _: () = msg_send![layer, setBackgroundColor:cg];
        let _: () = msg_send![layer, setCornerRadius:radius];
        let _: () = msg_send![parent, addSubview:&*view];
        Retained::as_ptr(&view) as Obj
    }
}
unsafe fn light_label(parent: Obj, frame: NSRect, value: &str, size: f64, bold: bool) -> Obj {
    unsafe {
        let v = label(
            parent,
            frame,
            value,
            size,
            if bold { 0.35 } else { 0. },
            false,
        );
        let _: () = msg_send![v,setTextColor:color(0.94,0.96,1.,1.)];
        v
    }
}
fn transparent_view_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut builder = ClassBuilder::new(c"AMSpaceLayoutGhost", class!(NSView)).unwrap();
        builder.add_method(
            sel!(hitTest:),
            no_hit as extern "C-unwind" fn(Obj, Sel, objc2_foundation::NSPoint) -> Obj,
        );
        builder.register()
    })
}
extern "C-unwind" fn no_hit(_: Obj, _: Sel, _: objc2_foundation::NSPoint) -> Obj {
    ptr::null_mut()
}
fn panel_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut b = ClassBuilder::new(c"AMSpaceLayoutPanel", class!(NSPanel)).unwrap();
        b.add_method(
            sel!(canBecomeKeyWindow),
            can_key as extern "C-unwind" fn(Obj, Sel) -> Bool,
        );
        b.add_method(
            sel!(sendEvent:),
            send_event as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        b.add_method(
            sel!(resignKeyWindow),
            resign as extern "C-unwind" fn(Obj, Sel),
        );
        b.register()
    })
}
extern "C-unwind" fn can_key(_: Obj, _: Sel) -> Bool {
    Bool::YES
}
extern "C-unwind" fn send_event(panel: Obj, _: Sel, event: Obj) {
    unsafe {
        let _keep = Retained::retain(panel);
        let kind: usize = msg_send![event, type];
        let flags: usize = msg_send![event, modifierFlags];
        if kind == 10 && flags & ((1 << 18) | (1 << 19) | (1 << 20)) == 0 {
            let key: u16 = msg_send![event, keyCode];
            crate::ui::layout_picker_key(key, flags & (1 << 17) != 0);
        } else {
            let _: () = msg_send![super(panel, class!(NSPanel)), sendEvent:event];
        }
    }
}
extern "C-unwind" fn resign(panel: Obj, _: Sel) {
    unsafe {
        let _keep = Retained::retain(panel);
        let _: () = msg_send![super(panel, class!(NSPanel)), resignKeyWindow];
        crate::ui::cancel_layout_picker();
    }
}
fn button_class() -> &'static AnyClass {
    static CLASS: OnceLock<&'static AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| unsafe {
        let mut b = ClassBuilder::new(c"AMSpaceLayoutChoice", class!(NSButton)).unwrap();
        b.add_method(
            sel!(mouseEntered:),
            entered as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        b.register()
    })
}
extern "C-unwind" fn entered(button: Obj, _: Sel, _: Obj) {
    let tag: isize = unsafe { msg_send![button, tag] };
    if tag >= 0 {
        crate::ui::layout_picker_hover(tag as usize);
    }
}
unsafe fn choice(
    parent: Obj,
    frame: NSRect,
    target: Obj,
    index: isize,
    name: &str,
    enabled: bool,
) -> Obj {
    unsafe {
        let button = alloc_view(button_class(), frame);
        let _: () = msg_send![&button,setTitle:&*NSString::from_str("")];
        let _: () = msg_send![&button,setBordered:false];
        let _: () = msg_send![&button,setTarget:target];
        let _: () = msg_send![&button,setAction:sel!(layoutChoice:)];
        let _: () = msg_send![&button,setTag:index];
        let _: () = msg_send![&button,setEnabled:enabled];
        let _: () = msg_send![&button,setAccessibilityLabel:&*NSString::from_str(name)];
        if index >= 0 && enabled {
            let tracking: Allocated<AnyObject> = msg_send![class!(NSTrackingArea), alloc];
            let tracking: Retained<AnyObject> = msg_send![tracking,initWithRect:NSRect::ZERO,options:(1usize|128|512),owner:&*button,userInfo:ptr::null::<AnyObject>()];
            let _: () = msg_send![&button,addTrackingArea:&*tracking];
        }
        let _: () = msg_send![parent,addSubview:&*button];
        Retained::as_ptr(&button) as Obj
    }
}

pub struct Picker {
    panel: Retained<AnyObject>,
    pub context: PickerContext,
    pub selected: usize,
    pub columns: usize,
    valid: Vec<bool>,
    titles: Vec<String>,
    custom: Vec<crate::custom_templates::Template>,
    card_views: Vec<Obj>,
    buttons: Vec<Obj>,
    ghost: Obj,
    status: Obj,
}
impl Picker {
    pub fn new(context: PickerContext, target: Obj, defaults: Obj) -> Option<Self> {
        unsafe {
            let screens: Obj = msg_send![class!(NSScreen), screens];
            let primary: Obj = msg_send![screens, firstObject];
            if primary.is_null() {
                return None;
            }
            let primary: NSRect = msg_send![primary, frame];
            let area = context.area;
            let appkit = rect(
                area.origin.x,
                primary.size.height - area.origin.y - area.size.height,
                area.size.width,
                area.size.height,
            );
            let panel: Allocated<AnyObject> = msg_send![panel_class(), alloc];
            let panel: Retained<AnyObject> = msg_send![panel,initWithContentRect:appkit,styleMask:128usize,backing:2usize,defer:false];
            let _: () = msg_send![&panel,setReleasedWhenClosed:false];
            let _: () = msg_send![&panel,setOpaque:false];
            let _: () = msg_send![&panel,setBackgroundColor:color(0.,0.,0.,0.10)];
            let _: () = msg_send![&panel,setHasShadow:false];
            let _: () = msg_send![&panel,setLevel:3isize];
            let _: () = msg_send![&panel,setHidesOnDeactivate:false];
            let _: () = msg_send![&panel,setCollectionBehavior:((1usize<<3)|(1<<6))];
            let _: () =
                msg_send![&panel,setTitle:&*NSString::from_str("Distribuciones de esta pantalla")];
            let root: Obj = msg_send![&panel, contentView];
            choice(
                root,
                rect(0., 0., area.size.width, area.size.height),
                target,
                -1,
                "Cancelar selección",
                true,
            );
            let ghost = alloc_view(
                transparent_view_class(),
                rect(0., 0., area.size.width, area.size.height),
            );
            let _: () = msg_send![root,addSubview:&*ghost];
            let custom = crate::custom_templates::entries(defaults);
            let total = Layout::ALL.len() + custom.len();
            let Gallery {
                frame: gallery,
                viewport,
                document,
                cards,
                columns,
            } = gallery_geometry(area, total);
            plate(root, gallery, 0.92, false, 22.);
            let orientation = if columns == 2 {
                "vertical"
            } else {
                "horizontal"
            };
            light_label(
                root,
                rect(
                    gallery.origin.x + 24.,
                    gallery.origin.y + gallery.size.height - 40.,
                    gallery.size.width - 48.,
                    26.,
                ),
                &format!("Distribuciones · pantalla {orientation}"),
                18.,
                true,
            );
            light_label(
                root,
                rect(
                    gallery.origin.x + 24.,
                    gallery.origin.y + gallery.size.height - 64.,
                    gallery.size.width - 48.,
                    20.,
                ),
                &format!(
                    "{} {} · miniaturas de ejemplo · solo esta pantalla",
                    context.desktop.order.len(),
                    if context.desktop.order.len() == 1 {
                        "hueco"
                    } else {
                        "huecos"
                    }
                ),
                12.,
                false,
            );
            let scroll = alloc_view(class!(NSScrollView), viewport);
            let _: () = msg_send![&scroll, setDrawsBackground:false];
            let _: () = msg_send![&scroll, setHasVerticalScroller:true];
            let _: () = msg_send![&scroll, setAutohidesScrollers:true];
            let document = alloc_view(class!(NSView), document);
            let _: () = msg_send![&scroll, setDocumentView:&*document];
            let _: () = msg_send![root, addSubview:&*scroll];
            let root = Retained::as_ptr(&document) as Obj;
            let mut card_views = vec![];
            let mut buttons = vec![];
            let mut valid: Vec<bool> = Layout::ALL
                .map(|layout| context.proposal(layout).is_some())
                .to_vec();
            let mut titles: Vec<String> = Layout::ALL
                .iter()
                .map(|layout| layout.screen_name(area).to_string())
                .collect();
            for (index, card) in cards.iter().take(Layout::ALL.len()).enumerate() {
                let layout = Layout::ALL[index];
                card_views.push(plate(root, *card, 0.10, true, 12.));
                let thumb = thumbnail_area(*card, area);
                plate(root, thumb, 0.75, false, 4.);
                // Representative thumbnails remain recognizable with 0–2 windows.
                let mut example = Desktop::configured(layout, None);
                let examples = layout.example_count();
                let ids: Vec<u32> = (1..=examples).collect();
                example.reconcile_at(&ids, area, 10., None);
                for (_, frame) in example.frames(area, 10.) {
                    let scale = thumb.size.width / area.size.width;
                    plate(
                        root,
                        rect(
                            thumb.origin.x + (frame.origin.x - area.origin.x) * scale,
                            thumb.origin.y + thumb.size.height
                                - (frame.origin.y - area.origin.y + frame.size.height) * scale,
                            frame.size.width * scale,
                            frame.size.height * scale,
                        ),
                        if valid[index] { 0.48 } else { 0.16 },
                        true,
                        2.,
                    );
                }
                light_label(
                    root,
                    rect(
                        card.origin.x + 12.,
                        card.origin.y + 10.,
                        card.size.width - 24.,
                        20.,
                    ),
                    &format!("{}  {}", index + 1, layout.short_name(area)),
                    13.,
                    true,
                );
                buttons.push(choice(
                    root,
                    *card,
                    target,
                    index as isize,
                    &format!(
                        "{} · {}{}",
                        index + 1,
                        layout.screen_name(area),
                        if valid[index] { "" } else { " · no cabe" }
                    ),
                    valid[index],
                ));
            }
            // Guarded library: same slot count, then the same hard 80×60 minimum.
            for (offset, template) in custom.iter().enumerate() {
                let index = Layout::ALL.len() + offset;
                let Some(card) = cards.get(index) else {
                    continue;
                };
                let proposed = custom_proposal(&context, template);
                let fits = proposed.is_some();
                valid.push(fits);
                titles.push(template.name.clone());
                card_views.push(plate(root, *card, 0.10, true, 12.));
                let thumb = thumbnail_area(*card, area);
                plate(root, thumb, 0.75, false, 4.);
                if let Some(desktop) = proposed.or_else(|| template.preview()) {
                    let scale = thumb.size.width / area.size.width;
                    for (_, frame) in desktop.window_frames(area, 10.) {
                        plate(
                            root,
                            rect(
                                thumb.origin.x + (frame.origin.x - area.origin.x) * scale,
                                thumb.origin.y + thumb.size.height
                                    - (frame.origin.y - area.origin.y + frame.size.height) * scale,
                                frame.size.width * scale,
                                frame.size.height * scale,
                            ),
                            if fits { 0.48 } else { 0.16 },
                            true,
                            2.,
                        );
                    }
                }
                light_label(
                    root,
                    rect(
                        card.origin.x + 12.,
                        card.origin.y + 10.,
                        card.size.width - 24.,
                        20.,
                    ),
                    &format!("{}  {}", index + 1, template.name),
                    13.,
                    true,
                );
                buttons.push(choice(
                    root,
                    *card,
                    target,
                    index as isize,
                    &format!(
                        "{} · {}{}",
                        index + 1,
                        template.name,
                        if fits { "" } else { " · no cabe" }
                    ),
                    fits,
                ));
            }
            let root: Obj = msg_send![&panel, contentView];
            let status = light_label(
                root,
                rect(
                    gallery.origin.x + 24.,
                    gallery.origin.y + 17.,
                    gallery.size.width - 48.,
                    24.,
                ),
                "",
                11.,
                false,
            );
            let selected = context.desktop.layout.selection_index() as usize;
            let mut picker = Self {
                panel,
                context,
                selected,
                columns,
                valid,
                titles,
                custom,
                card_views,
                buttons,
                ghost: Retained::as_ptr(&ghost) as Obj,
                status,
            };
            picker.render();
            Some(picker)
        }
    }
    pub fn show(&self) {
        unsafe {
            let _: () = msg_send![&self.panel,makeKeyAndOrderFront:ptr::null::<AnyObject>()];
        }
    }
    pub fn select(&mut self, index: usize) {
        if index < self.valid.len() && self.valid[index] && index != self.selected {
            self.selected = index;
            self.render();
        }
    }
    pub fn move_selection(&mut self, offset: isize) {
        let total = self.valid.len() as isize;
        for step in 1..=total {
            let index = (self.selected as isize + offset * step).rem_euclid(total) as usize;
            if self.valid[index] {
                self.select(index);
                break;
            }
        }
    }
    /// The chosen card as either a built-in layout or a saved distribution.
    pub fn selection(&self) -> Option<Selection> {
        if !self.valid[self.selected] {
            return None;
        }
        if let Some(layout) = Layout::ALL.get(self.selected) {
            return Some(Selection::Layout(*layout));
        }
        self.custom
            .get(self.selected - Layout::ALL.len())
            .map(|template| Selection::Custom(template.distribution.clone()))
    }
    fn render(&mut self) {
        unsafe {
            for (index, view) in self.card_views.iter().enumerate() {
                let title = format!(
                    "{} · {}{}",
                    index + 1,
                    self.titles[index],
                    if !self.valid[index] {
                        " · no cabe"
                    } else if index == self.selected {
                        " · seleccionada"
                    } else {
                        ""
                    }
                );
                let _: () = msg_send![self.buttons[index],setAccessibilityLabel:&*NSString::from_str(&title)];
                let layer: Obj = msg_send![*view, layer];
                let c = color(
                    0.48,
                    0.60,
                    1.,
                    if index == self.selected { 0.32 } else { 0.10 },
                );
                let cg: *const crate::native::CgColor = msg_send![c, CGColor];
                let _: () = msg_send![layer,setBackgroundColor:cg];
                let _: () = msg_send![self.buttons[index],setState:if index==self.selected {1isize}else{0isize}];
            }
            let selected = self.card_views[self.selected];
            let bounds: NSRect = msg_send![selected, bounds];
            let _: bool = msg_send![selected, scrollRectToVisible:bounds];
            let empty: Retained<AnyObject> = msg_send![class!(NSArray), new];
            let _: () = msg_send![self.ghost,setSubviews:&*empty];
            let proposed = if !self.valid[self.selected] {
                None
            } else if let Some(layout) = Layout::ALL.get(self.selected) {
                self.context.proposal(*layout)
            } else {
                self.custom
                    .get(self.selected - Layout::ALL.len())
                    .and_then(|template| custom_proposal(&self.context, template))
            };
            if let Some(desktop) = proposed {
                let area = self.context.area;
                for (index, (_, frame)) in desktop.window_frames(area, 10.).iter().enumerate() {
                    let r = rect(
                        frame.origin.x - area.origin.x,
                        area.size.height - (frame.origin.y - area.origin.y) - frame.size.height,
                        frame.size.width,
                        frame.size.height,
                    );
                    plate(self.ghost, r, 0.20, true, 10.);
                    light_label(
                        self.ghost,
                        rect(r.origin.x + 16., r.origin.y + r.size.height - 38., 60., 24.),
                        &(index + 1).to_string(),
                        18.,
                        true,
                    );
                }
            }
            crate::native::text(
                self.status,
                if self.valid[self.selected] {
                    "Ratón o teclas · Flechas · Intro: aplicar · Esc: cancelar"
                } else {
                    "Esta distribución no cabe. Elige otra plantilla."
                },
            );
        }
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&self.panel,orderOut:ptr::null::<AnyObject>()];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gallery_and_thumbnails_fit_landscape_portrait_and_small_screens() {
        for area in [
            rect(0., 0., 1600., 900.),
            rect(-900., -400., 900., 1600.),
            rect(0., 0., 640., 480.),
        ] {
            let geometry = gallery_geometry(area, Layout::ALL.len());
            let gallery = geometry.frame;
            assert_eq!(
                geometry.columns,
                if area.size.height > area.size.width {
                    2
                } else {
                    4
                }
            );
            assert!(gallery.origin.x >= 0. && gallery.origin.y >= 0.);
            assert!(gallery.origin.x + gallery.size.width <= area.size.width);
            assert!(gallery.origin.y + gallery.size.height <= area.size.height);
            for card in geometry.cards {
                assert!(card.origin.x >= 0. && card.origin.y >= -0.001);
                let thumb = thumbnail_area(card, area);
                assert!(thumb.size.width > 0. && thumb.size.height > 0.);
                assert!(
                    (thumb.size.width / thumb.size.height - area.size.width / area.size.height)
                        .abs()
                        < 0.001
                );
                assert!(thumb.origin.x >= card.origin.x && thumb.origin.y >= card.origin.y);
            }
        }
    }
    #[test]
    fn keyboard_selection_adapts_to_gallery_orientation() {
        assert_eq!(key_input(125, 3, false), Some(Input::Move(3)));
        assert_eq!(key_input(125, 2, false), Some(Input::Move(2)));
        assert_eq!(key_input(48, 2, true), Some(Input::Move(-1)));
        assert_eq!(key_input(53, 2, false), Some(Input::Cancel));
        assert_eq!(key_input(36, 2, false), Some(Input::Confirm));
        for (index, key) in [18, 19, 20, 21, 23, 22, 26, 28].into_iter().enumerate() {
            assert_eq!(key_input(key, 3, false), Some(Input::Select(index)));
        }
        // The two first saved templates answer to 9 and 0.
        assert_eq!(key_input(25, 3, false), Some(Input::Select(8)));
        assert_eq!(key_input(29, 3, false), Some(Input::Select(9)));
    }
    #[test]
    fn full_template_library_scrolls_without_shrinking_or_losing_cards() {
        for area in [
            rect(0., 0., 1600., 900.),
            rect(-1080., -200., 1080., 1889.),
            rect(0., 0., 640., 480.),
        ] {
            let short = gallery_geometry(area, Layout::ALL.len());
            let full = gallery_geometry(area, Layout::ALL.len() + 64);
            assert_eq!(short.cards[0].size, full.cards[0].size);
            assert!(full.document.size.height > full.viewport.size.height);
            assert!(full.frame.origin.y >= 0.);
            assert!(full.frame.origin.y + full.frame.size.height <= area.size.height);
            for card in &full.cards {
                assert!(card.origin.x >= 0. && card.origin.y >= -0.001);
                assert!(card.origin.x + card.size.width <= full.document.size.width + 0.001);
                assert!(card.origin.y + card.size.height <= full.document.size.height + 0.001);
                let thumb = thumbnail_area(*card, area);
                assert!(thumb.size.width > 0. && thumb.size.height > 0.);
                assert!(thumb.origin.y >= card.origin.y);
                assert!(thumb.origin.y + thumb.size.height <= card.origin.y + card.size.height);
            }
            assert!(full.cards.last().unwrap().origin.y.abs() < 0.001);
        }
    }
    #[test]
    fn saved_preview_rejects_current_app_minimums_and_wrong_slot_counts() {
        let area = rect(0., 0., 1600., 900.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile(&[101, 102], area, 10.);
        let text = desktop.save_distribution(area).unwrap();
        let template =
            crate::custom_templates::Template::parse("id".into(), "Mi reparto".into(), text)
                .unwrap();
        assert!(fits_area(&template.preview().unwrap(), area));
        desktop.set_minimum_sizes(&[(101, objc2_foundation::NSSize::new(1600., 90.))].into());
        let mut context = PickerContext {
            workspace: (1, 7),
            area,
            desktop,
        };
        assert!(custom_proposal(&context, &template).is_none());
        context.desktop.set_minimum_sizes(&Default::default());
        assert!(custom_proposal(&context, &template).is_some());
        context.desktop.remove(102);
        assert!(custom_proposal(&context, &template).is_none());
    }
    #[test]
    fn saved_preview_keeps_current_grid_identities_and_tab_bar_reservation() {
        let area = rect(-900., 40., 1800., 1200.);
        let mut saved = Desktop::configured(Layout::Spiral, Some((4, 4)));
        saved.reconcile(&[1, 2], area, 10.);
        let template = crate::custom_templates::Template::parse(
            "id".into(),
            "Con rejilla".into(),
            saved.save_distribution(area).unwrap(),
        )
        .unwrap();
        let mut desktop = Desktop::configured(Layout::Grid, Some((7, 3)));
        desktop.reconcile(&[101, 102], area, 10.);
        desktop = desktop
            .proposed_drop(103, crate::tiling::DropIntent::Group(101), area, 10.)
            .unwrap();
        let context = PickerContext {
            workspace: (1, 7),
            area,
            desktop: desktop.clone(),
        };
        let proposed = custom_proposal(&context, &template).unwrap();
        assert_eq!(proposed.grid, Some((7, 3)));
        assert_eq!(proposed.order, desktop.order);
        assert_eq!(proposed.groups(), desktop.groups());
        let slot = proposed
            .frames(area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 103)
            .unwrap()
            .1;
        let window = proposed
            .window_frames(area, 10.)
            .into_iter()
            .find(|(id, _)| *id == 103)
            .unwrap()
            .1;
        let header = crate::tiling::TAB_BAR_HEIGHT + crate::tiling::TAB_BAR_GAP;
        assert_eq!(window.origin.y, slot.origin.y + header);
        assert_eq!(window.size.height, slot.size.height - header);
        assert!(context.desktop == desktop);
    }
}
