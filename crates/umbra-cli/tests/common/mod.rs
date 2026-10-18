//! Shared helpers for the `umbra-cli` process tests (M2). Each test binary uses a subset.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `umbra-cli` with the crate directory as working directory, so relative fixture paths in the
/// arguments appear unchanged in the output.
pub fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbra"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|e| panic!("failed to run umbra-cli: {e}"))
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The example scenario file (absolute).
pub fn example() -> String {
    example_dir()
        .join("scenario.toml")
        .to_string_lossy()
        .into_owned()
}

pub fn example_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scenarios/walking_skeleton")
}

/// Asserts a clap usage error (exit code 2) about `mentions`. The message must not be the generic
/// "unrecognized subcommand" of a binary that lacks the command under test, so that the test cannot
/// pass before the command exists.
pub fn assert_usage_error(out: &Output, mentions: &str) {
    let err = stderr(out);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}, stderr: {err}",
        stdout(out)
    );
    assert!(
        !err.contains("unrecognized subcommand"),
        "the command is missing: {err}"
    );
    assert!(
        err.contains(mentions),
        "stderr does not mention {mentions:?}: {err}"
    );
}

/// Parses stdout as one JSON document.
pub fn json_of(out: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(out))
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{}", stdout(out)))
}

/// The value after `state_hash=` on the last text line.
pub fn final_hash(out: &str) -> Option<String> {
    out.lines()
        .rev()
        .find_map(|l| l.strip_prefix("state_hash="))
        .map(str::to_string)
}

pub fn is_16_lower_hex(s: &str) -> bool {
    s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
