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

#[expect(
    clippy::expect_used,
    reason = "test helper: spawn failure is a test error"
)]
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
        Violation::NonMemberPathDep {
            from: "cgf-cli".into(),
            to: "shim".into(),
            kind: DepKind::Normal,
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
        "cgf-cli -> shim (normal)",
        "NFR-2: cgf-api-types (src/lib.rs) lacks #![forbid(unsafe_code)]",
        "cgf-net-dis",
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

// ---------------------------------------------------------------- review M1 / M2 / m2 / n3 / n4

/// M1: the shim fixture edge is reported (`cgf-cli -> shim` would launder `cgf-kernel`).
#[test]
fn given_shim_path_dep_when_loaded_then_edge_is_path_and_not_workspace() {
    let g = load_graph(&fixture("violating")).expect("load");

    let e = g.edges.iter().find(|e| e.to == "shim").expect("shim edge");

    assert!(e.is_path && !e.is_workspace);
}

/// M2: a workspace whose only violation is a missing `forbid(unsafe_code)` exits 1.
#[test]
fn given_only_missing_forbid_when_check_deps_runs_then_exit_code_1() {
    let out = run_check_deps(&fixture("unsafe_only"));
    let text = combined(&out);

    assert_eq!(out.status.code(), Some(1), "output:\n{text}");
    assert!(
        text.contains("NFR-2: cgf-core (src/lib.rs) lacks #![forbid(unsafe_code)]"),
        "{text}"
    );
    assert!(!text.contains("REQ-A"), "{text}");
}

/// M2/m6: a workspace whose only violation is a missing `[lints] workspace = true` exits 1.
#[test]
fn given_only_missing_lints_when_check_deps_runs_then_exit_code_1() {
    let out = run_check_deps(&fixture("lints_only"));
    let text = combined(&out);

    assert_eq!(out.status.code(), Some(1), "output:\n{text}");
    assert!(text.contains("cgf-core"), "{text}");
    assert!(text.contains("[lints] workspace = true"), "{text}");
}

#[expect(
    clippy::expect_used,
    reason = "test helper: spawn failure is a test error"
)]
fn run_args(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("spawn xtask binary")
}

/// m2: usage errors exit 2.
#[test]
fn given_usage_errors_when_check_deps_runs_then_exit_code_2() {
    assert_eq!(run_args(&[]).status.code(), Some(2));
    assert_eq!(run_args(&["bogus"]).status.code(), Some(2));
    assert_eq!(run_args(&["check-deps", "--bogus"]).status.code(), Some(2));
    assert_eq!(
        run_args(&["check-deps", "--manifest-path"]).status.code(),
        Some(2)
    );
}

/// m2: a non-UTF-8 subcommand is a usage error (exit 2), not a panic.
#[cfg(unix)]
#[test]
fn given_non_utf8_argument_when_check_deps_runs_then_exit_code_2() {
    use std::os::unix::ffi::OsStrExt;
    let bad = std::ffi::OsStr::from_bytes(b"check-deps\xff");

    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg(bad)
        .output()
        .expect("spawn");

    assert_eq!(out.status.code(), Some(2));
}

/// n4: `--manifest-path=<p>` is accepted.
#[test]
fn given_equals_form_when_check_deps_runs_then_it_works() {
    let arg = format!("--manifest-path={}", fixture("conforming").display());

    let out = run_args(&["check-deps", &arg]);

    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
}

/// n3: without `--manifest-path`, cargo searches upward from the working directory.
#[test]
fn given_subdirectory_cwd_when_check_deps_runs_without_manifest_then_workspace_is_found() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("check-deps")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .output()
        .expect("spawn");

    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
}
