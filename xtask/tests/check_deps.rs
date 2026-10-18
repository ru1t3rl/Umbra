//! Integration tests: `Workspace::load` on fixture workspaces and the `xtask check-deps` binary.
//!
//! Covers AT-5 (fixture part) and AT-6. Requirements: REQ-A-1, REQ-A-2, REQ-A-3, NFR-2, ASM-005,
//! ASM-006. Fixtures: `tests/fixtures/*` (see README there). All offline.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask::deps::{DepKind, RULES, Violation, Workspace, WorkspaceGraph, check, check_workspace};

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

#[expect(
    clippy::expect_used,
    reason = "test helper: a fixture that fails to load is a test error"
)]
fn load_graph(manifest: &Path) -> WorkspaceGraph {
    Workspace::load(Some(manifest)).expect("load").graph
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a fixture that fails to load is a test error"
)]
fn workspace_violations(name: &str) -> Vec<Violation> {
    let mut v = check_workspace(
        &Workspace::load(Some(&fixture(name))).expect("load"),
        &RULES,
    );
    // Package ids contain absolute paths; tests compare them as `<id>`.
    for x in &mut v {
        if let Violation::NonMemberPathPackage { id, .. } | Violation::ImpostorPackage { id, .. } =
            x
        {
            *id = "<id>".to_string();
        }
    }
    v.sort();
    v
}

fn path_pkg(name: &str, version: &str) -> Violation {
    Violation::NonMemberPathPackage {
        name: name.into(),
        version: version.into(),
        id: "<id>".into(),
    }
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
    let g = load_graph(&fixture("violating"));

    let has = |from: &str, to: &str, kind: DepKind, target: bool| {
        g.edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind && e.target.is_some() == target)
    };
    assert!(g.members.contains("umbra-mystery"));
    assert!(!g.members.contains("fake_external"));
    assert!(has("umbra-core", "umbra-kernel", DepKind::Normal, false));
    assert!(has("umbra-kernel", "umbra-net-dis", DepKind::Dev, false));
    assert!(has(
        "umbra-kernel",
        "umbra-api-types",
        DepKind::Build,
        false
    ));
    assert!(has("umbra-sdk", "umbra-core", DepKind::Normal, true));
    assert!(
        has("umbra-motion", "umbra-kernel", DepKind::Normal, false),
        "rename must resolve to real name"
    );
}

/// ASM-006: `fake_external` is a registry dependency (patched to a local path only so the
/// resolved pass works offline), so it is neither a workspace nor a path edge.
#[test]
fn given_registry_dep_when_loaded_then_edge_is_neither_workspace_nor_path() {
    let g = load_graph(&fixture("violating"));

    let ext: Vec<_> = g.edges.iter().filter(|e| e.to == "fake_external").collect();

    assert_eq!(ext.len(), 2, "normal and build edge");
    assert!(ext.iter().all(|e| !e.is_workspace && !e.is_path));
    assert!(
        g.edges
            .iter()
            .filter(|e| e.to == "umbra-kernel")
            .all(|e| e.is_workspace)
    );
}

/// AT-5: loading the fixture and running `check` finds each seeded violation exactly.
#[test]
fn given_violating_fixture_when_loaded_and_checked_then_each_seeded_violation_is_found() {
    let g = load_graph(&fixture("violating"));

    let v = check(&g, &RULES);

    let f = |from: &str, to: &str, kind, requirement| Violation::ForbiddenWorkspaceDep {
        from: from.into(),
        to: to.into(),
        kind,
        requirement,
    };
    let x = |kind| Violation::ExternalDepNotAllowed {
        from: "umbra-plugin-api".into(),
        to: "fake_external".into(),
        kind,
    };
    let mut expected = vec![
        f("umbra-core", "umbra-kernel", DepKind::Normal, "REQ-A-1"),
        f("umbra-kernel", "umbra-server", DepKind::Normal, "REQ-A-2"),
        f("umbra-kernel", "umbra-net-dis", DepKind::Dev, "REQ-A-2"),
        f("umbra-kernel", "umbra-api-types", DepKind::Build, "REQ-A-2"),
        f("umbra-sdk", "umbra-core", DepKind::Normal, "REQ-A-3"),
        x(DepKind::Normal),
        x(DepKind::Build),
        Violation::UnknownCrate {
            name: "umbra-mystery".into(),
        },
        Violation::NonMemberPathDep {
            from: "umbra-cli".into(),
            to: "shim".into(),
            kind: DepKind::Normal,
        },
    ];
    // umbra-motion -> umbra-kernel (renamed): requirement label is "section 4" per the plan table.
    expected.push(f(
        "umbra-motion",
        "umbra-kernel",
        DepKind::Normal,
        "section 4",
    ));
    expected.sort();
    assert_eq!(v, expected);
}

