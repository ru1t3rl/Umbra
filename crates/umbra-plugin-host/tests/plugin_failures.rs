//! AT-4, gate 13: the plugin failure matrix against a REAL library, `umbra-test-plugin` built with
//! `umbra-sdk` (`crates/umbra-test-plugin`, WI-10). Requirements: REQ-P-1..10, REQ-SDK-3, section 15 plugin
//! tests, M4 acceptance 2, R-B4, R-B5, R-M2, R-M9, RR-B1, RR-M1..M3.
//!
//! The rows that need no real library (forged descriptors, raw-ABI misbehavior, image lifecycle states)
//! are in `descriptor.rs`, `manifest.rs`, `image.rs`, `faults.rs`, `tokens.rs`, `contention.rs`.
//!
//! Gate 13 covers only what a plugin built with `umbra-sdk` can produce (human decision 2026-10-19
//! after the WI-7 review, ASM-132): panics in every call, `intent_flood`, `bad_intent`, `bad_value`,
//! `init` and `shutdown` failures, plus the load and lifecycle rows. Misbehavior the safe SDK makes
//! impossible is covered only by the in-process raw-ABI tests: status left unset and status out of
//! range, intent item index `>= count`, `configure` OK with a null handle, `halt` returning an error
//! code, the output sink called twice (`faults.rs`), and a stale token and another item's RNG token
//! (`tokens.rs`).
//!
//! Step 4 of M4. Everything that needs the fixture crate was ignored until WI-10; the rows that need
//! only a garbage file or a C library (open error, missing and null entry, same file twice,
//! `keep_loaded`) run from WI-8.
//! The fixture contract the tests assume beyond the plan's capability list (plan gaps, reported; see
//! also `umbra-engine/tests/plugin_support/mod.rs`, the one build helper): cargo features
//! `entry_panics`, `init_fails`, `init_panics`, `shutdown_panics` (the plan's `no_entry` and
//! `panic_abort_profile` are not features: a library without the entry symbol, or with a null one, is
//! built from C with `cc`, and the panic strategy is a profile setting passed to the build helper as
//! `--config profile.dev.panic="abort"`); the package is `test.plugin.fixture` 0.1.0 with a
//! `plugin.toml` written by the helper; `test.behavior.lifecycle` logs exactly `create_behavior`,
//! `create_instance`, `destroy_instance`, `destroy_behavior` and the plugin's `shutdown` logs
//! `shutdown`; `test.behavior.ok` ticks Running; every faulty capability's fault shows in
//! `PluginHost::faults()` with the call named in its test and a message naming the problem.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[macro_use]
mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use support::bridge::{Ent, rate, tick_batched};
use support::plugins::{
    C_COUNTER, C_NO_ENTRY, C_NULL_ENTRY, c_package_dir, package_dir, package_dir_with,
};
use support::{Recorder, build_behavior, garbage_package, tmp_dir};
use umbra_core::{Behavior, BehaviorInstance, Registry, Status};
use umbra_plugin_host::{HostOptions, LogLevel, PluginError, PluginHost};

/// A host with the fixture (built with `features`) loaded and registered, and its log recorder.
fn fixture(variant: &str, features: &[&str]) -> (PluginHost, Registry, Arc<Recorder>) {
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    let mut host = PluginHost::new(options);
    let dir = package_dir(variant, features);
    for pkg in PluginHost::discover(&[dir]).expect("discover the fixture") {
        host.load(&pkg).expect("load the fixture");
    }
    let mut registry = Registry::new();
    host.register(&mut registry).expect("register the fixture");
    (host, registry, recorder)
}

fn build(host: &PluginHost, registry: &Registry, capability: &str) -> Arc<dyn Behavior> {
    build_behavior(host, registry, &format!("test.behavior.{capability}"), "{}").expect("build")
}

fn load_error_in(dir: PathBuf) -> PluginError {
    let mut host = PluginHost::new(HostOptions::default());
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    host.load(&pkgs[0]).expect_err("the load must fail")
}

fn load_error(variant: &str, features: &[&str]) -> PluginError {
    load_error_in(package_dir(variant, features))
}

/// A host whose plugin log goes to a recorder.
fn logging_host() -> (PluginHost, Arc<Recorder>) {
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    (PluginHost::new(options), recorder)
}

// ------------------------------------------------------------------------------------ load rows

#[test]
fn given_a_file_that_is_not_a_library_then_open_error() {
    let root = tmp_dir("not-a-library");
    garbage_package(&root, "junk", None);
    let mut host = PluginHost::new(HostOptions::default());
    let pkgs = PluginHost::discover(&[root]).expect("discover");

    let err = host.load(&pkgs[0]).expect_err("not a library");

    assert!(matches!(err, PluginError::Open { .. }), "{err:?}");
    assert!(err.to_string().contains("junk"), "names the path: {err}");
}

#[test]
fn given_a_library_without_umbra_plugin_entry_then_missing_entry() {
    let err = load_error_in(c_package_dir("no-entry", C_NO_ENTRY));

    assert!(matches!(err, PluginError::MissingEntry { .. }), "{err:?}");
    let msg = err.to_string();
    assert!(
        msg.contains("umbra_plugin_entry") && msg.contains("no-entry"),
        "names the symbol and the library path: {msg}"
    );
}

/// The entry symbol resolves to a null address (an ELF absolute symbol of value 0).
#[cfg(target_os = "linux")]
#[test]
fn given_a_null_entry_symbol_then_missing_entry() {
    let err = load_error_in(c_package_dir("null-entry", C_NULL_ENTRY));

    assert!(matches!(err, PluginError::MissingEntry { .. }), "{err:?}");
    assert!(err.to_string().contains("null-entry"), "{err}");
}

#[test]
fn given_an_entry_that_panics_then_null_descriptor() {
    let err = load_error("entry-panics", &["entry_panics"]);

    assert!(matches!(err, PluginError::NullDescriptor { .. }), "{err:?}");
    assert!(
        err.to_string().contains("entry-panics"),
        "names the origin: {err}"
    );
}

#[test]
fn given_init_returns_an_error_then_init_error() {
    let err = load_error("init-fails", &["init_fails"]);

    assert!(
        matches!(&err, PluginError::Init { plugin, .. } if plugin.as_str() == "test.plugin.fixture"),
        "{err:?}"
    );
}

#[test]
fn given_init_panics_then_init_error_with_the_panic_message() {
    let err = load_error("init-panics", &["init_panics"]);

    assert!(
        matches!(&err, PluginError::Init { plugin, .. } if plugin.as_str() == "test.plugin.fixture"),
        "{err:?}"
    );
    assert!(err.to_string().contains("panic"), "{err}");
}

