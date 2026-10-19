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
//! Step 4 of M4. Everything that needs the fixture crate is ignored with WI-10; the rows that need only
//! a garbage file or a C library (open error, missing and null entry, same file twice, `keep_loaded`)
//! run from WI-8.
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
        status.code(),
        None,
        "terminated by a signal (abort), not an exit code"
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
