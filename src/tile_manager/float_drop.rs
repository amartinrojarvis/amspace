//! Deterministic top-center card for the float/tile drop gesture.
//!
//! One passive, nonactivating panel is reused across the whole gesture: the
//! parent calls [`Overlay::new`] once through `get_or_insert_with`, then
//! [`Overlay::update`] on every pointer move. Only `setFrame:` and the two
//! cached layer tints change; the view tree is built exactly once.
use super::*;

/// Largest card width, matching the tab badge scale.
const CARD_WIDTH: f64 = 320.;
/// Preferred card height: symbol, title and subtitle stacked.
const CARD_HEIGHT: f64 = 104.;
/// Gap between the area's top edge and the card, and the side inset.
const EDGE_INSET: f64 = 12.;
/// Smallest useful area that still leaves room for a readable card.
const MIN_USEFUL_WIDTH: f64 = 200.;
const MIN_USEFUL_HEIGHT: f64 = 92.;
// Interior stack, all in plate-local points.
const PLATE_INSET: f64 = 6.;
const TITLE_HEIGHT: f64 = 20.;
const SUBTITLE_HEIGHT: f64 = 15.;
const TITLE_GAP: f64 = 3.;
const ICON_GAP: f64 = 8.;
const ICON_SIZE: f64 = 40.;
const TEXT_STACK: f64 = TITLE_HEIGHT + TITLE_GAP + SUBTITLE_HEIGHT + ICON_GAP;

/// Top-centered card in global AX (top-left) points. `None` for a nonfinite
/// or too small area, which keeps the caller's previous painting intact.
pub(super) fn frame(area: NSRect) -> Option<NSRect> {
    let width = area.size.width;
    let height = area.size.height;
    if !width.is_finite()
        || !height.is_finite()
        || !area.origin.x.is_finite()
        || !area.origin.y.is_finite()
    {
        return None;
    }
    if width < MIN_USEFUL_WIDTH || height < MIN_USEFUL_HEIGHT {
        return None;
    }
    let side = EDGE_INSET.min(width / 2.).max(0.);
    let card_width = CARD_WIDTH.min((width - side * 2.).max(1.));
    let card_height = CARD_HEIGHT.min((height - side * 2.).max(1.));
    Some(rect(
        area.origin.x + (width - card_width) / 2.,
        area.origin.y + EDGE_INSET.min((height - card_height).max(0.)),
        card_width,
        card_height,
    ))
}

/// One reused card: panel plus the painted subviews, all retained by the
/// panel's own view hierarchy.
pub(super) struct Overlay {
    pub(super) panel: Retained<AnyObject>,
    plate: Obj,
    title: Obj,
    subtitle: Obj,
    icon: Obj,
    floating: bool,
    highlighted: bool,
    width: f64,
    height: f64,
}

fn title_for(floating: bool) -> &'static str {
    if floating {
        crate::i18n::tr("Volver al mosaico")
    } else {
        crate::i18n::tr("Hacer flotante")
    }
}

fn symbol_for(floating: bool) -> &'static str {
    if floating {
        "rectangle.stack"
    } else {
        "rectangle.3.group"
    }
}

impl Overlay {
    /// Build the panel and its single view tree for `frame` (global AX points).
    pub(super) fn new(frame: NSRect, floating: bool, highlighted: bool, primary: f64) -> Self {
        unsafe {
            let panel = new_overlay(frame, primary);
            let _: () = msg_send![&panel, setLevel:4isize];
            let root = alloc_view(
                class!(NSView),
                rect(0., 0., frame.size.width, frame.size.height),
            );
            let parent = Retained::as_ptr(&root) as Obj;
            let _: () = msg_send![parent, setWantsLayer:true];
            let _: () = msg_send![parent, setAccessibilityHidden:true];
            let plate = rounded_plate(
                parent,
                rect(
                    PLATE_INSET,
                    PLATE_INSET,
                    (frame.size.width - PLATE_INSET * 2.).max(1.),
                    (frame.size.height - PLATE_INSET * 2.).max(1.),
                ),
                [0.08, 0.09, 0.12],
                0.92,
                12.,
            );
            let card = card_geometry(frame.size.width, frame.size.height);
            let tint = color(0.97, 0.98, 1., 1.);
            let title =
                crate::native::label(plate, card.title, title_for(floating), 16., 0.4, false);
            let _: () = msg_send![title, setAlignment:1isize];
            let _: () = msg_send![title, setTextColor:tint];
            let subtitle = crate::native::label(
                plate,
                card.subtitle,
                crate::i18n::tr("Suelta aquí para confirmar"),
                12.,
                0.,
                true,
            );
            let _: () = msg_send![subtitle, setAlignment:1isize];
            let _: () = msg_send![subtitle, setTextColor:tint];
            let icon = image_view(
                plate,
                card.icon,
                symbol_for(floating),
                title_for(floating),
                tint,
            );
            paint(plate, highlighted);
            let _: () = msg_send![&panel, setContentView:&*root];
            Overlay {
                panel,
                plate,
                title,
                subtitle,
                icon,
                floating,
                highlighted,
                width: frame.size.width,
                height: frame.size.height,
            }
        }
    }

