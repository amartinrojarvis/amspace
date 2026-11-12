//! Kairos Monitor's soft palette, applied once to the native status button layer.
use crate::native::{Obj, color};
use objc2::msg_send;

pub const OPTIONS: [(&str, Option<[u8; 3]>); 7] = [
    ("Sin color", None),
    ("Azul", Some([0x86, 0xAC, 0xCF])),
    ("Salvia", Some([0x95, 0xB4, 0x9B])),
    ("Lavanda", Some([0xAD, 0xA0, 0xC9])),
    ("Arena", Some([0xC5, 0xB1, 0x8B])),
    ("Rosa", Some([0xC7, 0x9E, 0xA8])),
    ("Gris", Some([0xA7, 0xAD, 0xB5])),
];

pub fn normalize(index: isize) -> usize {
    usize::try_from(index)
        .ok()
        .filter(|i| *i < OPTIONS.len())
        .unwrap_or(0)
}

pub unsafe fn apply(button: Obj, index: usize) {
    unsafe {
        let _: () = msg_send![button, setWantsLayer:true];
        let layer: Obj = msg_send![button, layer];
        let cg: *const crate::native::CgColor = OPTIONS
            .get(index)
            .and_then(|(_, rgb)| *rgb)
            .map_or(std::ptr::null(), |rgb| {
                let c = color(
                    rgb[0] as f64 / 255.,
                    rgb[1] as f64 / 255.,
                    rgb[2] as f64 / 255.,
                    0.24,
                );
                msg_send![c, CGColor]
            });
        let _: () = msg_send![layer, setBackgroundColor:cg];
        let _: () = msg_send![layer, setCornerRadius:6f64];
        // Keep AppKit's native highlight, hit testing and accessibility intact.
        let _: () = msg_send![button, setNeedsDisplay:true];
    }
}
