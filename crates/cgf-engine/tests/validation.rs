//! Input validation at the engine's user edge (ASM-044, CLAUDE.md rule 6).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::sync::Arc;

use cgf_core::{
    Ecef, Meters, MetersPerSecond, MotionModel, Ownership, Position, Radians, TickRate, Vec3,
};
use cgf_engine::{Engine, EngineConfig, EngineError, SpawnParam, SpawnSpec};
use cgf_kernel::{ClockMode, ManualWallClock};
use cgf_motion::GroundKinematic;

/// The spec 7.1 ground limits: 18 m/s, 40 deg/s, 2.5 m/s^2.
fn ground_params() -> cgf_motion::GroundKinematicParams {
    cgf_motion::GroundKinematicParams {
        max_speed: cgf_core::MetersPerSecond(18.0),
        max_turn_rate: cgf_core::RadiansPerSecond::from_degrees_per_second(40.0),
        max_accel: cgf_core::MetersPerSecondSquared(2.5),
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        seed: 1,
        tick_rate: TickRate::new(30).expect("non-zero rate"),
        clock: ClockMode::AsFastAsPossible,
    }
}

fn engine() -> Engine {
    Engine::with_wall_clock(
        config(),
        Arc::new(cgf_core::FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(ManualWallClock::new()),
    )
}

fn spec() -> SpawnSpec {
    let model: Arc<dyn MotionModel> =
        Arc::new(GroundKinematic::new(ground_params()).expect("valid params"));
    SpawnSpec {
        position: Position::Ecef(Ecef(Vec3::new(6_378_137.0, 0.0, 0.0))),
        heading: Radians(0.0),
        speed: MetersPerSecond(0.0),
        ownership: Ownership::Owned,
        motion: model,
        route: None,
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

/// Embedder-supplied terrain is not validated (documented on `Engine::new`), but a terrain that
/// reports non-finite heights must not poison the state: it stays finite and the run stays
/// deterministic (the ground model keeps the moved height).
#[test]
fn non_finite_terrain_heights_keep_the_state_finite_and_the_run_deterministic() {
    use cgf_core::{Enu, Geodetic, TerrainProvider};
    use std::ops::ControlFlow;

    struct Broken(f64);
    impl TerrainProvider for Broken {
        fn height_at(&self, _: Geodetic) -> Meters {
            Meters(self.0)
        }
        fn surface_normal_at(&self, _: Geodetic) -> Enu {
            Enu {
                east: 0.0,
                north: 0.0,
                up: 1.0,
            }
        }
    }

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let run = || {
            let mut e = Engine::with_wall_clock(
                config(),
                Arc::new(Broken(bad)),
                Box::new(ManualWallClock::new()),
            );
            let mut s = spec();
            s.speed = MetersPerSecond(5.0);
            s.route = Some(cgf_engine::RouteSpec {
                waypoints: vec![Position::Ecef(Ecef(Vec3::new(6_378_137.0, 500.0, 0.0)))],
                arrival_radius: Meters(5.0),
                cruise_speed: MetersPerSecond(10.0),
            });
            e.spawn(s).expect("spawn");
            e.run(cgf_engine::RunUntil::Ticks(300), |_| {
                ControlFlow::Continue(())
            })
            .expect("run");
            e
        };
        let (a, b) = (run(), run());

        let p = a.snapshot().entities[0].state.position.0;
        assert!(
            p.x.is_finite() && p.y.is_finite() && p.z.is_finite(),
            "{bad}: {p:?}"
        );
        assert_eq!(a.state_hash(), b.state_hash(), "{bad}");
    }
}
