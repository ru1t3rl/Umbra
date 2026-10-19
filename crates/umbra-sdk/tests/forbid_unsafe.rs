//! AT-3 `export_macro_compiles_under_forbid_unsafe_code` (ASM-116, R-M5): a plugin crate root with
//! `#![forbid(unsafe_code)]` invokes `export_plugin!`. The test is that this file compiles. Own file
//! because `forbid` is crate-wide and the other AT-3 files play the host with `unsafe`.
#![forbid(unsafe_code)]

use umbra_sdk::abi::{UMBRA_ABI_VERSION, UmbraHostApi};
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
#[ignore = "WI-7"]
fn export_macro_compiles_under_forbid_unsafe_code() {
    let host = UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: 40,
        host_ctx: 1,
        log: None,
        rng_next_u64: None,
        rng_next_f64: None,
    };
    assert!(!umbra_plugin_entry(&raw const host).is_null());
}
