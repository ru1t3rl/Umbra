//! Network adapter plugins in safe Rust (ABI 0.1, ADR 0033, REQ-N-1, REQ-SDK-1).
//!
//! A plugin implements [`NetworkAdapterFactory`] (the configure input to a configuration),
//! [`NetworkAdapterConfig`] (its key space, then `start`) and [`NetworkAdapter`] (publish, poll,
//! counters, stop) and lists the factory under `network_adapters:` in `export_plugin!`. The traits
//! mirror the engine's `umbra_core::net::{NetworkAdapterConfig, NetworkAdapter}` and the ABI
//! lifecycle `configure -> key_space -> start -> (publish | poll | counters)* -> stop -> destroy`
//! (`umbra-plugin-api` `network.rs`); the SDK does not depend on `umbra-core` (REQ-A-3), so the
//! message types here are owned mirrors of the canonical model with borrowed strings.
//!
//! What the SDK guarantees (ASM-411):
//! - every message the host sends is validated before [`NetworkAdapter::publish`] sees it (the
//!   fields its kind uses only: known codes, finite numbers, UTF-8 strings of at most
//!   `UMBRA_MAX_ID_LEN` bytes, non-zero subject keys, a non-empty weapon for `FIRE`); malformed input
//!   fails the call with `UMBRA_ERR_INVALID_ARGUMENT` before any trait call;
//! - every message the plugin pushes through [`Inbox::push`] is checked by the same rules first; a
//!   message the ABI cannot carry is refused with `UMBRA_ERR_INVALID_ARGUMENT` and never reaches the
//!   host, and at most `max` messages are passed to the host per `poll`;
//! - structs the SDK writes for a network adapter (its capability, vtable and messages) carry the
//!   network kind's minor (1) and that minor's size;
//! - the host's sink is only reachable through the [`Inbox`] of the current `poll` call, which cannot
//!   outlive the call or leave its thread;
//! - `start` runs at most once per configuration, `stop` reaches the adapter at most once and always
//!   before it is dropped, and the adapter is dropped before its configuration.
//!
//! The host checks some rules the SDK cannot see without the engine's grammars: a `force` must be a
//! valid force ID and a `weapon` a valid weapon ID (ADR 0002). A pushed message failing them is
//! refused by the host and faults the capability.
//!
//! Threads (ADR 0033): the adapter's I/O threads belong to the plugin. They must never call the host
//! API (logging from them is refused by the host), and [`NetworkAdapter::stop`] and the adapter's
//! `Drop` must join every one of them. `publish` and `poll` never block on I/O: move data to or from
//! bounded queues and count what is dropped in [`Counters`].
//!
//! Determinism (REQ-K-4): time arrives as [`SimTime`]; the SDK offers no clock and no RNG.

use std::collections::BTreeMap;

use crate::abi::{UMBRA_ERR_STATE, UMBRA_MAX_ID_LEN, UMBRA_OK, UmbraCode};
use crate::behavior::Fault;
use crate::convert::NetSink;
use crate::error::Error;

/// The identity of an entity as the network knows it (the engine's `NetKey`). `0` is never a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetKey(pub u64);

/// The keys an adapter issues for local entities: `base < key <= base + count`. Construct it with
/// [`KeySpace::new`], which refuses an empty or overflowing range (ADR 0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpace {
    base: u64,
    count: u64,
}

impl KeySpace {
    /// The key space `(base, base + count]`.
    ///
    /// # Errors
    /// `count` is 0 or `base + count` overflows `u64`.
    pub fn new(base: u64, count: u64) -> Result<Self, Error> {
        if count == 0 {
            return Err(Error::new(
                "key space: count is 0 (an adapter issues at least one key)",
            ));
        }
        if base.checked_add(count).is_none() {
            return Err(Error::new(format!(
                "key space: base {base} + count {count} overflows u64"
            )));
        }
        Ok(Self { base, count })
    }

