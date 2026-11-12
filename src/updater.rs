//! Native Sparkle controller. All calls and retained objects stay on the main thread.
//! The embedded framework verifies signed feeds and archives, installs and relaunches.
use crate::native::Obj;
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::NSString;
use std::ptr;

pub struct Updater {
    controller: Retained<AnyObject>,
}

impl Updater {
    pub fn new() -> Option<Self> {
        unsafe {
            let bundle: Obj = msg_send![class!(NSBundle), mainBundle];
            if bundle.is_null() {
                return None;
            }
            let frameworks: Obj = msg_send![bundle, privateFrameworksPath];
            if frameworks.is_null() {
                return None;
            }
            let path: Obj = msg_send![frameworks, stringByAppendingPathComponent:&*NSString::from_str("Sparkle.framework")];
            if path.is_null() {
                return None;
            }
            let framework: Obj = msg_send![class!(NSBundle), bundleWithPath:path];
            if framework.is_null() {
                return None;
            }
            let mut error: Obj = ptr::null_mut();
            let loaded: bool = msg_send![framework, loadAndReturnError:&mut error];
            if !loaded {
                return None;
            }
            let class = AnyClass::get(c"SPUStandardUpdaterController")?;
            let controller: Allocated<AnyObject> = msg_send![class, alloc];
            let controller: Retained<AnyObject> = msg_send![controller, initWithStartingUpdater:false, updaterDelegate:ptr::null::<AnyObject>(), userDriverDelegate:ptr::null::<AnyObject>()];
            let updater: Obj = msg_send![&controller, updater];
            let started: bool = msg_send![updater, startUpdater:&mut error];
            started.then_some(Self { controller })
        }
    }

    fn core(&self) -> Obj {
        unsafe { msg_send![&self.controller, updater] }
    }

    pub fn check(&self) {
        unsafe {
            let ready: bool = msg_send![self.core(), canCheckForUpdates];
            if ready {
                let _: () = msg_send![&self.controller, checkForUpdates:ptr::null::<AnyObject>()];
            }
        }
    }

    pub fn automatic_checks(&self) -> bool {
        unsafe { msg_send![self.core(), automaticallyChecksForUpdates] }
    }

    pub fn set_automatic_checks(&self, enabled: bool) {
        unsafe {
            let _: () = msg_send![self.core(), setAutomaticallyChecksForUpdates:enabled];
        }
    }
}
