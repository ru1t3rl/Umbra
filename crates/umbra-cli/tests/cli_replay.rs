//! AT-10 (CLI rows, M7) and AT-7 (golden hashes through the CLI): `umbra replay <recording>`, as real
//! processes. Requirements: REQ-K-7, REQ-K-4, REQ-X-2, ADR 0054, ASM-629..631.
//!
//! Exit codes: 0 ok, 1 divergence or runtime error, 2 input or usage error (ASM-630). Written in M7
//! WI-1: ignored until M7 WI-13 (`--record`) and M7 WI-14 (`replay`).

#![forbid(unsafe_code)]

mod common;

use std::path::{Path, PathBuf};

use common::{cli, example, example_dir, final_hash, json_of, stderr, stdout};

fn scratch(tag: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-replay");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("scratch dir: {e}"));
    dir.join(format!("{}-{tag}", std::process::id()))
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap_or_else(|| panic!("UTF-8 path"))
}

/// Records `scenario` for 300 ticks into a fresh file; returns the file and the run's final hash.
fn recorded(scenario: &str, tag: &str) -> (PathBuf, String) {
    let file = scratch(&format!("{tag}.umbrarec.jsonl"));
    let out = cli(&[
        "run",
        scenario,
        "--ticks",
        "300",
        "--record",
        path_str(&file),
    ]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let hash = final_hash(&stdout(&out)).unwrap_or_else(|| panic!("no final hash"));
    (file, hash)
}

/// ADR 0054: replay exits 0 and prints the final state hash of the recorded run.
#[test]
#[ignore = "M7 WI-14"]
fn replay_exits_0_and_prints_the_recorded_final_hash() {
    let (file, hash) = recorded(&example(), "ok");

    let out = cli(&["replay", path_str(&file)]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(final_hash(&stdout(&out)), Some(hash));
}

/// AT-7: replaying a full recording of `walking_skeleton` prints its golden hash.
#[test]
#[ignore = "M7 WI-14"]
fn replay_of_the_golden_scenario_prints_the_golden_hash() {
    let golden = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh"),
    )
    .unwrap_or_else(|e| panic!("gates.sh: {e}"))
    .lines()
    .find_map(|l| {
        l.trim_start_matches("GOLDENS='")
            .strip_prefix("walking_skeleton ")
    })
    .and_then(|rest| rest.split_whitespace().next().map(str::to_owned))
    .unwrap_or_else(|| panic!("no walking_skeleton golden in gates.sh"));
    let file = scratch("golden.umbrarec.jsonl");
    let scenario = example();
    let run = cli(&["run", scenario.as_str(), "--record", path_str(&file)]);
    assert_eq!(run.status.code(), Some(0), "stderr: {}", stderr(&run));

    let out = cli(&["replay", path_str(&file)]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(final_hash(&stdout(&out)), Some(golden));
}

/// ADR 0054: `--json` prints one JSON document that carries the final hash.
#[test]
#[ignore = "M7 WI-14"]
fn replay_json_is_one_document_with_the_final_hash() {
    let (file, hash) = recorded(&example(), "json");

    let out = cli(&["replay", path_str(&file), "--json"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(json_of(&out).to_string().contains(&hash));
}

/// ADR 0054: a changed hash record exits 1 with `replay diverged at tick N`.
#[test]
#[ignore = "M7 WI-14"]
fn a_mutated_hash_exits_1_and_names_the_tick() {
    let (file, _) = recorded(&example(), "diverge");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{e}"));
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let target = lines
        .iter()
        .position(|l| l.contains("\"type\":\"hash\""))
        .unwrap_or_else(|| panic!("no hash record"));
    let mut value: serde_json::Value =
        serde_json::from_str(&lines[target]).unwrap_or_else(|e| panic!("{e}"));
    let tick = value["tick"].as_u64().unwrap_or_else(|| panic!("tick"));
    value["hash"] = match &value["hash"] {
        serde_json::Value::String(h) => {
            let flipped = if h.ends_with('0') { "1" } else { "0" };
            serde_json::Value::String(format!("{}{flipped}", &h[..h.len() - 1]))
        }
        serde_json::Value::Number(n) => {
            serde_json::Value::from(n.as_u64().unwrap_or_else(|| panic!("u64")) ^ 1)
        }
        other => panic!("unexpected hash encoding {other}"),
    };
    lines[target] = value.to_string();
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap_or_else(|e| panic!("{e}"));

    let out = cli(&["replay", path_str(&file)]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains(&format!("replay diverged at tick {tick}")),
        "stderr: {}",
        stderr(&out)
    );
}

/// ASM-630: a missing recording is an input error (exit 2).
#[test]
#[ignore = "M7 WI-14"]
fn a_missing_recording_exits_2() {
    let out = cli(&[
        "replay",
        path_str(&scratch("does-not-exist.umbrarec.jsonl")),
    ]);

    assert_eq!(out.status.code(), Some(2), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("unrecognized subcommand"),
        "the command is missing"
    );
}

/// ASM-629, ASM-630: a scenario file that changed since the recording is refused with exit 2, and
/// `--allow-mismatch` goes on.
#[test]
#[ignore = "M7 WI-14"]
fn a_changed_scenario_file_exits_2_unless_mismatch_is_allowed() {
    let copy = scratch("changed-scenario");
    copy_dir(&example_dir(), &copy);
    let scenario = copy.join("scenario.toml");
    let (file, _) = recorded(path_str(&scenario), "changed");
    let text = std::fs::read_to_string(&scenario).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&scenario, format!("{text}\n# edited\n")).unwrap_or_else(|e| panic!("{e}"));

    let refused = cli(&["replay", path_str(&file)]);
    let allowed = cli(&["replay", path_str(&file), "--allow-mismatch"]);

    assert_eq!(
        refused.status.code(),
        Some(2),
        "stderr: {}",
        stderr(&refused)
    );
    assert!(
        stderr(&refused).contains("scenario.toml"),
        "names the file: {}",
        stderr(&refused)
    );
    assert_eq!(
        allowed.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&allowed)
    );
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap_or_else(|e| panic!("{e}"));
    for entry in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{e}")) {
        let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
        let target = to.join(path.file_name().unwrap_or_else(|| panic!("name")));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