    /// The key just below the first issued key (excluded).
    #[must_use]
    pub fn base(&self) -> u64 {
        self.base
    }

    /// How many keys are issued.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Whether `key` lies in `(base, base + count]`.
    #[must_use]
    pub fn contains(&self, key: NetKey) -> bool {
        key.0 > self.base && key.0 - self.base <= self.count
    }
}

/// Simulation time of a call: tick `tick` of a run at `rate_hz` (REQ-K-4). Never a wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimTime {
    tick: u64,
    rate_hz: u32,
}

impl SimTime {
    /// The time at tick `tick` of a run at `rate_hz` (`rate_hz > 0`, checked by the SDK at the
    /// boundary).
    pub(crate) fn new(tick: u64, rate_hz: u32) -> Self {
        Self { tick, rate_hz }
    }

    /// The tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// The tick rate in Hz (never 0).
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// Simulation seconds since tick 0 (`tick / rate_hz`).
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a tick count above 2^53 is not reached by any run"
    )]
    pub fn seconds(&self) -> f64 {
        self.tick as f64 / f64::from(self.rate_hz)
    }
}

/// How receivers extrapolate an entity state (ADR 0032; the ABI codes `UMBRA_DR_*` in this order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeadReckoning {
    /// No extrapolation.
    Static,
    /// Constant world velocity.
    Velocity,
    /// Constant world velocity and rotation.
    VelocityRotation,
    /// Constant world acceleration and rotation.
    AccelerationRotation,
    /// Constant world acceleration.
    Acceleration,
    /// Constant body velocity.
    BodyVelocity,
    /// Constant body velocity and rotation.
    BodyVelocityRotation,
    /// Constant body acceleration and rotation.
    BodyAccelerationRotation,
    /// Constant body acceleration.
    BodyAcceleration,
}

/// A run-control action (the ABI codes `UMBRA_NET_CONTROL_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ControlAction {
    /// Start the run.
    Start,
    /// Pause it.
    Pause,
    /// Resume it.
    Resume,
    /// Stop it.
    Stop,
}

/// A world-frame orientation as Euler angles in radians (ADR 0032).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Euler {
    /// Heading.
    pub yaw: f64,
    /// Elevation.
    pub pitch: f64,
    /// Bank.
    pub roll: f64,
}

/// The full kinematic state of one networked entity (the engine's `NetEntityState`). Strings are
/// borrowed; `None` is "not sent". ECEF metres and SI units (D-15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityState<'a> {
    /// The entity's network key (never 0).
    pub key: NetKey,
    /// The adapter-normalised entity type token; opaque to the engine.
    pub entity_type: Option<&'a str>,
    /// The force ID.
    pub force: Option<&'a str>,
    /// The display name.
    pub name: Option<&'a str>,
    /// ECEF position (m).
    pub position: [f64; 3],
    /// ECEF velocity (m/s).
    pub velocity: [f64; 3],
    /// ECEF acceleration (m/s^2).
    pub acceleration: [f64; 3],
    /// Body-axis angular velocity (rad/s).
    pub angular_velocity: [f64; 3],
    /// World-frame orientation.
    pub orientation: Euler,
    /// How receivers extrapolate this state.
    pub dead_reckoning: DeadReckoning,
}

/// A message crossing the network boundary, in either direction (the engine's `NetMessage`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NetMessage<'a> {
    /// An entity's state.
    EntityState(EntityState<'a>),
    /// An entity left the exercise.
    EntityRemoved {
        /// The entity that left (never 0).
        key: NetKey,
    },
    /// A weapon was fired.
    Fire {
        /// The firing entity (never 0).
        shooter: NetKey,
        /// The entity fired at, if any (never `Some(NetKey(0))`).
        target: Option<NetKey>,
        /// The weapon ID (non-empty).
        weapon: &'a str,
        /// The launch point (ECEF, m).
        position: [f64; 3],
    },
    /// A weapon effect happened.
    Detonation {
        /// The firing entity, if known (never `Some(NetKey(0))`).
        shooter: Option<NetKey>,
        /// The entity hit, if any (never `Some(NetKey(0))`).
        target: Option<NetKey>,
        /// The burst point (ECEF, m).
        position: [f64; 3],
    },
    /// A run-control action.
    SimulationControl(ControlAction),
}

