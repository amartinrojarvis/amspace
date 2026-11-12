#[cfg(not(target_os = "macos"))]
compile_error!("AMSpace requires macOS 13 or later.");

mod accessibility;
mod bar_background;
mod border;
mod custom_templates;
mod desktop_mode;
mod desktop_preview;
mod discovery;
mod focus;
mod instance;
mod layout_thumbnail;
mod model;
mod native;
mod shortcuts;
mod sky_bridge;
mod space_style;
mod space_switch;
mod spaces;
mod tile_hotkey;
mod tile_manager;
mod tile_mouse;
mod tile_picker;
mod tile_rules;
mod tiling;
mod ui;
mod updater;
mod window_move;
mod window_server;

fn main() {
    if std::env::args().any(|a| a == "--space-transfer-fixture") {
        objc2::rc::autoreleasepool(|_| window_move::run_transfer_fixture());
    } else if std::env::args().any(|a| a == "--tab-group-fixture") {
        objc2::rc::autoreleasepool(|_| tile_manager::diagnostics::run_fixture());
    } else if std::env::args().any(|a| a == "--check-tab-controls") {
        let result = objc2::rc::autoreleasepool(|_| tile_manager::diagnostics::check_controls());
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else if std::env::args().any(|a| a == "--check-tab-groups") {
        let result = objc2::rc::autoreleasepool(|_| tile_manager::diagnostics::check());
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else if std::env::args().any(|a| a == "--check-priority-shortcuts") {
        let result = objc2::rc::autoreleasepool(|_| shortcuts::check_priority_lifecycle());
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else if std::env::args().any(|a| a == "--check-transfer-preflight") {
        let result = objc2::rc::autoreleasepool(|_| window_move::check_transfer_preflight());
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else if std::env::args().any(|a| a == "--check-space-transfer") {
        let result = objc2::rc::autoreleasepool(|_| window_move::check_native_bridge());
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else if std::env::args().any(|a| a == "--render-desktop-previews") {
        objc2::rc::autoreleasepool(|_| desktop_preview::render_diagnostics());
    } else if std::env::args().any(|a| a == "--diagnose") {
        objc2::rc::autoreleasepool(|_| {
            println!("AMSpace {}", env!("CARGO_PKG_VERSION"));
            println!("Accessibility: {}", accessibility::trusted());
            println!(
                "Native Space transfer: {}",
                window_move::space_moves_available()
            );
            println!("Spaces: {:?}", spaces::snapshot());
            println!("Visible desktops: {:?}", spaces::visible_desktops());
            for (pid, id, frame) in window_server::tile_windows() {
                println!(
                    "Tile candidate: pid={pid} id={id} space={:?} frame={frame:?}",
                    spaces::window_space(id)
                );
                if let Some(window) = accessibility::window_for_id(pid, id) {
                    println!(
                        "  AX: tileable={} geometry={:?}",
                        accessibility::tileable(window.0),
                        accessibility::geometry(window.0)
                    );
                } else {
                    println!("  AX: window unavailable");
                }
            }
            unsafe {
                use objc2::{class, msg_send, runtime::AnyObject};
                let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
                let apps: *mut AnyObject = msg_send![workspace, runningApplications];
                let count: usize = msg_send![apps, count];
                for i in 0..count {
                    let running: *mut AnyObject = msg_send![apps, objectAtIndex:i];
                    let pid: i32 = msg_send![running, processIdentifier];
                    let Some(bundle) = tile_rules::application_id(pid) else {
                        continue;
                    };
                    if !bundle.contains("brave") && !bundle.contains("Spark") {
                        continue;
                    }
                    println!("App {bundle} pid={pid}");
                    if let Some(app) = accessibility::application(pid)
                        && let Some(windows) = accessibility::attr(app.0, "AXWindows")
                    {
                        for w in accessibility::array(windows.0) {
                            let id = accessibility::window_id(w);
                            println!(
                                "  Window {id:?} space={:?} tileable={} geometry={:?} fullscreen={}",
                                id.and_then(spaces::window_space),
                                accessibility::tileable(w),
                                accessibility::geometry(w),
                                accessibility::flag(w, "AXFullScreen")
                            );
                        }
                    }
                }
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
