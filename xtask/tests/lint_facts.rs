//! Lint facts the plugin design relies on (M4 WI-2, R-M5, ASM-116, NFR-2).
//!
//! `export_plugin!` (umbra-sdk) expands to one `#[unsafe(no_mangle)] pub unsafe extern "C" fn` whose
//! body is an `unsafe` block, and no lint attribute, so plugin crates keep `#![forbid(unsafe_code)]`
//! (amended 2026-10-19, human decision after the WI-7 review: the entry is an `unsafe fn`). These
//! tests build fixture workspaces (`tests/fixtures/forbid_*`, see the README there) with the pinned
//! toolchain and `-D warnings`, offline: if a future rustc starts to fire `unsafe_code` for such an
//! expansion, a test here fails before any plugin crate breaks.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml")
}

/// `cargo build --offline --locked` of a fixture with `-D warnings`, in its own target directory
/// under `CARGO_TARGET_TMPDIR` (cargo hashes path packages relative to their workspace, so the two
/// fixtures must not share one, or the second build may be reported fresh without compiling).
#[expect(
    clippy::expect_used,
    reason = "test helper: failing to spawn cargo is a test error"
)]
fn build_fixture(name: &str) -> Output {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("lint-facts")
        .join(name);
    Command::new(env!("CARGO"))
        .args(["build", "--offline", "--locked", "--manifest-path"])
        .arg(fixture(name))
        .arg("--target-dir")
        .arg(target)
        .env("RUSTFLAGS", "-D warnings")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("spawn cargo")
}

/// ASM-116: `#![forbid(unsafe_code)]` plus an external `macro_rules!` that emits only
/// `#[unsafe(no_mangle)] pub extern "C" fn` with a safe body compiles cleanly.
#[test]
fn forbid_plus_external_no_mangle_macro_compiles() {
    let out = build_fixture("forbid_external_no_mangle");

    assert!(
        out.status.success(),
        "fixture failed to build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ASM-116: the same macro also emitting `#[expect(unsafe_code)]` is error E0453 under `forbid`,
/// so `export_plugin!` must not emit a lint attribute.
#[test]
fn macro_emitting_expect_unsafe_code_is_e0453_under_forbid() {
    let out = build_fixture("forbid_external_expect");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "fixture unexpectedly built");
    assert!(
        stderr.contains("error[E0453]"),
        "expected E0453, got:\n{stderr}"
    );
}

/// ASM-116 (amended after the WI-7 review): `#![forbid(unsafe_code)]` plus `#![deny(warnings)]` and an
/// external `macro_rules!` that emits `#[unsafe(no_mangle)] pub unsafe extern "C" fn` with an `unsafe`
/// block calling the macro crate's `unsafe fn` compiles cleanly (the `export_plugin!` shape).
#[test]
fn forbid_plus_external_unsafe_entry_macro_compiles() {
    let out = build_fixture("forbid_external_unsafe_entry");

    assert!(
        out.status.success(),
        "fixture failed to build:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Control for the test above: the same entry written directly in the `forbid(unsafe_code)` crate is
/// rejected by `unsafe_code` (the `unsafe fn`, the `unsafe` block and the `no_mangle` attribute), so
/// the exemption really comes from the external macro.
#[test]
fn direct_unsafe_entry_is_rejected_under_forbid() {
    let out = build_fixture("forbid_direct_unsafe_entry");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "fixture unexpectedly built");
    for expected in [
        "declaration of an `unsafe` function",
        "usage of an `unsafe` block",
        "usage of the unsafe `no_mangle` attribute",
    ] {
        assert!(
            stderr.contains(expected),
            "expected {expected:?}, got:\n{stderr}"
        );
    }
}