impl NetMessage<'_> {
    /// Checks the ABI rules of the fields this message's kind uses (the rules the SDK applies to
    /// every pushed message, see the module docs).
    ///
    /// # Errors
    /// `UMBRA_ERR_INVALID_ARGUMENT`, naming the field.
    pub fn validate(&self) -> Result<(), Error> {
        match self {
            NetMessage::EntityState(s) => {
                subject(s.key, "ENTITY_STATE key")?;
                text(s.entity_type, "ENTITY_STATE entity_type")?;
                text(s.force, "ENTITY_STATE force")?;
                text(s.name, "ENTITY_STATE name")?;
                finite(&s.position, "ENTITY_STATE position")?;
                finite(&s.velocity, "ENTITY_STATE velocity")?;
                finite(&s.acceleration, "ENTITY_STATE acceleration")?;
                finite(&s.angular_velocity, "ENTITY_STATE angular_velocity")?;
                let o = s.orientation;
                finite(&[o.yaw, o.pitch, o.roll], "ENTITY_STATE orientation")
            }
            NetMessage::EntityRemoved { key } => subject(*key, "ENTITY_REMOVED key"),
            NetMessage::Fire {
                shooter,
                target,
                weapon,
                position,
            } => {
                subject(*shooter, "FIRE shooter")?;
                optional(*target, "FIRE target")?;
                if weapon.is_empty() {
                    return Err(Error::invalid("FIRE weapon is empty (required for FIRE)"));
                }
                text(Some(weapon), "FIRE weapon")?;
                finite(position, "FIRE position")
            }
            NetMessage::Detonation {
                shooter,
                target,
                position,
            } => {
                optional(*shooter, "DETONATION shooter")?;
                optional(*target, "DETONATION target")?;
                finite(position, "DETONATION position")
            }
            NetMessage::SimulationControl(_) => Ok(()),
        }
    }
}

fn subject(key: NetKey, what: &str) -> Result<(), Error> {
    if key.0 == 0 {
        Err(Error::invalid(format!("{what} is 0 (0 is never a key)")))
    } else {
        Ok(())
    }
}

fn optional(key: Option<NetKey>, what: &str) -> Result<(), Error> {
    match key {
        Some(NetKey(0)) => Err(Error::invalid(format!(
            "{what} is Some(0); use None for no key"
        ))),
        _ => Ok(()),
    }
}

/// An optional string: not empty when present (empty is "none" in the ABI), at most
/// `UMBRA_MAX_ID_LEN` bytes.
fn text(value: Option<&str>, what: &str) -> Result<(), Error> {
    match value {
        Some("") => Err(Error::invalid(format!(
            "{what} is Some(\"\"); use None for no value"
        ))),
        Some(t) if t.len() > UMBRA_MAX_ID_LEN => Err(Error::invalid(format!(
            "{what} is {} bytes; max is {UMBRA_MAX_ID_LEN}",
            t.len()
        ))),
        _ => Ok(()),
    }
}

fn finite(v: &[f64], what: &str) -> Result<(), Error> {
    if v.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(Error::invalid(format!("{what} is not finite")))
    }
}

/// Running totals an adapter reports (the ABI order tx dropped, rx dropped, decode errors, I/O
/// errors). All counters only grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Counters {
    /// Outbound messages dropped (queue full or send failed).
    pub tx_dropped: u64,
    /// Inbound messages dropped (queue full).
    pub rx_dropped: u64,
    /// Inbound datagrams or frames that could not be decoded.
    pub decode_errors: u64,
    /// I/O errors seen.
    pub io_errors: u64,
}

