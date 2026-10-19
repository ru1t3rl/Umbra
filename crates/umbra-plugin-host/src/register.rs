//! Registration of plugin capabilities in the one registry (D-8, REQ-P-3, REQ-P-4, REQ-B-13, REQ-T-1,
//! ADR 0020 "Metadata", ASM-091 d, ASM-099 e).
//!
//! Every `behavior` capability becomes a registered behavior with `Provenance::Plugin` (read-only in the
//! catalog): description from the schema's top-level `description`, version from the capability (else
//! the plugin), `param_types`, `outputs` and `result` from the metadata; no subgraphs (v0 metadata has
//! none, so a plugin behavior counts one level of subgraph depth). Registration is all-or-nothing: the
//! capabilities of every loaded plugin go into a copy of the registry, which replaces the original only
//! when all of them were accepted.

use std::sync::Arc;

use umbra_core::{
    Behavior, BuildError, ComponentId, ComponentInfo, Factory, ParamSchema, ParamValue, Provenance,
    Registry,
};

use crate::descriptor::Capability;
use crate::error::PluginError;
use crate::lifecycle::LoadedImage;

/// Registers every capability of `images`, in load order then descriptor order; all-or-nothing.
pub(crate) fn register<'a>(
    images: impl IntoIterator<Item = &'a Arc<LoadedImage>>,
    registry: &mut Registry,
) -> Result<(), PluginError> {
    let mut staged = registry.clone();
    for image in images {
        for (index, capability) in image.descriptor().capabilities.iter().enumerate() {
            let factory = PluginFactory {
                image: Arc::clone(image),
                index,
                id: capability.id.clone(),
                schema: capability.schema.clone(),
            };
            staged.register_behavior(Arc::new(factory), component_info(capability))?;
        }
    }
    *registry = staged;
    Ok(())
}

/// The catalog metadata of a capability (ADR 0020).
fn component_info(capability: &Capability) -> ComponentInfo {
    let meta = &capability.metadata;
    let mut info = ComponentInfo::new(
        capability.description.clone(),
        Provenance::Plugin,
        capability.version.clone(),
    );
    for (name, ty) in &meta.param_types {
        info = info.with_param_type(name.clone(), *ty);
    }
    for (port, ty) in &meta.outputs {
        info = info.with_output(port.clone(), *ty);
    }
    if let Some(result) = &meta.result {
        info = info.with_result(result.clone());
    }
    info
}

/// The registry factory of one plugin behavior capability. It holds its image, so a registry (or a
/// clone of it) keeps the library loaded (ADR 0021).
struct PluginFactory {
    image: Arc<LoadedImage>,
    #[expect(
        dead_code,
        reason = "selects the capability's vtable in the behavior bridge (M4 WI-9)"
    )]
    index: usize,
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn Behavior> for PluginFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    /// The behavior bridge (`configure` through the vtable, host-bound build scope) is M4 WI-9; until
    /// then every build is refused with a [`BuildError`] naming the plugin.
    fn build(&self, _params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        Err(BuildError::new(format!(
            "plugin behavior {} of {}: building plugin behaviors arrives with the behavior bridge \
             (M4 WI-9)",
            self.id,
            self.image.descriptor().id
        )))
    }
}
