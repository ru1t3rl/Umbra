//! Dependency-rule check (REQ-A-1..3) and the `forbid(unsafe_code)` check (ASM-024).
//! See `docs/plans/M0.md`.

mod load;
mod rules;

pub use load::{
    CrateRoot, LoadError, Manifest, Metadata, load_crate_roots, load_graph, load_manifests,
};
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
    /// Declared with a `path` (so not a registry or git dependency).
    pub is_path: bool,
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
    /// Path dependency outside the workspace that is not allow-listed (ASM-006).
    NonMemberPathDep {
        /// Depending crate.
        from: String,
        /// Dependency.
        to: String,
        /// Edge kind.
        kind: DepKind,
    },
    /// Member manifest lacks `[lints] workspace = true` (ASM-024).
    MissingWorkspaceLints {
        /// Package name.
        package: String,
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

impl Violation {
    /// One-line message; the allowed lists come from the injected `rules`.
    #[must_use]
    pub fn render(&self, rules: &RuleSet) -> String {
        match self {
            Violation::UnknownCrate { name } => {
                format!("unknown crate: {name} is not in the rule table")
            }
            Violation::ForbiddenWorkspaceDep {
                from,
                to,
                kind,
                requirement,
            } => {
                let allowed = rules
                    .get(from)
                    .map(|r| r.allowed_workspace_deps.join(", "))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "(none)".to_string());
                format!(
                    "{requirement}: {from} -> {to} ({}) not allowed; {from} may depend on: {allowed}",
                    kind.label()
                )
            }
            Violation::ExternalDepNotAllowed { from, to, kind } => {
                let requirement = rules.get(from).map_or("section 4", |r| r.requirement);
                format!(
                    "{requirement}: {from} -> {to} ({}) not allowed; {from} may have no normal/build external dependencies (ASM-009)",
                    kind.label()
                )
            }
            Violation::NonMemberPathDep { from, to, kind } => format!(
                "ASM-006: {from} -> {to} ({}) is a path dependency outside the workspace and is not allow-listed",
                kind.label()
            ),
            Violation::MissingWorkspaceLints { package } => {
                format!("NFR-2: {package} Cargo.toml lacks [lints] workspace = true")
            }
            Violation::MissingForbidUnsafe { package, file } => {
                format!("NFR-2: {package} ({file}) lacks #![forbid(unsafe_code)]")
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
        } else if e.is_path && !rules.allowed_path_deps.contains(&e.to.as_str()) {
            out.insert(Violation::NonMemberPathDep {
                from: e.from.clone(),
                to: e.to.clone(),
                kind: e.kind,
            });
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
/// Only the crate-root preamble is scanned: comments and inner attributes from the top of the
/// file, up to the first item. The attribute must start at column 0 and may span lines.
/// `cfg_attr(..., forbid(unsafe_code))` is conditional and does not count. The result is sorted.
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

/// Scans the preamble for a column-0 inner attribute `forbid(.. unsafe_code ..)`.
fn has_forbid_unsafe(source: &str) -> bool {
    let b = source.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < b.len() {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'#' => {
                let at_line_start = i == 0 || b[i - 1] == b'\n';
                let mut j = i + 1;
                if b.get(j) != Some(&b'!') {
                    return false;
                }
                j += 1;
                while b.get(j).is_some_and(u8::is_ascii_whitespace) {
                    j += 1;
                }
                if b.get(j) != Some(&b'[') {
                    return false;
                }
                let Some(end) = attribute_end(b, j) else {
                    return false;
                };
                if at_line_start && is_forbid_unsafe(&source[j + 1..end]) {
                    return true;
                }
                i = end + 1;
            }
            _ => return false,
        }
    }
    false
}

/// Index of the `]` matching the `[` at `open`, skipping string literals.
fn attribute_end(b: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `body` is the text between `[` and `]`: true for `forbid(a, unsafe_code, b)`.
fn is_forbid_unsafe(body: &str) -> bool {
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    compact
        .strip_prefix("forbid(")
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(|list| list.split(',').any(|i| i == "unsafe_code"))
}

/// A member's `Cargo.toml` must contain `[lints]` with `workspace = true` (ASM-024), otherwise
/// the workspace lint table (`unsafe_code = "deny"`, clippy) silently does not apply.
#[must_use]
pub fn check_workspace_lints(manifests: &[Manifest]) -> Vec<Violation> {
    let out: BTreeSet<Violation> = manifests
        .iter()
        .filter(|m| !inherits_workspace_lints(&m.source))
        .map(|m| Violation::MissingWorkspaceLints {
            package: m.package.clone(),
        })
        .collect();
    out.into_iter().collect()
}

fn inherits_workspace_lints(source: &str) -> bool {
    let mut in_lints = false;
    for line in source.lines() {
        let code = line.split('#').next().unwrap_or("");
        let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.starts_with('[') {
            in_lints = compact == "[lints]";
        } else if in_lints && compact == "workspace=true" {
            return true;
        }
    }
    false
}

/// Everything `check-deps` loads from one `cargo metadata` run.
#[derive(Debug)]
pub struct Workspace {
    /// Members and dependency edges.
    pub graph: WorkspaceGraph,
    /// Crate roots of all members.
    pub roots: Vec<CrateRoot>,
    /// Member manifests.
    pub manifests: Vec<Manifest>,
}

impl Workspace {
    /// Loads everything with a single `cargo metadata` run (review m5).
    pub fn load(manifest_path: Option<&std::path::Path>) -> Result<Self, LoadError> {
        let meta = Metadata::load(manifest_path)?;
        Ok(Self {
            graph: meta.graph()?,
            roots: meta.crate_roots()?,
            manifests: meta.manifests()?,
        })
    }
}

/// All `check-deps` checks; the result is sorted.
#[must_use]
pub fn check_workspace(ws: &Workspace, rules: &RuleSet) -> Vec<Violation> {
    let mut violations = check(&ws.graph, rules);
    violations.extend(check_forbid_unsafe(&ws.roots));
    violations.extend(check_workspace_lints(&ws.manifests));
    violations.sort();
    violations
}
