//! The `builtin.ground.kinematic` factory (REQ-T-1, REQ-T-3, REQ-M-4, ADR 0008).
//!
//! Parameters carry their unit in the key name; angles are converted from degrees once, with the
//! same `RadiansPerSecond::from_degrees_per_second` the typed constructor path uses.

use std::sync::Arc;

use cgf_core::{
    BuildError, ComponentId, Factory, MetersPerSecond, MetersPerSecondSquared, MotionModel,
    ParamSchema, ParamValue, RadiansPerSecond, Registry, RegistryError,
};

use crate::ground::{GROUND_KINEMATIC_ID, GroundKinematic, GroundKinematicParams};

const SCHEMA_JSON: &str = include_str!("../schemas/builtin.ground.kinematic.json");

/// Builds [`crate::GroundKinematic`] from validated parameters.
pub struct GroundKinematicFactory {
    id: ComponentId,
    schema: ParamSchema,
}

/// Reads a finite, strictly positive number at `key`.
fn positive(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let pointer = format!("/{key}");
    let value = params
        .get(key)
        .ok_or_else(|| BuildError::new(format!("missing parameter `{key}`")).at(&pointer))?;
    let number = value
        .as_f64()
        .ok_or_else(|| BuildError::new(format!("`{key}` must be a number")).at(&pointer))?;
    if number.is_finite() && number > 0.0 {
        Ok(number)
    } else {
        Err(BuildError::new(format!("`{key}` must be greater than 0, got {number}")).at(pointer))
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
            max_speed: MetersPerSecond(positive(params, "max_speed_mps")?),
            max_turn_rate: RadiansPerSecond::from_degrees_per_second(positive(
                params,
                "max_turn_rate_dps",
            )?),
            max_accel: MetersPerSecondSquared(positive(params, "max_accel_mps2")?),
        };
        let model = GroundKinematic::new(limits).map_err(|e| BuildError::new(e.to_string()))?;
        Ok(Arc::new(model))
    }
}

/// Registers every built-in motion model.
///
/// # Errors
/// [`RegistryError::Duplicate`] if called twice on one registry.
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
    fn build_never_panics_on_odd_params() {
        let f = GroundKinematicFactory::new();
        for v in [
            ParamValue::Null,
            serde_json::json!([]),
            serde_json::json!("x"),
            serde_json::json!({"max_speed_mps": 1e308, "max_turn_rate_dps": 1e308, "max_accel_mps2": 1}),
        ] {
            let _ = f.build(&v);
        }
    }
}
