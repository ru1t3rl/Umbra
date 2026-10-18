//! AT-3 (WI-3 rows): the `builtin.ground.kinematic` factory. Requirements: REQ-T-1, REQ-T-3,
//! REQ-M-4, ADR 0008.
//!
//! The factory turns validated JSON parameters (units in the key names: `_mps`, `_dps`, `_mps2`)
//! into the same model `GroundKinematic::new` builds from typed limits (degrees converted once).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use serde_json::json;

use umbra_core::{
    ComponentId, ComponentKind, Ecef, Enu, Euler, Factory, FlatTerrain, Geodetic, Intent,
    KinematicState, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    ParamValue, Radians, RadiansPerSecond, Registry, RegistryError, Seconds,
};
use umbra_motion::{
    GroundKinematic, GroundKinematicFactory, GroundKinematicParams, register_builtins,
};

fn factory() -> GroundKinematicFactory {
    GroundKinematicFactory::new()
}

fn spec_params() -> GroundKinematicParams {
    GroundKinematicParams {
        max_speed: MetersPerSecond(18.0),
        max_turn_rate: RadiansPerSecond::from_degrees_per_second(40.0),
        max_accel: MetersPerSecondSquared(2.5),
    }
}

fn start() -> KinematicState {
    let g = Geodetic::from_degrees(52.0, 4.0, 0.0).expect("origin");
    let frame = LocalFrame::at(g);
    KinematicState {
        position: g.to_ecef(),
        velocity: frame.vector_from_enu(Enu {
            east: 0.0,
            north: 0.0,
            up: 0.0,
        }),
        orientation: frame.orientation_from_euler(Euler {
            heading: Radians(1.0),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
    }
}

fn destination() -> Ecef {
    LocalFrame::at(Geodetic::from_degrees(52.0, 4.0, 0.0).expect("origin")).from_enu(Enu {
        east: 300.0,
        north: 400.0,
        up: 0.0,
    })
}

/// 300 steps of the same intents; every state along the way.
fn trajectory(model: &dyn MotionModel) -> Vec<KinematicState> {
    let terrain = FlatTerrain {
        height: Meters(0.0),
    };
    let intents = [
        Intent::MoveTo {
            destination: destination(),
        },
        Intent::SetSpeed(MetersPerSecond(15.0)),
    ];
    let mut state = start();
    let mut states = Vec::new();
    for _ in 0..300 {
        state = model.step(&state, &intents, Seconds(1.0 / 30.0), &terrain);
        states.push(state);
    }
    states
}

fn build(params: &ParamValue) -> std::sync::Arc<dyn MotionModel> {
    factory()
        .build(params)
        .unwrap_or_else(|e| panic!("build failed: {e}"))
}

#[test]
fn factory_id_is_builtin_ground_kinematic() {
    assert_eq!(
        factory().id(),
        &ComponentId::new("builtin.ground.kinematic").expect("id")
    );
}

#[test]
fn schema_requires_the_three_limits_and_forbids_other_keys() {
    let f = factory();
    let schema = f.param_schema().as_json();

    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required list")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    required.sort_unstable();

    assert_eq!(
        required,
        ["max_accel_mps2", "max_speed_mps", "max_turn_rate_dps"]
    );
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["type"], json!("object"));
}

#[test]
fn schema_describes_each_limit_as_a_positive_number() {
    let f = factory();
    let props = &f.param_schema().as_json()["properties"];

    for key in ["max_speed_mps", "max_turn_rate_dps", "max_accel_mps2"] {
        assert_eq!(props[key]["type"], json!("number"), "{key}");
        assert_eq!(props[key]["exclusiveMinimum"], json!(0), "{key}");
        assert!(props[key]["description"].is_string(), "{key} is documented");
    }
}

#[test]
fn schema_example_is_the_spec_7_1_ground_example() {
    let f = factory();

    let examples = &f.param_schema().as_json()["examples"];

    assert_eq!(
        examples[0],
        json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 })
    );
}

#[test]
fn build_of_schema_example_gives_a_model_equal_to_direct_construction() {
    let f = factory();
    let example = f.param_schema().as_json()["examples"][0].clone();

    let built = build(&example);

    let direct = GroundKinematic::new(spec_params()).expect("valid limits");
    assert_eq!(trajectory(built.as_ref()), trajectory(&direct));
}

