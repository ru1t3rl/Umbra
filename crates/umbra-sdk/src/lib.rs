//! `umbra-sdk`: write Umbra plugins in safe Rust (REQ-SDK-1, D-7).
//!
//! Single responsibility: safe traits, conversions and panic guards over the C ABI of
//! [`umbra-plugin-api`](abi), and the `export_plugin!` macro that emits the one exported symbol
//! `umbra_plugin_entry` (REQ-P-1). Plugin crates built with the SDK keep `#![forbid(unsafe_code)]`
//! (ADR 0023, amended R-M5).
//!
//! Dependency rule (REQ-A-3): the only workspace dependency is `umbra-plugin-api`.
//!
//! The plugin author's guide (ABI rules, lifecycle, determinism, packaging, the Rust and C examples)
//! is `docs/plugin-guide.md`; these docs cover the SDK API.
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
//! # Network adapters (ABI 0.1, ADR 0033)
//! Implement [`NetworkAdapterFactory`], [`NetworkAdapterConfig`] and [`NetworkAdapter`] and list the
//! factory under `network_adapters:` (after `behaviors:`, each list optional):
//!
//! ```
//! #![forbid(unsafe_code)]
//! use umbra_sdk::{
//!     ConfigureInput, Error, Inbox, KeySpace, NetMessage, NetworkAdapter, NetworkAdapterConfig,
//!     NetworkAdapterFactory, SimTime, export_plugin,
//! };
//!
//! struct NullFactory;
//! struct NullConfig { keys: KeySpace }
//! struct NullAdapter { sent: u64 }
//!
//! impl NetworkAdapterFactory for NullFactory {
//!     const ID: &'static str = "example.network.null";
//!     const PARAM_SCHEMA: &'static str = r#"{"type":"object","description":"Sends nowhere"}"#;
//!     type Config = NullConfig;
//!     fn configure(_input: &ConfigureInput) -> Result<NullConfig, Error> {
//!         Ok(NullConfig { keys: KeySpace::new(1_000, 1_000)? })
//!     }
//! }
//!
//! impl NetworkAdapterConfig for NullConfig {
//!     type Adapter = NullAdapter;
//!     fn key_space(&self) -> KeySpace { self.keys }
//!     fn start(&mut self, _now: SimTime) -> Result<NullAdapter, Error> {
//!         Ok(NullAdapter { sent: 0 }) // open sockets and start I/O threads here, never earlier
//!     }
//! }
//!
//! impl NetworkAdapter for NullAdapter {
//!     fn publish(&mut self, _now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error> {
//!         self.sent += out.len() as u64; // hand to a bounded queue; never block
//!         Ok(())
//!     }
//!     fn poll(&mut self, _now: SimTime, _inbox: &mut Inbox<'_>) -> Result<(), Error> {
//!         Ok(()) // push received messages while `!inbox.is_full()`
//!     }
//!     fn stop(&mut self) {} // join every I/O thread here
//! }
//!
//! export_plugin! {
//!     id: "example.plugin.null_net",
//!     name: "Null network example",
//!     version: "0.1.0",
//!     network_adapters: [NullFactory],
//! }
//! ```
//!
//! # Motion models, sensors, conditions and actions (ABI 0.2, ADR 0050)
//! Implement [`MotionModel`], [`Sensor`], [`Condition`] or [`Action`] (with its [`ActionInstance`]) and
//! list the type under `motion_models:`, `sensors:`, `conditions:` or `actions:` of `export_plugin!`.
//! Terrain ([`Terrain`]) and blackboard ([`BlackboardAccess`]) access arrive with the call and are valid
//! only during it. A plugin that exports any of these kinds declares ABI minor 2 (`docs/plugin-guide.md`,
//! "Motion models, sensors, conditions and actions in Rust"). Use [`math`] for trigonometry (ADR 0059):
//!
//! ```
//! #![forbid(unsafe_code)]
//! use umbra_sdk::{Error, Intent, KinematicState, MotionCtx, MotionModel, export_plugin, math};
//!
//! /// Keeps the speed and turns the velocity toward each `SetHeading` (east = 0, radians).
//! struct Turn;
//!
//! impl MotionModel for Turn {
//!     const ID: &'static str = "example.motion.turn";
//!     const SCHEMA: &'static str = r#"{"type":"object","description":"Turns on SetHeading"}"#;
//!     fn configure(_params: &str) -> Result<Self, Error> {
//!         Ok(Self)
//!     }
//!     fn step(&self, ctx: &MotionCtx<'_>, s: &KinematicState, intents: &[Intent<'_>], dt: f64)
//!         -> KinematicState {
//!         let speed = math::hypot(s.velocity[0], s.velocity[1]);
//!         let mut velocity = s.velocity;
//!         for intent in intents {
//!             if let Intent::SetHeading(h) = *intent {
//!                 let (sin, cos) = math::sin_cos(h);
//!                 velocity = [speed * cos, speed * sin, s.velocity[2]];
//!             }
//!         }
//!         // Terrain of this call (for example to hold a height above ground). A refused query makes
//!         // the step report the host's code, and the result below is then not used.
//!         let _ground_m = ctx.terrain().height_at(s.position).unwrap_or(0.0);
//!         let p = s.position;
//!         KinematicState {
//!             position: [p[0] + velocity[0] * dt, p[1] + velocity[1] * dt, p[2] + velocity[2] * dt],
//!             velocity,
//!             orientation: s.orientation,
//!         }
//!     }
//! }
//!
//! export_plugin! {
//!     id: "example.plugin.turn",
//!     name: "Turn example",
//!     version: "0.1.0",
//!     motion_models: [Turn],
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
//! - **Output** structs (descriptor, capabilities, vtables, intents, values, network messages,
//!   contacts) carry the minor of the capability kind they belong to (behavior 0, network adapter 1,
//!   the ABI 0.2 kinds 2; ADR 0033, [`BEHAVIOR_MINOR`] and the other kind minors) and that minor's
//!   size; the descriptor declares the highest of its capabilities'.
//! - **Network adapters** (ADR 0033): messages in both directions are checked against the ABI rules
//!   of their kind (malformed host input is `UMBRA_ERR_INVALID_ARGUMENT` before any trait call; an
//!   invalid pushed message is refused before it reaches the host), at most `max` messages are passed
//!   per `poll`, the sink is reachable only during its call, and the lifecycle (`start` once, `stop`
//!   before drop, adapter before configuration) is enforced by the SDK.
//! - **Determinism** (REQ-K-4): time arrives in [`TickContext`] (and the contexts of the ABI 0.2 kinds);
//!   randomness only through their `rng()`, with the call's own host token. The SDK has no clock and no
//!   RNG; [`math`] mirrors the engine's float functions (ADR 0059).
//! - **ABI 0.2 kinds** (ADR 0050): every call record is validated before the trait runs; a refused
//!   service call, draw, contact or intent ends the call with the host's code, and no result is written
//!   then; a blackboard read accepts exactly one report from the host.
//! - **Handles** are plugin-owned boxes tagged per capability; a handle of another capability is
//!   refused, never reinterpreted.
//!
//! # Rules for plugin authors (ADR 0021, `docs/plugin-guide.md` "State, reload and thread-locals")
//! - **Per-handle state** (RR-m3): results may depend only on per-handle state (the configured
//!   [`Behavior`] and its [`BehaviorInstance`]s, the call's arguments and its RNG draws), never on
//!   global state shared across handles, images, hosts or engines. Two engines in one process share
//!   one image, items of one batch may come from different configured behaviors, and grouping and
//!   call order are an engine detail. Globals that never influence results are fine.
//! - **`init` must reset global state** (RR-M1): an image can be loaded again in the same process
//!   after its last handle was released and `shutdown` ran (entry, `init` and a new host API again).
//!   With `keep_loaded`, or when the OS keeps the library mapped, statics keep their old values, so
//!   set any global state you keep in the `init:` hook of `export_plugin!`, never assume it is zero.
//!   The SDK resets its own state in `init`.
//! - **No thread-locals with destructors** (ASM-142): calls arrive on varying threads, and glibc never
//!   unmaps a library that registered a thread-local destructor (closing it is a crash risk on
//!   macOS). Do not use `thread_local!` values that need `Drop` on the load or call path; use a
//!   `static` with `Mutex` or `OnceLock`, as the SDK does.
//!
//! This is one of the three crates allowed to contain `unsafe` (CLAUDE.md rule 5, NFR-2). It lives in
//! the modules `convert`, `guard`, `host` and `export` (with their `network`, `kinds`, `motion`,
//! `sensor` and `leaf` children); every `unsafe` block carries a `// SAFETY:`
//! comment naming the invariant it relies on and is exercised by a test.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

