//! Authored UI messages only. Resolve once before AppKit; never translate user data.
use crate::native::Obj;
use objc2::{
    class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::AnyObject,
};
use objc2_foundation::{NSString, ns_string};
use std::sync::OnceLock;

mod help;
mod misc;
mod ui;
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Spanish,
    English,
}
impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Spanish => "es",
            Self::English => "en",
        }
    }
    fn from_tag(tag: &str) -> Option<Self> {
        match tag.split(['-', '_']).next()? {
            code if code.eq_ignore_ascii_case("es") => Some(Self::Spanish),
            code if code.eq_ignore_ascii_case("en") => Some(Self::English),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    System,
    Spanish,
    English,
}
impl Choice {
    fn parse(value: &str) -> Self {
        match value {
            "es" => Self::Spanish,
            "en" => Self::English,
            _ => Self::System,
        }
    }
    fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::System),
            1 => Some(Self::Spanish),
            2 => Some(Self::English),
            _ => None,
        }
    }
    fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Spanish => 1,
            Self::English => 2,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Spanish => "es",
            Self::English => "en",
        }
    }
}

static LANGUAGE: OnceLock<Language> = OnceLock::new();

fn resolve(choice: Choice, preferred: impl IntoIterator<Item = Language>) -> Language {
    match choice {
        Choice::Spanish => Language::Spanish,
        Choice::English => Language::English,
        Choice::System => preferred.into_iter().next().unwrap_or(Language::English),
    }
}

fn choice(defaults: Obj) -> Choice {
    unsafe {
        let stored: *mut NSString = msg_send![defaults, stringForKey:ns_string!("app.language")];
        if stored.is_null() {
            return Choice::System;
        }
        let bytes: *const std::ffi::c_char = msg_send![stored, UTF8String];
        if bytes.is_null() {
            return Choice::System;
        }
        std::ffi::CStr::from_ptr(bytes)
            .to_str()
            .map(Choice::parse)
            .unwrap_or(Choice::System)
    }
}

/// Selection for the next launch; this never changes the current view tree.
pub fn preference(defaults: Obj) -> usize {
    choice(defaults).index()
}

pub fn set_preference(defaults: Obj, index: usize) -> bool {
    let Some(selected) = Choice::from_index(index) else {
        return false;
    };
    if selected == choice(defaults) {
        return false;
    }
    unsafe {
        let _: () = msg_send![defaults, setObject:&*NSString::from_str(selected.key()), forKey:ns_string!("app.language")];
    }
    true
}

/// Must precede NSApplication, main-bundle localization and Sparkle construction.
pub fn init() {
    LANGUAGE.get_or_init(|| autoreleasepool(|_| unsafe {
        let defaults: Obj = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let preferred: Obj = msg_send![class!(NSLocale), preferredLanguages];
        let count: usize = msg_send![preferred, count];
        let preferences = (0..count).filter_map(|index| {
            let value: Obj = msg_send![preferred, objectAtIndex:index];
            let bytes: *const std::ffi::c_char = msg_send![value, UTF8String];
            if bytes.is_null() { return None; }
            std::ffi::CStr::from_ptr(bytes).to_str().ok().and_then(Language::from_tag)
        });
        let language = resolve(choice(defaults), preferences);
        // Process-local argument domain only. Preserve all other launch arguments;
        // never persist AppleLanguages or modify the user's system language list.
        let current: Obj = msg_send![defaults, volatileDomainForName:ns_string!("NSArgumentDomain")];
        let domain: Retained<AnyObject> = if current.is_null() {
            msg_send![class!(NSMutableDictionary), new]
        } else { msg_send![current, mutableCopy] };
        let languages: Obj = msg_send![class!(NSArray), arrayWithObject:&*NSString::from_str(language.code())];
        let _: () = msg_send![&domain, setObject:languages, forKey:ns_string!("AppleLanguages")];
        let _: () = msg_send![defaults, setVolatileDomain:&*domain, forName:ns_string!("NSArgumentDomain")];
        language
    }));
}

