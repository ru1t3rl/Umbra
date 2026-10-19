//! AT-3 `export_macro_compiles_under_forbid_unsafe_code` (ASM-116, R-M5): a plugin crate root with
//! `#![forbid(unsafe_code)]` invokes `export_plugin!`. The test is that this file compiles. Own file
//! because `forbid` is crate-wide and the other AT-3 files play the host with `unsafe`.
//!
//! Amended 2026-10-19 (human decision after the WI-7 review): the generated entry is an
//! `unsafe extern "C" fn`, so this crate cannot call it; it only checks that the symbol has the ABI's
//! entry type. The calls are made by the other AT-3 files, which are allowed `unsafe`.
#![forbid(unsafe_code)]

use umbra_sdk::abi::UmbraPluginEntry;
use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intents, Params, Status,
    TickContext, export_plugin,
};

struct F;
struct B;
struct I;
impl BehaviorFactory for F {
    const ID: &'static str = "test.behavior.forbid";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Behavior = B;
    fn configure(_: &Params<'_>) -> Result<B, Error> {
        Ok(B)
    }
}
impl Behavior for B {
    type Instance = I;
    fn instantiate(&self, _: u32) -> Result<I, Error> {
        Ok(I)
    }
}
impl BehaviorInstance for I {
    fn tick(&mut self, _: &mut TickContext<'_>, _: &EntityView<'_>, _: &mut Intents<'_>) -> Status {
        Status::Running
    }
    fn halt(&mut self) {}
}

export_plugin! {
    id: "test.plugin.forbid",
    name: "forbid",
    version: "0.1.0",
    behaviors: [F],
}

#[test]
fn export_macro_compiles_under_forbid_unsafe_code() {
    // REQ-P-1: the generated symbol has exactly the ABI's entry type (an `unsafe extern "C" fn`).
    let entry: UmbraPluginEntry = umbra_plugin_entry;
    let _ = entry;
}