impl Counters {
    /// The counters in the ABI order.
    pub(crate) fn in_abi_order(self) -> [u64; 4] {
        [
            self.tx_dropped,
            self.rx_dropped,
            self.decode_errors,
            self.io_errors,
        ]
    }
}

/// The input of `configure` (ADR 0030): the scenario's adapter name, its parameters (the capability's
/// param schema applies to these) and the `[network.<name>]` table of every template that has one.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigureInput {
    name: String,
    params: serde_json::Value,
    templates: BTreeMap<String, serde_json::Value>,
}

impl ConfigureInput {
    /// Parses the wrapper `{"name": <string>, "params": <value>, "templates": {<id>: <table>}}`.
    pub(crate) fn parse(json: &str) -> Result<Self, Error> {
        let bad =
            |what: &str| Error::invalid(format!("configure: input {what} (ADR 0030 wrapper)"));
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| bad(&format!("is not JSON: {e}")))?;
        let serde_json::Value::Object(mut map) = value else {
            return Err(bad("is not an object"));
        };
        let Some(serde_json::Value::String(name)) = map.remove("name") else {
            return Err(bad("has no string `name`"));
        };
        let params = map.remove("params").ok_or_else(|| bad("has no `params`"))?;
        let templates = match map.remove("templates") {
            Some(serde_json::Value::Object(t)) => t.into_iter().collect(),
            None => BTreeMap::new(),
            Some(_) => return Err(bad("has a `templates` that is not an object")),
        };
        Ok(Self {
            name,
            params,
            templates,
        })
    }

    /// The adapter's name in the scenario (unique per scenario).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The adapter's parameters (validated against the capability's param schema by the host).
    #[must_use]
    pub fn params(&self) -> &serde_json::Value {
        &self.params
    }

    /// The `[network.<name>]` table of each template that has one, by template ID (sorted).
    #[must_use]
    pub fn templates(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.templates
    }
}

/// The receive side of one `poll` call: pushes messages to the host's sink. It lives only for the
/// call and on the call's thread (REQ-P-11: the host's sink token is valid only during the call).
#[derive(Debug)]
pub struct Inbox<'a> {
    sink: NetSink,
    max: usize,
    pushed: usize,
    fault: &'a Fault,
}

impl<'a> Inbox<'a> {
    /// The inbox of one `poll` call with the host's `max` (already capped by `UMBRA_MAX_NET_POLL`).
    pub(crate) fn new(sink: NetSink, max: usize, fault: &'a Fault) -> Self {
        Self {
            sink,
            max,
            pushed: 0,
            fault,
        }
    }
}

impl Inbox<'_> {
    /// How many more messages this call accepts.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.max - self.pushed
    }

    /// Whether this call accepts no more messages.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.remaining() == 0
    }

    /// Hands `message` to the host, which copies it.
    ///
    /// # Errors
    /// - the message breaks an ABI rule ([`NetMessage::validate`]): `UMBRA_ERR_INVALID_ARGUMENT`; it
    ///   is not sent and nothing is faulted (count it as a decode error);
    /// - the inbox is full ([`Inbox::is_full`]): `UMBRA_ERR_STATE`; it is not sent and nothing is
    ///   faulted (keep it for the next poll);
    /// - the host refused it: the host's code; the capability is then faulted, and this `poll`
    ///   returns that code whether or not the adapter looks at the error. Later pushes of the call
    ///   are refused without a host call. The host checks two grammars the SDK does not (ADR 0002),
    ///   so check them before pushing (map anything else to `None` or count a decode error): the force must be one ID segment, `[a-z][a-z0-9_]*`, and the weapon an ADR 0002 namespaced ID,
    ///   `segment ("." segment)+` with that segment rule (at least two segments), each at most
    ///   `UMBRA_MAX_ID_LEN` (128) bytes, for example `blue` and `acme.cannon_120`.
    pub fn push(&mut self, message: &NetMessage<'_>) -> Result<(), Error> {
        if let Some((code, _)) = self.fault.get() {
            return Err(Error::with_code(
                code,
                format!("the host refused an earlier message of this poll (code {code})"),
            ));
        }
        message.validate()?;
        if self.is_full() {
            return Err(Error::with_code(
                UMBRA_ERR_STATE,
                format!("the inbox is full ({} messages per poll)", self.max),
            ));
        }
        let code: UmbraCode = self.sink.push(message);
        if code == UMBRA_OK {
            self.pushed += 1;
            Ok(())
        } else {
            self.fault.record(code, "message push");
            Err(Error::with_code(
                code,
                format!("the host refused a message (code {code})"),
            ))
        }
    }
}

