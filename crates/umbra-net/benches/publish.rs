//! Publish path benchmarks (M6 WI-18, NFR-5 groundwork, ASM-410).
//!
//! 1000 owned entities moving deterministically (entity `i` advances `0.05 * (i % 10 + 1)` m per
//! tick, so with a 1 m threshold each crosses it every 2 to 20 ticks, staggered by index):
//! the bare [`PublishTracker`], a whole [`NetworkManager`] tick (publish and ingest) with one
//! loopback adapter, and with two adapters, so the per-adapter cost is visible. Gate 14 only
//! checks that this compiles; there is no stored baseline.

#![forbid(unsafe_code)]
#![expect(
    clippy::expect_used,
    reason = "benchmark setup with known-valid values"
)]

use std::collections::BTreeMap;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use umbra_core::dead_reckoning::{DeadReckoning, RemoteState};
use umbra_core::net::{KeySpace, OwnedView};
use umbra_core::{
    ComponentId, Ecef, EntityId, Force, KinematicState, Quaternion, Radians, Seconds, SimEvent,
    SimTime, TickRate, Vec3,
};
use umbra_net::{
    AdapterSettings, AdapterSlot, GhostError, GhostPolicy, GhostSpec, GhostWorld, LoopbackHub,
    NetworkManager, PublishPolicy, PublishTracker, TypeMap,
};

const A: f64 = 6_378_137.0;
const ENTITIES: u32 = 1000;

fn rate() -> TickRate {
    TickRate::new(30).expect("non-zero rate")
}

fn tick(n: u64) -> SimTime {
    SimTime::at_tick(rate(), n)
}

fn policy() -> PublishPolicy {
    PublishPolicy {
        position_threshold_m: 1.0,
        orientation_threshold: Radians(3.0_f64.to_radians()),
        heartbeat: Seconds(5.0),
        min_interval: Seconds(0.0),
        model: DeadReckoning::Velocity,
        control: false,
    }
}

/// The 1000 owned entities at tick `n`.
fn views(n: u32) -> Vec<OwnedView> {
    let force = Force::new("blue").ok();
    (1..=ENTITIES)
        .map(|i| {
            let step = 0.05 * f64::from(i % 10 + 1);
            OwnedView {
                id: EntityId::new(u64::from(i)).expect("non-zero"),
                state: KinematicState {
                    position: Ecef(Vec3::new(A, f64::from(i) * 10.0 + step * f64::from(n), 0.0)),
                    velocity: Vec3::ZERO,
                    orientation: Quaternion::IDENTITY,
                },
                force: force.clone(),
            }
        })
        .collect()
}

/// A ghost world that never receives anything (nothing is mapped, so no ghost is created).
struct NoGhosts;

impl GhostWorld for NoGhosts {
    fn create_ghost(&mut self, _spec: GhostSpec<'_>) -> Result<EntityId, GhostError> {
        Err(GhostError::World("bench world".to_owned()))
    }
    fn update_ghost(&mut self, _id: EntityId, _remote: RemoteState) -> Result<(), GhostError> {
        Ok(())
    }
    fn remove_ghost(&mut self, _id: EntityId) -> Result<(), GhostError> {
        Ok(())
    }
}

/// A manager with `adapters` loopback adapters on one hub; each hears the others.
fn manager(adapters: u64) -> NetworkManager {
    let hub = LoopbackHub::new(8192);
    let slots = (0..adapters)
        .map(|k| {
            let name = format!("lb{k}");
            let settings = AdapterSettings {
                name: name.clone(),
                enabled: true,
                publish: policy(),
                ghosts: GhostPolicy {
                    timeout: Seconds(1.0),
                    max: 1000,
                    max_inbound_per_tick: 1000,
                },
            };
            let id = ComponentId::new(&format!("bench.net.{name}")).expect("valid component id");
            let space = KeySpace {
                base: 1_000_000 * (k + 1),
                count: 2000,
            };
            let input = serde_json::json!({"name": name, "params": {}, "templates": {}});
            let config = hub.factory(id, space).build(&input).expect("valid input");
            AdapterSlot::new(settings, config, TypeMap::default(), BTreeMap::new())
        })
        .collect();
    NetworkManager::new(slots, tick(0)).expect("loopback adapters start")
}

fn bench_tracker(c: &mut Criterion) {
    let mut tracker = PublishTracker::new(policy());
    let mut n = 0_u32;
    c.bench_function("publish_tracker_evaluate_1000", |b| {
        b.iter_batched(
            || {
                n += 1;
                (tick(u64::from(n)), views(n))
            },
            |(now, owned)| {
                let sent = owned
                    .iter()
                    .filter(|v| tracker.evaluate(now, v.id, &v.state).is_some())
                    .count();
                std::hint::black_box(sent)
            },
            BatchSize::LargeInput,
        );
    });
}

fn bench_manager(c: &mut Criterion, name: &str, adapters: u64) {
    let mut manager = manager(adapters);
    let meta = BTreeMap::new();
    let events: Vec<SimEvent> = Vec::new();
    let mut world = NoGhosts;
    let mut n = 0_u32;
    c.bench_function(name, |b| {
        b.iter_batched(
            || {
                n += 1;
                (tick(u64::from(n)), views(n))
            },
            |(now, owned)| {
                let mut out = Vec::new();
                manager.ingest(now, &mut world, &mut out);
                manager.publish(now, &owned, &meta, &events);
                std::hint::black_box(out.len())
            },
            BatchSize::LargeInput,
        );
    });
}

fn benches(c: &mut Criterion) {
    bench_tracker(c);
    bench_manager(c, "manager_tick_1000_one_adapter", 1);
    bench_manager(c, "manager_tick_1000_two_adapters", 2);
}

criterion_group!(group, benches);
criterion_main!(group);
