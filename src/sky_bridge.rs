//! Optional WindowManager bridge entry, resolved from our loaded SkyLight image.
//! Symbol lookup follows the Mach-O approach used by yabai (MIT; YABAI-LICENSE).
//! No code scanning, patching, injection, or process-external memory access.
use crate::native::Obj;
use std::{
    ffi::{CStr, c_void},
    ops::Range,
    sync::OnceLock,
};
pub type AsyncOperation = unsafe extern "C" fn(Obj) -> i64;
const NAME:&[u8]=b"__ZL54SLSPerformAsynchronousBridgedWindowManagementOperationP47SLSAsynchronousBridgedWindowManagementOperation";
#[repr(C)]
struct Header {
    magic: u32,
    cpu: i32,
    subtype: i32,
    kind: u32,
    commands: u32,
    bytes: u32,
    flags: u32,
    reserved: u32,
}
unsafe extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(index: u32) -> *const std::ffi::c_char;
    fn _dyld_get_image_header(index: u32) -> *const Header;
    fn _dyld_get_image_vmaddr_slide(index: u32) -> isize;
}
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(at..at.checked_add(8)?)?.try_into().ok()?,
    ))
}
#[derive(Default)]
struct Layout {
    link: Option<(u64, u64, u64)>,
    symbols: Option<(u32, u32, u32, u32)>,
    executable: Vec<Range<u64>>,
}
fn layout(commands: &[u8], count: u32) -> Option<Layout> {
    if count > 2048 || commands.len() > 1024 * 1024 {
        return None;
    }
    let mut result = Layout::default();
    let mut offset = 0usize;
    for _ in 0..count {
        let command = u32_at(commands, offset)?;
        let size = u32_at(commands, offset + 4)? as usize;
        if size < 8 || size % 8 != 0 {
            return None;
        }
        let bytes = commands.get(offset..offset.checked_add(size)?)?;
        match command {
            0x19 => {
                if size < 72 {
                    return None;
                }
                let vm = u64_at(bytes, 24)?;
                let span = u64_at(bytes, 32)?;
                if u32_at(bytes, 60)? & 4 != 0 {
                    result.executable.push(vm..vm.checked_add(span)?);
                }
                if bytes.get(8..24)? == b"__LINKEDIT\0\0\0\0\0\0" {
                    result.link = Some((vm, u64_at(bytes, 40)?, u64_at(bytes, 48)?));
                }
            }
            2 => {
                if size < 24 {
                    return None;
                }
                result.symbols = Some((
                    u32_at(bytes, 8)?,
                    u32_at(bytes, 12)?,
                    u32_at(bytes, 16)?,
                    u32_at(bytes, 20)?,
                ));
            }
            _ => {}
        }
        offset = offset.checked_add(size)?;
    }
    Some(result)
}
fn slide_address(value: u64, slide: isize) -> Option<usize> {
    usize::try_from(i128::from(value) + slide as i128).ok()
}
fn file_range(
    offset: u32,
    length: usize,
    file_offset: u64,
    file_size: u64,
) -> Option<Range<usize>> {
    let start = u64::from(offset).checked_sub(file_offset)?;
    let end = start.checked_add(length as u64)?;
    if end > file_size {
        return None;
    }
    Some(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
}
fn symbol_value(
    table: &[u8],
    strings: &[u8],
    name: &[u8],
    executable: &[Range<u64>],
) -> Option<u64> {
    for entry in table.chunks_exact(16) {
        if entry[4] & 0xe0 != 0 || entry[4] & 0x0e != 0x0e {
            continue;
        }
        let index = u32_at(entry, 0)? as usize;
        let Some(tail) = strings.get(index..) else {
            continue;
        };
        if tail.get(..name.len()) != Some(name) || tail.get(name.len()) != Some(&0) {
            continue;
        }
        let value = u64_at(entry, 8)?;
        if executable.iter().any(|range| range.contains(&value)) {
            return Some(value);
        }
    }
    None
}
pub fn asynchronous_operation() -> Option<AsyncOperation> {
    static OPERATION: OnceLock<Option<AsyncOperation>> = OnceLock::new();
    *OPERATION.get_or_init(|| unsafe {
        for index in 0.._dyld_image_count() {
            let name = _dyld_get_image_name(index);
            if name.is_null() {
                continue;
            }
            let name = CStr::from_ptr(name).to_bytes();
            if !name.ends_with(b"/SkyLight.framework/Versions/A/SkyLight")
                && !name.ends_with(b"/SkyLight.framework/SkyLight")
            {
                continue;
            }
            let header = _dyld_get_image_header(index);
            if header.is_null() {
                return None;
            }
            let header_ref = &*header;
            if header_ref.magic != 0xfeedfacf || header_ref.bytes > 1024 * 1024 {
                return None;
            }
            // The dyld header and load commands are trusted mapped image data;
            // all subsequent file offsets are checked against __LINKEDIT.
            let commands =
                std::slice::from_raw_parts(header.add(1).cast::<u8>(), header_ref.bytes as usize);
            let layout = layout(commands, header_ref.commands)?;
            let (vm, file_offset, file_size) = layout.link?;
            let (symbol_offset, count, string_offset, string_size) = layout.symbols?;
            // dyld may share a large string table across the shared cache.
            // file_range below validates its actual mapped __LINKEDIT extent;
            // symbol comparison only reads NAME.len() + 1 bytes per entry.
            if count > 2_000_000 {
                return None;
            }
            let table = file_range(
                symbol_offset,
                (count as usize).checked_mul(16)?,
                file_offset,
                file_size,
            )?;
            let strings = file_range(string_offset, string_size as usize, file_offset, file_size)?;
            let slide = _dyld_get_image_vmaddr_slide(index);
            let base = slide_address(vm, slide)?;
            let table_address = base.checked_add(table.start)?;
            let string_address = base.checked_add(strings.start)?;
            let table = std::slice::from_raw_parts(table_address as *const u8, table.len());
            let strings = std::slice::from_raw_parts(string_address as *const u8, strings.len());
            let value = symbol_value(table, strings, NAME, &layout.executable)?;
            let address = slide_address(value, slide)?;
            return Some(std::mem::transmute::<*mut c_void, AsyncOperation>(
                address as *mut c_void,
            ));
        }
        None
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_load_commands_and_file_offsets_fail_closed() {
        assert!(layout(&[0; 7], 1).is_none());
        assert!(layout(&[0; 8], 1).is_none());
        let mut bytes = [0u8; 8];
        bytes[4..8].copy_from_slice(&16u32.to_le_bytes());
        assert!(layout(&bytes, 1).is_none());
        assert!(file_range(90, 16, 100, 100).is_none());
        assert!(file_range(190, 16, 100, 100).is_none());
        assert_eq!(file_range(110, 16, 100, 100), Some(10..26));
        assert_eq!(slide_address(1, -2), None);
    }
    #[test]
    fn exact_symbol_name_must_resolve_into_an_executable_section() {
        let mut entry = [0u8; 16];
        entry[4] = 0x0e;
        entry[8..16].copy_from_slice(&150u64.to_le_bytes());
        let strings = b"wanted\0other\0";
        assert_eq!(
            symbol_value(
                &entry,
                strings,
                b"wanted",
                std::slice::from_ref(&(100..200))
            ),
            Some(150)
        );
        assert_eq!(
            symbol_value(&entry, strings, b"want", std::slice::from_ref(&(100..200))),
            None
        );
        assert_eq!(
            symbol_value(
                &entry,
                strings,
                b"wanted",
                std::slice::from_ref(&(200..300))
            ),
            None
        );
        entry[..4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(
            symbol_value(
                &entry,
                strings,
                b"wanted",
                std::slice::from_ref(&(100..200))
            ),
            None
        );
    }
}
