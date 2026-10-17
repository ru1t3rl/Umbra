//! Resolved templates (ADR 0010). Data types are real; resolution is STUB (WI-7).

use std::collections::BTreeMap;
use std::path::PathBuf;

use cgf_core::{ComponentId, ParamValue, TemplateId};

/// A component by ID with its parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRef {
    /// The component's ID.
    pub id: ComponentId,
    /// Its parameters.
    pub params: ParamValue,
}

/// A template with inheritance applied.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTemplate {
    /// The template's ID.
    pub id: TemplateId,
    /// The inheritance chain, the template first and the root last.
    pub chain: Vec<TemplateId>,
    /// The description, not inherited.
    pub description: Option<String>,
    /// The motion model and its merged parameters.
    pub motion: ComponentRef,
    /// Opaque `network.<adapter>` tables (REQ-T-4).
    pub network: BTreeMap<String, ParamValue>,
    /// The file the template was defined in.
    pub source: PathBuf,
}

/// All templates of a load, by ID.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateSet {
    templates: BTreeMap<TemplateId, ResolvedTemplate>,
}

impl TemplateSet {
    /// The template with this ID.
    pub fn get(&self, id: &TemplateId) -> Option<&ResolvedTemplate> {
        self.templates.get(id)
    }

    /// All templates in ID order.
    pub fn iter(&self) -> impl Iterator<Item = &ResolvedTemplate> {
        self.templates.values()
    }

    /// How many templates there are.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}
