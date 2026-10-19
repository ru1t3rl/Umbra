//! `cargo xtask build-plugins [--release]`: builds the example plugins and writes plugin packages
//! (gate 12, ADR 0022, ADR 0023 item 4, REQ-SDK-4).
//!
//! Each package is `<cargo target dir>/plugins/<name>/` holding the built library and the example's
//! `plugin.toml` (ADR 0022 layout), so `umbra run .. --plugin-path target/plugins` loads them
//! (ASM-113). The target directory is the one cargo reports (`cargo metadata`), so
//! `CARGO_TARGET_DIR` and `build.target-dir` are honoured (ASM-125 d). A package directory is
//! replaced as a whole, so it never holds a stale second library. The C example (`c-circle`) is
//! compiled with the `cc` crate against the generated header, with the same strict flags as the host
//! tests (`-std=c11 -pedantic -Wall -Wextra -Werror`, MSVC `/std:c11 /W4 /WX`; ADR 0023 item 3).

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

/// A C example plugin: one source file compiled against `umbra_plugin_api.h` into a shared library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CPlugin {
    /// Package directory name under `<target dir>/plugins/`.
    pub name: &'static str,
    /// Library stem: the file is `<DLL_PREFIX><stem>.<DLL_EXTENSION>`.
    pub stem: &'static str,
    /// Source directory relative to the workspace root; holds `plugin.c` and the `plugin.toml`.
    pub source: &'static str,
}

/// The C example plugins, in package-name order (packaged after [`RUST_PLUGINS`]).
pub const C_PLUGINS: &[CPlugin] = &[CPlugin {
    name: "c-circle",
    stem: "c_circle",
    source: "examples/plugins/c-circle",
}];

/// Directory of the generated C header, relative to the workspace root.
pub const C_HEADER_DIR: &str = "crates/umbra-plugin-api/include";

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
    /// No C compiler was found (a gate prerequisite, ADR 0023 item 3).
    #[error("no C compiler for {source_file}: {message}")]
    Compiler {
        /// The C source that was to be compiled.
        source_file: PathBuf,
        /// What `cc` reported.
        message: String,
    },
    /// The C compiler rejected a source (warnings are errors); its output is in `output`.
    #[error("C compile of {source_file} failed ({status}):\n{output}")]
    CompileC {
        /// The C source.
        source_file: PathBuf,
        /// Compiler exit status.
        status: std::process::ExitStatus,
        /// Compiler stdout and stderr.
        output: String,
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

/// The file name cargo gives the `cdylib` of `package` on this platform; also the name of a C
/// plugin library with that stem (`libc_circle.so`, `c_circle.dll`, `libc_circle.dylib`).
pub fn library_file_name(package: &str) -> String {
    format!(
        "{}{}.{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_EXTENSION
    )
}

/// Builds every Rust example plugin with `cargo build --locked` (release profile with `release`),
/// compiles every C example plugin with `cc`, and writes their packages. Returns the package
/// directories, [`RUST_PLUGINS`] first, then [`C_PLUGINS`].
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
    let mut libraries = Vec::new();
    for plugin in RUST_PLUGINS {
        let file = library_file_name(plugin.package);
        let library = target.join(profile).join(&file);
        if !library.is_file() {
            return Err(PluginsError::MissingLibrary(library));
        }
        libraries.push((plugin.name, library, file, root.join(plugin.source)));
    }
    // Every C library is compiled before any package is written, so a failed compile leaves the
    // existing packages untouched too.
    for plugin in C_PLUGINS {
        let file = library_file_name(plugin.stem);
        let work = target.join(profile).join("c-plugins").join(plugin.name);
        let source = root.join(plugin.source);
        let library = compile_c(
            &source.join("plugin.c"),
            &root.join(C_HEADER_DIR),
            &work,
            &file,
            release,
        )?;
        libraries.push((plugin.name, library, file, source));
    }
    libraries
        .iter()
        .map(|(name, library, file, source)| {
            write_package(
                &target.join("plugins").join(name),
                library,
                file,
                &source.join("plugin.toml"),
            )
        })
        .collect()
}

