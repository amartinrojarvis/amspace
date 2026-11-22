//! Scene preferences contain display identities, app bundle IDs and ordinal layouts only.
use crate::{i18n::tr, native::Obj, tile_manager::scenes::SceneDisplay, tiling::Desktop};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSString, ns_string};

const LIMIT: usize = 64;
fn key() -> &'static NSString {
    ns_string!("tile.scenes.v1")
}

pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    pub displays: Vec<SceneDisplay>,
}

fn valid_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

pub(crate) fn ordinal_desktop(display: &SceneDisplay) -> Option<Desktop> {
    let mut desktop = Desktop::configured(Default::default(), None);
    desktop.order = (1..=display.bundles.len() as u32).collect();
    desktop
        .restore_distribution(&display.distribution)
        .then_some(desktop)
}

fn valid(displays: &[SceneDisplay]) -> bool {
    (1..=32).contains(&displays.len())
        && displays.iter().enumerate().all(|(i, d)| {
            if !valid_text(&d.uuid, 64)
                || !valid_text(&d.name, 512)
                || displays[..i].iter().any(|other| other.uuid == d.uuid)
                || d.distribution.len() > 65536
                || !(1..=128).contains(&d.bundles.len())
                || !d.bundles.iter().all(|id| valid_text(id, 512))
            {
                return false;
            }
            ordinal_desktop(d).is_some()
        })
}

fn string(obj: Obj) -> Option<String> {
    (!obj.is_null() && unsafe { msg_send![obj, isKindOfClass:class!(NSString)] })
        .then(|| unsafe { (&*obj.cast::<NSString>()).to_string() })
}

fn array<T>(obj: Obj, limit: usize, parse: impl Fn(Obj) -> Option<T>) -> Option<Vec<T>> {
    if obj.is_null() || !unsafe { msg_send![obj, isKindOfClass:class!(NSArray)] } {
        return None;
    }
    let count: usize = unsafe { msg_send![obj, count] };
    if count == 0 || count > limit {
        return None;
    }
    (0..count)
        .map(|i| parse(unsafe { msg_send![obj, objectAtIndex:i] }))
        .collect()
}

fn is_dict(obj: Obj) -> bool {
    !obj.is_null() && unsafe { msg_send![obj, isKindOfClass:class!(NSDictionary)] }
}

fn property(dict: Obj, key: &NSString) -> Obj {
    unsafe { msg_send![dict, objectForKey:key] }
}

fn parse_display(obj: Obj) -> Option<SceneDisplay> {
    if !is_dict(obj) {
        return None;
    }
    Some(SceneDisplay {
        uuid: string(property(obj, ns_string!("uuid")))?,
        name: string(property(obj, ns_string!("name")))?,
        distribution: string(property(obj, ns_string!("layout")))?,
        bundles: array(property(obj, ns_string!("apps")), 128, string)?,
    })
}

fn parse_entry(dict: Obj, key: Obj, id: String) -> Option<Entry> {
    let obj: Obj = unsafe { msg_send![dict, objectForKey:key] };
    if !valid_text(&id, 80) || !is_dict(obj) {
        return None;
    }
    let name = string(property(obj, ns_string!("name")))?;
    let displays = array(property(obj, ns_string!("displays")), 32, parse_display)?;
    (valid_text(&name, 240) && valid(&displays)).then_some(Entry { id, name, displays })
}

pub(crate) fn entry(defaults: Obj, id: &str) -> Option<Entry> {
    let dict: Obj = unsafe { msg_send![defaults, dictionaryForKey:key()] };
    if dict.is_null() {
        return None;
    }
    let key = NSString::from_str(id);
    parse_entry(dict, Retained::as_ptr(&key) as Obj, id.to_owned())
}

pub(crate) fn entries(defaults: Obj) -> Vec<Entry> {
    unsafe {
        let dict: Obj = msg_send![defaults, dictionaryForKey:key()];
        if dict.is_null() {
            return Vec::new();
        }
        let keys: Obj = msg_send![dict, allKeys];
        let count: usize = msg_send![keys, count];
        let mut entries: Vec<_> = (0..count.min(LIMIT))
            .filter_map(|i| {
                let key: Obj = msg_send![keys, objectAtIndex:i];
                parse_entry(dict, key, string(key)?)
            })
            .collect();
        entries.sort_by_cached_key(|item| item.name.to_lowercase());
        entries
    }
}

