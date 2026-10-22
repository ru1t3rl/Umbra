//! The adapter proper: the SDK traits over the [`Translator`] and the [`Transport`] (REQ-N-1, ADR 0034).
//!
//! `publish` and `poll` never block: they move datagrams to and from the bounded queues of the
//! transport. The logic sits in inherent methods that take plain values ([`Stamp`], a message list, a
//! vector to fill), so tests can drive an adapter without a host; the trait impls only adapt the SDK
//! types (`SimTime`, `Inbox`). Everything is per instance (ASM-410): the plugin holds no statics.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::net::SocketAddr;

use umbra_sdk::{
    ConfigureInput, Counters, Error, Inbox, KeySpace, Level, NetMessage, NetworkAdapter,
    NetworkAdapterConfig, NetworkAdapterFactory, SimTime, log,
};

use crate::codec::DecodeCounters;
use crate::config::{Config, PARAM_SCHEMA};
use crate::translate::{Inbound, Skip, Stamp, Translator, key_space};
use crate::transport::{Closed, Transport, TransportCounters};

/// Most datagrams one `poll` examines; the rest stay queued for the next call.
const POLL_BUDGET: usize = 1024;

/// The capability: DIS over UDP.
#[derive(Debug, Clone, Copy)]
pub struct DisFactory;

impl NetworkAdapterFactory for DisFactory {
    const ID: &'static str = "umbra.net.dis";
    const PARAM_SCHEMA: &'static str = PARAM_SCHEMA;
    type Config = DisConfig;

    fn configure(input: &ConfigureInput) -> Result<DisConfig, Error> {
        DisConfig::new(Config::from_input(input)?)
    }
}

/// A configured adapter that has not started (no socket yet).
#[derive(Debug)]
pub struct DisConfig {
    config: Config,
    keys: KeySpace,
    started: bool,
}

impl DisConfig {
    /// The configured adapter for `config`.
    ///
    /// # Errors
    /// The key space cannot be built (not reachable for a validated [`Config`]).
    pub fn new(config: Config) -> Result<Self, Error> {
        let keys = key_space(config.site, config.application)?;
        Ok(Self {
            config,
            keys,
            started: false,
        })
    }

    /// The validated configuration.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Opens the socket, starts the I/O threads and logs the security warning when the
    /// configuration calls for one. This is the whole of `start`, without the SDK types.
    ///
    /// # Errors
    /// Already started, or the socket cannot be opened (for example the port is in use).
    pub fn start_adapter(&mut self) -> Result<DisAdapter, Error> {
        if self.started {
            return Err(Error::new("DIS adapter already started"));
        }
        let c = &self.config;
        let transport = Transport::start(c.bind, &c.mode, c.queues.send, c.queues.receive)
            .map_err(|e| Error::new(format!("DIS adapter `{}`: {e}", c.name)))?;
        self.started = true;
        // Logged from this call (the host's thread), never from an I/O thread (ADR 0033).
        if let Some(warning) = c.security_warning() {
            log(Level::Warn, &warning);
        }
        Ok(DisAdapter {
            translator: Translator::new(c),
            transport,
            decode: DecodeCounters::default(),
            skips: SkipTally::default(),
            rejected_out: 0,
            delivered: 0,
            scratch: Vec::new(),
        })
    }
}

impl NetworkAdapterConfig for DisConfig {
    type Adapter = DisAdapter;

    fn key_space(&self) -> KeySpace {
        self.keys
    }

    fn start(&mut self, _now: SimTime) -> Result<DisAdapter, Error> {
        self.start_adapter()
    }
}

/// Received PDUs that were decoded and then not turned into a message, by reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkipTally {
    /// Another exercise.
    pub exercise: u64,
    /// A reserved entity ID.
    pub entity_id: u64,
    /// A value that is not finite.
    pub not_finite: u64,
    /// A Fire whose munition type is in no `weapons` entry.
    pub unmapped_weapon: u64,
    /// Run control this adapter itself sent, echoed back by broadcast or multicast.
    pub own_control: u64,
    /// A message the SDK checks refused.
    pub invalid: u64,
}

impl SkipTally {
    fn count(&mut self, skip: Skip) {
        let slot = match skip {
            Skip::Exercise => &mut self.exercise,
            Skip::EntityId => &mut self.entity_id,
            Skip::NotFinite => &mut self.not_finite,
            Skip::UnmappedWeapon => &mut self.unmapped_weapon,
            Skip::OwnControl => &mut self.own_control,
            Skip::Invalid => &mut self.invalid,
        };
        *slot = slot.saturating_add(1);
    }

    /// The total of all reasons.
    #[must_use]
    pub fn total(&self) -> u64 {
        [
            self.exercise,
            self.entity_id,
            self.not_finite,
            self.unmapped_weapon,
            self.own_control,
            self.invalid,
        ]
        .into_iter()
        .fold(0, u64::saturating_add)
    }
}

