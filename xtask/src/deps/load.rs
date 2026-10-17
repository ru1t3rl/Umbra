//! Loader around `cargo metadata --no-deps` (no network, no resolution; ASM-006).
//!
//! The metadata is read once ([`Metadata::load`]) and every check is fed from it (review m5).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::{DepEdge, DepKind, WorkspaceGraph};

/// Failure to load the workspace graph.
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

fn read(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// A crate root source file of a workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateRoot {
    /// Package name.
    pub package: String,
    /// Path of the root relative to the package directory, with `/` separators
    /// (for example `src/lib.rs`, `src/bin/tool/main.rs`).
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

const ROOT_KINDS: &[&str] = &[
    "lib",
    "rlib",
    "dylib",
    "cdylib",
    "staticlib",
    "proc-macro",
    "bin",
];

/// Parsed output of one `cargo metadata --no-deps` run.
#[derive(Debug)]
pub struct Metadata(Value);

impl Metadata {
    /// Runs `cargo metadata --no-deps --format-version 1`. With `None`, cargo searches upward
    /// from the working directory for the manifest.
    pub fn load(manifest_path: Option<&Path>) -> Result<Self, LoadError> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut cmd = Command::new(cargo);
        cmd.args(["metadata", "--no-deps", "--format-version", "1"]);
        if let Some(p) = manifest_path {
            cmd.arg("--manifest-path").arg(p);
        }
        let out = cmd
            .output()
            .map_err(|e| LoadError::Metadata(format!("cannot run cargo: {e}")))?;
        if !out.status.success() {
            return Err(LoadError::Metadata(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        let v =
            serde_json::from_slice(&out.stdout).map_err(|e| LoadError::Metadata(e.to_string()))?;
        Ok(Self(v))
    }

    fn packages(&self) -> Result<&[Value], LoadError> {
        self.0
            .get("packages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| bad("packages"))
    }

    /// Members and direct dependency edges of every kind (ASM-005). Renamed dependencies are
    /// reported under their real package name.
    pub fn graph(&self) -> Result<WorkspaceGraph, LoadError> {
        let pkgs = self.packages()?;
        let mut members = BTreeSet::new();
        for p in pkgs {
            members.insert(str_field(p, "name")?.to_string());
        }
        let mut edges = Vec::new();
        for p in pkgs {
            let from = str_field(p, "name")?;
            let deps = p
                .get("dependencies")
                .and_then(Value::as_array)
                .ok_or_else(|| bad("dependencies"))?;
            for d in deps {
                let to = str_field(d, "name")?;
                let kind = match d.get("kind").and_then(Value::as_str) {
                    None => DepKind::Normal,
                    Some("build") => DepKind::Build,
                    Some("dev") => DepKind::Dev,
                    Some(_) => return Err(bad("dependency kind")),
                };
                edges.push(DepEdge {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                    target: d.get("target").and_then(Value::as_str).map(str::to_string),
                    is_workspace: members.contains(to),
                    is_path: d.get("path").is_some_and(|p| !p.is_null()),
                });
            }
        }
        Ok(WorkspaceGraph { members, edges })
    }

    /// Library and binary crate roots of all members, sorted by package and relative path.
    pub fn crate_roots(&self) -> Result<Vec<CrateRoot>, LoadError> {
        let mut roots = Vec::new();
        for p in self.packages()? {
            let package = str_field(p, "name")?;
            let dir = Path::new(str_field(p, "manifest_path")?).parent();
            let targets = p
                .get("targets")
                .and_then(Value::as_array)
                .ok_or_else(|| bad("targets"))?;
            for t in targets {
                let is_root = t
                    .get("kind")
                    .and_then(Value::as_array)
                    .ok_or_else(|| bad("kind"))?
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|k| ROOT_KINDS.contains(&k));
                if !is_root {
                    continue;
                }
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
        Ok(roots)
    }

    /// The `Cargo.toml` of every member, sorted by package.
    pub fn manifests(&self) -> Result<Vec<Manifest>, LoadError> {
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

/// Loads the dependency graph of the workspace at `manifest_path`.
pub fn load_graph(manifest_path: &Path) -> Result<WorkspaceGraph, LoadError> {
    Metadata::load(Some(manifest_path))?.graph()
}

/// Loads the crate roots of the workspace at `manifest_path`.
pub fn load_crate_roots(manifest_path: &Path) -> Result<Vec<CrateRoot>, LoadError> {
    Metadata::load(Some(manifest_path))?.crate_roots()
}

/// Loads the member manifests of the workspace at `manifest_path`.
pub fn load_manifests(manifest_path: &Path) -> Result<Vec<Manifest>, LoadError> {
    Metadata::load(Some(manifest_path))?.manifests()
}
