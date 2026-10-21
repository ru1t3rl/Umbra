//! Helpers for the network engine tests (M6 step 4): the `net_pair` fixtures, a registry holding
//! loopback factories, and per-engine stepping with event capture. Each test binary uses a subset.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::time::Duration;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;

use umbra_core::net::KeySpace;
use umbra_core::{EntityId, Registry, SimEvent, StateHash};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::scenario::{RunOptions, ScenarioRun, instantiate, load_scenario};
use umbra_engine::{ClockMode, Engine, EntitySnapshot, ManualWallClock, Ownership};
use umbra_net::LoopbackHub;

pub fn fixture(dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/net_pair")
        .join(dir)
        .join("scenario.toml")
}

/// The built-in registry plus one loopback adapter factory per `(component id, hub, key base)`;
/// every endpoint gets 100 keys.
pub fn registry_with(adapters: &[(&str, &Arc<LoopbackHub>, u64)]) -> Registry {
    let mut registry = builtin_registry().expect("the built-in registry builds");
    for (id, hub, base) in adapters {
        let factory = hub.factory(
            umbra_core::ComponentId::new(id).expect("valid id"),
            KeySpace {
                base: *base,
                count: 100,
            },
        );
        registry
            .register_network_adapter(factory)
            .expect("register loopback");
    }
    registry
}

/// Loads and instantiates `net_pair/<dir>` on a manual wall clock.
pub fn start(dir: &str, registry: &Registry, clock: Option<ClockMode>) -> ScenarioRun {
    let scenario = load_scenario(registry, &fixture(dir))
        .unwrap_or_else(|e| panic!("loading net_pair/{dir}: {e}"));
    instantiate(
        registry,
        &scenario,
        RunOptions {
            wall: Box::new(ManualWallClock::with_auto_advance(Duration::from_micros(
                500,
            ))),
            seed: None,
            clock,
        },
    )
    .unwrap_or_else(|e| panic!("instantiating net_pair/{dir}: {e}"))
}

/// One engine and what it has reported.
pub struct Peer {
    pub engine: Engine,
    pub events: Vec<Vec<SimEvent>>,
    pub hashes: Vec<StateHash>,
}

impl Peer {
    pub fn new(run: ScenarioRun) -> (Self, Vec<umbra_engine::EntityInfo>) {
        let infos = run.entities.clone();
        (
            Self {
                engine: run.engine,
                events: Vec::new(),
                hashes: Vec::new(),
            },
            infos,
        )
    }

    /// Runs one tick, recording its events and the state hash after it.
    pub fn tick(&mut self) {
        let (events, hashes) = (&mut self.events, &mut self.hashes);
        self.engine
            .step(1, |view| {
                events.push(view.report.events.clone());
                hashes.push(view.state_hash());
                ControlFlow::Continue(())
            })
            .expect("step");
    }

    pub fn ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    pub fn ghosts(&self) -> Vec<EntitySnapshot> {
        self.engine
            .snapshot()
            .entities
            .into_iter()
            .filter(|e| !e.ownership.is_owned())
            .collect()
    }

    pub fn owned(&self) -> Vec<EntitySnapshot> {
        self.engine
            .snapshot()
            .entities
            .into_iter()
            .filter(|e| e.ownership.is_owned())
            .collect()
    }

    pub fn entity_count(&self) -> usize {
        self.engine.snapshot().entities.len()
    }

    /// Every event of every tick so far, in order.
    pub fn all_events(&self) -> Vec<&SimEvent> {
        self.events.iter().flatten().collect()
    }
}

pub fn is_remote_from(e: &EntitySnapshot, source: &str) -> bool {
    matches!(&e.ownership, Ownership::Remote { source: s } if s.as_str() == source)
}

pub fn id_of(infos: &[umbra_engine::EntityInfo], name: &str) -> EntityId {
    infos
        .iter()
        .find(|i| i.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no entity named {name}"))
        .id
}

pub fn distance(a: umbra_core::Ecef, b: umbra_core::Ecef) -> f64 {
    let (dx, dy, dz) = (a.0.x - b.0.x, a.0.y - b.0.y, a.0.z - b.0.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// How many different hashes a trace holds.
pub fn distinct(hashes: &[StateHash]) -> usize {
    hashes
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}