#[test]
fn built_model_reports_the_builtin_id() {
    let model =
        build(&json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 }));

    assert_eq!(model.id().as_str(), "builtin.ground.kinematic");
}

#[test]
fn integer_and_float_spellings_give_identical_models() {
    let ints = build(&json!({ "max_speed_mps": 18, "max_turn_rate_dps": 40, "max_accel_mps2": 3 }));
    let floats =
        build(&json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 3.0 }));

    assert_eq!(trajectory(ints.as_ref()), trajectory(floats.as_ref()));
}

#[test]
fn different_limits_give_a_different_trajectory() {
    let fast =
        build(&json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 }));
    let slow =
        build(&json!({ "max_speed_mps": 9.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 }));

    assert_ne!(trajectory(fast.as_ref()), trajectory(slow.as_ref()));
}

fn with_value(key: &str, value: f64) -> ParamValue {
    let mut p = json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 });
    p[key] = json!(value);
    p
}

/// Defence in depth: the schema normally rejects these before `build` is called.
#[test]
fn non_positive_value_gives_build_error_naming_the_parameter() {
    for key in ["max_speed_mps", "max_turn_rate_dps", "max_accel_mps2"] {
        for bad in [0.0, -1.0] {
            let error = factory().build(&with_value(key, bad)).err();

            let error = error.unwrap_or_else(|| panic!("{key} = {bad} must be rejected"));
            assert_eq!(
                error.param.as_deref(),
                Some(format!("/{key}").as_str()),
                "{key} = {bad}"
            );
        }
    }
}

#[test]
fn missing_parameter_gives_build_error_naming_it() {
    let params = json!({ "max_speed_mps": 18.0, "max_turn_rate_dps": 40.0 });

    let error = factory().build(&params).err();

    let error = error.unwrap_or_else(|| panic!("a missing limit must be rejected"));
    assert_eq!(error.param.as_deref(), Some("/max_accel_mps2"));
}

#[test]
fn parameter_of_the_wrong_type_gives_build_error_naming_it() {
    let params =
        json!({ "max_speed_mps": "fast", "max_turn_rate_dps": 40.0, "max_accel_mps2": 2.5 });

    let error = factory().build(&params).err();

    let error = error.unwrap_or_else(|| panic!("a string limit must be rejected"));
    assert_eq!(error.param.as_deref(), Some("/max_speed_mps"));
}

#[test]
fn register_builtins_makes_the_factory_available_by_id() {
    let mut registry = Registry::new();

    register_builtins(&mut registry).expect("first registration");

    let id = ComponentId::new("builtin.ground.kinematic").expect("id");
    assert!(registry.motion_model(&id).is_some());
    assert!(
        registry
            .param_schema(ComponentKind::MotionModel, &id)
            .is_some()
    );
    assert_eq!(registry.ids(ComponentKind::MotionModel), [&id]);
}

#[test]
fn register_builtins_twice_is_a_duplicate_error() {
    let mut registry = Registry::new();
    register_builtins(&mut registry).expect("first registration");

    let second = register_builtins(&mut registry);

    assert!(
        matches!(second, Err(RegistryError::Duplicate { .. })),
        "{second:?}"
    );
}

#[test]
fn register_builtins_adds_no_terrain_providers() {
    let mut registry = Registry::new();

    register_builtins(&mut registry).expect("registration");

    assert_eq!(
        registry.ids(ComponentKind::Terrain),
        Vec::<&ComponentId>::new()
    );
}

/// 5e-324 is schema-valid but rounds to 0 rad/s after the degree conversion; the error is still
/// located at the TOML key and names it, not an internal field.
#[test]
fn subnormal_turn_rate_is_located_at_its_key_and_named_by_it() {
    let error = factory()
        .build(&with_value("max_turn_rate_dps", 5e-324))
        .err()
        .unwrap_or_else(|| panic!("must be rejected"));

    assert_eq!(error.param.as_deref(), Some("/max_turn_rate_dps"));
    assert!(error.message.contains("max_turn_rate_dps"), "{error}");
}

#[test]
fn schema_bounds_each_limit_from_above() {
    let f = factory();
    let props = &f.param_schema().as_json()["properties"];
    assert_eq!(props["max_speed_mps"]["maximum"], json!(1000));
    assert_eq!(props["max_turn_rate_dps"]["maximum"], json!(3600));
    assert_eq!(props["max_accel_mps2"]["maximum"], json!(1000));
}
