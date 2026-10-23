//! A recording `InboundTap` and a logging ghost world for the WI-12 tests.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::dead_reckoning::RemoteState;
use umbra_core::net::{AdapterCounters, AdapterStart, KeySpace, NetError, NetworkAdapterConfig};
use umbra_core::net::{NetMessage, NetworkAdapter};
use umbra_core::{EntityId, SimTime};
use umbra_net::{GhostError, GhostSpec, GhostWorld, InboundTap};

/// What the tap heard, and what the world was asked, in one shared order.
#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    Polled {
        tick: u64,
        adapter: String,
        messages: Vec<NetMessage>,
    },
    Faulted {
        tick: u64,
        adapter: String,
        call: String,
    },
    WorldCall(&'static str),
}

pub type Log = Arc<Mutex<Vec<Seen>>>;

pub fn log() -> Log {
    Arc::default()
}

pub fn snapshot(log: &Log) -> Vec<Seen> {
    guard(log).clone()
}

fn guard(log: &Log) -> MutexGuard<'_, Vec<Seen>> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct RecTap(pub Log);

impl InboundTap for RecTap {
    fn polled(&mut self, tick: u64, adapter: &str, messages: &[NetMessage]) {
        guard(&self.0).push(Seen::Polled {
            tick,
            adapter: adapter.to_owned(),
            messages: messages.to_vec(),
        });
    }

    fn faulted(&mut self, tick: u64, adapter: &str, call: &str) {
        guard(&self.0).push(Seen::Faulted {
            tick,
            adapter: adapter.to_owned(),
            call: call.to_owned(),
        });
    }
}

/// Wraps a world and logs each mutating call into the tap's log.
pub struct LoggingWorld<W: GhostWorld>(pub W, pub Log);

impl<W: GhostWorld> GhostWorld for LoggingWorld<W> {
    fn create_ghost(&mut self, spec: GhostSpec<'_>) -> Result<EntityId, GhostError> {
        guard(&self.1).push(Seen::WorldCall("create"));
        self.0.create_ghost(spec)
    }

    fn update_ghost(&mut self, id: EntityId, remote: RemoteState) -> Result<(), GhostError> {
        guard(&self.1).push(Seen::WorldCall("update"));
        self.0.update_ghost(id, remote)
    }

    fn remove_ghost(&mut self, id: EntityId) -> Result<(), GhostError> {
        guard(&self.1).push(Seen::WorldCall("remove"));
        self.0.remove_ghost(id)
    }
}

/// An adapter config whose poll, from tick `fail_from` on, delivers `message` and then fails.
/// Before that it delivers nothing. Publishes succeed.
pub struct PartialThenFail {
    pub message: NetMessage,
    pub fail_from: u64,
    pub key_space: KeySpace,
}

struct PartialThenFailAdapter {
    message: NetMessage,
    fail_from: u64,
}

impl NetworkAdapterConfig for PartialThenFail {
    fn key_space(&self) -> KeySpace {
        self.key_space
    }

    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Ok(Box::new(PartialThenFailAdapter {
            message: self.message.clone(),
            fail_from: self.fail_from,
        }))
    }
}

impl NetworkAdapter for PartialThenFailAdapter {
    fn publish(&mut self, _: SimTime, _: &[NetMessage]) -> Result<(), NetError> {
        Ok(())
    }

    fn poll(&mut self, now: SimTime, _: usize, into: &mut Vec<NetMessage>) -> Result<(), NetError> {
        if now.tick() < self.fail_from {
            return Ok(());
        }
        into.push(self.message.clone());
        Err(NetError::Io("cut".to_owned()))
    }

    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }

    fn stop(&mut self) {}
}
