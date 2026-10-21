//! The protocol-neutral network model and the adapter traits (REQ-N-1, D-9, ADR 0030).
//!
//! Nothing here names a wire protocol. An adapter maps these types to and from its own messages;
//! the manager in `umbra-net` and the engine only see this model. Positions are ECEF and values
//! SI (D-15, REQ-C-4).

pub mod contract;

use std::sync::Arc;

use crate::coords::Ecef;
use crate::dead_reckoning::DeadReckoning;
use crate::event::ControlAction;
use crate::geo::{Vec3, WorldEuler};
use crate::id::WeaponId;
use crate::perception::Force;
use crate::time::SimTime;

/// The identity of an entity as the network knows it, unique within one network exercise.
/// Adapters allocate and interpret the number; the engine only compares and orders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetKey(pub u64);

/// The range of keys an adapter may issue for local entities: `base < key <= base + count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpace {
    /// The key just below the first issued key (excluded).
    pub base: u64,
    /// How many keys are issued.
    pub count: u64,
}

impl KeySpace {
    /// Whether `key` lies in `(base, base + count]`. Never overflows.
    #[must_use]
    pub fn contains(&self, key: NetKey) -> bool {
        key.0 > self.base && key.0 - self.base <= self.count
    }
}

/// An entity type as an opaque token the adapter normalised (for example a dotted type string).
/// The engine compares it for equality and shows it; it never parses it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetEntityType(Arc<str>);

impl NetEntityType {
    /// Wraps the token as given.
    #[must_use]
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self(text.into())
    }

    /// The token text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The full kinematic state of one networked entity.
#[derive(Debug, Clone, PartialEq)]
pub struct NetEntityState {
    /// The entity's network identity.
    pub key: NetKey,
    /// Its type, if the peer sent one.
    pub entity_type: Option<NetEntityType>,
    /// Its side, if the peer sent one.
    pub force: Option<Force>,
    /// Its display name, if the peer sent one.
    pub name: Option<Arc<str>>,
    /// ECEF position.
    pub position: Ecef,
    /// ECEF velocity, m/s.
    pub velocity: Vec3,
    /// ECEF acceleration, m/s^2.
    pub acceleration: Vec3,
    /// Body-axis angular velocity, rad/s.
    pub angular_velocity: Vec3,
    /// Orientation as world-frame Euler angles.
    pub orientation: WorldEuler,
    /// How receivers extrapolate this state.
    pub dead_reckoning: DeadReckoning,
}

/// A message crossing the network boundary, in either direction.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NetMessage {
    /// An entity's state.
    EntityState(NetEntityState),
    /// An entity left the exercise.
    EntityRemoved {
        /// The entity that left.
        key: NetKey,
    },
    /// A weapon was fired.
    Fire {
        /// The firing entity.
        shooter: NetKey,
        /// The entity fired at, if any.
        target: Option<NetKey>,
        /// The weapon used.
        weapon: WeaponId,
        /// Where it was fired.
        position: Ecef,
    },
    /// A weapon effect happened.
    Detonation {
        /// The firing entity, if known.
        shooter: Option<NetKey>,
        /// The entity hit, if any.
        target: Option<NetKey>,
        /// Where it happened.
        position: Ecef,
    },
    /// A run-control action.
    SimulationControl(ControlAction),
}

/// A message the engine hands to an adapter.
pub type OutboundEvent = NetMessage;
/// A message an adapter hands to the engine.
pub type InboundEvent = NetMessage;

/// What an adapter learns when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterStart {
    /// The simulation time at start.
    pub now: SimTime,
}

/// Running totals an adapter reports. All counters only grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct AdapterCounters {
    /// Outbound messages dropped (queue full or send failed).
    pub tx_dropped: u64,
    /// Inbound messages dropped (queue full).
    pub rx_dropped: u64,
    /// Inbound datagrams or frames that could not be decoded.
    pub decode_errors: u64,
    /// I/O errors seen.
    pub io_errors: u64,
}

/// Why an adapter operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NetError {
    /// The adapter could not start.
    #[error("adapter start failed: {0}")]
    Start(String),
    /// An I/O error.
    #[error("adapter I/O error: {0}")]
    Io(String),
    /// The adapter is stopped or broken and does nothing more.
    #[error("adapter faulted: {0}")]
    Faulted(String),
    /// `start` was called a second time on the same configuration.
    #[error("adapter already started")]
    AlreadyStarted,
}

/// A configured adapter that can be started once (the registry's product for the
/// `NetworkAdapter` kind).
pub trait NetworkAdapterConfig: Send + Sync {
    /// The keys the started adapter issues for local entities.
    fn key_space(&self) -> KeySpace;

    /// Starts the adapter. At most once per configuration.
    ///
    /// # Errors
    /// [`NetError::AlreadyStarted`] on a second call; [`NetError::Start`] if it cannot start.
    fn start(&self, ctx: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError>;
}

/// A running adapter. Neither `publish` nor `poll` blocks.
pub trait NetworkAdapter: Send {
    /// Queues `out` for sending.
    ///
    /// # Errors
    /// An error (never a panic) once the adapter is stopped or faulted.
    fn publish(&mut self, now: SimTime, out: &[OutboundEvent]) -> Result<(), NetError>;

    /// Appends at most `max` received messages to `into`, in arrival order.
    ///
    /// # Errors
    /// An error (never a panic) once the adapter is stopped or faulted.
    fn poll(
        &mut self,
        now: SimTime,
        max: usize,
        into: &mut Vec<InboundEvent>,
    ) -> Result<(), NetError>;

    /// The running totals.
    fn counters(&self) -> AdapterCounters;

    /// Stops the adapter and joins its I/O threads. Idempotent.
    fn stop(&mut self);
}
