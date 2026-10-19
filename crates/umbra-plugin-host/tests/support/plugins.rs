//! Build helper of the host tests (R-M8): one helper for all M4 plugin tests, shared by path with the
//! engine and CLI tests (`umbra-engine/tests/plugin_support/mod.rs`, included by `#[path]`: the host
//! tests therefore depend on the engine test directory layout, not on the engine crate). Builds a variant of
//! `umbra-test-plugin` with a nested `cargo build --locked --offline` into `CARGO_TARGET_TMPDIR` and
//! copies the library to a package directory under a unique file name, so a library that may still be
//! loaded is never overwritten. Libraries that a Rust crate under `forbid(unsafe_code)` cannot be
//! (no entry symbol, a null entry symbol) are compiled from a few lines of C with `cc`.
//!
//! Fixture features: `entry_panics`, `init_fails`, `init_panics`, `shutdown_panics`, `bad_schema`; the
//! empty list is the default build with one capability per fault mode. The packages carry a
//! `plugin.toml` for `test.plugin.fixture` 0.1.0.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[path = "../../../umbra-engine/tests/plugin_support/mod.rs"]
pub mod shared;

use std::path::{Path, PathBuf};
use std::process::Command;

/// The platform's `cc` target triple, as the `cc` crate wants it outside a build script.
fn target_triple() -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => format!("{arch}-unknown-linux-gnu"),
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        os => format!("{arch}-unknown-{os}"),
    }
}

/// Compiles the C file `source` into the shared library `lib` (`-std=c11 -pedantic` and warnings as
/// errors, MSVC equivalents); `Err` is the compiler output.
pub fn compile_shared(source: &Path, lib: &Path, include: Option<&Path>) -> Result<(), String> {
    let triple = target_triple();
    let out_dir = lib.parent().expect("library directory");
    let mut build = cc::Build::new();
    build
        .cargo_metadata(false)
        .cargo_warnings(false)
        .emit_rerun_if_env_changed(false)
        .target(&triple)
        .host(&triple)
        .opt_level(0)
        .debug(false)
        .out_dir(out_dir)
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .std("c11");
    if let Some(dir) = include {
        build.include(dir);
    }
    let msvc = build
        .try_get_compiler()
        .map_err(|e| e.to_string())?
        .is_like_msvc();
    if !msvc {
        build.flag("-pedantic");
    }
    let mut cmd: Command = build
        .try_get_compiler()
        .map_err(|e| e.to_string())?
        .to_command();
    if msvc {
        cmd.arg("/LD")
            .arg(source)
            .arg(format!("/Fe{}", lib.display()));
    } else {
        cmd.arg("-shared")
            .arg("-fPIC")
            .arg(source)
            .arg("-o")
            .arg(lib);
    }
    let out = cmd.output().map_err(|e| format!("spawn compiler: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// A plugin directory with one package `variant` holding the fixture built with `features` and a
/// `plugin.toml` matching it.
pub fn package_dir(variant: &str, features: &[&str]) -> PathBuf {
    package_dir_with(variant, features, &[])
}

/// [`package_dir`] with extra `cargo build` arguments (for example the `panic = "abort"` profile).
pub fn package_dir_with(variant: &str, features: &[&str], extra: &[&str]) -> PathBuf {
    let lib = shared::library_with("umbra-test-plugin", features, extra);
    let dir = shared::plugins_dir(&[(variant, &lib)]);
    shared::write_manifest(&dir, variant, "test.plugin.fixture", "0.1.0");
    dir
}

/// A plugin directory with one package `variant` holding a library compiled from the C `source`
/// (no manifest).
pub fn c_package_dir(variant: &str, source: &str) -> PathBuf {
    let work = super::tmp_dir(&format!("c-{variant}"));
    let c_file = work.join("plugin.c");
    std::fs::write(&c_file, source).expect("write the C source");
    let lib = work.join(super::lib_name(variant));
    let include = Path::new(env!("CARGO_MANIFEST_DIR")).join("../umbra-plugin-api/include");
    compile_shared(&c_file, &lib, Some(&include))
        .unwrap_or_else(|e| panic!("compile {variant}:\n{e}"));
    shared::plugins_dir(&[(variant, &lib)])
}

/// A library that exports no `umbra_plugin_entry` at all.
pub const C_NO_ENTRY: &str = "int umbra_not_the_entry(void) { return 1; }\n";

/// A library whose `umbra_plugin_entry` symbol resolves to a null address (an absolute symbol of value
/// 0; a plain `void *umbra_plugin_entry = 0;` would be a non-null variable). ELF targets only.
pub const C_NULL_ENTRY: &str = "__asm__(\".globl umbra_plugin_entry\\n.set umbra_plugin_entry, 0\");\n\
                                int umbra_filler(void) { return 0; }\n";

/// A C plugin with no capabilities that counts its `umbra_plugin_entry` calls and reports the count
/// through the host `log` from `shutdown` (`entry_calls=N`): the one way to observe, from a test, how
/// often the host called the entry of a real library.
pub const C_COUNTER: &str = r#"#include "umbra_plugin_api.h"
#include <stdio.h>

static int entry_calls = 0;
static const UmbraHostApi *host_api = NULL;

static UmbraCode counter_init(const UmbraHostApi *host, const UmbraErrorSink *err)
{
    (void)err;
    host_api = host;
    return UMBRA_OK;
}

static void counter_shutdown(void)
{
    char text[64];
    int n = snprintf(text, sizeof text, "entry_calls=%d", entry_calls);
    host_api->log(host_api->host_ctx, UMBRA_LOG_INFO, (UmbraStr){ (const uint8_t *)text, (size_t)n });
}

static const UmbraPluginDescriptor descriptor = {
    UMBRA_ABI_VERSION, (uint32_t)sizeof(UmbraPluginDescriptor),
    UMBRA_STR_INIT("test.plugin.counter"), UMBRA_STR_INIT("Counter"), UMBRA_STR_INIT("0.1.0"),
    NULL, 0, counter_init, counter_shutdown
};

UMBRA_EXPORT const UmbraPluginDescriptor *umbra_plugin_entry(const UmbraHostApi *host)
{
    (void)host;
    entry_calls++;
    return &descriptor;
}
"#;