/// ADR 0021: `shutdown` runs only after a successful `init`. The fixture's `shutdown` logs `shutdown`.
#[test]
fn given_init_panics_then_no_shutdown_runs_afterwards() {
    let (mut host, recorder) = logging_host();
    let pkgs = PluginHost::discover(&[package_dir("init-panics-quiet", &["init_panics"])])
        .expect("discover");

    host.load(&pkgs[0]).expect_err("init panics");
    drop(host);

    assert!(
        recorder.all().iter().all(|e| e.2 != "shutdown"),
        "{:?}",
        recorder.all()
    );
}

#[test]
fn given_shutdown_panics_then_dropping_the_host_swallows_and_logs_it() {
    let (host, registry, recorder) = fixture("shutdown-panics", &["shutdown_panics"]);

    drop(registry);
    drop(host);

    let errors: Vec<_> = recorder
        .all()
        .into_iter()
        .filter(|e| e.1 == LogLevel::Error)
        .collect();
    assert!(
        errors.iter().any(|e| e.2.contains("shutdown")),
        "{errors:?}"
    );
}

#[test]
fn given_a_manifest_whose_id_differs_from_the_descriptor_then_manifest_mismatch() {
    let dir = package_dir("manifest-id", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let manifest = pkgs[0]
        .manifest
        .clone()
        .expect("the helper writes a plugin.toml");
    let text = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(
        &manifest,
        text.replace("test.plugin.fixture", "test.plugin.other"),
    )
    .expect("write");
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load(&pkgs[0]).expect_err("mismatch");

    assert!(
        matches!(err, PluginError::ManifestMismatch { field: "id", .. }),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("manifest-id"),
        "names the origin: {err}"
    );
}

#[test]
fn given_a_manifest_whose_version_differs_from_the_descriptor_then_manifest_mismatch() {
    let dir = package_dir("manifest-version", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let manifest = pkgs[0].manifest.clone().expect("plugin.toml");
    let text = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(&manifest, text.replace("0.1.0", "9.9.9")).expect("write");
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load(&pkgs[0]).expect_err("mismatch");

    assert!(
        matches!(
            err,
            PluginError::ManifestMismatch {
                field: "version",
                ..
            }
        ),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("manifest-version"),
        "names the origin: {err}"
    );
}

#[test]
fn given_a_manifest_with_another_abi_then_rejected_without_opening_the_library() {
    let dir = package_dir("manifest-abi", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let manifest = pkgs[0].manifest.clone().expect("plugin.toml");
    let text = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(&manifest, text.replace("abi = \"0.0\"", "abi = \"1.0\"")).expect("write");
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load(&pkgs[0]).expect_err("mismatch");

    assert!(
        matches!(&err, PluginError::AbiMismatch { field, .. } if field == "plugin.toml abi"),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("manifest-abi"),
        "names the origin: {err}"
    );
}

#[test]
fn given_the_same_plugin_id_from_two_images_then_duplicate() {
    let first = PluginHost::discover(&[package_dir("twin-a", &[])]).expect("discover a");
    let second = PluginHost::discover(&[package_dir("twin-b", &[])]).expect("discover b");
    let mut host = PluginHost::new(HostOptions::default());
    host.load(&first[0]).expect("the first image loads");

    let err = host
        .load(&second[0])
        .expect_err("same plugin id, other image");

    assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
    let msg = err.to_string();
    assert!(
        msg.contains("test.plugin.fixture") && msg.contains("twin-b"),
        "{msg}"
    );
}

#[cfg(unix)]
#[test]
fn given_the_same_file_reached_through_a_symlink_then_duplicate_in_one_host() {
    let dir = package_dir("symlink-twice", &[]);
    let alias_root = tmp_dir("symlink-alias");
    let pkgs = PluginHost::discover(std::slice::from_ref(&dir)).expect("discover");
    let real = pkgs[0].dir.clone();
    std::os::unix::fs::symlink(&real, alias_root.join("alias")).expect("symlink");
    let mut host = PluginHost::new(HostOptions::default());
    host.load(&pkgs[0]).expect("first path");
    let aliased = PluginHost::discover(&[alias_root]).expect("discover alias");

    let err = host
        .load(&aliased[0])
        .expect_err("the same canonical library");

    assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
    assert!(
        err.to_string().contains("alias"),
        "names the second path: {err}"
    );
}

/// ADR 0022: the same file reached twice is found by its canonical path BEFORE a second entry call.
/// A C plugin counts its entry calls and reports the count from `shutdown`: exactly the two calls of
/// one lifecycle.
#[cfg(unix)]
#[test]
fn the_same_file_reached_twice_is_found_before_a_second_entry_call() {
    let dir = c_package_dir("counter-link", C_COUNTER);
    let alias_root = tmp_dir("counter-alias");
    let pkgs = PluginHost::discover(std::slice::from_ref(&dir)).expect("discover");
    std::os::unix::fs::symlink(&pkgs[0].dir, alias_root.join("alias")).expect("symlink");
    let aliased = PluginHost::discover(&[alias_root]).expect("discover alias");
    let (mut host, recorder) = logging_host();
    host.load(&pkgs[0]).expect("first path");

    let err = host
        .load(&aliased[0])
        .expect_err("the same canonical library");
    drop(host);

    assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
    let reported: Vec<_> = recorder.all().into_iter().map(|e| e.2).collect();
    assert!(
        reported.iter().any(|m| m == "entry_calls=2"),
        "one lifecycle, two entry calls: {reported:?}"
    );
}

#[test]
fn two_hosts_load_the_same_library_concurrently() {
    let dir = package_dir("two-hosts", &[]);
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let dir = dir.clone();
            thread::spawn(move || {
                let mut host = PluginHost::new(HostOptions::default());
                let pkgs = PluginHost::discover(&[dir]).expect("discover");
                host.load(&pkgs[0])
                    .map(|i| i.id.as_str().to_owned())
                    .map_err(|e| e.to_string())
            })
        })
        .collect();

    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("thread"))
        .collect();

    assert!(results.iter().all(Result::is_ok), "{results:?}");
}

#[test]
fn reload_after_release() {
    let dir = package_dir("reload", &[]);
    for round in 0..3 {
        let mut host = PluginHost::new(HostOptions::default());
        let pkgs = PluginHost::discover(std::slice::from_ref(&dir)).expect("discover");
        host.load(&pkgs[0])
            .unwrap_or_else(|e| panic!("round {round}: {e}"));
        let mut registry = Registry::new();
        host.register(&mut registry).expect("register");
        let b = build(&host, &registry, "ok");
        let mut ents = vec![Ent::new(1, &b, 1)];
        tick_batched(&b.batch().expect("batch"), &mut ents, 0);
        assert_eq!(host.faults(), Vec::new(), "round {round}");
    }
}

