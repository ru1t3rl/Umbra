//! Loaders around `cargo metadata` (ASM-005, ASM-006).
//!
//! Two runs, both offline:
//! - `--no-deps`: members, their declared (direct) dependencies, targets and manifests.
//! - `--offline --locked --all-features`: the resolved graph over all platforms (no
//!   `--filter-platform`), which sees through `[patch]`, `[replace]`, git and registry
//!   dependencies to the packages that are actually linked.
//!
//! With a manifest path, cargo runs in that manifest's directory, so `.cargo/config.toml` files
//! (which can hold `[patch]`) are read as they would be for a build there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::{DepEdge, DepKind, WorkspaceGraph};

/// Failure to load the workspace.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// `cargo metadata` failed or returned bad JSON.
    #[error("cargo metadata failed: {0}")]
    Metadata(String),
    /// A crate root or manifest could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// Path of the file.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
}

fn bad(what: &str) -> LoadError {
    LoadError::Metadata(format!("unexpected metadata shape: {what}"))
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, LoadError> {
    v.get(key).and_then(Value::as_str).ok_or_else(|| bad(key))
}

fn array_field<'a>(v: &'a Value, key: &str) -> Result<&'a [Value], LoadError> {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| bad(key))
}

fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn dep_kind(v: Option<&str>) -> Result<DepKind, LoadError> {
    match v {
        None => Ok(DepKind::Normal),
        Some("build") => Ok(DepKind::Build),
        Some("dev") => Ok(DepKind::Dev),
        Some(_) => Err(bad("dependency kind")),
    }
}

/// A root source file of one target (lib, bin, test, example, bench or build script) of a member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateRoot {
    /// Package name.
    pub package: String,
    /// Path of the root relative to the package directory, with `/` separators
    /// (for example `src/lib.rs`, `src/bin/tool/main.rs`, `build.rs`).
    pub file: String,
    /// Full source text.
    pub source: String,
}

/// A workspace member's `Cargo.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Package name.
    pub package: String,
    /// Full manifest text.
    pub source: String,
}

/// A package in the resolved graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
}

/// One resolved dependency edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDep {
    /// Package id of the dependency.
    pub id: String,
    /// Kinds under which it is a dependency (any platform).
    pub kinds: BTreeSet<DepKind>,
}

/// The resolved dependency graph (all features, all platforms), keyed by package id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedGraph {
    /// Package ids of the workspace members.
    pub members: BTreeSet<String>,
    /// Every resolved package.
    pub packages: BTreeMap<String, ResolvedPackage>,
    /// Dependencies of each package, sorted by id.
    pub deps: BTreeMap<String, Vec<ResolvedDep>>,
}

