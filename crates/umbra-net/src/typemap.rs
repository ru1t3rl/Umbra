//! Entity type token to template mapping (ADR 0031, REQ-N-4, ASM-232, ASM-236).

use std::collections::BTreeMap;

use umbra_core::TemplateId;
use umbra_core::net::NetEntityType;

/// A problem found while building a [`TypeMap`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TypeMapWarning {
    /// Several templates declare one token; the smallest template id was kept.
    #[error(
        "entity type {entity_type:?} is declared by several templates; kept {kept}, ignored {ignored:?}"
    )]
    DuplicateDeclaration {
        /// The shared token.
        entity_type: NetEntityType,
        /// The template that won.
        kept: TemplateId,
        /// The other declaring templates, ascending.
        ignored: Vec<TemplateId>,
    },
}

/// Maps an adapter's entity type tokens to templates. A `type_map` entry overrides a template
/// declaration; `default` applies only to an absent token or one with no entry.
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
        let mut groups: BTreeMap<&NetEntityType, Vec<&TemplateId>> = BTreeMap::new();
        for (template, token) in declared {
            groups.entry(token).or_default().push(template);
        }
        let mut by_type = BTreeMap::new();
        let mut warnings = Vec::new();
        for (token, mut templates) in groups {
            templates.sort();
            templates.dedup();
            let kept = templates.remove(0).clone();
            if !templates.is_empty() {
                warnings.push(TypeMapWarning::DuplicateDeclaration {
                    entity_type: token.clone(),
                    kept: kept.clone(),
                    ignored: templates.into_iter().cloned().collect(),
                });
            }
            by_type.insert(token.clone(), kept);
        }
        // The scenario's `type_map` overrides declarations. A repeated token cannot come from a
        // TOML table (duplicate keys are rejected), only from a programmatic caller; the last wins.
        for (token, template) in overrides {
            by_type.insert(token.clone(), template.clone());
        }
        (Self { by_type, default }, warnings)
    }

    /// The template for `t`; the default for an unknown or absent token; `None` if there is none.
    #[must_use]
    pub fn resolve(&self, t: Option<&NetEntityType>) -> Option<&TemplateId> {
        t.and_then(|t| self.by_type.get(t))
            .or(self.default.as_ref())
    }
}
