//! The loader entry points (WI-7 templates; WI-8 scenarios is still a STUB).
#![expect(clippy::todo, reason = "stub until WI-8: scenario entry points")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cgf_core::{ComponentId, ComponentKind, Registry};

use crate::diagnostic::{Diagnostic, Diagnostics};
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
        let (set, diagnostics) = self.load_templates(files);
        match Diagnostics::new(diagnostics) {
            Some(errors) => Err(errors),
            None => Ok(set),
        }
    }

    /// Loads a scenario file and template files, all in memory.
    pub fn scenario_from_sources(
        &self,
        _scenario: &SourceFile,
        _templates: &[SourceFile],
    ) -> Result<Scenario, Diagnostics> {
        todo!("WI-8")
    }

    /// Loads a scenario file and the templates under its `template_paths`.
    pub fn load_scenario(&self, _path: &Path) -> Result<Scenario, Diagnostics> {
        todo!("WI-8")
    }

    /// Checks a scenario file, a template file or a directory (recursively). Never runs anything.
    pub fn validate_path(&self, _path: &Path) -> ValidationReport {
        todo!("WI-8")
    }
}
