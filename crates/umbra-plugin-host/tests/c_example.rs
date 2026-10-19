//! AT-9, M4 acceptance 3: the C example `examples/plugins/c-circle/plugin.c` compiles against the generated
//! header and drives an entity (REQ-SDK-2, REQ-P-1, ADR 0023 item 3). Together with AT-2 (`c_header.rs`)
//! this is the evidence that a plugin can be written in C. Needs a C compiler (a gate prerequisite).
//!
//! Written in step 4 of M4, ignored until WI-14 added the example. Assumptions the plan left open
//! (confirmed by the example, ASM-148): the manifest is optional here (no version is named for
//! `example.plugin.c_circle`), `turn_rate_dps` is degrees per second and the reported heading is in radians
//! (`turn_rate_dps * tick / rate_hz`, converted), and the speed is reported as `SetSpeed(speed_mps)` every
//! tick.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[macro_use]
mod support;

use std::path::{Path, PathBuf};

use support::bridge::{Ent, rate, tick_batched};
use support::{build_behavior, lib_name, tmp_dir};
use umbra_core::{Intent, Provenance, Registry};
use umbra_plugin_host::{HostOptions, PluginHost};

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/c-circle/plugin.c")
}

fn include_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../umbra-plugin-api/include")
}

/// Compiles the example into a shared library with the strict header flags; `Err` is the compiler output.
fn compile(dir: &Path) -> Result<PathBuf, String> {
    let lib = dir.join(lib_name("c_circle"));
    support::plugins::compile_shared(&source(), &lib, Some(&include_dir()))?;
    Ok(lib)
}

/// A plugin directory holding one package with the freshly compiled example.
fn package(name: &str) -> PathBuf {
    let root = tmp_dir(name);
    let pkg = root.join("c-circle");
    std::fs::create_dir_all(&pkg).expect("package dir");
    compile(&pkg).unwrap_or_else(|e| panic!("the C example must compile:\n{e}"));
    root
}

fn loaded(name: &str) -> (PluginHost, Registry) {
    let mut host = PluginHost::new(HostOptions::default());
    for pkg in PluginHost::discover(&[package(name)]).expect("discover") {
        host.load(&pkg).expect("load the C example");
    }
    let mut registry = Registry::new();
    host.register(&mut registry).expect("register");
    (host, registry)
}

#[test]
fn c_example_compiles_with_strict_flags() {
    let dir = tmp_dir("c-example-compile");

    let result = compile(&dir);

    assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
}

#[test]
fn c_example_loads_and_registers() {
    let (host, registry) = loaded("c-example-load");

    let info = &host.plugins()[0];
    assert_eq!(info.id.as_str(), "example.plugin.c_circle");
    assert_eq!(
        info.capabilities[0].id.as_str(),
        "example.behavior.c_circle"
    );
    let id = umbra_core::ComponentId::new("example.behavior.c_circle").expect("id");
    assert_eq!(
        registry.info(&id).map(|i| i.source),
        Some(Provenance::Plugin)
    );
}

#[test]
fn c_example_drives_an_entity_on_a_circle() {
    let (host, registry) = loaded("c-example-circle");
    let b = build_behavior(
        &host,
        &registry,
        "example.behavior.c_circle",
        r#"{"speed_mps":12.0,"turn_rate_dps":30.0}"#,
    )
    .expect("build");
    let mut ents = vec![Ent::new(1, &b, 1)];
    let batch = b.batch().expect("batch");
    let (mut speeds, mut headings) = (Vec::new(), Vec::new());

    for tick in 0..90 {
        tick_batched(&batch, &mut ents, tick);
        for i in &ents[0].buf {
            match i {
                Intent::SetSpeed(s) => speeds.push(s.0),
                Intent::SetHeading(h) => headings.push(h.0),
                other => panic!("unexpected intent {other:?}"),
            }
        }
    }

    assert_eq!(host.faults(), Vec::new());
    assert_eq!(speeds.len(), 90);
    assert!(
        speeds.iter().all(|s| (*s - 12.0).abs() < 1e-12),
        "constant speed"
    );
    assert_eq!(headings.len(), 90);
    for (tick, h) in headings.iter().enumerate() {
        let expected = (30.0 * f64::from(u32::try_from(tick).expect("small")) / 30.0).to_radians();
        assert!(
            (h - expected).abs() < 1e-9,
            "tick {tick}: heading {h}, expected {expected}"
        );
    }
}

#[test]
fn c_example_without_halt_running_gets_halt() {
    let (host, registry) = loaded("c-example-halt");
    let b = build_behavior(
        &host,
        &registry,
        "example.behavior.c_circle",
        r#"{"speed_mps":1.0,"turn_rate_dps":10.0}"#,
    )
    .expect("build");
    let mut inst = b.instantiate(rate()).expect("instantiate");

    inst.halt_running();
    inst.halt();

    assert_eq!(
        host.faults(),
        Vec::new(),
        "the null halt_running entry falls back to halt (ADR 0020)"
    );
}
