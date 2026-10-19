//! Control: the same `unsafe` entry written directly in a `forbid(unsafe_code)` crate is rejected by
//! the `unsafe_code` lint (only expansions of an external macro are exempt).

#![forbid(unsafe_code)]
#![deny(warnings)]

/// Exported entry point.
///
/// # Safety
/// `host` points at a readable byte.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn umbra_plugin_entry(host: *const u8) -> *const u8 {
    unsafe { entry_macro::entry(host) }
}
