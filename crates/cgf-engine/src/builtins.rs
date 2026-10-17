//! The registry with every built-in component (D-8, REQ-T-1, REQ-T-3).

use std::sync::Arc;

use cgf_core::{
    BuildError, ComponentId, Factory, FlatTerrain, Meters, ParamSchema, ParamValue, Registry,
    RegistryError, TerrainProvider,
};

/// ID of the flat terrain provider.
pub const FLAT_TERRAIN_ID: &str = "builtin.terrain.flat";

const SCHEMA_JSON: &str = include_str!("../schemas/builtin.terrain.flat.json");

/// Builds `builtin.terrain.flat` from `{ height_m }`.
pub struct FlatTerrainFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl FlatTerrainFactory {
    /// The factory, with its schema from `schemas/builtin.terrain.flat.json`.
    ///
    /// # Panics
    /// Never in practice: the id constant and the embedded schema are valid (unit-tested).
    #[expect(
        clippy::expect_used,
        reason = "embedded constants, proven valid by tests"
    )]
    pub fn new() -> Self {
        Self {
            id: ComponentId::new(FLAT_TERRAIN_ID).expect("built-in id is valid"),
            schema: ParamSchema::from_json_str(SCHEMA_JSON).expect("embedded schema is valid"),
        }
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

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
        let height = params
            .get("height_m")
            .ok_or_else(|| BuildError::new("missing parameter `height_m`").at("/height_m"))?
            .as_f64()
            .filter(|h| h.is_finite())
            .ok_or_else(|| BuildError::new("`height_m` must be a finite number").at("/height_m"))?;
        Ok(Arc::new(FlatTerrain {
            height: Meters(height),
        }))
    }
}

/// A registry with `cgf_motion::register_builtins` and [`FlatTerrainFactory`].
///
/// # Errors
/// [`RegistryError`] if a built-in registers twice (not expected).
pub fn builtin_registry() -> Result<Registry, RegistryError> {
    let mut registry = Registry::new();
    cgf_motion::register_builtins(&mut registry)?;
    registry.register_terrain(Arc::new(FlatTerrainFactory::new()))?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schema_and_id_are_valid() {
        let f = FlatTerrainFactory::new();
        assert_eq!(f.id().as_str(), FLAT_TERRAIN_ID);
        assert!(f.param_schema().as_json().is_object());
    }

    #[test]
    fn build_locates_bad_params() {
        let f = FlatTerrainFactory::new();
        for v in [
            ParamValue::Null,
            serde_json::json!({}),
            serde_json::json!({"height_m": "tall"}),
        ] {
            let e = f.build(&v).err();
            assert_eq!(e.and_then(|e| e.param).as_deref(), Some("/height_m"), "{v}");
        }
    }

    #[test]
    fn registry_holds_both_builtins_once() {
        let mut r = builtin_registry().unwrap_or_default();
        assert_eq!(r.ids(cgf_core::ComponentKind::Terrain).len(), 1);
        assert!(cgf_motion::register_builtins(&mut r).is_err());
    }
}
