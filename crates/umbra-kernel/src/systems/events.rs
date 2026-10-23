//! The Events phase system (REQ-C-5, REQ-K-2, REQ-K-5, ADR 0028).
//!
//! Turns the `Fire` and `Custom` intents of this tick into canonical events, Owned entities in id
//! order, each entity's intents in emission order. It only reads: no state changes and nothing is
//! hashed. `Spawn` and `Despawn` are not applied here: they are collected into the tick report for
//! the engine to apply after the tick (ADR 0052; a bare kernel applies neither).

use umbra_core::{EntityId, Intent, SimEvent};

use crate::world::World;

/// Appends this tick's events to `out` and the `Spawn` and `Despawn` intents to `lifecycle` (ADR
/// 0052, REQ-B-2), in one walk: entities in id order, each entity's intents in emission order.
///
/// The Owned filter is defensive (REQ-K-5): the Behavior phase gives no intents to Remote entities,
/// so they hold none, but this system must not act for one that did.
pub(crate) fn run_with_lifecycle(
    world: &World,
    out: &mut Vec<SimEvent>,
    lifecycle: &mut Vec<(EntityId, Intent)>,
) {
    world.for_each(|e| {
        if !e.ownership.is_owned() {
            return;
        }
        for intent in &e.intents.0 {
            match intent {
                Intent::Fire { target, weapon } => out.push(SimEvent::Fire {
                    shooter: e.id,
                    target: *target,
                    weapon: weapon.clone(),
                }),
                Intent::Custom { kind, payload } => out.push(SimEvent::Custom {
                    source: Some(e.id),
                    kind: kind.clone(),
                    payload: payload.clone(),
                }),
                Intent::Spawn { .. } | Intent::Despawn => lifecycle.push((e.id, intent.clone())),
                _ => {}
            }
        }
    });
}

/// [`run_with_lifecycle`] without the lifecycle list (the events alone, for the unit tests).
#[cfg(test)]
pub(crate) fn run(world: &World, out: &mut Vec<SimEvent>) {
    run_with_lifecycle(world, out, &mut Vec::new());
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use umbra_core::{
        Ecef, EntityId, KinematicState, MotionModel, MotionModelId, Ownership, Quaternion,
        RemoteSource, Seconds, SimRng, TerrainProvider, Vec3, WeaponId,
    };

    use super::*;
    use crate::kernel::EntityInit;

    struct Still(MotionModelId);

    impl MotionModel for Still {
        fn id(&self) -> &MotionModelId {
            &self.0
        }
        fn step(
            &self,
            state: &KinematicState,
            _: &[Intent],
            _: Seconds,
            _: &dyn TerrainProvider,
        ) -> KinematicState {
            *state
        }
    }

    fn spawn(world: &mut World, ownership: Ownership) -> Result<EntityId, crate::KernelError> {
        let Ok(model_id) = MotionModelId::new("test.model.still") else {
            panic!("valid id");
        };
        world.spawn(
            EntityInit {
                state: KinematicState {
                    position: Ecef(Vec3::new(6_378_137.0, 0.0, 0.0)),
                    velocity: Vec3::ZERO,
                    orientation: Quaternion::IDENTITY,
                },
                ownership,
                motion: Arc::new(Still(model_id)),
                route: None,
            },
            SimRng::new(1),
        )
    }

    /// REQ-K-5: the ownership guard itself. A Remote entity holding a `Fire` intent (which the
    /// Behavior phase never gives it) yields no event, while an Owned one does.
    #[test]
    fn remote_entity_with_a_fire_intent_emits_nothing() {
        let mut world = World::new();
        let remote = Ownership::Remote {
            source: RemoteSource::new("net.test"),
        };
        assert!(spawn(&mut world, remote).is_ok());
        assert!(spawn(&mut world, Ownership::Owned).is_ok());
        let Some(target) = EntityId::new(1) else {
            panic!("non-zero");
        };
        let Ok(weapon) = WeaponId::new("example.weapon.main_gun") else {
            panic!("valid id");
        };
        world.for_each_mut(|e| {
            e.intents.0.push(Intent::Fire {
                target,
                weapon: weapon.clone(),
            });
        });

        let mut out = Vec::new();
        run(&world, &mut out);

        let shooters: Vec<_> = out
            .iter()
            .filter_map(|e| match e {
                SimEvent::Fire { shooter, .. } => Some(shooter.get()),
                _ => None,
            })
            .collect();
        assert_eq!(shooters, vec![2], "only the Owned entity fires");
    }

    /// REQ-K-5, ADR 0052: the ownership guard of the lifecycle walk. A Remote entity holding a
    /// `Despawn` intent (which the Behavior phase never gives it) is not reported, an Owned one is.
    #[test]
    fn remote_entity_with_a_despawn_intent_is_not_in_the_lifecycle_list() {
        let mut world = World::new();
        let remote = Ownership::Remote {
            source: RemoteSource::new("net.test"),
        };
        assert!(spawn(&mut world, remote).is_ok());
        assert!(spawn(&mut world, Ownership::Owned).is_ok());
        world.for_each_mut(|e| e.intents.0.push(Intent::Despawn));

        let (mut events, mut lifecycle) = (Vec::new(), Vec::new());
        run_with_lifecycle(&world, &mut events, &mut lifecycle);

        let who: Vec<_> = lifecycle.iter().map(|(id, _)| id.get()).collect();
        assert_eq!(who, vec![2], "only the Owned entity");
        assert!(events.is_empty(), "a Despawn is no event");
    }
}
