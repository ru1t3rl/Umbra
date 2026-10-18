//! AT-8 (CLI rows, ported M1 AT-2): `umbra run <scenario.toml>`, run as a real process.
//! Requirements: M2 acceptance 2, REQ-S-1, REQ-X-2, REQ-X-3, REQ-K-4 (cross-process determinism),
//! ASM-062.
//!
//! Two processes have different per-process hasher seeds and address-space layouts, so equal state
//! hashes across processes catch nondeterminism that an in-process comparison cannot (hidden
//! `RandomState`, pointer-ordered iteration). State lines are
//! `tick=30 t=1.000s id=1 name=base-1 lat=.. lon=.. h=.. heading_deg=.. speed_mps=.. waypoint=i/n`.
//! WI-11 deletes the demo-based `cli.rs`; this file replaces it.

#![forbid(unsafe_code)]

mod common;

use std::process::Output;

use common::{assert_usage_error, cli, example, final_hash, is_16_lower_hex, stderr, stdout};

fn run(extra: &[&str]) -> Output {
    let scenario = example();
    let mut args = vec!["run", scenario.as_str()];
    args.extend_from_slice(extra);
    cli(&args)
}

/// REQ-K-4: two separate processes running the example end with the same state hash.
#[test]
fn given_example_when_cli_runs_twice_then_final_state_hash_is_identical() {
    let first = run(&["--ticks", "300"]);
    let second = run(&["--ticks", "300"]);

    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert_eq!(second.status.code(), Some(0));
    let (a, b) = (final_hash(&stdout(&first)), final_hash(&stdout(&second)));
    assert!(
        a.as_deref().is_some_and(is_16_lower_hex),
        "no 16-digit hash in {:?}",
        stdout(&first)
    );
    assert_eq!(a, b);
}

/// REQ-K-4: four separate processes print byte-identical output, with every entity line and the
/// state hash after every tick, so a divergence anywhere in the run shows up even if it later
/// cancels out.
#[test]
fn given_example_when_run_in_four_processes_then_stdout_is_byte_identical() {
    let args = [
        "--json",
        "--print-every",
        "1",
        "--hash-every",
        "1",
        "--ticks",
        "300",
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
    let final_lines = text
        .lines()
        .filter(|l| l.contains("\"final\":true"))
        .count();
    assert_eq!(hash_lines, 301, "one hash per tick plus the final line");
    assert_eq!(final_lines, 1, "only the last hash line is marked final");
    for r in &runs[1..] {
        assert_eq!(r.stdout, runs[0].stdout);
    }
}

/// The scenario's seed can be overridden; another seed gives another final hash (the example's
/// motion does not depend on the seed, ASM-061, but the hashed RNG state does).
#[test]
fn given_different_seeds_then_final_state_hashes_differ() {
    let a = run(&["--ticks", "120", "--seed", "1"]);
    let b = run(&["--ticks", "120", "--seed", "2"]);

    assert_ne!(final_hash(&stdout(&a)), None);
    assert_ne!(final_hash(&stdout(&a)), final_hash(&stdout(&b)));
}

#[test]
fn given_seed_equal_to_the_scenario_seed_then_the_hash_is_the_default_one() {
    let default = run(&["--ticks", "60"]);
    let explicit = run(&["--ticks", "60", "--seed", "42"]);

    assert!(final_hash(&stdout(&default)).is_some_and(|h| is_16_lower_hex(&h)));
    assert_eq!(
        final_hash(&stdout(&default)),
        final_hash(&stdout(&explicit))
    );
}

/// REQ-S-1: with no flags the scenario runs for its own `duration_s` (240 s at 30 Hz = 7200 ticks).
#[test]
fn given_no_ticks_flag_then_the_scenario_duration_is_run() {
    let out = run(&["--json"]);

    let text = stdout(&out);
    let last: serde_json::Value = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| panic!("no JSON last line in:\n{text}"));
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(last["tick"], 7200);
}

