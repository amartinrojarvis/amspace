//! Optional active session event tap. Only configured shortcuts are consumed.
//! No keystrokes are logged, and all window work is deferred to the main queue.
use crate::{
    accessibility::{Cf, Owned},
    native::Obj,
    tile_hotkey::{Action, HotKey, KEYS, MODIFIERS},
};
use std::{
    collections::HashSet,
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};
static GENERATION: AtomicU64 = AtomicU64::new(0);
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub key: usize,
    pub mods: usize,
    pub action: Action,
}
impl Binding {
    fn physical(self) -> (u32, u32) {
        (KEYS[self.key].1, MODIFIERS[self.mods].1)
    }
}
pub fn conflicts(bindings: &[Binding]) -> HashSet<Action> {
    bindings
        .iter()
        .filter(|b| {
            bindings
                .iter()
                .any(|other| other.action != b.action && other.physical() == b.physical())
        })
        .map(|b| b.action)
        .collect()
}
pub fn bindings(defaults: Obj, tiling: bool) -> Vec<Binding> {
    configured(
        defaults,
        tiling,
        crate::tile_manager::pref(defaults, "shortcuts.enabled") != 0,
    )
}
fn configured(defaults: Obj, tiling: bool, navigation: bool) -> Vec<Binding> {
    use crate::tile_manager::{pref, pref_or};
    let mut result = Vec::new();
    if navigation {
        for (prefix, send, fallback) in [
            ("shortcuts.desktop", false, 11),
            ("shortcuts.send", true, 4),
        ] {
            let mods = crate::tile_hotkey::modifier_index(pref_or(
                defaults,
                &format!("{prefix}.mods"),
                fallback,
            ));
            for (i, digit) in (1..=9).enumerate() {
                let key = KEYS
                    .iter()
                    .position(|(name, _)| *name == digit.to_string())
                    .unwrap();
                result.push(Binding {
                    key,
                    mods,
                    action: if send {
                        Action::SendSpace(i as u8)
                    } else {
                        Action::Desktop(i as u8)
                    },
                });
            }
        }
        for (name, action, key) in [
            ("left", Action::MonitorLeft, 50),
            ("right", Action::MonitorRight, 51),
        ] {
            result.push(Binding {
                key: crate::tile_hotkey::key_index(pref_or(
                    defaults,
                    &format!("shortcuts.{name}.key"),
                    key,
                )),
                mods: crate::tile_hotkey::modifier_index(pref_or(
                    defaults,
                    &format!("shortcuts.{name}.mods"),
                    9,
                )),
                action,
            });
        }
    }
    if tiling {
        for (prefix, action, key) in [
            ("tile", Action::Float, 0),
            ("tile.picker", Action::Layouts, 2),
        ] {
            result.push(Binding {
                key: crate::tile_hotkey::key_index(pref_or(
                    defaults,
                    &format!("{prefix}.key"),
                    key,
                )),
                mods: crate::tile_hotkey::modifier_index(pref(defaults, &format!("{prefix}.mods"))),
                action,
            });
        }
    }
    result
}
/// Apply a group of preference edits atomically. Invalid combinations retain
/// both old values and the distinction between a missing key and an explicit one.
pub fn edit(defaults: Obj, edits: &[(&str, isize)]) -> bool {
    use objc2::msg_send;
    use objc2_foundation::NSString;
    let previous: Vec<_> = edits
        .iter()
        .map(|(key, _)| unsafe {
            let value: Obj = msg_send![defaults,objectForKey:&*NSString::from_str(key)];
            (!value.is_null()).then(|| crate::tile_manager::pref(defaults, key))
        })
        .collect();
    for (key, value) in edits {
        crate::tile_manager::save(defaults, key, *value);
    }
    let valid = conflicts(&configured(defaults, true, true)).is_empty();
    if !valid {
        for ((key, _), before) in edits.iter().zip(previous) {
            if let Some(value) = before {
                crate::tile_manager::save(defaults, key, value);
            } else {
                unsafe {
                    let _: () = msg_send![defaults,removeObjectForKey:&*NSString::from_str(key)];
                }
            }
        }
    }
    valid
}
#[derive(Default)]
pub struct Service {
    hotkeys: Vec<HotKey>,
    tap: Option<Tap>,
    pub unavailable: Vec<Action>,
    pub priority: bool,
    active: HashSet<Action>,
}
impl Service {
    pub fn new(bindings: Vec<Binding>, priority: bool) -> Self {
        GENERATION.fetch_add(1, Ordering::AcqRel);
        let duplicates = conflicts(&bindings);
        let valid: Vec<_> = bindings
            .iter()
            .filter(|b| !duplicates.contains(&b.action))
            .copied()
            .collect();
        let mut result = Self {
            unavailable: bindings
                .iter()
                .filter(|b| duplicates.contains(&b.action))
                .map(|b| b.action)
                .collect(),
            ..Self::default()
        };
        if priority && crate::accessibility::trusted() {
            result.tap = Tap::new(valid.clone());
            result.priority = result.tap.is_some();
            if result.priority {
                result.active.extend(valid.iter().map(|b| b.action));
            }
        }
        if !result.priority {
            for b in valid {
                if let Some(key) = HotKey::new(b.key, b.mods, b.action) {
                    result.hotkeys.push(key);
                    result.active.insert(b.action);
                } else {
                    result.unavailable.push(b.action);
                }
            }
        }
        result
    }
    pub fn available(&self, action: Action) -> bool {
        self.active.contains(&action) && (!self.priority || self.priority_active())
    }
    pub fn priority_active(&self) -> bool {
        self.tap
            .as_ref()
            .is_some_and(|tap| unsafe { CGEventTapIsEnabled(tap.port.0) })
    }
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: extern "C" fn(Cf, u32, Cf, *mut c_void) -> Cf,
        data: *mut c_void,
    ) -> Cf;
    fn CGEventTapEnable(tap: Cf, enable: bool);
    fn CGEventTapIsEnabled(tap: Cf) -> bool;
    fn CGEventGetIntegerValueField(event: Cf, field: u32) -> i64;
    fn CGEventGetFlags(event: Cf) -> u64;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFMachPortCreateRunLoopSource(allocator: Cf, port: Cf, order: isize) -> Cf;
    fn CFRunLoopGetCurrent() -> Cf;
    fn CFRunLoopRun();
    fn CFRunLoopStop(run: Cf);
    fn CFRunLoopWakeUp(run: Cf);
    fn CFRunLoopPerformBlock(run: Cf, mode: Cf, block: &block2::Block<dyn Fn()>);
    fn CFRetain(value: Cf) -> Cf;
    fn CFRunLoopAddSource(run: Cf, source: Cf, mode: Cf);
    fn CFRunLoopRemoveSource(run: Cf, source: Cf, mode: Cf);
    fn CFMachPortInvalidate(port: Cf);
    static kCFRunLoopCommonModes: Cf;
}
struct Startup {
    port: usize,
    runloop: usize,
}
impl Drop for Startup {
    fn drop(&mut self) {
        drop(Owned::new(self.port as Cf));
        drop(Owned::new(self.runloop as Cf));
    }
}
struct Tap {
    port: Owned,
    runloop: Owned,
    worker: Option<JoinHandle<()>>,
}
#[derive(Default)]
struct TapState {
    bindings: Vec<Binding>,
    held: HashSet<u32>,
    port: Cf,
    generation: u64,
}
fn carbon_flags(flags: u64) -> u32 {
    [
        (1 << 20, 256),
        (1 << 17, 512),
        (1 << 19, 2048),
        (1 << 18, 4096),
    ]
    .into_iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, value)| value)
    .sum()
}
impl TapState {
    fn key(
        &mut self,
        key: u32,
        flags: u64,
        down: bool,
        repeat: bool,
        secure: bool,
    ) -> (bool, Option<Action>) {
        if !down {
            return (self.held.remove(&key), None);
        }
        if secure {
            self.held.clear();
            return (false, None);
        }
        if self.held.contains(&key) && repeat {
            return (true, None);
        }
        // A fresh non-repeat press after a missing key-up (e.g. secure input)
        // starts a new chord, rather than permanently swallowing this key.
        self.held.remove(&key);
        // Never trigger from an orphan repeat after permission/tap changes.
        if repeat {
            return (false, None);
        }
        let action = self
            .bindings
            .iter()
            .find(|b| b.physical() == (key, carbon_flags(flags)))
            .map(|b| b.action);
        if action.is_some() {
            self.held.insert(key);
        }
        (action.is_some(), action)
    }
}
impl Tap {
    fn new(bindings: Vec<Binding>) -> Option<Self> {
        if bindings.is_empty() {
            return None;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let (accept, accepted) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("amspace-shortcuts".into())
            .spawn(move || unsafe {
                let mut state = Box::new(TapState {
                    bindings,
                    generation: generation(),
                    ..TapState::default()
                });
                let Some(port) = Owned::new(CGEventTapCreate(
                    1,
                    0,
                    0,
                    (1 << 10) | (1 << 11),
                    callback,
                    (&mut *state as *mut TapState).cast(),
                )) else {
                    let _ = sender.send(None);
                    return;
                };
                state.port = port.0;
                let Some(source) =
                    Owned::new(CFMachPortCreateRunLoopSource(ptr::null(), port.0, 0))
                else {
                    CFMachPortInvalidate(port.0);
                    let _ = sender.send(None);
                    return;
                };
                let run = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run, source.0, kCFRunLoopCommonModes);
                // Transfer +1 references to the owner; it can query and stop safely
                // even if macOS invalidates the tap and the worker exits first.
                let startup = Startup {
                    port: CFRetain(port.0) as usize,
                    runloop: CFRetain(run) as usize,
                };
                if sender.send(Some(startup)).is_ok()
                    && accepted.recv_timeout(Duration::from_secs(1)).is_ok()
                {
                    CFRunLoopRun();
                }
                CGEventTapEnable(port.0, false);
                CFRunLoopRemoveSource(run, source.0, kCFRunLoopCommonModes);
                CFMachPortInvalidate(port.0);
            })
            .ok()?;
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(startup)) => unsafe {
                let port = Owned::new(CFRetain(startup.port as Cf))?;
                let runloop = Owned::new(CFRetain(startup.runloop as Cf))?;
                if accept.send(()).is_err() {
                    return None;
                }
                Some(Self {
                    port,
                    runloop,
                    worker: Some(worker),
                })
            },
            _ => None,
        }
    }
}
impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            // A queued block also handles stopping before the worker enters Run.
            let run = self.runloop.0 as usize;
            let stop = block2::RcBlock::new(move || CFRunLoopStop(run as Cf));
            CFRunLoopPerformBlock(self.runloop.0, kCFRunLoopCommonModes, &stop);
            CFRunLoopWakeUp(self.runloop.0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
extern "C" fn callback(_: Cf, kind: u32, event: Cf, data: *mut c_void) -> Cf {
    if data.is_null() {
        return event;
    }
    let state = unsafe { &mut *data.cast::<TapState>() };
    if kind == u32::MAX - 1 {
        state.held.clear();
        unsafe {
            CGEventTapEnable(state.port, true);
        }
        return event;
    }
    // Respect an explicit user/system disable; don't fight another input tool.
    if kind == u32::MAX {
        state.held.clear();
        return event;
    }
    if event.is_null() || ![10, 11].contains(&kind) {
        return event;
    }
    let key = unsafe { CGEventGetIntegerValueField(event, 9) } as u32;
    let flags = unsafe { CGEventGetFlags(event) };
    let repeat = unsafe { CGEventGetIntegerValueField(event, 8) } != 0;
    // macOS withholds protected secure-input events from session taps. The main
    // queue rechecks Carbon's secure state before executing any action.
    let (consume, action) = state.key(key, flags, kind == 10, repeat, false);
    if let Some(action) = action {
        crate::ui::queue_shortcut_generation(action, state.generation);
    }
    if consume { ptr::null() } else { event }
}
/// Main-thread developer check: a temporary, unlikely chord is installed and
/// removed repeatedly. No synthetic input is posted and no window is touched.
pub fn check_priority_lifecycle() -> Result<(), String> {
    if !crate::accessibility::trusted() {
        return Err("Accessibility is required for the active shortcut tap".into());
    }
    for _ in 0..20 {
        let tap = Tap::new(vec![Binding {
            key: 49,
            mods: 10,
            action: Action::Layouts,
        }])
        .ok_or("Priority event tap creation failed")?;
        if !unsafe { CGEventTapIsEnabled(tap.port.0) } {
            return Err("Priority event tap is not enabled".into());
        }
        drop(tap);
    }
    println!("Verified 20 priority tap startup/shutdown cycles");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> TapState {
        TapState {
            bindings: vec![Binding {
                key: 29,
                mods: 11,
                action: Action::Desktop(0),
            }],
            ..TapState::default()
        }
    }
    #[test]
    fn legacy_preferences_survive_and_duplicate_edits_roll_back_atomically() {
        use objc2::{
            class, msg_send,
            rc::{Allocated, Retained},
        };
        use objc2_foundation::NSString;
        objc2::rc::autoreleasepool(|_| unsafe {
            let name = NSString::from_str(&format!("amspace.shortcut-test.{}", std::process::id()));
            let defaults: Allocated<objc2::runtime::AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<objc2::runtime::AnyObject> =
                msg_send![defaults,initWithSuiteName:&*name];
            let obj = Retained::as_ptr(&defaults) as Obj;
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*name];
            crate::tile_manager::save(obj, "tile.key", 14);
            crate::tile_manager::save(obj, "tile.mods", 4);
            let legacy = bindings(obj, true);
            assert_eq!(legacy.len(), 2);
            assert_eq!(
                legacy[0],
                Binding {
                    key: 14,
                    mods: 4,
                    action: Action::Float
                }
            );
            assert!(bindings(obj, false).is_empty());
            crate::tile_manager::save(obj, "shortcuts.enabled", 1);
            let all = bindings(obj, true);
            assert_eq!(all.len(), 22);
            assert!(conflicts(&all).is_empty());
            assert_eq!(
                all.iter()
                    .find(|b| b.action == Action::Desktop(0))
                    .unwrap()
                    .physical(),
                (18, 256)
            );
            assert_eq!(crate::tile_manager::pref_or(obj, "shortcuts.stay", 1), 1);
            assert!(!edit(
                obj,
                &[("shortcuts.desktop.mods", 4), ("shortcuts.right.key", 0)]
            ));
            let absent: Obj =
                msg_send![&defaults,objectForKey:&*NSString::from_str("shortcuts.desktop.mods")];
            assert!(absent.is_null());
            let absent: Obj =
                msg_send![&defaults,objectForKey:&*NSString::from_str("shortcuts.right.key")];
            assert!(absent.is_null());
            assert_eq!(bindings(obj, true), all);
            // Both values change together: an intermediate conflict is harmless.
            assert!(edit(obj, &[("tile.key", 2), ("tile.mods", 3)]));
            assert_eq!(
                bindings(obj, true)
                    .iter()
                    .find(|b| b.action == Action::Float)
                    .unwrap()
                    .physical(),
                (49, 256 | 512)
            );
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*name];
        });
    }
    #[test]
    fn duplicate_detection_keeps_unrelated_shortcuts_available() {
        let a = state().bindings[0];
        let b = Binding {
            action: Action::SendSpace(0),
            ..a
        };
        let c = Binding {
            key: 30,
            action: Action::Desktop(1),
            ..a
        };
        let bad = conflicts(&[a, b, c]);
        assert!(bad.contains(&a.action));
        assert!(bad.contains(&b.action));
        assert!(!bad.contains(&c.action));
    }
    #[test]
    fn secure_input_clears_a_held_chord_without_triggering_an_action() {
        let mut s = state();
        assert!(s.key(18, 1 << 20, true, false, false).0);
        assert_eq!(s.key(18, 1 << 20, true, true, true), (false, None));
        assert!(s.held.is_empty());
        assert_eq!(s.key(18, 0, false, false, false), (false, None));
        assert_eq!(s.key(18, 1 << 20, true, true, false), (false, None));
    }
    #[test]
    fn unrelated_key_releases_do_not_clear_the_consumed_chord() {
        let mut s = state();
        s.key(18, 1 << 20, true, false, false);
        assert_eq!(s.key(19, 0, false, false, false), (false, None));
        assert_eq!(s.key(18, 0, false, false, false), (true, None));
        assert!(s.held.is_empty());
    }
    #[test]
    fn caps_lock_and_keypad_flags_do_not_change_the_configured_modifiers() {
        assert_eq!(carbon_flags((1 << 20) | (1 << 16) | (1 << 21)), 256);
        assert_eq!(
            carbon_flags((1 << 20) | (1 << 17) | (1 << 19) | (1 << 18)),
            256 | 512 | 2048 | 4096
        );
    }
    #[test]
    fn priority_consumes_only_exact_bindings_and_balances_release() {
        let mut s = state();
        assert_eq!(
            s.key(18, 1 << 20, true, false, false),
            (true, Some(Action::Desktop(0)))
        );
        assert_eq!(s.key(18, 1 << 20, true, true, false), (true, None));
        assert_eq!(s.key(18, 0, false, false, false), (true, None));
        assert_eq!(s.key(18, 0, false, false, false), (false, None));
        assert_eq!(
            s.key(18, (1 << 20) | (1 << 17), true, false, false),
            (false, None)
        );
        assert_eq!(s.key(19, 1 << 20, true, false, false), (false, None));
        assert_eq!(s.key(18, 1 << 20, true, false, true), (false, None));
        assert_eq!(s.key(18, 1 << 20, true, true, false), (false, None));
    }
    #[test]
    fn duplicates_disable_both_actions_and_action_ids_round_trip() {
        let first = state().bindings[0];
        let second = Binding {
            action: Action::Float,
            ..first
        };
        assert_eq!(conflicts(&[first, second]).len(), 2);
        for id in [1, 2, 10, 18, 30, 38, 50, 51] {
            assert_eq!(Action::from_id(id).unwrap().id(), id);
        }
        assert_eq!(Action::from_id(19), None);
    }
}
