//! AT-4, gate 13: the plugin failure matrix against a REAL library, `umbra-test-plugin` built with
//! `umbra-sdk` (`crates/umbra-test-plugin`, WI-10). Requirements: REQ-P-1..10, REQ-SDK-3, section 15 plugin
//! tests, M4 acceptance 2, R-B4, R-B5, R-M2, R-M9, RR-B1, RR-M1..M3.
//!
//! The rows that need no real library (forged descriptors, raw-ABI misbehavior, image lifecycle states)
//! are in `descriptor.rs`, `manifest.rs`, `image.rs`, `faults.rs`, `tokens.rs`, `contention.rs`.
//!
//! Step 4 of M4. Everything is ignored with WI-10: the fixture crate and the build helper do not exist.
//! The fixture contract the tests assume beyond the plan's capability list (plan gaps, reported):
//! cargo features `entry_panics` and `null_entry` (the plan lists only `no_entry`, `init_fails`,
//! `init_panics`, `shutdown_panics`, `panic_abort_profile`); the package is `test.plugin.fixture` 0.1.0
//! with a `plugin.toml`; `test.behavior.lifecycle` logs exactly `create_behavior`, `create_instance`,
//! `destroy_instance`, `destroy_behavior` and the plugin's `shutdown` logs `shutdown`; every faulty
//! capability's fault shows in `PluginHost::faults()` with the call named in its test.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[macro_use]
mod support;

use std::sync::Arc;
use std::thread;

use support::bridge::{Ent, rate, tick_batched};
use support::plugins::package_dir;
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

fn load_error(variant: &str, features: &[&str]) -> PluginError {
    let mut host = PluginHost::new(HostOptions::default());
    let pkgs = PluginHost::discover(&[package_dir(variant, features)]).expect("discover");
    host.load(&pkgs[0]).expect_err("the load must fail")
}

// ------------------------------------------------------------------------------------ load rows

#[test]
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
fn given_a_library_without_umbra_plugin_entry_then_missing_entry() {
    let err = load_error("no-entry", &["no_entry"]);

    assert!(matches!(err, PluginError::MissingEntry { .. }), "{err:?}");
    assert!(err.to_string().contains("umbra_plugin_entry"), "{err}");
}

