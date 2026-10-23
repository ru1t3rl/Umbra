//! Shared harness of the M4 engine and CLI plugin tests (step 4): builds the real plugin libraries
//! and writes small scenarios as files. Included by `umbra-engine` tests with `mod plugin_support;`
//! and by `umbra-cli/tests/cli_plugin.rs` with a `#[path]` (DRY; `CARGO_MANIFEST_DIR` and
//! `CARGO_TARGET_TMPDIR` resolve to the including crate, and both sit two levels below the workspace
//! root). Each test binary uses a subset.
//!
//! Libraries are built with a nested `cargo build --locked --offline -p <package>` into their own
//! target directory under `CARGO_TARGET_TMPDIR` (never `/tmp`), then copied to a package directory
//! under a unique file name, so a library that may still be loaded is never overwritten (R-M8, ADR
//! 0023 item 4, ASM-115). Plugin packages follow ADR 0022: `<plugin dir>/<package>/<library>`; the
//! manifest is optional and these helpers do not write one.
//!
//! The fixture plugin contract assumed here beyond the plan's WI-10 list (plan gaps, reported):
//! package `umbra-test-plugin`, plugin `test.plugin.fixture` 0.1.0, and in the default build, besides
//! the fault-mode capabilities, these behaviors (all plain `number` or `entity` params):
//! - `test.behavior.ok`: ticks Running forever, no fault, no intents that need params;
//! - `test.behavior.draw_speed`: each tick draws `next_u64() % 100` from the entity stream, emits
//!   `SetSpeed(draw as f64)` and returns Running (twin of the Rust stub in `plugin_example.rs`);
//! - `test.behavior.needs_target`: metadata `param_types = { target = "entity" }` (binding-only,
//!   REQ-B-10) and a schema WITHOUT a `target` property (ADR 0020: `entity` params are binding-only
//!   and absent from the schema; the host refuses otherwise, ASM-135, ASM-142); ticks Running;
//! - `test.behavior.cross_param`: params `lo` and `hi` (numbers); `configure` fails when `lo > hi`.
//!
//! The fault-mode capabilities are only those a plugin built with `umbra-sdk` can produce (human
//! decision 2026-10-19 after the WI-7 review, ASM-132): `lifecycle` (logs its calls), `panic_configure`,
//! `panic_instantiate`, `panic_tick`, `panic_halt`, `panic_halt_running`, `panic_set_input`,
//! `panic_output`, `panic_destroy`, `bad_intent` (an intent the host refuses), `intent_flood` (65
//! intents for one item) and `bad_value` (an output value that breaks the `BbValue` invariants), all
//! `test.behavior.*`. There is no `no_status`, `bad_status`, `bad_index`, `foreign_rng`, `stale_token`,
//! `null_handle`, `halt_error` or `output_twice`: the safe SDK cannot express them, and the
//! in-process raw-ABI host tests (`faults.rs`, `tokens.rs`) cover those rows.
//!
//! The cargo feature `bad_schema` adds `test.behavior.bad_schema`, whose param schema is outside the
//! ADR 0009 subset (an `anyOf`; `oneOf` IS in the subset). The plugin-level fault features are
//! `entry_panics`, `init_fails`, `init_panics` and `shutdown_panics`. A library without the entry symbol,
//! or whose entry symbol resolves to null, cannot be built from a `forbid(unsafe_code)` Rust crate: the
//! host tests build those two from a few lines of C with `cc`. The panic strategy is a profile setting,
//! not a feature: `library_with` passes extra cargo arguments, for example `--config
//! profile.dev.panic="abort"`.
//!
//! This is the one build helper of the M4 tests: the host tests include it by path too
//! (`umbra-plugin-host/tests/support/plugins.rs`).
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp_root() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).to_path_buf()
}

/// The built library of `package` with `features`, cached per process. Builds are serialised.
pub fn library(package: &str, features: &[&str]) -> PathBuf {
    library_with(package, features, &[])
}

/// [`library`] with extra arguments for `cargo build` (for example `--config` overrides); they are part
/// of the cache key and of the build directory name.
pub fn library_with(package: &str, features: &[&str], extra: &[&str]) -> PathBuf {
    static CACHE: Mutex<BTreeMap<String, PathBuf>> = Mutex::new(BTreeMap::new());
    let mut key = format!("{package}+{}", features.join("+"));
    if !extra.is_empty() {
        let tag: String = extra
            .iter()
            .flat_map(|a| a.chars())
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        key = format!("{key}+{tag}");
    }
    let mut cache = CACHE.lock().expect("build cache lock");
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    let target = tmp_root().join("plugin-build").join(&key);
    let mut cmd = Command::new(env!("CARGO"));
    cmd.current_dir(workspace_root())
        .args(["build", "--locked", "--offline", "-p", package])
        .arg("--target-dir")
        .arg(&target)
        .args(extra);
    if !features.is_empty() {
        cmd.arg("--features").arg(features.join(","));
    }
    let status = cmd.status().expect("run the nested cargo build");
    assert!(status.success(), "building {package} {features:?} failed");
    let lib = target.join("debug").join(format!(
        "{}{}.{}",
        std::env::consts::DLL_PREFIX,
        package.replace('-', "_"),
        std::env::consts::DLL_EXTENSION
    ));
    assert!(lib.is_file(), "built library missing: {}", lib.display());
    cache.insert(key, lib.clone());
    lib
}

