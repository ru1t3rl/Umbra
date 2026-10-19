//! Shared harness of the plugin-host acceptance tests (M4 step 4): forged descriptors, a hand-written
//! raw-ABI behavior plugin with selectable misbehavior, a log recorder, a temp-directory helper and the
//! WI-10 build-helper stub. Each test binary uses a subset.
#![expect(dead_code, reason = "each test binary uses a subset of the harness")]
#![expect(unsafe_code, reason = "the harness plays the plugin side of the C ABI")]
#![expect(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "test harness: small ABI sizes, known-valid values, raw pointers on purpose"
)]

#[macro_use]
pub mod forge;
pub mod bridge;
pub mod plugins;
pub mod raw;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use umbra_plugin_host::{LogLevel, PluginLog};

/// Records every plugin log message.
#[derive(Default)]
pub struct Recorder {
    pub entries: Mutex<Vec<(String, LogLevel, String)>>,
}

impl PluginLog for Recorder {
    fn log(&self, plugin: &umbra_core::PluginId, level: LogLevel, message: &str) {
        self.entries.lock().expect("log lock").push((
            plugin.as_str().to_owned(),
            level,
            message.to_owned(),
        ));
    }
}

impl Recorder {
    pub fn arc() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn all(&self) -> Vec<(String, LogLevel, String)> {
        self.entries.lock().expect("log lock").clone()
    }
}

/// A unique, empty directory under `CARGO_TARGET_TMPDIR` (never `/tmp`, never reused).
pub fn tmp_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "host-{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// The platform library file name for `stem` (`libstem.so`, `stem.dll`, `libstem.dylib`).
pub fn lib_name(stem: &str) -> String {
    format!(
        "{}{stem}.{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_EXTENSION
    )
}

/// Writes `plugins/<package>/<lib>` (garbage bytes: discovery and manifest checks never open it) and
/// an optional `plugin.toml`; returns the plugin directory.
pub fn garbage_package(root: &Path, package: &str, manifest: Option<&str>) -> PathBuf {
    let dir = root.join(package);
    std::fs::create_dir_all(&dir).expect("package dir");
    std::fs::write(dir.join(lib_name("garbage")), b"not a library").expect("lib");
    if let Some(m) = manifest {
        std::fs::write(dir.join("plugin.toml"), m).expect("manifest");
    }
    dir
}

/// Builds a behavior of `registry`'s plugin factory `id` from JSON `params` inside the build scope of the
/// host's engine (ASM-118, RR-m2).
///
/// Plan gap: the plan defines the build scope only through `Engine::attach_plugins` and
/// `instantiate_with_plugins` (umbra-engine), which this crate's tests cannot use (`umbra-plugin-host`
/// may not depend on the engine), and names no host-level way in. WI-9 must provide one (for example a
/// scope guard on `PluginHost`) and implement this helper with it; every bridge, fault and token test
/// builds through it.
#[expect(clippy::todo, reason = "M4 WI-9 stub")]
pub fn build_behavior(
    _host: &umbra_plugin_host::PluginHost,
    _registry: &umbra_core::Registry,
    _id: &str,
    _params: &str,
) -> Result<Arc<dyn umbra_core::Behavior>, umbra_core::BuildError> {
    todo!("M4 WI-9: enter the host's build scope, then Factory::build")
}

/// Keeps the textual macros "used" in binaries that do not need them.
fn macros_are_used() {
    let _ = forged!(forge::parts());
    let _ = forged_null!();
}
