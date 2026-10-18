//! `cargo xtask gen-header [--check]`: the C header of the plugin ABI (REQ-SDK-2, gate 11, ADR 0023
//! item 2, ASM-009).
//!
//! Runs the pinned `cbindgen` binary (an installed tool, never a dependency: MPL-2.0 stays out of
//! `Cargo.lock`) on a crate directory with its `cbindgen.toml` and writes or compares
//! `<crate>/include/umbra_plugin_api.h`. Line endings are normalised to LF on both sides, so a CRLF
//! checkout is not drift. The binary is `cbindgen` on `PATH`, or the program named by the `CBINDGEN`
//! environment variable. The crate metadata comes from `cargo metadata --locked --offline` run here
//! (written under `target/gen-header/` and passed as `cbindgen --metadata`), so cbindgen never resolves
//! or updates `Cargo.lock` itself and a missing or stale lockfile is exit 2.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The pinned `cbindgen` version; any other version is refused (its output may differ).
pub const CBINDGEN_VERSION: &str = "0.29.4";

/// How to install the pinned `cbindgen`.
pub const INSTALL_HINT: &str = "cargo install --locked cbindgen@0.29.4";

/// Environment variable naming the `cbindgen` program to run instead of `cbindgen` on `PATH`.
pub const CBINDGEN_ENV: &str = "CBINDGEN";

/// The header file, relative to the crate directory.
pub const HEADER_PATH: &str = "include/umbra_plugin_api.h";

/// Errors of `gen-header` (all exit 2).
#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    /// `cbindgen` could not be started.
    #[error("cbindgen not found (`{program}`); install it with `{INSTALL_HINT}`")]
    Missing {
        /// Program that was tried.
        program: String,
        /// Spawn error.
        source: std::io::Error,
    },
    /// `cbindgen --version` is not the pinned version.
    #[error(
        "cbindgen {CBINDGEN_VERSION} required, `{program} --version` printed `{found}`; install it with `{INSTALL_HINT}`"
    )]
    WrongVersion {
        /// Program that was run.
        program: String,
        /// Its first output line.
        found: String,
    },
    /// `cargo metadata --locked --offline` failed (missing or stale `Cargo.lock`, or not cached).
    #[error("cargo metadata --locked --offline failed for {manifest}:\n{stderr}")]
    Metadata {
        /// Manifest.
        manifest: PathBuf,
        /// Its stderr.
        stderr: String,
    },
    /// `cbindgen` ran and failed.
    #[error("cbindgen failed on {dir}:\n{stderr}")]
    Failed {
        /// Crate directory.
        dir: PathBuf,
        /// Its stderr.
        stderr: String,
    },
    /// Reading or writing a file failed.
    #[error("{path}")]
    Io {
        /// File.
        path: PathBuf,
        /// Error.
        source: std::io::Error,
    },
}

/// Result of [`run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `--check`: the committed header equals the generated one.
    Current,
    /// `--check`: they differ, first at this 1-based line.
    Drift {
        /// Header path.
        path: PathBuf,
        /// First differing line (1-based).
        line: usize,
    },
    /// The header was written.
    Written(PathBuf),
}

/// `text` with every CRLF replaced by LF.
#[must_use]
pub fn normalise_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// The first 1-based line where the LF-normalised texts differ, or `None` if they are equal.
#[must_use]
pub fn first_difference(generated: &str, committed: &str) -> Option<usize> {
    let (a, b) = (normalise_newlines(generated), normalise_newlines(committed));
    if a == b {
        return None;
    }
    let mut left = a.split('\n');
    let mut right = b.split('\n');
    let mut line = 1;
    loop {
        match (left.next(), right.next()) {
            (Some(x), Some(y)) if x == y => line += 1,
            _ => return Some(line),
        }
    }
}

/// The `cbindgen` program: `$CBINDGEN` if set, else `cbindgen`.
#[must_use]
pub fn cbindgen_program() -> OsString {
    std::env::var_os(CBINDGEN_ENV).unwrap_or_else(|| OsString::from("cbindgen"))
}

/// Checks that `program --version` prints `cbindgen <CBINDGEN_VERSION>`.
pub fn check_version(program: &OsString) -> Result<(), HeaderError> {
    let name = program.to_string_lossy().into_owned();
    let out = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|source| HeaderError::Missing {
            program: name.clone(),
            source,
        })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let found = stdout.lines().next().unwrap_or("").trim().to_string();
    if out.status.success() && found == format!("cbindgen {CBINDGEN_VERSION}") {
        Ok(())
    } else {
        Err(HeaderError::WrongVersion {
            program: name,
            found,
        })
    }
}

/// The directory for scratch files: `$CARGO_TARGET_DIR/gen-header`, else `<repo>/target/gen-header`.
fn scratch_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || {
            let xtask_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
            xtask_dir.parent().unwrap_or(xtask_dir).join("target")
        },
        PathBuf::from,
    );
    target.join("gen-header")
}

/// `cargo metadata --locked --offline --all-features --format-version 1` for `crate_dir`, written to a
/// per-process file under [`scratch_dir`]; returns its path.
fn write_metadata(crate_dir: &Path) -> Result<PathBuf, HeaderError> {
    let manifest = crate_dir.join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--all-features",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .map_err(|source| HeaderError::Io {
            path: manifest.clone(),
            source,
        })?;
    if !out.status.success() {
        return Err(HeaderError::Metadata {
            manifest,
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).map_err(|source| HeaderError::Io {
        path: dir.clone(),
        source,
    })?;
    let path = dir.join(format!("metadata-{}.json", std::process::id()));
    std::fs::write(&path, &out.stdout).map_err(|source| HeaderError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// Runs `program` on `crate_dir` with `crate_dir/cbindgen.toml` and the locked metadata and returns the
/// LF-normalised header.
pub fn generate(program: &OsString, crate_dir: &Path) -> Result<String, HeaderError> {
    let metadata = write_metadata(crate_dir)?;
    let out = Command::new(program)
        .arg("--quiet")
        .arg("--config")
        .arg(crate_dir.join("cbindgen.toml"))
        .arg("--metadata")
        .arg(&metadata)
        .arg(crate_dir)
        .output();
    // Best effort: a leftover scratch file under target/ is harmless.
    let _ = std::fs::remove_file(&metadata);
    let out = out.map_err(|source| HeaderError::Missing {
        program: program.to_string_lossy().into_owned(),
        source,
    })?;
    if !out.status.success() {
        return Err(HeaderError::Failed {
            dir: crate_dir.to_path_buf(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(normalise_newlines(&String::from_utf8_lossy(&out.stdout)))
}

/// Generates the header of `crate_dir` and writes it to `crate_dir/include/umbra_plugin_api.h`, or with
/// `check` compares it to the file there (LF-normalised).
pub fn run(crate_dir: &Path, check: bool) -> Result<Outcome, HeaderError> {
    let program = cbindgen_program();
    check_version(&program)?;
    let generated = generate(&program, crate_dir)?;
    let path = crate_dir.join(HEADER_PATH);
    if check {
        let committed = std::fs::read_to_string(&path).map_err(|source| HeaderError::Io {
            path: path.clone(),
            source,
        })?;
        return Ok(match first_difference(&generated, &committed) {
            None => Outcome::Current,
            Some(line) => Outcome::Drift { path, line },
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| HeaderError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(&path, generated).map_err(|source| HeaderError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(Outcome::Written(path))
}
