//! Counter-fixture: the same macro, but its expansion also carries `#[expect(unsafe_code)]`. Under the
//! user crate's `#![forbid(unsafe_code)]` this is error E0453, which is why `export_plugin!` emits no
//! lint attribute (R-M5, ASM-116).

/// Emits the exported entry function with a lint attribute.
#[macro_export]
macro_rules! export_entry {
    () => {
        /// Exported entry point.
        #[expect(unsafe_code, reason = "no_mangle is an unsafe attribute")]
        #[unsafe(no_mangle)]
        pub extern "C" fn umbra_plugin_entry(host: *const u8) -> *const u8 {
            host
        }
    };
}
