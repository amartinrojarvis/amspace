//! Metadata-only desktop thumbnails. Never captures pixels, switches Spaces or activates apps.
use crate::{native::*, spaces, window_server};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

#[derive(Default)]
pub struct Preview {
    pub pending: Option<(u64, NSRect)>,
    panel: Option<Retained<AnyObject>>,
}
impl Preview {
    pub fn hide(&mut self) {
        self.pending = None;
        if let Some(panel) = &self.panel {
            unsafe {
                let _: () = msg_send![panel, orderOut:std::ptr::null::<AnyObject>()];
            }
        }
    }
    pub fn show(&mut self) {
        let Some((id, anchor)) = self.pending else {
            return;
        };
        let Some(destination) = spaces::destination(id) else {
            self.hide();
            return;
        };
        let Some(screen) = screen_configuration()
            .into_iter()
            .find(|s| s.id == destination.display)
        else {
            self.hide();
            return;
        };
        unsafe {
            let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            if buttons != 0 || !contains(anchor, mouse) {
                self.hide();
                return;
            }
            let thumb = thumbnail(id, screen.frame);
            let image_size: NSSize = msg_send![&thumb, size];
            let width = image_size.width + 12.;
            let height = image_size.height + 12.;
            let frame = popup_frame(anchor, screen.frame, width, height);
            let panel = self.panel.get_or_insert_with(|| {
                let panel: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
                let panel: Retained<AnyObject> = msg_send![panel, initWithContentRect:frame, styleMask:128usize, backing:2usize, defer:false];
                let _: () = msg_send![&panel, setReleasedWhenClosed:false];
                let _: () = msg_send![&panel, setOpaque:false];
                let clear: Obj = msg_send![class!(NSColor), clearColor];
                let _: () = msg_send![&panel, setBackgroundColor:clear];
                let _: () = msg_send![&panel, setLevel:25isize];
                let _: () = msg_send![&panel, setCollectionBehavior:((1usize<<0)|(1usize<<3)|(1usize<<6))];
                let _: () = msg_send![&panel, setIgnoresMouseEvents:true];
                let _: () = msg_send![&panel, setHasShadow:true];
                let _: () = msg_send![&panel, setHidesOnDeactivate:false];
                panel
            });
            let _: () = msg_send![&*panel, setFrame:frame, display:false];
            let root = alloc_view(class!(NSVisualEffectView), rect(0., 0., width, height));
            let _: () = msg_send![&root, setMaterial:13isize];
            let _: () = msg_send![&root, setBlendingMode:0isize];
            let _: () = msg_send![&root, setState:1isize];
            let _: () = msg_send![&root, setWantsLayer:true];
            let layer: Obj = msg_send![&root, layer];
            let _: () = msg_send![layer, setCornerRadius:10f64];
            let _: () = msg_send![layer, setMasksToBounds:true];
            let image = alloc_view(
                class!(NSImageView),
                rect(6., 6., image_size.width, image_size.height),
            );
            let _: () = msg_send![&image, setImage:&*thumb];
            let _: () = msg_send![&root, addSubview:&*image];
            let _: () = msg_send![&*panel, setContentView:&*root];
            let _: () = msg_send![&*panel, setAlphaValue:0f64];
            let _: () = msg_send![&*panel, orderFrontRegardless];
            let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
            let context: Obj = msg_send![class!(NSAnimationContext), currentContext];
            let _: () = msg_send![context, setDuration:0.16f64];
            let animator: Obj = msg_send![&*panel, animator];
            let _: () = msg_send![animator, setAlphaValue:1f64];
            let _: () = msg_send![class!(NSAnimationContext), endGrouping];
        }
    }
}
fn contains(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x
        && p.x <= r.origin.x + r.size.width
        && p.y >= r.origin.y
        && p.y <= r.origin.y + r.size.height
}
fn popup_frame(anchor: NSRect, screen: NSRect, width: f64, height: f64) -> NSRect {
    rect(
        (anchor.origin.x + anchor.size.width / 2. - width / 2.).clamp(
            screen.origin.x + 8.,
            screen.origin.x + screen.size.width - width - 8.,
        ),
        (anchor.origin.y - height - 8.).max(screen.origin.y + 8.),
        width,
        height,
    )
}
fn union(screens: &[ScreenGeometry]) -> Option<NSRect> {
    let first = screens.first()?.frame;
    Some(screens.iter().skip(1).fold(first, |a, s| {
        let left = a.origin.x.min(s.frame.origin.x);
        let bottom = a.origin.y.min(s.frame.origin.y);
        rect(
            left,
            bottom,
            (a.origin.x + a.size.width).max(s.frame.origin.x + s.frame.size.width) - left,
            (a.origin.y + a.size.height).max(s.frame.origin.y + s.frame.size.height) - bottom,
        )
    }))
}
fn thumbnail_size(canvas: NSRect) -> NSSize {
    let scale = (280. / canvas.size.width).min(180. / canvas.size.height);
    NSSize::new(canvas.size.width * scale, canvas.size.height * scale)
}
fn project(frame: NSRect, canvas: NSRect) -> NSRect {
    let size = thumbnail_size(canvas);
    let scale = (size.width / canvas.size.width).min(size.height / canvas.size.height);
    rect(
        (size.width - canvas.size.width * scale) / 2. + (frame.origin.x - canvas.origin.x) * scale,
        (size.height - canvas.size.height * scale) / 2.
            + (frame.origin.y - canvas.origin.y) * scale,
        frame.size.width * scale,
        frame.size.height * scale,
    )
}
fn miniature(frame: NSRect, screen: NSRect, primary_height: f64, canvas: NSRect) -> Option<NSRect> {
    let frame = rect(
        frame.origin.x,
        primary_height - frame.origin.y - frame.size.height,
        frame.size.width,
        frame.size.height,
    );
    let left = frame.origin.x.max(screen.origin.x);
    let bottom = frame.origin.y.max(screen.origin.y);
    let right = (frame.origin.x + frame.size.width).min(screen.origin.x + screen.size.width);
    let top = (frame.origin.y + frame.size.height).min(screen.origin.y + screen.size.height);
    if right <= left || top <= bottom {
        return None;
    }
    Some(project(
        rect(left, bottom, right - left, top - bottom),
        canvas,
    ))
}
fn thumbnail(space: u64, screen: NSRect) -> Retained<AnyObject> {
    unsafe {
        let screens = screen_configuration();
        let canvas = union(&screens).unwrap_or(screen);
        let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
        let image: Retained<AnyObject> = msg_send![image, initWithSize:thumbnail_size(canvas)];
        let _: () = msg_send![&image, lockFocus];
        let visible = spaces::visible_desktops();
        let selected_display = spaces::destination(space).map(|d| d.display);
        // With shared Spaces, choosing a desktop affects every physical monitor.
        let shared = !visible.is_empty()
            && visible.len() == screens.len()
            && visible
                .iter()
                .all(|(_, id)| visible.first().is_none_or(|(_, first)| id == first));
        let native_screens: Obj = msg_send![class!(NSScreen), screens];
        let first: Obj = msg_send![native_screens, firstObject];
        let primary: NSRect = msg_send![first, frame];
        let windows = window_server::transition_windows().unwrap_or_default();
        // Query each real window once, and project its current compositor geometry.
        let windows: Vec<_> = windows
            .into_iter()
            .filter(|w| w.layer == 0 && w.pid != std::process::id() as i32)
            .filter_map(|w| {
                if window_server::ordered_in(w.id) == Some(false) {
                    return None;
                }
                Some((w.pid, spaces::window_spaces(w.id)?, w.frame?))
            })
            .collect();
        let blue = color(0.48, 0.60, 1., 0.20);
        let stroke = color(0.48, 0.60, 1., 0.65);
        for display in &screens {
            let viewport = project(display.frame, canvas);
            // A faint surface marks each monitor without an exterior frame.
            // Inset adjoining displays slightly to retain their separation.
            let base = color(0.48, 0.60, 1., 0.035);
            let _: () = msg_send![base, setFill];
            let surface = rect(
                viewport.origin.x + 0.5,
                viewport.origin.y + 0.5,
                (viewport.size.width - 1.).max(0.),
                (viewport.size.height - 1.).max(0.),
            );
            let outline: Obj = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect:surface, xRadius:3f64, yRadius:3f64];
            let _: () = msg_send![outline, fill];
            let display_space = if shared || Some(display.id) == selected_display {
                Some(space)
            } else {
                visible
                    .iter()
                    .find(|(id, _)| *id == display.id)
                    .map(|(_, id)| *id)
            };
            for (pid, _, geometry) in windows
                .iter()
                .rev()
                .filter(|(_, ids, _)| display_space.is_some_and(|space| ids.contains(&space)))
            {
                let Some(mut frame) =
                    miniature(*geometry, display.frame, primary.size.height, canvas)
                else {
                    continue;
                };
                // Preserve a fine separation even when windows share an exact edge.
                frame.origin.x += 0.7;
                frame.origin.y += 0.7;
                frame.size.width = (frame.size.width - 1.4).max(0.);
                frame.size.height = (frame.size.height - 1.4).max(0.);
                if frame.size.width < 1. || frame.size.height < 1. {
                    continue;
                }
                let _: () = msg_send![blue, setFill];
                let path: Obj = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect:frame, xRadius:2.5f64, yRadius:2.5f64];
                let _: () = msg_send![path, fill];
                let _: () = msg_send![stroke, setStroke];
                let _: () = msg_send![path, setLineWidth:0.65f64];
                let _: () = msg_send![path, stroke];
                let app: Obj = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier:*pid];
                let icon: Obj = msg_send![app, icon];
                let size = 20f64.min(frame.size.width - 4.).min(frame.size.height - 4.);
                if !icon.is_null() && size >= 8. {
                    let _: () = msg_send![icon, drawInRect:rect(frame.origin.x+(frame.size.width-size)/2.,frame.origin.y+(frame.size.height-size)/2.,size,size), fromRect:NSRect::ZERO, operation:2usize, fraction:1f64];
                }
            }
        }
        let _: () = msg_send![&image, unlockFocus];
        image
    }
}
/// Render metadata-only thumbnails on AppKit's main thread for visual inspection.
pub fn render_diagnostics() {
    unsafe {
        let _: Obj = msg_send![class!(NSApplication), sharedApplication];
        for w in window_server::transition_windows()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w.layer == 0 && w.pid != std::process::id() as i32)
        {
            println!(
                "Window {} pid={} desktop={:?} ordered={:?} onscreen={} frame={:?}",
                w.id,
                w.pid,
                spaces::window_space(w.id),
                window_server::ordered_in(w.id),
                w.onscreen,
                w.frame
            );
        }
        for id in spaces::snapshot().desktops {
            let Some(destination) = spaces::destination(id) else {
                continue;
            };
            let Some(screen) = screen_configuration()
                .into_iter()
                .find(|s| s.id == destination.display)
            else {
                continue;
            };
            let image = thumbnail(id, screen.frame);
            let tiff: Obj = msg_send![&image, TIFFRepresentation];
            let bitmap: Allocated<AnyObject> = msg_send![class!(NSBitmapImageRep), alloc];
            let bitmap: Retained<AnyObject> = msg_send![bitmap, initWithData:tiff];
            let properties: Obj = msg_send![class!(NSDictionary), dictionary];
            let png: Obj =
                msg_send![&bitmap, representationUsingType:4usize, properties:properties];
            let path = format!(".cache/desktop-preview-{id}.png");
            let written: bool =
                msg_send![png, writeToFile:&*NSString::from_str(&path), atomically:true];
            println!("Desktop {id}: rendered={written}, {path}");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_on_screen_near_both_edges_and_negative_monitors() {
        for screen in [rect(0., 0., 1440., 900.), rect(-1440., -200., 1440., 900.)] {
            for x in [screen.origin.x, screen.origin.x + screen.size.width - 24.] {
                let p = popup_frame(
                    rect(x, screen.origin.y + screen.size.height - 24., 24., 24.),
                    screen,
                    264.,
                    190.,
                );
                assert!(
                    p.origin.x >= screen.origin.x
                        && p.origin.x + p.size.width <= screen.origin.x + screen.size.width
                );
                assert!(
                    p.origin.y >= screen.origin.y
                        && p.origin.y + p.size.height <= screen.origin.y + screen.size.height
                );
            }
        }
    }
    #[test]
    fn thumbnail_uses_its_whole_surface_at_each_monitor_aspect_ratio() {
        for canvas in [
            rect(0., 0., 1680., 1050.),
            rect(-4000., 0., 6000., 1000.),
            rect(0., -1200., 900., 2400.),
        ] {
            let size = thumbnail_size(canvas);
            assert!(size.width <= 280. && size.height <= 180.);
            assert!(
                (size.width / size.height - canvas.size.width / canvas.size.height).abs() < 1e-9
            );
            assert_eq!(
                project(canvas, canvas),
                rect(0., 0., size.width, size.height)
            );
        }
    }
    #[test]
    fn monitor_projection_preserves_offsets_and_shared_boundaries() {
        let screens = [
            ScreenGeometry {
                id: 1,
                frame: rect(-1000., 0., 1000., 800.),
                visible: NSRect::ZERO,
                scale: 2.,
            },
            ScreenGeometry {
                id: 2,
                frame: rect(0., 100., 1600., 900.),
                visible: NSRect::ZERO,
                scale: 1.,
            },
        ];
        let canvas = union(&screens).unwrap();
        assert_eq!(canvas, rect(-1000., 0., 2600., 1000.));
        let left = project(screens[0].frame, canvas);
        let right = project(screens[1].frame, canvas);
        assert!((left.origin.x + left.size.width - right.origin.x).abs() < 1e-9);
        assert!(right.origin.y > left.origin.y);
    }
    #[test]
    fn thumbnail_clips_and_converts_global_coordinates_on_offset_screens() {
        assert_eq!(
            miniature(
                rect(-1100., 150., 800., 600.),
                rect(-1200., 0., 1200., 800.),
                900.,
                rect(-1200., 0., 1200., 800.)
            ),
            Some(rect(22.5, 33.75, 180., 135.))
        );
        assert_eq!(
            miniature(
                rect(500., 0., 200., 200.),
                rect(-1200., 0., 1200., 800.),
                900.,
                rect(-1200., 0., 1200., 800.)
            ),
            None
        );
    }
}
