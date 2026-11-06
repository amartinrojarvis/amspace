//! Native attributed status text: semantic colors and SF Symbols, no bitmap loop.
use crate::{
    native::{Obj, rect},
    spaces::Snapshot,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
};
use objc2_foundation::NSString;
pub const ICONS: &[(&str, &str)] = &[
    ("Número", ""),
    ("Trabajo", "briefcase"),
    ("Código", "chevron.left.forwardslash.chevron.right"),
    ("Web", "globe"),
    ("Mensajes", "bubble.left.and.bubble.right"),
    ("Música", "music.note"),
    ("Diseño", "paintbrush.pointed"),
    ("Casa", "house"),
    ("Juegos", "gamecontroller"),
    ("Lectura", "book"),
    ("Estrella", "star"),
    ("Terminal", "terminal"),
    ("Correo", "envelope"),
    ("Edición de vídeo", "film"),
    ("Edición de foto", "slider.horizontal.3"),
    ("Fotografía", "camera"),
    ("Comunicaciones", "antenna.radiowaves.left.and.right"),
    ("Videollamadas", "video"),
    ("Equipo", "person.2"),
    ("Audio", "waveform"),
    ("Calendario", "calendar"),
    ("Tareas", "checklist"),
    ("Escritura", "pencil.and.outline"),
    ("Documentos", "doc.text"),
    ("Archivos", "folder"),
    ("Nube", "cloud"),
    ("Analítica", "chart.bar"),
    ("Creatividad", "lightbulb"),
    ("Ilustración", "paintpalette"),
    ("Herramientas", "wrench.and.screwdriver"),
    ("Redes", "network"),
];
pub fn normalize_mode(value: isize) -> isize {
    if (0..=2).contains(&value) { value } else { 1 }
}
pub fn normalize_icon(value: isize) -> usize {
    if (0..ICONS.len() as isize).contains(&value) {
        value as usize
    } else {
        0
    }
}
pub fn mode(defaults: Obj) -> isize {
    unsafe {
        let key = NSString::from_str("spaceStyle");
        let saved: Obj = msg_send![defaults, objectForKey:&*key];
        if saved.is_null() {
            1
        } else {
            normalize_mode(msg_send![defaults, integerForKey:&*key])
        }
    }
}
pub fn icon(defaults: Obj, id: u64, index: usize) -> usize {
    unsafe {
        let key = NSString::from_str(&format!("spaceIcon.{id}"));
        let saved: Obj = msg_send![defaults, objectForKey:&*key];
        if saved.is_null() {
            index % (ICONS.len() - 1) + 1
        } else {
            normalize_icon(msg_send![defaults, integerForKey:&*key])
        }
    }
}
pub fn render(snapshot: &Snapshot, defaults: Obj) -> Retained<AnyObject> {
    render_selection(snapshot, defaults, None)
}
pub fn render_desktop(snapshot: &Snapshot, defaults: Obj, index: usize) -> Retained<AnyObject> {
    render_selection(snapshot, defaults, Some(index))
}
fn render_selection(
    snapshot: &Snapshot,
    defaults: Obj,
    only: Option<usize>,
) -> Retained<AnyObject> {
    unsafe {
        let result: Allocated<AnyObject> = msg_send![class!(NSMutableAttributedString), alloc];
        let result: Retained<AnyObject> =
            msg_send![result, initWithString:&*NSString::from_str("")];
        let style = mode(defaults);
        let font: Obj =
            msg_send![class!(NSFont), monospacedDigitSystemFontOfSize:13f64, weight:0.23f64];
        if only.is_none() && (snapshot.active.is_none() || snapshot.desktops.len() > 9) {
            let part: Allocated<AnyObject> = msg_send![class!(NSAttributedString), alloc];
            let part: Retained<AnyObject> =
                msg_send![part, initWithString:&*NSString::from_str(&snapshot.title)];
            let _: () = msg_send![&result, appendAttributedString:&*part];
            return result;
        }
        for (index, id) in snapshot.desktops.iter().enumerate() {
            if only.is_some_and(|selected| selected != index) {
                continue;
            }
            let active = snapshot.active == Some(index);
            let base: Obj = msg_send![class!(NSColor), labelColor];
            let color: Obj =
                msg_send![base, colorWithAlphaComponent:if active {1f64} else {0.38f64}];
            let attrs: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
            let _: () = msg_send![&attrs, setObject:color, forKey:&*NSString::from_str("NSColor")];
            let _: () = msg_send![&attrs, setObject:font, forKey:&*NSString::from_str("NSFont")];
            if only.is_none() && index > 0 {
                let gap: Allocated<AnyObject> = msg_send![class!(NSAttributedString), alloc];
                let gap: Retained<AnyObject> =
                    msg_send![gap, initWithString:&*NSString::from_str("  "), attributes:&*attrs];
                let _: () = msg_send![&result, appendAttributedString:&*gap];
            }
            let choice = if style == 2 {
                icon(defaults, *id, index)
            } else {
                0
            };
            let mut image: Obj = std::ptr::null_mut();
            if choice != 0 {
                image = msg_send![class!(NSImage), imageWithSystemSymbolName:&*NSString::from_str(ICONS[choice].1), accessibilityDescription:&*NSString::from_str(ICONS[choice].0)];
            }
            if image.is_null() {
                let title = if style == 0 && !active {
                    "·".into()
                } else {
                    (index + 1).to_string()
                };
                let part: Allocated<AnyObject> = msg_send![class!(NSAttributedString), alloc];
                let part: Retained<AnyObject> = msg_send![part, initWithString:&*NSString::from_str(&title), attributes:&*attrs];
                let _: () = msg_send![&result, appendAttributedString:&*part];
            } else {
                let config: Obj = msg_send![class!(NSImageSymbolConfiguration), configurationWithHierarchicalColor:color];
                let image: Obj = msg_send![image, imageWithSymbolConfiguration:config];
                let attachment: Retained<AnyObject> = msg_send![class!(NSTextAttachment), new];
                let _: () = msg_send![&attachment, setImage:image];
                let _: () = msg_send![&attachment, setBounds:rect(0., -2., 15., 15.)];
                let part: Obj = msg_send![class!(NSAttributedString), attributedStringWithAttachment:&*attachment];
                let _: () = msg_send![&result, appendAttributedString:part];
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_preferences_fall_back_to_readable_numbers() {
        for value in [-1, 3, isize::MAX] {
            assert_eq!(normalize_mode(value), 1);
        }
        for value in [-1, ICONS.len() as isize, isize::MAX] {
            assert_eq!(normalize_icon(value), 0);
        }
        assert_eq!(normalize_mode(2), 2);
        assert_eq!(normalize_icon(1), 1);
    }
}
