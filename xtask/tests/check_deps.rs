//! Integration tests: `load_graph` on fixture workspaces and the `xtask check-deps` binary.
//!
//! Covers AT-5 (fixture part) and AT-6. Requirements: REQ-A-1, REQ-A-2, REQ-A-3, ASM-005,
//! ASM-006. Fixtures: `tests/fixtures/{violating,conforming}` (see README there). All offline.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask::deps::{DepKind, RULES, Violation, check, load_graph};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .join("Cargo.toml")
}

fn real_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml")
}

#[allow(clippy::expect_used)] // test helper: spawn failure is a test error
fn run_check_deps(manifest: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("check-deps")
        .arg("--manifest-path")
        .arg(manifest)
        .output()
        .expect("spawn xtask binary")
}

fn combined(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ---------------------------------------------------------------- loader (AT-5, fixture part)

/// ASM-005/006: the loader reports members and every edge kind, and resolves renames.
#[test]
fn given_violating_fixture_when_loaded_then_all_edge_kinds_and_renames_are_resolved() {
    let g = load_graph(&fixture("violating")).expect("load");

    let has = |from: &str, to: &str, kind: DepKind, target: bool| {
        g.edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind && e.target.is_some() == target)
    };
    assert!(g.members.contains("cgf-mystery"));
    assert!(!g.members.contains("fake_external"));
    assert!(has("cgf-core", "cgf-kernel", DepKind::Normal, false));
    assert!(has("cgf-kernel", "cgf-net-dis", DepKind::Dev, false));
    assert!(has("cgf-kernel", "cgf-api-types", DepKind::Build, false));
    assert!(has("cgf-sdk", "cgf-core", DepKind::Normal, true));
    assert!(
        has("cgf-motion", "cgf-kernel", DepKind::Normal, false),
        "rename must resolve to real name"
    );
}

/// ASM-006: path dependencies outside the workspace count as external.
#[test]
fn given_path_dep_outside_workspace_when_loaded_then_edge_is_not_workspace() {
    let g = load_graph(&fixture("violating")).expect("load");

    let ext: Vec<_> = g.edges.iter().filter(|e| e.to == "fake_external").collect();

    assert_eq!(ext.len(), 2, "normal and build edge");
    assert!(ext.iter().all(|e| !e.is_workspace));
    assert!(
        g.edges
            .iter()
            .filter(|e| e.to == "cgf-kernel")
            .all(|e| e.is_workspace)
    );
}

/// AT-5: loading the fixture and running `check` finds each seeded violation exactly.
#[test]
fn given_violating_fixture_when_loaded_and_checked_then_each_seeded_violation_is_found() {
    let g = load_graph(&fixture("violating")).expect("load");

    let v = check(&g, &RULES);

    let f = |from: &str, to: &str, kind, requirement| Violation::ForbiddenWorkspaceDep {
        from: from.into(),
        to: to.into(),
        kind,
        requirement,
    };
    let x = |kind| Violation::ExternalDepNotAllowed {
        from: "cgf-plugin-api".into(),
        to: "fake_external".into(),
        kind,
    };
    let mut expected = vec![
        f("cgf-core", "cgf-kernel", DepKind::Normal, "REQ-A-1"),
        f("cgf-kernel", "cgf-server", DepKind::Normal, "REQ-A-2"),
        f("cgf-kernel", "cgf-net-dis", DepKind::Dev, "REQ-A-2"),
        f("cgf-kernel", "cgf-api-types", DepKind::Build, "REQ-A-2"),
        f("cgf-sdk", "cgf-core", DepKind::Normal, "REQ-A-3"),
        x(DepKind::Normal),
        x(DepKind::Build),
        Violation::UnknownCrate {
            name: "cgf-mystery".into(),
        },
    ];
    // cgf-motion -> cgf-kernel (renamed): requirement label is "section 4" per the plan table.
    expected.push(f("cgf-motion", "cgf-kernel", DepKind::Normal, "section 4"));
    expected.sort();
    assert_eq!(v, expected);
}

/// AT-6: the conforming fixture loads and has no violation.
#[test]
fn given_conforming_fixture_when_loaded_and_checked_then_no_violations() {
    let g = load_graph(&fixture("conforming")).expect("load");

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

/// Loader error path: a missing manifest is an error, not a panic.
#[test]
fn given_missing_manifest_when_loaded_then_error() {
    let r = load_graph(&fixture("does_not_exist"));

    assert!(r.is_err());
}

// ---------------------------------------------------------------- binary (AT-6)

/// M0 acceptance 2: the check fails (exit 1) on a violating workspace and lists each violation.
#[test]
fn given_violating_fixture_when_check_deps_runs_then_exit_code_1_and_lists_each_violation() {
    let out = run_check_deps(&fixture("violating"));
    let text = combined(&out);

    assert_eq!(out.status.code(), Some(1), "output:\n{text}");
    for needle in [
        "REQ-A-1: cgf-core -> cgf-kernel (normal)",
        "REQ-A-2: cgf-kernel -> cgf-server (normal)",
        "REQ-A-2: cgf-kernel -> cgf-net-dis (dev)",
        "REQ-A-2: cgf-kernel -> cgf-api-types (build)",
        "REQ-A-3: cgf-sdk -> cgf-core (normal)",
        "cgf-plugin-api -> fake_external (normal)",
        "cgf-plugin-api -> fake_external (build)",
        "cgf-motion -> cgf-kernel (normal)",
        "cgf-mystery",
    ] {
        assert!(
            text.contains(needle),
            "missing `{needle}` in output:\n{text}"
        );
    }
}

/// M0 acceptance 2: running twice yields byte-identical output (tool determinism).
#[test]
fn given_violating_fixture_when_check_deps_runs_twice_then_output_is_identical() {
    let a = run_check_deps(&fixture("violating"));
    let b = run_check_deps(&fixture("violating"));

    assert_eq!(a.status.code(), Some(1));
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(a.stderr, b.stderr);
}

/// AT-6: a conforming workspace exits 0.
#[test]
fn given_conforming_fixture_when_check_deps_runs_then_exit_code_0() {
    let out = run_check_deps(&fixture("conforming"));

    assert_eq!(out.status.code(), Some(0), "output:\n{}", combined(&out));
}

/// AT-6 (also a CI job): the real workspace conforms.
#[test]
fn given_real_workspace_when_check_deps_runs_then_exit_code_0() {
    let out = run_check_deps(&real_workspace());

    assert_eq!(out.status.code(), Some(0), "output:\n{}", combined(&out));
}

/// CLI contract: a load error exits with 2.
#[test]
fn given_missing_manifest_when_check_deps_runs_then_exit_code_2() {
    let out = run_check_deps(&fixture("does_not_exist"));

    assert_eq!(out.status.code(), Some(2), "output:\n{}", combined(&out));
}
