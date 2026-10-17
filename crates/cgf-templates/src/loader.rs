//! The loader entry points (REQ-T-1..6).

use std::collections::BTreeMap;
use std::path::PathBuf;

use cgf_core::{ComponentId, ComponentKind, Registry};

use crate::diagnostic::{Diagnostic, DiagnosticCode, Diagnostics};
use crate::resolve::TemplateSet;
use crate::scenario::Scenario;
use crate::schema::{CompiledSchema, SchemaError};
use crate::source::SourceFile;

/// A registered component schema is outside the supported subset (a component bug).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoaderError {
    /// The schema of a registered component does not compile.
    #[error("{kind} {id}: {source}")]
    Schema {
        /// Noun of the component kind.
        kind: &'static str,
        /// The component.
        id: ComponentId,
        /// Why.
        #[source]
        source: SchemaError,
    },
}

/// The result of `validate_path`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationReport {
    /// The files that were checked, in sorted path order.
    pub files_checked: Vec<PathBuf>,
    /// Every diagnostic, sorted.
    pub diagnostics: Vec<Diagnostic>,
}

/// Loads and validates templates and scenarios against a registry.
pub struct Loader<'r> {
    pub(crate) registry: &'r Registry,
    pub(crate) schemas: BTreeMap<(ComponentKind, ComponentId), CompiledSchema>,
}

impl<'r> Loader<'r> {
    /// Compiles every registered schema.
    ///
    /// # Errors
    /// A registered schema outside the ADR 0009 subset is a component bug: the error names the
    /// component and the offending keyword.
    pub fn new(registry: &'r Registry) -> Result<Self, LoaderError> {
        let mut schemas = BTreeMap::new();
        for &kind in ComponentKind::ALL {
            for id in registry.ids(kind) {
                let Some(schema) = registry.param_schema(kind, id) else {
                    continue;
                };
                let compiled =
                    CompiledSchema::compile(schema).map_err(|source| LoaderError::Schema {
                        kind: kind.noun(),
                        id: id.clone(),
                        source,
                    })?;
                schemas.insert((kind, id.clone()), compiled);
            }
        }
        Ok(Self { registry, schemas })
    }

    /// Loads a set of template files.
    ///
    /// # Errors
    /// Every diagnostic of every file and of the set, sorted.
    pub fn templates_from_sources(&self, files: &[SourceFile]) -> Result<TemplateSet, Diagnostics> {
        let loaded = self.load_templates(files);
        match Diagnostics::new(loaded.diagnostics) {
            Some(errors) => Err(errors),
            None => Ok(loaded.set),
        }
    }

    /// Loads a scenario file and template files, all in memory (`template_paths` is not read).
    ///
    /// # Errors
    /// Every diagnostic of every file, sorted.
    pub fn scenario_from_sources(
        &self,
        scenario: &SourceFile,
        templates: &[SourceFile],
    ) -> Result<Scenario, Diagnostics> {
        let (loaded, diagnostics) = self.build_scenario(scenario, templates, false);
        match (loaded, Diagnostics::new(diagnostics)) {
            (_, Some(errors)) => Err(errors),
            (Some(scenario), None) => Ok(scenario),
            (None, None) => Err(Diagnostics::from_one(Diagnostic::new(
                DiagnosticCode::NotAScenario,
                "the file is not a scenario",
                None,
            ))),
        }
    }
}