/// Builds configured network adapters of one capability (the engine's adapter factory).
pub trait NetworkAdapterFactory: Send + Sync + 'static {
    /// Component ID (ADR 0002 grammar; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of [`ConfigureInput::params`] (ADR 0009 subset); the top-level `description` is
    /// the catalog text.
    const PARAM_SCHEMA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;
    /// The configured adapter.
    type Config: NetworkAdapterConfig;
    /// Builds a configuration. No I/O here: a dry-run build never starts.
    ///
    /// # Errors
    /// Bad params (use [`Error::params`]); the host reports a build error naming the capability.
    fn configure(input: &ConfigureInput) -> Result<Self::Config, Error>;
}

/// A configured adapter that can be started once (the engine's `NetworkAdapterConfig`).
pub trait NetworkAdapterConfig: Send + 'static {
    /// The started adapter.
    type Adapter: NetworkAdapter;
    /// The keys the adapter issues for local entities; read once, right after `configure`.
    fn key_space(&self) -> KeySpace;
    /// Starts the adapter's I/O at `now`. Called at most once (the SDK refuses a second call).
    ///
    /// # Errors
    /// It cannot start (a port in use, ...); the host reports a start error naming the capability.
    fn start(&mut self, now: SimTime) -> Result<Self::Adapter, Error>;
}

