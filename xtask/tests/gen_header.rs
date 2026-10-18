//! AT-2 (M4 WI-4): `cargo xtask gen-header [--check]` (REQ-SDK-2, gate 11, ADR 0023 item 2).
//!
//! The process tests need the pinned `cbindgen` (`cargo install --locked cbindgen@0.29.4`, on `PATH`
//! or named by `CBINDGEN`), like gate 11 itself. Fixture: `tests/fixtures/gen_header` (one struct, its
//! committed header); mutated copies live under `CARGO_TARGET_TMPDIR`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use xtask::header::{
    CBINDGEN_ENV, HEADER_PATH, INSTALL_HINT, first_difference, normalise_newlines,
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gen_header")
}

#[expect(
    clippy::expect_used,
    reason = "test helper: spawn failure is a test error"
)]
fn gen_header(args: &[&str], cbindgen: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    cmd.arg("gen-header").args(args);
    if let Some(program) = cbindgen {
        cmd.env(CBINDGEN_ENV, program);
    }
    cmd.output().expect("spawn xtask binary")
}

fn check_dir(dir: &Path) -> Output {
    let dir = dir
        .to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path {}", dir.display()));
    gen_header(&["--check", "--crate-dir", dir], None)
}

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}

fn text(out: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a failing copy is a test error"
)]
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            if entry.file_name() != "target" {
                copy_dir(&entry.path(), &target);
            }
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

/// A fresh copy of the fixture under `CARGO_TARGET_TMPDIR/gen-header/<name>`.
fn fixture_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gen-header")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("remove {}: {e}", dir.display()));
    }
    copy_dir(&fixture(), &dir);
    dir
}

#[expect(
    clippy::expect_used,
    reason = "test helper: a failing edit is a test error"
)]
fn edit(path: &Path, f: impl FnOnce(String) -> String) {
    let text = std::fs::read_to_string(path).expect("read");
    std::fs::write(path, f(text)).expect("write");
}

/// Gate 11: the committed `umbra_plugin_api.h` is what the pinned cbindgen generates.
#[test]
fn committed_header_is_current() {
    let out = gen_header(&["--check"], None);

    assert_eq!(code(&out), Some(0), "{}", text(&out));
}

/// Control for the drift test: the fixture's committed header is current.
#[test]
fn fixture_header_is_current() {
    let out = check_dir(&fixture());

    assert_eq!(code(&out), Some(0), "{}", text(&out));
}

/// A field appended in Rust but not regenerated is drift: exit 1, with the line.
#[test]
fn appended_field_is_drift() {
    let dir = fixture_copy("appended");
    edit(&dir.join("src/lib.rs"), |s| {
        s.replace(
            "    pub struct_size: u32,\n",
            "    pub struct_size: u32,\n    /// Appended.\n    pub appended: u64,\n",
        )
    });

    let out = check_dir(&dir);

    assert_eq!(code(&out), Some(1), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("differs from the generated header"),
        "{}",
        text(&out)
    );
}

/// A CRLF checkout of the header (Windows, `core.autocrlf`) is not drift.
#[test]
fn crlf_is_not_drift() {
    let dir = fixture_copy("crlf");
    edit(&dir.join(HEADER_PATH), |s| s.replace('\n', "\r\n"));

    let out = check_dir(&dir);

    assert_eq!(code(&out), Some(0), "{}", text(&out));
}

/// Write mode regenerates a header that was edited by hand.
#[test]
fn write_mode_restores_the_header() {
    let dir = fixture_copy("write");
    edit(&dir.join(HEADER_PATH), |s| s + "/* hand edit */\n");
    let dir_text = dir.to_str().unwrap_or_else(|| panic!("non-UTF-8 path"));

    let out = gen_header(&["--crate-dir", dir_text], None);

    assert_eq!(code(&out), Some(0), "{}", text(&out));
    assert_eq!(code(&check_dir(&dir)), Some(0));
}

/// A `cbindgen` of another version is refused with exit 2 and the install line (any program whose
/// `--version` is not `cbindgen 0.29.4`; cargo itself is one on every platform).
#[test]
fn wrong_cbindgen_version_is_exit_2() {
    let out = gen_header(&["--check"], Some(env!("CARGO")));

    assert_eq!(code(&out), Some(2), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(INSTALL_HINT),
        "{}",
        text(&out)
    );
}

/// A missing `cbindgen` is exit 2 with the install line.
#[test]
fn missing_cbindgen_is_exit_2() {
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-cbindgen");
    let missing = missing.to_str().unwrap_or_else(|| panic!("non-UTF-8 path"));

    let out = gen_header(&["--check"], Some(missing));

    assert_eq!(code(&out), Some(2), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(INSTALL_HINT),
        "{}",
        text(&out)
    );
}

/// Usage errors are exit 2.
#[test]
fn unknown_argument_is_exit_2() {
    let out = gen_header(&["--chek"], None);

    assert_eq!(code(&out), Some(2), "{}", text(&out));
}

/// The comparison is LF-normalised and reports the first differing line.
#[test]
fn comparison_normalises_line_endings_and_finds_the_line() {
    assert_eq!(normalise_newlines("a\r\nb\r\n"), "a\nb\n");
    assert_eq!(first_difference("a\nb\n", "a\r\nb\r\n"), None);
    assert_eq!(first_difference("a\nb\nc\n", "a\nx\nc\n"), Some(2));
    assert_eq!(first_difference("a\nb\n", "a\nb\nc\n"), Some(3));
}
