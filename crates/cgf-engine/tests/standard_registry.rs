//! WI-15: `standard_registry`, `scenario_registry` and the instantiation of behaviors, sensors and
//! forces. Requirements: REQ-S-2, REQ-T-1, REQ-B-9, REQ-B-13, ADR 0011, ADR 0015, ASM-077, ASM-084.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod behavior_support;
mod common;

use std::sync::Arc;

use behavior_support::{load_example, standard};
use cgf_core::{
    Behavior, BehaviorId, BehaviorInstance, ComponentId, ComponentKind, EntityId, InstantiateError,
    Provenance, TickRate, Version,
};
use cgf_engine::builtins::builtin_registry;
use cgf_engine::scenario::{instantiate, scenario_registry};
use cgf_engine::{AgentInit, ClockMode, Engine, EngineConfig, EngineError, SpawnSpec};
use cgf_kernel::{KernelError, ManualWallClock};
use cgf_templates::{ComponentRef, Loader, SourceFile};
use common::options;
use core::time::Duration;

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).expect("id")
}

/// ASM-084: the M2 registry is unchanged (no behaviors, sensors, conditions or actions).
#[test]
fn builtin_registry_is_still_the_m2_set() {
    let registry = builtin_registry().expect("builds");

    for kind in [
        ComponentKind::Behavior,
        ComponentKind::Sensor,
        ComponentKind::Condition,
        ComponentKind::Action,
    ] {
        assert!(registry.ids(kind).is_empty(), "{kind:?}");
    }
}

/// ASM-077, ASM-084: the standard registry is the M2 set plus eight leaves, the visual sensor and
/// the two built-in graphs.
#[test]
fn standard_registry_holds_the_m2_set_and_the_m3_builtins() {
    let registry = standard();
    let ids =
        |kind| -> Vec<String> { registry.ids(kind).iter().map(ToString::to_string).collect() };

    assert_eq!(
        ids(ComponentKind::MotionModel),
        ["builtin.ground.kinematic"]
    );
    assert_eq!(ids(ComponentKind::Terrain), ["builtin.terrain.flat"]);
    assert_eq!(ids(ComponentKind::Sensor), ["builtin.visual"]);
    assert_eq!(
        ids(ComponentKind::Behavior),
        [
            "builtin.fsm.patrol_and_engage",
            "builtin.subgraph.engage_target"
        ]
    );
    assert_eq!(ids(ComponentKind::Condition).len(), 4);
    assert_eq!(ids(ComponentKind::Action).len(), 4);
}

/// REQ-B-13: the built-in graphs are `Builtin`, version 1.0.0, with a description, a result text
/// and (for the patrol) the spec 7.1 `aggression` parameter.
#[test]
fn builtin_graphs_are_builtin_version_1_with_their_interfaces() {
    let registry = standard();

    for id in [
        "builtin.fsm.patrol_and_engage",
        "builtin.subgraph.engage_target",
    ] {
        let info = registry.info(&cid(id)).expect("info");
        assert_eq!(info.source, Provenance::Builtin, "{id}");
        assert_eq!(info.version, Version::new(1, 0, 0), "{id}");
        assert!(!info.description.is_empty(), "{id}");
        assert!(info.result.is_some(), "{id}");
    }
    let patrol = registry
        .info(&cid("builtin.fsm.patrol_and_engage"))
        .expect("info");
    assert_eq!(
        patrol
            .param_types
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["aggression", "engage_range_m", "fire_range_m"]
    );
    let engage = registry
        .info(&cid("builtin.subgraph.engage_target"))
        .expect("info");
    assert_eq!(
        engage
            .param_types
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["aggression", "approach_speed_mps", "fire_range_m", "target"]
    );
    assert_eq!(
        engage.result.as_deref(),
        Some("Failure when the target is unset or lost; otherwise Running")
    );
}

/// ADR 0011: `scenario_registry` adds the scenario's graphs as `File` behaviors, leaves the base
/// untouched and gives the same result every time.
#[test]
fn scenario_registry_adds_file_graphs_to_a_clone() {
    let base = standard();
    let scenario = load_example();

    let first = scenario_registry(&base, &scenario).expect("registers");
    let second = scenario_registry(&base, &scenario).expect("registers");

    assert!(base.behavior(&cid("example.subgraph.attack")).is_none());
    assert_eq!(base.ids(ComponentKind::Behavior).len(), 2);
    assert_eq!(
        first.ids(ComponentKind::Behavior),
        second.ids(ComponentKind::Behavior)
    );
    assert_eq!(first.ids(ComponentKind::Behavior).len(), 5);
    let attack = first.info(&cid("example.subgraph.attack")).expect("info");
    assert_eq!(attack.source, Provenance::File);
    let own = first
        .info(&cid("builtin.subgraph.engage_target"))
        .expect("info");
    assert_eq!(own.source, Provenance::Builtin);
}

