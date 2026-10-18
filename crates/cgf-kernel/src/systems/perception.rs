//! The Perception phase system (REQ-K-2, REQ-K-5, ADR 0015, ASM-080).
//!
//! Sensors run for Owned entities in id order against copies of every other entity's pre-tick
//! state (Owned and Remote). Each observer has its own contact vector and RNG stream, so the
//! outcome does not depend on the visiting order.

use cgf_core::{
    Contact, ContactBuffer, EntityId, Force, KinematicState, Observer, Ownership, SenseContext,
    Target,
};

use crate::pipeline::TickContext;
use crate::world::World;

/// A candidate as it was before the tick.
struct Row {
    id: EntityId,
    state: KinematicState,
    force: Option<Force>,
    ownership: Ownership,
}

/// Buffers reused across ticks (ASM-093 (f)); cleared at the start of every run.
#[derive(Default)]
pub(crate) struct Scratch {
    rows: Vec<Row>,
    sensed: Vec<Contact>,
}

/// Recomputes the contacts of every Owned entity that has sensors.
pub(crate) fn run(world: &mut World, ctx: &TickContext<'_>) {
    if !world.has_sensors() {
        return;
    }
    // Taken out of the world so the rows can be borrowed while the observers are visited.
    let mut scratch = std::mem::take(&mut world.perception);
    sense_all(world, &mut scratch, ctx);
    world.perception = scratch;
}

fn sense_all(world: &mut World, scratch: &mut Scratch, ctx: &TickContext<'_>) {
    let Scratch { rows, sensed } = scratch;
    rows.clear();
    world.for_each(|e| {
        rows.push(Row {
            id: e.id,
            state: *e.state,
            force: e.force.map(|f| f.0.clone()),
            ownership: e.ownership.clone(),
        });
    });
    let mut candidates: Vec<Target<'_>> = Vec::with_capacity(rows.len());
    world.for_each_observer_mut(|o| {
        candidates.clear();
        candidates.extend(
            rows.iter()
                .filter(|r| r.id != o.id)
                .map(|r| Target::new(r.id, &r.state, r.force.as_ref(), &r.ownership)),
        );
        let observer = Observer::new(o.id, o.state, o.force.map(|f| &f.0));
        o.contacts.0.clear();
        for sensor in &o.sensors.0 {
            sensed.clear();
            {
                let mut sense_ctx = SenseContext::new(ctx.now, &mut o.rng.0, ctx.terrain);
                let mut out = ContactBuffer::new(sensed);
                sensor.sense(&mut sense_ctx, &observer, &candidates, &mut out);
            }
            o.contacts.0.append(sensed);
        }
        // The stable sort keeps template order among equal ids, so the first sensor wins.
        o.contacts.0.sort_by_key(|c| c.entity);
        o.contacts.0.dedup_by_key(|c| c.entity);
    });
}
