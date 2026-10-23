//! AT-14 (M7, WI-5 rows): `builtin.sea.kinematic` is in the builtin and standard registries with a
//! schema that compiles in the ADR 0009 subset, validates its example and builds from it.
//! Requirements: REQ-T-3, REQ-M-2, ADR 0051. Motion models are checked by containment.

#![forbid(unsafe_code)]

use umbra_core::{ComponentId, ComponentKind};
use umbra_engine::builtins::{builtin_registry, standard_registry};
use umbra_templates::CompiledSchema;

#[test]
fn given_the_registries_when_listing_motion_models_then_sea_is_present_beside_ground() {
    for registry in [
        builtin_registry().expect("builtin"),
        standard_registry().expect("standard"),
    ] {
        let ids: Vec<String> = registry
            .ids(ComponentKind::MotionModel)
            .iter()
            .map(ToString::to_string)
            .collect();

        for id in ["builtin.ground.kinematic", "builtin.sea.kinematic"] {
            assert_eq!(ids.iter().filter(|i| *i == id).count(), 1, "{id}: {ids:?}");
        }
    }
}

#[test]
fn given_the_sea_schema_when_compiled_then_its_example_validates_and_builds() {
    let registry = standard_registry().expect("standard");
    let id = ComponentId::new("builtin.sea.kinematic").expect("id");
    let schema = registry
        .param_schema(ComponentKind::MotionModel, &id)
        .expect("registered");

    let compiled = CompiledSchema::compile(schema).expect("inside the subset");

    let examples = schema.as_json()["examples"].as_array().expect("examples");
    assert!(!examples.is_empty(), "documents an example");
    for example in examples {
        let built = registry.motion_model(&id).expect("factory").build(example);
        assert!(built.is_ok(), "{example}: {:?}", built.err());
    }
    // the unknown-key and range rules come from the schema, not only the factory
    let value = umbra_templates::value::parse(&umbra_templates::SourceFile::new(
        "x.toml",
        "x = { max_speed_mps = 0, max_accel_mps2 = 1, min_turn_radius_m = 1 }\n",
    ))
    .0
    .expect("table");
    assert!(
        !compiled
            .validate(value.get("x").expect("x"), "p")
            .is_empty(),
        "zero speed violates the schema"
    );
}
