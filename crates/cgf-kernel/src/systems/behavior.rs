//! The Behavior phase system (REQ-K-2, REQ-K-5, REQ-B-1, ADR 0013, ADR 0016).
//!
//! Owned entities with a behavior tick it with a view of their own pre-tick state and contacts;
//! the behavior can only append intents. Owned entities without one run the M1 route stand-in
//! exactly as before (ASM-082).

use cgf_core::{BehaviorContext, EntityView, Intent, IntentBuffer, KinematicState, RouteView};

use crate::pipeline::TickContext;
use crate::systems::route;
use crate::world::{EntityMut, Route};

/// Clears the entity's intents and refills them for this tick. Remote entities get none.
pub(crate) fn run(e: EntityMut<'_>, ctx: &TickContext<'_>) {
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
        return;
    }
    let Some(slot) = behavior else {
        if let Some(r) = route {
            route::steer(r, state.position, &mut intents.0);
        }
        return;
    };
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
}

/// Replaces the first `FollowRoute` in place with the stand-in's intents for this tick and drops
/// the others (ADR 0016). Without a route the expansion is empty.
fn expand_follow_route(
    intents: &mut Vec<Intent>,
    mut route: Option<&mut Route>,
    state: &KinematicState,
) {
    if !intents.iter().any(|i| matches!(i, Intent::FollowRoute)) {
        return;
    }
    let mut expanded = Vec::with_capacity(intents.len() + 1);
    let mut done = false;
    for intent in intents.drain(..) {
        if matches!(intent, Intent::FollowRoute) {
            if !std::mem::replace(&mut done, true)
                && let Some(r) = route.as_deref_mut()
            {
                route::steer(r, state.position, &mut expanded);
            }
        } else {
            expanded.push(intent);
        }
    }
    *intents = expanded;
}
