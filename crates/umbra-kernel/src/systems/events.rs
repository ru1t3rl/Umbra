//! The Events phase system (REQ-C-5, REQ-K-2, REQ-K-5, ADR 0028).
//!
//! Turns the `Fire` and `Custom` intents of this tick into canonical events, Owned entities in id
//! order, each entity's intents in emission order. It only reads: no state changes and nothing is
//! hashed. `Spawn` and `Despawn` are not applied here: they are collected into the tick report for
//! the engine to apply after the tick (ADR 0052; a bare kernel applies neither).

use umbra_core::{EntityId, Intent, SimEvent};

use crate::world::World;

/// Appends this tick's events to `out`.
pub(crate) fn run(world: &World, out: &mut Vec<SimEvent>) {
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
                _ => {}
            }
        }
    });
}

/// Appends the `Spawn` and `Despawn` intents of Owned entities to `out`, entities in id order and
/// each entity's intents in emission order (ADR 0052, REQ-B-2, REQ-K-5).
pub(crate) fn collect_lifecycle(world: &World, out: &mut Vec<(EntityId, Intent)>) {
    world.for_each(|e| {
        if !e.ownership.is_owned() {
            return;
        }
        out.extend(
            e.intents
                .0
                .iter()
                .filter(|i| matches!(i, Intent::Spawn { .. } | Intent::Despawn))
                .map(|i| (e.id, i.clone())),
        );
    });
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
}
