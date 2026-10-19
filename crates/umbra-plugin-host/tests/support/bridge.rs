//! Bridge-level helpers (WI-9 tests): an environment with the raw plugin loaded and registered, and
//! entities that can be ticked through the plugin's batch or one by one.

use std::sync::Arc;

use umbra_core::{
    Behavior, BehaviorBatch, BehaviorContext, BehaviorInstance, BuildError, ComponentId, Ecef,
    EntityId, EntityView, Intent, IntentBuffer, KinematicState, Quaternion, Registry, RngStream,
    SimRng, SimTime, Status, StreamKey, TickRate, Vec3,
};
use umbra_plugin_host::{HostOptions, PluginHost};

use super::forge::Forged;
use super::{build_behavior, raw};

/// The raw plugin loaded into a host and registered.
pub struct Env {
    pub host: PluginHost,
    pub registry: Registry,
    pub f: Forged,
}

/// An [`Env`] over the raw plugin published by the caller's own `forged!`.
///
/// One call site per test (see `forge.rs`): use the macros `raw_env!()` and `raw_env_with!(options,
/// with_halt_running)`, which expand `forged!` in the test function. A shared helper function would
/// give every test the same image key and the same counters.
macro_rules! raw_env_with {
    ($options:expr, $with_halt_running:expr) => {{
        let f = forged!($crate::support::raw::parts($with_halt_running));
        $crate::support::bridge::Env::load(f, $options)
    }};
}

/// [`raw_env_with!`] with default options and a `halt_running` entry.
macro_rules! raw_env {
    () => {
        raw_env_with!(umbra_plugin_host::HostOptions::default(), true)
    };
}

impl Drop for Env {
    /// Problems the raw plugin met inside its `extern "C"` callbacks (it cannot panic there).
    fn drop(&mut self) {
        let failures = raw::failures();
        assert!(
            failures.is_empty() || std::thread::panicking(),
            "raw plugin callbacks failed: {failures:?}"
        );
    }
}

impl Env {
    /// Loads the forged raw plugin into a new host and registers it.
    pub fn load(f: Forged, options: HostOptions) -> Self {
        let mut host = PluginHost::new(options);
        host.load_static("bridge-origin", f.entry)
            .expect("load raw plugin");
        let mut registry = Registry::new();
        host.register(&mut registry).expect("register raw plugin");
        Self { host, registry, f }
    }

    /// Builds a configured behavior in `mode` (see `raw`).
    pub fn try_behavior(&self, mode: &str) -> Result<Arc<dyn Behavior>, BuildError> {
        let addr = self.f.host.load(std::sync::atomic::Ordering::SeqCst);
        build_behavior(
            &self.host,
            &self.registry,
            raw::CAPABILITY,
            &raw::params(mode, addr),
        )
    }

    pub fn behavior(&self, mode: &str) -> Arc<dyn Behavior> {
        self.try_behavior(mode).expect("build the raw behavior")
    }

    pub fn id() -> ComponentId {
        ComponentId::new(raw::CAPABILITY).expect("valid id")
    }
}

pub fn rate() -> TickRate {
    TickRate::new(30).expect("non-zero")
}

pub fn eid(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

pub fn state(x: f64) -> KinematicState {
    KinematicState {
        position: Ecef(Vec3 {
            x: 6_378_137.0 + x,
            y: 1.0,
            z: 2.0,
        }),
        velocity: Vec3 {
            x: 0.5,
            y: 0.25,
            z: 0.0,
        },
        orientation: Quaternion::IDENTITY,
    }
}

/// One entity: its state, own RNG stream, intent buffer and plugin instance.
pub struct Ent {
    pub id: EntityId,
    pub state: KinematicState,
    pub rng: RngStream,
    pub buf: Vec<Intent>,
    pub inst: Box<dyn BehaviorInstance>,
    pub status: Option<Status>,
}

impl Ent {
    pub fn new(n: u64, behavior: &Arc<dyn Behavior>, seed: u64) -> Self {
        let id = eid(n);
        Self {
            id,
            state: state(f64::from(u32::try_from(n).unwrap_or(0))),
            rng: SimRng::new(seed).stream(StreamKey::entity(id)),
            buf: Vec::new(),
            inst: behavior.instantiate(rate()).expect("instantiate"),
            status: None,
        }
    }

    /// The first value of this entity's stream, as the raw plugin reports it (`SetSpeed`).
    pub fn expected_speed(n: u64, seed: u64) -> f64 {
        let mut s = SimRng::new(seed).stream(StreamKey::entity(eid(n)));
        raw::speed_of(s.next_u64())
    }
}

/// Ticks `ents` as one batch (the kernel's grouped path).
pub fn tick_batched(batch: &Arc<dyn BehaviorBatch>, ents: &mut [Ent], tick: u64) {
    let now = SimTime::at_tick(rate(), tick);
    let mut items: Vec<umbra_core::BatchItem<'_>> = ents
        .iter_mut()
        .map(|e| {
            e.buf.clear();
            let Ent {
                id,
                state,
                rng,
                buf,
                inst,
                ..
            } = e;
            umbra_core::BatchItem::new(
                BehaviorContext::new(now, rng),
                EntityView::new(*id, state),
                inst.as_mut(),
                IntentBuffer::new(buf),
            )
        })
        .collect();
    batch.tick_batch(&mut items);
    let statuses: Vec<Status> = items.iter().map(|i| i.status).collect();
    drop(items);
    for (e, s) in ents.iter_mut().zip(statuses) {
        e.status = Some(s);
    }
}

/// Ticks `ents` one by one through `BehaviorInstance::tick` (the reference path).
pub fn tick_singly(ents: &mut [Ent], tick: u64) {
    let now = SimTime::at_tick(rate(), tick);
    for e in ents {
        e.buf.clear();
        let mut ctx = BehaviorContext::new(now, &mut e.rng);
        let view = EntityView::new(e.id, &e.state);
        let mut out = IntentBuffer::new(&mut e.buf);
        e.status = Some(e.inst.tick(&mut ctx, &view, &mut out));
    }
}
