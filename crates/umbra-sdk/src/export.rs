//! `export_plugin!` (REQ-P-1, REQ-SDK-1). M4 step 4 stub; WI-7 fills in the machinery.

/// Support items the macro expands to; not public API.
#[doc(hidden)]
pub mod __private {
    use crate::abi::{UmbraHostApi, UmbraPluginDescriptor};

    /// The body of the generated `umbra_plugin_entry`.
    #[expect(clippy::todo, reason = "M4 WI-7 stub")]
    #[must_use]
    pub fn entry(_host: *const UmbraHostApi) -> *const UmbraPluginDescriptor {
        todo!("M4 WI-7")
    }
}

/// Generates exactly one item, `#[unsafe(no_mangle)] pub extern "C" fn umbra_plugin_entry`, with a
/// safe body and no lint attribute (ASM-116).
#[macro_export]
macro_rules! export_plugin {
    (
        id: $id:literal, name: $name:literal, version: $version:literal,
        behaviors: [$($factory:ty),* $(,)?]
        $(, init: $init:path)? $(, shutdown: $shutdown:path)? $(,)?
    ) => {
        // Stub only: keeps the factories used until WI-7 builds the descriptor from them.
        const _: fn() = || {
            $(let _ = <$factory as $crate::BehaviorFactory>::ID;)*
        };

        /// The plugin entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn umbra_plugin_entry(
            host: *const $crate::abi::UmbraHostApi,
        ) -> *const $crate::abi::UmbraPluginDescriptor {
            $crate::__private::entry(host)
        }
    };
}
