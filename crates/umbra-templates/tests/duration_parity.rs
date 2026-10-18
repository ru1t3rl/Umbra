//! M3 WI-2: drift guard between the scenario reader's `read_duration` (M2) and
//! `TickRate::ticks_for` (ASM-078, ASM-066). Both must agree for every duration.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::num::NonZeroU64;

use common::{alpha_template, lines, scenario, src};
use proptest::prelude::*;
use umbra_core::{Seconds, TickRate};
use umbra_templates::SourceFile;

fn template() -> SourceFile {
    src(
        "scn/templates/veh.toml",
        &alpha_template("veh.base", None, "2.0"),
    )
}

/// `duration_ticks` from the scenario reader, or `None` when the scenario is rejected.
fn reader_ticks(hz: u32, d: f64) -> Option<u64> {
    let text = lines(&[
        "schema_version = 1",
        "",
        "[scenario]",
        "name = \"test\"",
        "seed = 7",
        &format!("tick_rate_hz = {hz}"),
        "clock = \"afap\"",
        &format!("duration_s = {d:?}"),
        "template_paths = [\"templates\"]",
        "",
        "[[entities]]",
        "template = \"veh.base\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
    ]);
    scenario(&src("scn/scenario.toml", &text), &[template()])
        .ok()
        .map(|s| s.duration_ticks)
}

fn core_ticks(hz: u32, d: f64) -> Option<u64> {
    let rate = TickRate::new(hz).expect("non-zero");
    rate.ticks_for(Seconds(d)).map(NonZeroU64::get)
}

fn assert_parity(hz: u32, d: f64) {
    assert_eq!(reader_ticks(hz, d), core_ticks(hz, d), "hz {hz}, d {d:?}");
}

#[test]
fn explicit_guard_and_boundary_values_agree() {
    let three = 3.0_f64;
    let durations = [
        0.1,
        2.5,
        10.01,
        0.01,
        1e-12,
        f64::from_bits(1),
        three,
        three.next_up().next_up(),
        three.next_up().next_up().next_up(),
        18_446_744_073_709_549_568.0,
        18_446_744_073_709_551_616.0,
        1e300,
        f64::MAX,
    ];
    for hz in [1, 30, 60, 1000] {
        for d in durations {
            assert_parity(hz, d);
        }
    }
}

proptest! {
    #[test]
    fn reader_and_ticks_for_agree(hz in 1u32..=1000, d in 1e-9f64..1e6) {
        assert_parity(hz, d);
    }

    #[test]
    fn agree_near_whole_tick_products(hz in 1u32..=1000, k in 1u32..100_000, ulps in 0u32..6) {
        let mut d = f64::from(k) / f64::from(hz);
        for _ in 0..ulps {
            d = d.next_up();
        }
        assert_parity(hz, d);
    }
}
