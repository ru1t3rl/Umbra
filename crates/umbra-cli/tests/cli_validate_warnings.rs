//! `umbra validate --json` always carries a `warnings` array (M6 WI-8, ASM-290). The standard
//! registry has no network adapter yet, so a real warning is covered by the unit tests of the
//! document builder and by `umbra-templates` (`scenario_network.rs`).

#![forbid(unsafe_code)]

mod common;

use common::{cli, example, json_of, stderr, stdout};

#[test]
fn a_valid_example_has_an_empty_warnings_array_and_no_warning_on_stderr() {
    let out = cli(&["validate", "--json", &example()]);

    let doc = json_of(&out);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(doc["warnings"], serde_json::json!([]));
    assert!(!stderr(&out).contains("warning["), "{}", stderr(&out));
}

#[test]
fn the_text_output_of_a_valid_example_is_unchanged_by_warnings() {
    let out = cli(&["validate", &example()]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).starts_with("ok: "), "{}", stdout(&out));
}