    /// Move, relabel and retint in place. Never rebuilds the view tree.
    pub(super) fn update(
        &mut self,
        frame: NSRect,
        floating: bool,
        highlighted: bool,
        primary: f64,
    ) {
        unsafe {
            let target = appkit(frame, primary);
            let current: NSRect = msg_send![&self.panel, frame];
            if current != target {
                let _: () = msg_send![&self.panel, setFrame:target, display:true];
            }
            if self.width != target.size.width || self.height != target.size.height {
                let root: Obj = msg_send![&self.panel, contentView];
                let bounds = rect(0., 0., target.size.width, target.size.height);
                let _: () = msg_send![root, setFrame:bounds];
                let _: () = msg_send![
                    self.plate,
                    setFrame:rect(
                        PLATE_INSET,
                        PLATE_INSET,
                        (target.size.width - PLATE_INSET * 2.).max(1.),
                        (target.size.height - PLATE_INSET * 2.).max(1.),
                    )
                ];
                let card = card_geometry(target.size.width, target.size.height);
                for (view, slot) in [(self.title, card.title), (self.subtitle, card.subtitle)] {
                    let _: () = msg_send![view, setFrame:slot];
                }
                let _: () = msg_send![self.icon, setFrame:card.icon];
                self.width = target.size.width;
                self.height = target.size.height;
            }
            if self.floating != floating {
                let title = title_for(floating);
                crate::native::text(self.title, title);
                crate::native::text(self.subtitle, crate::i18n::tr("Suelta aquí para confirmar"));
                let image: Obj = msg_send![
                    class!(NSImage),
                    imageWithSystemSymbolName:&*NSString::from_str(symbol_for(floating)),
                    accessibilityDescription:&*NSString::from_str(title)
                ];
                if !image.is_null() {
                    let _: () = msg_send![self.icon, setImage:image];
                }
                self.floating = floating;
            }
            if self.highlighted != highlighted {
                paint(self.plate, highlighted);
                self.highlighted = highlighted;
            }
            let visible: bool = msg_send![&self.panel, isVisible];
            if !visible {
                let _: () = msg_send![&self.panel, orderFrontRegardless];
            }
        }
    }

    /// Detach from the screen, keeping the panel for a later `update`.
    pub(super) fn hide(&self) {
        unsafe {
            let visible: bool = msg_send![&self.panel, isVisible];
            if visible {
                let _: () = msg_send![&self.panel, orderOut:ptr::null::<AnyObject>()];
            }
        }
    }
}

/// Centered icon/title/subtitle stack inside the plate, in plate-local points.
struct Card {
    icon: NSRect,
    title: NSRect,
    subtitle: NSRect,
}

fn card_geometry(width: f64, height: f64) -> Card {
    let plate_width = (width - PLATE_INSET * 2.).max(1.);
    let plate_height = (height - PLATE_INSET * 2.).max(1.);
    let icon = (plate_height - TEXT_STACK).clamp(0., ICON_SIZE);
    let stack = icon + TEXT_STACK;
    let bottom = ((plate_height - stack) / 2.).max(0.);
    let text_width = (plate_width - 8.).max(1.);
    Card {
        icon: rect((plate_width - icon) / 2., bottom + TEXT_STACK, icon, icon),
        title: rect(
            4.,
            bottom + SUBTITLE_HEIGHT + TITLE_GAP,
            text_width,
            TITLE_HEIGHT,
        ),
        subtitle: rect(4., bottom, text_width, SUBTITLE_HEIGHT),
    }
}