/// The raw C ABI types the SDK is built on (`umbra-plugin-api`), re-exported so that code generated
/// by `export_plugin!` can name them as `$crate::abi::..` without its own dependency.
pub use umbra_plugin_api as abi;

/// `serde_json`, re-exported: the type of `ConfigureInput::params` and `templates`, so a plugin names
/// `umbra_sdk::serde_json::Value` without its own dependency.
pub use serde_json;

mod behavior;
mod convert;
mod error;
mod export;
mod guard;
mod host;
mod leaf;
pub mod math;
mod minor;
mod motion;
mod network;
mod sensor;
mod services;

pub use behavior::{
    Behavior, BehaviorFactory, BehaviorInstance, Contact, EntityView, Intent, Intents, Interface,
    Level, Params, Rng, Route, Status, TickContext, Value, ValueType, log,
};
pub use error::Error;
#[doc(hidden)]
pub use export::__private;
pub use leaf::{Action, ActionInstance, Condition, LeafCtx};
pub use minor::{
    ACTION_MINOR, BEHAVIOR_MINOR, CONDITION_MINOR, MOTION_MODEL_MINOR, NETWORK_ADAPTER_MINOR,
    SENSOR_MINOR,
};
pub use motion::{KinematicState, MotionCtx, MotionModel};
pub use network::{
    ConfigureInput, ControlAction, Counters, DeadReckoning, EntityState, Euler, Inbox, KeySpace,
    MANAGER_TEMPLATE_KEYS, NetKey, NetMessage, NetworkAdapter, NetworkAdapterConfig,
    NetworkAdapterFactory, SimTime,
};
pub use sensor::{Contacts, Ownership, SenseCtx, Sensor, TargetView};
pub use services::{BlackboardAccess, Terrain, Vec3};
