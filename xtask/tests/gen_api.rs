//! AT-12 (M5 WI-12): `cargo xtask gen-api [--check] [--api-dir <dir>]` (REQ-X-4, gate 10,
//! M5 acceptance 1, ADR 0025). Exit codes: 0 current or written, 1 drift, 2 error or usage.
//!
//! The tests run the built `xtask` binary as a process; `gen-api` itself runs the `gen_api` example
//! of `umbra-server` through nested `cargo` (offline). Mutated copies of the committed `api/`
//! directory live under `CARGO_TARGET_TMPDIR`.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FILES: [&str; 3] = ["openapi.json", "stream.schema.json", "asyncapi.json"];

fn committed_api_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../api")
}

fn gen_api(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("gen-api")
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn xtask: {e}"))
}

fn text(out: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A fresh copy of the committed `api/` under `CARGO_TARGET_TMPDIR/gen-api/<name>`.
fn api_copy(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gen-api")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    for file in FILES {
        let from = committed_api_dir().join(file);
        std::fs::copy(&from, dir.join(file))
            .unwrap_or_else(|e| panic!("copy {}: {e}", from.display()));
    }
    dir
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn dir_arg(dir: &Path) -> &str {
    dir.to_str()
        .unwrap_or_else(|| panic!("non-UTF-8 path {}", dir.display()))
}

#[test]
fn committed_contract_is_current() {
    let out = gen_api(&["--check"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    for file in FILES {
        assert!(
            committed_api_dir().join(file).is_file(),
            "{file} is committed"
        );
    }
}

#[test]
fn edited_committed_file_is_drift_exit_1() {
    let dir = api_copy("edited");
    // The unedited copy is current, so the failure below is caused by the edit.
    let clean = gen_api(&["--check", "--api-dir", dir_arg(&dir)]);
    assert_eq!(clean.status.code(), Some(0), "{}", text(&clean));

    let file = dir.join("openapi.json");
    let original = read(&file);
    let edited = original.replacen("\"title\"", "\"titel\"", 1);
    assert_ne!(edited, original, "the edit changed the file");
    write(&file, &edited);
    let drift = gen_api(&["--check", "--api-dir", dir_arg(&dir)]);
    assert_eq!(drift.status.code(), Some(1), "{}", text(&drift));
    assert!(
        text(&drift).contains("openapi.json"),
        "names the file: {}",
        text(&drift)
    );

    // Without `--check` the file is rewritten and the next check passes.
    let fixed = gen_api(&["--api-dir", dir_arg(&dir)]);
    assert_eq!(fixed.status.code(), Some(0), "{}", text(&fixed));
    assert_eq!(read(&file), original);
}

#[test]
fn crlf_is_not_drift() {
    let dir = api_copy("crlf");
    for file in FILES {
        let path = dir.join(file);
        let text = read(&path);
        assert!(!text.contains('\r'), "{file} is committed with LF");
        write(&path, &text.replace('\n', "\r\n"));
    }
    let out = gen_api(&["--check", "--api-dir", dir_arg(&dir)]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn unknown_argument_is_exit_2() {
    for args in [
        vec!["--bogus"],
        vec!["--check", "extra"],
        vec!["--api-dir"],
        vec!["--check", "--api-dir", "a", "--api-dir", "b"],
    ] {
        let out = gen_api(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", text(&out));
    }
}

#[test]
fn documents_are_stable_across_two_generations() {
    // Extra (WI-12 lists it): two generations into empty directories are byte-identical, and equal
    // the committed files.
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gen-api");
    let first = base.join("gen-1");
    let second = base.join("gen-2");
    for dir in [&first, &second] {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
        let out = gen_api(&["--api-dir", dir_arg(dir)]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    }
    for file in FILES {
        let a = read(&first.join(file));
        assert_eq!(
            a,
            read(&second.join(file)),
            "{file} differs between generations"
        );
        assert_eq!(
            a,
            read(&committed_api_dir().join(file)),
            "{file} differs from the committed file"
        );
    }
}

#[test]
fn unexpected_json_file_is_drift_exit_1() {
    let dir = api_copy("extra");
    write(&dir.join("old.json"), "{}\n");
    let out = gen_api(&["--check", "--api-dir", dir_arg(&dir)]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("old.json"),
        "names the file: {}",
        text(&out)
    );
    // A file that is not JSON is not a contract file.
    std::fs::remove_file(dir.join("old.json")).unwrap_or_else(|e| panic!("remove: {e}"));
    write(&dir.join("notes.txt"), "x\n");
    let clean = gen_api(&["--check", "--api-dir", dir_arg(&dir)]);
    assert_eq!(clean.status.code(), Some(0), "{}", text(&clean));
}
