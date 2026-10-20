//! Dependency-rule check (REQ-A-1..3) and the `forbid(unsafe_code)` and `[lints]` checks
//! (NFR-2, ASM-024). See `docs/plans/M0.md`.
//!
//! Two passes over the dependency graph (ASM-005, ASM-006):
//! - [`check`]: declared direct edges from `cargo metadata --no-deps`, for precise diagnostics.
//! - [`check_resolved`]: the resolved graph, by package id, so `[patch]`, git, registry and
//!   same-name packages cannot launder a forbidden edge.

mod load;
mod rules;

pub use load::{CrateRoot, LoadError, Manifest, ResolvedDep, ResolvedGraph, ResolvedPackage};
pub use rules::{
    CrateRule, PROTOCOL_NEUTRAL, RULES, RuleSet, UNSAFE_ALLOWED, is_protocol_crate, protocol_scope,
};

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    /// `to` is a path dependency on the workspace member's own directory (ASM-006).
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
    /// Path dependency on a package outside the workspace (ASM-006; there is no allow-list).
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
        /// Root file relative to the package, for example `src/lib.rs` or `tests/it.rs`.
        file: String,
    },
    /// The resolved graph reaches a member outside the allowed closure through a non-member
    /// package (ASM-005), for example `umbra-sdk -> shim -> umbra-kernel`.
    IndirectWorkspaceDep {
        /// Depending member.
        from: String,
        /// Non-member packages on the path, in order.
        via: Vec<String>,
        /// Member reached.
        to: String,
        /// Requirement the rule serves.
        requirement: &'static str,
    },
    /// A resolved non-member package carries the name of a workspace or rule-table crate
    /// (ASM-006).
    ImpostorPackage {
        /// Package name.
        name: String,
        /// Package version.
        version: String,
        /// Cargo package id (shows the source).
        id: String,
    },
    /// A declared dependency uses the name of a workspace or rule-table crate but is not a path
    /// dependency on that member, for example a git or registry `umbra-kernel` (ASM-006).
    ImpostorDep {
        /// Depending crate.
        from: String,
        /// Dependency name.
        to: String,
        /// Edge kind.
        kind: DepKind,
    },
    /// A resolved non-member package comes from a local path (path dependency, `[patch]` or
    /// `[replace]`), so its code escapes the forbid and lints checks (ASM-006, NFR-2).
    NonMemberPathPackage {
        /// Package name.
        name: String,
        /// Package version.
        version: String,
        /// Cargo package id.
        id: String,
    },
    /// A protocol-neutral crate depends on a protocol crate by name pattern (ADR 0030, ASM-226).
    ProtocolDep {
        /// Depending crate.
        from: String,
        /// Dependency (real package name).
        to: String,
        /// Edge kind.
        kind: DepKind,
    },
    /// The resolved graph reaches a protocol crate from a protocol-neutral crate through
    /// non-member packages (ADR 0030, ASM-005), for example `umbra-engine -> shim -> dis-rs`.
    IndirectProtocolDep {
        /// Depending member.
        from: String,
        /// Non-member packages on the path, in order.
        via: Vec<String>,
        /// Protocol package reached.
        to: String,
    },
    /// A crate root or manifest could not be parsed, so it cannot be verified (ASM-024).
    UnparsableSource {
        /// Package name.
        package: String,
        /// File relative to the package, for example `src/lib.rs` or `Cargo.toml`.
        file: String,
        /// First line of the parser error.
        error: String,
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
                "ASM-006: {from} -> {to} ({}) is a path dependency outside the workspace; make it a member or a registry dependency",
                kind.label()
            ),
            Violation::MissingWorkspaceLints { package } => {
                format!("NFR-2: {package} Cargo.toml lacks [lints] workspace = true")
            }
            Violation::MissingForbidUnsafe { package, file } => {
                format!("NFR-2: {package} ({file}) lacks #![forbid(unsafe_code)]")
            }
            Violation::IndirectWorkspaceDep {
                from,
                via,
                to,
                requirement,
            } => {
                let reach: Vec<&str> = rules.closure(from).into_iter().collect();
                let reach = if reach.is_empty() {
                    "(none)".to_string()
                } else {
                    reach.join(", ")
                };
                let path: Vec<&str> = std::iter::once(from.as_str())
                    .chain(via.iter().map(String::as_str))
                    .chain(std::iter::once(to.as_str()))
                    .collect();
                format!(
                    "{requirement}: {} (resolved) not allowed; {from} may reach: {reach}",
                    path.join(" -> ")
                )
            }
            Violation::ImpostorPackage { name, version, id } => format!(
                "ASM-006: package {name} {version} ({id}) has a workspace crate name but is not that workspace member"
            ),
            Violation::ImpostorDep { from, to, kind } => format!(
                "ASM-006: {from} -> {to} ({}) uses the name of a workspace crate but is not a path dependency on that member",
                kind.label()
            ),
            Violation::NonMemberPathPackage { name, version, id } => format!(
                "ASM-006: package {name} {version} ({id}) is a path package outside the workspace (path dependency, [patch] or [replace]); its code is not checked"
            ),
            Violation::ProtocolDep { from, to, kind } => format!(
                "ADR 0030: {from} -> {to} ({}) not allowed; {from} is protocol-neutral and must not depend on protocol crates (name pattern)",
                kind.label()
            ),
            Violation::IndirectProtocolDep { from, via, to } => {
                let path: Vec<&str> = std::iter::once(from.as_str())
                    .chain(via.iter().map(String::as_str))
                    .chain(std::iter::once(to.as_str()))
                    .collect();
                format!(
                    "ADR 0030: {} (resolved) not allowed; {from} is protocol-neutral and must not reach protocol crates (name pattern)",
                    path.join(" -> ")
                )
            }
            Violation::UnparsableSource {
                package,
                file,
                error,
            } => format!(
                "NFR-2: {package} ({file}) cannot be parsed, so it cannot be verified: {error}"
            ),
        }
    }
}