/// A graph file that reuses a built-in ID is refused at load (`duplicate_id`, ADR 0011), and
/// registering it anyway is `EngineError::Graphs`, with the base untouched.
#[test]
fn a_file_graph_with_a_builtin_id_is_refused() {
    let base = standard();
    let mut scenario = load_example();
    let text = r#"
schema_version = 1
id = "builtin.subgraph.engage_target"
version = "1.0.0"
root = "a"
[nodes.a]
type = "action"
ref = "builtin.action.follow_route"
"#;
    let sources = [SourceFile::new("dup.behavior.toml", text)];
    let at_load = Loader::new(&base)
        .expect("loader")
        .graphs_from_sources(&sources, Provenance::File)
        .err()
        .map(cgf_templates::Diagnostics::into_vec);
    // Read against a registry without the built-in graphs, to reach `scenario_registry`.
    let mut without_graphs = builtin_registry().expect("builds");
    cgf_behavior::register_builtins(&mut without_graphs).expect("leaves");
    scenario.graphs = Loader::new(&without_graphs)
        .expect("loader")
        .graphs_from_sources(&sources, Provenance::File)
        .expect("reads");

    let refused = scenario_registry(&base, &scenario).err();

    let codes: Vec<&str> = at_load.iter().flatten().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["duplicate_id"]);
    assert!(
        matches!(refused, Some(EngineError::Graphs(_))),
        "{refused:?}"
    );
    assert_eq!(base.ids(ComponentKind::Behavior).len(), 2);
}

const GROUND: &str = r#"
schema_version = 1
id = "t.ground"
[motion]
model = "builtin.ground.kinematic"
params = { max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }
[behavior]
ref = "t.graph.long_wait"
"#;

const SCENARIO: &str = r#"
schema_version = 1
[scenario]
name = "long_wait"
seed = 1
tick_rate_hz = 30
clock = "afap"
duration_s = 1.0
[[entities]]
template = "t.ground"
position = { lat_deg = 52.0, lon_deg = 4.0 }
heading_deg = 0.0
speed_mps = 0.0
"#;

fn long_wait_scenario() -> (cgf_core::Registry, cgf_templates::Scenario) {
    let registry = standard();
    let graph = r#"
schema_version = 1
id = "t.graph.long_wait"
version = "1.0.0"
root = "w"
[nodes.w]
type = "action"
ref = "builtin.action.wait"
params = { duration_s = 1.0e30 }
"#;
    let scenario = Loader::new(&registry)
        .expect("loader")
        .scenario_from_sources(
            &SourceFile::new("scenario.toml", SCENARIO),
            &[
                SourceFile::new("t.toml", GROUND),
                SourceFile::new("long_wait.behavior.toml", graph),
            ],
        )
        .unwrap_or_else(|e| panic!("scenario: {e}"));
    (registry, scenario)
}

/// REQ-B-9, ADR 0011: a behavior that cannot be instantiated at the tick rate (a wait that does
/// not fit in ticks) arrives as `EngineError::Kernel(KernelError::Instantiate(..))`.
#[test]
fn instantiate_error_at_spawn_is_a_kernel_instantiate_error() {
    let (registry, scenario) = long_wait_scenario();

    let err = instantiate(
        &registry,
        &scenario,
        options(Duration::from_micros(500), None, None),
    )
    .err();

    assert!(
        matches!(
            err,
            Some(EngineError::Kernel(KernelError::Instantiate(
                InstantiateError::Duration { .. }
            )))
        ),
        "{err:?}"
    );
}

struct Refusing(BehaviorId);

impl Behavior for Refusing {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Err(InstantiateError::Other {
            message: "no".into(),
        })
    }
}

fn engine() -> Engine {
    let config = EngineConfig {
        seed: 1,
        tick_rate: TickRate::new(30).expect("30 Hz"),
        clock: ClockMode::Stepped,
    };
    let terrain = Arc::new(cgf_core::FlatTerrain {
        height: cgf_core::Meters(0.0),
    });
    Engine::with_wall_clock(config, terrain, Box::new(ManualWallClock::new()))
}

fn spec() -> SpawnSpec {
    let registry = standard();
    let motion = registry
        .motion_model(&cid("builtin.ground.kinematic"))
        .expect("registered")
        .build(&serde_json::json!({
            "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5
        }))
        .expect("builds");
    SpawnSpec {
        position: cgf_core::Position::Geodetic(
            cgf_core::Geodetic::from_degrees(52.0, 4.0, 0.0).expect("position"),
        ),
        heading: cgf_core::Radians(0.0),
        speed: cgf_core::MetersPerSecond(0.0),
        ownership: cgf_core::Ownership::Owned,
        motion,
        route: None,
    }
}

