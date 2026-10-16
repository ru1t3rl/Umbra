//! Loader around `cargo metadata --no-deps`. STUB.

use super::WorkspaceGraph;

/// Failure to load the workspace graph.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// `cargo metadata` failed or returned bad JSON.
    #[error("cargo metadata failed: {0}")]
    Metadata(String),
}

/// Runs `cargo metadata --no-deps --format-version 1 --manifest-path <p>`.
#[expect(clippy::todo, reason = "stub until WI-3")]
pub fn load_graph(manifest_path: &std::path::Path) -> Result<WorkspaceGraph, LoadError> {
    let _ = manifest_path;
    todo!("WI-3")
}
