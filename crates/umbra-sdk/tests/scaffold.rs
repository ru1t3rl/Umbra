//! Scaffold contract (M4 WI-1, REQ-A-3): the SDK re-exports the ABI crate it is built on as
//! `umbra_sdk::abi`, which `export_plugin!` relies on (WI-7), so a plugin crate needs no dependency
//! of its own on `umbra-plugin-api` (REQ-SDK-3).

#![forbid(unsafe_code)]

/// `umbra_sdk::abi` resolves (this file does not compile otherwise).
#[test]
fn abi_is_reexported() {
    #[expect(unused_imports, reason = "the import itself is the check")]
    use umbra_sdk::abi;
}
