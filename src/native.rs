//! Objective-C objects stay on the main thread. Parents retain their subviews.
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
pub type Obj = *mut AnyObject;
pub unsafe fn add_common_timer(run_loop: Obj, timer: &AnyObject) {
    unsafe {
        // Use the exported constant. Its actual value is kCFRunLoopCommonModes;
        // a literal "NSRunLoopCommonModes" creates an unused custom mode.
        let _: () =
            msg_send![run_loop, addTimer:timer, forMode:objc2_foundation::NSRunLoopCommonModes];
    }
}
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

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::{
        rc::autoreleasepool,
        runtime::{ClassBuilder, Sel},
        sel,
    };
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };
    static FIRED: AtomicUsize = AtomicUsize::new(0);
    extern "C-unwind" fn tick(_: Obj, _: Sel, _: Obj) {
        FIRED.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn common_timer_fires_in_default_mode_and_stops_after_invalidation() {
        autoreleasepool(|_| unsafe {
            let mut builder =
                ClassBuilder::new(c"AMSpaceTimerRegression", class!(NSObject)).unwrap();
            builder.add_method(sel!(tick:), tick as extern "C-unwind" fn(Obj, Sel, Obj));
            let target: Retained<AnyObject> = msg_send![builder.register(), new];
            let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.01f64, target:&*target, selector:sel!(tick:), userInfo:std::ptr::null::<AnyObject>(), repeats:true];
            let run_loop: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
            add_common_timer(run_loop, &timer);
            let start = Instant::now();
            while FIRED.load(Ordering::Relaxed) == 0 && start.elapsed() < Duration::from_millis(300)
            {
                let until: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:0.02f64];
                let _: () = msg_send![run_loop, runUntilDate:until];
            }
            let _: () = msg_send![&timer, invalidate];
            let count = FIRED.load(Ordering::Relaxed);
            assert!(
                count > 0,
                "cursor timer never ran in the normal application run loop"
            );
            let until: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:0.04f64];
            let _: () = msg_send![run_loop, runUntilDate:until];
            assert_eq!(
                FIRED.load(Ordering::Relaxed),
                count,
                "disabled timers must stop waking up"
            );
        });
    }
}
