//! AT-17 (part, M7 WI-2): what the descriptor of the ABI 0.2 kinds carries (ADR 0050): the component
//! id, the parameter schema, and the metadata (`blackboard_params` for conditions and actions, ASM-643;
//! empty for motion models and sensors). Ignored until WI-10 (the stubs panic with `M7 WI-10`).
#![expect(clippy::expect_used, reason = "test harness")]

mod kinds_plugins;
mod net_harness;

use kinds_plugins::{
    ACTION_ID, ACTION_METADATA, ACTION_SCHEMA, ActionOnly, CONDITION_ID, CONDITION_METADATA,
    CONDITION_SCHEMA, ConditionOnly, MOTION_ID, MOTION_SCHEMA, MotionOnly, SENSOR_ID,
    SENSOR_SCHEMA, SensorOnly,
};
use net_harness::{capabilities, text};
use umbra_sdk::__private::{OwnedDescriptor, Plugin};

/// `(id, param_schema, metadata)` of the one capability of `P`.
fn only_capability<P: Plugin>() -> (String, String, String) {
    let built = OwnedDescriptor::build::<P>();
    let caps = capabilities(built.descriptor());
    assert_eq!(caps.len(), 1);
    (
        text(caps[0].id),
        text(caps[0].param_schema),
        text(caps[0].metadata),
    )
}

fn is_empty_metadata(text: &str) -> bool {
    text.is_empty() || text == "{}"
}

#[test]
#[ignore = "M7 WI-10"]
fn a_motion_model_has_its_id_and_schema_and_no_metadata() {
    let (id, schema, metadata) = only_capability::<MotionOnly>();

    assert_eq!((id.as_str(), schema.as_str()), (MOTION_ID, MOTION_SCHEMA));
    assert!(is_empty_metadata(&metadata), "{metadata:?}");
}

#[test]
#[ignore = "M7 WI-10"]
fn a_sensor_has_its_id_and_schema_and_no_metadata() {
    let (id, schema, metadata) = only_capability::<SensorOnly>();

    assert_eq!((id.as_str(), schema.as_str()), (SENSOR_ID, SENSOR_SCHEMA));
    assert!(is_empty_metadata(&metadata), "{metadata:?}");
}

#[test]
#[ignore = "M7 WI-10"]
fn a_condition_has_its_id_schema_and_blackboard_metadata() {
    let (id, schema, metadata) = only_capability::<ConditionOnly>();

    assert_eq!(
        (id.as_str(), schema.as_str(), metadata.as_str()),
        (CONDITION_ID, CONDITION_SCHEMA, CONDITION_METADATA)
    );
}

#[test]
#[ignore = "M7 WI-10"]
fn an_action_has_its_id_schema_and_blackboard_metadata() {
    let (id, schema, metadata) = only_capability::<ActionOnly>();

    assert_eq!(
        (id.as_str(), schema.as_str(), metadata.as_str()),
        (ACTION_ID, ACTION_SCHEMA, ACTION_METADATA)
    );
}
