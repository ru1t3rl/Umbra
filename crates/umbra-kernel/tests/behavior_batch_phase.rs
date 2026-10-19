//! AT-6: the grouped Behavior phase. Requirements: REQ-B-4, REQ-K-4, REQ-K-5, ADR 0013, ADR 0020.
//!
//! Step 4 of M4: written before WI-6. The kernel does not group yet (it ticks every instance through
//! `BehaviorInstance::tick`), so the tests that observe a `tick_batch` call are ignored with that
//! work item; the equivalence tests already hold and stay live as a guard on the unbatched path.
//! Stub batched and unbatched behaviors only, no plugin; the kernel may not use `umbra-behavior`
//! (ASM-005) or `umbra-plugin-host`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use umbra_core::{
    BatchItem, BbValue, Behavior, BehaviorBatch, BehaviorContext, BehaviorId, BehaviorInstance,
    BlackboardError, EntityId, EntityView, InstantiateError, Intent, IntentBuffer, MetersPerSecond,
    Ownership, RemoteSource, Status, TickRate, tick_each,
};
use umbra_kernel::{AgentInit, Kernel};

use common::{Behavior as Motion, Stub, at, kernel, owned, route};

/// One `tick_batch` call as the batch saw it.
#[derive(Debug, Clone, PartialEq)]
struct Call {
    /// Which batch (the tag given at construction).
    tag: u8,
    /// Simulation tick of the call.
    tick: u64,
    /// Member entity ids in slice order.
    members: Vec<EntityId>,
}

type CallLog = Arc<Mutex<Vec<Call>>>;

/// Records every call, then ticks each item through `tick_each` (the reference semantics).
struct Recording {
    tag: u8,
    log: CallLog,
}

impl BehaviorBatch for Recording {
    fn tick_batch(&self, items: &mut [BatchItem<'_>]) {
        let call = Call {
            tag: self.tag,
            tick: items.first().map_or(0, |i| i.ctx.now.tick()),
            members: items.iter().map(|i| i.entity.id).collect(),
        };
        self.log.lock().expect("log lock").push(call);
        tick_each(items);
    }
}

/// Draws once per tick from the entity's stream, emits `SetSpeed(draw % 100)` (and `FollowRoute`
/// when asked), and runs forever.
struct Draws {
    follow: bool,
}

impl BehaviorInstance for Draws {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        let draw = ctx.rng.next_u64() % 100;
        #[expect(clippy::cast_precision_loss, reason = "draw is below 100")]
        out.push(Intent::SetSpeed(MetersPerSecond(draw as f64)));
        if self.follow {
            out.push(Intent::FollowRoute);
        }
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

/// The same instance logic, batched or not depending on `batch`.
struct Prog {
    id: BehaviorId,
    batch: Option<Arc<dyn BehaviorBatch>>,
    follow: bool,
}

impl Behavior for Prog {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Draws {
            follow: self.follow,
        }))
    }
    fn batch(&self) -> Option<Arc<dyn BehaviorBatch>> {
        self.batch.clone()
    }
}

fn prog(batch: Option<Arc<dyn BehaviorBatch>>, follow: bool) -> Arc<dyn Behavior> {
    Arc::new(Prog {
        id: BehaviorId::new("test.behavior.draws").expect("valid id"),
        batch,
        follow,
    })
}

fn recording(tag: u8, log: &CallLog) -> Arc<dyn BehaviorBatch> {
    Arc::new(Recording {
        tag,
        log: Arc::clone(log),
    })
}

fn agent(b: Arc<dyn Behavior>) -> AgentInit {
    AgentInit {
        behavior: Some(b),
        ..AgentInit::default()
    }
}

fn new_log() -> CallLog {
    Arc::new(Mutex::new(Vec::new()))
}

fn calls(log: &CallLog) -> Vec<Call> {
    log.lock().expect("log lock").clone()
}

fn ids(raw: &[u64]) -> Vec<EntityId> {
    raw.iter()
        .map(|i| EntityId::new(*i).expect("non-zero"))
        .collect()
}

/// Spawns one still entity per behavior in `behaviors` (ids 1, 2, ...), 20 m apart.
fn world(behaviors: &[Arc<dyn Behavior>], seed: u64) -> Kernel {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(seed);
    for (n, b) in behaviors.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "small index")]
        let y = n as f64 * 20.0;
        k.spawn_with(owned(at(y), &model), agent(Arc::clone(b)))
            .expect("spawn_with");
    }
    k
}

/// ADR 0020: batching is an optimization, never an observable difference: a kernel whose behaviors
/// are batched gives, tick by tick, the same snapshot and hash as one whose behaviors are not.
#[test]
fn batched_equals_unbatched_tick_by_tick() {
    let log = new_log();
    let batch = recording(1, &log);
    let batched: Vec<_> = (0..5)
        .map(|_| prog(Some(Arc::clone(&batch)), false))
        .collect();
    let plain: Vec<_> = (0..5).map(|_| prog(None, false)).collect();
    let (mut a, mut b) = (world(&batched, 7), world(&plain, 7));

    for tick in 1..=40 {
        a.tick().expect("tick");
        b.tick().expect("tick");
        assert_eq!(a.snapshot(), b.snapshot(), "snapshot at tick {tick}");
        assert_eq!(a.state_hash(), b.state_hash(), "hash at tick {tick}");
    }
}

