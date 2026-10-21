//! AT-20 (compatibility): a scenario without `[[network_adapters]]` creates no network link, so
//! the M5 behavior is untouched. Requirements: REQ-S-1, ADR 0030, ASM-224 (nothing runs around the
//! tick without a link). The golden hashes and the M2 fixtures are checked by `scripts/gates.sh` and
//! `scripts/m2-compat.sh`.

#![forbid(unsafe_code)]

mod common;

use common::{example_engine, run_silent};
use umbra_engine::RunUntil;

#[test]
fn scenarios_without_adapters_create_no_network_link() {
    let engine = example_engine(None, None);

    assert!(engine.network().is_none());
}

#[test]
fn a_scenario_without_adapters_still_has_no_link_after_running() {
    let mut engine = example_engine(None, None);

    run_silent(&mut engine, RunUntil::Ticks(30));

    assert!(engine.network().is_none());
    assert_eq!(engine.now().tick(), 30);
}
