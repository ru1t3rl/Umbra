//! AT-1 (kernel rows): `Kernel::remove`. Requirements: REQ-C-2, REQ-K-4, REQ-K-5, ADR 0028,
//! ADR 0020, review m14.
//!
//! Removal happens between ticks. It drops the entity, its behavior instance and its batch
//! membership; ids are never reused; everything else evolves bit-identically. The plugin row
//! (instance destroyed outside a tick) is in `umbra-engine/tests/plugin_remove.rs`: the kernel
//! may not use `umbra-plugin-host`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod agents;
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use umbra_core::{
    BatchItem, BbValue, Behavior, BehaviorBatch, BehaviorContext, BehaviorId, BehaviorInstance,
    BlackboardError, EntityId, EntityView, InstantiateError, Intent, IntentBuffer, MetersPerSecond,
    Status, TickRate, Vec3, tick_each,
};
use umbra_kernel::{AgentInit, Kernel, KernelError};

use agents::RangeSensor;
use common::{Behavior as Motion, Stub, at, kernel, owned};

fn id(n: u64) -> EntityId {
    EntityId::new(n).expect("non-zero")
}

fn agent(b: Arc<dyn Behavior>) -> AgentInit {
    AgentInit {
        behavior: Some(b),
        ..AgentInit::default()
    }
}

/// Counts live instances: +1 on `instantiate`, -1 when an instance is dropped. Draws from the
/// entity stream and emits `SetSpeed`, so a stream that shifted would show in the state.
struct Counted {
    id: BehaviorId,
    live: Arc<AtomicUsize>,
    batch: Option<Arc<dyn BehaviorBatch>>,
}

struct CountedInstance {
    live: Arc<AtomicUsize>,
}

impl Drop for CountedInstance {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

impl BehaviorInstance for CountedInstance {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        #[expect(clippy::cast_precision_loss, reason = "draw is below 100")]
        out.push(Intent::SetSpeed(MetersPerSecond(
            (ctx.rng.next_u64() % 100) as f64,
        )));
        Status::Running
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

impl Behavior for Counted {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        self.live.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(CountedInstance {
            live: Arc::clone(&self.live),
        }))
    }
    fn batch(&self) -> Option<Arc<dyn BehaviorBatch>> {
        self.batch.clone()
    }
}

fn counted(live: &Arc<AtomicUsize>, batch: Option<Arc<dyn BehaviorBatch>>) -> Arc<dyn Behavior> {
    Arc::new(Counted {
        id: BehaviorId::new("test.behavior.counted").expect("valid id"),
        live: Arc::clone(live),
        batch,
    })
}

/// A batch that records the member ids of each call, then ticks each item.
struct Recording(Arc<Mutex<Vec<Vec<EntityId>>>>);

impl BehaviorBatch for Recording {
    fn tick_batch(&self, items: &mut [BatchItem<'_>]) {
        let members = items.iter().map(|i| i.entity.id).collect();
        self.0.lock().expect("log lock").push(members);
        tick_each(items);
    }
}

fn drifting() -> Arc<Stub> {
    Stub::new("test.model.drift", Motion::Drift(Vec3::new(0.0, 3.0, 0.0)))
}

/// Spawns `n` drifting counted entities (ids 1..=n), 20 m apart.
fn world(
    n: u32,
    seed: u64,
    live: &Arc<AtomicUsize>,
    batch: Option<&Arc<dyn BehaviorBatch>>,
) -> Kernel {
    let model = drifting();
    let mut k = kernel(seed);
    for i in 0..n {
        k.spawn_with(
            owned(at(f64::from(i) * 20.0), &model),
            agent(counted(live, batch.map(Arc::clone))),
        )
        .expect("spawn_with");
    }
    k
}

/// REQ-C-2, ADR 0028: removing an entity drops it from the world and drops its behavior
/// instance at once, between ticks.
#[test]
fn remove_drops_entity_and_its_behavior_instance() {
    let live = Arc::new(AtomicUsize::new(0));
    let mut k = world(3, 1, &live, None);
    k.tick().expect("tick");
    assert_eq!(live.load(Ordering::SeqCst), 3);

    assert_eq!(k.remove(id(2)), Ok(()));

    assert_eq!(live.load(Ordering::SeqCst), 2, "the instance was dropped");
    assert_eq!(k.entity_count(), 2);
    let ids: Vec<_> = k.snapshot().entities.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![id(1), id(3)]);
    k.tick().expect("tick after remove");
}

