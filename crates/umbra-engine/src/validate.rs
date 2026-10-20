//! Validating a posted behavior graph document (REQ-X-11, ASM-154).
//!
//! The document is loaded by the same graph loader as the files of a scenario, together with the
//! other file graphs of the session, so call sites in those graphs are checked against it too.

use umbra_core::{Provenance, Registry};
use umbra_templates::diagnostic::sort_and_dedup;
use umbra_templates::{Diagnostic, Loader, SourceFile};

use crate::scenario::ScenarioError;

/// The outcome of validating a graph document.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct GraphValidation {
    /// Whether there are no diagnostics.
    pub valid: bool,
    /// Every diagnostic, sorted, located in `<name>` for the document and in the files of the other
    /// graphs for theirs.
    pub diagnostics: Vec<Diagnostic>,
}

/// Validates `document` (named `name` in diagnostics) against `registry`, alone.
///
/// # Errors
/// [`ScenarioError::Loader`] if a registered component schema is broken.
pub fn validate_graph_document(
    registry: &Registry,
    name: &str,
    document: &str,
) -> Result<GraphValidation, ScenarioError> {
    validate_with_others(registry, name, document, Vec::new(), Vec::new())
}

/// Validates `document` together with `others`, the sources of the other graph files. `unread`
/// are the diagnostics of other files that could not be read; they are part of the result.
pub(crate) fn validate_with_others(
    registry: &Registry,
    name: &str,
    document: &str,
    mut files: Vec<SourceFile>,
    unread: Vec<Diagnostic>,
) -> Result<GraphValidation, ScenarioError> {
    files.push(SourceFile::new(name, document));
    let loader = Loader::new(registry)?;
    let mut diagnostics = unread;
    if let Err(errors) = loader.graphs_from_sources(&files, Provenance::File) {
        diagnostics.extend(errors.into_vec());
    }
    sort_and_dedup(&mut diagnostics);
    Ok(GraphValidation {
        valid: diagnostics.is_empty(),
        diagnostics,
    })
}