/// AT-6: the conforming fixture loads and has no violation.
#[test]
fn given_conforming_fixture_when_loaded_and_checked_then_no_violations() {
    let g = load_graph(&fixture("conforming"));

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

/// Loader error path: a missing manifest is an error, not a panic.
#[test]
fn given_missing_manifest_when_loaded_then_error() {
    let r = Workspace::load(Some(&fixture("does_not_exist")));

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
        "REQ-A-1: umbra-core -> umbra-kernel (normal)",
        "REQ-A-2: umbra-kernel -> umbra-server (normal)",
        "REQ-A-2: umbra-kernel -> umbra-net-dis (dev)",
        "REQ-A-2: umbra-kernel -> umbra-api-types (build)",
        "REQ-A-3: umbra-sdk -> umbra-core (normal)",
        "umbra-plugin-api -> fake_external (normal)",
        "umbra-plugin-api -> fake_external (build)",
        "umbra-motion -> umbra-kernel (normal)",
        "umbra-mystery",
        "umbra-cli -> shim (normal)",
        "NFR-2: umbra-api-types (src/lib.rs) lacks #![forbid(unsafe_code)]",
        "umbra-net-dis",
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

/// M1: the shim fixture edge is reported (`umbra-cli -> shim` would launder `umbra-kernel`).
#[test]
fn given_shim_path_dep_when_loaded_then_edge_is_path_and_not_workspace() {
    let g = load_graph(&fixture("violating"));

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
        text.contains("NFR-2: umbra-core (src/lib.rs) lacks #![forbid(unsafe_code)]"),
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
    assert!(text.contains("umbra-core"), "{text}");
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

// ---------------------------------------------------------------- review of 0a25b26: laundering

fn assert_rejected_with(fixture_name: &str, needles: &[&str]) {
    let out = run_check_deps(&fixture(fixture_name));
    let text = combined(&out);

    assert_eq!(out.status.code(), Some(1), "output:\n{text}");
    for needle in needles {
        assert!(
            text.contains(needle),
            "missing `{needle}` in output:\n{text}"
        );
    }
}

/// Finding 1 (REQ-A-3): `[patch.crates-io] shim = { path = "shim" }` in the root manifest turns a
/// registry dependency into a path package that depends on `umbra-kernel`. Only the resolved graph
/// shows it. The same table patches a registry `umbra-kernel = "0.1"` to the member itself, which the
/// `--no-deps` pass sees as an external dependency.
#[test]
fn given_root_patch_launder_when_check_deps_runs_then_resolved_edges_are_reported() {
    assert_rejected_with(
        "launder_patch",
        &[
            "REQ-A-3: umbra-sdk -> shim -> umbra-kernel (resolved) not allowed",
            "section 4: umbra-net-dis -> umbra-kernel (normal) not allowed",
        ],
    );
}

/// Finding 1 (REQ-A-3): the same patch in `.cargo/config.toml` of the checked workspace.
#[test]
fn given_config_patch_launder_when_check_deps_runs_then_resolved_edge_is_reported() {
    assert_rejected_with(
        "launder_config_patch",
        &["REQ-A-3: umbra-sdk -> shim -> umbra-kernel (resolved) not allowed"],
    );
}

/// Finding 2 (REQ-A-3): a non-member path package named like a member (`umbra-plugin-api`) must not
/// count as that member.
#[test]
fn given_same_name_impostor_when_check_deps_runs_then_it_is_reported() {
    assert_rejected_with(
        "launder_same_name",
        &[
            "ASM-006: umbra-sdk -> umbra-plugin-api (normal) is a path dependency outside the workspace",
            "ASM-006: package umbra-plugin-api 9.0.0",
            "REQ-A-3: umbra-sdk -> umbra-plugin-api -> umbra-kernel (resolved) not allowed",
        ],
    );
}

/// Finding 3 (ASM-006): `fake_external` is no longer a production allow-list entry.
#[test]
fn given_fake_external_path_launder_when_check_deps_runs_then_it_is_reported() {
    assert_rejected_with(
        "launder_fake_external",
        &[
            "ASM-006: umbra-sdk -> fake_external (normal) is a path dependency outside the workspace",
            "REQ-A-3: umbra-sdk -> fake_external -> umbra-kernel (resolved) not allowed",
        ],
    );
}

/// Finding 7 (NFR-2, ASM-024): every target kind of a non-FFI crate is scanned; the FFI crate's
/// test target stays exempt.
#[test]
fn given_targets_without_forbid_when_check_deps_runs_then_each_is_reported() {
    let out = run_check_deps(&fixture("unsafe_targets"));
    let text = combined(&out);

    assert_eq!(out.status.code(), Some(1), "output:\n{text}");
    for file in [
        "benches/bench.rs",
        "build.rs",
        "examples/ex.rs",
        "src/bin/tool.rs",
        "tests/it.rs",
    ] {
        let needle = format!("NFR-2: umbra-core ({file}) lacks #![forbid(unsafe_code)]");
        assert!(
            text.contains(&needle),
            "missing `{needle}` in output:\n{text}"
        );
    }
    assert!(
        !text.contains("umbra-sdk"),
        "FFI crate must be exempt:\n{text}"
    );
    assert!(
        !text.contains("src/lib.rs"),
        "lib.rs has the attribute:\n{text}"
    );
}

/// Nit 10: `--manifest-path` given twice is a usage error.
#[test]
fn given_repeated_manifest_path_when_check_deps_runs_then_exit_code_2() {
    let m = fixture("conforming");
    let m = m.to_str().expect("fixture path is UTF-8");
    let eq = format!("--manifest-path={m}");

    assert_eq!(
        run_args(&["check-deps", "--manifest-path", m, "--manifest-path", m])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run_args(&["check-deps", &eq, "--manifest-path", m])
            .status
            .code(),
        Some(2)
    );
}

/// Nit 10: a non-UTF-8 value in `--manifest-path=<p>` is reported as such (exit 2).
#[cfg(unix)]
#[test]
fn given_non_utf8_equals_value_when_check_deps_runs_then_message_says_not_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let bad = std::ffi::OsStr::from_bytes(b"--manifest-path=\xff");

    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("check-deps")
        .arg(bad)
        .output()
        .expect("spawn");

    assert_eq!(out.status.code(), Some(2));
    assert!(
        combined(&out).contains("--manifest-path value is not UTF-8"),
        "{}",
        combined(&out)
    );
}

fn indirect(from: &str, via: &str, to: &str) -> Violation {
    Violation::IndirectWorkspaceDep {
        from: from.into(),
        via: vec![via.into()],
        to: to.into(),
        requirement: "REQ-A-3",
    }
}

/// Finding 1 (library level): exactly the two laundered edges, nothing else.
#[test]
fn given_root_patch_launder_when_checked_then_exact_violations() {
    let mut expected = vec![
        Violation::ForbiddenWorkspaceDep {
            from: "umbra-net-dis".into(),
            to: "umbra-kernel".into(),
            kind: DepKind::Normal,
            requirement: "section 4",
        },
        indirect("umbra-sdk", "shim", "umbra-kernel"),
        Violation::ImpostorDep {
            from: "umbra-net-dis".into(),
            to: "umbra-kernel".into(),
            kind: DepKind::Normal,
        },
        path_pkg("shim", "0.1.0"),
    ];
    expected.sort();

    assert_eq!(workspace_violations("launder_patch"), expected);
}

/// Finding 1 (library level): the `.cargo/config.toml` patch is applied because cargo runs in
/// the checked workspace's directory.
#[test]
fn given_config_patch_launder_when_checked_then_exact_violations() {
    let mut expected = vec![
        indirect("umbra-sdk", "shim", "umbra-kernel"),
        path_pkg("shim", "0.1.0"),
    ];
    expected.sort();

    assert_eq!(workspace_violations("launder_config_patch"), expected);
}

/// Finding 2 (library level): direct non-member path edge, the impostor and the laundered edge.
#[test]
fn given_same_name_impostor_when_checked_then_exact_violations() {
    let v = workspace_violations("launder_same_name");

    let (impostors, rest): (Vec<_>, Vec<_>) = v
        .into_iter()
        .partition(|v| matches!(v, Violation::ImpostorPackage { .. }));
    assert!(
        matches!(
            impostors.as_slice(),
            [Violation::ImpostorPackage { name, version, .. }]
                if name == "umbra-plugin-api" && version == "9.0.0"
        ),
        "{impostors:?}"
    );
    let mut expected = vec![
        Violation::NonMemberPathDep {
            from: "umbra-sdk".into(),
            to: "umbra-plugin-api".into(),
            kind: DepKind::Normal,
        },
        indirect("umbra-sdk", "umbra-plugin-api", "umbra-kernel"),
        path_pkg("umbra-plugin-api", "9.0.0"),
    ];
    expected.sort();
    assert_eq!(rest, expected);
}

/// Finding 2: membership is decided by path, so the edge to the impostor is not a workspace edge.
#[test]
fn given_same_name_impostor_when_loaded_then_edge_is_path_but_not_workspace() {
    let g = load_graph(&fixture("launder_same_name"));

    let e = g
        .edges
        .iter()
        .find(|e| e.from == "umbra-sdk" && e.to == "umbra-plugin-api")
        .expect("edge");

    assert!(e.is_path && !e.is_workspace, "{e:?}");
}

/// Finding 3 (library level): no allow-list, so the `fake_external` shim is reported twice over.
#[test]
fn given_fake_external_path_launder_when_checked_then_exact_violations() {
    let mut expected = vec![
        Violation::NonMemberPathDep {
            from: "umbra-sdk".into(),
            to: "fake_external".into(),
            kind: DepKind::Normal,
        },
        indirect("umbra-sdk", "fake_external", "umbra-kernel"),
        path_pkg("fake_external", "0.1.0"),
    ];
    expected.sort();

    assert_eq!(workspace_violations("launder_fake_external"), expected);
}

/// Both passes agree on the conforming fixture and on the real workspace.
#[test]
fn given_conforming_and_real_workspace_when_checked_then_no_violations() {
    assert_eq!(workspace_violations("conforming"), Vec::<Violation>::new());
    let real = Workspace::load(Some(&real_workspace())).expect("load");
    assert_eq!(check_workspace(&real, &RULES), Vec::<Violation>::new());
}

/// A workspace without `Cargo.lock` cannot be resolved with `--locked`: load error, exit 2
/// (fail closed, never a silent pass).
#[test]
fn given_workspace_without_lockfile_when_check_deps_runs_then_exit_code_2() {
    let dir = std::env::temp_dir().join(format!("umbra-xtask-nolock-{}", std::process::id()));
    let src = dir.join("crates/umbra-core/src");
    std::fs::create_dir_all(&src).expect("mkdir");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"3\"\n",
    )
    .expect("write");
    std::fs::write(
        dir.join("crates/umbra-core/Cargo.toml"),
        "[package]\nname = \"umbra-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lints]\nworkspace = true\n",
    )
    .expect("write");
    std::fs::write(src.join("lib.rs"), "#![forbid(unsafe_code)]\n").expect("write");

    let out = run_check_deps(&dir.join("Cargo.toml"));
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(out.status.code(), Some(2), "output:\n{}", combined(&out));
}

// ---------------------------------------------------------------- re-review of 341eca7

/// M1 (NFR-2, ASM-006): `[patch.crates-io] shim = { path = "shim" }` brings a non-member path
/// package with unchecked `unsafe` code into the build. Every non-member path package in the
/// resolved graph is a violation, however it got there.
#[test]
fn given_patch_to_unchecked_path_package_when_check_deps_runs_then_it_is_reported() {
    assert_rejected_with(
        "launder_unsafe_patch",
        &[
            "ASM-006: package shim 1.0.0 (",
            "is a path package outside the workspace",
        ],
    );
}

/// m1: when the resolved graph cannot be loaded, the declared-pass diagnostics are still printed
/// and the run exits 2 (the gate fails; it is not a skip).
#[test]
fn given_unresolvable_workspace_when_check_deps_runs_then_declared_violations_and_exit_2() {
    let out = run_check_deps(&fixture("resolve_fails"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(2), "output:\n{}", combined(&out));
    for needle in [
        "REQ-A-1: umbra-core -> umbra-kernel (normal) not allowed",
        "ASM-006: umbra-sdk -> umbra-kernel (normal) uses the name of a workspace crate",
    ] {
        assert!(
            stdout.contains(needle),
            "missing `{needle}` in stdout:\n{stdout}"
        );
    }
    assert!(stderr.contains("resolved graph"), "stderr:\n{stderr}");
}

/// M1 (library level): the patched path package is the only violation.
#[test]
fn given_patch_to_unchecked_path_package_when_checked_then_exact_violation() {
    assert_eq!(
        workspace_violations("launder_unsafe_patch"),
        vec![path_pkg("shim", "1.0.0")]
    );
}

/// M1: the fixtures' external crate comes from a vendored directory source, so it has a
/// crates.io source and is not a path package; in the violating fixture the only path package is
/// the seeded `shim` (`umbra-cli -> shim`).
#[test]
fn given_vendored_registry_dep_when_checked_then_only_seeded_path_package_is_reported() {
    let ws = Workspace::load(Some(&fixture("violating"))).expect("load");
    let resolved = ws.resolved.as_ref().expect("resolved");
    let fake = resolved
        .packages
        .values()
        .find(|p| p.name == "fake_external")
        .expect("fake_external");
    assert_eq!(
        fake.source.as_deref(),
        Some("registry+https://github.com/rust-lang/crates.io-index")
    );

    let path_pkgs: Vec<Violation> = workspace_violations("violating")
        .into_iter()
        .filter(|v| matches!(v, Violation::NonMemberPathPackage { .. }))
        .collect();
    assert_eq!(path_pkgs, vec![path_pkg("shim", "0.1.0")]);
}
