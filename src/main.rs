#[cfg(not(target_os = "macos"))]
compile_error!("AMSpace requires macOS 13 or later.");

mod accessibility;
mod focus;
mod instance;
mod model;
mod native;
mod space_style;
mod spaces;
mod ui;
mod updater;
mod window_server;

fn main() {
    if std::env::args().any(|a| a == "--diagnose") {
        objc2::rc::autoreleasepool(|_| {
            println!("AMSpace {}", env!("CARGO_PKG_VERSION"));
            println!("Accessibility: {}", accessibility::trusted());
            println!("Spaces: {:?}", spaces::snapshot());
            unsafe {
                use objc2::{class, msg_send, runtime::AnyObject};
                let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
                let pid: i32 = msg_send![app, processIdentifier];
                println!("Front window: {:?}", window_server::front_window(pid));
            }
        });
    } else {
        let _instance = match instance::acquire() {
            Ok(Some(lock)) => lock,
            Ok(None) => return,
            Err(error) => {
                eprintln!("AMSpace cannot acquire its instance lock: {error}");
                return;
            }
        };
        ui::run();
    }
}