#[test]
fn given_ticks_flag_then_it_overrides_the_scenario_duration() {
    let out = run(&["--json", "--ticks", "90"]);

    let text = stdout(&out);
    let last: serde_json::Value = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str(l).ok())
        .unwrap_or_else(|| panic!("no JSON last line in:\n{text}"));
    assert_eq!(last["tick"], 90);
}

#[test]
fn given_example_then_prints_state_lines_with_names() {
    let out = run(&["--ticks", "60", "--print-every", "30"]);
    let text = stdout(&out);

    let line = text
        .lines()
        .find(|l| l.starts_with("tick=30 ") && l.contains(" id=1 "));

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let line = line.unwrap_or_else(|| panic!("no tick=30 line for entity 1 in:\n{text}"));
    let keys: Vec<&str> = line
        .split(' ')
        .filter_map(|t| t.split('=').next())
        .collect();
    assert_eq!(
        keys,
        [
            "tick",
            "t",
            "id",
            "name",
            "lat",
            "lon",
            "h",
            "heading_deg",
            "speed_mps",
            "waypoint"
        ],
        "{line}"
    );
    assert!(line.contains(" t=1.000s "), "{line}");
    assert!(line.contains(" name=base-1 "), "{line}");
    let waypoint = line.rsplit("waypoint=").next().unwrap_or_default();
    assert!(waypoint.contains('/'), "waypoint is next/len: {line}");
}

#[test]
fn given_print_every_then_each_printed_tick_lists_all_three_entities_by_name() {
    let out = run(&["--ticks", "90", "--print-every", "30"]);
    let text = stdout(&out);

    let at_60: Vec<&str> = text.lines().filter(|l| l.starts_with("tick=60 ")).collect();
    let at_45 = text.lines().filter(|l| l.starts_with("tick=45 ")).count();

    assert_eq!(at_60.len(), 3, "{text}");
    assert!(at_60[0].contains(" id=1 name=base-1 "));
    assert!(at_60[1].contains(" id=2 name=tank-1 "));
    assert!(at_60[2].contains(" id=3 name=truck-1 "));
    assert_eq!(at_45, 0);
}

#[test]
fn given_json_then_every_line_parses_and_last_has_state_hash() {
    let out = run(&["--ticks", "60", "--print-every", "30", "--json"]);
    let text = stdout(&out);

    let parsed: Vec<serde_json::Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad JSON line {l:?}: {e}")))
        .collect();

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(parsed.len() >= 2, "{text}");
    let last = &parsed[parsed.len() - 1];
    assert_eq!(last["tick"], 60);
    assert!(
        last["state_hash"].as_str().is_some_and(is_16_lower_hex),
        "{last}"
    );
    let first_entity = parsed.iter().find(|v| v["id"] == 1);
    assert!(
        first_entity.is_some_and(|v| v["name"] == "base-1"),
        "an entity line for id 1: {text}"
    );
}

#[test]
fn given_json_and_text_runs_then_final_hashes_agree() {
    let text = run(&["--ticks", "100", "--seed", "9"]);
    let json = run(&["--ticks", "100", "--seed", "9", "--json"]);

    let last_json = stdout(&json)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok());

    let json_hash = last_json.and_then(|v| v["state_hash"].as_str().map(str::to_string));
    assert!(json_hash.is_some(), "no JSON hash");
    assert_eq!(json_hash, final_hash(&stdout(&text)));
}

/// REQ-K-3, REQ-K-4: every accepted clock mode ends with the same hash as `afap`.
#[test]
fn given_each_clock_mode_then_final_hash_matches_afap() {
    let reference = final_hash(&stdout(&run(&["--ticks", "60", "--clock", "afap"])));

    assert!(reference.is_some(), "no reference hash");
    for mode in ["stepped", "scaled=1000"] {
        let out = run(&["--ticks", "60", "--clock", mode]);
        assert_eq!(out.status.code(), Some(0), "{mode}: {}", stderr(&out));
        assert_eq!(final_hash(&stdout(&out)), reference, "{mode}");
    }
}

// --- invalid input and usage errors --------------------------------------------------------------

