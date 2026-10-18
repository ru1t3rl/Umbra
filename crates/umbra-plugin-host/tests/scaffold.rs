//! Scaffold contract (M4 WI-1, section 4): the host crate builds and links with its two allowed
//! workspace dependencies. Replaced by the loader tests of WI-8.

#![forbid(unsafe_code)]

/// The crate links into a test binary.
#[test]
fn host_crate_links() {
    #[expect(unused_imports, reason = "the import itself is the check")]
    use umbra_plugin_host;
}