#[test]
fn reload_with_keep_loaded() {
    let dir = package_dir("reload-kept", &[]);
    for round in 0..3 {
        let mut options = HostOptions::default();
        options.keep_loaded = true;
        let mut host = PluginHost::new(options);
        let pkgs = PluginHost::discover(std::slice::from_ref(&dir)).expect("discover");
        host.load(&pkgs[0])
            .unwrap_or_else(|e| panic!("round {round}: {e}"));
    }
}

/// ADR 0021 (RR-M3): with `keep_loaded` the library stays mapped after the last handle is gone (and
/// `shutdown` still ran); without it the close unmaps it. A C plugin, because a Rust cdylib with
/// thread-local destructors may pin itself. Linux only: it reads `/proc/self/maps`.
#[cfg(target_os = "linux")]
#[test]
fn keep_loaded_skips_the_close() {
    let mapped = |marker: &str| {
        std::fs::read_to_string("/proc/self/maps")
            .expect("maps")
            .contains(marker)
    };
    for keep in [false, true] {
        let variant = format!("maps-{keep}");
        let marker = format!("lib{variant}-");
        let mut options = HostOptions::default();
        options.keep_loaded = keep;
        let mut host = PluginHost::new(options);
        let pkgs = PluginHost::discover(&[c_package_dir(&variant, C_COUNTER)]).expect("discover");
        host.load(&pkgs[0]).expect("load");
        assert!(mapped(&marker), "mapped while loaded (keep_loaded {keep})");

        drop(host);

        assert_eq!(mapped(&marker), keep, "mapped after the last release");
    }
}

/// RR-M1 on a real library: loads racing with last releases never fail or crash. The deterministic
/// version of the closing wait is `image.rs::load_waits_while_image_is_closing` (gated `shutdown`).
#[test]
fn concurrent_load_and_release_never_fails() {
    let dir = package_dir("churn", &[]);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let dir = dir.clone();
            thread::spawn(move || {
                for _ in 0..25 {
                    let mut host = PluginHost::new(HostOptions::default());
                    let pkgs = PluginHost::discover(std::slice::from_ref(&dir)).expect("discover");
                    host.load(&pkgs[0]).map(|_| ()).map_err(|e| e.to_string())?;
                }
                Ok::<(), String>(())
            })
        })
        .collect();

    for h in handles {
        assert_eq!(h.join().expect("thread"), Ok(()));
    }
}

// ------------------------------------------------------------------------------------ call rows

#[test]
fn panic_in_configure_is_a_build_error() {
    let (host, registry, _) = fixture("calls", &[]);

    let err = build_behavior(&host, &registry, "test.behavior.panic_configure", "{}")
        .err()
        .expect("build error");

    assert!(err.message.to_lowercase().contains("panic"), "{err}");
}

#[test]
fn panic_in_instantiate_is_an_instantiate_error() {
    let (host, registry, _) = fixture("calls", &[]);
    let b = build(&host, &registry, "panic_instantiate");

    let err = b.instantiate(rate()).err().expect("instantiate error");

    assert!(err.to_string().to_lowercase().contains("panic"), "{err}");
}

macro_rules! tick_fault {
    ($name:ident, $capability:expr, $needle:expr) => {
        #[test]
        fn $name() {
            let (host, registry, _) = fixture("calls", &[]);
            let b = build(&host, &registry, $capability);
            let mut ents = vec![Ent::new(1, &b, 1), Ent::new(2, &b, 1)];

            tick_batched(&b.batch().expect("batch"), &mut ents, 0);

            let faults = host.faults();
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert_eq!(faults[0].call, "tick_batch");
            assert_eq!(faults[0].plugin.as_str(), "test.plugin.fixture");
            assert_eq!(
                faults[0].capability.as_str(),
                concat!("test.behavior.", $capability)
            );
            assert!(
                faults[0].message.to_lowercase().contains($needle),
                "{:?} in {:?}",
                $needle,
                faults[0].message
            );
            assert!(
                ents.iter()
                    .all(|e| e.status == Some(Status::Failure) && e.buf.is_empty())
            );
        }
    };
}

tick_fault!(panic_in_tick_batch_is_a_fault, "panic_tick", "panic");
tick_fault!(invalid_intent_is_a_fault, "bad_intent", "intent");
tick_fault!(more_than_64_intents_is_a_fault, "intent_flood", "64");

macro_rules! method_fault {
    ($name:ident, $capability:expr, $call:expr, $act:expr) => {
        #[test]
        fn $name() {
            let (host, registry, _) = fixture("calls", &[]);
            let b = build(&host, &registry, $capability);
            let mut inst = b.instantiate(rate()).expect("instantiate");

            let act: fn(&mut dyn BehaviorInstance) = $act;
            act(inst.as_mut());

            let faults = host.faults();
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert_eq!(faults[0].call, $call);
            assert_eq!(faults[0].plugin.as_str(), "test.plugin.fixture");
            assert!(!faults[0].message.is_empty(), "a message names the problem");
        }
    };
}

method_fault!(panic_in_halt_is_a_fault, "panic_halt", "halt", |i| i.halt());
method_fault!(
    panic_in_halt_running_is_a_fault,
    "panic_halt_running",
    "halt_running",
    |i| i.halt_running()
);
method_fault!(
    panic_in_set_input_is_a_fault,
    "panic_set_input",
    "set_input",
    |i| {
        let _ = i.set_input("speed_mps", None);
    }
);
method_fault!(panic_in_output_is_a_fault, "panic_output", "output", |i| {
    let _ = i.output("turns");
});
method_fault!(
    invalid_output_value_is_a_fault,
    "bad_value",
    "output",
    |i| {
        let _ = i.output("turns");
    }
);

#[test]
fn panic_in_destroy_is_swallowed_and_logged() {
    let (host, registry, recorder) = fixture("calls", &[]);
    let b = build(&host, &registry, "panic_destroy");
    let inst = b.instantiate(rate()).expect("instantiate");

    drop(inst);
    drop(b);

    let errors: Vec<_> = recorder
        .all()
        .into_iter()
        .filter(|e| e.1 == LogLevel::Error)
        .collect();
    assert!(errors.iter().any(|e| e.2.contains("destroy")), "{errors:?}");
}

// ------------------------------------------------------------------------------------ lifecycle rows

