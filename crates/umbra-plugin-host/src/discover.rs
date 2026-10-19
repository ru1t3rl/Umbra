//! Plugin package discovery (REQ-P-10, ADR 0022 and its amendment, ASM-121).
//!
//! A plugin path is a directory; each immediate subdirectory is one package holding exactly one library
//! with the platform extension (`.so`, `.dll`, `.dylib`; matched by extension only) and optionally a
//! `plugin.toml`. Loose files in the plugin path are ignored. Packages come in path order, then by the
//! UTF-8 bytes of the subdirectory name; a non-UTF-8 name is an error on every OS, so the order never
//! depends on UTF-16 or locale rules. Symlinked packages and libraries are followed; the library path
//! of a package is canonical (the identity used for duplicates and the image table). Discovery never
//! opens a library.

use std::fs;
use std::path::{Path, PathBuf};

use crate::api::PluginPackage;
use crate::error::PluginError;

/// The manifest file name inside a package.
const MANIFEST: &str = "plugin.toml";

/// Discovers the packages under `paths` (see the module docs).
pub(crate) fn discover(paths: &[PathBuf]) -> Result<Vec<PluginPackage>, PluginError> {
    let mut packages = Vec::new();
    for path in paths {
        for (_, dir) in package_dirs(path)? {
            packages.push(package(&dir)?);
        }
    }
    Ok(packages)
}

fn discovery(path: &Path, message: impl Into<String>) -> PluginError {
    PluginError::Discovery {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

/// The package directories of one plugin path, sorted by their UTF-8 names (bytes).
fn package_dirs(path: &Path) -> Result<Vec<(String, PathBuf)>, PluginError> {
    let entries = fs::read_dir(path)
        .map_err(|e| discovery(path, format!("cannot read the plugin directory: {e}")))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|e| discovery(path, format!("cannot read the plugin directory: {e}")))?;
        let dir = entry.path();
        // `fs::metadata` follows symlinks: a symlinked package directory is a package.
        if !fs::metadata(&dir).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| discovery(&dir, "package directory name is not valid UTF-8"))?;
        dirs.push((name, dir));
    }
    dirs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(dirs)
}

/// The package in `dir`: exactly one library, an optional manifest.
fn package(dir: &Path) -> Result<PluginPackage, PluginError> {
    let entries =
        fs::read_dir(dir).map_err(|e| discovery(dir, format!("cannot read the package: {e}")))?;
    let mut libraries = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| discovery(dir, format!("cannot read the package: {e}")))?;
        let file = entry.path();
        let is_library = file
            .extension()
            .is_some_and(|ext| ext == std::env::consts::DLL_EXTENSION);
        if is_library && fs::metadata(&file).is_ok_and(|m| m.is_file()) {
            libraries.push(file);
        }
    }
    libraries.sort();
    let library = match libraries.as_slice() {
        [one] => one,
        [] => {
            return Err(discovery(
                dir,
                format!(
                    "the package holds no library (*.{})",
                    std::env::consts::DLL_EXTENSION
                ),
            ));
        }
        several => {
            let names: Vec<String> = several
                .iter()
                .map(|l| {
                    l.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            return Err(discovery(
                dir,
                format!(
                    "the package holds {} libraries, expected one: {}",
                    names.len(),
                    names.join(", ")
                ),
            ));
        }
    };
    let library = fs::canonicalize(library)
        .map_err(|e| discovery(library, format!("cannot resolve the library path: {e}")))?;
    let manifest = dir.join(MANIFEST);
    let manifest = fs::metadata(&manifest)
        .is_ok_and(|m| m.is_file())
        .then_some(manifest);
    Ok(PluginPackage {
        dir: dir.to_path_buf(),
        library,
        manifest,
    })
}