fn mutable_library(defaults: Obj) -> Retained<AnyObject> {
    unsafe {
        let saved: Obj = msg_send![defaults, dictionaryForKey:key()];
        if saved.is_null() {
            msg_send![class!(NSMutableDictionary), new]
        } else {
            msg_send![saved, mutableCopy]
        }
    }
}

pub(crate) fn save(
    defaults: Obj,
    name: &str,
    displays: &[SceneDisplay],
) -> Result<String, &'static str> {
    let name = name.trim();
    if !valid_text(name, 240) || name.chars().count() > 60 {
        return Err(tr("Escribe un nombre de entre 1 y 60 caracteres."));
    }
    if !valid(displays) {
        return Err(tr("No se puede guardar esta escena."));
    }
    let normalized = name.to_lowercase();
    if entries(defaults)
        .iter()
        .any(|item| item.name.to_lowercase() == normalized)
    {
        return Err(tr("Ya existe una escena con ese nombre."));
    }
    unsafe {
        let library = mutable_library(defaults);
        let count: usize = msg_send![&library, count];
        if count >= LIMIT {
            return Err(tr("La biblioteca admite hasta 64 escenas."));
        }
        let screens: Retained<AnyObject> = msg_send![class!(NSMutableArray), new];
        for d in displays {
            let record: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            for (key, value) in [
                (ns_string!("uuid"), &d.uuid),
                (ns_string!("name"), &d.name),
                (ns_string!("layout"), &d.distribution),
            ] {
                let _: () = msg_send![&record, setObject:&*NSString::from_str(value), forKey:key];
            }
            let apps: Retained<AnyObject> = msg_send![class!(NSMutableArray), new];
            for bundle in &d.bundles {
                let _: () = msg_send![&apps, addObject:&*NSString::from_str(bundle)];
            }
            let _: () = msg_send![&record, setObject:&*apps, forKey:ns_string!("apps")];
            let _: () = msg_send![&screens, addObject:&*record];
        }
        let entry: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
        let _: () = msg_send![&entry, setObject:&*screens, forKey:ns_string!("displays")];
        let _: () =
            msg_send![&entry, setObject:&*NSString::from_str(name), forKey:ns_string!("name")];
        let uuid: Obj = msg_send![class!(NSUUID), UUID];
        let id: Obj = msg_send![uuid, UUIDString];
        let result = string(id).ok_or(tr("No se puede guardar esta escena."))?;
        let _: () = msg_send![&library, setObject:&*entry, forKey:id];
        let _: () = msg_send![defaults, setObject:&*library, forKey:key()];
        Ok(result)
    }
}

pub(crate) fn remove(defaults: Obj, id: &str) {
    unsafe {
        let library = mutable_library(defaults);
        let _: () = msg_send![&library, removeObjectForKey:&*NSString::from_str(id)];
        let _: () = msg_send![defaults, setObject:&*library, forKey:key()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{native::rect, tiling::Layout};
    use objc2::rc::{Allocated, autoreleasepool};

    #[test]
    fn saved_scene_round_trips_without_runtime_window_ids_and_rejects_duplicate_names() {
        autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!("amspace.scene-test.{}", std::process::id()));
            let alloc: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![alloc, initWithSuiteName:&*suite];
            let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
            let prefs = Retained::as_ptr(&defaults) as Obj;
            let area = rect(0., 0., 1200., 900.);
            let mut desktop = Desktop::configured(Layout::Spiral, None);
            desktop.reconcile(&[3041, 7923], area, 10.);
            let displays = [SceneDisplay {
                name: "Principal".into(),
                uuid: "fixture-screen".into(),
                distribution: desktop.save_distribution(area).unwrap(),
                bundles: vec!["app.notes".into(), "app.notes".into()],
            }];
            let id = save(prefs, "  Trabajo  ", &displays).unwrap();
            assert!(save(prefs, "trabajo", &displays).is_err());
            let entry = entries(prefs).pop().unwrap();
            assert_eq!(
                (entry.id.as_str(), entry.name.as_str()),
                (id.as_str(), "Trabajo")
            );
            let display = &entry.displays[0];
            assert_eq!(display.uuid, displays[0].uuid);
            assert_eq!(display.bundles, displays[0].bundles);
            let mut restarted = Desktop::configured(Layout::Spiral, None);
            restarted.reconcile(&[15, 14], area, 10.);
            assert!(restarted.restore_distribution(&display.distribution));
            assert_eq!(restarted.order, [15, 14]);
            remove(prefs, &id);
            assert!(entries(prefs).is_empty());
            let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        });
    }
}
