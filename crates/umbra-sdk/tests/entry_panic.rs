//! AT-3 `entry_panic_returns_null` (R-M1): a panic while the entry builds the descriptor (here in
//! `interface()`) must not unwind across the FFI boundary; the entry returns null. Own binary because a
//! plugin that panics in its entry has no usable descriptor.
#![forbid(unsafe_code)]

use umbra_sdk::abi::{UMBRA_ABI_VERSION, UmbraHostApi};
use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intents, Interface, Params,
    Status, TickContext, export_plugin,
};

struct F;
struct B;
struct I;
impl BehaviorFactory for F {
    const ID: &'static str = "test.behavior.entry_panic";
    const PARAM_SCHEMA: &'static str = r#"{"type":"object"}"#;
    type Behavior = B;
    fn interface() -> Interface {
        panic!("boom entry");
    }
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
    id: "test.plugin.entry_panic",
    name: "entry panic",
    version: "0.1.0",
    behaviors: [F],
}

#[test]
#[ignore = "WI-7"]
fn entry_panic_returns_null() {
    let host = UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: 40,
        host_ctx: 1,
        log: None,
        rng_next_u64: None,
        rng_next_f64: None,
    };
    assert!(umbra_plugin_entry(&raw const host).is_null());
}
