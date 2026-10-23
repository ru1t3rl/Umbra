//! AT-14 (M7, WI-5 row): `umbra schema component builtin.sea.kinematic` prints the schema the
//! registry serves. Requirements: REQ-T-3, REQ-X-2, ADR 0051.

#![forbid(unsafe_code)]

mod common;

use common::{cli, json_of, stderr};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::{ComponentId, ComponentKind};

#[test]
fn schema_component_lists_the_sea_model() {
    let out = cli(&["schema", "component", "builtin.sea.kinematic"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let registry = builtin_registry().unwrap_or_else(|e| panic!("registry: {e}"));
    let id = ComponentId::new("builtin.sea.kinematic").unwrap_or_else(|e| panic!("id: {e}"));
    let served = registry
        .param_schema(ComponentKind::MotionModel, &id)
        .unwrap_or_else(|| panic!("{id} is not registered"))
        .as_json()
        .clone();
    assert_eq!(json_of(&out), served);
    assert!(served["properties"]["min_turn_radius_m"].is_object());
}
