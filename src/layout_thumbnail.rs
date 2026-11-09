//! Small, cached template images drawn from the actual layout model, without assets or timers.
use crate::{
    native::rect,
    tiling::{Desktop, Layout},
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::NSSize;

pub fn image(layout: Layout, portrait: bool) -> Retained<AnyObject> {
    let screen = if portrait {
        rect(0., 0., 1000., 1600.)
    } else {
        rect(0., 0., 1600., 1000.)
    };
    let count = layout.example_count();
    let mut example = Desktop::configured(layout, None);
    example.reconcile_at(&(1..=count).collect::<Vec<_>>(), screen, 24., None);
    let scale = (96. / screen.size.width).min(42. / screen.size.height);
    let x = (110. - screen.size.width * scale) / 2.;
    let y = (54. - screen.size.height * scale) / 2.;
    unsafe {
        let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
        let image: Retained<AnyObject> = msg_send![image, initWithSize:NSSize::new(110.,54.)];
        let _: () = msg_send![&image, lockFocus];
        for (id, frame) in example.frames(screen, 24.) {
            let c: *mut AnyObject = msg_send![class!(NSColor), colorWithWhite:0f64,alpha:if id == 1 {1f64} else {0.48f64}];
            let _: () = msg_send![c, setFill];
            let tile = rect(
                x + frame.origin.x * scale,
                y + (screen.size.height - frame.origin.y - frame.size.height) * scale,
                frame.size.width * scale,
                frame.size.height * scale,
            );
            let path: *mut AnyObject = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect:tile,xRadius:2f64,yRadius:2f64];
            let _: () = msg_send![path, fill];
        }
        let _: () = msg_send![&image, unlockFocus];
        let _: () = msg_send![&image, setTemplate:true];
        image
    }
}

pub fn custom_image(desktop: &Desktop, size: NSSize) -> Retained<AnyObject> {
    let screen = rect(0., 0., size.width, size.height);
    let scale = (208. / size.width).min(104. / size.height);
    let x = (220. - size.width * scale) / 2.;
    let y = (116. - size.height * scale) / 2.;
    unsafe {
        let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
        let image: Retained<AnyObject> = msg_send![image,initWithSize:NSSize::new(220.,116.)];
        let _: () = msg_send![&image, lockFocus];
        let blue: *mut AnyObject = msg_send![class!(NSColor), systemBlueColor];
        for (_, frame) in desktop.frames(screen, 10.) {
            let fill: *mut AnyObject = msg_send![blue,colorWithAlphaComponent:0.24f64];
            let _: () = msg_send![fill, setFill];
            let _: () = msg_send![blue, setStroke];
            let tile = rect(
                x + frame.origin.x * scale,
                y + (size.height - frame.origin.y - frame.size.height) * scale,
                frame.size.width * scale,
                frame.size.height * scale,
            );
            let path: *mut AnyObject = msg_send![class!(NSBezierPath),bezierPathWithRoundedRect:tile,xRadius:3f64,yRadius:3f64];
            let _: () = msg_send![path, fill];
            let _: () = msg_send![path,setLineWidth:0.7f64];
            let _: () = msg_send![path, stroke];
        }
        let _: () = msg_send![&image, unlockFocus];
        image
    }
}