pub fn language() -> Language {
    LANGUAGE.get().copied().unwrap_or(Language::Spanish)
}
pub fn is_english() -> bool {
    language() == Language::English
}

fn translate(language: Language, text: &str) -> &str {
    if language == Language::Spanish {
        return text;
    }
    ui::english(text)
        .or_else(|| windows::english(text))
        .or_else(|| help::english(text))
        .or_else(|| misc::english(text))
        .unwrap_or(text)
}

/// Spanish source keys are intentional; missing/technical/user strings are unchanged.
/// Pass authored messages, not window titles, application names or saved names.
pub fn tr(text: &str) -> &str {
    translate(language(), text)
}

/// Literal tokens stay at the caller, so Rust checks argument types and captures.
/// The selected branch performs exactly the ordinary format! allocation.
#[macro_export]
macro_rules! tr_format {
    ($spanish:literal, $english:literal) => {
        if $crate::i18n::is_english() { format!($english) } else { format!($spanish) }
    };
    ($spanish:literal, $english:literal, $($args:tt)*) => {
        if $crate::i18n::is_english() { format!($english, $($args)*) } else { format!($spanish, $($args)*) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_resolution_respects_override_regional_tags_order_and_english_fallback() {
        for (choice, preferred, expected) in [
            (Choice::System, vec!["en-GB", "es"], Language::English),
            (Choice::System, vec!["es_MX", "en"], Language::Spanish),
            (
                Choice::System,
                vec!["fr-FR", "ES-es", "en"],
                Language::Spanish,
            ),
            (Choice::System, vec!["de-DE", "fr"], Language::English),
            (Choice::System, vec![], Language::English),
            (Choice::Spanish, vec!["en-US"], Language::Spanish),
            (Choice::English, vec!["es-ES"], Language::English),
        ] {
            assert_eq!(
                resolve(choice, preferred.into_iter().filter_map(Language::from_tag)),
                expected
            );
        }
        assert_eq!(Choice::parse("invalid"), Choice::System);
        assert_eq!(Language::from_tag("english"), None);
    }
    #[test]
    fn language_selection_preserves_other_preferences_and_applies_only_next_launch() {
        autoreleasepool(|_| unsafe {
            let name = NSString::from_str(&format!("amspace.language-test.{}", std::process::id()));
            let allocated: objc2::rc::Allocated<AnyObject> =
                msg_send![class!(NSUserDefaults), alloc];
            let defaults: Retained<AnyObject> = msg_send![allocated, initWithSuiteName:&*name];
            let raw = Retained::as_ptr(&defaults) as Obj;
            let _: () = msg_send![&defaults, removePersistentDomainForName:&*name];
            let _: () =
                msg_send![&defaults, setInteger:7isize, forKey:ns_string!("tile.layout.42.7")];
            let before = language();
            for index in [2, 1, 0] {
                assert!(set_preference(raw, index));
                assert_eq!(preference(raw), index);
                assert_eq!(language(), before);
                let layout: isize =
                    msg_send![&defaults, integerForKey:ns_string!("tile.layout.42.7")];
                assert_eq!(layout, 7);
            }
            assert!(!set_preference(raw, 999));
            assert_eq!(preference(raw), 0);
            let domain: Obj = msg_send![&defaults, persistentDomainForName:&*name];
            let languages: Obj = msg_send![domain, objectForKey:ns_string!("AppleLanguages")];
            assert!(languages.is_null());
            let _: () = msg_send![&defaults, removePersistentDomainForName:&*name];
        });
    }
    #[test]
    fn authored_catalog_selection_never_changes_external_names() {
        for name in [
            "Mi proyecto 🎬",
            "Safari",
            "Cuadrícula personal",
            "照片",
            "",
        ] {
            assert_eq!(translate(Language::English, name), name);
            assert_eq!(translate(Language::Spanish, name), name);
        }
        assert_eq!(
            translate(
                Language::English,
                "Escritorios no disponibles en esta versión de macOS"
            ),
            "Desktops are unavailable on this version of macOS"
        );
    }
}