/// The target triple `cc` should compile for: the platform this xtask runs on (the plugins are
/// loaded by binaries built for the same host). Outside a build script `cc` cannot read `TARGET`.
fn c_target_triple() -> String {
    let arch = std::env::consts::ARCH;
    if cfg!(target_os = "windows") {
        if cfg!(target_env = "gnu") {
            format!("{arch}-pc-windows-gnu")
        } else {
            format!("{arch}-pc-windows-msvc")
        }
    } else if cfg!(target_os = "macos") {
        format!("{arch}-apple-darwin")
    } else if cfg!(target_os = "linux") {
        format!("{arch}-unknown-linux-gnu")
    } else {
        format!("{arch}-unknown-{}", std::env::consts::OS)
    }
}

/// Compiles the C plugin `source` against the header in `include` into `<work>/<file>` and returns
/// that path. Strict C11 with warnings as errors (ADR 0023 item 3): GCC/Clang `-std=c11 -pedantic
/// -Wall -Wextra -Werror -ffp-contract=off -fvisibility=hidden -shared -fPIC`, MSVC `/std:c11 /W4
/// /WX /fp:precise /LD` (objects and the import library stay in `work`); `-O2` (MSVC `/O2`) with `release`, else
/// no optimisation. `-ffp-contract=off` keeps the compiler from fusing a multiply and an add, so the
/// plugin's floating-point results do not depend on the CPU having FMA (REQ-K-4).
fn compile_c(
    source: &Path,
    include: &Path,
    work: &Path,
    file: &str,
    release: bool,
) -> Result<PathBuf, PluginsError> {
    let io = |action: &'static str, path: &Path| {
        let path = path.to_path_buf();
        move |source| PluginsError::Io {
            action,
            path,
            source,
        }
    };
    if work.exists() {
        std::fs::remove_dir_all(work).map_err(io("removing", work))?;
    }
    std::fs::create_dir_all(work).map_err(io("creating", work))?;
    let library = work.join(file);
    let triple = c_target_triple();
    let mut build = cc::Build::new();
    build
        .cargo_metadata(false)
        .cargo_warnings(false)
        .emit_rerun_if_env_changed(false)
        .target(&triple)
        .host(&triple)
        .opt_level(if release { 2 } else { 0 })
        .debug(false)
        .out_dir(work)
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .std("c11")
        .include(include);
    let compiler_error = |e: cc::Error| PluginsError::Compiler {
        source_file: source.to_path_buf(),
        message: e.to_string(),
    };
    let msvc = build
        .try_get_compiler()
        .map_err(compiler_error)?
        .is_like_msvc();
    if !msvc {
        build
            .flag("-pedantic")
            .flag("-ffp-contract=off")
            .flag("-fvisibility=hidden");
    }
    let mut cmd = build
        .try_get_compiler()
        .map_err(compiler_error)?
        .to_command();
    cmd.current_dir(work);
    if msvc {
        // `/Fo` with a trailing separator names the object directory; `/LD` links a DLL whose import
        // library and export file land next to it, in `work`, never in the package.
        let mut object_dir = work.as_os_str().to_owned();
        object_dir.push(std::path::MAIN_SEPARATOR_STR);
        let mut fo = OsString::from("/Fo");
        fo.push(&object_dir);
        let mut fe = OsString::from("/Fe");
        fe.push(&library);
        // REQ-K-4: `/fp:precise` (MSVC's default, passed explicitly so a `CFLAGS` `/fp:fast` cannot
        // win silently) keeps IEEE semantics and, since VS 2022, no multiply-add contraction, the
        // counterpart of `-ffp-contract=off` above. Unverified: no Windows run (ASM-148 f).
        cmd.arg("/fp:precise")
            .arg(source)
            .arg(fo)
            .arg(fe)
            .arg("/LD");
    } else {
        cmd.arg("-shared")
            .arg("-fPIC")
            .arg(source)
            .arg("-o")
            .arg(&library);
    }
    let out = cmd.output().map_err(|e| PluginsError::Spawn {
        program: "C compiler".to_owned(),
        source: e,
    })?;
    if !out.status.success() {
        return Err(PluginsError::CompileC {
            source_file: source.to_path_buf(),
            status: out.status,
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        });
    }
    if !library.is_file() {
        return Err(PluginsError::MissingLibrary(library));
    }
    Ok(library)
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
