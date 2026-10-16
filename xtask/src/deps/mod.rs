//! Dependency-rule check (REQ-A-1..3). STUB: signatures only, see `docs/plans/M0.md`.

mod load;
mod rules;

pub use load::{LoadError, load_graph};
pub use rules::{CrateRule, RULES, RuleSet};

use std::collections::BTreeSet;

/// Kind of a dependency edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DepKind {
    /// `[dependencies]`.
    Normal,
    /// `[build-dependencies]`.
    Build,
    /// `[dev-dependencies]`.
    Dev,
}

/// One dependency edge between a workspace package and its dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepEdge {
    /// Workspace package name.
    pub from: String,
    /// Real package name (rename resolved).
    pub to: String,
    /// Edge kind.
    pub kind: DepKind,
    /// `cfg(...)` or triple for target-specific deps.
    pub target: Option<String>,
    /// `to` is a workspace member (ASM-006).
    pub is_workspace: bool,
}

/// Workspace members plus direct dependency edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceGraph {
    /// Workspace member names.
    pub members: BTreeSet<String>,
    /// All direct edges.
    pub edges: Vec<DepEdge>,
}

/// A rule violation.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Violation {
    /// Workspace crate missing from the rule table.
    UnknownCrate {
        /// Crate name.
        name: String,
    },
    /// Workspace dependency not in the allow-list.
    ForbiddenWorkspaceDep {
        /// Depending crate.
        from: String,
        /// Dependency.
        to: String,
        /// Edge kind.
        kind: DepKind,
        /// Requirement the rule serves.
        requirement: &'static str,
    },
    /// External dependency where none is allowed.
    ExternalDepNotAllowed {
        /// Depending crate.
        from: String,
        /// Dependency.
        to: String,
        /// Edge kind.
        kind: DepKind,
    },
}

/// Pure and deterministic: the result is sorted.
#[expect(clippy::todo, reason = "stub until WI-3")]
#[must_use]
pub fn check(graph: &WorkspaceGraph, rules: &RuleSet) -> Vec<Violation> {
    let _ = (graph, rules);
    todo!("WI-3")
}
