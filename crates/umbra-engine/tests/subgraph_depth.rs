//! `validate` and run agree on `subgraph` depth (ASM-099 e): a chain of file graphs into a registered
//! graph counts the registered graph's own chain, at load and at instantiate.
//! Requirements: REQ-B-12, REQ-T-2, ASM-073, ASM-099.
//!
//! `builtin.fsm.patrol_and_engage` calls `builtin.subgraph.engage_target`, so it is a chain of 2:
//! 14 file graphs ending in it are exactly the limit of 16, 15 are 17.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use core::time::Duration;
use std::path::{Path, PathBuf};

use behavior_support::{fixture_dir, instantiate_with, standard};
use umbra_engine::scenario::{ScenarioError, load_scenario, validate_path};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy");
        }
    }
}

/// The swap fixture `b` with blue-1 on `test.depth.d00 -> ... -> builtin.fsm.patrol_and_engage`
/// through `n` file graphs.
fn scenario_with_chain(name: &str, n: usize) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    drop(std::fs::remove_dir_all(&dir));
    copy_dir(&fixture_dir("behavior_swap/b"), &dir);
    let template = dir.join("templates/tank_blue.toml");
    let text = std::fs::read_to_string(&template).expect("template");
    let text = text.replace("example.behavior.follow_route", "test.depth.d00");
    std::fs::write(&template, text).expect("write");
    for i in 0..n {
        let target = if i + 1 == n {
            "builtin.fsm.patrol_and_engage".to_string()
        } else {
            format!("test.depth.d{:02}", i + 1)
        };
        let graph = format!(
            "schema_version = 1\nid = \"test.depth.d{i:02}\"\nversion = \"1.0.0\"\nroot = \"call\"\n\n\
             [nodes.call]\ntype = \"subgraph\"\nref = \"{target}\"\n"
        );
        std::fs::write(dir.join(format!("behaviors/d{i:02}.behavior.toml")), graph).expect("write");
    }
    dir.join("scenario.toml")
}

/// Exactly the limit: load (validate) accepts and instantiate succeeds.
#[test]
fn validate_ok_implies_instantiate_ok_at_exactly_the_limit() {
    let scenario = load_scenario(&standard(), &scenario_with_chain("depth_ok", 14))
        .unwrap_or_else(|e| panic!("validate: {e}"));

    let run = instantiate_with(&scenario, Duration::from_micros(500), None, None);

    assert_eq!(run.entities.len(), 3);
}

/// One more file graph: `validate` and the scenario load reject it with a located
/// `subgraph_too_deep` at the call of the last file graph, with a note naming the registered chain.
#[test]
fn one_level_more_is_rejected_at_load_with_a_located_error() {
    let scenario = scenario_with_chain("depth_over", 15);
    let dir = scenario.parent().expect("directory");

    let report = validate_path(&standard(), dir).expect("validate runs");
    let loaded = load_scenario(&standard(), &scenario);

    let Err(ScenarioError::Invalid(from_load)) = loaded else {
        panic!("expected ScenarioError::Invalid");
    };
    for found in [report.diagnostics, from_load.into_vec()] {
        assert_eq!(found.len(), 1, "{found:?}");
        let d = &found[0];
        assert_eq!(d.code.as_str(), "subgraph_too_deep");
        let at = d.location.as_ref().expect("located");
        assert!(at.path.ends_with("behaviors/d14.behavior.toml"), "{at:?}");
        let notes = format!("{:?}", d.notes);
        assert!(notes.contains("builtin.subgraph.engage_target"), "{notes}");
    }
}