#[test]
#[ignore = "WI-10"]
fn given_a_null_entry_symbol_then_missing_entry() {
    let err = load_error("null-entry", &["null_entry"]);

    assert!(matches!(err, PluginError::MissingEntry { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-10"]
fn given_an_entry_that_panics_then_null_descriptor() {
    let err = load_error("entry-panics", &["entry_panics"]);

    assert!(matches!(err, PluginError::NullDescriptor { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-10"]
fn given_init_returns_an_error_then_init_error() {
    let err = load_error("init-fails", &["init_fails"]);

    assert!(matches!(err, PluginError::Init { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-10"]
fn given_init_panics_then_init_error_with_the_panic_message() {
    let err = load_error("init-panics", &["init_panics"]);

    assert!(matches!(err, PluginError::Init { .. }), "{err:?}");
    assert!(err.to_string().contains("panic"), "{err}");
}

#[test]
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
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
}

#[test]
#[ignore = "WI-10"]
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
}

#[test]
#[ignore = "WI-10"]
fn given_a_manifest_with_another_abi_then_rejected_without_opening_the_library() {
    let dir = package_dir("manifest-abi", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let manifest = pkgs[0].manifest.clone().expect("plugin.toml");
    let text = std::fs::read_to_string(&manifest).expect("read");
    std::fs::write(&manifest, text.replace("abi = \"0.0\"", "abi = \"1.0\"")).expect("write");
    let mut host = PluginHost::new(HostOptions::default());

    let err = host.load(&pkgs[0]).expect_err("mismatch");

    assert!(matches!(err, PluginError::AbiMismatch { .. }), "{err:?}");
}

#[test]
#[ignore = "WI-10"]
fn given_the_same_plugin_id_from_two_images_then_duplicate() {
    let first = package_dir("twin-a", &[]);
    let second = package_dir("twin-b", &[]);
    let mut host = PluginHost::new(HostOptions::default());
    for dir in [first, second.clone()] {
        let pkgs = PluginHost::discover(&[dir]).expect("discover");
        if let Err(err) = host.load(&pkgs[0]) {
            assert!(matches!(err, PluginError::Duplicate { .. }), "{err:?}");
            assert!(err.to_string().contains("test.plugin.fixture"), "{err}");
            return;
        }
    }
    panic!("the second image with the same plugin id must be a duplicate");
}

#[cfg(unix)]
#[test]
#[ignore = "WI-10"]
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
}

#[test]
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
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

/// RR-M1 on a real library: loads racing with last releases never fail or crash. The deterministic
/// version of the closing wait is `image.rs::load_waits_while_image_is_closing` (gated `shutdown`).
#[test]
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
fn panic_in_configure_is_a_build_error() {
    let (host, registry, _) = fixture("calls", &[]);

    let err = build_behavior(&host, &registry, "test.behavior.panic_configure", "{}")
        .err()
        .expect("build error");

    assert!(err.message.to_lowercase().contains("panic"), "{err}");
}

#[test]
#[ignore = "WI-10"]
fn panic_in_instantiate_is_an_instantiate_error() {
    let (host, registry, _) = fixture("calls", &[]);
    let b = build(&host, &registry, "panic_instantiate");

    let err = b.instantiate(rate()).err().expect("instantiate error");

    assert!(err.to_string().to_lowercase().contains("panic"), "{err}");
}

macro_rules! tick_fault {
    ($name:ident, $capability:expr) => {
        #[test]
        #[ignore = "WI-10"]
        fn $name() {
            let (host, registry, _) = fixture("calls", &[]);
            let b = build(&host, &registry, $capability);
            let mut ents = vec![Ent::new(1, &b, 1), Ent::new(2, &b, 1)];

            tick_batched(&b.batch().expect("batch"), &mut ents, 0);

            let faults = host.faults();
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert_eq!(faults[0].call, "tick_batch");
            assert_eq!(
                faults[0].capability.as_str(),
                concat!("test.behavior.", $capability)
            );
            assert!(
                ents.iter()
                    .all(|e| e.status == Some(Status::Failure) && e.buf.is_empty())
            );
        }
    };
}

tick_fault!(panic_in_tick_batch_is_a_fault, "panic_tick");
tick_fault!(status_left_unset_is_a_fault, "no_status");
tick_fault!(status_out_of_range_is_a_fault, "bad_status");
tick_fault!(invalid_intent_is_a_fault, "bad_intent");
tick_fault!(more_than_64_intents_is_a_fault, "intent_flood");
tick_fault!(intent_index_at_count_is_a_fault, "bad_index");
tick_fault!(rng_with_another_calls_token_is_a_fault, "stale_token");

macro_rules! method_fault {
    ($name:ident, $capability:expr, $call:expr, $act:expr) => {
        #[test]
        #[ignore = "WI-10"]
        fn $name() {
            let (host, registry, _) = fixture("calls", &[]);
            let b = build(&host, &registry, $capability);
            let mut inst = b.instantiate(rate()).expect("instantiate");

            let act: fn(&mut dyn BehaviorInstance) = $act;
            act(inst.as_mut());

            let faults = host.faults();
            assert_eq!(faults.len(), 1, "{faults:?}");
            assert_eq!(faults[0].call, $call);
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
    output_sink_called_twice_is_a_fault,
    "output_twice",
    "output",
    |i| {
        let _ = i.output("turns");
    }
);
method_fault!(
    invalid_output_value_is_a_fault,
    "bad_value",
    "output",
    |i| {
        let _ = i.output("turns");
    }
);
method_fault!(
    halt_returning_an_error_code_is_a_fault,
    "halt_error",
    "halt",
    |i| i.halt()
);

#[test]
#[ignore = "WI-10"]
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

#[test]
#[ignore = "WI-10"]
fn configure_ok_with_a_null_handle_is_a_build_error() {
    let (host, registry, _) = fixture("calls", &[]);

    let err = build_behavior(&host, &registry, "test.behavior.null_handle", "{}")
        .err()
        .expect("build error");

    assert!(err.message.contains("null"), "{err}");
}

#[test]
#[ignore = "WI-10"]
fn rng_with_another_items_token_draws_that_items_stream_and_does_not_fault() {
    let (host, registry, _) = fixture("calls", &[]);
    let b = build(&host, &registry, "foreign_rng");
    let mut ents = vec![Ent::new(1, &b, 1), Ent::new(2, &b, 1)];

    tick_batched(&b.batch().expect("batch"), &mut ents, 0);

    assert_eq!(
        host.faults(),
        Vec::new(),
        "a documented contract breach, defined behavior"
    );
}

// ------------------------------------------------------------------------------------ lifecycle rows

#[test]
#[ignore = "WI-10"]
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
#[ignore = "WI-10"]
fn the_library_stays_loaded_while_a_handle_lives() {
    let (host, registry, _) = fixture("handle-lives", &[]);
    let b = build(&host, &registry, "ok");
    let clone = registry.clone();
    drop(registry);
    drop(host);
    drop(clone);

    let mut ents = vec![Ent::new(1, &b, 1)];
    tick_batched(&b.batch().expect("batch"), &mut ents, 0);

    assert!(
        ents[0].status.is_some(),
        "ticking a handle after host and registry dropped"
    );
}

// ------------------------------------------------------------------------------------ optional rows

/// A plugin built with `panic = "abort"` aborts the process on a panic (documented in the plugin guide):
/// run as a subprocess of this test binary so the abort does not take the test run down.
#[test]
#[ignore = "WI-10"]
fn a_plugin_built_with_panic_abort_aborts_the_process() {
    let exe = std::env::current_exe().expect("exe");

    let status = std::process::Command::new(exe)
        .args(["--exact", "panic_abort_child", "--ignored", "--nocapture"])
        .env("UMBRA_PANIC_ABORT_CHILD", "1")
        .status()
        .expect("spawn the child");

    assert!(
        !status.success(),
        "the child must not exit normally: {status:?}"
    );
    assert_eq!(
        status.code(),
        None,
        "terminated by a signal (abort), not an exit code"
    );
}

/// The child of the test above; does nothing unless the parent set the variable.
#[test]
#[ignore = "WI-10 (subprocess child)"]
fn panic_abort_child() {
    if std::env::var_os("UMBRA_PANIC_ABORT_CHILD").is_none() {
        return;
    }
    let (host, registry, _) = fixture("panic-abort", &["panic_abort_profile"]);
    let b = build(&host, &registry, "panic_tick");
    let mut ents = vec![Ent::new(1, &b, 1)];
    tick_batched(&b.batch().expect("batch"), &mut ents, 0);
    panic!("unreachable: the plugin's panic must abort the process");
}

/// "Exactly one exported symbol" (REQ-P-1), where `nm` exists.
#[cfg(unix)]
#[test]
#[ignore = "WI-10"]
fn the_library_exports_exactly_one_umbra_symbol() {
    let dir = package_dir("symbols", &[]);
    let pkgs = PluginHost::discover(&[dir]).expect("discover");
    let Ok(out) = std::process::Command::new("nm")
        .arg("-D")
        .arg("--defined-only")
        .arg(&pkgs[0].library)
        .output()
    else {
        return; // no nm on this machine: not a failure
    };
    let text = String::from_utf8_lossy(&out.stdout);

    let umbra: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().last())
        .filter(|s| s.contains("umbra") || s.contains("plugin"))
        .collect();

    assert_eq!(umbra, ["umbra_plugin_entry"], "{text}");
}
