//! On-demand native desktop swipes; no Mission Control overview or idle polling.
//! The protocol is adapted from joshuarli/iss (0BSD) and rileycx/strafe (MIT).
use crate::{accessibility as ax, spaces};
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSString};
use std::{
    ptr,
    sync::OnceLock,
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn mach_absolute_time() -> u64;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGEventCreate(source: ax::Cf) -> ax::Cf;
    fn CGEventSetIntegerValueField(event: ax::Cf, field: u32, value: i64);
    fn CGEventSetDoubleValueField(event: ax::Cf, field: u32, value: f64);
    fn CGEventSetLocation(event: ax::Cf, location: NSPoint);
    fn CGEventGetTimestamp(event: ax::Cf) -> u64;
    fn CGEventCreateData(allocator: ax::Cf, event: ax::Cf) -> ax::Cf;
    fn CGEventCreateFromData(allocator: ax::Cf, data: ax::Cf) -> ax::Cf;
    fn CGEventPost(tap: u32, event: ax::Cf);
    fn CGDisplayBounds(display: u32) -> NSRect;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataGetLength(data: ax::Cf) -> isize;
    fn CFDataGetBytePtr(data: ax::Cf) -> *const u8;
    fn CFDataCreate(allocator: ax::Cf, bytes: *const u8, length: isize) -> ax::Cf;
}

fn modern_protocol() -> bool {
    static MODERN: OnceLock<bool> = OnceLock::new();
    *MODERN.get_or_init(|| unsafe {
        let mut version = [0u8; 64];
        let mut size = version.len();
        if libc::sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            version.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        ) != 0
        {
            return false;
        }
        std::str::from_utf8(&version)
            .ok()
            .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
            .is_some_and(|v| v >= 27)
    })
}
fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn payload(timestamp: u64, phase: u32, progress: f64, velocity: f64) -> Vec<u8> {
    // Explicit byte offsets avoid packed-struct alignment/ABI dependencies.
    let ended = phase == 4;
    let mut bytes = vec![0; if ended { 96 } else { 68 }];
    bytes[..8].copy_from_slice(&timestamp.to_le_bytes());
    put32(&mut bytes, 24, if ended { 2 } else { 1 });
    put32(&mut bytes, 28, 40); // fluid gesture size
    put32(&mut bytes, 32, 23); // fluid gesture type
    put32(&mut bytes, 36, phase << 24);
    put32(&mut bytes, 44, (0.1f64 * 65536.) as i32 as u32);
    bytes[60..62].copy_from_slice(&1u16.to_le_bytes()); // horizontal
    bytes[62..64].copy_from_slice(&3u16.to_le_bytes()); // Dock primary
    put32(&mut bytes, 64, (progress * 65536.) as i32 as u32);
    if ended {
        put32(&mut bytes, 68, 28); // velocity size
        put32(&mut bytes, 72, 9); // velocity type
        bytes[80] = 1; // child depth
        put32(&mut bytes, 84, (velocity * 65536.) as i32 as u32);
    }
    bytes
}
fn augment(event: ax::Cf, phase: u32, progress: f64, velocity: f64) -> Option<ax::Owned> {
    unsafe {
        let data = ax::Owned::new(CGEventCreateData(ptr::null(), event))?;
        let length = CFDataGetLength(data.0);
        if !(4..=65536).contains(&length) {
            return None;
        }
        let bytes = std::slice::from_raw_parts(CFDataGetBytePtr(data.0), length as usize);
        if bytes[..4] != [0, 0, 0, 2] {
            return None;
        }
        let timestamp = CGEventGetTimestamp(event);
        let payload = payload(
            if timestamp == 0 {
                mach_absolute_time()
            } else {
                timestamp
            },
            phase,
            progress,
            velocity,
        );
        let mut serialized = bytes.to_vec();
        serialized.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        serialized.extend_from_slice(&4205u16.to_be_bytes());
        serialized.extend_from_slice(&payload);
        let data = ax::Owned::new(CFDataCreate(
            ptr::null(),
            serialized.as_ptr(),
            serialized.len() as isize,
        ))?;
        ax::Owned::new(CGEventCreateFromData(ptr::null(), data.0))
    }
}
fn swipe(right: bool, display: u32) -> Option<()> {
    let modern = modern_protocol();
    trace(&format!("modern={modern} post_access={}", unsafe {
        CGPreflightPostEventAccess()
    }));
    let natural = unsafe {
        let defaults: crate::native::Obj = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let value: crate::native::Obj = msg_send![defaults, objectForKey:&*NSString::from_str("com.apple.swipescrolldirection")];
        value.is_null() || msg_send![value, boolValue]
    };
    let sign = posted_sign(right, modern, natural);
    let mut events = Vec::with_capacity(6);
    unsafe {
        let bounds = CGDisplayBounds(display);
        let point = NSPoint::new(
            bounds.origin.x + bounds.size.width / 2.,
            bounds.origin.y + bounds.size.height / 2.,
        );
        // Construct the complete gesture first, so allocation failure cannot
        // leave the Dock in a partially opened gesture.
        for phase in [1, 2, 4] {
            let event = ax::Owned::new(CGEventCreate(ptr::null()))?;
            CGEventSetLocation(event.0, point);
            for (field, value) in [(55, 30), (110, 23), (123, 1), (132, phase as i64)] {
                CGEventSetIntegerValueField(event.0, field, value);
            }
            let event = if modern {
                let progress = sign * 1e-4;
                let velocity = if phase == 4 { sign * 2000. } else { 0. };
                CGEventSetIntegerValueField(event.0, 134, phase as i64);
                for (field, value) in [(124, progress), (125, 0.1), (129, velocity), (138, 3.)] {
                    CGEventSetDoubleValueField(event.0, field, value);
                }
                augment(event.0, phase, progress, velocity)?
            } else {
                let tiny = f32::from_bits(1);
                let flag = if right { tiny } else { -tiny };
                CGEventSetIntegerValueField(event.0, 135, flag.to_bits() as i32 as i64);
                CGEventSetDoubleValueField(event.0, 119, 0.);
                CGEventSetDoubleValueField(event.0, 139, f64::from(tiny));
                CGEventSetDoubleValueField(event.0, 129, if phase == 4 { sign * 400. } else { 0. });
                event
            };
            let companion = ax::Owned::new(CGEventCreate(ptr::null()))?;
            CGEventSetLocation(companion.0, point);
            CGEventSetIntegerValueField(companion.0, 55, 29);
            events.push(event);
            events.push(companion);
        }
        for event in events {
            CGEventPost(1, event.0);
        }
    }
    Some(())
}
fn posted_sign(right: bool, modern: bool, natural: bool) -> f64 {
    let sign = if right { 1. } else { -1. };
    if modern && natural { -sign } else { sign }
}

