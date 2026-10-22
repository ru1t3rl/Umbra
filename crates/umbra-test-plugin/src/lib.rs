//! `umbra-test-plugin`: the test fixture plugin of the plugin host (M4 WI-10, ADR 0023 item 4).
//!
//! Plugin `test.plugin.fixture` 0.1.0, built with `umbra-sdk` only (REQ-SDK-3) and
//! `#![forbid(unsafe_code)]` like every plugin crate (ASM-116). It offers one capability per fault mode
//! a plugin built with the SDK can produce (ASM-132), plus the well-behaved capabilities the engine
//! tests use (ASM-125 b). Every capability is a `test.behavior.*`:
//!
//! | Capability | Behavior |
//! |---|---|
//! | `ok` | Running forever, no intents |
//! | `draw_speed` | each tick one draw `next_u64() % 100` from the entity stream, `SetSpeed(draw)`, Running |
//! | `needs_target` | binding-only param `target` (`entity`, not in the schema, ADR 0020); Running |
//! | `cross_param` | params `lo`, `hi` (numbers, defaults 0 and 10); `configure` fails when `lo > hi` |
//! | `lifecycle` | logs `create_behavior`, `create_instance`, `destroy_instance`, `destroy_behavior` |
//! | `panic_configure` … `panic_destroy` | panics in that call (`configure`, `instantiate`, `tick`, `halt`, `halt_running`, `set_input` of `speed_mps`, `output` of `turns`, the instance's `Drop`) |
//! | `bad_intent` | pushes `SetSpeed(NaN)`, which the host refuses |
//! | `intent_flood` | pushes 65 intents for its item (the cap is 64) |
//! | `bad_value` | reports output `turns` as entity 0, which breaks the `BbValue` invariants |
//!
//! The plugin's `init` logs `init` and its `shutdown` logs `shutdown`. Cargo features (one library per feature set, built by the
//! tests' build helper): `entry_panics` (building the descriptor panics, so the entry returns null),
//! `init_fails`, `init_panics`, `shutdown_panics`, and `bad_schema` (adds `test.behavior.bad_schema`,
//! whose param schema uses `anyOf`, outside the ADR 0009 subset). A library without the entry symbol,
//! or with a null one, cannot be written in safe Rust; the host tests build those from C.
//!
//! Network adapters (ABI 0.1, M6 WI-12, module `network`): the feature `network` adds the network
//! capabilities `test.network.net_ok`, `.net_panics_in_poll`, `.net_bad_message` and
//! `.net_blocks_join`, so the descriptor declares ABI 0.1 (the default build stays 0.0, ADR 0033).
//! The feature `network_peer` makes the crate a second, independent plugin, `test.plugin.net_peer`
//! 0.1.0, with only `test.network.peer_ok` and `.peer_panics_in_poll` (ASM-410, ASM-414).

#![forbid(unsafe_code)]
#![cfg_attr(
    feature = "network_peer",
    expect(
        dead_code,
        reason = "the network peer exports only its network capabilities, not the behaviors"
    )
)]

#[cfg(any(feature = "network", feature = "network_peer"))]
mod network;

use umbra_sdk::{
    Behavior, BehaviorFactory, BehaviorInstance, EntityView, Error, Intent, Intents, Interface,
    Level, Params, Status, TickContext, Value, ValueType, export_plugin, log,
};

/// What a capability does (see the crate docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Ok,
    DrawSpeed,
    NeedsTarget,
    CrossParam,
    Lifecycle,
    PanicConfigure,
    PanicInstantiate,
    PanicTick,
    PanicHalt,
    PanicHaltRunning,
    PanicSetInput,
    PanicOutput,
    PanicDestroy,
    BadIntent,
    IntentFlood,
    BadValue,
    #[cfg(feature = "bad_schema")]
    BadSchema,
}

/// The interface (metadata) of a capability.
fn interface_of(mode: Mode) -> Interface {
    match mode {
        Mode::Ok if cfg!(feature = "entry_panics") => {
            panic!("entry_panics: building the descriptor panics on purpose")
        }
        Mode::NeedsTarget => Interface::default().param("target", ValueType::Entity),
        Mode::CrossParam => Interface::default()
            .param("lo", ValueType::Number)
            .param("hi", ValueType::Number),
        Mode::PanicSetInput => Interface::default().param("speed_mps", ValueType::Number),
        Mode::PanicOutput | Mode::BadValue => {
            Interface::default().output("turns", ValueType::Integer)
        }
        _ => Interface::default(),
    }
}

/// A configured behavior of any capability.
struct Fixture {
    mode: Mode,
}

