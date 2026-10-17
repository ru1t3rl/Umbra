//! STUB (WI-3): the `builtin.ground.kinematic` factory (REQ-T-1, REQ-T-3, REQ-M-4).
#![expect(clippy::todo, reason = "stub until WI-3: signatures only")]

use std::sync::Arc;

use cgf_core::{
    BuildError, ComponentId, Factory, MotionModel, ParamSchema, ParamValue, Registry, RegistryError,
};

/// Builds [`crate::GroundKinematic`] from validated parameters.
pub struct GroundKinematicFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl GroundKinematicFactory {
    /// The factory, with its schema loaded from `schemas/builtin.ground.kinematic.json`.
    pub fn new() -> Self {
        todo!("WI-3")
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

    fn build(&self, _params: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
        todo!("WI-3")
    }
}

/// Registers every built-in motion model.
///
/// # Errors
/// [`RegistryError::Duplicate`] if called twice on one registry.
pub fn register_builtins(_registry: &mut Registry) -> Result<(), RegistryError> {
    todo!("WI-3")
}
