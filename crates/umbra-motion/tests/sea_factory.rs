//! AT-14 (M7, WI-5 rows): the `builtin.sea.kinematic` factory. Requirements: REQ-T-1, REQ-T-3,
//! REQ-M-4, ADR 0051, ASM-610, ASM-695, ASM-696.
//!
//! Parameters carry their unit in the key name; a missing `surface_height_m` means 0; a bad value
//! is located at its key.

#![forbid(unsafe_code)]

mod common;

use common::{DT, flat, height_of, move_to, run, start};
use serde_json::json;
use umbra_core::{ComponentId, ComponentKind, Factory, Registry};
use umbra_core::{Meters, MetersPerSecond, MetersPerSecondSquared};
use umbra_motion::{
    SEA_KINEMATIC_ID, SeaKinematic, SeaKinematicFactory, SeaKinematicParams, register_builtins,
};

fn limits(surface: f64) -> SeaKinematicParams {
    SeaKinematicParams {
        max_speed: MetersPerSecond(12.0),
        max_accel: MetersPerSecondSquared(0.5),
        min_turn_radius: Meters(80.0),
        surface_height: Meters(surface),
    }
}

fn full() -> serde_json::Value {
    json!({"max_speed_mps": 12.0, "max_accel_mps2": 0.5, "min_turn_radius_m": 80.0, "surface_height_m": 3.0})
}

#[test]
fn given_the_factory_when_asked_then_id_and_schema_are_the_sea_models() {
    let f = SeaKinematicFactory::new();

    assert_eq!(f.id().as_str(), SEA_KINEMATIC_ID);
    let schema = f.param_schema().as_json();
    assert!(schema["properties"]["min_turn_radius_m"].is_object());
    assert_eq!(schema["required"].as_array().map(Vec::len), Some(3));
}

/// REQ-M-4: the factory builds the same model the typed constructor builds.
#[test]
fn given_valid_params_when_built_then_the_motion_equals_the_typed_model() {
    let built = SeaKinematicFactory::new().build(&full()).expect("builds");
    let typed = SeaKinematic::new(limits(3.0)).expect("valid");
    let intents = [move_to(300.0, 100.0, 0.0)];

    let a = run(built.as_ref(), &flat(), start(0.3, 5.0, 3.0), &intents, 200);
    let b = run(&typed, &flat(), start(0.3, 5.0, 3.0), &intents, 200);

    assert_eq!(a, b);
    assert!((height_of(&a[199]) - 3.0).abs() < 1e-6);
}

/// ASM-696: `surface_height_m` is optional and defaults to 0.
#[test]
fn given_no_surface_height_when_built_then_the_surface_is_zero() {
    let p = json!({"max_speed_mps": 12.0, "max_accel_mps2": 0.5, "min_turn_radius_m": 80.0});

    let m = SeaKinematicFactory::new().build(&p).expect("builds");

    let s = m.step(&start(0.0, 0.0, 40.0), &[], DT, &flat());
    assert!(height_of(&s).abs() < 1e-6);
}

/// ASM-695: a bad or missing parameter is reported at its key.
#[test]
fn given_a_bad_parameter_when_built_then_the_error_names_its_key() {
    let f = SeaKinematicFactory::new();
    let cases = [
        ("max_speed_mps", json!(0.0)),
        ("max_accel_mps2", json!(-1.0)),
        ("min_turn_radius_m", json!(0.0)),
        ("surface_height_m", json!("high")),
        // serde_json turns an infinite f64 into null: a non-number rejection, not NotFinite
        // (that arm is unit-tested in `factory.rs`)
        ("surface_height_m", json!(null)),
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
    let mut missing = full();
    missing
        .as_object_mut()
        .expect("object")
        .remove("min_turn_radius_m");
    let error = f.build(&missing).err().expect("missing key rejected");
    assert_eq!(error.param.as_deref(), Some("/min_turn_radius_m"));
}

#[test]
fn given_non_object_params_when_built_then_it_is_an_error_not_a_panic() {
    let f = SeaKinematicFactory::new();
    for v in [json!(null), json!([]), json!("x")] {
        assert!(f.build(&v).is_err(), "{v}");
    }
}

#[test]
fn given_register_builtins_then_ground_and_sea_are_registered_and_a_second_call_is_a_duplicate() {
    let mut registry = Registry::new();
    register_builtins(&mut registry).expect("first registration");

    let id = ComponentId::new(SEA_KINEMATIC_ID).expect("id");

    assert!(registry.motion_model(&id).is_some());
    assert!(
        registry
            .param_schema(ComponentKind::MotionModel, &id)
            .is_some()
    );
    let ground = ComponentId::new("builtin.ground.kinematic").expect("id");
    let motion = registry.ids(ComponentKind::MotionModel);
    assert!(motion.contains(&&id) && motion.contains(&&ground));
    assert_eq!(motion.iter().filter(|i| ***i == id).count(), 1);
    assert!(register_builtins(&mut registry).is_err());
}
