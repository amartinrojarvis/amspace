//! Persistent application defaults keyed by bundle identifier, never display name.
use crate::native::Obj;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
const KEY: &str = "tile.floatingApps";
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
        let dict: Obj = msg_send![defaults,dictionaryForKey:&*NSString::from_str(KEY)];
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
    let Some(id) = application_id(pid) else {
        return false;
    };
    entries(defaults).iter().any(|(key, _)| *key == id)
}
pub fn set(defaults: Obj, id: &str, name: Option<&str>) {
    unsafe {
        let saved: Obj = msg_send![defaults,dictionaryForKey:&*NSString::from_str(KEY)];
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
        let _: () = msg_send![defaults,setObject:&*dict,forKey:&*NSString::from_str(KEY)];
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