/// REQ-T-2 in `run`: an invalid scenario simulates nothing; every diagnostic goes to stderr and the
/// exit code is 1.
#[test]
fn given_invalid_scenario_then_exit_1_with_diagnostics_on_stderr_and_no_state() {
    let path = "tests/fixtures/bad_scenario/scenario.toml";

    let out = cli(&["run", path]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "", "nothing is simulated or printed");
    let err = stderr(&out);
    assert!(
        err.contains(&format!("{path}:8:16: error[invalid_value]")),
        "{err}"
    );
    assert!(
        err.contains(&format!("{path}:14:12: error[unknown_template]")),
        "{err}"
    );
}

#[test]
fn given_missing_scenario_file_then_exit_1_with_io_on_stderr() {
    let out = cli(&["run", "tests/fixtures/no_such_scenario.toml"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("error[io]"), "{}", stderr(&out));
}

#[test]
fn given_a_template_file_instead_of_a_scenario_then_exit_1_not_a_scenario() {
    let out = cli(&["run", "tests/fixtures/bad/e_unknown_model.toml"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("error[not_a_scenario]"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn given_run_without_a_scenario_then_exit_2() {
    assert_usage_error(&cli(&["run"]), "required");
}

/// REQ-X-2: an unknown clock mode is a usage error (exit code 2).
#[test]
fn given_bad_clock_arg_then_exit_2() {
    assert_usage_error(&run(&["--clock", "warp"]), "warp");
}

#[test]
fn given_scaled_zero_then_exit_2_with_message() {
    assert_usage_error(&run(&["--clock", "scaled=0"]), "scaled=0");
}

#[test]
fn given_malformed_flags_then_exit_2() {
    for (extra, mentions) in [
        (&["--clock", "scaled=-1"][..], "scaled=-1"),
        (&["--clock", "scaled=abc"][..], "scaled=abc"),
        (&["--no-such-flag"][..], "--no-such-flag"),
        (&["--ticks", "many"][..], "many"),
        (&["--log-level", "bogus"][..], "bogus"),
        (&["--clock", "scaled=0.0001"][..], "scaled=0.0001"),
        (&["--clock", "scaled=10000000"][..], "scaled=10000000"),
        (&["--hash-every", "x"][..], "--hash-every"),
        (&["--seed", "-1"][..], "-1"),
    ] {
        assert_usage_error(&run(extra), mentions);
    }
}

/// Human decision 2026-10-17: `umbra demo` is removed; the demo is the example scenario.
#[test]
fn given_demo_subcommand_then_exit_2() {
    let out = cli(&["demo"]);

    assert_eq!(out.status.code(), Some(2), "stdout: {}", stdout(&out));
    assert_eq!(stdout(&out), "");
    assert!(
        stderr(&out).contains("unrecognized subcommand 'demo'"),
        "{}",
        stderr(&out)
    );
}

/// M1 carry-over closed: a write error (here a closed stdout pipe) stops the run instead of
/// simulating to the end, and the exit code is 1 with the cause on stderr. 7200 ticks at one line
/// per entity per tick is far more than a pipe buffer holds, so only an early stop is fast.
#[test]
fn given_closed_stdout_pipe_then_the_run_stops_with_exit_1() {
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_umbra"))
        .args(["run", example().as_str(), "--print-every", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn: {e}"));
    drop(child.stdout.take());

    let out = child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("wait: {e}"));

    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("writing output"),
        "stderr: {}",
        stderr(&out)
    );
}

/// JSON mode marks the last line as the run's final state hash, and only that line.
#[test]
fn given_json_hash_every_then_only_the_last_hash_line_is_marked_final() {
    let out = run(&[
        "--ticks",
        "30",
        "--json",
        "--hash-every",
        "10",
        "--print-every",
        "0",
    ]);

    let lines: Vec<serde_json::Value> = stdout(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l:?}: {e}")))
        .collect();

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(lines.len(), 4);
    for line in &lines[..3] {
        assert!(line.get("final").is_none(), "{line}");
    }
    assert_eq!(lines[3]["final"], true);
    assert_eq!(lines[3]["tick"], 30);
}
