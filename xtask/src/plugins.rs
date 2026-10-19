//! `cargo xtask build-plugins [--release]`: builds the example plugins and writes plugin packages
//! (gate 12, ADR 0022, ADR 0023 item 4, REQ-SDK-4).
//!
//! Each package is `<cargo target dir>/plugins/<name>/` holding the built library and the example's
//! `plugin.toml` (ADR 0022 layout), so `umbra run .. --plugin-path target/plugins` loads them
//! (ASM-113). The target directory is the one cargo reports (`cargo metadata`), so
//! `CARGO_TARGET_DIR` and `build.target-dir` are honoured (ASM-125 d). A package directory is
//! replaced as a whole, so it never holds a stale second library. The C example (`c-circle`) is
//! packaged from WI-14.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A Rust example plugin: the package directory name, the cargo package, its source directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RustPlugin {
    /// Package directory name under `<target dir>/plugins/`.
    pub name: &'static str,
    /// Cargo package name (a `cdylib` workspace member).
    pub package: &'static str,
    /// Source directory relative to the workspace root; holds the `plugin.toml`.
    pub source: &'static str,
}

/// The Rust example plugins, in package-name order.
pub const RUST_PLUGINS: &[RustPlugin] = &[RustPlugin {
    name: "wander",
    package: "example-wander",
    source: "examples/plugins/wander",
}];

/// Errors of `build-plugins` (all exit 2).
#[derive(Debug, thiserror::Error)]
pub enum PluginsError {
    /// `cargo` could not be started.
    #[error("could not run `{program}`")]
    Spawn {
        /// Program that was tried.
        program: String,
        /// Spawn error.
        source: std::io::Error,
    },
    /// `cargo metadata` failed or printed no `target_directory`.
    #[error("cargo metadata failed for {root}: {message}")]
    Metadata {
        /// Workspace root.
        root: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// `cargo build` failed (its diagnostics are already on stderr).
    #[error("cargo build of {packages} failed ({status})")]
    Build {
        /// Packages that were built.
        packages: String,
        /// Exit status.
        status: std::process::ExitStatus,
    },
    /// The built library is not where cargo puts it.
    #[error("built library missing: {0}")]
    MissingLibrary(PathBuf),
    /// Reading, copying or writing a file failed.
    #[error("{action} {path}")]
    Io {
        /// What was being done.
        action: &'static str,
        /// File or directory.
        path: PathBuf,
        /// Error.
        source: std::io::Error,
    },
}

/// The cargo program: `CARGO` (set by cargo for `cargo xtask` and tests), else `cargo` on `PATH`.
fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// The cargo target directory of the workspace at `root` (honours `CARGO_TARGET_DIR`).
///
/// # Errors
/// [`PluginsError::Spawn`] or [`PluginsError::Metadata`].
pub fn target_dir(root: &Path) -> Result<PathBuf, PluginsError> {
    let program = cargo();
    let out = Command::new(&program)
        .current_dir(root)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
        ])
        .output()
        .map_err(|source| PluginsError::Spawn {
            program: program.to_string_lossy().into_owned(),
            source,
        })?;
    let metadata_error = |message: String| PluginsError::Metadata {
        root: root.to_path_buf(),
        message,
    };
    if !out.status.success() {
        return Err(metadata_error(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| metadata_error(e.to_string()))?;
    json.get("target_directory")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| metadata_error("no `target_directory` in the output".to_owned()))
}

/// The file name cargo gives the `cdylib` of `package` on this platform.
pub fn library_file_name(package: &str) -> String {
    format!(
        "{}{}.{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_EXTENSION
    )
}

/// Builds every example plugin with `cargo build --locked` (release profile with `release`) and
/// writes its package. Returns the package directories, in [`RUST_PLUGINS`] order.
///
/// # Errors
/// [`PluginsError`]; a failed build leaves the existing packages untouched.
pub fn run(root: &Path, release: bool) -> Result<Vec<PathBuf>, PluginsError> {
    let target = target_dir(root)?;
    let program = cargo();
    let mut build = Command::new(&program);
    build.current_dir(root).args(["build", "--locked"]);
    if release {
        build.arg("--release");
    }
    for plugin in RUST_PLUGINS {
        build.args(["-p", plugin.package]);
    }
    let status = build.status().map_err(|source| PluginsError::Spawn {
        program: program.to_string_lossy().into_owned(),
        source,
    })?;
    if !status.success() {
        return Err(PluginsError::Build {
            packages: RUST_PLUGINS
                .iter()
                .map(|p| p.package)
                .collect::<Vec<_>>()
                .join(", "),
            status,
        });
    }
    let profile = if release { "release" } else { "debug" };
    RUST_PLUGINS
        .iter()
        .map(|plugin| {
            let file = library_file_name(plugin.package);
            let library = target.join(profile).join(&file);
            if !library.is_file() {
                return Err(PluginsError::MissingLibrary(library));
            }
            let manifest = root.join(plugin.source).join("plugin.toml");
            write_package(
                &target.join("plugins").join(plugin.name),
                &library,
                &file,
                &manifest,
            )
        })
        .collect()
}

/// Replaces `dir` by a package holding `library` (as `file`) and a copy of `manifest`.
fn write_package(
    dir: &Path,
    library: &Path,
    file: &str,
    manifest: &Path,
) -> Result<PathBuf, PluginsError> {
    let io = |action: &'static str, path: &Path| {
        let path = path.to_path_buf();
        move |source| PluginsError::Io {
            action,
            path,
            source,
        }
    };
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(io("removing", dir))?;
    }
    std::fs::create_dir_all(dir).map_err(io("creating", dir))?;
    std::fs::copy(library, dir.join(file)).map_err(io("copying", library))?;
    std::fs::copy(manifest, dir.join("plugin.toml")).map_err(io("copying", manifest))?;
    Ok(dir.to_path_buf())
}
