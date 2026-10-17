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

/// Recomputes the contacts of every Owned entity that has sensors.
pub(crate) fn run(world: &mut World, ctx: &TickContext<'_>) {
    if !world.has_sensors() {
        return;
    }
    let mut rows = Vec::with_capacity(world.len());
    world.for_each(|e| {
        rows.push(Row {
            id: e.id,
            state: *e.state,
            force: e.force.map(|f| f.0.clone()),
            ownership: e.ownership.clone(),
        });
    });
    let mut candidates: Vec<Target<'_>> = Vec::with_capacity(rows.len());
    let mut scratch: Vec<Contact> = Vec::new();
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
            scratch.clear();
            {
                let mut sense_ctx = SenseContext::new(ctx.now, &mut o.rng.0, ctx.terrain);
                let mut out = ContactBuffer::new(&mut scratch);
                sensor.sense(&mut sense_ctx, &observer, &candidates, &mut out);
            }
            for c in scratch.drain(..) {
                // First sensor in template order wins per target.
                if !o.contacts.0.iter().any(|known| known.entity == c.entity) {
                    o.contacts.0.push(c);
                }
            }
        }
        o.contacts.0.sort_by_key(|c| c.entity);
    });
}