/// Same two states as `paint_hint`: idle dark plate, active blue plate.
unsafe fn paint(plate: Obj, highlighted: bool) {
    unsafe {
        if plate.is_null() {
            return;
        }
        let layer: Obj = msg_send![plate, layer];
        let fill = if highlighted {
            color(0.12, 0.28, 0.52, 0.96)
        } else {
            color(0.08, 0.09, 0.12, 0.92)
        };
        let outline = if highlighted {
            color(0.50, 0.77, 1., 1.)
        } else {
            color(1., 1., 1., 0.45)
        };
        let fill: *const crate::native::CgColor = msg_send![fill, CGColor];
        let outline: *const crate::native::CgColor = msg_send![outline, CGColor];
        let _: () = msg_send![layer, setBackgroundColor:fill];
        let _: () = msg_send![layer, setBorderColor:outline];
        let _: () = msg_send![layer, setBorderWidth:if highlighted { 3f64 } else { 1f64 }];
    }
}

unsafe fn image_view(parent: Obj, slot: NSRect, symbol: &str, description: &str, tint: Obj) -> Obj {
    unsafe {
        let image: Obj = msg_send![
            class!(NSImage),
            imageWithSystemSymbolName:&*NSString::from_str(symbol),
            accessibilityDescription:&*NSString::from_str(description)
        ];
        if image.is_null() {
            return ptr::null_mut();
        }
        let view = alloc_view(class!(NSImageView), slot);
        let _: () = msg_send![&view, setImage:image];
        let _: () = msg_send![&view, setImageScaling:3isize];
        let _: () = msg_send![&view, setContentTintColor:tint];
        let _: () = msg_send![&view, setAccessibilityHidden:true];
        let _: () = msg_send![parent, addSubview:&*view];
        Retained::as_ptr(&view) as Obj
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(area: NSRect) -> NSRect {
        frame(area).expect("area must be usable")
    }

    fn right(r: NSRect) -> f64 {
        r.origin.x + r.size.width
    }

    fn top(r: NSRect) -> f64 {
        r.origin.y + r.size.height
    }

    #[test]
    fn landscape_area_centers_the_card_below_the_top_edge() {
        assert_eq!(
            card(rect(0., 0., 1920., 1080.)),
            rect(800., 12., 320., 104.)
        );
    }

    #[test]
    fn portrait_area_keeps_the_same_height_and_recenters_horizontally() {
        assert_eq!(
            card(rect(0., 0., 1080., 1920.)),
            rect(380., 12., 320., 104.)
        );
    }

    #[test]
    fn negative_origin_area_offsets_the_card_without_normalizing() {
        assert_eq!(
            card(rect(-1420., -500., 1400., 2400.)),
            rect(-880., -488., 320., 104.)
        );
    }

    #[test]
    fn narrow_area_shrinks_the_card_and_clamps_its_height() {
        assert_eq!(card(rect(10., 41., 260., 120.)), rect(22., 53., 236., 96.));
    }

    #[test]
    fn minimal_useful_area_still_fits_inside_the_area() {
        let area = rect(0., 0., MIN_USEFUL_WIDTH, MIN_USEFUL_HEIGHT);
        let placed = card(area);
        assert_eq!(placed, rect(12., 12., 176., 68.));
        assert!(placed.origin.x >= area.origin.x);
        assert!(placed.origin.y >= area.origin.y);
        assert!(right(placed) <= right(area) + f64::EPSILON);
        assert!(top(placed) <= top(area) + f64::EPSILON);
    }

    #[test]
    fn tiny_nonfinite_and_empty_areas_are_rejected() {
        for area in [
            rect(0., 0., 199., 900.),
            rect(0., 0., 1000., 91.),
            rect(0., 0., 0., 0.),
            rect(0., 0., f64::NAN, 900.),
            rect(0., 0., 1000., f64::NAN),
            rect(0., 0., f64::INFINITY, 900.),
            rect(0., 0., 1000., f64::NEG_INFINITY),
            rect(f64::NAN, 0., 1000., 900.),
            rect(0., f64::NAN, 1000., 900.),
            rect(f64::INFINITY, 0., 1000., 900.),
            rect(0., f64::NEG_INFINITY, 1000., 900.),
        ] {
            assert_eq!(frame(area), None, "{area:?} must not produce a card");
        }
    }

    #[test]
    fn interior_stack_is_ordered_and_never_overflows_the_plate() {
        for (width, height) in [(320., 104.), (176., 68.), (320., 92.), (240., 300.)] {
            let card = card_geometry(width, height);
            let plate = height - PLATE_INSET * 2.;
            assert!(card.subtitle.origin.y >= 0.);
            assert!(card.title.origin.y >= top(card.subtitle) - 1e-9);
            assert!(card.icon.origin.y >= top(card.title) - 1e-9);
            assert!(top(card.icon) <= plate + 1e-9);
            assert!(card.icon.size.width <= ICON_SIZE + 1e-9);
        }
    }
}
