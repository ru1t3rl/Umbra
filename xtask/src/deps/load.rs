//! Loader around `cargo metadata --no-deps` (no network, no resolution; ASM-006).

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
    /// A crate root could not be read.
    #[error("cannot read crate root {path}: {source}")]
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

fn run_metadata(manifest_path: &Path) -> Result<Value, LoadError> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest_path)
        .output()
        .map_err(|e| LoadError::Metadata(format!("cannot run cargo: {e}")))?;
    if !out.status.success() {
        return Err(LoadError::Metadata(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| LoadError::Metadata(e.to_string()))
}

fn packages(meta: &Value) -> Result<&[Value], LoadError> {
    meta.get("packages")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| bad("packages"))
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, LoadError> {
    v.get(key).and_then(Value::as_str).ok_or_else(|| bad(key))
}

/// Runs `cargo metadata --no-deps --format-version 1 --manifest-path <p>`.
///
/// Renamed dependencies are reported under their real package name. Dev, build and
/// target-specific dependencies are included (ASM-005).
pub fn load_graph(manifest_path: &Path) -> Result<WorkspaceGraph, LoadError> {
    let meta = run_metadata(manifest_path)?;
    let pkgs = packages(&meta)?;
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
            });
        }
    }
    Ok(WorkspaceGraph { members, edges })
}

/// A crate root source file of a workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateRoot {
    /// Package name.
    pub package: String,
    /// File name of the root (`lib.rs`, `main.rs`).
    pub file: String,
    /// Full source text.
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

/// Loads the crate roots (library and binary targets) of all workspace members.
pub fn load_crate_roots(manifest_path: &Path) -> Result<Vec<CrateRoot>, LoadError> {
    let meta = run_metadata(manifest_path)?;
    let mut roots = Vec::new();
    for p in packages(&meta)? {
        let package = str_field(p, "name")?;
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
            let source = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
                path: path.display().to_string(),
                source,
            })?;
            let file = path
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            roots.push(CrateRoot {
                package: package.to_string(),
                file,
                source,
            });
        }
    }
    roots.sort_by(|a, b| (&a.package, &a.file).cmp(&(&b.package, &b.file)));
    Ok(roots)
}