/// A running adapter (the engine's `NetworkAdapter`). Neither `publish` nor `poll` blocks.
pub trait NetworkAdapter: Send + 'static {
    /// Queues `out` for sending. A transient failure is counted in [`Counters`] and returns `Ok`.
    ///
    /// # Errors
    /// The adapter can do nothing more; the host faults the capability.
    fn publish(&mut self, now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error>;
    /// Pushes received messages, in arrival order, into `inbox` (at most [`Inbox::remaining`]). A
    /// transient failure is counted and returns `Ok`.
    ///
    /// # Errors
    /// The adapter can do nothing more; the host faults the capability and discards the messages of
    /// this call.
    fn poll(&mut self, now: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error>;
    /// The running totals; default: all 0.
    fn counters(&self) -> Counters {
        Counters::default()
    }
    /// Stops the I/O and joins every thread the adapter started. The SDK calls it at most once,
    /// also when the host destroys a started adapter without `stop` (then right before the drop,
    /// under its own panic guard). The `Drop` impls of the adapter and its configuration must not
    /// panic: one panic is caught and logged, a second one while unwinding aborts the process.
    fn stop(&mut self);
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigureInput, DeadReckoning, EntityState, Euler, KeySpace, NetKey, NetMessage, SimTime,
    };
    use crate::abi::{UMBRA_ERR_INVALID_ARGUMENT, UMBRA_MAX_ID_LEN};

    fn state<'a>() -> EntityState<'a> {
        EntityState {
            key: NetKey(1),
            entity_type: Some("tank"),
            force: Some("blue"),
            name: None,
            position: [1.0, 2.0, 3.0],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            angular_velocity: [0.0; 3],
            orientation: Euler::default(),
            dead_reckoning: DeadReckoning::Velocity,
        }
    }

    #[test]
    fn key_space_refuses_empty_and_overflowing_ranges() {
        assert!(KeySpace::new(0, 0).is_err());
        assert!(KeySpace::new(u64::MAX, 1).is_err());
        let largest = KeySpace::new(u64::MAX - 1, 1).expect("fits");
        assert!(largest.contains(NetKey(u64::MAX)));
        let k = KeySpace::new(10, 5).expect("fits");
        assert!(!k.contains(NetKey(10)) && k.contains(NetKey(11)) && k.contains(NetKey(15)));
        assert!(!k.contains(NetKey(16)));
    }

    #[test]
    fn validate_checks_only_the_rules_of_the_kind() {
        let long = "x".repeat(UMBRA_MAX_ID_LEN + 1);
        let max = "x".repeat(UMBRA_MAX_ID_LEN);
        let ok = NetMessage::EntityState(EntityState {
            name: Some(&max),
            ..state()
        });
        assert!(ok.validate().is_ok());
        let rows: [(NetMessage<'_>, &str); 9] = [
            (
                NetMessage::EntityState(EntityState {
                    key: NetKey(0),
                    ..state()
                }),
                "key is 0",
            ),
            (
                NetMessage::EntityState(EntityState {
                    name: Some(&long),
                    ..state()
                }),
                "max is 128",
            ),
            (
                NetMessage::EntityState(EntityState {
                    force: Some(""),
                    ..state()
                }),
                "use None",
            ),
            (
                NetMessage::EntityState(EntityState {
                    orientation: Euler {
                        yaw: f64::NAN,
                        ..Euler::default()
                    },
                    ..state()
                }),
                "orientation is not finite",
            ),
            (
                NetMessage::EntityRemoved { key: NetKey(0) },
                "ENTITY_REMOVED key",
            ),
            (
                NetMessage::Fire {
                    shooter: NetKey(1),
                    target: None,
                    weapon: "",
                    position: [0.0; 3],
                },
                "FIRE weapon is empty",
            ),
            (
                NetMessage::Fire {
                    shooter: NetKey(1),
                    target: Some(NetKey(0)),
                    weapon: "w",
                    position: [0.0; 3],
                },
                "FIRE target is Some(0)",
            ),
            (
                NetMessage::Detonation {
                    shooter: None,
                    target: None,
                    position: [f64::INFINITY, 0.0, 0.0],
                },
                "DETONATION position",
            ),
            (
                NetMessage::Detonation {
                    shooter: Some(NetKey(0)),
                    target: None,
                    position: [0.0; 3],
                },
                "DETONATION shooter",
            ),
        ];
        for (m, needle) in rows {
            let e = m.validate().expect_err(needle);
            assert_eq!(e.code(), UMBRA_ERR_INVALID_ARGUMENT, "{needle}");
            assert!(e.message().contains(needle), "{needle}: {}", e.message());
        }
        // A detonation without shooter and target is valid; so is every control action.
        assert!(
            NetMessage::Detonation {
                shooter: None,
                target: None,
                position: [0.0; 3]
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn configure_input_reads_the_adr_0030_wrapper() {
        let i = ConfigureInput::parse(
            r#"{"name":"lb","params":{"a":1},"templates":{"t.x":{"entity_type":"tank"}}}"#,
        )
        .expect("wrapper");
        assert_eq!(i.name(), "lb");
        assert_eq!(i.params()["a"], 1);
        assert_eq!(i.templates()["t.x"]["entity_type"], "tank");
        for bad in [
            "[]",
            "{}",
            r#"{"name":1,"params":{}}"#,
            r#"{"name":"a"}"#,
            r#"{"name":"a","params":{},"templates":[]}"#,
            "not json",
        ] {
            let e = ConfigureInput::parse(bad).expect_err(bad);
            assert_eq!(e.code(), UMBRA_ERR_INVALID_ARGUMENT, "{bad}");
        }
    }

    #[test]
    fn sim_time_seconds() {
        assert!((SimTime::new(45, 30).seconds() - 1.5).abs() < 1e-12);
    }
}
