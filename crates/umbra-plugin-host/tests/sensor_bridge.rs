//! AT-16 (M7 WI-2, sensor rows): the host bridge of the sensor kind (ABI 0.2, ADR 0050; REQ-P-4,
//! REQ-P-5, REQ-B-5, REQ-K-4, ASM-612) through the SDK-built fixture plugin `test.plugin.kinds`: a
//! plugin sensor sees the candidates in the host's order with their pre-tick state, terrain line of
//! sight equals the provider's, and its draws advance the observer's own stream exactly like a
//! built-in's. The fault rows (a contact naming a non-candidate, a panic) are in `plugin_failures.rs`
//! (gate 13).
//!
//! Step 4 of M7 (WI-2): ignored until WI-8 (and WI-10 for the SDK side, see `motion_bridge.rs`).

#[macro_use]
mod support;

use support::kinds::{Wall, env, flat, ids, sense, stream, world};
use umbra_core::TerrainProvider;

#[test]
#[ignore = "M7 WI-8"]
fn given_the_all_sensor_when_sensing_then_every_candidate_is_a_contact_with_its_pre_tick_state() {
    let env = env();
    let sensor = env.sensor("test.sensor.all", "{}").expect("build");
    let world = world();

    let (contacts, _) = sense(sensor.as_ref(), &world, &flat());

    assert_eq!(ids(&contacts), [2, 3, 4]);
    for (contact, (id, state)) in contacts.iter().zip(&world[1..]) {
        assert_eq!(contact.entity, *id);
        assert_eq!(contact.position, state.position);
        assert_eq!(contact.velocity, state.velocity);
        assert_eq!(
            contact.range.0,
            (state.position - world[0].1.position).norm()
        );
    }
    assert_eq!(env.host.faults(), Vec::new());
}

/// REQ-C-6, ADR 0050: line of sight through the service equals the provider's (a wall between the
/// observer and the farthest candidate blocks only that one).
#[test]
#[ignore = "M7 WI-8"]
fn given_a_wall_when_the_los_sensor_senses_then_only_visible_candidates_are_contacts() {
    let env = env();
    let sensor = env.sensor("test.sensor.los", "{}").expect("build");
    let world = world();
    let wall = Wall {
        lon_lo: 4.0025_f64.to_radians(),
        lon_hi: 4.0027_f64.to_radians(),
    };
    let want: Vec<u64> = world[1..]
        .iter()
        .filter(|(_, s)| wall.line_of_sight(world[0].1.position, s.position))
        .map(|(id, _)| id.get())
        .collect();

    let (contacts, _) = sense(sensor.as_ref(), &world, &wall);

    assert_eq!(want, [2, 3], "the wall hides the candidate beyond it");
    assert_eq!(ids(&contacts), want);
}

/// REQ-K-4, ADR 0050: the draws go through the observer's own stream: the contacts are the candidates
/// whose draw is below 0.5, in candidate order, and the stream ends where the reference stream ends.
#[test]
#[ignore = "M7 WI-8"]
fn given_the_coin_sensor_when_sensing_then_it_draws_from_the_observers_stream_in_candidate_order() {
    let env = env();
    let sensor = env.sensor("test.sensor.coin", "{}").expect("build");
    let world = world();
    let mut reference = stream(1);
    let want: Vec<u64> = world[1..]
        .iter()
        .filter(|_| reference.next_f64() < 0.5)
        .map(|(id, _)| id.get())
        .collect();

    let (contacts, after) = sense(sensor.as_ref(), &world, &flat());

    assert_eq!(ids(&contacts), want);
    assert_eq!(after.state(), reference.state());
}
