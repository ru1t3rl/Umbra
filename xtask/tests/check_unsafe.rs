//! Tests for the `#![forbid(unsafe_code)]` check (ASM-024, CLAUDE.md rule 5, NFR-2).
//!
//! Pure tests use synthetic [`CrateRoot`] values; fixture tests load
//! `tests/fixtures/{violating,conforming}`.

use std::path::{Path, PathBuf};

use xtask::deps::{CrateRoot, Violation, check_forbid_unsafe, load_crate_roots};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml")
}

fn root(package: &str, source: &str) -> CrateRoot {
    CrateRoot {
        package: package.to_string(),
        file: "lib.rs".to_string(),
        source: source.to_string(),
    }
}

fn missing(package: &str) -> Violation {
    Violation::MissingForbidUnsafe {
        package: package.to_string(),
        file: "lib.rs".to_string(),
    }
}

/// ASM-024: a non-FFI crate with the attribute conforms.
#[test]
fn given_core_with_forbid_when_checked_then_no_violation() {
    let r = root(
        "cgf-core",
        "//! docs\n\n#![forbid(unsafe_code)]\n\npub mod a;\n",
    );

    assert_eq!(check_forbid_unsafe(&[r]), Vec::<Violation>::new());
}

/// ASM-024: a non-FFI crate without the attribute is reported.
#[test]
fn given_core_without_forbid_when_checked_then_violation() {
    let r = root("cgf-core", "//! docs\npub fn f() {}\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("cgf-core")]);
}

/// ASM-024: the three FFI crates are exempt.
#[test]
fn given_ffi_crates_without_forbid_when_checked_then_no_violation() {
    let roots = [
        root("cgf-plugin-api", ""),
        root("cgf-plugin-host", ""),
        root("cgf-sdk", ""),
    ];

    assert_eq!(check_forbid_unsafe(&roots), Vec::<Violation>::new());
}

/// ASM-024: `deny` is not `forbid`.
#[test]
fn given_deny_instead_of_forbid_when_checked_then_violation() {
    let r = root("cgf-kernel", "#![deny(unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("cgf-kernel")]);
}

/// ASM-024: a commented-out attribute does not count.
#[test]
fn given_commented_out_forbid_when_checked_then_violation() {
    let r = root("cgf-kernel", "// #![forbid(unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("cgf-kernel")]);
}

/// ASM-024: an outer attribute (`#[forbid(...)]` on an item) does not count.
#[test]
fn given_outer_attribute_only_when_checked_then_violation() {
    let r = root("cgf-kernel", "#[forbid(unsafe_code)]\nmod m {}\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("cgf-kernel")]);
}

/// ASM-024: the attribute may sit next to other lints in one list.
#[test]
fn given_forbid_in_a_lint_list_when_checked_then_no_violation() {
    let r = root("cgf-kernel", "#![forbid(missing_docs, unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), Vec::<Violation>::new());
}

/// ASM-024: every root of a package is checked (lib and bin).
#[test]
fn given_bin_root_without_forbid_when_checked_then_that_root_is_reported() {
    let lib = root("xtask", "#![forbid(unsafe_code)]\n");
    let bin = CrateRoot {
        package: "xtask".to_string(),
        file: "main.rs".to_string(),
        source: "fn main() {}\n".to_string(),
    };

    assert_eq!(
        check_forbid_unsafe(&[lib, bin]),
        vec![Violation::MissingForbidUnsafe {
            package: "xtask".to_string(),
            file: "main.rs".to_string()
        }]
    );
}

/// ASM-024: the output is sorted whatever the input order.
#[test]
fn given_several_missing_when_checked_then_sorted() {
    let a = check_forbid_unsafe(&[root("cgf-net", ""), root("cgf-core", "")]);

    assert_eq!(a, vec![missing("cgf-core"), missing("cgf-net")]);
}

/// ASM-024 (fixture): the violating fixture has exactly one seeded crate without the attribute;
/// the FFI crates are empty and exempt.
#[test]
fn given_violating_fixture_when_loaded_then_only_api_types_is_missing_forbid() {
    let roots = load_crate_roots(&fixture("violating")).expect("load");

    assert!(roots.iter().any(|r| r.package == "cgf-plugin-api"));
    assert_eq!(check_forbid_unsafe(&roots), vec![missing("cgf-api-types")]);
}

/// ASM-024 (fixture): the conforming fixture has no violation.
#[test]
fn given_conforming_fixture_when_loaded_then_no_forbid_violation() {
    let roots = load_crate_roots(&fixture("conforming")).expect("load");

    assert!(roots.len() >= 4, "expected the fixture crate roots");
    assert_eq!(check_forbid_unsafe(&roots), Vec::<Violation>::new());
}

/// Loader error path: a missing manifest is an error.
#[test]
fn given_missing_manifest_when_loading_roots_then_error() {
    assert!(load_crate_roots(&fixture("does_not_exist")).is_err());
}
