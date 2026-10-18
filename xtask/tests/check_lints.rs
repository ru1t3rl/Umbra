//! Tests for the `[lints] workspace = true` check (review m6, ASM-024, NFR-2).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use xtask::deps::{LoadError, Manifest, Violation, Workspace, check_workspace_lints};

fn load_manifests(manifest: &Path) -> Result<Vec<Manifest>, LoadError> {
    Workspace::load(Some(manifest)).map(|ws| ws.manifests)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml")
}

fn manifest(package: &str, source: &str) -> Manifest {
    Manifest {
        package: package.to_string(),
        source: source.to_string(),
    }
}

fn missing(package: &str) -> Violation {
    Violation::MissingWorkspaceLints {
        package: package.to_string(),
    }
}

/// m6: `[lints]` with `workspace = true` conforms.
#[test]
fn given_lints_workspace_true_when_checked_then_no_violation() {
    let m = manifest(
        "a",
        "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n\n[dependencies]\n",
    );

    assert_eq!(check_workspace_lints(&[m]), Vec::<Violation>::new());
}

/// m6: spacing variants are accepted.
#[test]
fn given_spacing_variants_when_checked_then_no_violation() {
    let m = manifest("a", "[ lints ]\r\nworkspace=true # inherit\r\n");

    assert_eq!(check_workspace_lints(&[m]), Vec::<Violation>::new());
}

/// m6: no `[lints]` table at all.
#[test]
fn given_no_lints_table_when_checked_then_violation() {
    let m = manifest("a", "[package]\nname = \"a\"\n");

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// m6: `workspace = true` in another table does not count.
#[test]
fn given_workspace_true_in_other_table_when_checked_then_violation() {
    let m = manifest(
        "a",
        "[package]\nversion.workspace = true\n[dependencies.x]\nworkspace = true\n",
    );

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// m6: `[lints]` with its own rules (no inheritance) is a violation.
#[test]
fn given_own_lints_without_workspace_when_checked_then_violation() {
    let m = manifest("a", "[lints.rust]\nunsafe_code = \"allow\"\n");

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// m6: `workspace = false` does not count.
#[test]
fn given_workspace_false_when_checked_then_violation() {
    let m = manifest("a", "[lints]\nworkspace = false\n");

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// m6 (fixture): the violating fixture has exactly one member without inherited lints.
#[test]
fn given_violating_fixture_when_loaded_then_only_net_dis_lacks_lints() {
    let ms = load_manifests(&fixture("violating")).expect("load");

    assert_eq!(check_workspace_lints(&ms), vec![missing("umbra-net-dis")]);
}

/// m6 (fixture): the conforming fixture inherits everywhere.
#[test]
fn given_conforming_fixture_when_loaded_then_no_lints_violation() {
    let ms = load_manifests(&fixture("conforming")).expect("load");

    assert_eq!(check_workspace_lints(&ms), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- review of 0a25b26: finding 6

const PKG: &str = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n";

/// Finding 6: every TOML spelling of the top-level `lints.workspace = true` is accepted.
#[test]
fn given_toml_spellings_of_lints_workspace_true_when_checked_then_no_violation() {
    for source in [
        format!("lints.workspace = true\n{PKG}"),
        format!("lints = {{ workspace = true }}\n{PKG}"),
        format!("{PKG}[\"lints\"]\nworkspace = true\n"),
        format!("{PKG}[lints] # x\nworkspace = true # y\n"),
        format!("{PKG}[lints]\nworkspace=true\n"),
    ] {
        let m = manifest("a", &source);
        assert_eq!(
            check_workspace_lints(&[m]),
            Vec::<Violation>::new(),
            "rejected wrongly:\n{source}"
        );
    }
}

/// Finding 6: after `[package]`, `lints.workspace = true` is the key `package.lints.workspace`,
/// so the workspace lints are not inherited (TOML semantics, not a false positive).
#[test]
fn given_lints_key_inside_package_table_when_checked_then_violation() {
    for source in [
        format!("{PKG}lints.workspace = true\n"),
        format!("{PKG}lints = {{ workspace = true }}\n"),
    ] {
        let m = manifest("a", &source);
        assert_eq!(
            check_workspace_lints(&[m]),
            vec![missing("a")],
            "accepted wrongly:\n{source}"
        );
    }
}

/// Finding 6: `[lints]` text inside a multi-line string is not a table.
#[test]
fn given_lints_text_inside_multiline_string_when_checked_then_violation() {
    let m = manifest(
        "a",
        &format!("{PKG}description = \"\"\"\n[lints]\nworkspace = true\n\"\"\"\n"),
    );

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// Finding 6: `workspace = true` in `[lints.rust]` is not `lints.workspace`.
#[test]
fn given_workspace_true_in_lints_rust_when_checked_then_violation() {
    let m = manifest("a", &format!("{PKG}[lints.rust]\nworkspace = true\n"));

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// Finding 6: `workspace = "true"` (a string) is not `true`.
#[test]
fn given_workspace_string_true_when_checked_then_violation() {
    let m = manifest("a", &format!("{PKG}[lints]\nworkspace = \"true\"\n"));

    assert_eq!(check_workspace_lints(&[m]), vec![missing("a")]);
}

/// Finding 6: a manifest that is not valid TOML cannot be verified.
#[test]
fn given_invalid_toml_when_checked_then_violation() {
    let m = manifest(
        "a",
        "[lints]\nworkspace = true\n[lints]\nworkspace = true\n",
    );

    let v = check_workspace_lints(&[m]);

    assert!(
        matches!(
            v.as_slice(),
            [Violation::UnparsableSource { package, file, .. }]
                if package == "a" && file == "Cargo.toml"
        ),
        "{v:?}"
    );
}
