//! Dependency-rule check (REQ-A-1..3) and the `forbid(unsafe_code)` check (ASM-024).
//! See `docs/plans/M0.md`.

mod load;
mod rules;

pub use load::{CrateRoot, LoadError, load_crate_roots, load_graph};
pub use rules::{CrateRule, RULES, RuleSet, UNSAFE_ALLOWED};

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
    /// Crate root lacks `#![forbid(unsafe_code)]` (ASM-024, NFR-2).
    MissingForbidUnsafe {
        /// Package name.
        package: String,
        /// File name of the crate root, for example `lib.rs`.
        file: String,
    },
}

impl DepKind {
    /// Lower-case label used in output.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            DepKind::Normal => "normal",
            DepKind::Build => "build",
            DepKind::Dev => "dev",
        }
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::UnknownCrate { name } => {
                write!(f, "unknown crate: {name} is not in the rule table")
            }
            Violation::ForbiddenWorkspaceDep {
                from,
                to,
                kind,
                requirement,
            } => {
                let allowed = RULES
                    .get(from)
                    .map(|r| r.allowed_workspace_deps.join(", "))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(none)".to_string());
                write!(
                    f,
                    "{requirement}: {from} -> {to} ({}) not allowed; {from} may depend on: {allowed}",
                    kind.label()
                )
            }
            Violation::ExternalDepNotAllowed { from, to, kind } => write!(
                f,
                "section 4: {from} -> {to} ({}) not allowed; {from} may have no external dependencies",
                kind.label()
            ),
            Violation::MissingForbidUnsafe { package, file } => {
                write!(f, "NFR-2: {package} ({file}) lacks #![forbid(unsafe_code)]")
            }
        }
    }
}

/// Pure and deterministic: the result is sorted and free of duplicates.
#[must_use]
pub fn check(graph: &WorkspaceGraph, rules: &RuleSet) -> Vec<Violation> {
    let mut out = BTreeSet::new();
    let names = graph
        .members
        .iter()
        .chain(graph.edges.iter().map(|e| &e.from));
    for name in names {
        if rules.get(name).is_none() {
            out.insert(Violation::UnknownCrate { name: name.clone() });
        }
    }
    for e in &graph.edges {
        let Some(rule) = rules.get(&e.from) else {
            continue;
        };
        if e.is_workspace {
            if !rule.allowed_workspace_deps.contains(&e.to.as_str()) {
                out.insert(Violation::ForbiddenWorkspaceDep {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    kind: e.kind,
                    requirement: rule.requirement,
                });
            }
        } else if !rule.allow_external_deps && e.kind != DepKind::Dev {
            out.insert(Violation::ExternalDepNotAllowed {
                from: e.from.clone(),
                to: e.to.clone(),
                kind: e.kind,
            });
        }
    }
    out.into_iter().collect()
}

/// ASM-024: every root of a non-FFI crate must carry `#![forbid(unsafe_code)]`.
///
/// A source scan, not a parser: a single-line inner attribute whose list contains
/// `unsafe_code`; commented-out lines do not count. The result is sorted.
#[must_use]
pub fn check_forbid_unsafe(roots: &[CrateRoot]) -> Vec<Violation> {
    let out: BTreeSet<Violation> = roots
        .iter()
        .filter(|r| !UNSAFE_ALLOWED.contains(&r.package.as_str()))
        .filter(|r| !has_forbid_unsafe(&r.source))
        .map(|r| Violation::MissingForbidUnsafe {
            package: r.package.clone(),
            file: r.file.clone(),
        })
        .collect();
    out.into_iter().collect()
}

fn has_forbid_unsafe(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim();
        line.strip_prefix("#![forbid(")
            .and_then(|rest| rest.split_once(")]"))
            .is_some_and(|(list, _)| list.split(',').any(|i| i.trim() == "unsafe_code"))
    })
}
