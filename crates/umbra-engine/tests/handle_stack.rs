//! AT-4 (engine thread stack; M5 WI-8): the behavior interpreter recurses, so the engine thread has
//! at least 4 MiB (ASM-098, ADR 0024). Requirements: REQ-X-1, ASM-098.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod handle_support;
mod plugin_support;

use std::path::{Path, PathBuf};

use handle_support::{config, load_stepped, start_engine};
use umbra_engine::handle::{EngineHandle, MIN_STACK_SIZE, SpawnError};

#[test]
#[ignore = "WI-8"]
fn stack_below_4_mib_is_refused() {
    let mut small = config();
    small.stack_size = MIN_STACK_SIZE - 1;

    let result = EngineHandle::spawn(small);

    let Err(SpawnError::StackTooSmall { requested, minimum }) = result else {
        panic!("expected StackTooSmall");
    };
    assert_eq!((requested, minimum), (MIN_STACK_SIZE - 1, MIN_STACK_SIZE));
}

#[test]
#[ignore = "WI-8"]
fn stack_of_exactly_4_mib_is_accepted() {
    // extra: the limit is inclusive.
    let mut exact = config();
    exact.stack_size = MIN_STACK_SIZE;

    let (handle, _thread) = start_engine(exact);

    assert!(handle.status().is_ok());
}

/// The swap fixture `b` with blue-1 on `test.depth.d00 -> ... -> builtin.fsm.patrol_and_engage`
/// through `n` file graphs (as in `subgraph_depth.rs`; 14 is the longest chain the loader allows).
fn scenario_with_chain(n: usize) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/behavior_swap/b");
    let dir = plugin_support::scratch_copy(&fixture, "stack-chain");
    let template = dir.join("templates/tank_blue.toml");
    let text = std::fs::read_to_string(&template).expect("template");
    std::fs::write(
        &template,
        text.replace("example.behavior.follow_route", "test.depth.d00"),
    )
    .expect("write");
    for i in 0..n {
        let target = if i + 1 == n {
            "builtin.fsm.patrol_and_engage".to_string()
        } else {
            format!("test.depth.d{:02}", i + 1)
        };
        let graph = plugin_support::call_graph(&format!("test.depth.d{i:02}"), &target);
        std::fs::write(dir.join(format!("behaviors/d{i:02}.behavior.toml")), graph).expect("write");
    }
    dir.join("scenario.toml")
}

#[test]
#[ignore = "WI-9"]
fn deepest_allowed_graph_runs_on_a_4_mib_engine_thread() {
    let mut smallest = config();
    smallest.stack_size = MIN_STACK_SIZE;
    let (handle, _thread) = start_engine(smallest);
    load_stepped(&handle, scenario_with_chain(14));
    handle.start().expect("start");

    let outcome = handle.step(5).expect("the deepest allowed graph ticks");

    assert!(outcome.completed);
    assert_eq!(outcome.reached.tick(), 5);
}