/// Pure and deterministic: the result is sorted and free of duplicates.
#[must_use]
pub fn check(graph: &WorkspaceGraph, rules: &RuleSet) -> Vec<Violation> {
    let mut out = BTreeSet::new();
    let scope = protocol_scope(rules);
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
        // Workspace edges to adapter crates are rule-table violations below, not reported twice.
        if !e.is_workspace && scope.contains(e.from.as_str()) && is_protocol_crate(&e.to) {
            out.insert(Violation::ProtocolDep {
                from: e.from.clone(),
                to: e.to.clone(),
                kind: e.kind,
            });
        }
        if e.is_workspace {
            if !rule.allowed_workspace_deps.contains(&e.to.as_str()) {
                out.insert(Violation::ForbiddenWorkspaceDep {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    kind: e.kind,
                    requirement: rule.requirement,
                });
            }
        } else if e.is_path {
            out.insert(Violation::NonMemberPathDep {
                from: e.from.clone(),
                to: e.to.clone(),
                kind: e.kind,
            });
        } else if graph.members.contains(&e.to) || rules.get(&e.to).is_some() {
            out.insert(Violation::ImpostorDep {
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

/// Resolved-graph check (ASM-005, ASM-006); the result is sorted and free of duplicates.
///
/// For each member with a rule:
/// - a direct dependency on a member (by package id) must be in its allow-list; this catches a
///   registry dependency that `[patch]` points at a member;
/// - a member reached through non-member packages must be in the transitive closure of its
///   allow-list. Members are not expanded: their own edges are checked from their side.
///
/// Every non-member package that has the name of a member or a rule-table crate is reported, and
/// so is every non-member package without a source (a local path package, however it got into the
/// graph: path dependency, `[patch]` or `[replace]`). There is no allow-list (ASM-006).
#[must_use]
pub fn check_resolved(graph: &ResolvedGraph, rules: &RuleSet) -> Vec<Violation> {
    let mut out = BTreeSet::new();
    let scope = protocol_scope(rules);
    let name = |id| graph.name(id);
    let member_names: BTreeSet<&str> = graph.members.iter().map(|id| name(id)).collect();
    for (id, p) in &graph.packages {
        if !graph.members.contains(id) && p.source.is_none() {
            out.insert(Violation::NonMemberPathPackage {
                name: p.name.clone(),
                version: p.version.clone(),
                id: id.clone(),
            });
        }
        let taken = member_names.contains(p.name.as_str()) || rules.get(&p.name).is_some();
        if !graph.members.contains(id) && taken {
            out.insert(Violation::ImpostorPackage {
                name: p.name.clone(),
                version: p.version.clone(),
                id: id.clone(),
            });
        }
    }
    let deps = |id: &str| graph.deps.get(id).map_or(&[][..], Vec::as_slice);
    for m in &graph.members {
        let from = name(m);
        let Some(rule) = rules.get(from) else {
            continue; // UnknownCrate comes from `check`.
        };
        let allowed = rules.closure(from);
        // Breadth-first over non-members; `prev` gives the shortest path back to `m`.
        let mut prev: BTreeMap<&str, &str> = BTreeMap::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        for d in deps(m) {
            if graph.members.contains(&d.id) {
                if !rule.allowed_workspace_deps.contains(&name(&d.id)) {
                    for kind in &d.kinds {
                        out.insert(Violation::ForbiddenWorkspaceDep {
                            from: from.to_string(),
                            to: name(&d.id).to_string(),
                            kind: *kind,
                            requirement: rule.requirement,
                        });
                    }
                }
            } else if !prev.contains_key(d.id.as_str()) {
                prev.insert(&d.id, m);
                queue.push_back(&d.id);
            }
        }
        while let Some(n) = queue.pop_front() {
            for d in deps(n) {
                if d.id == *m || prev.contains_key(d.id.as_str()) {
                    continue;
                }
                prev.insert(&d.id, n);
                if !graph.members.contains(&d.id) {
                    queue.push_back(&d.id);
                    if scope.contains(from) && is_protocol_crate(name(&d.id)) {
                        let via = path_back(&prev, graph, n, m);
                        out.insert(Violation::IndirectProtocolDep {
                            from: from.to_string(),
                            via,
                            to: name(&d.id).to_string(),
                        });
                    }
                } else if !allowed.contains(name(&d.id)) {
                    let via = path_back(&prev, graph, n, m);
                    out.insert(Violation::IndirectWorkspaceDep {
                        from: from.to_string(),
                        via,
                        to: name(&d.id).to_string(),
                        requirement: rule.requirement,
                    });
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Names of the non-member packages from `n` back to (excluding) member `m`, in order from `m`.
fn path_back<'a>(
    prev: &BTreeMap<&'a str, &'a str>,
    graph: &'a ResolvedGraph,
    n: &'a str,
    m: &'a str,
) -> Vec<String> {
    let mut via = Vec::new();
    let mut cur = n;
    while cur != m {
        via.push(graph.name(cur).to_string());
        cur = prev.get(cur).copied().unwrap_or(m);
    }
    via.reverse();
    via
}

fn first_line(error: &str) -> String {
    error.lines().next().unwrap_or("").trim().to_string()
}

/// ASM-024: every target root (lib, bin, test, example, bench, build script) of a non-FFI crate
/// must carry the inner attribute `#![forbid(unsafe_code)]` at file level.
///
/// The root is parsed with `syn`; a root that does not parse is a violation
/// ([`Violation::UnparsableSource`]). `cfg_attr(..., forbid(unsafe_code))` is conditional and
/// does not count. The result is sorted.
#[must_use]
pub fn check_forbid_unsafe(roots: &[CrateRoot]) -> Vec<Violation> {
    let out: BTreeSet<Violation> = roots
        .iter()
        .filter(|r| !UNSAFE_ALLOWED.contains(&r.package.as_str()))
        .filter_map(|r| match syn::parse_file(&r.source) {
            Ok(file) if file.attrs.iter().any(is_forbid_unsafe) => None,
            Ok(_) => Some(Violation::MissingForbidUnsafe {
                package: r.package.clone(),
                file: r.file.clone(),
            }),
            Err(e) => Some(Violation::UnparsableSource {
                package: r.package.clone(),
                file: r.file.clone(),
                error: first_line(&e.to_string()),
            }),
        })
        .collect();
    out.into_iter().collect()
}

/// True for the inner attribute `#![forbid(.., unsafe_code, ..)]` (a `reason = ".."` is allowed).
fn is_forbid_unsafe(attr: &syn::Attribute) -> bool {
    if !matches!(attr.style, syn::AttrStyle::Inner(_)) || !attr.path().is_ident("forbid") {
        return false;
    }
    let mut found = false;
    let parsed = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("unsafe_code") {
            found = true;
        }
        if meta.input.peek(syn::Token![=]) {
            let _: syn::Lit = meta.value()?.parse()?;
        }
        Ok(())
    });
    parsed.is_ok() && found
}

/// A member's `Cargo.toml` must set `lints.workspace = true` (ASM-024), otherwise the workspace
/// lint table (`unsafe_code = "deny"`, clippy) silently does not apply. The manifest is parsed
/// as TOML, so every spelling (`[lints]`, dotted key, inline table) counts and text inside
/// strings does not. A manifest that does not parse is a violation. The result is sorted.
#[must_use]
pub fn check_workspace_lints(manifests: &[Manifest]) -> Vec<Violation> {
    let out: BTreeSet<Violation> = manifests
        .iter()
        .filter_map(|m| match m.source.parse::<toml::Table>() {
            Ok(t) => {
                let inherits = t
                    .get("lints")
                    .and_then(|l| l.get("workspace"))
                    .and_then(toml::Value::as_bool)
                    == Some(true);
                (!inherits).then(|| Violation::MissingWorkspaceLints {
                    package: m.package.clone(),
                })
            }
            Err(e) => Some(Violation::UnparsableSource {
                package: m.package.clone(),
                file: "Cargo.toml".to_string(),
                error: first_line(&e.to_string()),
            }),
        })
        .collect();
    out.into_iter().collect()
}

/// Everything `check-deps` loads: one `cargo metadata --no-deps` run and one resolved run.
#[derive(Debug)]
pub struct Workspace {
    /// Members and declared direct dependency edges.
    pub graph: WorkspaceGraph,
    /// Resolved graph by package id; `None` until loaded (see [`Workspace::load_declared`]).
    pub resolved: Option<ResolvedGraph>,
    /// Crate roots of all members.
    pub roots: Vec<CrateRoot>,
    /// Member manifests.
    pub manifests: Vec<Manifest>,
}

impl Workspace {
    /// Loads both passes (with `None`, cargo searches upward from the working directory).
    /// Fails, rather than skipping the resolved pass, when the resolved graph cannot be loaded.
    pub fn load(manifest_path: Option<&std::path::Path>) -> Result<Self, LoadError> {
        let mut ws = Self::load_declared(manifest_path)?;
        ws.resolved = Some(ResolvedGraph::load(manifest_path)?);
        Ok(ws)
    }

    /// Loads only the declared pass (`cargo metadata --no-deps`); `resolved` is `None`. The
    /// binary uses this to print declared diagnostics even when resolution fails.
    pub fn load_declared(manifest_path: Option<&std::path::Path>) -> Result<Self, LoadError> {
        let meta = load::Metadata::load(manifest_path)?;
        Ok(Self {
            graph: meta.graph()?,
            resolved: None,
            roots: meta.crate_roots()?,
            manifests: meta.manifests()?,
        })
    }
}

/// All `check-deps` checks on what is loaded; the result is sorted and free of duplicates (both
/// passes report a forbidden direct member edge identically). Without a resolved graph only the
/// declared checks run, which is not a complete check.
#[must_use]
pub fn check_workspace(ws: &Workspace, rules: &RuleSet) -> Vec<Violation> {
    let mut violations = check(&ws.graph, rules);
    if let Some(resolved) = &ws.resolved {
        violations.extend(check_resolved(resolved, rules));
    }
    violations.extend(check_forbid_unsafe(&ws.roots));
    violations.extend(check_workspace_lints(&ws.manifests));
    violations.sort();
    violations.dedup();
    violations
}
