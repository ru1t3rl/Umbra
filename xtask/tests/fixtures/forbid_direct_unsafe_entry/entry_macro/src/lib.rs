//! Control fixture (WI-7 review): the plugin crate does not use this macro crate's macro; it writes
//! the `unsafe` entry itself. Kept so the workspace has the same shape as the other fixtures.
#![expect(unsafe_code, reason = "stands in for umbra-sdk, an FFI crate")]

/// Reads the first byte at `host`.
///
/// # Safety
/// `host` points at a readable byte.
pub unsafe fn entry(host: *const u8) -> *const u8 {
    // SAFETY: forwarded from this function's contract.
    let _first = unsafe { host.read() };
    host
}
