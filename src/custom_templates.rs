//! Named, portable tile trees. No native window IDs or application contents.
use crate::{native::Obj, tiling::Desktop};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;

const KEY: &str = "tile.templates.v1";
const LIMIT: usize = 64;
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub distribution: String,
    pub count: usize,
}
impl Template {
    pub fn parse(id: String, name: String, distribution: String) -> Option<Self> {
        if id.is_empty() || id.len() > 80 || distribution.len() > 65536 {
            return None;
        }
        let name = valid_name(&name).ok()?;
        let count = distribution.split_whitespace().nth(2)?.parse().ok()?;
        if !(1..=128).contains(&count) {
            return None;
        }
        let template = Self {
            id,
            name,
            distribution,
            count,
        };
        template.preview()?;
        Some(template)
    }
    pub fn preview(&self) -> Option<Desktop> {
        let mut desktop = Desktop::default();
        desktop.order = (1..=self.count as u32).collect();
        desktop
            .restore_distribution(&self.distribution)
            .then_some(desktop)
    }
}
fn valid_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(crate::i18n::tr(
            "Escribe un nombre de entre 1 y 60 caracteres.",
        ));
    }
    Ok(name.to_string())
}
unsafe fn string(object: Obj) -> Option<String> {
    if object.is_null() || !unsafe { msg_send![object,isKindOfClass:class!(NSString)] } {
        return None;
    }
    unsafe { object.cast::<NSString>().as_ref() }.map(ToString::to_string)
}
pub fn entries(defaults: Obj) -> Vec<Template> {
    unsafe {
        let dictionary: Obj = msg_send![defaults,dictionaryForKey:&*NSString::from_str(KEY)];
        if dictionary.is_null() {
            return vec![];
        }
        let keys: Obj = msg_send![dictionary, allKeys];
        let count: usize = msg_send![keys, count];
        let mut templates = vec![];
        for index in 0..count.min(LIMIT) {
            let key: Obj = msg_send![keys,objectAtIndex:index];
            let Some(id) = string(key) else { continue };
            let value: Obj = msg_send![dictionary,objectForKey:key];
            let dictionary: bool = msg_send![value,isKindOfClass:class!(NSDictionary)];
            if !dictionary {
                continue;
            }
            let name: Obj = msg_send![value,objectForKey:&*NSString::from_str("name")];
            let data: Obj = msg_send![value,objectForKey:&*NSString::from_str("distribution")];
            if let (Some(name), Some(data)) = (string(name), string(data))
                && let Some(template) = Template::parse(id, name, data)
            {
                templates.push(template);
            }
        }
        templates.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        templates
    }
}
unsafe fn mutable_library(defaults: Obj) -> Retained<AnyObject> {
    unsafe {
        let saved: Obj = msg_send![defaults,dictionaryForKey:&*NSString::from_str(KEY)];
        if saved.is_null() {
            msg_send![class!(NSMutableDictionary), new]
        } else {
            msg_send![class!(NSMutableDictionary),dictionaryWithDictionary:saved]
        }
    }
}
pub fn save(defaults: Obj, name: &str, distribution: String) -> Result<String, &'static str> {
    let name = valid_name(name)?;
    if entries(defaults)
        .iter()
        .any(|item| item.name.to_lowercase() == name.to_lowercase())
    {
        return Err(crate::i18n::tr(
            "Ya existe una plantilla con ese nombre. Elige otro nombre.",
        ));
    }
    unsafe {
        let library = mutable_library(defaults);
        let count: usize = msg_send![&library, count];
        if count >= LIMIT {
            return Err(crate::i18n::tr(
                "Puedes guardar hasta 64 plantillas. Elimina una antes de añadir otra.",
            ));
        }
        let uuid: Obj = msg_send![class!(NSUUID), UUID];
        let id: Obj = msg_send![uuid, UUIDString];
        let id = string(id).ok_or(crate::i18n::tr("No se pudo crear la plantilla."))?;
        let template = Template::parse(id.clone(), name, distribution)
            .ok_or(crate::i18n::tr("Esta distribución no se puede guardar."))?;
        let value: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
        let _: () = msg_send![&value,setObject:&*NSString::from_str(&template.name),forKey:&*NSString::from_str("name")];
        let _: () = msg_send![&value,setObject:&*NSString::from_str(&template.distribution),forKey:&*NSString::from_str("distribution")];
        let _: () = msg_send![&library,setObject:&*value,forKey:&*NSString::from_str(&id)];
        let _: () = msg_send![defaults,setObject:&*library,forKey:&*NSString::from_str(KEY)];
        Ok(id)
    }
}
pub fn remove(defaults: Obj, id: &str) {
    unsafe {
        let library = mutable_library(defaults);
        let _: () = msg_send![&library,removeObjectForKey:&*NSString::from_str(id)];
        let _: () = msg_send![defaults,setObject:&*library,forKey:&*NSString::from_str(KEY)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{native::rect, tiling::Layout};
    #[test]
    fn templates_are_portable_between_window_ids_and_keep_custom_proportions() {
        for layout in Layout::ALL {
            for grid in [None, Some((12, 8))] {
                let area = rect(10., 41., 1600., 950.);
                let mut desktop = Desktop::configured(layout, grid);
                desktop.reconcile(&[81, 92, 103], area, 10.);
                let from = desktop.frames(area, 10.)[0].1;
                let mut to = from;
                to.size.width *= 0.85;
                desktop.resize(81, from, to, area, 10.);
                let text = desktop.save_distribution(area).unwrap();
                let template =
                    Template::parse("id".into(), " Mi diseño ".into(), text.clone()).unwrap();
                assert_eq!(template.name, "Mi diseño");
                assert_eq!(template.count, 3);
                assert_eq!(
                    template.preview().unwrap().save_distribution(area),
                    Some(text)
                );
                let mut other = Desktop::configured(Layout::Grid, None);
                other.reconcile(&[201, 202, 203], area, 10.);
                assert!(other.restore_distribution(&template.distribution));
                let before = desktop.frames(area, 10.);
                for ((_, a), (_, b)) in before.iter().zip(other.frames(area, 10.)) {
                    assert_eq!(*a, b);
                }
                assert_eq!(other.order, vec![201, 202, 203]);
            }
        }
    }
    #[test]
    fn malformed_templates_and_names_are_rejected() {
        for text in [
            "",
            "2 1 99999999 0 0 w0",
            "2 1 1 0 0 vNaN w0 w0",
            "2 1 2 0 0 v0.5 w0 w0",
        ] {
            assert!(Template::parse("id".into(), "Test".into(), text.into()).is_none());
        }
        for name in ["", "  ", "bad\nname"] {
            assert!(valid_name(name).is_err());
        }
        assert!(valid_name(&"a".repeat(61)).is_err());
    }
    #[test]
    fn named_library_persists_duplicates_do_not_replace_and_removal_is_isolated() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite = NSString::from_str(&format!("amspace.custom-test.{}", std::process::id()));
            let defaults: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let obj = Retained::as_ptr(&defaults) as Obj;
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let data = "2 1 2 0 0 v0.6 w0 w1".to_string();
            let id = save(obj, "Trabajo", data.clone()).unwrap();
            assert!(save(obj, " trabajo ", data.clone()).is_err());
            let other = save(obj, "Lectura", data).unwrap();
            assert_eq!(entries(obj).len(), 2);
            remove(obj, &id);
            assert_eq!(
                entries(obj).iter().map(|t| &t.id).collect::<Vec<_>>(),
                vec![&other]
            );
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
    #[test]
    fn library_capacity_unicode_and_failed_saves_preserve_all_existing_templates() {
        objc2::rc::autoreleasepool(|_| unsafe {
            let suite =
                NSString::from_str(&format!("amspace.custom-capacity.{}", std::process::id()));
            let defaults: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
            let obj = Retained::as_ptr(&defaults) as Obj;
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
            let data = "2 1 2 0 0 v0.6 w0 w1".to_string();
            for i in 0..LIMIT {
                save(obj, &format!("Edición 🎬 {i:02}"), data.clone()).unwrap();
            }
            let before = entries(obj);
            assert_eq!(before.len(), LIMIT);
            for (name, text) in [
                ("Otra", data.clone()),
                ("edición 🎬 00", data.clone()),
                ("", data.clone()),
                ("Dañada", "2 1 2 0 0 vNaN w0 w1".into()),
            ] {
                assert!(save(obj, name, text).is_err());
                assert_eq!(entries(obj), before);
            }
            remove(obj, "does-not-exist");
            assert_eq!(entries(obj), before);
            remove(obj, &before[0].id);
            let name = "ñ".repeat(60);
            save(obj, &name, data.clone()).unwrap();
            assert_eq!(entries(obj).len(), LIMIT);
            let reloaded: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let reloaded: Retained<AnyObject> = msg_send![reloaded,initWithSuiteName:&*suite];
            assert_eq!(entries(Retained::as_ptr(&reloaded) as Obj), entries(obj));
            assert!(Template::parse("x".into(), "OK".into(), "x".repeat(65537)).is_none());
            assert!(Template::parse("x".repeat(81), "OK".into(), data).is_none());
            assert!(valid_name(&"ñ".repeat(61)).is_err());
            let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        });
    }
}