#[test]
fn unload_ordering_is_instances_then_behaviors_then_shutdown() {
    let (host, registry, recorder) = fixture("lifecycle", &[]);
    let b = build(&host, &registry, "lifecycle");
    let inst = b.instantiate(rate()).expect("instantiate");

    drop(inst);
    drop(b);
    drop(registry);
    drop(host);

    let got: Vec<String> = recorder.all().into_iter().map(|e| e.2).collect();
    let pos = |what: &str| {
        got.iter()
            .position(|m| m == what)
            .unwrap_or_else(|| panic!("{what} in {got:?}"))
    };
    assert!(pos("create_behavior") < pos("create_instance"));
    assert!(pos("create_instance") < pos("destroy_instance"));
    assert!(pos("destroy_instance") < pos("destroy_behavior"));
    assert!(pos("destroy_behavior") < pos("shutdown"));
}

#[test]
fn the_library_stays_loaded_while_a_handle_lives() {
    let (host, registry, _) = fixture("handle-lives", &[]);
    let b = build(&host, &registry, "ok");
    let clone = registry.clone();
    let factory = clone
        .behavior(&umbra_core::ComponentId::new("test.behavior.ok").expect("id"))
        .expect("the factory of the registry clone")
        .clone();
    drop(registry);
    drop(host);
    drop(clone);

    // The behavior and the factory still hold the image: the library is mapped, ticking works.
    let mut ents = vec![Ent::new(1, &b, 1)];
    tick_batched(&b.batch().expect("batch"), &mut ents, 0);

    assert_eq!(
        ents[0].status,
        Some(Status::Running),
        "a handle ticks after host and registry dropped (a fault would be Failure)"
    );
    drop(factory);
}

/// ADR 0021 (step-4 re-review, deferred to WI-10): a registry clone alone keeps the library mapped.
/// The behavior, the host and the original registry are gone; only the clone's factory holds the
/// image. When the clone goes too, the last release runs `shutdown` and closes the library, and it is
/// unmapped (the SDK registers no thread-local destructor, ASM-127). Linux only: `/proc/self/maps`.
#[cfg(target_os = "linux")]
#[test]
fn a_registry_clone_alone_keeps_the_library_mapped() {
    let mapped = |marker: &str| {
        std::fs::read_to_string("/proc/self/maps")
            .expect("maps")
            .contains(marker)
    };
    let variant = "clone-keeps-mapped";
    let marker = format!("lib{variant}-");
    let (host, registry, _) = fixture(variant, &[]);
    let b = build(&host, &registry, "ok");
    let clone = registry.clone();

    drop(b);
    drop(registry);
    drop(host);

    assert!(
        mapped(&marker),
        "the registry clone keeps the library mapped"
    );
    drop(clone);
    assert!(!mapped(&marker), "the last release closes the library");
}

// ------------------------------------------------------------------------------------ optional rows

/// A plugin built with `panic = "abort"` aborts the process on a panic (documented in the plugin guide):
/// run as a subprocess of this test binary so the abort does not take the test run down.
#[test]
fn a_plugin_built_with_panic_abort_aborts_the_process() {
    let exe = std::env::current_exe().expect("exe");

    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "panic_abort_child",
            "--include-ignored",
            "--nocapture",
        ])
        .env("UMBRA_PANIC_ABORT_CHILD", "1")
        .status()
        .expect("spawn the child");

    assert!(
        !status.success(),
        "the child must not exit normally: {status:?}"
    );
    #[cfg(unix)]
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(6),
        "terminated by SIGABRT, not another signal or an exit code: {status:?}"
    );
    #[cfg(windows)]
    assert!(
        // The CRT abort exit code, or STATUS_STACK_BUFFER_OVERRUN (fast fail).
        matches!(status.code(), Some(3 | -1_073_740_791)),
        "abort on Windows: {status:?}"
    );
}

/// The child of the test above; a no-op unless the parent set the variable, so it is not ignored.
#[test]
fn panic_abort_child() {
    if std::env::var_os("UMBRA_PANIC_ABORT_CHILD").is_none() {
        return;
    }
    // The panic strategy is a profile setting, not a feature: build the fixture with the profile
    // override.
    let dir = package_dir_with(
        "panic-abort",
        &[],
        &["--config", "profile.dev.panic=\"abort\""],
    );
    let mut host = PluginHost::new(HostOptions::default());
    for pkg in PluginHost::discover(&[dir]).expect("discover") {
        host.load(&pkg).expect("load");
    }
    let mut registry = Registry::new();
    host.register(&mut registry).expect("register");
    let b = build(&host, &registry, "panic_tick");
    let mut ents = vec![Ent::new(1, &b, 1)];
    tick_batched(&b.batch().expect("batch"), &mut ents, 0);
    panic!("unreachable: the plugin's panic must abort the process");
}

/// "Exactly one exported symbol" (REQ-P-1), where `nm` exists. Linker-provided text symbols are
/// ignored; everything else the library defines in its dynamic symbol table must be the entry.
#[cfg(unix)]
#[test]
fn the_library_exports_exactly_one_umbra_symbol() {
    const LINKER: [&str; 7] = [
        "_init",
        "_fini",
        "__cxa_finalize",
        "__gmon_start__",
        "__bss_start",
        "_edata",
        "_end",
    ];
    let dir = package_dir("symbols", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let Ok(out) = std::process::Command::new("nm")
        .arg("-D")
        .arg("--defined-only")
        .arg(&pkgs[0].library)
        .output()
    else {
        eprintln!("SKIPPED the_library_exports_exactly_one_umbra_symbol: `nm` is not installed");
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut exported: Vec<&str> = text
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (_addr, kind, name) = (f.next()?, f.next()?, f.next()?);
            (matches!(kind, "T" | "t" | "W" | "w" | "i" | "D" | "B" | "R")
                && !LINKER.contains(&name))
            .then_some(name)
        })
        .collect();
    exported.sort_unstable();

    assert_eq!(exported, ["umbra_plugin_entry"], "{text}");
}

// ------------------------------------------------------------- network rows (M6 WI-12, ASM-407)
//
// The network capabilities of the fixture (feature `network`, plugin `test.plugin.fixture` at ABI
// 0.1) and of the second plugin image (feature `network_peer`, `test.plugin.net_peer`), built with
// `umbra-sdk` (see `umbra-test-plugin/src/network.rs`). The raw-ABI rows the SDK cannot produce
// (unknown kind, non-UTF-8 string, more than `max` pushes, a stale sink token) are in
// `network_bridge.rs`.