/// ADR 0020, REQ-B-4: one `tick_batch` call per group per tick, members in entity id order.
#[test]
#[ignore = "WI-6"]
fn one_tick_batch_per_group_per_tick_members_in_id_order() {
    let log = new_log();
    let batch = recording(1, &log);
    let behaviors: Vec<_> = (0..4)
        .map(|_| prog(Some(Arc::clone(&batch)), false))
        .collect();
    let mut k = world(&behaviors, 3);

    for _ in 0..3 {
        k.tick().expect("tick");
    }

    let got = calls(&log);
    assert_eq!(got.len(), 3, "one call per tick: {got:?}");
    for (n, call) in got.iter().enumerate() {
        assert_eq!(call.tick, n as u64, "tick of call {n}");
        assert_eq!(call.members, ids(&[1, 2, 3, 4]), "members of call {n}");
    }
}

/// ADR 0020: groups are ordered by their first entity id, never by address or creation order.
#[test]
#[ignore = "WI-6"]
fn groups_ordered_by_first_entity_id() {
    let log = new_log();
    // The batch created first is used by the entities that come later: creation order and address
    // order must not matter.
    let late = recording(2, &log);
    let early = recording(1, &log);
    let behaviors = vec![
        prog(Some(Arc::clone(&early)), false), // id 1
        prog(Some(Arc::clone(&late)), false),  // id 2
        prog(Some(Arc::clone(&early)), false), // id 3
        prog(Some(Arc::clone(&late)), false),  // id 4
    ];
    let mut k = world(&behaviors, 3);

    k.tick().expect("tick");

    let got = calls(&log);
    let order: Vec<(u8, Vec<EntityId>)> = got.into_iter().map(|c| (c.tag, c.members)).collect();
    assert_eq!(
        order,
        vec![(1, ids(&[1, 3])), (2, ids(&[2, 4]))],
        "group of the first id (1) before the group of id 2"
    );
}

/// ADR 0020: the group order follows the first id even when the first group's tag sorts last.
#[test]
#[ignore = "WI-6"]
fn group_order_does_not_follow_spawn_of_the_batch_object() {
    let log = new_log();
    let first_created = recording(1, &log);
    let second_created = recording(2, &log);
    let behaviors = vec![
        prog(Some(Arc::clone(&second_created)), false), // id 1
        prog(Some(Arc::clone(&first_created)), false),  // id 2
    ];
    let mut k = world(&behaviors, 3);

    k.tick().expect("tick");

    let tags: Vec<u8> = calls(&log).iter().map(|c| c.tag).collect();
    assert_eq!(tags, vec![2, 1]);
}

/// REQ-K-5, ADR 0020: a Remote entity never reaches a batch; its Owned twin does.
#[test]
#[ignore = "WI-6"]
fn remote_entities_never_batched() {
    let log = new_log();
    let batch = recording(1, &log);
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut k = kernel(1);
    let mut remote = owned(at(0.0), &model);
    remote.ownership = Ownership::Remote {
        source: RemoteSource::new("net.test"),
    };
    k.spawn_with(remote, agent(prog(Some(Arc::clone(&batch)), false)))
        .expect("spawn_with"); // id 1, remote
    k.spawn_with(
        owned(at(50.0), &model),
        agent(prog(Some(Arc::clone(&batch)), false)),
    )
    .expect("spawn_with"); // id 2, owned

    for _ in 0..4 {
        k.tick().expect("tick");
    }

    let got = calls(&log);
    assert_eq!(got.len(), 4, "the owned entity is batched every tick");
    assert!(
        got.iter().all(|c| c.members == ids(&[2])),
        "only the owned entity: {got:?}"
    );
}

/// ADR 0020, ADR 0013: batched and unbatched entities side by side give the same world as an
/// all-unbatched run; the batch sees only its own members.
#[test]
#[ignore = "WI-6"]
fn mixed_batched_and_unbatched() {
    let log = new_log();
    let batch = recording(1, &log);
    let mixed = vec![
        prog(None, false),                     // id 1
        prog(Some(Arc::clone(&batch)), false), // id 2
        prog(None, false),                     // id 3
        prog(Some(Arc::clone(&batch)), false), // id 4
        prog(None, false),                     // id 5
    ];
    let plain: Vec<_> = (0..5).map(|_| prog(None, false)).collect();
    let (mut a, mut b) = (world(&mixed, 11), world(&plain, 11));

    for tick in 1..=25 {
        a.tick().expect("tick");
        b.tick().expect("tick");
        assert_eq!(a.state_hash(), b.state_hash(), "hash at tick {tick}");
    }

    let got = calls(&log);
    assert_eq!(got.len(), 25, "one call per tick");
    assert!(got.iter().all(|c| c.members == ids(&[2, 4])), "{got:?}");
}

