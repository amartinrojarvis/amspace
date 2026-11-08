//! Observe native gesture boundaries without intercepting or synthesizing input.
use crate::native::Obj;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSPoint;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> NSPoint;
}

pub struct Monitor(Retained<AnyObject>);
impl Monitor {
    pub fn new(handler: impl Fn(bool, NSPoint) + 'static) -> Option<Self> {
        let block = block2::RcBlock::new(move |event: Obj| unsafe {
            let kind: usize = msg_send![event, type];
            let cg: *const std::ffi::c_void = msg_send![event, CGEvent];
            if !cg.is_null() && matches!(kind, 1 | 2) {
                handler(kind == 1, CGEventGetLocation(cg));
            }
        });
        unsafe {
            // Left-down/up only. Motion remains coalesced by the existing timer;
            // there is no new idle polling and no keyboard event monitoring.
            let token: Obj = msg_send![class!(NSEvent),
                addGlobalMonitorForEventsMatchingMask:((1usize << 1) | (1 << 2)),
                handler:&*block];
            Retained::retain(token).map(Self)
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![class!(NSEvent), removeMonitor:&*self.0];
        }
    }
}
