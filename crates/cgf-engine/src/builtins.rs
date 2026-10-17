//! STUB (WI-9): the registry with every built-in component (D-8, REQ-T-1).
#![expect(clippy::todo, reason = "stub until WI-9: signatures only")]

use std::sync::Arc;

use cgf_core::{
    BuildError, ComponentId, Factory, ParamSchema, ParamValue, Registry, RegistryError,
    TerrainProvider,
};

/// Builds `builtin.terrain.flat` from `{ height_m }`.
pub struct FlatTerrainFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl FlatTerrainFactory {
    /// The factory, with its schema from `schemas/builtin.terrain.flat.json`.
    pub fn new() -> Self {
        todo!("WI-9")
    }
}

impl Default for FlatTerrainFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Factory<dyn TerrainProvider> for FlatTerrainFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, _params: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
        todo!("WI-9")
    }
}

/// A registry with `cgf_motion::register_builtins` and [`FlatTerrainFactory`].
///
/// # Errors
/// [`RegistryError`] if a built-in registers twice (not expected).
pub fn builtin_registry() -> Result<Registry, RegistryError> {
    todo!("WI-9")
}