/// ADR 0016, ADR 0020: `FollowRoute` is expanded for batched entities after the group ticked, and
/// gives the route progress of an unbatched entity.
#[test]
#[ignore = "WI-6"]
fn follow_route_expanded_for_batched_entities() {
    let model = Stub::new(
        "test.model.drift",
        Motion::Drift(umbra_core::Vec3::new(0.0, 10.0, 0.0)),
    );
    let log = new_log();
    let batch = recording(1, &log);
    let spawn = |k: &mut Kernel, b: Arc<dyn Behavior>| {
        let mut init = owned(at(0.0), &model);
        init.route = Some(route(vec![at(20.0), at(40.0), at(60.0)], 8.0));
        k.spawn_with(init, agent(b)).expect("spawn_with");
    };
    let (mut a, mut b) = (kernel(9), kernel(9));
    spawn(&mut a, prog(Some(batch), true));
    spawn(&mut b, prog(None, true));

    for tick in 1..=150 {
        a.tick().expect("tick");
        b.tick().expect("tick");
        assert_eq!(
            a.snapshot().entities[0].intents,
            b.snapshot().entities[0].intents,
            "intents at tick {tick}"
        );
        assert_eq!(
            a.snapshot().entities[0].route_next,
            b.snapshot().entities[0].route_next,
            "route progress at tick {tick}"
        );
        assert_eq!(a.state_hash(), b.state_hash(), "hash at tick {tick}");
    }

    assert_eq!(
        calls(&log).len(),
        150,
        "the batch ticked the entity every tick"
    );
    let intents = &a.snapshot().entities[0].intents;
    assert!(
        !intents.contains(&Intent::FollowRoute),
        "expanded in place: {intents:?}"
    );
}

/// ADR 0020 (scratch vectors are kept in the world and reused, no per-tick allocation once warm):
/// the capacity of the phase's scratch vectors must not GROW after warm-up.
///
/// Measured through `Kernel::behavior_scratch_capacity` (a `#[doc(hidden)]` diagnostic probe stubbed
/// for WI-6; a plan gap, reported): kernel test targets are `forbid(unsafe_code)`, so a counting
/// global allocator is not available, and the slice address handed to `tick_batch` is deliberately
/// not asserted (`BatchItem<'a>` is invariant in `'a`, so a stable address would be an accident of
/// std's allocation reuse, not a guarantee).
#[test]
#[ignore = "WI-6"]
fn no_allocation_growth_after_warm_up() {
    let log = new_log();
    let batch = recording(1, &log);
    let behaviors: Vec<_> = (0..64)
        .map(|_| prog(Some(Arc::clone(&batch)), false))
        .collect();
    let mut k = world(&behaviors, 5);
    for _ in 0..10 {
        k.tick().expect("tick");
    }
    let warm = k.behavior_scratch_capacity();

    for _ in 0..200 {
        k.tick().expect("tick");
    }

    assert!(
        warm >= 64,
        "the scratch holds the full group once warm: {warm}"
    );
    assert!(
        k.behavior_scratch_capacity() <= warm,
        "capacity grew after warm-up: {warm} -> {}",
        k.behavior_scratch_capacity()
    );
    let got = calls(&log);
    assert_eq!(got.len(), 210, "one call per tick");
    assert!(
        got.iter().all(|c| c.members.len() == 64),
        "full group each tick"
    );
}

/// The assignment of entity `n` to a group: 0 = unbatched, 1..=3 = batch tag.
fn run_assignment(assignment: &[u8], batched: bool, reverse_creation: bool) -> Vec<(u64, String)> {
    let log = new_log();
    let mut order = [1_u8, 2, 3];
    if reverse_creation {
        order.reverse();
    }
    let mut by_tag: [Option<Arc<dyn BehaviorBatch>>; 3] = [None, None, None];
    for tag in order {
        by_tag[usize::from(tag) - 1] = Some(recording(tag, &log));
    }
    let behaviors: Vec<_> = assignment
        .iter()
        .map(|g| {
            if batched && *g > 0 {
                prog(by_tag[usize::from(*g) - 1].clone(), false)
            } else {
                prog(None, false)
            }
        })
        .collect();
    let mut k = world(&behaviors, 21);
    let mut out = Vec::new();
    for tick in 0..12_u64 {
        k.tick().expect("tick");
        out.push((tick, format!("{:?} {:?}", k.state_hash(), k.snapshot())));
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// ADR 0020, REQ-K-4: whatever the group sizes and spawn order, a run with batched groups
    /// equals the all-unbatched run, and does not depend on the order the batch objects were made.
    #[test]
    fn random_groups_and_spawn_orders_give_identical_runs(
        assignment in proptest::collection::vec(0_u8..4, 1..10),
    ) {
        let reference = run_assignment(&assignment, false, false);
        prop_assert_eq!(&reference, &run_assignment(&assignment, true, false));
        prop_assert_eq!(&reference, &run_assignment(&assignment, true, true));
    }
}
