//! Stands in for `umbra-sdk` after the WI-7 review (human decision 2026-10-19, ASM-116): an external
//! `macro_rules!` whose expansion is exactly one `#[unsafe(no_mangle)] pub unsafe extern "C" fn`
//! whose body is an `unsafe` block calling an `unsafe fn` of this crate, and no lint attribute (the
//! `export_plugin!` shape).
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

/// Emits the exported entry function.
#[macro_export]
macro_rules! export_entry {
    () => {
        /// Exported entry point.
        ///
        /// # Safety
        /// `host` points at a readable byte.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn umbra_plugin_entry(host: *const u8) -> *const u8 {
            unsafe { $crate::entry(host) }
        }
    };
}
