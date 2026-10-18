//! Stands in for `umbra-sdk`: an external `macro_rules!` whose expansion is exactly one
//! `#[unsafe(no_mangle)]` item with a safe body and no lint attribute (the `export_plugin!` shape).

/// Emits the exported entry function.
#[macro_export]
macro_rules! export_entry {
    () => {
        /// Exported entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn umbra_plugin_entry(host: *const u8) -> *const u8 {
            host
        }
    };
}
