//! AT-16 (part, M6 WI-10): the plugin's declared minor is the ceiling for every struct it writes
//! during a call (ADR 0033, clarified 2026-10-21: structs written during a capability's calls carry the
//! capability's minor). The raw plugin of `support/raw.rs` writes its intents and values with the
//! header's `UMBRA_ABI_VERSION` (the host's minor); declaring 0.0 in its descriptor makes those structs
//! too new.
#![expect(unsafe_code, reason = "forged! expands forged entry functions")]

#[macro_use]
mod support;

use support::bridge::{Ent, Env, rate, tick_batched};
use support::raw;
use umbra_core::{BbValue, Status};
use umbra_plugin_api::{UMBRA_ABI_VERSION_MAJOR, UMBRA_ABI_VERSION_MINOR};
use umbra_plugin_host::HostOptions;

/// `major << 16 | minor` of this host's major.
const fn abi(minor: u32) -> u32 {
    (UMBRA_ABI_VERSION_MAJOR << 16) | minor
}

/// The raw plugin with every descriptor header declaring `minor`. A macro: each test needs its own
/// forged image (one call site per test, `support/forge.rs`).
macro_rules! raw_env_of_minor {
    ($minor:expr) => {{
        let mut p = raw::parts(true);
        p.desc.abi_version = abi($minor);
        for c in &mut p.caps {
            c.abi_version = abi($minor);
        }
        p.vtable.abi_version = abi($minor);
        p.link();
        Env::load(forged!(p), HostOptions::default())
    }};
}

#[test]
fn a_minor_0_plugin_pushing_a_minor_1_intent_faults_naming_the_field() {
    raw::clear();
    let env = raw_env_of_minor!(0);
    assert_eq!(env.host.plugins()[0].abi, (0, 0));
    let b = env.behavior("ok");
    let batch = b.batch().expect("batch");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch, &mut ents, 0);

    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].call, "tick_batch");
    assert!(
        faults[0].message.contains("intent.abi_version"),
        "{:?}",
        faults[0].message
    );
    assert_eq!(ents[0].status, Some(Status::Failure));
    assert!(
        ents[0].buf.is_empty(),
        "the refused intent is not passed on"
    );
}

#[test]
fn a_minor_0_plugin_reporting_a_minor_1_value_faults_naming_the_field() {
    raw::clear();
    let env = raw_env_of_minor!(0);
    let b = env.behavior("ok");
    let mut inst = b.instantiate(rate()).expect("instantiate");
    inst.set_input("in_integer", Some(BbValue::Integer(5)))
        .expect("set_input");

    let value = inst.output("out_integer");

    assert_eq!(value, None, "a refused value is not passed on");
    let faults = env.host.faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].call, "output");
    assert!(
        faults[0].message.contains("value.abi_version"),
        "{:?}",
        faults[0].message
    );
}

/// Control: the same plugin declaring minor 1 writes the same structs without a fault.
#[test]
fn a_minor_1_plugin_writing_minor_1_structs_does_not_fault() {
    raw::clear();
    let env = raw_env_of_minor!(UMBRA_ABI_VERSION_MINOR);
    let b = env.behavior("ok");
    let batch = b.batch().expect("batch");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch, &mut ents, 0);
    let mut inst = b.instantiate(rate()).expect("instantiate");
    inst.set_input("in_integer", Some(BbValue::Integer(5)))
        .expect("set_input");
    let value = inst.output("out_integer");

    assert_eq!(env.host.faults(), Vec::new());
    assert_eq!(ents[0].buf.len(), 1, "the SetSpeed intent arrived");
    assert_eq!(value, Some(BbValue::Integer(5)), "the value arrived");
}
