//! AT-17 (CLI rows): the stderr `note:` about a defaulted tick rate. Requirements: REQ-C-1,
//! REQ-X-2, ADR 0010 (note of 2026-10-19), ADR 0029, ASM-160, ASM-168.
//!
//! stdout stays in its existing format; only stderr gains a `note:` line, and only when the scenario
//! omits `tick_rate_hz`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{cli, example, example_dir, final_hash, is_16_lower_hex, stderr, stdout};

/// A copy of the example scenario without its `tick_rate_hz` line, in a scratch directory.
fn scenario_without_rate(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-rate-{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates")).expect("mkdir");
    let source = example_dir();
    for entry in fs::read_dir(source.join("templates")).expect("templates") {
        let path = entry.expect("entry").path();
        fs::copy(
            &path,
            dir.join("templates").join(path.file_name().expect("name")),
        )
        .expect("copy");
    }
    let text = fs::read_to_string(source.join("scenario.toml")).expect("scenario");
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with("tick_rate_hz"))
        .collect();
    let text = kept.join("\n") + "\n";
    fs::write(dir.join("scenario.toml"), text).expect("write");
    dir.join("scenario.toml")
}

fn notes(err: &str) -> Vec<&str> {
    err.lines().filter(|l| l.starts_with("note:")).collect()
}

#[test]
fn run_without_rate_writes_the_stderr_note_and_unchanged_stdout_format() {
    let path = scenario_without_rate("run");
    let out = cli(&["run", path.to_str().expect("utf8"), "--ticks", "120"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    let found = notes(&err);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("tick_rate_hz"), "{}", found[0]);
    assert!(found[0].contains("default of 60 Hz"), "{}", found[0]);
    let text = stdout(&out);
    assert!(!text.contains("note:"), "the note must not reach stdout");
    assert!(final_hash(&text).as_deref().is_some_and(is_16_lower_hex));
    assert!(
        text.contains("t=2.000s"),
        "120 ticks at 60 Hz is 2 s:\n{text}"
    );
}

#[test]
fn run_with_explicit_rate_writes_no_note() {
    let out = cli(&["run", example().as_str(), "--ticks", "30"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(notes(&stderr(&out)).is_empty(), "{}", stderr(&out));
}

#[test]
fn validate_without_rate_notes_the_default() {
    let path = scenario_without_rate("validate");
    let out = cli(&["validate", path.to_str().expect("utf8")]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    let found = notes(&err);
    assert_eq!(found.len(), 1, "{err}");
    assert!(found[0].contains("default of 60 Hz"), "{}", found[0]);
    assert!(!stdout(&out).contains("note:"));
    let explicit = cli(&["validate", example().as_str()]);
    assert_eq!(notes(&stderr(&explicit)).len(), 0);
}
