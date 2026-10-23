//! The replay source: network adapters that return recorded inbound messages and open no sockets
//! (REQ-K-7, ADR 0054, ASM-628).
//!
//! A [`ReplaySource`] holds, per adapter name and tick, the messages the recording saw and the
//! faults it saw. An adapter built from [`ReplaySource::factory`] hands out the messages of the
//! tick it is polled for (at most `max`; the rest stay for the next poll of that tick), discards
//! everything published, and fails the `poll`, `publish` or `control` the recording says failed at
//! that tick. It reads no clock and no socket: delivery depends on the tick only.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetError, NetMessage, NetworkAdapter,
    NetworkAdapterConfig,
};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, SimTime};

/// The factories' component id (replay replaces whatever adapter the recording used).
const FACTORY_ID: &str = "builtin.net.replay";

/// The factories take any object; the recorded run already validated the real parameters.
const PARAM_SCHEMA: &str = r#"{ "type": "object" }"#;

/// A poisoned lock only means another thread panicked mid-update; the maps stay valid.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What one adapter does at one tick.
#[derive(Debug, Default)]
struct TickPlan {
    messages: VecDeque<NetMessage>,
    poll_fault: bool,
    publish_fault: bool,
    control_fault: bool,
}

type Plans = BTreeMap<String, BTreeMap<u64, TickPlan>>;

/// Holds the recorded inbound messages and faults per tick and adapter.
#[derive(Debug, Default)]
pub struct ReplaySource {
    plans: Mutex<Plans>,
}

impl ReplaySource {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A factory for the adapter named `name` that replays from this source with the recorded
    /// `key_space`. Its adapters discard `publish`.
    #[must_use]
    pub fn factory(
        self: &Arc<Self>,
        name: &str,
        key_space: KeySpace,
    ) -> Arc<dyn Factory<dyn NetworkAdapterConfig>> {
        Arc::new(ReplayFactory {
            source: Arc::clone(self),
            adapter: name.to_owned(),
            key_space,
            id: ComponentId::new(FACTORY_ID)
                .unwrap_or_else(|e| unreachable!("the constant id is valid: {e}")),
            schema: ParamSchema::from_json_str(PARAM_SCHEMA)
                .unwrap_or_else(|e| unreachable!("the constant schema is valid JSON: {e}")),
        })
    }

    /// Records that `adapter` delivers `messages` when polled for `tick`. Calling it again for
    /// the same tick and adapter appends (the recorder splits large batches, ASM-727).
    pub fn push(&self, tick: u64, adapter: &str, messages: Vec<NetMessage>) {
        self.with_plan(tick, adapter, |p| p.messages.extend(messages));
    }

    /// Records that `adapter` faults during `call` at `tick`: `poll` makes its poll fail (after it
    /// delivered the tick's messages, as a live poll may), `publish` its publish of entity
    /// traffic, `control` its publish of run control; any other call is ignored (a fault the
    /// replay cannot reproduce).
    pub fn fault_at(&self, tick: u64, adapter: &str, call: &str) {
        self.with_plan(tick, adapter, |p| match call {
            "poll" => p.poll_fault = true,
            "publish" => p.publish_fault = true,
            "control" => p.control_fault = true,
            _ => {}
        });
    }

    fn with_plan(&self, tick: u64, adapter: &str, f: impl FnOnce(&mut TickPlan)) {
        let mut plans = lock(&self.plans);
        f(plans
            .entry(adapter.to_owned())
            .or_default()
            .entry(tick)
            .or_default());
    }
}

struct ReplayFactory {
    source: Arc<ReplaySource>,
    adapter: String,
    key_space: KeySpace,
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn NetworkAdapterConfig> for ReplayFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, _input: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        Ok(Arc::new(ReplayConfig {
            source: Arc::clone(&self.source),
            adapter: self.adapter.clone(),
            key_space: self.key_space,
            started: AtomicBool::new(false),
        }))
    }
}

struct ReplayConfig {
    source: Arc<ReplaySource>,
    adapter: String,
    key_space: KeySpace,
    started: AtomicBool,
}

impl NetworkAdapterConfig for ReplayConfig {
    fn key_space(&self) -> KeySpace {
        self.key_space
    }

    fn start(&self, _ctx: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Err(NetError::AlreadyStarted);
        }
        Ok(Box::new(ReplayAdapter {
            source: Arc::clone(&self.source),
            adapter: self.adapter.clone(),
            stopped: false,
        }))
    }
}

struct ReplayAdapter {
    source: Arc<ReplaySource>,
    adapter: String,
    stopped: bool,
}

impl ReplayAdapter {
    fn fault(&self, call: &str) -> NetError {
        if self.stopped {
            NetError::Faulted("replay adapter stopped".to_owned())
        } else {
            NetError::Faulted(format!("recorded {call} fault"))
        }
    }

    fn recorded_publish_fault(&self, tick: u64, out: &[NetMessage]) -> bool {
        // Only `publish_control` sends run control (it sends nothing else), so a batch with a
        // run-control message is the "control" call and any other batch the "publish" call.
        let control = out
            .iter()
            .any(|m| matches!(m, NetMessage::SimulationControl(_)));
        lock(&self.source.plans)
            .get(&self.adapter)
            .and_then(|ticks| ticks.get(&tick))
            .is_some_and(|p| {
                if control {
                    p.control_fault
                } else {
                    p.publish_fault
                }
            })
    }

    fn recorded_poll_fault(&self, tick: u64) -> bool {
        lock(&self.source.plans)
            .get(&self.adapter)
            .and_then(|ticks| ticks.get(&tick))
            .is_some_and(|p| p.poll_fault)
    }
}

impl NetworkAdapter for ReplayAdapter {
    /// Discards `out` (ASM-628); fails only when stopped or when the recording says so.
    fn publish(&mut self, now: SimTime, out: &[NetMessage]) -> Result<(), NetError> {
        if self.stopped || self.recorded_publish_fault(now.tick(), out) {
            return Err(self.fault("publish"));
        }
        Ok(())
    }

    fn poll(
        &mut self,
        now: SimTime,
        max: usize,
        into: &mut Vec<NetMessage>,
    ) -> Result<(), NetError> {
        if self.stopped {
            return Err(self.fault("poll"));
        }
        let fails = self.recorded_poll_fault(now.tick());
        let mut plans = lock(&self.source.plans);
        if let Some(plan) = plans
            .get_mut(&self.adapter)
            .and_then(|ticks| ticks.get_mut(&now.tick()))
        {
            let n = max.min(plan.messages.len());
            into.extend(plan.messages.drain(..n));
        }
        if fails {
            return Err(self.fault("poll"));
        }
        Ok(())
    }

    fn counters(&self) -> AdapterCounters {
        AdapterCounters::default()
    }

    fn stop(&mut self) {
        self.stopped = true;
    }
}