use umbra_core::PluginFaultPolicy;
use umbra_core::net::{
    AdapterStart, NetEntityState, NetEntityType, NetError, NetKey, NetMessage, NetworkAdapter,
    NetworkAdapterConfig, configure_input,
};
use umbra_core::{BuildError, ComponentId, ComponentKind, SimTime, TickRate};

/// The features of the fixture with network capabilities, and of the peer image.
const NET: &[&str] = &["network"];
const PEER: &[&str] = &["network_peer"];

/// A plugin directory with the fixture built with `features` as package `variant` and a manifest
/// for plugin `id` 0.1.0 declaring ABI 0.1 (the required-minor rule: a network adapter needs 0.1).
fn net_package(variant: &str, features: &[&str], id: &str) -> PathBuf {
    let lib = support::plugins::shared::library("umbra-test-plugin", features);
    let dir = support::plugins::shared::plugins_dir(&[(variant, &lib)]);
    std::fs::write(
        dir.join(variant).join("plugin.toml"),
        format!("schema_version = 1\nid = \"{id}\"\nversion = \"0.1.0\"\nabi = \"0.1\"\n"),
    )
    .expect("manifest");
    dir
}

/// A run-mode host with every package of `dirs` loaded and registered under `policy`.
fn net_host(dirs: &[PathBuf], policy: PluginFaultPolicy) -> (PluginHost, Registry, Arc<Recorder>) {
    let (mut host, recorder) = logging_host();
    for pkg in PluginHost::discover(dirs).expect("discover") {
        host.load(&pkg).expect("load");
    }
    host.set_fault_policy(policy);
    let mut registry = Registry::new();
    host.register(&mut registry).expect("register");
    (host, registry, recorder)
}

/// The fixture with its network capabilities in a new host under `policy`.
fn net_fixture(variant: &str, policy: PluginFaultPolicy) -> (PluginHost, Registry, Arc<Recorder>) {
    net_host(&[net_package(variant, NET, "test.plugin.fixture")], policy)
}

/// Builds adapter `name` of capability `test.network.<cap>` with the JSON `params`, inside the
/// host's build scope.
fn net_build(
    host: &PluginHost,
    registry: &Registry,
    cap: &str,
    name: &str,
    params: &str,
) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
    let id = ComponentId::new(&format!("test.network.{cap}")).expect("valid id");
    let factory = registry.network_adapter(&id).expect("registered");
    let params: umbra_core::ParamValue = serde_json::from_str(params).expect("params JSON");
    let _scope = host.build_scope();
    factory.build(&configure_input(name, &params, []))
}

fn at(tick: u64) -> SimTime {
    SimTime::at_tick(TickRate::new(30).expect("30 Hz"), tick)
}

fn net_start(config: &dyn NetworkAdapterConfig) -> Box<dyn NetworkAdapter> {
    config.start(&AdapterStart { now: at(0) }).expect("start")
}

fn net_poll(adapter: &mut dyn NetworkAdapter, tick: u64) -> Result<Vec<NetMessage>, NetError> {
    let mut into = Vec::new();
    adapter.poll(at(tick), 64, &mut into).map(|()| into)
}

/// An owned entity state with key `key` as the manager publishes it.
fn owned_state(key: u64) -> NetMessage {
    NetMessage::EntityState(NetEntityState {
        key: NetKey(key),
        entity_type: Some(NetEntityType::new("tank")),
        force: Some(umbra_core::Force::new("blue").expect("force")),
        name: Some(Arc::from("a-tank")),
        position: umbra_core::Ecef(umbra_core::Vec3 {
            x: 3_900_000.0,
            y: 300_000.0,
            z: 5_000_000.0,
        }),
        velocity: umbra_core::Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        acceleration: umbra_core::Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        angular_velocity: umbra_core::Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        orientation: umbra_core::WorldEuler {
            yaw: umbra_core::Radians(0.5),
            pitch: umbra_core::Radians(0.0),
            roll: umbra_core::Radians(0.0),
        },
        dead_reckoning: umbra_core::dead_reckoning::DeadReckoning::Velocity,
    })
}

/// The plugin's log lines (messages only), in order.
fn lines(recorder: &Recorder) -> Vec<String> {
    recorder.all().into_iter().map(|(_, _, m)| m).collect()
}

#[test]
fn the_network_fixture_loads_at_abi_0_1_and_registers_its_network_adapters() {
    let (host, registry, _) = net_fixture("net-load", PluginFaultPolicy::Abort);

    let info = &host.plugins()[0];
    assert_eq!(
        info.abi,
        (0, 1),
        "a network adapter makes the descriptor declare 0.1"
    );
    let net: Vec<&str> = info
        .capabilities
        .iter()
        .filter(|c| c.kind == ComponentKind::NetworkAdapter)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(
        net,
        [
            "test.network.net_ok",
            "test.network.net_panics_in_poll",
            "test.network.net_bad_message",
            "test.network.net_blocks_join",
        ]
    );
    for id in &net {
        let id = ComponentId::new(id).expect("valid id");
        assert!(
            registry.network_adapter(&id).is_some(),
            "{id} is registered"
        );
    }
    assert!(
        info.capabilities
            .iter()
            .any(|c| c.kind == ComponentKind::Behavior && c.id.as_str() == "test.behavior.ok"),
        "the behaviors stay"
    );
}

#[test]
fn the_network_fixture_under_a_0_0_manifest_is_a_manifest_mismatch() {
    let err = load_error("net-manifest-0-0", NET);

    assert!(
        matches!(err, PluginError::ManifestMismatch { .. }),
        "{err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("0.0") && msg.contains("0.1"),
        "names both ABIs: {msg}"
    );
}