/// REQ-C-2: ids are never reused, whether the highest or a middle id was removed.
#[test]
fn ids_are_never_reused_after_remove() {
    let model = drifting();
    let mut k = kernel(1);
    for i in 0..3 {
        k.spawn(owned(at(f64::from(i) * 5.0), &model))
            .expect("spawn");
    }
    k.remove(id(3)).expect("remove the highest");
    k.remove(id(1)).expect("remove the first");

    let next = k.spawn(owned(at(50.0), &model)).expect("spawn");

    assert_eq!(next, id(4));
    let ids: Vec<_> = k.snapshot().entities.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![id(2), id(4)]);
}

/// Removing an id that is not in the world (never spawned, or already removed) is an error and
/// changes nothing.
#[test]
fn remove_unknown_is_unknown_entity() {
    let model = drifting();
    let mut k = kernel(1);
    k.spawn(owned(at(0.0), &model)).expect("spawn");
    let before = k.state_hash();

    assert_eq!(k.remove(id(9)), Err(KernelError::UnknownEntity(id(9))));
    assert_eq!(k.remove(id(1)), Ok(()));
    assert_eq!(k.remove(id(1)), Err(KernelError::UnknownEntity(id(1))));

    assert_eq!(k.entity_count(), 0);
    assert_ne!(k.state_hash(), before);
    assert!(
        KernelError::UnknownEntity(id(9)).to_string().contains('9'),
        "the message names the id"
    );
}

/// REQ-K-4: removal between ticks leaves every other entity bit-identical to a twin kernel that
/// did not remove: state, intents and RNG stream (the behavior draws every tick).
#[test]
fn remove_between_ticks_keeps_other_entities_bit_identical() {
    let live = Arc::new(AtomicUsize::new(0));
    let mut with = world(4, 11, &live, None);
    let mut twin = world(4, 11, &live, None);
    for _ in 0..5 {
        with.tick().expect("tick");
        twin.tick().expect("tick");
    }

    with.remove(id(2)).expect("remove");

    for tick in 6..=30 {
        with.tick().expect("tick");
        twin.tick().expect("tick");
        let kept: Vec<_> = twin
            .snapshot()
            .entities
            .into_iter()
            .filter(|e| e.id != id(2))
            .collect();
        assert_eq!(with.snapshot().entities, kept, "entities at tick {tick}");
    }
}

/// An entity with sensors can be removed; observers and contacts keep working.
#[test]
fn remove_a_sensor_owner_leaves_the_others_sensing() {
    let model = drifting();
    let sensor = Arc::new(RangeSensor::new("test.sensor.range", 1_000.0));
    let mut k = kernel(1);
    for i in 0..2 {
        k.spawn_with(
            owned(at(f64::from(i) * 10.0), &model),
            AgentInit {
                sensors: vec![sensor.clone()],
                ..AgentInit::default()
            },
        )
        .expect("spawn");
    }
    k.tick().expect("tick");

    k.remove(id(1)).expect("remove the observer");
    k.tick().expect("tick");

    let entities = k.snapshot().entities;
    assert_eq!(entities.len(), 1);
    assert!(
        entities[0].contacts.is_empty(),
        "the removed entity is no contact"
    );
}