/// A new plugin directory holding one package per `(package name, library)`, created in the given
/// order, each library copied under a unique file name. Returns the plugin directory.
pub fn plugins_dir(packages: &[(&str, &Path)]) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let id = std::process::id();
    let parent = tmp_root().join("plugin-packages");
    prune_stale_packages(&parent, id);
    let root = parent.join(format!("{id}-{n}"));
    let _ = std::fs::remove_dir_all(&root);
    for (name, lib) in packages {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).expect("package dir");
        let file = format!(
            "{}{name}-{id}-{n}.{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_EXTENSION
        );
        std::fs::copy(lib, dir.join(file)).expect("copy the library");
    }
    root
}

/// Removes, once per process, the `<pid>-<n>` package directories under `parent` left by earlier test
/// processes. Each holds copies of whole plugin libraries and was never deleted, because a library
/// may still be loaded while its process runs; over many runs they filled the disk (57 GB), and RAM
/// where the target dir sits on a tmpfs. A directory is stale when its process is gone: on Linux when
/// `/proc/<pid>` is absent, elsewhere when it is older than an hour (no test process runs that long).
/// Removal errors are ignored, so a library still loaded (Windows) only keeps its directory.
fn prune_stale_packages(parent: &Path, own: u32) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Ok(entries) = std::fs::read_dir(parent) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|n| n.split_once('-'))
                .and_then(|(pid, _)| pid.parse::<u32>().ok())
            else {
                continue;
            };
            if pid != own && !process_alive(pid, &entry.path()) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    });
}

#[cfg(target_os = "linux")]
fn process_alive(pid: u32, _dir: &Path) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_alive(_pid: u32, dir: &Path) -> bool {
    const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(3600);
    std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age < MAX_AGE)
}

/// Writes `<plugins dir>/<package>/plugin.toml` for plugin `id` `version` and ABI 0.0 (ADR 0022).
pub fn write_manifest(plugins: &Path, package: &str, id: &str, version: &str) {
    let text =
        format!("schema_version = 1\nid = \"{id}\"\nversion = \"{version}\"\nabi = \"0.0\"\n");
    std::fs::write(plugins.join(package).join("plugin.toml"), text).expect("manifest");
}

/// A plugin directory with the example plugin (`example.plugin.wander`) as package `wander`.
pub fn wander() -> PathBuf {
    plugins_dir(&[("wander", &library("example-wander", &[]))])
}

/// A plugin directory with the fixture plugin built with `features` as package `fixture`.
pub fn fixture(features: &[&str]) -> PathBuf {
    plugins_dir(&[("fixture", &library("umbra-test-plugin", features))])
}

/// A plugin directory with the DIS adapter as package `dis`: the built library and the manifest the
/// crate ships (M6 WI-16; the same layout `cargo xtask build-plugins` packages).
pub fn dis() -> PathBuf {
    let dir = plugins_dir(&[("dis", &library("umbra-net-dis", &[]))]);
    std::fs::copy(
        workspace_root().join("crates/umbra-net-dis/plugin.toml"),
        dir.join("dis/plugin.toml"),
    )
    .expect("copy the shipped plugin.toml");
    dir
}

/// `examples/scenarios/dis_pair` (M6 WI-16).
pub fn dis_pair_example_dir() -> PathBuf {
    workspace_root().join("examples/scenarios/dis_pair")
}

/// `examples/scenarios/plugin_wander` (WI-13).
pub fn wander_example_dir() -> PathBuf {
    workspace_root().join("examples/scenarios/plugin_wander")
}

/// One generated template: its ID, the behavior `ref` and the inline `params` table text (may be
/// empty).
pub struct Tpl<'a> {
    pub id: &'a str,
    pub behavior: &'a str,
    pub params: &'a str,
}