impl Fixture {
    fn configure(mode: Mode, params: &Params<'_>) -> Result<Self, Error> {
        match mode {
            Mode::PanicConfigure => panic!("panic_configure: configure panics on purpose"),
            Mode::Lifecycle => log(Level::Info, "create_behavior"),
            Mode::CrossParam => {
                let value = params.value()?;
                #[expect(
                    clippy::redundant_closure_for_method_calls,
                    reason = "the method path would name serde_json, which is not a dependency (REQ-SDK-3)"
                )]
                let number = |key: &str, default: f64| {
                    value.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
                };
                let (lo, hi) = (number("lo", 0.0), number("hi", 10.0));
                if lo > hi {
                    return Err(Error::params(format!(
                        "cross_param: lo ({lo}) must not exceed hi ({hi})"
                    )));
                }
            }
            _ => {}
        }
        Ok(Self { mode })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.mode == Mode::Lifecycle {
            log(Level::Info, "destroy_behavior");
        }
    }
}

impl Behavior for Fixture {
    type Instance = FixtureInstance;

    fn instantiate(&self, _rate_hz: u32) -> Result<FixtureInstance, Error> {
        match self.mode {
            Mode::PanicInstantiate => panic!("panic_instantiate: instantiate panics on purpose"),
            Mode::Lifecycle => log(Level::Info, "create_instance"),
            _ => {}
        }
        Ok(FixtureInstance {
            mode: self.mode,
            turns: 0,
        })
    }
}

/// One entity's instance of any capability.
struct FixtureInstance {
    mode: Mode,
    turns: i64,
}

impl Drop for FixtureInstance {
    fn drop(&mut self) {
        match self.mode {
            Mode::Lifecycle => log(Level::Info, "destroy_instance"),
            Mode::PanicDestroy if !std::thread::panicking() => {
                panic!("panic_destroy: destroy panics on purpose")
            }
            _ => {}
        }
    }
}

impl BehaviorInstance for FixtureInstance {
    fn tick(
        &mut self,
        ctx: &mut TickContext<'_>,
        _entity: &EntityView<'_>,
        out: &mut Intents<'_>,
    ) -> Status {
        self.turns += 1;
        match self.mode {
            Mode::PanicTick => panic!("panic_tick: tick panics on purpose"),
            Mode::DrawSpeed => {
                let draw = ctx.rng().next_u64() % 100;
                #[expect(clippy::cast_precision_loss, reason = "draw < 100, exact in f64")]
                let _ = out.push(Intent::SetSpeed(draw as f64));
            }
            Mode::BadIntent => {
                let _ = out.push(Intent::SetSpeed(f64::NAN));
            }
            Mode::IntentFlood => {
                for _ in 0..65 {
                    let _ = out.push(Intent::SetSpeed(1.0));
                }
            }
            _ => {}
        }
        Status::Running
    }

    fn halt(&mut self) {
        assert!(
            self.mode != Mode::PanicHalt,
            "panic_halt: halt panics on purpose"
        );
    }

    fn halt_running(&mut self) {
        assert!(
            self.mode != Mode::PanicHaltRunning,
            "panic_halt_running: halt_running panics on purpose"
        );
        self.halt();
    }

    fn set_input(&mut self, param: &str, _value: Option<Value>) -> Result<(), Error> {
        match (self.mode, param) {
            (Mode::PanicSetInput, _) => panic!("panic_set_input: set_input panics on purpose"),
            (Mode::NeedsTarget, "target") | (Mode::CrossParam, "lo" | "hi") => Ok(()),
            _ => Err(Error::new(format!("undeclared input {param}"))),
        }
    }

    fn output(&self, port: &str) -> Option<Value> {
        match (self.mode, port) {
            (Mode::PanicOutput, _) => panic!("panic_output: output panics on purpose"),
            (Mode::BadValue, "turns") => Some(Value::Entity(0)),
            (_, "turns") => Some(Value::Integer(self.turns)),
            _ => None,
        }
    }
}

/// Declares one capability type of `mode`.
macro_rules! capability {
    ($ty:ident, $id:literal, $mode:expr, $schema:expr) => {
        #[doc = concat!("Capability `", $id, "`.")]
        struct $ty;

        impl BehaviorFactory for $ty {
            const ID: &'static str = $id;
            const PARAM_SCHEMA: &'static str = $schema;
            type Behavior = Fixture;

            fn interface() -> Interface {
                interface_of($mode)
            }

            fn configure(params: &Params<'_>) -> Result<Fixture, Error> {
                Fixture::configure($mode, params)
            }
        }
    };
}

