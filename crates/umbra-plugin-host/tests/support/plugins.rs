//! WI-10 build helper stub (R-M8): builds a variant of `umbra-test-plugin` and copies the library to
//! `<CARGO_TARGET_TMPDIR>/plugin-packages/<variant>/` under a unique file name.
//!
//! The plan names the helper's behavior but not its signature (plan gap, reported); the signature
//! below is the test-side assumption and WI-10 may change it together with the call sites. Features are
//! the fixture's cargo features (`no_entry`, `init_fails`, `init_panics`, `shutdown_panics`,
//! `panic_abort_profile`); the empty list is the default build with one capability per fault mode.
#![expect(clippy::todo, reason = "M4 WI-10 stub")]

use std::path::PathBuf;

/// The plugin directory holding one package `<variant>/` with the built library and a `plugin.toml`
/// matching the fixture (`id = "test.plugin.fixture"`).
pub fn package_dir(_variant: &str, _features: &[&str]) -> PathBuf {
    todo!("M4 WI-10: build umbra-test-plugin with --features, copy to a unique package directory")
}
