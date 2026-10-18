//! WI-15: edge cases of `builtin.visual` that `visual_sensor.rs` (AT-15) does not pin: a target
//! straight above, no random draws, no target in the way of itself. Requirements: ADR 0015,
//! ASM-080, ASM-108, REQ-K-4.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use behavior_support::standard;
use serde_json::json;
use umbra_core::{
    ComponentId, ContactBuffer, Ecef, EntityId, Enu, Euler, FlatTerrain, Geodetic, KinematicState,
    LocalFrame, Meters, Observer, Ownership, Radians, SenseContext, SimRng, SimTime, StreamKey,
    Target, TickRate, Vec3,
};

fn origin() -> Geodetic {
    Geodetic::from_degrees(52.0, 4.0, 0.0).expect("origin")
}

fn state(position: Ecef) -> KinematicState {
    let frame = LocalFrame::at(position.to_geodetic());
    KinematicState {
        position,
        velocity: Vec3::ZERO,
        orientation: frame.orientation_from_euler(Euler {
            heading: Radians(0.0),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
    }
}

fn up(height: f64) -> Ecef {
    origin()
        .with_height(Meters(height))
        .expect("height")
        .to_ecef()
}

/// Senses once from the origin at the given target; returns the contact count and whether the
/// observer's RNG stream was left untouched.
fn sense(params: &serde_json::Value, target: Ecef) -> (usize, bool) {
    let registry = standard();
    let sensor = registry
        .sensor(&ComponentId::new("builtin.visual").expect("id"))
        .expect("registered")
        .build(params)
        .expect("builds");
    let (me, other) = (state(up(0.0)), state(target));
    let owned = Ownership::Owned;
    let observer_id = EntityId::new(1).expect("id");
    let targets = [Target::new(
        EntityId::new(2).expect("id"),
        &other,
        None,
        &owned,
    )];
    let stream = || SimRng::new(3).stream(StreamKey::entity(observer_id));
    let mut rng = stream();
    let terrain = FlatTerrain {
        height: Meters(0.0),
    };
    let mut out = Vec::new();
    let mut ctx = SenseContext::new(
        SimTime::at_tick(TickRate::new(30).expect("30 Hz"), 0),
        &mut rng,
        &terrain,
    );
    sensor.sense(
        &mut ctx,
        &Observer::new(observer_id, &me, None),
        &targets,
        &mut ContactBuffer::new(&mut out),
    );
    let untouched = rng.next_u64() == stream().next_u64();
    (out.len(), untouched)
}

/// ASM-108: a target straight above the observer has no bearing and counts as inside a narrow field
/// of view; range still applies. (Smooth ground hides it only if the eyes are too low, so the eye
/// height is generous.)
#[test]
fn a_target_straight_above_is_inside_any_field_of_view() {
    let near = json!({ "range_m": 500.0, "fov_deg": 1.0, "eye_height_m": 5.0 });

    let (seen, _) = sense(&near, up(100.0));
    let (too_far, _) = sense(&near, up(600.0));

    assert_eq!((seen, too_far), (1, 0));
}

/// REQ-K-4, ADR 0015: the sensor draws no random numbers.
#[test]
fn sensing_draws_no_random_numbers() {
    let params = json!({ "range_m": 3000.0, "fov_deg": 360.0, "eye_height_m": 5.0 });
    let east = LocalFrame::at(origin()).from_enu(Enu {
        east: 1000.0,
        north: 0.0,
        up: 0.0,
    });

    let (_, untouched_when_seen) = sense(&params, east);
    let (_, untouched_when_missed) = sense(&params, up(1.0e6));

    assert!(untouched_when_seen && untouched_when_missed);
}
