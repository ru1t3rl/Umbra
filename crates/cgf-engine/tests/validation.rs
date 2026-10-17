//! Input validation at the engine's user edge (ASM-044, CLAUDE.md rule 6).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::sync::Arc;

use cgf_core::{
    Ecef, Meters, MetersPerSecond, MotionModel, Ownership, Position, Radians, TickRate, Vec3,
};
use cgf_engine::{Engine, EngineConfig, EngineError, SpawnParam, SpawnSpec, demo};
use cgf_kernel::{ClockMode, ManualWallClock};
use cgf_motion::GroundKinematic;

fn config(terrain: f64) -> EngineConfig {
    EngineConfig {
        seed: 1,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
        terrain_height: Meters(terrain),
    }
}

fn engine() -> Engine {
    Engine::with_wall_clock(config(0.0), Box::new(ManualWallClock::new())).expect("valid config")
}

fn spec() -> SpawnSpec {
    let model: Arc<dyn MotionModel> =
        Arc::new(GroundKinematic::new(demo::ground_params()).expect("valid params"));
    SpawnSpec {
        position: Position::Ecef(Ecef(Vec3::new(6_378_137.0, 0.0, 0.0))),
        heading: Radians(0.0),
        speed: MetersPerSecond(0.0),
        ownership: Ownership::Owned,
        motion: model,
        route: None,
    }
}

/// A non-finite terrain height is rejected when the engine is built.
#[test]
fn non_finite_terrain_height_is_rejected() {
    for bad in [f64::NAN, f64::INFINITY] {
        let got = Engine::with_wall_clock(config(bad), Box::new(ManualWallClock::new()));

        assert!(
            matches!(got, Err(EngineError::InvalidTerrainHeight)),
            "{bad}"
        );
    }
}

/// Non-finite heading, non-finite or negative speed and a non-finite ECEF position are rejected
/// with the offending parameter named, and nothing is spawned.
#[test]
fn invalid_spawn_input_is_rejected_with_the_parameter() {
    let mut cases = Vec::new();
    for bad in [f64::NAN, f64::INFINITY] {
        cases.push((
            SpawnSpec {
                heading: Radians(bad),
                ..spec()
            },
            SpawnParam::Heading,
        ));
        cases.push((
            SpawnSpec {
                speed: MetersPerSecond(bad),
                ..spec()
            },
            SpawnParam::Speed,
        ));
    }
    cases.push((
        SpawnSpec {
            speed: MetersPerSecond(-1.0),
            ..spec()
        },
        SpawnParam::Speed,
    ));
    cases.push((
        SpawnSpec {
            position: Position::Ecef(Ecef(Vec3::new(f64::NAN, 0.0, 0.0))),
            ..spec()
        },
        SpawnParam::Position,
    ));

    for (case, param) in cases {
        let mut e = engine();

        let got = e.spawn(case);

        assert!(
            matches!(got, Err(EngineError::InvalidSpawn(p)) if p == param),
            "{param:?}: {got:?}"
        );
        assert_eq!(e.snapshot().entities.len(), 0);
    }
}
