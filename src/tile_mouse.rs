//! Observe native gesture boundaries without intercepting or synthesizing input.
use crate::native::Obj;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> NSPoint;
    fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
}

/// Preserve the recipient recorded when the native event was generated, even
/// when AppKit delivers its global-monitor copy after the window has moved.
#[derive(Clone, Copy)]
pub(crate) struct Grab {
    pub pid: i32,
    pub id: u32,
    pub frame: NSRect,
}
impl Grab {
    /// # Safety
    /// `event` must be a live CGEvent, or null when no native payload is available.
    pub unsafe fn from_event(event: *const std::ffi::c_void, point: NSPoint) -> Option<Self> {
        if !event.is_null() {
            // Public CGEvent fields: recipient under pointer (92), geometric
            // window under pointer (91), and target Unix process ID (40).
            let recipient = unsafe { CGEventGetIntegerValueField(event, 92) };
            let id = if recipient > 0 {
                recipient
            } else {
                unsafe { CGEventGetIntegerValueField(event, 91) }
            };
            if let Ok(id) = u32::try_from(id)
                && id != 0
            {
                let pid = i32::try_from(unsafe { CGEventGetIntegerValueField(event, 40) })
                    .ok()
                    .filter(|pid| *pid > 0)
                    .or_else(|| crate::window_server::owner_pid(id))?;
                // A vanished recipient must not become a different window
                // merely because that window now occupies the old point.
                let frame = crate::window_server::bounds(id)?;
                return Some(Self { pid, id, frame });
            }
        }
        // Older or synthetic events can omit recipient metadata.
        let (pid, id) = crate::window_server::window_at(point)?;
        let frame = crate::window_server::bounds(id)?;
        Some(Self { pid, id, frame })
    }
}

pub struct Monitor(Retained<AnyObject>);
impl Monitor {
    pub fn new(handler: impl Fn(bool, NSPoint, Option<Grab>) + 'static) -> Option<Self> {
        let block = block2::RcBlock::new(move |event: Obj| unsafe {
            let kind: usize = msg_send![event, type];
            let cg: *const std::ffi::c_void = msg_send![event, CGEvent];
            if !cg.is_null() && matches!(kind, 1 | 2) {
                let down = kind == 1;
                let point = CGEventGetLocation(cg);
                let grab = down.then(|| Grab::from_event(cg, point)).flatten();
                handler(down, point, grab);
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