/// Runs `cargo metadata --format-version 1` plus `args`.
fn cargo_metadata(manifest_path: Option<&Path>, args: &[&str]) -> Result<Value, LoadError> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args(["metadata", "--format-version", "1"]).args(args);
    if let Some(p) = manifest_path {
        let abs = std::path::absolute(p)
            .map_err(|e| LoadError::Metadata(format!("bad manifest path {}: {e}", p.display())))?;
        let dir: PathBuf = abs.parent().map(Path::to_path_buf).unwrap_or_default();
        if !dir.is_dir() {
            return Err(LoadError::Metadata(format!(
                "manifest directory {} does not exist",
                dir.display()
            )));
        }
        cmd.arg("--manifest-path").arg(&abs).current_dir(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| LoadError::Metadata(format!("cannot run cargo: {e}")))?;
    if !out.status.success() {
        return Err(LoadError::Metadata(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| LoadError::Metadata(e.to_string()))
}

/// Parsed output of one `cargo metadata --no-deps` run.
#[derive(Debug)]
pub(super) struct Metadata(Value);

impl Metadata {
    /// Runs `cargo metadata --no-deps`. With `None`, cargo searches upward from the working
    /// directory for the manifest.
    pub(super) fn load(manifest_path: Option<&Path>) -> Result<Self, LoadError> {
        cargo_metadata(manifest_path, &["--no-deps"]).map(Self)
    }

    fn packages(&self) -> Result<&[Value], LoadError> {
        array_field(&self.0, "packages")
    }

    /// Members and direct dependency edges of every kind (ASM-005). Renamed dependencies are
    /// reported under their real package name. A dependency is a workspace edge only when it is a
    /// path dependency on the member's own directory (ASM-006), not merely a member's name.
    pub(super) fn graph(&self) -> Result<WorkspaceGraph, LoadError> {
        let pkgs = self.packages()?;
        let mut member_dirs: BTreeMap<&str, PathBuf> = BTreeMap::new();
        for p in pkgs {
            let dir = Path::new(str_field(p, "manifest_path")?)
                .parent()
                .ok_or_else(|| bad("manifest_path"))?;
            member_dirs.insert(str_field(p, "name")?, dir.to_path_buf());
        }
        let members = member_dirs.keys().map(|n| (*n).to_string()).collect();
        let mut edges = Vec::new();
        for p in pkgs {
            let from = str_field(p, "name")?;
            for d in array_field(p, "dependencies")? {
                let to = str_field(d, "name")?;
                let path = d.get("path").and_then(Value::as_str).map(Path::new);
                edges.push(DepEdge {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind: dep_kind(d.get("kind").and_then(Value::as_str))?,
                    target: d.get("target").and_then(Value::as_str).map(str::to_string),
                    is_workspace: path.is_some_and(|p| member_dirs.get(to).is_some_and(|m| m == p)),
                    is_path: path.is_some(),
                });
            }
        }
        Ok(WorkspaceGraph { members, edges })
    }

    /// Root files of every target of every member (ASM-024), sorted by package and relative path.
    pub(super) fn crate_roots(&self) -> Result<Vec<CrateRoot>, LoadError> {
        let mut roots = Vec::new();
        for p in self.packages()? {
            let package = str_field(p, "name")?;
            let dir = Path::new(str_field(p, "manifest_path")?).parent();
            for t in array_field(p, "targets")? {
                let path = Path::new(str_field(t, "src_path")?);
                let rel = dir.and_then(|d| path.strip_prefix(d).ok()).unwrap_or(path);
                let file = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                roots.push(CrateRoot {
                    package: package.to_string(),
                    file,
                    source: read(path)?,
                });
            }
        }
        roots.sort_by(|a, b| (&a.package, &a.file).cmp(&(&b.package, &b.file)));
        roots.dedup_by(|a, b| a.package == b.package && a.file == b.file);
        Ok(roots)
    }

    /// The `Cargo.toml` of every member, sorted by package.
    pub(super) fn manifests(&self) -> Result<Vec<Manifest>, LoadError> {
        let mut out = Vec::new();
        for p in self.packages()? {
            out.push(Manifest {
                package: str_field(p, "name")?.to_string(),
                source: read(Path::new(str_field(p, "manifest_path")?))?,
            });
        }
        out.sort_by(|a, b| a.package.cmp(&b.package));
        Ok(out)
    }
}

impl ResolvedGraph {
    /// Package name for `id` (the id itself if unknown).
    #[must_use]
    pub fn name<'a>(&'a self, id: &'a str) -> &'a str {
        self.packages.get(id).map_or(id, |p| p.name.as_str())
    }

    /// Runs `cargo metadata --offline --locked --all-features` and reads `resolve`. Fails when
    /// `Cargo.lock` is missing or stale, or a dependency is not in the local cache (`cargo fetch`).
    pub fn load(manifest_path: Option<&Path>) -> Result<Self, LoadError> {
        let v = cargo_metadata(manifest_path, &["--offline", "--locked", "--all-features"])?;
        Self::from_metadata(&v)
    }

    fn from_metadata(v: &Value) -> Result<Self, LoadError> {
        let mut g = Self::default();
        for m in array_field(v, "workspace_members")? {
            g.members.insert(
                m.as_str()
                    .ok_or_else(|| bad("workspace_members"))?
                    .to_string(),
            );
        }
        for p in array_field(v, "packages")? {
            g.packages.insert(
                str_field(p, "id")?.to_string(),
                ResolvedPackage {
                    name: str_field(p, "name")?.to_string(),
                    version: str_field(p, "version")?.to_string(),
                },
            );
        }
        let resolve = v.get("resolve").ok_or_else(|| bad("resolve"))?;
        for node in array_field(resolve, "nodes")? {
            let mut deps = Vec::new();
            for d in array_field(node, "deps")? {
                let mut kinds = BTreeSet::new();
                for k in array_field(d, "dep_kinds")? {
                    kinds.insert(dep_kind(k.get("kind").and_then(Value::as_str))?);
                }
                deps.push(ResolvedDep {
                    id: str_field(d, "pkg")?.to_string(),
                    kinds,
                });
            }
            deps.sort_by(|a, b| a.id.cmp(&b.id));
            g.deps.insert(str_field(node, "id")?.to_string(), deps);
        }
        Ok(g)
    }
}
