//! AT-16 (M7 WI-2, motion rows): the host bridge of the motion model kind (ABI 0.2, ADR 0050; REQ-P-4,
//! REQ-P-5, REQ-M-1, REQ-M-4, REQ-C-6, ASM-612, ASM-613) through the SDK-built fixture plugin
//! `test.plugin.kinds`: a plugin motion model passes the core `motion::contract` suite, steps like the
//! reference, sees the terrain of the call exactly as the provider answers, and one model's fault
//! leaves another working. The fault rows (output the host refuses, panics) are in `plugin_failures.rs`
//! (gate 13).
//!
//! Raw-ABI misbehavior the safe SDK cannot produce (a stale or foreign terrain token) is written with
//! the raw fixtures in WI-8, as are the fault policies (`abort` ends the run after the tick).
//!
//! Step 4 of M7 (WI-2): ignored until WI-8 (the SDK stub `Capabilities::motion_model` panics with
//! `M7 WI-10` first, so the rows also need WI-10; they are listed under WI-8 because the bridge is
//! what they test).

#[macro_use]
mod support;

use support::kinds::{DT, Slope, Wall, env, state_at, step};
use umbra_core::motion::contract::check_motion_model;
use umbra_core::{Ecef, Geodetic, Intent, MetersPerSecond, TerrainProvider};

fn distance(a: Ecef, b: Ecef) -> f64 {
    (a - b).norm()
}

#[test]
#[ignore = "M7 WI-8"]
fn given_the_coast_model_when_checked_by_the_core_contract_then_it_passes() {
    let env = env();
    let model = env.motion("test.motion.coast", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };
    let cases = vec![
        (
            state_at(4.0, 50.0, 12.0),
            vec![Intent::SetSpeed(MetersPerSecond(5.0))],
        ),
        (state_at(4.0, 50.0, 0.0), vec![]),
        (state_at(4.0, 50.0, 0.0), vec![Intent::Hold]),
    ];

    let verdict = check_motion_model(model.as_ref(), &cases, DT, &terrain);

    assert_eq!(verdict, Ok(()));
    assert_eq!(env.host.faults(), Vec::new());
}

#[test]
#[ignore = "M7 WI-8"]
fn given_the_coast_model_when_stepped_then_position_advances_by_velocity_times_dt() {
    let env = env();
    let model = env.motion("test.motion.coast", "{}").expect("build");
    let start = state_at(4.0, 50.0, 12.0);

    let next = step(model.as_ref(), &start, &[], &Slope { per_rad: 0.0 });

    let want = start.position + start.velocity * DT.0;
    assert!(distance(next.position, want) < 1e-9);
    assert_eq!(next.velocity, start.velocity);
    assert_eq!(next.orientation, start.orientation);
}

/// REQ-C-6, ADR 0050: terrain answers equal the provider's (height, normal, line of sight).
#[test]
#[ignore = "M7 WI-8"]
fn given_the_terrain_echo_model_when_stepped_then_it_reports_what_the_provider_answers() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };
    let start = state_at(4.0, 500.0, 0.0);
    let at = start.position.to_geodetic();

    let next = step(model.as_ref(), &start, &[], &terrain);

    assert_eq!(next.position, start.position);
    assert_eq!(next.velocity.x, terrain.height_at(at).0);
    assert_eq!(next.velocity.y, terrain.surface_normal_at(at).up);
    assert_eq!(next.velocity.z, 1.0, "line of sight above gentle terrain");
}

#[test]
#[ignore = "M7 WI-8"]
fn given_a_wall_when_the_echo_model_looks_up_from_inside_it_then_line_of_sight_is_blocked() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let lon = 4.0_f64.to_radians();
    let wall = Wall {
        lon_lo: lon - 0.001,
        lon_hi: lon + 0.001,
    };
    let start = state_at(4.0, 10.0, 0.0);

    let next = step(model.as_ref(), &start, &[], &wall);

    assert_eq!(next.velocity.x, 300.0);
    assert_eq!(next.velocity.z, 0.0);
}

/// The query ignores the point's own height (the `TerrainProvider` contract).
#[test]
#[ignore = "M7 WI-8"]
fn given_two_heights_over_one_point_when_echoed_then_the_terrain_height_is_the_same() {
    let env = env();
    let model = env.motion("test.motion.terrain_echo", "{}").expect("build");
    let terrain = Slope { per_rad: 1000.0 };

    let heights: Vec<f64> = [10.0, 9000.0]
        .into_iter()
        .map(|alt| {
            step(model.as_ref(), &state_at(4.0, alt, 0.0), &[], &terrain)
                .velocity
                .x
        })
        .collect();

    let ground = terrain
        .height_at(Geodetic::from_degrees(52.0, 4.0, 0.0).expect("valid"))
        .0;
    assert_eq!(heights, [ground, ground]);
}