/// ADR 0020, review m14: a removed member leaves its batch group. The next grouped phase makes one
/// call without it, ids ascending, and the remaining members are bit-identical to a twin that
/// kept it. Removing the last member ends the calls.
#[test]
fn remove_member_of_a_batch_group_keeps_the_group_consistent() {
    let live = Arc::new(AtomicUsize::new(0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let batch: Arc<dyn BehaviorBatch> = Arc::new(Recording(Arc::clone(&log)));
    let mut with = world(4, 5, &live, Some(&batch));
    let mut twin = world(4, 5, &live, Some(&batch));
    with.tick().expect("tick");
    twin.tick().expect("tick");

    with.remove(id(2)).expect("remove");
    log.lock().expect("log lock").clear();
    with.tick().expect("tick");
    twin.tick().expect("tick");

    assert_eq!(
        log.lock().expect("log lock").clone(),
        vec![vec![id(1), id(3), id(4)], vec![id(1), id(2), id(3), id(4)]],
        "one call per kernel per tick; the first kernel without the removed member"
    );
    let kept: Vec<_> = twin
        .snapshot()
        .entities
        .into_iter()
        .filter(|e| e.id != id(2))
        .collect();
    assert_eq!(with.snapshot().entities, kept);

    for n in [1, 3, 4] {
        with.remove(id(n)).expect("remove");
    }
    log.lock().expect("log lock").clear();
    with.tick().expect("tick with no members");
    assert!(
        log.lock().expect("log lock").is_empty(),
        "no member, no call"
    );
    assert_eq!(
        live.load(Ordering::SeqCst),
        4,
        "the twin's four instances remain"
    );
}

fn sensing(sensor: &Arc<RangeSensor>) -> AgentInit {
    AgentInit {
        sensors: vec![sensor.clone()],
        ..AgentInit::default()
    }
}

/// Removing an entity without sensors must not touch the sensor-owner count: observer 1 keeps
/// sensing, and sees exactly the remaining entity 3.
#[test]
fn remove_a_plain_entity_keeps_perception_running() {
    let model = drifting();
    let sensor = Arc::new(RangeSensor::new("test.sensor.range", 1_000.0));
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), sensing(&sensor))
        .expect("spawn observer");
    k.spawn(owned(at(10.0), &model)).expect("spawn");
    k.spawn(owned(at(20.0), &model)).expect("spawn");
    k.tick().expect("tick");

    k.remove(id(2)).expect("remove a plain entity");
    k.tick().expect("tick");

    let contacts: Vec<_> = k.snapshot().entities[0]
        .contacts
        .iter()
        .map(|c| c.entity)
        .collect();
    assert_eq!(contacts, vec![id(3)]);
    // The contact scrub alone would give the same list; the sensor must really have run again.
    let seen = sensor.records();
    assert_eq!(seen.len(), 2, "the sensor ran in both ticks: {seen:?}");
    assert_eq!(seen[1].candidates, vec![id(3)]);
}

/// ASM-170 (c): no contact list names the removed id right after `remove`, before any tick.
#[test]
fn remove_scrubs_the_id_from_every_contact_list() {
    let model = drifting();
    let sensor = Arc::new(RangeSensor::new("test.sensor.range", 1_000.0));
    let mut k = kernel(1);
    k.spawn_with(owned(at(0.0), &model), sensing(&sensor))
        .expect("spawn");
    k.spawn_with(owned(at(10.0), &model), sensing(&sensor))
        .expect("spawn");
    k.spawn(owned(at(20.0), &model)).expect("spawn");
    k.tick().expect("tick");
    let before: Vec<_> = k
        .snapshot()
        .entities
        .iter()
        .map(|e| e.contacts.len())
        .collect();
    assert_eq!(
        before,
        vec![2, 2, 0],
        "setup: both observers see the others"
    );

    k.remove(id(3)).expect("remove");

    for e in k.snapshot().entities {
        assert!(
            e.contacts.iter().all(|c| c.entity != id(3)),
            "{:?} still lists the removed entity",
            e.id
        );
        assert_eq!(e.contacts.len(), 1, "the other contact stays: {:?}", e.id);
    }
}

/// ASM-170 (d): removing a batched entity releases the kernel's hold on the group at once, not at
/// the next tick (a plugin batch would otherwise stay alive until then).
#[test]
fn remove_a_batched_entity_releases_the_batch_between_ticks() {
    let live = Arc::new(AtomicUsize::new(0));
    let batch: Arc<dyn BehaviorBatch> = Arc::new(Recording(Arc::new(Mutex::new(Vec::new()))));
    let mut k = world(1, 1, &live, Some(&batch));
    k.tick().expect("tick");
    assert_eq!(
        Arc::strong_count(&batch),
        3,
        "test, slot and the tick's group list"
    );

    k.remove(id(1)).expect("remove");

    assert_eq!(
        Arc::strong_count(&batch),
        1,
        "the kernel holds nothing after remove"
    );
}