/// Everything an adapter instance counts, for tests and diagnostics (the ABI carries only
/// [`Counters`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stats {
    /// What the codec made of the datagrams.
    pub decode: DecodeCounters,
    /// Decoded PDUs that became no message.
    pub skips: SkipTally,
    /// Messages refused by the translator (not sendable), counted in `tx_dropped`.
    pub rejected_out: u64,
    /// Messages handed to the engine by `poll`.
    pub delivered: u64,
    /// Entity States with a reserved dead reckoning code (read as static).
    pub reserved_dead_reckoning: u64,
    /// The socket and queue counters.
    pub transport: TransportCounters,
}

/// A started DIS adapter.
#[derive(Debug)]
pub struct DisAdapter {
    translator: Translator,
    transport: Transport,
    decode: DecodeCounters,
    skips: SkipTally,
    rejected_out: u64,
    delivered: u64,
    /// Reused by every `poll` (cleared first).
    scratch: Vec<Inbound>,
}

fn stopped() -> Error {
    Error::new("the DIS adapter is stopped or its I/O thread ended")
}

impl DisAdapter {
    /// The address the adapter listens on (the chosen port when the configured port was 0).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.transport.local_addr()
    }

    /// Every counter of the instance.
    #[must_use]
    pub fn stats(&self) -> Stats {
        Stats {
            decode: self.decode.clone(),
            skips: self.skips,
            rejected_out: self.rejected_out,
            delivered: self.delivered,
            reserved_dead_reckoning: self.translator.tally().reserved_dead_reckoning,
            transport: self.transport.counters(),
        }
    }

    /// Translates `out` and queues the datagrams. A message that cannot be sent, a full queue and
    /// a failed send are counted and still `Ok`.
    ///
    /// # Errors
    /// The adapter is stopped or its send thread ended: it can do nothing more.
    pub fn publish_at(&mut self, at: Stamp, out: &[NetMessage<'_>]) -> Result<(), Error> {
        if self.transport.is_stopped() {
            return Err(stopped());
        }
        for message in out {
            match self.translator.outbound(at, message) {
                Ok(datagram) => self.transport.send(datagram).map_err(|Closed| stopped())?,
                Err(_) => self.rejected_out = self.rejected_out.saturating_add(1),
            }
        }
        Ok(())
    }

    /// Appends at most `max` received messages to `into`, in arrival order, and looks at no more than
    /// a fixed budget of datagrams; the rest stay queued. Datagrams that are no message are counted.
    ///
    /// # Errors
    /// The adapter is stopped or its receive thread ended and the queue is empty.
    pub fn poll_into(&mut self, max: usize, into: &mut Vec<Inbound>) -> Result<(), Error> {
        let mut taken = 0_usize;
        for _ in 0..POLL_BUDGET {
            if taken >= max {
                break;
            }
            let Some(datagram) = self.transport.receive().map_err(|Closed| stopped())? else {
                break;
            };
            let Some((_, pdu)) = self.decode.decode(&datagram) else {
                continue;
            };
            match self.translator.inbound(&pdu) {
                Ok(message) => {
                    into.push(message);
                    taken = taken.saturating_add(1);
                    self.delivered = self.delivered.saturating_add(1);
                }
                Err(skip) => self.skips.count(skip),
            }
        }
        Ok(())
    }

    /// The counters in the ABI's shape: `tx_dropped` (queue full, oversized, failed send, message
    /// not sendable), `rx_dropped` (queue full, oversized), `decode_errors` (malformed, other
    /// version, unknown PDU type, another exercise, unusable content) and `io_errors`.
    #[must_use]
    pub fn abi_counters(&self) -> Counters {
        let t = self.transport.counters();
        let d = &self.decode;
        let mut c = Counters::default();
        c.tx_dropped = t.tx_dropped.saturating_add(self.rejected_out);
        c.rx_dropped = t.rx_dropped;
        c.decode_errors = [
            d.malformed,
            d.unsupported_version,
            d.skipped_unsupported_type(),
            self.skips.total(),
        ]
        .into_iter()
        .fold(0, u64::saturating_add);
        c.io_errors = t.io_errors;
        c
    }
}

impl NetworkAdapter for DisAdapter {
    fn publish(&mut self, now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error> {
        self.publish_at(
            Stamp {
                tick: now.tick(),
                rate_hz: now.rate_hz(),
            },
            out,
        )
    }

    fn poll(&mut self, _now: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error> {
        let mut received = std::mem::take(&mut self.scratch);
        received.clear();
        let result = self
            .poll_into(inbox.remaining(), &mut received)
            .and_then(|()| {
                received
                    .iter()
                    .try_for_each(|message| inbox.push(&message.message()))
            });
        self.scratch = received;
        result
    }

    fn counters(&self) -> Counters {
        self.abi_counters()
    }

    fn stop(&mut self) {
        self.transport.stop();
    }
}
