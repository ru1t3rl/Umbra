//! AT-2: the `cgf-cli demo` command, run as a real process. Requirements: REQ-X-2 (minimal),
//! REQ-X-3, REQ-K-4 (cross-process determinism), ASM-027.
//!
//! Two processes have different per-process hasher seeds and address-space layouts, so equal state
//! hashes across processes catch nondeterminism that an in-process comparison cannot (hidden
//! `RandomState`, pointer-ordered iteration).

#![forbid(unsafe_code)]

use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgf-cli"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run cgf-cli: {e}"))
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The value after `state_hash=` on the last text line.
fn final_hash(out: &str) -> Option<String> {
    out.lines()
        .rev()
        .find_map(|l| l.strip_prefix("state_hash="))
        .map(str::to_string)
}

fn is_16_lower_hex(s: &str) -> bool {
    s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// REQ-K-4: two separate processes running the same demo end with the same state hash.
#[test]
fn given_demo_when_cli_runs_twice_then_final_state_hash_is_identical() {
    let args = ["demo", "--ticks", "300", "--seed", "42"];

    let first = cli(&args);
    let second = cli(&args);

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

/// REQ-K-4: the seed reaches the simulation, so another seed ends with another hash.
#[test]
fn given_different_seeds_then_final_state_hashes_differ() {
    let a = cli(&["demo", "--ticks", "120", "--seed", "1"]);
    let b = cli(&["demo", "--ticks", "120", "--seed", "2"]);

    assert_ne!(final_hash(&stdout(&a)), None);
    assert_ne!(final_hash(&stdout(&a)), final_hash(&stdout(&b)));
}

/// REQ-X-2: the default invocation succeeds and ends with a state hash.
#[test]
fn given_demo_with_defaults_then_exits_zero_with_a_hash() {
    let out = cli(&["demo"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(final_hash(&stdout(&out)).is_some_and(|h| is_16_lower_hex(&h)));
}

/// ASM-027: state lines have the documented shape
/// `tick=30 t=1.000s id=1 lat=.. lon=.. h=.. heading_deg=.. speed_mps=.. waypoint=i/n`.
#[test]
fn given_demo_then_prints_state_lines_for_entity_1() {
    let out = cli(&["demo", "--ticks", "60", "--print-every", "30"]);
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
    let waypoint = line.rsplit("waypoint=").next().unwrap_or_default();
    assert!(waypoint.contains('/'), "waypoint is next/len: {line}");
}

/// ASM-027: `--print-every` thins the output; every printed tick covers all three entities.
#[test]
fn given_print_every_then_each_printed_tick_lists_all_three_entities() {
    let out = cli(&["demo", "--ticks", "90", "--print-every", "30"]);
    let text = stdout(&out);

    let at_60: Vec<&str> = text.lines().filter(|l| l.starts_with("tick=60 ")).collect();
    let at_45 = text.lines().filter(|l| l.starts_with("tick=45 ")).count();

    assert_eq!(at_60.len(), 3, "{text}");
    assert!(
        at_60[0].contains(" id=1 ") && at_60[1].contains(" id=2 ") && at_60[2].contains(" id=3 ")
    );
    assert_eq!(at_45, 0);
}

/// ASM-027: `--json` prints JSON Lines: every non-empty line parses, entity lines carry the entity
/// fields and the last object carries `tick` and `state_hash`.
#[test]
fn given_json_flag_then_every_line_parses_and_last_has_state_hash() {
    let out = cli(&["demo", "--ticks", "60", "--print-every", "30", "--json"]);
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
    assert!(
        parsed.iter().any(|v| v["id"] == 1),
        "an entity line for id 1: {text}"
    );
}

/// ASM-027: the JSON hash equals the text hash for the same run.
#[test]
fn given_json_and_text_runs_then_final_hashes_agree() {
    let text = cli(&["demo", "--ticks", "100", "--seed", "9"]);
    let json = cli(&["demo", "--ticks", "100", "--seed", "9", "--json"]);

    let last_json = stdout(&json)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok());

    let json_hash = last_json.and_then(|v| v["state_hash"].as_str().map(str::to_string));
    assert!(json_hash.is_some(), "no JSON hash");
    assert_eq!(json_hash, final_hash(&stdout(&text)));
}

/// REQ-X-2: an unknown clock mode is a usage error (exit code 2).
#[test]
fn given_bad_clock_arg_then_exit_code_2() {
    let out = cli(&["demo", "--clock", "warp"]);

    assert_eq!(out.status.code(), Some(2), "stdout: {}", stdout(&out));
}

/// REQ-X-2, REQ-K-3: a zero time scale is rejected with exit code 2 and a message on stderr.
#[test]
fn given_scaled_zero_then_exit_code_2_with_message() {
    let out = cli(&["demo", "--clock", "scaled=0"]);

    assert_eq!(out.status.code(), Some(2), "stdout: {}", stdout(&out));
    assert!(
        !stderr(&out).trim().is_empty(),
        "an error message is expected on stderr"
    );
}

/// REQ-X-2: other malformed scales and unknown flags are usage errors too.
#[test]
fn given_malformed_scale_or_unknown_flag_then_exit_code_2() {
    for args in [
        &["demo", "--clock", "scaled=-1"][..],
        &["demo", "--clock", "scaled=abc"][..],
        &["demo", "--no-such-flag"][..],
        &["demo", "--ticks", "many"][..],
    ] {
        assert_eq!(cli(args).status.code(), Some(2), "{args:?}");
    }
}

/// REQ-K-3, REQ-K-4: every accepted clock mode ends with the same hash as `afap` (the CLI
/// stepped and scaled modes run without real waiting in this short run).
#[test]
fn given_each_clock_mode_then_final_hash_matches_afap() {
    let reference = final_hash(&stdout(&cli(&["demo", "--ticks", "60", "--clock", "afap"])));

    assert!(reference.is_some(), "no reference hash");
    for mode in ["stepped", "scaled=1000"] {
        let out = cli(&["demo", "--ticks", "60", "--clock", mode]);
        assert_eq!(out.status.code(), Some(0), "{mode}: {}", stderr(&out));
        assert_eq!(final_hash(&stdout(&out)), reference, "{mode}");
    }
}
