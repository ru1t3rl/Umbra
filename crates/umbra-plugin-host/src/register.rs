//! Registration of plugin capabilities in the one registry (D-8, REQ-P-3, REQ-P-4, REQ-B-13,
//! REQ-T-1, ADR 0020 "Metadata", ASM-091 d, ASM-099 e).
//!
//! Every `behavior` capability becomes a registered behavior with `Provenance::Plugin` (read-only
//! in the catalog): description from the schema's top-level `description`, version from the
//! capability (else the plugin), `param_types`, `outputs` and `result` from the metadata; no
//! subgraphs (v0 metadata has none, so a plugin behavior counts one level of subgraph depth).
//! Registration is all-or-nothing: the capabilities of every loaded plugin go into a copy of the
//! registry, which replaces the original only when all of them were accepted.

use std::sync::Arc;

use umbra_core::{ComponentInfo, Provenance, Registry};

use crate::bridge::batch::PluginCapability;
use crate::bridge::factory::PluginFactory;
use crate::descriptor::Capability;
use crate::error::PluginError;

/// Registers `capabilities` (the host's, in load order then descriptor order); all-or-nothing. Each
/// factory holds its capability and so its image: a registry (or a clone) keeps the library loaded.
pub(crate) fn register<'a>(
    capabilities: impl IntoIterator<Item = &'a Arc<PluginCapability>>,
    registry: &mut Registry,
) -> Result<(), PluginError> {
    let mut staged = registry.clone();
    for capability in capabilities {
        let info = component_info(capability.capability());
        let factory = PluginFactory::new(Arc::clone(capability));
        staged.register_behavior(Arc::new(factory), info)?;
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
