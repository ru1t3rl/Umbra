//! AT-11 (CLI rows, M7): `umbra run --record <file>`, as real processes. Requirements: REQ-K-7,
//! REQ-K-4, REQ-X-2, ADR 0053, ASM-621, ASM-625.
//!
//! Two processes have different per-process hasher seeds and address-space layouts, so byte-identical
//! recordings across processes catch nondeterminism an in-process comparison cannot. Written in M7
//! WI-1: ignored until M7 WI-13 adds `--record`. Not here, left to WI-13: `serve --record-dir`
//! writing one file per load (needs a running server).

#![forbid(unsafe_code)]

mod common;

use std::path::PathBuf;
use std::process::Output;

use common::{cli, example, stderr, stdout};

fn scratch(tag: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-record");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("scratch dir: {e}"));
    dir.join(format!("{}-{tag}.umbrarec.jsonl", std::process::id()))
}

fn run_recorded(file: &std::path::Path) -> Output {
    let scenario = example();
    cli(&[
        "run",
        scenario.as_str(),
        "--ticks",
        "300",
        "--record",
        file.to_str().unwrap_or_else(|| panic!("UTF-8 path")),
    ])
}

/// ASM-621: two processes recording the same run write byte-identical files.
#[test]
#[ignore = "M7 WI-13"]
fn two_recordings_are_byte_identical() {
    let (a, b) = (scratch("a"), scratch("b"));

    let (first, second) = (run_recorded(&a), run_recorded(&b));

    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert_eq!(second.status.code(), Some(0));
    let (bytes_a, bytes_b) = (
        std::fs::read(&a).unwrap_or_else(|e| panic!("{e}")),
        std::fs::read(&b).unwrap_or_else(|e| panic!("{e}")),
    );
    assert!(bytes_a.len() > 100, "a recording with a header and records");
    assert_eq!(bytes_a, bytes_b);
}

/// REQ-K-4: four processes record byte-identical files.
#[test]
#[ignore = "M7 WI-13"]
fn four_processes_record_byte_identical_files() {
    let files: Vec<PathBuf> = (0..4).map(|i| scratch(&format!("p{i}"))).collect();

    let outputs: Vec<Output> = files.iter().map(|f| run_recorded(f)).collect();

    assert!(outputs.iter().all(|o| o.status.code() == Some(0)));
    let first = std::fs::read(&files[0]).unwrap_or_else(|e| panic!("{e}"));
    for f in &files[1..] {
        assert_eq!(std::fs::read(f).unwrap_or_else(|e| panic!("{e}")), first);
    }
}

/// ADR 0053: recording leaves `run`'s stdout untouched (same lines, same final hash).
#[test]
#[ignore = "M7 WI-13"]
fn run_stdout_is_unchanged_by_recording() {
    let scenario = example();
    let plain = cli(&["run", scenario.as_str(), "--ticks", "300"]);

    let recorded = run_recorded(&scratch("stdout"));

    assert_eq!(plain.status.code(), Some(0));
    assert_eq!(stdout(&recorded), stdout(&plain));
}

/// ASM-625: a write failure never stops the simulation; `run --record` exits 1 after the run (Linux:
/// `/dev/full` accepts the open and fails every write).
#[cfg(target_os = "linux")]
#[test]
#[ignore = "M7 WI-13"]
fn write_failure_exits_1_after_the_run() {
    let scenario = example();
    let plain = cli(&["run", scenario.as_str(), "--ticks", "300"]);

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--ticks",
        "300",
        "--record",
        "/dev/full",
    ]);

    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), stdout(&plain), "the run itself completed");
    assert!(
        stderr(&out).contains("record"),
        "the failure is reported: {}",
        stderr(&out)
    );
}

/// ADR 0055: `run --state-hash 3` prints the v3 hashes: stable across processes and different from
/// the default; `--state-hash 2` is the default and prints the same output.
#[test]
#[ignore = "M7 WI-15"]
fn state_hash_3_is_selectable_stable_and_different_from_the_default() {
    let scenario = example();
    let run = |extra: &[&str]| {
        let mut args = vec!["run", scenario.as_str(), "--ticks", "300"];
        args.extend_from_slice(extra);
        cli(&args)
    };

    let (default, two) = (run(&[]), run(&["--state-hash", "2"]));
    let (v3_a, v3_b) = (run(&["--state-hash", "3"]), run(&["--state-hash", "3"]));

    assert_eq!(v3_a.status.code(), Some(0), "stderr: {}", stderr(&v3_a));
    assert_eq!(stdout(&two), stdout(&default), "2 is the default");
    assert_eq!(stdout(&v3_a), stdout(&v3_b), "stable across processes");
    assert_ne!(
        common::final_hash(&stdout(&v3_a)),
        common::final_hash(&stdout(&default))
    );
}

/// ADR 0055: `run --state-hash 3 --record` writes version 3 into the header.
#[test]
#[ignore = "M7 WI-15"]
fn recording_with_state_hash_3_names_version_3_in_the_header() {
    let scenario = example();
    let file = scratch("v3");

    let out = cli(&[
        "run",
        scenario.as_str(),
        "--ticks",
        "60",
        "--state-hash",
        "3",
        "--record",
        file.to_str().unwrap_or_else(|| panic!("UTF-8 path")),
    ]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{e}"));
    let header = text.lines().next().unwrap_or_default();
    assert!(header.contains("\"state_hash_version\":3"), "{header}");
}
