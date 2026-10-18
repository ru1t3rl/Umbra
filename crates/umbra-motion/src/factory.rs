//! The `builtin.ground.kinematic` factory (REQ-T-1, REQ-T-3, REQ-M-4, ADR 0008).
//!
//! Parameters carry their unit in the key name; angles are converted from degrees once, with the
//! same `RadiansPerSecond::from_degrees_per_second` the typed constructor path uses.

use std::sync::Arc;

use umbra_core::{
    BuildError, ComponentId, Factory, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    ParamSchema, ParamValue, RadiansPerSecond, Registry, RegistryError,
};

use crate::ground::{
    GROUND_KINEMATIC_ID, GroundKinematic, GroundKinematicParams, MotionParamError,
};

const SCHEMA_JSON: &str = include_str!("../schemas/builtin.ground.kinematic.json");

/// Builds [`crate::GroundKinematic`] from validated parameters.
pub struct GroundKinematicFactory {
    id: ComponentId,
    schema: ParamSchema,
}

/// Reads the number at `key`; a missing or non-numeric value is located at the key.
fn number(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let pointer = format!("/{key}");
    let value = params
        .get(key)
        .ok_or_else(|| BuildError::new(format!("missing parameter `{key}`")).at(&pointer))?;
    value
        .as_f64()
        .ok_or_else(|| BuildError::new(format!("`{key}` must be a number")).at(pointer))
}

/// Maps a model limit name to the parameter key that sets it (`None` for an unknown name).
fn key_of(name: &str) -> Option<&'static str> {
    match name {
        "max_speed" => Some("max_speed_mps"),
        "max_turn_rate" => Some("max_turn_rate_dps"),
        "max_accel" => Some("max_accel_mps2"),
        _ => None,
    }
}

/// Locates a model error at the TOML key that caused it.
fn located(error: &MotionParamError) -> BuildError {
    match error {
        MotionParamError::NotPositive { name, value } => match key_of(name) {
            Some(key) => BuildError::new(format!("`{key}` must be greater than 0, got {value}"))
                .at(format!("/{key}")),
            None => BuildError::new(error.to_string()),
        },
        // The built-in id is a constant checked by a unit test: no parameter is at fault, so
        // the error stays unlocated (it points at the params table).
        MotionParamError::InvalidId => BuildError::new(error.to_string()),
    }
}

impl GroundKinematicFactory {
    /// The factory, with its schema loaded from `schemas/builtin.ground.kinematic.json`.
    ///
    /// # Panics
    /// Never in practice: the id constant and the embedded schema are valid (unit-tested).
    #[expect(
        clippy::expect_used,
        reason = "embedded constants, proven valid by tests"
    )]
    pub fn new() -> Self {
        Self {
            id: ComponentId::new(GROUND_KINEMATIC_ID).expect("built-in id is valid"),
            schema: ParamSchema::from_json_str(SCHEMA_JSON).expect("embedded schema is valid"),
        }
    }
}

impl Default for GroundKinematicFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Factory<dyn MotionModel> for GroundKinematicFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
        let limits = GroundKinematicParams {
            max_speed: MetersPerSecond(number(params, "max_speed_mps")?),
            max_turn_rate: RadiansPerSecond::from_degrees_per_second(number(
                params,
                "max_turn_rate_dps",
            )?),
            max_accel: MetersPerSecondSquared(number(params, "max_accel_mps2")?),
        };
        let model = GroundKinematic::new(limits).map_err(|e| located(&e))?;
        Ok(Arc::new(model))
    }
}

/// Registers every built-in motion model.
///
/// # Errors
/// [`RegistryError::Duplicate`] if called twice on one registry, or if any kind already holds
/// the id (IDs are unique across kinds, ASM-063).
pub fn register_builtins(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_motion_model(Arc::new(GroundKinematicFactory::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schema_and_id_are_valid() {
        let f = GroundKinematicFactory::new();
        assert_eq!(f.id().as_str(), GROUND_KINEMATIC_ID);
        assert!(f.param_schema().as_json().is_object());
    }

    #[test]
    fn build_rejects_non_object_params_with_a_pointer() {
        let f = GroundKinematicFactory::new();
        for v in [
            ParamValue::Null,
            serde_json::json!([]),
            serde_json::json!("x"),
        ] {
            let error = f.build(&v).err();
            let error = error.unwrap_or_else(|| panic!("{v} must be rejected"));
            assert_eq!(error.param.as_deref(), Some("/max_speed_mps"), "{v}");
        }
    }

    #[test]
    fn build_does_not_panic_on_huge_values() {
        let f = GroundKinematicFactory::new();
        let v = serde_json::json!({"max_speed_mps": 1e308, "max_turn_rate_dps": 1e308, "max_accel_mps2": 1});
        let _ = f.build(&v);
    }
}
