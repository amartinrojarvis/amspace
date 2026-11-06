//! Objective-C objects stay on the main thread. Parents retain their subviews.
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
pub type Obj = *mut AnyObject;
pub fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
pub unsafe fn alloc_view(
    name: &'static objc2::runtime::AnyClass,
    frame: NSRect,
) -> Retained<AnyObject> {
    unsafe {
        let v: Allocated<AnyObject> = msg_send![name, alloc];
        msg_send![v, initWithFrame: frame]
    }
}
pub unsafe fn color(r: f64, g: f64, b: f64, a: f64) -> Obj {
    unsafe { msg_send![class!(NSColor), colorWithSRGBRed:r, green:g, blue:b, alpha:a] }
}
pub unsafe fn label(
    parent: Obj,
    frame: NSRect,
    text: &str,
    size: f64,
    weight: f64,
    muted: bool,
) -> Obj {
    unsafe {
        let v: Retained<AnyObject> =
            msg_send![class!(NSTextField), labelWithString:&*NSString::from_str(text)];
        let font: Obj = msg_send![class!(NSFont), systemFontOfSize:size, weight:weight];
        let c: Obj = if muted {
            msg_send![class!(NSColor), secondaryLabelColor]
        } else {
            msg_send![class!(NSColor), labelColor]
        };
        let _: () = msg_send![&v, setFrame:frame];
        let _: () = msg_send![&v, setFont:font];
        let _: () = msg_send![&v, setTextColor:c];
        let _: () = msg_send![parent, addSubview:&*v];
        Retained::as_ptr(&v) as Obj
    }
}
pub unsafe fn text(obj: Obj, value: &str) {
    unsafe {
        let _: () = msg_send![obj, setStringValue:&*NSString::from_str(value)];
    }
}