pub enum Progress {
    Waiting,
    Done,
}
pub struct Switch {
    target: u64,
    display: u32,
    started: Instant,
    flight: Option<(u64, u64, Instant)>,
    settled: Option<Instant>,
    hops: usize,
}
impl Switch {
    pub fn retarget(&mut self, target: u64) -> Result<(), &'static str> {
        let destination = spaces::destination(target)
            .ok_or(crate::i18n::tr("Ese escritorio ya no está disponible."))?;
        if destination.display != self.display {
            return Err(crate::i18n::tr(
                "Espera a que termine el cambio de pantalla.",
            ));
        }
        self.target = target;
        self.started = Instant::now();
        self.hops = 0;
        Ok(())
    }
    pub fn start(target: u64) -> Result<Self, &'static str> {
        if !ax::trusted() {
            return Err(crate::i18n::tr(
                "Concede Accesibilidad para cambiar de escritorio.",
            ));
        }
        let destination = spaces::destination(target)
            .ok_or(crate::i18n::tr("Ese escritorio ya no está disponible."))?;
        Ok(Self {
            target,
            display: destination.display,
            started: Instant::now(),
            flight: None,
            settled: None,
            hops: 0,
        })
    }
    pub fn step(&mut self) -> Result<Progress, &'static str> {
        let destination = spaces::destination(self.target).ok_or(crate::i18n::tr(
            "El escritorio seleccionado ya no está disponible.",
        ))?;
        if destination.display != self.display {
            return Err(crate::i18n::tr("Ha cambiado la pantalla del escritorio."));
        }
        if self.started.elapsed() > Duration::from_secs(8) {
            return Err(crate::i18n::tr(
                "macOS no confirmó el cambio de escritorio.",
            ));
        }
        if let Some((source, expected, sent)) = self.flight {
            if destination.current == source {
                if sent.elapsed() > Duration::from_secs(2) {
                    return Err(crate::i18n::tr(
                        "macOS no confirmó el cambio de escritorio.",
                    ));
                }
                return Ok(Progress::Waiting);
            }
            if destination.current != expected {
                return Err(crate::i18n::tr(
                    "Cambio cancelado: se ha seleccionado otro escritorio.",
                ));
            }
            self.flight = None;
            self.settled = Some(Instant::now());
        }
        if self
            .settled
            .is_some_and(|t| t.elapsed() < Duration::from_millis(120))
        {
            return Ok(Progress::Waiting);
        }
        if destination.current == self.target {
            trace(&format!("confirmed={}", self.target));
            return Ok(Progress::Done);
        }
        let current = spaces::destination(destination.current).ok_or(crate::i18n::tr(
            "No se pudo localizar el escritorio actual.",
        ))?;
        let right = current.position < destination.position;
        let expected = spaces::neighbor(destination.current, right).ok_or(crate::i18n::tr(
            "No se pudo localizar el siguiente escritorio.",
        ))?;
        if self.hops >= destination.count {
            return Err(crate::i18n::tr("Ha cambiado el orden de los escritorios."));
        }
        swipe(right, self.display).ok_or(crate::i18n::tr(
            "Esta versión de macOS no admite el cambio directo.",
        ))?;
        self.hops += 1;
        self.flight = Some((destination.current, expected, Instant::now()));
        trace(&format!(
            "swipe={} -> {} target={}",
            destination.current, expected, self.target
        ));
        Ok(Progress::Waiting)
    }
}
fn trace(message: &str) {
    if std::env::var_os("AMSPACE_TRACE_SPACES").is_some() {
        eprintln!("spaces: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_switch_respects_both_natural_scrolling_preferences() {
        for right in [true, false] {
            let expected = if right { 1. } else { -1. };
            assert_eq!(posted_sign(right, true, false), expected);
            assert_eq!(posted_sign(right, true, true), -expected);
            assert_eq!(posted_sign(right, false, false), expected);
            assert_eq!(posted_sign(right, false, true), expected);
        }
    }
    #[test]
    fn gesture_payload_has_matching_lengths_phases_and_signed_velocity() {
        for phase in [1u32, 2, 4] {
            for sign in [-1., 1.] {
                let bytes = payload(42, phase, sign, sign * 9999.);
                let number =
                    |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                assert_eq!(bytes.len(), if phase == 4 { 96 } else { 68 });
                assert_eq!(number(24), if phase == 4 { 2 } else { 1 });
                assert_eq!(number(36), phase << 24);
                assert_eq!(number(64) as i32, (sign * 65536.) as i32);
                if phase == 4 {
                    assert_eq!(number(84) as i32, (sign * 9999. * 65536.) as i32);
                }
            }
        }
    }
}
