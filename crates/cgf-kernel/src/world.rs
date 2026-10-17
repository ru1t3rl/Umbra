//! The private ECS wrapper (REQ-K-1, ADR 0005, ASM-037).
//!
//! `hecs` types never leave this module. Entities are addressed by [`EntityId`] through a
//! `BTreeMap`; every walk over entities goes through that map, so the order is the id order and
//! never the ECS storage order (REQ-K-4).

#![expect(
    dead_code,
    reason = "the mutable walk and route fields are used by the WI-11 systems"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{
    Ecef, EntityId, Intent, KinematicState, MotionModel, Ownership, RngStream, SimRng, StreamKey,
};

use crate::error::KernelError;
use crate::kernel::{EntityInit, RouteInit};

/// The motion model instance of an entity (REQ-M-4).
pub(crate) struct MotionRef(pub(crate) Arc<dyn MotionModel>);

/// Intents of the current tick; cleared at the start of the Behavior phase.
pub(crate) struct Intents(pub(crate) Vec<Intent>);

/// Route-following state for the M1 stand-in (ASM-029).
pub(crate) struct Route {
    pub(crate) waypoints: Vec<Ecef>,
    pub(crate) next: usize,
    pub(crate) arrival_radius: f64,
    pub(crate) cruise: f64,
}

/// The entity's own random stream (ADR 0006).
pub(crate) struct EntityRng(pub(crate) RngStream);

/// Mutable view of one entity, handed to systems in id order.
pub(crate) struct EntityMut<'a> {
    pub(crate) ownership: &'a Ownership,
    pub(crate) state: &'a mut KinematicState,
    pub(crate) motion: &'a MotionRef,
    pub(crate) intents: &'a mut Intents,
    pub(crate) route: Option<&'a mut Route>,
}

/// Read-only view of one entity, handed out in id order.
pub(crate) struct EntityRef<'a> {
    pub(crate) id: EntityId,
    pub(crate) ownership: &'a Ownership,
    pub(crate) state: &'a KinematicState,
    pub(crate) motion: &'a MotionRef,
    pub(crate) route: Option<&'a Route>,
    pub(crate) rng: &'a EntityRng,
}

type MutQuery<'a> = (
    &'a Ownership,
    &'a mut KinematicState,
    &'a MotionRef,
    &'a mut Intents,
    Option<&'a mut Route>,
);

type RefQuery<'a> = (
    &'a Ownership,
    &'a KinematicState,
    &'a MotionRef,
    Option<&'a Route>,
    &'a EntityRng,
);

/// All entities of a kernel.
pub(crate) struct World {
    ecs: hecs::World,
    index: BTreeMap<EntityId, hecs::Entity>,
    next_id: Option<EntityId>,
}

fn validate_route(route: &RouteInit) -> Result<Route, KernelError> {
    if route.waypoints.is_empty() {
        return Err(KernelError::EmptyRoute);
    }
    let radius = route.arrival_radius.0;
    if !(radius.is_finite() && radius > 0.0) {
        return Err(KernelError::InvalidRoute("arrival_radius"));
    }
    let cruise = route.cruise_speed.0;
    if !(cruise.is_finite() && cruise > 0.0) {
        return Err(KernelError::InvalidRoute("cruise_speed"));
    }
    Ok(Route {
        waypoints: route.waypoints.clone(),
        next: 0,
        arrival_radius: radius,
        cruise,
    })
}

impl World {
    pub(crate) fn new() -> Self {
        Self {
            ecs: hecs::World::new(),
            index: BTreeMap::new(),
            next_id: EntityId::new(1),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }

    /// Validates `init` and adds the entity under the next id (from 1, never reused, REQ-C-2).
    pub(crate) fn spawn(&mut self, init: EntityInit, rng: SimRng) -> Result<EntityId, KernelError> {
        let route = init.route.as_ref().map(validate_route).transpose()?;
        let id = self.next_id.ok_or(KernelError::IdsExhausted)?;
        self.next_id = id.get().checked_add(1).and_then(EntityId::new);
        let entity = self.ecs.spawn((
            id,
            init.ownership,
            init.state,
            MotionRef(init.motion),
            Intents(Vec::new()),
            EntityRng(rng.stream(StreamKey::entity(id))),
        ));
        if let Some(route) = route {
            // The entity was spawned just above, so it exists.
            let _ = self.ecs.insert_one(entity, route);
        }
        self.index.insert(id, entity);
        Ok(id)
    }

    /// Visits every entity mutably, in ascending id order.
    pub(crate) fn for_each_mut(&mut self, mut f: impl FnMut(EntityMut<'_>)) {
        for entity in self.index.values() {
            let Ok((ownership, state, motion, intents, route)) =
                self.ecs.query_one_mut::<MutQuery<'_>>(*entity)
            else {
                continue;
            };
            f(EntityMut {
                ownership,
                state,
                motion,
                intents,
                route,
            });
        }
    }

    /// Visits every entity read-only, in ascending id order.
    pub(crate) fn for_each(&self, mut f: impl FnMut(EntityRef<'_>)) {
        for (id, entity) in &self.index {
            let mut query = self.ecs.query_one::<RefQuery<'_>>(*entity);
            if let Ok((ownership, state, motion, route, rng)) = query.get() {
                f(EntityRef {
                    id: *id,
                    ownership,
                    state,
                    motion,
                    route,
                    rng,
                });
            }
        }
    }
}