#[test]
fn net_ok_mirrors_and_stop_and_destroy_run_once_before_shutdown() {
    let (host, registry, recorder) = net_fixture("net-ok", PluginFaultPolicy::Abort);
    let config =
        net_build(&host, &registry, "net_ok", "left", r#"{"key_base":500}"#).expect("build");
    assert_eq!(config.key_space().base, 500);
    assert_eq!(config.key_space().count, 100);
    let mut adapter = net_start(config.as_ref());

    adapter
        .publish(at(1), &[owned_state(501)])
        .expect("publish");
    let got = net_poll(adapter.as_mut(), 2).expect("poll");

    let [NetMessage::EntityState(s)] = &got[..] else {
        panic!("one mirrored state: {got:?}")
    };
    assert_eq!(
        s.key,
        NetKey(100_501),
        "moved out of the own key space by remote_offset"
    );
    let NetMessage::EntityState(sent) = owned_state(501) else {
        unreachable!()
    };
    assert_eq!(
        NetEntityState {
            key: NetKey(501),
            ..s.clone()
        },
        sent
    );
    adapter.stop();
    adapter.stop();
    drop(adapter);
    drop(config);
    drop(registry);
    drop(host);
    let log = lines(&recorder);
    let lifecycle: Vec<&str> = log
        .iter()
        .map(String::as_str)
        .filter(|l| l.starts_with("net ") || *l == "shutdown")
        .collect();
    assert_eq!(
        lifecycle,
        [
            "net configure left",
            "net start left",
            "net stop left",
            "net destroy left",
            "shutdown"
        ],
        "stop and destroy once each, before the image shuts down"
    );
}

#[test]
fn net_blocks_join_joins_its_io_thread_in_stop_before_destroy() {
    let (host, registry, recorder) = net_fixture("net-join", PluginFaultPolicy::Abort);
    let config = net_build(&host, &registry, "net_blocks_join", "io", "{}").expect("build");
    let adapter = net_start(config.as_ref());

    // Dropping the started adapter without `stop`: the bridge stops it, the SDK calls the
    // adapter's `stop`, which releases and joins the thread.
    drop(adapter);
    drop(config);
    drop(registry);
    drop(host);

    let log = lines(&recorder);
    let pos = |line: &str| {
        log.iter()
            .position(|l| l == line)
            .unwrap_or_else(|| panic!("no {line:?} in {log:?}"))
    };
    assert!(pos("net io thread joined io") < pos("net stop io"));
    assert!(pos("net stop io") < pos("net destroy io"));
    assert!(pos("net destroy io") < pos("shutdown"));
}

#[test]
fn net_panics_in_poll_faults_the_capability_and_aborts_once() {
    let (host, registry, _) = net_fixture("net-poll-panic", PluginFaultPolicy::Abort);
    let config = net_build(
        &host,
        &registry,
        "net_panics_in_poll",
        "p",
        r#"{"panic_at_tick":3}"#,
    )
    .expect("build");
    let mut adapter = net_start(config.as_ref());
    adapter
        .publish(at(1), &[owned_state(10_001)])
        .expect("publish");
    assert!(
        net_poll(adapter.as_mut(), 2).is_ok(),
        "healthy before tick 3"
    );

    let result = net_poll(adapter.as_mut(), 3);

    assert!(
        matches!(&result, Err(NetError::Faulted(m)) if m.contains("test.network.net_panics_in_poll")),
        "{result:?}"
    );
    let faults = host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (
            faults[0].plugin.as_str(),
            faults[0].capability.as_str(),
            faults[0].call
        ),
        (
            "test.plugin.fixture",
            "test.network.net_panics_in_poll",
            "poll"
        )
    );
    assert!(
        faults[0]
            .message
            .contains("poll panics on purpose at tick 3"),
        "{}",
        faults[0].message
    );
    let abort = host.take_abort().expect("abort pending");
    assert_eq!(abort.capability.as_str(), "test.network.net_panics_in_poll");
    assert!(host.take_abort().is_none(), "handed out once");
    assert!(
        adapter.publish(at(4), &[]).is_err(),
        "no call after the fault"
    );
}

#[test]
fn net_bad_message_is_refused_by_the_host_and_faults_the_capability() {
    let (host, registry, _) = net_fixture("net-bad-message", PluginFaultPolicy::DisableCapability);
    let config = net_build(&host, &registry, "net_bad_message", "bad", "{}").expect("build");
    let mut adapter = net_start(config.as_ref());

    let result = net_poll(adapter.as_mut(), 1);

    assert!(matches!(result, Err(NetError::Faulted(_))), "{result:?}");
    let faults = host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (faults[0].capability.as_str(), faults[0].call),
        ("test.network.net_bad_message", "poll")
    );
    assert!(
        faults[0].message.contains("force"),
        "names the field: {}",
        faults[0].message
    );
    assert!(
        host.take_abort().is_none(),
        "no abort under disable_capability"
    );
}