/// Writes a small scenario under a fresh directory and returns the path of its `scenario.toml`:
/// flat terrain, 30 Hz, seed 7, as fast as possible, `scenario_extra` lines in `[scenario]`
/// (for example `on_plugin_fault = "disable_capability"`), one template file per `Tpl` (a ground
/// vehicle with `behavior`), the given graph files (`(file stem, full text)`), and one entity per
/// `(template id, entity name)`, 50 m apart on a row.
pub fn write_scenario(
    name: &str,
    duration_s: f64,
    scenario_extra: &str,
    templates: &[Tpl<'_>],
    graphs: &[(&str, String)],
    entities: &[(&str, &str)],
) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = tmp_root().join(format!(
        "plugin-scenarios/{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("templates")).expect("templates dir");
    std::fs::create_dir_all(dir.join("behaviors")).expect("behaviors dir");
    for t in templates {
        let params = if t.params.is_empty() {
            String::new()
        } else {
            format!("params = {}\n", t.params)
        };
        let text = format!(
            "schema_version = 1\nid = \"{id}\"\ndescription = \"generated\"\n\n[motion]\n\
             model = \"builtin.ground.kinematic\"\n\
             params = {{ max_speed_mps = 18.0, max_turn_rate_dps = 40.0, max_accel_mps2 = 2.5 }}\n\n\
             [behavior]\nref = \"{behavior}\"\n{params}",
            id = t.id,
            behavior = t.behavior
        );
        std::fs::write(dir.join(format!("templates/{}.toml", t.id)), text).expect("template");
    }
    for (stem, text) in graphs {
        std::fs::write(dir.join(format!("behaviors/{stem}.behavior.toml")), text).expect("graph");
    }
    let mut scenario = format!(
        "schema_version = 1\n\n[scenario]\nname = \"{name}\"\nseed = 7\ntick_rate_hz = 30\n\
         clock = \"afap\"\nduration_s = {duration_s:?}\n\
         template_paths = [\"templates\", \"behaviors\"]\n{scenario_extra}\n\n\
         [terrain]\nprovider = \"builtin.terrain.flat\"\nparams = {{ height_m = 0.0 }}\n"
    );
    for (i, (template, entity)) in entities.iter().enumerate() {
        let lon = 4.0 + 0.0007 * f64::from(u32::try_from(i).expect("small index"));
        write!(
            scenario,
            "\n[[entities]]\ntemplate = \"{template}\"\nname = \"{entity}\"\n\
             position = {{ lat_deg = 52.0, lon_deg = {lon:.7} }}\nheading_deg = 90.0\nspeed_mps = 5.0\n"
        )
        .expect("write to a String");
    }
    let path = dir.join("scenario.toml");
    std::fs::write(&path, scenario).expect("scenario");
    path
}

/// A graph file whose only node calls `target` as a `subgraph` (no params).
pub fn call_graph(id: &str, target: &str) -> String {
    format!(
        "schema_version = 1\nid = \"{id}\"\nversion = \"1.0.0\"\nroot = \"call\"\n\n\
         [nodes.call]\ntype = \"subgraph\"\nref = \"{target}\"\n"
    )
}

/// Copies `from` to `to` recursively.
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read_dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy");
        }
    }
}

/// A fresh copy of `from` under the temp directory, for tests that edit the files.
pub fn scratch_copy(from: &Path, name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let to = tmp_root().join(format!(
        "plugin-copies/{}-{}-{name}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&to);
    copy_dir(from, &to);
    to
}

/// The fault scenario of the engine and CLI fault tests (fixture plugin), with the entities named in
/// `which` out of: `healthy` runs `test.behavior.ok`, `direct` runs `test.behavior.panic_tick`,
/// `via-graph` calls `panic_tick` as a `subgraph` child of a file graph. `policy` is `abort` or
/// `disable_capability`.
pub fn fault_scenario(name: &str, policy: &str, duration_s: f64, which: &[&str]) -> PathBuf {
    let all = [
        ("test.entity.healthy", "healthy"),
        ("test.entity.direct", "direct"),
        ("test.entity.via_graph", "via-graph"),
    ];
    let entities: Vec<(&str, &str)> = all.into_iter().filter(|(_, n)| which.contains(n)).collect();
    assert_eq!(entities.len(), which.len(), "unknown entity in {which:?}");
    write_scenario(
        name,
        duration_s,
        &format!("on_plugin_fault = \"{policy}\""),
        &[
            Tpl {
                id: "test.entity.healthy",
                behavior: "test.behavior.ok",
                params: "",
            },
            Tpl {
                id: "test.entity.direct",
                behavior: "test.behavior.panic_tick",
                params: "",
            },
            Tpl {
                id: "test.entity.via_graph",
                behavior: "test.graph.calls_panic",
                params: "",
            },
        ],
        &[(
            "calls_panic",
            call_graph("test.graph.calls_panic", "test.behavior.panic_tick"),
        )],
        &entities,
    )
}
