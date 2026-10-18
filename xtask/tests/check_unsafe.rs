//! Tests for the `#![forbid(unsafe_code)]` check (ASM-024, CLAUDE.md rule 5, NFR-2).
//!
//! Pure tests use synthetic [`CrateRoot`] values; fixture tests load
//! `tests/fixtures/{violating,conforming}` through [`Workspace::load`].

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use xtask::deps::{CrateRoot, LoadError, Violation, Workspace, check_forbid_unsafe};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml")
}

fn load_crate_roots(manifest: &Path) -> Result<Vec<CrateRoot>, LoadError> {
    Workspace::load(Some(manifest)).map(|ws| ws.roots)
}

fn root(package: &str, source: &str) -> CrateRoot {
    CrateRoot {
        package: package.to_string(),
        file: "src/lib.rs".to_string(),
        source: source.to_string(),
    }
}

fn missing(package: &str) -> Violation {
    Violation::MissingForbidUnsafe {
        package: package.to_string(),
        file: "src/lib.rs".to_string(),
    }
}

/// ASM-024: a non-FFI crate with the attribute conforms.
#[test]
fn given_core_with_forbid_when_checked_then_no_violation() {
    let r = root(
        "umbra-core",
        "//! docs\n\n#![forbid(unsafe_code)]\n\npub mod a;\n",
    );

    assert_eq!(check_forbid_unsafe(&[r]), Vec::<Violation>::new());
}

