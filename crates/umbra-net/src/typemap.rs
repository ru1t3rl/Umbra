//! Entity type token to template mapping (ADR 0031, REQ-N-4, ASM-232, ASM-236).
//!
//! Signature stub of M6 step 4; the bodies arrive with WI-5.
#![expect(
    clippy::todo,
    dead_code,
    clippy::needless_pass_by_value,
    unused_variables,
    reason = "M6 step-4 stubs, bodies from WI-5"
)]

use std::collections::BTreeMap;

use umbra_core::TemplateId;
use umbra_core::net::NetEntityType;

/// A problem found while building a [`TypeMap`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypeMapWarning {
    /// Several templates declare one token; the smallest template id was kept.
    DuplicateDeclaration {
        /// The shared token.
        entity_type: NetEntityType,
        /// The template that won.
        kept: TemplateId,
        /// The other declaring templates, ascending.
        ignored: Vec<TemplateId>,
    },
}

/// Maps an adapter's entity type tokens to templates. Precedence: template declaration, then the
/// scenario's `type_map`, then the default template.
#[derive(Debug, Clone, Default)]
pub struct TypeMap {
    by_type: BTreeMap<NetEntityType, TemplateId>,
    default: Option<TemplateId>,
}

impl TypeMap {
    /// Builds the map from the templates' declarations, the scenario's overrides and the default.
    #[must_use]
    pub fn new(
        declared: &[(TemplateId, NetEntityType)],
        overrides: &[(NetEntityType, TemplateId)],
        default: Option<TemplateId>,
    ) -> (Self, Vec<TypeMapWarning>) {
        todo!("WI-5: TypeMap::new")
    }

    /// The template for `t`; the default for an unknown or absent token; `None` if there is none.
    #[must_use]
    pub fn resolve(&self, t: Option<&NetEntityType>) -> Option<&TemplateId> {
        todo!("WI-5: TypeMap::resolve")
    }
}
