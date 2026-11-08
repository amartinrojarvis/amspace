//! Persistent application defaults keyed by bundle identifier, never display name.
use crate::native::Obj;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSString, ns_string};
pub fn application_id(pid: i32) -> Option<String> {
    unsafe {
        let app: Obj =
            msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid];
        if app.is_null() {
            return None;
        }
        let id: *mut NSString = msg_send![app, bundleIdentifier];
        id.as_ref().map(|s| s.to_string())
    }
}
pub fn entries(defaults: Obj) -> Vec<(String, String)> {
    unsafe {
        let dict: Obj = msg_send![defaults,dictionaryForKey:ns_string!("tile.floatingApps")];
        if dict.is_null() {
            return vec![];
        }
        let keys: Obj = msg_send![dict, allKeys];
        let count: usize = msg_send![keys, count];
        let mut result = vec![];
        for i in 0..count {
            let key: *mut NSString = msg_send![keys,objectAtIndex:i];
            let name: *mut NSString = msg_send![dict,objectForKey:key];
            if !key.is_null() && !name.is_null() {
                let valid: bool = msg_send![name,isKindOfClass:class!(NSString)];
                if valid {
                    result.push(((*key).to_string(), (*name).to_string()));
                }
            }
        }
        result.sort_by_key(|(_, name)| name.to_lowercase());
        result
    }
}
pub fn floats(defaults: Obj, pid: i32) -> bool {
    unsafe {
        let app: Obj =
            msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid];
        if app.is_null() {
            return false;
        }
        let id: *mut NSString = msg_send![app, bundleIdentifier];
        id.as_ref()
            .is_some_and(|id| floats_identifier(defaults, id))
    }
}
fn floats_identifier(defaults: Obj, id: &NSString) -> bool {
    unsafe {
        let dict: Obj = msg_send![defaults,dictionaryForKey:ns_string!("tile.floatingApps")];
        if dict.is_null() {
            return false;
        }
        let entry: Obj = msg_send![dict,objectForKey:id];
        !entry.is_null() && msg_send![entry,isKindOfClass:class!(NSString)]
    }
}
pub fn set(defaults: Obj, id: &str, name: Option<&str>) {
    unsafe {
        let saved: Obj = msg_send![defaults,dictionaryForKey:ns_string!("tile.floatingApps")];
        let dict: Retained<AnyObject> = if saved.is_null() {
            msg_send![class!(NSMutableDictionary), new]
        } else {
            msg_send![class!(NSMutableDictionary),dictionaryWithDictionary:saved]
        };
        if let Some(name) = name {
            let _: () = msg_send![&dict,setObject:&*NSString::from_str(name),forKey:&*NSString::from_str(id)];
        } else {
            let _: () = msg_send![&dict,removeObjectForKey:&*NSString::from_str(id)];
        }
        let _: () = msg_send![defaults,setObject:&*dict,forKey:ns_string!("tile.floatingApps")];
    }
}
/// NSOpenPanel-selected bundle URL; does not launch the application.
pub fn bundle(url: Obj) -> Option<(String, String)> {
    unsafe {
        let bundle: Obj = msg_send![class!(NSBundle),bundleWithURL:url];
        if bundle.is_null() {
            return None;
        }
        let id: *mut NSString = msg_send![bundle, bundleIdentifier];
        let id = id.as_ref()?.to_string();
        let name: *mut NSString = msg_send![bundle,objectForInfoDictionaryKey:&*NSString::from_str("CFBundleDisplayName")];
        let name = if name.is_null() {
            msg_send![bundle,objectForInfoDictionaryKey:&*NSString::from_str("CFBundleName")]
        } else {
            name
        };
        let name = name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| id.clone());
        Some((id, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "CPU-only preferences lookup benchmark; run with --release --ignored --nocapture"]
    fn benchmark_floating_membership() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!(
                "amspace.tests.rules.benchmark.{}",
                std::process::id()
            ));
            let defaults: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            for index in 0..64 {
                set(
                    raw,
                    &format!("org.example.app.{index}"),
                    Some(&format!("Application {index}")),
                );
            }
            let id = ns_string!("org.example.app.31");
            let start = std::time::Instant::now();
            for _ in 0..2000 {
                objc2::rc::autoreleasepool(|_| {
                    assert!(
                        std::hint::black_box(entries(raw))
                            .iter()
                            .any(|(key, _)| key == "org.example.app.31")
                    );
                });
            }
            let old = start.elapsed();
            let start = std::time::Instant::now();
            for _ in 0..2000 {
                objc2::rc::autoreleasepool(|_| {
                    assert!(std::hint::black_box(floats_identifier(raw, id)));
                });
            }
            let new = start.elapsed();
            println!(
                "floating_membership samples=2000 rules=64 before_ns={:.1} after_ns={:.1} speedup={:.2}",
                old.as_nanos() as f64 / 2000.,
                new.as_nanos() as f64 / 2000.,
                old.as_secs_f64() / new.as_secs_f64()
            );
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
    #[test]
    fn direct_membership_matches_catalog_for_updates_empty_names_and_invalid_values() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!(
                "amspace.tests.rules.lookup.{}",
                std::process::id()
            ));
            let defaults: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            assert!(!floats_identifier(raw, ns_string!("test.one")));
            for (id, name) in [
                ("test.one", "Finder"),
                ("test.two", "Editor"),
                ("test.empty", ""),
                ("test.unicode", "Árbol 🪟"),
            ] {
                set(raw, id, Some(name));
            }
            let saved: Obj = msg_send![raw,dictionaryForKey:ns_string!("tile.floatingApps")];
            let dict: Retained<AnyObject> =
                msg_send![class!(NSMutableDictionary),dictionaryWithDictionary:saved];
            let invalid: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithInt:1i32];
            let _: () = msg_send![&dict,setObject:&*invalid,forKey:ns_string!("test.invalid")];
            let _: () = msg_send![raw,setObject:&*dict,forKey:ns_string!("tile.floatingApps")];
            for id in [
                "test.one",
                "test.two",
                "test.empty",
                "test.unicode",
                "test.invalid",
                "test.missing",
            ] {
                assert_eq!(
                    floats_identifier(raw, &NSString::from_str(id)),
                    entries(raw).iter().any(|(key, _)| key == id)
                );
            }
            set(raw, "test.one", Some("Renamed"));
            assert!(floats_identifier(raw, ns_string!("test.one")));
            set(raw, "test.one", None);
            assert!(!floats_identifier(raw, ns_string!("test.one")));
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
    #[test]
    fn rules_persist_by_bundle_id_and_deduplicate() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!("amspace.tests.rules.{}", std::process::id()));
            let defaults: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let raw = Retained::as_ptr(&defaults) as Obj;
            set(raw, "test.one", Some("Editor"));
            set(raw, "test.two", Some("Editor"));
            set(raw, "test.one", Some("Renamed"));
            assert_eq!(
                entries(raw),
                vec![
                    ("test.two".into(), "Editor".into()),
                    ("test.one".into(), "Renamed".into())
                ]
            );
            set(raw, "test.one", None);
            assert_eq!(entries(raw), vec![("test.two".into(), "Editor".into())]);
            set(raw, "test.two", None);
            assert!(entries(raw).is_empty());
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
}
