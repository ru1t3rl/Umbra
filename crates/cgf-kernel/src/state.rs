//! State hash and snapshot (REQ-K-4, REQ-K-6 partial, ADR 0007, ASM-036, ASM-039).

use cgf_core::{Ownership, SimTime, StateHash, StateHasher};

use crate::kernel::{EntitySnapshot, Snapshot};
use crate::world::World;

/// Hashes the simulation state in the ADR 0007 encoding (version 1).
///
/// Content: tick, tick rate (Hz), entity count; then per entity in ascending id order: id,
/// ownership (tag, plus the source string for remote), motion model id, position, velocity,
/// orientation, route state (tag 0 absent; tag 1 then waypoint count and next index) and RNG
/// state. Intents and wall timings are never hashed.
pub(crate) fn state_hash(time: SimTime, world: &World) -> StateHash {
    let mut h = StateHasher::new();
    h.write_u64(time.tick());
    h.write_u32(time.rate().hz());
    h.write_u64(world.len() as u64);
    world.for_each(|e| {
        h.write_u64(e.id.get());
        match e.ownership {
            Ownership::Owned => h.write_u8(0),
            Ownership::Remote { source } => {
                h.write_u8(1);
                h.write_str(source.as_str());
            }
        }
        h.write_str(e.motion.0.id().as_str());
        h.write_vec3(e.state.position.0);
        h.write_vec3(e.state.velocity);
        h.write_quaternion(e.state.orientation);
        match e.route {
            None => h.write_u8(0),
            Some(r) => {
                h.write_u8(1);
                h.write_u64(r.waypoints.len() as u64);
                h.write_u64(r.next as u64);
            }
        }
        for word in e.rng.0.state() {
            h.write_u64(word);
        }
    });
    h.finish()
}

/// An immutable copy of all entities, sorted by id, labelled with `time`.
pub(crate) fn snapshot(time: SimTime, world: &World) -> Snapshot {
    let mut entities = Vec::with_capacity(world.len());
    world.for_each(|e| {
        entities.push(EntitySnapshot {
            id: e.id,
            ownership: e.ownership.clone(),
            motion_model: e.motion.0.id().clone(),
            state: *e.state,
            route_next: e.route.map(|r| (r.next, r.waypoints.len())),
        });
    });
    Snapshot { time, entities }
}
