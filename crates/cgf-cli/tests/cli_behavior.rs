//! AT-12 (process row) and AT-16 (process rows): `cgf-cli validate` and `cgf-cli run` on the
//! patrol-and-engage example, as real processes.
//! Requirements: M3 acceptance 1, REQ-K-4 (cross-process determinism), REQ-T-6, REQ-X-2, REQ-S-1.
//!
//! Written in step 4 of M3; all rows run since WI-16 (the CLI uses the standard registry since WI-15).

#![forbid(unsafe_code)]

mod common;

use std::path::Path;
use std::process::Output;

use common::{cli, final_hash, is_16_lower_hex, stderr, stdout};

fn example_dir() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/scenarios/patrol_and_engage")
        .to_string_lossy()
        .into_owned()
}

fn run(extra: &[&str]) -> Output {
    let scenario = format!("{}/scenario.toml", example_dir());
    let mut args = vec!["run", scenario.as_str()];
    args.extend_from_slice(extra);
    cli(&args)
}

/// REQ-T-6: the example directory validates: the scenario, four templates and three graphs.
#[test]
fn validate_example_ok() {
    let dir = example_dir();

    let out = cli(&["validate", dir.as_str()]);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {} stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out).lines().last(), Some("ok: 8 files checked"));
}

/// REQ-S-1, M3 acceptance 1: `run` prints the state lines of all three entities and a 16-digit
/// final state hash.
#[test]
fn run_example_prints_states_and_hash() {
    let out = run(&["--ticks", "300"]);

    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    for name in ["blue-1", "red-1", "blue-2"] {
        assert!(
            text.lines()
                .any(|l| l.starts_with("tick=30 ") && l.contains(&format!("name={name} "))),
            "no state line for {name} at tick 30 in:\n{text}"
        );
    }
    assert!(
        final_hash(&text).as_deref().is_some_and(is_16_lower_hex),
        "{text}"
    );
}

/// REQ-K-4, AT-12: four separate processes print byte-identical output, with every entity state
/// line and the state hash after every tick (the CLI output has no intents or contacts; those are
/// compared in `cgf-engine/tests/behavior_determinism.rs`). Separate processes have different
/// hasher seeds and address-space layouts, so hidden `RandomState` or pointer-ordered iteration
/// shows up here. 1800 ticks (60 s) run past blue-1's first `Fire` (tick 1130), so the file
/// subgraph, the FSM and the `chance` draws are compared too (amended after the WI-16 review).
#[test]
fn patrol_and_engage_in_four_processes_is_byte_identical() {
    let args = [
        "--json",
        "--print-every",
        "1",
        "--hash-every",
        "1",
        "--ticks",
        "1800",
    ];

    let runs: Vec<Output> = (0..4).map(|_| run(&args)).collect();

    for r in &runs {
        assert_eq!(r.status.code(), Some(0), "stderr: {}", stderr(r));
    }
    let text = stdout(&runs[0]);
    let hash_lines = text
        .lines()
        .filter(|l| l.contains("\"state_hash\""))
        .count();
    assert_eq!(hash_lines, 1801, "one hash per tick plus the final line");
    for r in &runs[1..] {
        assert_eq!(r.stdout, runs[0].stdout);
    }
}

/// Extracts every fenced block whose info string is exactly `toml file=<relative path>`
/// (convention in the header comment of `docs/behavior.md`), as `(path, content)` pairs.
fn extract_file_blocks(doc: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(String, String)> = None;
    for line in doc.lines() {
        if let Some((_, body)) = current.as_mut() {
            if line == "```" {
                blocks.extend(current.take());
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some(path) = line.strip_prefix("```toml file=") {
            current = Some((path.trim().to_string(), String::new()));
        }
    }
    blocks
}

/// NFR-5, REQ-B-8 (WI-17): the `file=` examples in `docs/behavior.md` form one valid example set.
#[test]
fn behavior_doc_examples_validate() {
    let doc_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/behavior.md");
    let doc = std::fs::read_to_string(&doc_path).unwrap_or_else(|e| panic!("read doc: {e}"));
    let blocks = extract_file_blocks(&doc);
    assert!(blocks.len() >= 8, "found {} blocks", blocks.len());
    let mut paths: Vec<&str> = blocks.iter().map(|(p, _)| p.as_str()).collect();
    paths.sort_unstable();
    let before = paths.len();
    paths.dedup();
    assert_eq!(paths.len(), before, "two blocks share a path");

    let dir = std::env::temp_dir().join(format!("cgf-behavior-doc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, content) in &blocks {
        let target = dir.join(rel);
        let parent = target.parent().unwrap_or(&dir);
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir: {e}"));
        std::fs::write(&target, content).unwrap_or_else(|e| panic!("write: {e}"));
    }

    let out = cli(&["validate", dir.to_string_lossy().as_ref()]);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {} stderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert_eq!(text.lines().last(), Some("ok: 8 files checked"), "{text}");
}
