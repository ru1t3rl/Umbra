//! The private ECS wrapper (REQ-K-1, ADR 0005, ASM-037).
//!
//! `hecs` types never leave this module. Entities are addressed by [`EntityId`] through a
//! `BTreeMap`; every walk over entities goes through that map, so the order is the id order and
//! never the ECS storage order (REQ-K-4).

use std::collections::BTreeMap;
use std::sync::Arc;

use cgf_core::{
    Ecef, EntityId, Intent, KinematicState, MotionModel, Ownership, RngStream, SimRng, StreamKey,
};

use crate::error::{KernelError, RouteParam};
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

fn is_finite(v: cgf_core::Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

fn validate_state(s: &KinematicState) -> Result<(), KernelError> {
    let q = s.orientation;
    let finite = is_finite(s.position.0)
        && is_finite(s.velocity)
        && [q.w, q.x, q.y, q.z].iter().all(|c| c.is_finite());
    if finite {
        Ok(())
    } else {
        Err(KernelError::InvalidState)
    }
}

fn validate_route(route: &RouteInit) -> Result<Route, KernelError> {
    if route.waypoints.is_empty() {
        return Err(KernelError::EmptyRoute);
    }
    let radius = route.arrival_radius.0;
    if !(radius.is_finite() && radius > 0.0) {
        return Err(KernelError::InvalidRoute(RouteParam::ArrivalRadius));
    }
    let cruise = route.cruise_speed.0;
    if !(cruise.is_finite() && cruise > 0.0) {
        return Err(KernelError::InvalidRoute(RouteParam::CruiseSpeed));
    }
    if !route.waypoints.iter().all(|w| is_finite(w.0)) {
        return Err(KernelError::InvalidRoute(RouteParam::Waypoints));
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
        validate_state(&init.state)?;
        let route = init.route.as_ref().map(validate_route).transpose()?;
        let id = self.next_id.ok_or(KernelError::IdsExhausted)?;
        self.next_id = id.get().checked_add(1).and_then(EntityId::new);
        let mut builder = hecs::EntityBuilder::new();
        builder
            .add(id)
            .add(init.ownership)
            .add(init.state)
            .add(MotionRef(init.motion))
            .add(Intents(Vec::new()))
            .add(EntityRng(rng.stream(StreamKey::entity(id))));
        if let Some(route) = route {
            builder.add(route);
        }
        let entity = self.ecs.spawn(builder.build());
        self.index.insert(id, entity);
        Ok(id)
    }

    /// Visits every entity mutably, in ascending id order.
    pub(crate) fn for_each_mut(&mut self, mut f: impl FnMut(EntityMut<'_>)) {
        for entity in self.index.values() {
            let Ok((ownership, state, motion, intents, route)) =
                self.ecs.query_one_mut::<MutQuery<'_>>(*entity)
            else {
                debug_assert!(false, "every indexed entity has all kernel components");
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
            let Ok((ownership, state, motion, route, rng)) = query.get() else {
                debug_assert!(false, "every indexed entity has all kernel components");
                continue;
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use cgf_core::{Quaternion, Seconds, TerrainProvider};

    struct Still(cgf_core::MotionModelId);

    impl MotionModel for Still {
        fn id(&self) -> &cgf_core::MotionModelId {
            &self.0
        }
        fn step(
            &self,
            state: &KinematicState,
            _intents: &[Intent],
            _dt: Seconds,
            _terrain: &dyn TerrainProvider,
        ) -> KinematicState {
            *state
        }
    }

    fn init() -> EntityInit {
        let Ok(id) = cgf_core::MotionModelId::new("test.stub.still") else {
            panic!("valid id");
        };
        EntityInit {
            state: KinematicState {
                position: Ecef(cgf_core::Vec3::new(6_378_137.0, 0.0, 0.0)),
                velocity: cgf_core::Vec3::ZERO,
                orientation: Quaternion::IDENTITY,
            },
            ownership: Ownership::Owned,
            motion: Arc::new(Still(id)),
            route: None,
        }
    }

    fn rng_states(world: &World) -> Vec<(u64, [u64; 4])> {
        let mut out = Vec::new();
        world.for_each(|e| out.push((e.id.get(), e.rng.0.state())));
        out
    }

    fn world_with(count: usize, rng: SimRng) -> World {
        let mut world = World::new();
        for _ in 0..count {
            assert!(world.spawn(init(), rng).is_ok());
        }
        world
    }

    /// ADR 0006: entity `n` gets `rng.stream(StreamKey::entity(n))`, and its stream does not
    /// depend on how many other entities were spawned.
    #[test]
    fn entity_rng_stream_is_independent_of_other_spawns() {
        let rng = SimRng::new(42);

        let one = rng_states(&world_with(1, rng));
        let four = rng_states(&world_with(4, rng));

        for (id, state) in &four {
            let Some(entity_id) = EntityId::new(*id) else {
                panic!("non-zero id");
            };
            assert_eq!(*state, rng.stream(StreamKey::entity(entity_id)).state());
        }
        assert_eq!(one[0], four[0]);
        assert_ne!(four[0].1, four[1].1);
    }
}
