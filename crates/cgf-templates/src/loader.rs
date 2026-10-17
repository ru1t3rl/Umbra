//! STUB (WI-7, WI-8): the loader entry points.
#![expect(clippy::todo, reason = "stub until WI-7/WI-8: signatures only")]

use std::path::{Path, PathBuf};

use cgf_core::{ComponentId, Registry};

use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::resolve::TemplateSet;
use crate::scenario::Scenario;
use crate::schema::SchemaError;
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
    _registry: &'r Registry,
}

impl<'r> Loader<'r> {
    /// Compiles every registered schema.
    pub fn new(_registry: &'r Registry) -> Result<Self, LoaderError> {
        todo!("WI-7")
    }

    /// Loads a set of template files.
    pub fn templates_from_sources(
        &self,
        _files: &[SourceFile],
    ) -> Result<TemplateSet, Diagnostics> {
        todo!("WI-7")
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