#[test]
fn configure_with_bad_params_is_a_build_error_naming_the_capability_and_not_a_fault() {
    let (host, registry, _) = net_fixture("net-bad-params", PluginFaultPolicy::Abort);

    let err = net_build(&host, &registry, "net_ok", "left", r#"{"remote_offset":7}"#)
        .err()
        .expect("bad params");

    assert!(
        err.message.contains("test.network.net_ok"),
        "{}",
        err.message
    );
    assert!(err.message.contains("remote_offset"), "{}", err.message);
    assert!(host.faults().is_empty(), "a configure error is not a fault");
    assert!(
        net_build(&host, &registry, "net_ok", "left", "{}").is_ok(),
        "the capability still builds"
    );
}

#[test]
fn under_disable_capability_a_network_fault_leaves_the_other_capabilities_working() {
    let (host, registry, _) = net_fixture("net-disable", PluginFaultPolicy::DisableCapability);
    let healthy = net_build(&host, &registry, "net_ok", "ok", "{}").expect("build");
    let faulty = net_build(
        &host,
        &registry,
        "net_panics_in_poll",
        "p",
        r#"{"key_base":20000}"#,
    )
    .expect("build");
    let (mut a, mut b) = (net_start(healthy.as_ref()), net_start(faulty.as_ref()));

    assert!(net_poll(b.as_mut(), 0).is_err());

    assert!(host.take_abort().is_none());
    let info = &host.plugins()[0];
    let faulted: Vec<&str> = info
        .capabilities
        .iter()
        .filter(|c| c.faulted)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(faulted, ["test.network.net_panics_in_poll"]);
    a.publish(at(1), &[owned_state(10_001)])
        .expect("the other capability publishes");
    assert_eq!(net_poll(a.as_mut(), 1).expect("and polls").len(), 1);
}

#[test]
fn two_plugin_images_with_network_adapters_run_side_by_side_and_fault_independently() {
    let fixture = net_package("net-two-a", NET, "test.plugin.fixture");
    let peer = net_package("net-two-b", PEER, "test.plugin.net_peer");
    let (host, registry, recorder) =
        net_host(&[fixture, peer], PluginFaultPolicy::DisableCapability);
    let ids: Vec<&str> = host.plugins().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"test.plugin.fixture") && ids.contains(&"test.plugin.net_peer"));

    let left = net_build(&host, &registry, "net_ok", "left", r#"{"key_base":1000}"#).expect("a");
    let right = net_build(&host, &registry, "peer_ok", "right", r#"{"key_base":2000}"#).expect("b");
    let broken = net_build(
        &host,
        &registry,
        "peer_panics_in_poll",
        "broken",
        r#"{"key_base":3000}"#,
    )
    .expect("c");
    let (mut l, mut r, mut x) = (
        net_start(left.as_ref()),
        net_start(right.as_ref()),
        net_start(broken.as_ref()),
    );
    l.publish(at(1), &[owned_state(1001)])
        .expect("left publishes");
    r.publish(at(1), &[owned_state(2001)])
        .expect("right publishes");

    assert!(
        net_poll(x.as_mut(), 1).is_err(),
        "the peer's panicking capability faults"
    );
    let got_l = net_poll(l.as_mut(), 1).expect("left still polls");
    let got_r = net_poll(r.as_mut(), 1).expect("right, same image as the faulty one, still polls");

    let keys = |m: &[NetMessage]| -> Vec<u64> {
        m.iter()
            .filter_map(|m| match m {
                NetMessage::EntityState(s) => Some(s.key.0),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        keys(&got_l),
        [101_001],
        "each adapter mirrors only its own messages"
    );
    assert_eq!(keys(&got_r), [102_001]);
    let faults = host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(
        (faults[0].plugin.as_str(), faults[0].capability.as_str()),
        ("test.plugin.net_peer", "test.network.peer_panics_in_poll")
    );
    drop((l, r, x, left, right, broken, registry, host));
    let by_plugin = |plugin: &str| -> Vec<String> {
        recorder
            .all()
            .into_iter()
            .filter(|(p, _, m)| p == plugin && (m.starts_with("net destroy") || m == "shutdown"))
            .map(|(_, _, m)| m)
            .collect()
    };
    assert_eq!(
        by_plugin("test.plugin.fixture"),
        ["net destroy left", "shutdown"]
    );
    let peer_log = by_plugin("test.plugin.net_peer");
    assert_eq!(
        peer_log.last().map(String::as_str),
        Some("shutdown"),
        "{peer_log:?}"
    );
    assert_eq!(
        peer_log.len(),
        3,
        "both peer adapters destroyed before its shutdown: {peer_log:?}"
    );
}

// ----------------------------------------------------- ABI 0.2 kinds rows (M7 WI-2, ADR 0050, ASM-668)
//
// The fixture is `umbra-test-plugin` built with the feature `kinds` (plugin `test.plugin.kinds`, ABI
// 0.2; capabilities in its `kinds` module; until WI-9 the transitional `kinds_motion_sensor` build,
// ASM-718), loaded through the shared build helper. Rows that a safe
// SDK plugin cannot produce (a stale or foreign service token, an unset condition result) are in the
// raw-ABI files written with WI-8/9. Ignored until the bridge of their kind exists (and WI-10, the SDK
// side): motion models and sensors WI-8, conditions and actions WI-9.

mod kinds_rows {
    use super::support::kinds::{
        Ctx, Env, assert_one_fault, board, check, env, env_with, flat, ids, instance, sense,
        state_at, step, tick, world,
    };
    use umbra_core::{
        BbType, BbValue, Blackboard, ComponentKind, Intent, PluginFaultPolicy, Status, Vec3,
    };

    /// The ids of the faulted capabilities of the one loaded plugin.
    fn faulted_ids(env: &Env) -> Vec<String> {
        env.host.plugins()[0]
            .capabilities
            .iter()
            .filter(|c| c.faulted)
            .map(|c| c.id.as_str().to_owned())
            .collect()
    }

    /// The entity keeps position and orientation with zero velocity (ASM-614).
    fn assert_held(start: &umbra_core::KinematicState, next: &umbra_core::KinematicState) {
        assert_eq!(next.position, start.position);
        assert_eq!(next.orientation, start.orientation);
        assert_eq!(next.velocity, Vec3::ZERO);
    }

    #[test]
    #[ignore = "M7 WI-9"]
    fn the_kinds_fixture_loads_at_abi_0_2_and_registers_every_new_kind() {
        let env = env();

        let info = &env.host.plugins()[0];

        assert_eq!(info.id.as_str(), "test.plugin.kinds");
        assert_eq!(info.abi, (0, 2));
        let of_kind = |kind: ComponentKind| -> Vec<&str> {
            info.capabilities
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| c.id.as_str())
                .collect()
        };
        assert_eq!(
            of_kind(ComponentKind::MotionModel),
            [
                "test.motion.coast",
                "test.motion.terrain_echo",
                "test.motion.non_unit",
                "test.motion.nan",
                "test.motion.panic_step"
            ]
        );
        assert_eq!(of_kind(ComponentKind::Sensor).len(), 5);
        assert_eq!(of_kind(ComponentKind::Condition).len(), 7);
        assert_eq!(of_kind(ComponentKind::Action).len(), 5);
        assert_eq!(env.host.faults(), Vec::new());
    }

    macro_rules! motion_fault {
        ($name:ident, $capability:literal, $needle:literal) => {
            #[test]
            fn $name() {
                let env = env();
                let model = env
                    .motion(concat!("test.motion.", $capability), "{}")
                    .expect("build");
                let start = state_at(4.0, 50.0, 12.0);

                let next = step(model.as_ref(), &start, &[], &flat());

                assert_held(&start, &next);
                assert_one_fault(&env, concat!("test.motion.", $capability), "step", $needle);
                let abort = env.host.take_abort().expect("abort pending");
                assert_eq!(
                    abort.capability.as_str(),
                    concat!("test.motion.", $capability)
                );
                assert!(env.host.take_abort().is_none(), "handed out once");
            }
        };
    }

    motion_fault!(
        a_motion_model_returning_a_non_unit_orientation_holds_the_entity_and_faults,
        "non_unit",
        "unit"
    );
    motion_fault!(
        a_motion_model_returning_a_nan_position_holds_the_entity_and_faults,
        "nan",
        "finite"
    );
    motion_fault!(
        a_panic_in_motion_step_holds_the_entity_and_faults,
        "panic_step",
        "panic"
    );

    /// ADR 0021: under `disable_capability` the run goes on and the other capabilities keep working.
    #[test]
    fn under_disable_capability_a_faulty_motion_model_is_marked_faulted_and_another_still_steps() {
        let env = env_with(PluginFaultPolicy::DisableCapability);
        let bad = env.motion("test.motion.nan", "{}").expect("build");
        let good = env.motion("test.motion.coast", "{}").expect("build");
        let start = state_at(4.0, 50.0, 12.0);

        let held = step(bad.as_ref(), &start, &[], &flat());
        let moved = step(good.as_ref(), &start, &[], &flat());

        assert_held(&start, &held);
        assert_ne!(moved.position, start.position, "the healthy model moved");
        assert!(
            env.host.take_abort().is_none(),
            "no abort under disable_capability"
        );
        assert_eq!(faulted_ids(&env), ["test.motion.nan"]);
        let again = step(bad.as_ref(), &start, &[], &flat());
        assert_held(&start, &again);
        assert_eq!(
            env.host.faults().len(),
            1,
            "a disabled capability faults only once"
        );
    }

    fn assert_no_contacts_and_one_sense_fault(env: &Env, capability: &str, needle: &str) {
        let sensor = env.sensor(capability, "{}").expect("build");

        let (contacts, _) = sense(sensor.as_ref(), &world(), &flat());

        assert_eq!(contacts, Vec::new());
        assert_one_fault(env, capability, "sense", needle);
    }

    #[test]
    fn a_contact_naming_a_non_candidate_is_refused_and_faults_the_sensor() {
        assert_no_contacts_and_one_sense_fault(&env(), "test.sensor.unknown_contact", "candidate");
    }

    #[test]
    fn a_panic_in_sense_yields_no_contacts_and_faults_the_sensor() {
        assert_no_contacts_and_one_sense_fault(&env(), "test.sensor.panic_sense", "panic");
    }

    #[test]
    fn under_disable_capability_a_faulty_sensor_is_marked_faulted_and_another_still_contacts() {
        let env = env_with(PluginFaultPolicy::DisableCapability);
        let bad = env.sensor("test.sensor.panic_sense", "{}").expect("build");
        let good = env.sensor("test.sensor.all", "{}").expect("build");
        let world = world();

        let (none, _) = sense(bad.as_ref(), &world, &flat());
        let (all, _) = sense(good.as_ref(), &world, &flat());

        assert_eq!(none, Vec::new());
        assert_eq!(ids(&all), [2, 3, 4]);
        assert!(env.host.take_abort().is_none());
        assert_eq!(env.host.faults().len(), 1);
        assert_eq!(faulted_ids(&env), ["test.sensor.panic_sense"]);
    }

    /// REQ-B-15, ADR 0050: a refused service call faults the capability and the condition answers
    /// false.
    macro_rules! condition_fault {
        ($name:ident, $capability:literal, $params:literal, $keys:expr, $needle:literal) => {
            #[test]
            #[ignore = "M7 WI-9"]
            fn $name() {
                let env = env();
                let condition = env
                    .condition(concat!("test.cond.", $capability), $params)
                    .expect("build");
                let mut bb = board($keys);

                let answer = check(condition.as_ref(), &mut Ctx::new(1), &mut bb, 1);

                assert!(!answer);
                assert_one_fault(&env, concat!("test.cond.", $capability), "check", $needle);
            }
        };
    }

    condition_fault!(
        a_read_of_an_undeclared_key_faults_the_condition_naming_the_key,
        "undeclared",
        "{}",
        &[("secret", Some(true))],
        "secret"
    );
    condition_fault!(
        a_write_of_the_wrong_type_faults_the_condition,
        "wrong_type",
        r#"{"key":"seen"}"#,
        &[("seen", None)],
        "type"
    );
    condition_fault!(
        a_panic_in_check_faults_the_condition,
        "panic_check",
        "{}",
        &[],
        "panic"
    );

    #[test]
    #[ignore = "M7 WI-9"]
    fn a_write_of_a_non_finite_number_faults_the_condition_and_leaves_the_key_unchanged() {
        let env = env();
        let condition = env
            .condition("test.cond.non_finite", r#"{"key":"level"}"#)
            .expect("build");
        let mut bb = Blackboard::default();
        bb.declare("level", BbType::Number);
        bb.set("level", BbValue::Number(3.0)).expect("declared");

        let answer = check(condition.as_ref(), &mut Ctx::new(1), &mut bb, 1);

        assert!(!answer);
        assert_eq!(bb.get("level"), Some(&BbValue::Number(3.0)));
        assert_one_fault(&env, "test.cond.non_finite", "check", "finite");
    }

    #[test]
    #[ignore = "M7 WI-9"]
    fn a_refused_write_leaves_the_blackboard_key_unchanged() {
        let env = env();
        let condition = env
            .condition("test.cond.wrong_type", r#"{"key":"seen"}"#)
            .expect("build");
        let mut bb = board(&[("seen", None)]);

        let _ = check(condition.as_ref(), &mut Ctx::new(1), &mut bb, 1);

        assert_eq!(bb.get("seen"), None);
    }

    #[test]
    #[ignore = "M7 WI-9"]
    fn a_write_to_a_read_only_key_fails_the_action_and_faults_it() {
        let env = env();
        let mut action = instance(&env, "test.action.read_only_write", r#"{"key":"armed"}"#);
        let mut bb = board(&[("armed", Some(false))]);

        let (status, intents) = tick(action.as_mut(), &mut Ctx::new(1), &mut bb, 1);

        assert_eq!((status, intents), (Status::Failure, Vec::new()));
        assert_eq!(bb.get("armed"), Some(&BbValue::Bool(false)));
        assert_one_fault(&env, "test.action.read_only_write", "tick", "armed");
    }

    /// ADR 0050: the intents of a faulting call are discarded.
    #[test]
    #[ignore = "M7 WI-9"]
    fn a_panic_after_an_intent_fails_the_action_and_discards_the_intent() {
        let env = env();
        let mut action = instance(&env, "test.action.panic_tick", "{}");

        let (status, intents) = tick(
            action.as_mut(),
            &mut Ctx::new(1),
            &mut Blackboard::default(),
            1,
        );

        assert_eq!((status, intents), (Status::Failure, Vec::<Intent>::new()));
        assert_one_fault(&env, "test.action.panic_tick", "tick", "panic");
    }

    #[test]
    #[ignore = "M7 WI-9"]
    fn under_disable_capability_a_faulty_action_is_marked_faulted_and_another_still_runs() {
        let env = env_with(PluginFaultPolicy::DisableCapability);
        let mut bad = instance(&env, "test.action.panic_tick", "{}");
        let mut good = instance(&env, "test.action.emit", "{}");
        let mut ctx = Ctx::new(1);

        let (failed, _) = tick(bad.as_mut(), &mut ctx, &mut Blackboard::default(), 1);
        let (done, _) = tick(good.as_mut(), &mut ctx, &mut Blackboard::default(), 1);

        assert_eq!((failed, done), (Status::Failure, Status::Success));
        assert!(env.host.take_abort().is_none());
        assert_eq!(env.host.faults().len(), 1);
        assert_eq!(faulted_ids(&env), ["test.action.panic_tick"]);
    }
}
