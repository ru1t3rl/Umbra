//! The registry with every built-in component (D-8, REQ-T-1, REQ-T-3).

use std::sync::Arc;

use cgf_behavior::GraphRegistrationError;
use cgf_core::{
    BuildError, ComponentId, Factory, FlatTerrain, Meters, ParamSchema, ParamValue, Provenance,
    Registry, RegistryError, TerrainProvider,
};
use cgf_templates::{Diagnostics, Loader, LoaderError, SourceFile};

pub use crate::visual::{VISUAL_SENSOR_ID, VisualSensorFactory};

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
            .ok_or_else(|| BuildError::new("`height_m` must be a number").at("/height_m"))?;
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

/// Why [`standard_registry`] failed (not expected: every part is built in and tested).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StandardRegistryError {
    /// A component registered twice.
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A registered schema is outside the supported subset.
    #[error(transparent)]
    Loader(#[from] LoaderError),
    /// A built-in graph file has errors.
    #[error("built-in graphs: {0}")]
    BuiltinGraphs(Diagnostics),
    /// A built-in graph could not be registered.
    #[error(transparent)]
    Graphs(#[from] GraphRegistrationError),
}

/// The built-in graph files, parsed with the loader (ASM-077): `(file name, contents)`.
const BUILTIN_GRAPH_FILES: [(&str, &str); 2] = [
    (
        "builtin.fsm.patrol_and_engage.behavior.toml",
        include_str!("../behaviors/builtin.fsm.patrol_and_engage.behavior.toml"),
    ),
    (
        "builtin.subgraph.engage_target.behavior.toml",
        include_str!("../behaviors/builtin.subgraph.engage_target.behavior.toml"),
    ),
];

/// [`builtin_registry`] plus the built-in conditions and actions, `builtin.visual` and the built-in
/// graphs (`builtin.fsm.patrol_and_engage`, `builtin.subgraph.engage_target`), ASM-084. The
/// graphs are `Provenance::Builtin`, read-only in the catalog.
///
/// # Errors
/// [`StandardRegistryError`]; not expected, every part is built in and tested.
pub fn standard_registry() -> Result<Registry, StandardRegistryError> {
    let mut registry = builtin_registry()?;
    cgf_behavior::register_builtins(&mut registry)?;
    registry.register_sensor(Arc::new(VisualSensorFactory::new()))?;
    let files: Vec<SourceFile> = BUILTIN_GRAPH_FILES
        .iter()
        .map(|(name, text)| SourceFile::new(*name, *text))
        .collect();
    let graphs = Loader::new(&registry)?
        .graphs_from_sources(&files, Provenance::Builtin)
        .map_err(StandardRegistryError::BuiltinGraphs)?;
    cgf_behavior::register_graphs(&mut registry, graphs.iter())?;
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
