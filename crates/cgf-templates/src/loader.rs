//! The loader entry points (REQ-T-1..6).

use std::collections::BTreeMap;
use std::path::PathBuf;

use cgf_core::{ComponentId, ComponentKind, Provenance, Registry};

use crate::diagnostic::{Diagnostic, DiagnosticCode, Diagnostics};
use crate::graph_resolve::GraphSet;
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

/// Loader settings (library API only, no file key; ASM-073).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LoaderOptions {
    /// The most graphs on one `subgraph` chain, the referencing root graph included.
    pub max_subgraph_depth: u32,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            max_subgraph_depth: 16,
        }
    }
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

    /// Like [`Loader::new`] with explicit options.
    ///
    /// # Errors
    /// As [`Loader::new`].
    #[expect(clippy::todo, reason = "M3 WI-13 stub")]
    pub fn with_options(
        _registry: &'r Registry,
        _options: LoaderOptions,
    ) -> Result<Self, LoaderError> {
        todo!("M3 WI-13")
    }

    /// Reads and validates behavior graph files (`*.behavior.toml` contents) into a [`GraphSet`];
    /// `source` is the provenance recorded in each [`cgf_core::GraphDef`].
    ///
    /// # Errors
    /// Every diagnostic of every file and of the set, sorted.
    #[expect(clippy::todo, reason = "M3 WI-12 stub")]
    pub fn graphs_from_sources(
        &self,
        _files: &[SourceFile],
        _source: Provenance,
    ) -> Result<GraphSet, Diagnostics> {
        todo!("M3 WI-12")
    }

    /// Loads a set of template files.
    ///
    /// # Errors
    /// Every diagnostic of every file and of the set, sorted.
    pub fn templates_from_sources(&self, files: &[SourceFile]) -> Result<TemplateSet, Diagnostics> {
        let loaded = self.load_templates(files, &[]);
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
        let (loaded, diagnostics) = self.build_scenario(scenario, templates, &[]);
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