/// ASM-024: a non-FFI crate without the attribute is reported.
#[test]
fn given_core_without_forbid_when_checked_then_violation() {
    let r = root("umbra-core", "//! docs\npub fn f() {}\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("umbra-core")]);
}

/// ASM-024: the three FFI crates are exempt.
#[test]
fn given_ffi_crates_without_forbid_when_checked_then_no_violation() {
    let roots = [
        root("umbra-plugin-api", ""),
        root("umbra-plugin-host", ""),
        root("umbra-sdk", ""),
    ];

    assert_eq!(check_forbid_unsafe(&roots), Vec::<Violation>::new());
}

/// ASM-024: `deny` is not `forbid`.
#[test]
fn given_deny_instead_of_forbid_when_checked_then_violation() {
    let r = root("umbra-kernel", "#![deny(unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("umbra-kernel")]);
}

/// ASM-024: a commented-out attribute does not count.
#[test]
fn given_commented_out_forbid_when_checked_then_violation() {
    let r = root("umbra-kernel", "// #![forbid(unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("umbra-kernel")]);
}

/// ASM-024: an outer attribute (`#[forbid(...)]` on an item) does not count.
#[test]
fn given_outer_attribute_only_when_checked_then_violation() {
    let r = root("umbra-kernel", "#[forbid(unsafe_code)]\nmod m {}\n");

    assert_eq!(check_forbid_unsafe(&[r]), vec![missing("umbra-kernel")]);
}

/// ASM-024: the attribute may sit next to other lints in one list.
#[test]
fn given_forbid_in_a_lint_list_when_checked_then_no_violation() {
    let r = root("umbra-kernel", "#![forbid(missing_docs, unsafe_code)]\n");

    assert_eq!(check_forbid_unsafe(&[r]), Vec::<Violation>::new());
}

/// ASM-024: every root of a package is checked (lib and bin).
#[test]
fn given_bin_root_without_forbid_when_checked_then_that_root_is_reported() {
    let lib = root("xtask", "#![forbid(unsafe_code)]\n");
    let bin = CrateRoot {
        package: "xtask".to_string(),
        file: "src/main.rs".to_string(),
        source: "fn main() {}\n".to_string(),
    };

    assert_eq!(
        check_forbid_unsafe(&[lib, bin]),
        vec![Violation::MissingForbidUnsafe {
            package: "xtask".to_string(),
            file: "src/main.rs".to_string()
        }]
    );
}

/// ASM-024: the output is sorted whatever the input order.
#[test]
fn given_several_missing_when_checked_then_sorted() {
    let a = check_forbid_unsafe(&[root("umbra-net", ""), root("umbra-core", "")]);

    assert_eq!(a, vec![missing("umbra-core"), missing("umbra-net")]);
}

/// ASM-024 (fixture): the violating fixture has exactly one seeded crate without the attribute;
/// the FFI crates are empty and exempt.
#[test]
fn given_violating_fixture_when_loaded_then_only_api_types_is_missing_forbid() {
    let roots = load_crate_roots(&fixture("violating")).expect("load");

    assert!(roots.iter().any(|r| r.package == "umbra-plugin-api"));
    assert_eq!(
        check_forbid_unsafe(&roots),
        vec![Violation::MissingForbidUnsafe {
            package: "umbra-api-types".to_string(),
            file: "src/lib.rs".to_string()
        }]
    );
}

/// ASM-024 (fixture): the conforming fixture has no violation.
#[test]
fn given_conforming_fixture_when_loaded_then_no_forbid_violation() {
    let roots = load_crate_roots(&fixture("conforming")).expect("load");

    let ids: Vec<(&str, &str)> = roots
        .iter()
        .map(|r| (r.package.as_str(), r.file.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            ("umbra-core", "src/lib.rs"),
            ("umbra-kernel", "src/lib.rs"),
            ("umbra-motion", "src/lib.rs"),
            ("umbra-net-dis", "src/lib.rs"),
            ("umbra-plugin-api", "src/lib.rs"),
            ("umbra-plugin-host", "src/lib.rs"),
            ("umbra-sdk", "src/lib.rs"),
        ]
    );
    assert_eq!(check_forbid_unsafe(&roots), Vec::<Violation>::new());
}

/// Loader error path: a missing manifest is an error.
#[test]
fn given_missing_manifest_when_loading_roots_then_error() {
    assert!(load_crate_roots(&fixture("does_not_exist")).is_err());
}

// ---------------------------------------------------------------- review M3 / m1 / m4

/// Exactly one violation for the root: the attribute is missing, or the file does not parse
/// (rustc rejects such a file too, so either way the root is not accepted).
fn assert_missing(source: &str) {
    let r = root("umbra-kernel", source);
    let v = check_forbid_unsafe(&[r]);
    assert!(
        matches!(
            v.as_slice(),
            [Violation::MissingForbidUnsafe { package, file }
                | Violation::UnparsableSource { package, file, .. }]
                if package == "umbra-kernel" && file == "src/lib.rs"
        ),
        "source accepted wrongly ({v:?}):\n{source}"
    );
}

fn assert_accepted(source: &str) {
    let r = root("umbra-kernel", source);
    assert_eq!(
        check_forbid_unsafe(&[r]),
        Vec::<Violation>::new(),
        "source rejected wrongly:\n{source}"
    );
}

/// M3: an attribute inside a block comment does not count.
#[test]
fn given_forbid_in_block_comment_when_checked_then_violation() {
    assert_missing("/*\n#![forbid(unsafe_code)]\n*/\npub fn f() {}\n");
}

/// M3: an attribute inside a multi-line string literal does not count.
#[test]
fn given_forbid_in_string_literal_when_checked_then_violation() {
    assert_missing("pub const S: &str = \"\n#![forbid(unsafe_code)]\n\";\n");
}

/// M3: an attribute inside an inner module does not count.
#[test]
fn given_forbid_in_inner_module_when_checked_then_violation() {
    assert_missing("mod inner {\n    #![forbid(unsafe_code)]\n}\n");
    assert_missing("mod inner {\n#![forbid(unsafe_code)]\n}\n");
}

/// M3: after the first item the preamble is over.
#[test]
fn given_forbid_after_first_item_when_checked_then_violation() {
    assert_missing("pub fn f() {}\n#![forbid(unsafe_code)]\n");
}

/// Indentation does not matter to rustc, so an indented file-level inner attribute counts
/// (the old text scanner required column 0).
#[test]
fn given_indented_forbid_when_checked_then_no_violation() {
    assert_accepted("  #![forbid(unsafe_code)]\n");
}

/// M3: comments, doc comments, blank lines and other inner attributes may precede it.
#[test]
fn given_preamble_before_forbid_when_checked_then_no_violation() {
    assert_accepted(
        "//! docs\n\n// note\n/* block\n   #![deny(x)] */\n/*! inner doc */\n#![warn(missing_docs)]\n#![forbid(unsafe_code)]\npub fn f() {}\n",
    );
}

/// m1: the attribute may span lines and have spaces after `#!`.
#[test]
fn given_multiline_or_spaced_forbid_when_checked_then_no_violation() {
    assert_accepted("#![forbid(\n    unsafe_code\n)]\n");
    assert_accepted("#! [forbid(unsafe_code)]\n");
    assert_accepted("#![forbid(unsafe_code)]\r\npub fn f() {}\r\n");
}

/// m1 / ASM-024: `cfg_attr(..., forbid(unsafe_code))` is conditional and does not count.
#[test]
fn given_cfg_attr_forbid_when_checked_then_violation() {
    assert_missing("#![cfg_attr(all(), forbid(unsafe_code))]\n");
}

/// m1: `forbid` of another lint does not count.
#[test]
fn given_forbid_of_other_lint_when_checked_then_violation() {
    assert_missing("#![forbid(unused)]\n");
}

/// M3: a string containing `]` inside an earlier attribute does not end it early.
#[test]
fn given_attribute_with_bracket_in_string_when_checked_then_scan_continues() {
    assert_accepted("#![doc = \"a ] b\"]\n#![forbid(unsafe_code)]\n");
}

/// m4 / finding 7: roots are identified by path relative to the package, with `/` separators,
/// and every target kind is a root (here lib, bin and the integration tests).
#[test]
fn given_real_workspace_when_loading_roots_then_every_target_is_listed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");

    let roots = load_crate_roots(&manifest).expect("load");

    let xtask: Vec<&str> = roots
        .iter()
        .filter(|r| r.package == "xtask")
        .map(|r| r.file.as_str())
        .collect();
    assert_eq!(
        xtask,
        [
            "src/lib.rs",
            "src/main.rs",
            "tests/check_deps.rs",
            "tests/check_lints.rs",
            "tests/check_rules.rs",
            "tests/check_unsafe.rs",
            "tests/gen_header.rs",
            "tests/lint_facts.rs",
            "tests/repo_config.rs",
        ]
    );
}

// ---------------------------------------------------------------- review of 0a25b26: findings 4, 5

/// Finding 4: a `]` inside a raw string of an earlier attribute must not end that attribute, so the
/// `#![forbid(unsafe_code)]` text inside the string does not count.
#[test]
fn given_forbid_text_inside_raw_string_of_attribute_when_checked_then_violation() {
    assert_missing("#![doc = r#\"x\"]\n#![forbid(unsafe_code)]\n\"#]\n");
}

/// Finding 4: the same through a block comment inside an attribute.
#[test]
fn given_forbid_text_inside_block_comment_of_attribute_when_checked_then_violation() {
    assert_missing("#![allow(dead_code /* ]\n#![forbid(unsafe_code)]\n*/)]\n");
}

/// Finding 4: the same through a line comment inside an attribute.
#[test]
fn given_forbid_text_after_line_comment_inside_attribute_when_checked_then_violation() {
    assert_missing("#![doc = \"a\" // ]\n#![forbid(unsafe_code)]\n]\n");
}

/// Finding 4: a nested block comment hides the attribute.
#[test]
fn given_forbid_inside_nested_block_comment_when_checked_then_violation() {
    assert_missing("/* /* nested */ #![forbid(unsafe_code)]\n */\n");
}

/// Finding 4: a source that does not parse cannot be verified, so it is a violation.
#[test]
fn given_unparsable_source_when_checked_then_violation() {
    let r = root("umbra-kernel", "#![forbid(unsafe_code)]\nfn f( {\n");

    let v = check_forbid_unsafe(&[r]);

    assert!(
        matches!(
            v.as_slice(),
            [Violation::UnparsableSource { package, file, .. }]
                if package == "umbra-kernel" && file == "src/lib.rs"
        ),
        "{v:?}"
    );
}

/// Finding 5: valid crate roots that the old scanner rejected.
#[test]
fn given_valid_preamble_variants_when_checked_then_no_violation() {
    for source in [
        "\u{feff}#![forbid(unsafe_code)]\n",
        "# ! [forbid(unsafe_code)]\n",
        "#![cfg_attr(any(), x(']'))]\n#![forbid(unsafe_code)]\n",
        "#![cfg_attr(any(), doc = r\"\\\")]\n#![forbid(unsafe_code)]\n",
        "#![doc = concat!(\"a\", r##\"b\"#c\"##)]\n#![forbid(unsafe_code)]\n",
        "#![doc = b\"x\"]\n#![forbid(unsafe_code)]\n",
        "#![doc = \"a ] )]\"]\n#![forbid(unsafe_code)]\n",
        "/* /* nested */ */\n#![forbid(unsafe_code)]\n",
        "#![forbid(unsafe_code, reason = \"x\")]\n",
        "//! doc\r\n//! more\r\n#![forbid(unsafe_code)]\r\n",
        "#![warn(missing_docs)]\n#![forbid(\n    unsafe_code,\n)]\n",
    ] {
        assert_accepted(source);
    }
}
