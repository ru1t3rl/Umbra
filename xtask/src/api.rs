//! `cargo xtask gen-api [--check] [--api-dir <dir>]`: the committed contract documents
//! `api/openapi.json`, `api/stream.schema.json` and `api/asyncapi.json` (REQ-X-4, REQ-X-6, gate 10,
//! ADR 0025).
//!
//! The documents come from the `gen_api` example of `umbra-server`, run as a process (`cargo run
//! --locked --offline -q -p umbra-server --example gen_api`), so xtask keeps no workspace dependency
//! (ASM-002). The example prints a JSON object of file name to rendered text. Line endings are
//! normalised to LF on both sides, so a CRLF checkout is not drift.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::header::{first_difference, normalise_newlines};

/// The directory of the committed files, relative to the workspace root.
pub const API_DIR: &str = "api";

/// Errors of `gen-api` (all exit 2).
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The nested `cargo` could not be started.
    #[error("cannot run cargo: {0}")]
    Spawn(#[source] std::io::Error),
    /// The `gen_api` example failed to build or run.
    #[error("the gen_api example failed ({status}):\n{stderr}")]
    Example {
        /// Its exit status.
        status: String,
        /// Its standard error.
        stderr: String,
    },
    /// The example printed something other than an object of strings.
    #[error("the gen_api example printed an unexpected output: {0}")]
    Output(String),
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

/// How a file of the `api` directory is not current.
#[derive(Debug, PartialEq, Eq)]
pub enum Drift {
    /// The file is not there.
    Missing,
    /// The file differs from the generated document from this 1-based line.
    Differs(usize),
    /// The file is a `.json` file that no generated document has.
    Unexpected,
}

/// What `gen-api` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// With `--check`: every file is current.
    Current,
    /// With `--check`: these files are not current, sorted by name.
    Drift(Vec<(String, Drift)>),
    /// Without `--check`: these files were written.
    Written(Vec<PathBuf>),
}

/// The generated `(file name, LF text)` pairs, sorted by name.
///
/// # Errors
/// The example failed or printed something unexpected.
pub fn generate(workspace_root: &Path) -> Result<Vec<(String, String)>, ApiError> {
    let out = Command::new(crate::cargo_program())
        .args(["run", "--locked", "--offline", "-q", "-p", "umbra-server"])
        .args(["--example", "gen_api"])
        .current_dir(workspace_root)
        .output()
        .map_err(ApiError::Spawn)?;
    if !out.status.success() {
        return Err(ApiError::Example {
            status: out.status.to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| ApiError::Output(e.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ApiError::Output("not an object".to_string()))?;
    let mut files = Vec::new();
    for (name, text) in object {
        let text = text
            .as_str()
            .ok_or_else(|| ApiError::Output(format!("{name} is not a string")))?;
        files.push((name.clone(), normalise_newlines(text)));
    }
    if files.is_empty() {
        return Err(ApiError::Output("no documents".to_string()));
    }
    Ok(files)
}

/// Writes the documents to `api_dir`, or with `check` compares them with the files there.
///
/// # Errors
/// See [`ApiError`].
pub fn run(workspace_root: &Path, api_dir: &Path, check: bool) -> Result<Outcome, ApiError> {
    let files = generate(workspace_root)?;
    if check {
        let mut drift = Vec::new();
        for (name, text) in &files {
            let path = api_dir.join(name);
            match std::fs::read_to_string(&path) {
                Ok(committed) => {
                    if let Some(line) = first_difference(text, &committed) {
                        drift.push((name.clone(), Drift::Differs(line)));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    drift.push((name.clone(), Drift::Missing));
                }
                Err(source) => return Err(ApiError::Io { path, source }),
            }
        }
        match std::fs::read_dir(api_dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|source| ApiError::Io {
                        path: api_dir.to_path_buf(),
                        source,
                    })?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let known = files.iter().any(|(n, _)| *n == name);
                    if !known && Path::new(&name).extension().is_some_and(|e| e == "json") {
                        drift.push((name, Drift::Unexpected));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ApiError::Io {
                    path: api_dir.to_path_buf(),
                    source,
                });
            }
        }
        drift.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(if drift.is_empty() {
            Outcome::Current
        } else {
            Outcome::Drift(drift)
        });
    }
    std::fs::create_dir_all(api_dir).map_err(|source| ApiError::Io {
        path: api_dir.to_path_buf(),
        source,
    })?;
    let mut written = Vec::new();
    for (name, text) in files {
        let path = api_dir.join(name);
        std::fs::write(&path, text).map_err(|source| ApiError::Io {
            path: path.clone(),
            source,
        })?;
        written.push(path);
    }
    Ok(Outcome::Written(written))
}
