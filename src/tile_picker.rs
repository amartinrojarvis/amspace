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
        123 => Input::Move(-1),
        124 => Input::Move(1),
        125 => Input::Move(columns as isize),
        126 => Input::Move(-(columns as isize)),
        48 => Input::Move(if shift { -1 } else { 1 }),
        36 | 76 => Input::Confirm,
        53 => Input::Cancel,
        _ => return None,
    })
}
fn gallery_geometry(area: NSRect) -> (NSRect, Vec<NSRect>, usize) {
    let portrait = area.size.height > area.size.width;
    let columns = if portrait { 2 } else { 4 };
    let rows = Layout::ALL.len().div_ceil(columns);
    let (cw, ch) = if portrait { (180., 244.) } else { (210., 158.) };
    let width = columns as f64 * (cw + 12.) + 36.;
    let height = rows as f64 * (ch + 12.) + 120.;
    let scale = ((area.size.width - 32.) / width)
        .min((area.size.height - 32.) / height)
        .clamp(0.1, 1.);
    let (width, height, cw, ch) = (width * scale, height * scale, cw * scale, ch * scale);
    let panel = rect(
        (area.size.width - width) / 2.,
        (area.size.height - height) / 2.,
        width,
        height,
    );
    let cards = (0..Layout::ALL.len())
        .map(|index| {
            rect(
                panel.origin.x + 24. * scale + (index % columns) as f64 * (cw + 12. * scale),
                panel.origin.y + height
                    - 82. * scale
                    - ch
                    - (index / columns) as f64 * (ch + 12. * scale),
                cw,
                ch,
            )
        })
        .collect();
    (panel, cards, columns)
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
        let cg: *const std::ffi::c_void = msg_send![c, CGColor];
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
    valid: [bool; Layout::ALL.len()],
    card_views: Vec<Obj>,
    buttons: Vec<Obj>,
    ghost: Obj,
    status: Obj,
}
impl Picker {
    pub fn new(context: PickerContext, target: Obj) -> Option<Self> {
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
            let (gallery, cards, columns) = gallery_geometry(area);
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
                        "ventana"
                    } else {
                        "ventanas"
                    }
                ),
                12.,
                false,
            );
            let mut card_views = vec![];
            let mut buttons = vec![];
            let valid = Layout::ALL.map(|layout| context.proposal(layout).is_some());
            for (index, card) in cards.iter().enumerate() {
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
        for step in 1..=Layout::ALL.len() as isize {
            let index = (self.selected as isize + offset * step)
                .rem_euclid(Layout::ALL.len() as isize) as usize;
            if self.valid[index] {
                self.select(index);
                break;
            }
        }
    }
    pub fn selected_layout(&self) -> Option<Layout> {
        self.valid[self.selected].then_some(Layout::ALL[self.selected])
    }
    fn render(&mut self) {
        unsafe {
            for (index, view) in self.card_views.iter().enumerate() {
                let title = format!(
                    "{} · {}{}",
                    index + 1,
                    Layout::ALL[index].screen_name(self.context.area),
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
                let cg: *const std::ffi::c_void = msg_send![c, CGColor];
                let _: () = msg_send![layer,setBackgroundColor:cg];
                let _: () = msg_send![self.buttons[index],setState:if index==self.selected {1isize}else{0isize}];
            }
            let empty: Retained<AnyObject> = msg_send![class!(NSArray), new];
            let _: () = msg_send![self.ghost,setSubviews:&*empty];
            if let Some(desktop) = self.context.proposal(Layout::ALL[self.selected]) {
                let area = self.context.area;
                for (index, (_, frame)) in desktop.frames(area, 10.).iter().enumerate() {
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
                    "Ratón o 1–8 · Flechas · Intro: aplicar · Esc: cancelar"
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
            let (gallery, cards, cols) = gallery_geometry(area);
            assert_eq!(
                cols,
                if area.size.height > area.size.width {
                    2
                } else {
                    4
                }
            );
            assert!(gallery.origin.x >= 0. && gallery.origin.y >= 0.);
            assert!(gallery.origin.x + gallery.size.width <= area.size.width);
            assert!(gallery.origin.y + gallery.size.height <= area.size.height);
            for card in cards {
                assert!(card.origin.x >= gallery.origin.x && card.origin.y >= gallery.origin.y);
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
    }
}
