//! The Behavior phase system (REQ-K-2, REQ-K-5, REQ-B-1, ADR 0013, ADR 0016).
//!
//! Owned entities with a behavior tick it with a view of their own pre-tick state and contacts;
//! the behavior can only append intents. Owned entities without one run the M1 route stand-in
//! exactly as before (ASM-082).

use std::sync::Arc;

use umbra_core::{
    BehaviorBatch, BehaviorContext, EntityView, Intent, IntentBuffer, KinematicState, RouteView,
};

use crate::pipeline::TickContext;
use crate::systems::route;
use crate::world::{EntityMut, Route, World};

/// Runs the Behavior phase (ADR 0020): entities in id order, then each batch group once, then the
/// `FollowRoute` expansion of the batched entities in id order.
pub(crate) fn run_phase(world: &mut World, ctx: &TickContext<'_>) {
    // Taken out of the world so the groups can borrow it while the phase runs.
    let mut scratch = std::mem::take(&mut world.batch);
    scratch.begin_tick();
    world.for_each_mut(|e| {
        let id = e.id;
        if let Some(batch) = run(e, ctx) {
            scratch.add(id, &batch);
        }
    });
    if !scratch.batched.is_empty() {
        world.tick_batches(&mut scratch, ctx.now);
        world.for_each_batched_mut(&scratch.batched, |intents, route, state| {
            expand_follow_route(&mut intents.0, route, state);
        });
    }
    world.batch = scratch;
}

/// Clears the entity's intents and refills them for this tick. Remote entities get none. An
/// entity whose behavior has a batch is not ticked here: its batch is returned and the entity is
/// ticked with its group.
fn run(e: EntityMut<'_>, ctx: &TickContext<'_>) -> Option<Arc<dyn BehaviorBatch>> {
    let EntityMut {
        id,
        rng,
        force,
        contacts,
        behavior,
        ownership,
        state,
        intents,
        route,
        ..
    } = e;
    intents.0.clear();
    if !ownership.is_owned() {
        return None;
    }
    let Some(slot) = behavior else {
        if let Some(r) = route {
            route::steer(r, state.position, &mut intents.0);
        }
        return None;
    };
    if let Some(batch) = &slot.batch {
        return Some(Arc::clone(batch));
    }
    let status = {
        let mut view = EntityView::new(id, state).with_force(force.map(|f| &f.0));
        if let Some(r) = route.as_deref() {
            view = view.with_route(RouteView::new(
                &r.waypoints,
                r.next,
                route::is_complete(r, state.position),
            ));
        }
        if let Some(c) = contacts {
            view = view.with_contacts(&c.0);
        }
        let mut behavior_ctx = BehaviorContext::new(ctx.now, &mut rng.0);
        let mut out = IntentBuffer::new(&mut intents.0);
        slot.instance.tick(&mut behavior_ctx, &view, &mut out)
    };
    slot.last = Some(status);
    expand_follow_route(&mut intents.0, route, state);
    None
}

/// Replaces the first `FollowRoute` in place with the stand-in's intents for this tick and drops
/// the others (ADR 0016). Without a route the expansion is empty. Allocation-free once the vector
/// has its capacity: the steer output is appended at the end and rotated into place.
fn expand_follow_route(
    intents: &mut Vec<Intent>,
    route: Option<&mut Route>,
    state: &KinematicState,
) {
    let Some(p) = intents
        .iter()
        .position(|i| matches!(i, Intent::FollowRoute))
    else {
        return;
    };
    let mut index = 0;
    intents.retain(|i| {
        let keep = index <= p || !matches!(i, Intent::FollowRoute);
        index += 1;
        keep
    });
    intents.remove(p);
    let before = intents.len();
    if let Some(r) = route {
        route::steer(r, state.position, intents);
    }
    let pushed = intents.len() - before;
    if let Some(tail) = intents.get_mut(p..) {
        tail.rotate_right(pushed);
    }
}
