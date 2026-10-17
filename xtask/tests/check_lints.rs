//! Tests for the `[lints] workspace = true` check (review m6, ASM-024).

use std::path::{Path, PathBuf};

use xtask::deps::{Manifest, Violation, check_workspace_lints, load_manifests};

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

    assert_eq!(check_workspace_lints(&ms), vec![missing("cgf-net-dis")]);
}

/// m6 (fixture): the conforming fixture inherits everywhere.
#[test]
fn given_conforming_fixture_when_loaded_then_no_lints_violation() {
    let ms = load_manifests(&fixture("conforming")).expect("load");

    assert_eq!(check_workspace_lints(&ms), Vec::<Violation>::new());
}
