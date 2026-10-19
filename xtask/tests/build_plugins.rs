//! AT-13 (gate 12): `cargo xtask build-plugins [--release]` builds the example plugins and writes
//! packages `<target dir>/plugins/{wander,c-circle}/` (library plus `plugin.toml`, ADR 0022).
//! Requirements: REQ-SDK-3, REQ-P-10, ADR 0022, ADR 0023 item 3 and 4, gate 12.
//!
//! Step 4 of M4: written before WI-13 (the `wander` package) and WI-14 (the C package); ignored
//! with the work item that makes each pass. The tests run the xtask binary from the workspace root
//! with `CARGO_TARGET_DIR` pointing into `CARGO_TARGET_TMPDIR`, so the real `target/plugins` is never
//! touched and parallel tests never share a build directory. Assumption the plan leaves open
//! (reported): "`target/plugins`" means `<cargo target dir>/plugins`, honoring `CARGO_TARGET_DIR`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[expect(
    clippy::expect_used,
    reason = "test helper: spawn failure is a test error"
)]
fn build_plugins(target: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("build-plugins")
        .args(args)
        .current_dir(workspace_root())
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("spawn xtask binary")
}

fn text(out: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A fresh, empty target directory under `CARGO_TARGET_TMPDIR`.
#[expect(
    clippy::expect_used,
    reason = "test helper: a failing mkdir is a test error"
)]
fn fresh_target(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("build-plugins-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the target dir");
    dir
}

/// The files of `dir` (not recursive), names only, sorted.
#[expect(
    clippy::expect_used,
    reason = "test helper: unreadable dir is a test error"
)]
fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn is_library(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e == std::env::consts::DLL_EXTENSION)
}

/// Gate 12, M4 acceptance 1: the wander package is one library plus a `plugin.toml` naming the
/// plugin, its version and the ABI the host was built for.
#[test]
fn build_plugins_writes_the_wander_package() {
    let target = fresh_target("wander");

    let out = build_plugins(&target, &[]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let package = target.join("plugins/wander");
    let files = names(&package);
    assert_eq!(
        files.iter().filter(|f| is_library(f)).count(),
        1,
        "{files:?}"
    );
    assert!(files.iter().any(|f| f == "plugin.toml"), "{files:?}");
    let manifest = std::fs::read_to_string(package.join("plugin.toml")).unwrap_or_default();
    assert!(manifest.contains("schema_version = 1"), "{manifest}");
    assert!(
        manifest.contains("id = \"example.plugin.wander\""),
        "{manifest}"
    );
    assert!(manifest.contains("version = \"0.1.0\""), "{manifest}");
    assert!(manifest.contains("abi = \"0.0\""), "{manifest}");
}

/// Gate 12 with `--release`: the same package from the release profile.
#[test]
fn build_plugins_release_writes_the_wander_package() {
    let target = fresh_target("wander-release");

    let out = build_plugins(&target, &["--release"]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let files = names(&target.join("plugins/wander"));
    assert_eq!(
        files.iter().filter(|f| is_library(f)).count(),
        1,
        "{files:?}"
    );
    assert!(
        target.join("release").is_dir(),
        "built with the release profile"
    );
}

/// ADR 0023 item 3, M4 acceptance 3: the C example is compiled with `cc` and packaged as
/// `c-circle`, with a manifest for `example.plugin.c_circle`.
#[test]
#[ignore = "WI-14"]
fn build_plugins_writes_the_c_circle_package() {
    let target = fresh_target("c-circle");

    let out = build_plugins(&target, &[]);

    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let package = target.join("plugins/c-circle");
    let files = names(&package);
    assert_eq!(
        files.iter().filter(|f| is_library(f)).count(),
        1,
        "{files:?}"
    );
    let manifest = std::fs::read_to_string(package.join("plugin.toml")).unwrap_or_default();
    assert!(
        manifest.contains("id = \"example.plugin.c_circle\""),
        "{manifest}"
    );
}

/// An unknown flag is a usage error: exit 2 (the xtask convention), and nothing is written.
#[test]
fn build_plugins_unknown_flag_is_exit_2() {
    let target = fresh_target("bad-flag");

    let out = build_plugins(&target, &["--no-such-flag"]);

    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(!target.join("plugins").exists());
}
