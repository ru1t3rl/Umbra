//! AT-13 (M7, WI-6 rows): the `builtin.air.kinematic` factory. Requirements: REQ-T-1, REQ-T-3,
//! REQ-M-4, ADR 0051, ASM-607..609, ASM-700.
//!
//! Parameters carry their unit in the key name (degrees for the bank and the turn-rate cap);
//! `min_speed_mps` and `min_altitude_agl_m` default to 0, `max_descent_rate_mps` to the climb rate;
//! a bad value is located at its key.

#![forbid(unsafe_code)]

mod common;

use common::{DT, flat, move_to, run, start};
use serde_json::json;
use umbra_core::{ComponentId, ComponentKind, Factory, Registry};
use umbra_core::{Meters, MetersPerSecond, MetersPerSecondSquared, Radians, RadiansPerSecond};
use umbra_motion::{
    AIR_KINEMATIC_ID, AirKinematic, AirKinematicFactory, AirKinematicParams, register_builtins,
};

fn full() -> serde_json::Value {
    json!({
        "max_speed_mps": 200.0, "min_speed_mps": 50.0, "max_accel_mps2": 5.0,
        "max_climb_rate_mps": 15.0, "max_descent_rate_mps": 10.0, "max_bank_deg": 30.0,
        "max_turn_rate_dps": 3.0, "min_altitude_agl_m": 100.0
    })
}

fn typed() -> AirKinematicParams {
    AirKinematicParams {
        max_speed: MetersPerSecond(200.0),
        min_speed: MetersPerSecond(50.0),
        max_accel: MetersPerSecondSquared(5.0),
        max_climb_rate: MetersPerSecond(15.0),
        max_descent_rate: MetersPerSecond(10.0),
        max_bank: Radians::from_degrees(30.0),
        max_turn_rate: Some(RadiansPerSecond::from_degrees_per_second(3.0)),
        min_altitude_agl: Meters(100.0),
    }
}

#[test]
fn given_the_factory_when_asked_then_id_and_schema_are_the_air_models() {
    let f = AirKinematicFactory::new();

    assert_eq!(f.id().as_str(), AIR_KINEMATIC_ID);
    let schema = f.param_schema().as_json();
    assert!(schema["properties"]["max_bank_deg"].is_object());
    assert_eq!(schema["required"].as_array().map(Vec::len), Some(4));
}

/// REQ-M-4: the factory builds the same model the typed constructor builds, degrees converted.
#[test]
fn given_valid_params_when_built_then_the_motion_equals_the_typed_model() {
    let built = AirKinematicFactory::new().build(&full()).expect("builds");
    let model = AirKinematic::new(typed()).expect("valid");
    let intents = [move_to(3000.0, 1000.0, 1500.0)];

    let a = run(
        built.as_ref(),
        &flat(),
        start(0.3, 120.0, 1000.0),
        &intents,
        300,
    );
    let b = run(&model, &flat(), start(0.3, 120.0, 1000.0), &intents, 300);

    assert_eq!(a, b);
}

/// ASM-700: the optional keys default (0 minimum speed, 0 AGL floor, descent = climb, no turn cap).
#[test]
fn given_only_the_required_keys_when_built_then_the_defaults_apply() {
    let p = json!({"max_speed_mps": 100.0, "max_accel_mps2": 5.0, "max_climb_rate_mps": 12.0, "max_bank_deg": 45.0});

    let built = AirKinematicFactory::new().build(&p).expect("builds");

    let model = AirKinematic::new(AirKinematicParams {
        max_speed: MetersPerSecond(100.0),
        min_speed: MetersPerSecond(0.0),
        max_accel: MetersPerSecondSquared(5.0),
        max_climb_rate: MetersPerSecond(12.0),
        max_descent_rate: MetersPerSecond(12.0),
        max_bank: Radians::from_degrees(45.0),
        max_turn_rate: None,
        min_altitude_agl: Meters(0.0),
    })
    .expect("valid");
    let intents = [move_to(500.0, 500.0, 0.0)];
    let a = run(
        built.as_ref(),
        &flat(),
        start(0.0, 20.0, 300.0),
        &intents,
        200,
    );
    let b = run(&model, &flat(), start(0.0, 20.0, 300.0), &intents, 200);
    assert_eq!(a, b);
    let _ = DT;
}

/// A bad or missing parameter is reported at its key, with the bank in degrees.
#[test]
fn given_a_bad_parameter_when_built_then_the_error_names_its_key() {
    let f = AirKinematicFactory::new();
    let cases = [
        ("max_speed_mps", json!(0.0)),
        ("min_speed_mps", json!(250.0)),
        ("min_speed_mps", json!(-1.0)),
        ("max_accel_mps2", json!(-1.0)),
        ("max_climb_rate_mps", json!(0.0)),
        ("max_descent_rate_mps", json!(-2.0)),
        ("max_bank_deg", json!(0.0)),
        ("max_bank_deg", json!(90.0)),
        ("max_turn_rate_dps", json!(0.0)),
        ("min_altitude_agl_m", json!(-5.0)),
        ("max_bank_deg", json!("steep")),
    ];

    for (key, value) in cases {
        let mut p = full();
        p[key] = value.clone();
        let error = f.build(&p).err().unwrap_or_else(|| panic!("{key}={value}"));
        assert_eq!(
            error.param.as_deref(),
            Some(format!("/{key}").as_str()),
            "{key}={value}: {error}"
        );
    }
    let mut p = full();
    p["max_bank_deg"] = json!(95.0);
    let error = f.build(&p).err().expect("rejected");
    assert!(
        error.to_string().contains("got 95"),
        "degrees, not radians: {error}"
    );
    for key in [
        "max_speed_mps",
        "max_accel_mps2",
        "max_climb_rate_mps",
        "max_bank_deg",
    ] {
        let mut missing = full();
        missing.as_object_mut().expect("object").remove(key);
        let error = f.build(&missing).err().expect("missing key rejected");
        assert_eq!(error.param.as_deref(), Some(format!("/{key}").as_str()));
    }
}

#[test]
fn given_non_object_params_when_built_then_it_is_an_error_not_a_panic() {
    let f = AirKinematicFactory::new();
    for v in [json!(null), json!([]), json!("x")] {
        assert!(f.build(&v).is_err(), "{v}");
    }
}

#[test]
fn given_register_builtins_then_air_is_registered_beside_the_others() {
    let mut registry = Registry::new();
    register_builtins(&mut registry).expect("first registration");

    let id = ComponentId::new(AIR_KINEMATIC_ID).expect("id");

    assert!(registry.motion_model(&id).is_some());
    assert!(
        registry
            .param_schema(ComponentKind::MotionModel, &id)
            .is_some()
    );
    let motion = registry.ids(ComponentKind::MotionModel);
    for other in ["builtin.ground.kinematic", "builtin.sea.kinematic"] {
        let other = ComponentId::new(other).expect("id");
        assert!(motion.contains(&&other));
    }
    assert_eq!(motion.iter().filter(|i| ***i == id).count(), 1);
    assert!(register_builtins(&mut registry).is_err());
}