/// A schema with a description and no properties.
macro_rules! plain {
    ($what:literal) => {
        concat!(r#"{"type":"object","description":""#, $what, r#""}"#)
    };
}

capability!(
    Healthy,
    "test.behavior.ok",
    Mode::Ok,
    plain!("Runs forever")
);
capability!(
    DrawSpeed,
    "test.behavior.draw_speed",
    Mode::DrawSpeed,
    plain!("One draw per tick as the speed")
);
capability!(
    NeedsTarget,
    "test.behavior.needs_target",
    Mode::NeedsTarget,
    plain!("Needs a bound target entity")
);
capability!(
    CrossParam,
    "test.behavior.cross_param",
    Mode::CrossParam,
    r#"{"type":"object","description":"lo must not exceed hi","properties":{"lo":{"type":"number"},"hi":{"type":"number"}}}"#
);
capability!(
    Lifecycle,
    "test.behavior.lifecycle",
    Mode::Lifecycle,
    plain!("Logs its lifecycle calls")
);
capability!(
    PanicConfigure,
    "test.behavior.panic_configure",
    Mode::PanicConfigure,
    plain!("Panics in configure")
);
capability!(
    PanicInstantiate,
    "test.behavior.panic_instantiate",
    Mode::PanicInstantiate,
    plain!("Panics in instantiate")
);
capability!(
    PanicTick,
    "test.behavior.panic_tick",
    Mode::PanicTick,
    plain!("Panics in tick")
);
capability!(
    PanicHalt,
    "test.behavior.panic_halt",
    Mode::PanicHalt,
    plain!("Panics in halt")
);
capability!(
    PanicHaltRunning,
    "test.behavior.panic_halt_running",
    Mode::PanicHaltRunning,
    plain!("Panics in halt_running")
);
capability!(
    PanicSetInput,
    "test.behavior.panic_set_input",
    Mode::PanicSetInput,
    r#"{"type":"object","description":"Panics in set_input","properties":{"speed_mps":{"type":"number"}}}"#
);
capability!(
    PanicOutput,
    "test.behavior.panic_output",
    Mode::PanicOutput,
    plain!("Panics in output")
);
capability!(
    PanicDestroy,
    "test.behavior.panic_destroy",
    Mode::PanicDestroy,
    plain!("Panics when its instance is destroyed")
);
capability!(
    BadIntent,
    "test.behavior.bad_intent",
    Mode::BadIntent,
    plain!("Pushes an intent the host refuses")
);
capability!(
    IntentFlood,
    "test.behavior.intent_flood",
    Mode::IntentFlood,
    plain!("Pushes 65 intents for one item")
);
capability!(
    BadValue,
    "test.behavior.bad_value",
    Mode::BadValue,
    plain!("Reports an invalid output value")
);
#[cfg(feature = "bad_schema")]
capability!(
    BadSchema,
    "test.behavior.bad_schema",
    Mode::BadSchema,
    r#"{"type":"object","description":"Schema outside the subset","anyOf":[{"required":["a"]},{"required":["b"]}]}"#
);

/// Plugin `init` of the feature `init_panics`: panics.
#[cfg(feature = "init_panics")]
fn init() -> Result<(), Error> {
    panic!("init_panics: init panics on purpose")
}

/// Plugin `init` of the feature `init_fails`: fails.
#[cfg(all(feature = "init_fails", not(feature = "init_panics")))]
fn init() -> Result<(), Error> {
    Err(Error::new("init_fails: init fails on purpose"))
}

/// Plugin `init`: logs `init` (so a test can count image lifecycles), nothing else to set up.
#[cfg(not(any(feature = "init_fails", feature = "init_panics")))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "export_plugin! takes `fn() -> Result<(), Error>`"
)]
fn init() -> Result<(), Error> {
    log(Level::Info, "init");
    Ok(())
}

/// Plugin `shutdown`: logs `shutdown`, then panics with the feature `shutdown_panics`.
fn shutdown() {
    log(Level::Info, "shutdown");
    #[cfg(feature = "shutdown_panics")]
    panic!("shutdown_panics: shutdown panics on purpose");
}

/// Exports the plugin with the default capabilities plus the behaviors `$extra` and the network
/// adapters `$net`.
#[cfg(not(feature = "network_peer"))]
macro_rules! fixture {
    ([$($extra:ty),*], [$($net:ty),*]) => {
        export_plugin! {
            id: "test.plugin.fixture",
            name: "Umbra test fixture",
            version: "0.1.0",
            behaviors: [
                Healthy, DrawSpeed, NeedsTarget, CrossParam, Lifecycle, PanicConfigure, PanicInstantiate,
                PanicTick, PanicHalt, PanicHaltRunning, PanicSetInput, PanicOutput, PanicDestroy,
                BadIntent, IntentFlood, BadValue, $($extra),*
            ],
            network_adapters: [$($net),*],
            init: init,
            shutdown: shutdown,
        }
    };
}

#[cfg(not(any(feature = "bad_schema", feature = "network", feature = "network_peer")))]
fixture!([], []);
#[cfg(all(
    feature = "bad_schema",
    not(any(feature = "network", feature = "network_peer"))
))]
fixture!([BadSchema], []);
#[cfg(all(
    feature = "network",
    not(any(feature = "bad_schema", feature = "network_peer"))
))]
fixture!(
    [],
    [
        network::NetOk,
        network::NetPanicsInPoll,
        network::NetBadMessage,
        network::NetBlocksJoin
    ]
);
#[cfg(all(
    feature = "network",
    feature = "bad_schema",
    not(feature = "network_peer")
))]
fixture!(
    [BadSchema],
    [
        network::NetOk,
        network::NetPanicsInPoll,
        network::NetBadMessage,
        network::NetBlocksJoin
    ]
);
#[cfg(feature = "network_peer")]
export_plugin! {
    id: "test.plugin.net_peer",
    name: "Umbra test network peer",
    version: "0.1.0",
    network_adapters: [network::PeerOk, network::PeerPanicsInPoll],
    init: init,
    shutdown: shutdown,
}
