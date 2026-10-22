//! The loader entry points (REQ-T-1..6).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use umbra_core::{ComponentId, ComponentKind, Provenance, Registry, TickRate};

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
    /// The most graphs on one `subgraph` chain, the referencing root graph included. 0 means 1
    /// (a graph alone). `register_graphs` in `umbra-behavior` always applies
    /// [`umbra_core::DEFAULT_MAX_SUBGRAPH_DEPTH`], so a limit above it accepts chains that
    /// registration then refuses (ASM-073). Values supplied at a call site are followed only to
    /// `MAX_CHAIN` (256) graphs deep, whatever this says (ASM-106 d).
    pub max_subgraph_depth: u32,
    /// The tick rate of a scenario that omits `[scenario] tick_rate_hz`; 60 Hz unless changed
    /// (ADR 0010 note of 2026-10-19, ADR 0029). An explicit value in the file always wins.
    pub default_tick_rate: TickRate,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            max_subgraph_depth: umbra_core::DEFAULT_MAX_SUBGRAPH_DEPTH,
            default_tick_rate: TickRate::default(),
        }
    }
}

/// The result of `validate_path`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationReport {
    /// The files that were checked, in sorted path order.
    pub files_checked: Vec<PathBuf>,
    /// Valid scenario files that omitted `tick_rate_hz`, with the default rate that applied, in
    /// sorted path order.
    pub defaulted_tick_rates: Vec<(PathBuf, TickRate)>,
    /// Every diagnostic, sorted.
    pub diagnostics: Vec<Diagnostic>,
    /// The warnings of the valid scenarios, sorted (M6, ASM-290); they do not make the check fail.
    pub warnings: Vec<Diagnostic>,
}

/// What a scoped loader knows about the graph files of its load (WI-14, `overlay.rs`); empty for
/// a plain loader.
#[derive(Debug, Clone, Default)]
pub(crate) struct GraphScope {
    /// IDs of graphs that exist but cannot be used (syntax or other errors, duplicates): naming
    /// one is not an `unknown_id`, its own error is reported elsewhere (ASM-056).
    pub(crate) silent: BTreeSet<ComponentId>,
    /// Graph files with syntax errors whose `id` is gone, named in a note of `unknown_id`
    /// (ASM-054 rule 3).
    pub(crate) unreadable: Vec<PathBuf>,
}

/// Loads and validates templates and scenarios against a registry.
pub struct Loader<'r> {
    pub(crate) registry: &'r Registry,
    pub(crate) schemas: BTreeMap<(ComponentKind, ComponentId), CompiledSchema>,
    pub(crate) options: LoaderOptions,
    pub(crate) scope: GraphScope,
}

impl<'r> Loader<'r> {
    /// Compiles every registered schema.
    ///
    /// # Errors
    /// A registered schema outside the ADR 0009 subset is a component bug: the error names the
    /// component and the offending keyword.
    pub fn new(registry: &'r Registry) -> Result<Self, LoaderError> {
        Self::with_options(registry, LoaderOptions::default())
    }

    /// Like [`Loader::new`] with explicit options.
    ///
    /// # Errors
    /// As [`Loader::new`].
    pub fn with_options(
        registry: &'r Registry,
        options: LoaderOptions,
    ) -> Result<Self, LoaderError> {
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
        Ok(Self {
            registry,
            schemas,
            options,
            scope: GraphScope::default(),
        })
    }

    /// Reads and validates behavior graph files (`*.behavior.toml` contents) into a [`GraphSet`];
    /// `source` is the provenance recorded in each [`umbra_core::GraphDef`].
    ///
    /// # Errors
    /// Every diagnostic of every file and of the set, sorted.
    pub fn graphs_from_sources(
        &self,
        files: &[SourceFile],
        source: Provenance,
    ) -> Result<GraphSet, Diagnostics> {
        let refs: Vec<&SourceFile> = files.iter().collect();
        let loaded = self.load_graphs(&refs, source);
        match Diagnostics::new(loaded.diagnostics) {
            Some(errors) => Err(errors),
            None => Ok(loaded.set),
        }
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
