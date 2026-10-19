//! `umbra-sdk`: write Umbra plugins in safe Rust (REQ-SDK-1, D-7).
//!
//! Single responsibility: safe traits, conversions and panic guards over the C ABI of
//! [`umbra-plugin-api`](abi), and the `export_plugin!` macro that emits the one exported symbol
//! `umbra_plugin_entry` (REQ-P-1). Plugin crates built with the SDK keep `#![forbid(unsafe_code)]`
//! (ADR 0023, amended R-M5).
//!
//! Dependency rule (REQ-A-3): the only workspace dependency is `umbra-plugin-api`.
//!
//! # Writing a plugin
//! Implement [`BehaviorFactory`], [`Behavior`] and [`BehaviorInstance`] for each behavior, then export
//! them once per crate (a `cdylib` with `#![forbid(unsafe_code)]`):
//!
//! ```
//! #![forbid(unsafe_code)]
//! use umbra_sdk::{
//!     Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intent, Intents, Interface,
//!     Params, Status, TickContext, ValueType, export_plugin,
//! };
//!
//! struct CruiseFactory;
//! struct Cruise { speed: f64 }
//! struct CruiseInstance { speed: f64 }
//!
//! impl BehaviorFactory for CruiseFactory {
//!     const ID: &'static str = "example.behavior.cruise";
//!     const PARAM_SCHEMA: &'static str = r#"{"type":"object","description":"Cruise at a speed",
//!         "properties":{"speed_mps":{"type":"number"}}}"#;
//!     type Behavior = Cruise;
//!     fn interface() -> Interface {
//!         Interface::default().param("speed_mps", ValueType::Number)
//!     }
//!     fn configure(params: &Params<'_>) -> Result<Cruise, Error> {
//!         let speed = params.value()?.get("speed_mps").and_then(|v| v.as_f64()).unwrap_or(5.0);
//!         Ok(Cruise { speed })
//!     }
//! }
//!
//! impl Behavior for Cruise {
//!     type Instance = CruiseInstance;
//!     fn instantiate(&self, _rate_hz: u32) -> Result<CruiseInstance, Error> {
//!         Ok(CruiseInstance { speed: self.speed })
//!     }
//! }
//!
//! impl BehaviorInstance for CruiseInstance {
//!     fn tick(&mut self, ctx: &mut TickContext<'_>, _: &EntityView<'_>, out: &mut Intents<'_>) -> Status {
//!         // Randomness only from the entity's own stream, through the host (REQ-K-4).
//!         let heading = std::f64::consts::TAU * ctx.rng().next_f64();
//!         let _ = out.push(Intent::SetSpeed(self.speed));
//!         let _ = out.push(Intent::SetHeading(heading));
//!         Status::Running
//!     }
//!     fn halt(&mut self) {}
//!     fn set_input(&mut self, param: &str, value: Option<umbra_sdk::Value>) -> Result<(), Error> {
//!         match (param, value) {
//!             ("speed_mps", Some(umbra_sdk::Value::Number(v))) => { self.speed = v; Ok(()) }
//!             _ => Err(Error::new(format!("unsupported input {param}"))),
//!         }
//!     }
//! }
//!
//! export_plugin! {
//!     id: "example.plugin.cruise",
//!     name: "Cruise example",
//!     version: "0.1.0",
//!     behaviors: [CruiseFactory],
//! }
//! ```
//!
//! # What the SDK guarantees at the boundary (ADR 0019, ADR 0020, ADR 0021)
//! - **Panics** never unwind into the host: every exported function runs under `catch_unwind`; the
//!   message goes to the call's error sink (or, for calls without one, to the host log at error level)
//!   and the call returns `UMBRA_ERR_PANIC`; the payload is dropped inside a nested guard. A panic
//!   while the entry builds the descriptor makes the entry return null (REQ-P-7, R-M1).
//! - **Input** from the host is validated before any trait call: nulls, alignment, the header rule
//!   (own major, `struct_size` in range and a multiple of 8), bounds, UTF-8, enum values and the
//!   `BbValue` invariants. Malformed input is `UMBRA_ERR_INVALID_ARGUMENT` (R-M2).
//! - **Batches** (`tick_batch`) are validated completely first, then each item is ticked in order and
//!   gets its status; the first host refusal or panic ends the call with its code (REQ-B-4).
//! - **Output** structs carry the SDK's own `UMBRA_ABI_VERSION` and size.
//! - **Determinism** (REQ-K-4): time arrives in [`TickContext`]; randomness only through
//!   [`TickContext::rng`], with the batch item's own host token. The SDK has no clock and no RNG.
//! - **Handles** are plugin-owned boxes tagged per capability; a handle of another capability is
//!   refused, never reinterpreted.
//!
//! This is one of the three crates allowed to contain `unsafe` (CLAUDE.md rule 5, NFR-2). It lives in
//! the modules `convert`, `guard`, `host` and `export`; every `unsafe` block carries a `// SAFETY:`
//! comment naming the invariant it relies on and is exercised by a test.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

/// The raw C ABI types the SDK is built on (`umbra-plugin-api`), re-exported so that code generated
/// by `export_plugin!` can name them as `$crate::abi::..` without its own dependency.
pub use umbra_plugin_api as abi;

mod behavior;
mod convert;
mod error;
mod export;
mod guard;
mod host;

pub use behavior::{
    Behavior, BehaviorFactory, BehaviorInstance, Contact, EntityView, Intent, Intents, Interface,
    Level, Params, Rng, Route, Status, TickContext, Value, ValueType, log,
};
pub use error::Error;
#[doc(hidden)]
pub use export::__private;