/// A refused instantiation consumes no entity ID and leaves the engine as it was.
#[test]
fn a_refused_spawn_consumes_no_id_and_changes_no_hash() {
    let mut engine = engine();
    let before = engine.state_hash();
    let agent = AgentInit {
        behavior: Some(Arc::new(Refusing(
            BehaviorId::new("t.behavior.no").expect("id"),
        ))),
        ..AgentInit::default()
    };

    let refused = engine.spawn_with(spec(), agent).err();
    let id = engine
        .spawn_with(spec(), AgentInit::default())
        .expect("spawns");

    assert!(matches!(
        refused,
        Some(EngineError::Kernel(KernelError::Instantiate(
            InstantiateError::Other { .. }
        )))
    ));
    assert_ne!(
        engine.state_hash(),
        before,
        "the second spawn changed the world"
    );
    assert_eq!(id, EntityId::new(1).expect("first id"));
}

/// `spawn(spec)` is `spawn_with(spec, AgentInit::default())` to the bit.
#[test]
fn spawn_is_spawn_with_the_default_agent() {
    let (mut a, mut b) = (engine(), engine());

    a.spawn(spec()).expect("spawns");
    b.spawn_with(spec(), AgentInit::default()).expect("spawns");

    assert_eq!(a.state_hash(), b.state_hash());
}

/// Not expected after validation, but reported with the placement index: an unknown behavior or
/// sensor ID is `UnknownComponent`, bad parameters are `BehaviorBuild` and `SensorBuild`.
#[test]
fn instantiate_reports_unknown_and_unbuildable_components_by_placement() {
    let registry = standard();
    let run = |edit: &dyn Fn(&mut cgf_templates::Scenario)| {
        let mut scenario = load_example();
        edit(&mut scenario);
        instantiate(
            &registry,
            &scenario,
            options(Duration::from_micros(500), None, None),
        )
        .err()
    };

    let unknown_behavior = run(&|s| {
        s.entities[0].behavior = Some(ComponentRef {
            id: cid("t.behavior.ghost"),
            params: serde_json::json!({}),
        });
    });
    let unknown_sensor = run(&|s| {
        s.entities[0].sensors = vec![ComponentRef {
            id: cid("t.sensor.ghost"),
            params: serde_json::json!({}),
        }];
    });
    let bad_behavior = run(&|s| {
        s.entities[1].behavior = Some(ComponentRef {
            id: cid("builtin.fsm.patrol_and_engage"),
            params: serde_json::json!({ "aggression": 5.0 }),
        });
    });
    let bad_sensor = run(&|s| {
        s.entities[0].sensors = vec![
            s.entities[0].sensors[0].clone(),
            ComponentRef {
                id: cid("builtin.visual"),
                params: serde_json::json!({ "range_m": -1.0, "fov_deg": 90.0 }),
            },
        ];
    });

    assert!(matches!(
        unknown_behavior,
        Some(EngineError::UnknownComponent {
            kind: ComponentKind::Behavior,
            ..
        })
    ));
    assert!(matches!(
        unknown_sensor,
        Some(EngineError::UnknownComponent {
            kind: ComponentKind::Sensor,
            ..
        })
    ));
    assert!(matches!(
        bad_behavior,
        Some(EngineError::BehaviorBuild { entity: 1, .. })
    ));
    assert!(matches!(
        bad_sensor,
        Some(EngineError::SensorBuild {
            entity: 0,
            index: 1,
            ..
        })
    ));
}

/// `builtin.visual` rejects parameters the schema also rejects, with a located message.
#[test]
fn visual_sensor_build_rejects_out_of_range_parameters() {
    let registry = standard();
    let factory = registry.sensor(&cid("builtin.visual")).expect("registered");
    let bad = |v: serde_json::Value| factory.build(&v).err().and_then(|e| e.param);

    assert_eq!(
        bad(serde_json::json!({ "fov_deg": 90.0 })).as_deref(),
        Some("/range_m")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": 0.0, "fov_deg": 90.0 })).as_deref(),
        Some("/range_m")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": 100_001.0, "fov_deg": 90.0 })).as_deref(),
        Some("/range_m")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": 10.0, "fov_deg": 361.0 })).as_deref(),
        Some("/fov_deg")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": 10.0, "fov_deg": 90.0, "eye_height_m": -1.0 }))
            .as_deref(),
        Some("/eye_height_m")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": "far", "fov_deg": 90.0 })).as_deref(),
        Some("/range_m")
    );
    assert_eq!(
        bad(serde_json::json!({ "range_m": 10.0, "fov_deg": 90.0 })),
        None
    );
}
