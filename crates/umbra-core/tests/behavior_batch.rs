//! M4 WI-5: batch hook, `tick_each`, `as_any_mut`. Requirements: REQ-B-4, ADR 0020.

#![forbid(unsafe_code)]

use std::any::Any;
use std::sync::Arc;

use umbra_core::{
    BatchItem, BbValue, Behavior, BehaviorBatch, BehaviorContext, BehaviorId, BehaviorInstance,
    BlackboardError, Ecef, EntityId, EntityView, InstantiateError, Intent, IntentBuffer,
    KinematicState, Quaternion, RngStream, SimRng, SimTime, Status, StreamKey, TickRate, Vec3,
    tick_each,
};

/// Draws once from the RNG, emits `SetSpeed`-free `Hold`, status from the draw's parity.
#[derive(Default)]
struct Counter {
    ticks: u64,
}

fn step(c: &mut Counter, ctx: &mut BehaviorContext<'_>, out: &mut IntentBuffer<'_>) -> Status {
    c.ticks += 1;
    let draw = ctx.rng.next_u64();
    out.push(Intent::Hold);
    if draw.is_multiple_of(2) {
        Status::Success
    } else {
        Status::Running
    }
}

impl BehaviorInstance for Counter {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        step(self, ctx, out)
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

/// An instance that keeps the default `as_any_mut`.
struct Opaque;

impl BehaviorInstance for Opaque {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        ctx.rng.next_u64();
        out.push(Intent::Despawn);
        Status::Success
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

/// Recognises `Counter` by downcast and ticks it directly; everything else falls back.
struct CounterBatch;

impl BehaviorBatch for CounterBatch {
    fn tick_batch(&self, items: &mut [BatchItem<'_>]) {
        for item in items {
            if let Some(c) = item
                .instance
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<Counter>())
            {
                item.status = step(c, &mut item.ctx, &mut item.out);
            } else {
                item.status = item
                    .instance
                    .tick(&mut item.ctx, &item.entity, &mut item.out);
            }
        }
    }
}

struct Batched(BehaviorId, Arc<dyn BehaviorBatch>);

impl Behavior for Batched {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Counter::default()))
    }
    fn batch(&self) -> Option<Arc<dyn BehaviorBatch>> {
        Some(Arc::clone(&self.1))
    }
}

fn state() -> KinematicState {
    KinematicState {
        position: Ecef(Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        }),
        velocity: Vec3::default(),
        orientation: Quaternion::IDENTITY,
    }
}

fn rate() -> TickRate {
    TickRate::new(30).unwrap_or_else(|| unreachable!())
}

fn rng(i: u64) -> RngStream {
    let id = EntityId::new(i + 1).unwrap_or_else(|| unreachable!());
    SimRng::new(7).stream(StreamKey::entity(id))
}

/// Runs `f` over three items (two `Counter`, one `Opaque`) for four ticks; returns the observed
/// trace: statuses, intents and RNG states.
type Trace = (Vec<Vec<Status>>, Vec<Vec<Intent>>, Vec<[u64; 4]>);

fn run(f: &dyn Fn(&mut [BatchItem<'_>])) -> Trace {
    let st = state();
    let mut insts: Vec<Box<dyn BehaviorInstance>> = vec![
        Box::new(Counter::default()),
        Box::new(Opaque),
        Box::new(Counter::default()),
    ];
    let mut rngs: Vec<RngStream> = (0..3).map(rng).collect();
    let mut bufs: Vec<Vec<Intent>> = vec![Vec::new(); 3];
    let mut statuses = Vec::new();
    for tick in 0..4 {
        let now = SimTime::at_tick(rate(), tick);
        let mut items: Vec<BatchItem<'_>> = insts
            .iter_mut()
            .zip(rngs.iter_mut())
            .zip(bufs.iter_mut())
            .enumerate()
            .map(|(i, ((inst, r), b))| {
                let id = EntityId::new(i as u64 + 1).unwrap_or_else(|| unreachable!());
                BatchItem::new(
                    BehaviorContext::new(now, r),
                    EntityView::new(id, &st),
                    inst.as_mut(),
                    IntentBuffer::new(b),
                )
            })
            .collect();
        f(&mut items);
        statuses.push(items.iter().map(|i| i.status).collect());
    }
    (statuses, bufs, rngs.iter().map(RngStream::state).collect())
}

/// ADR 0020: a batch equals `tick_each` in statuses, intents and RNG states.
#[test]
fn stub_batch_equals_tick_each() {
    let reference = run(&tick_each);
    let batched = run(&|items| CounterBatch.tick_batch(items));
    assert_eq!(reference, batched);
    assert_eq!(reference.1[0].len(), 4);
}

#[test]
fn batch_item_starts_at_failure() {
    let st = state();
    let mut r = rng(0);
    let mut v = Vec::new();
    let mut inst = Opaque;
    let item = BatchItem::new(
        BehaviorContext::new(SimTime::start(rate()), &mut r),
        EntityView::new(EntityId::new(1).unwrap_or_else(|| unreachable!()), &st),
        &mut inst,
        IntentBuffer::new(&mut v),
    );
    assert_eq!(item.status, Status::Failure);
}

#[test]
fn as_any_mut_defaults_to_none_and_override_downcasts() {
    let mut o = Opaque;
    assert!(o.as_any_mut().is_none());
    let mut c = Counter::default();
    let any = c.as_any_mut();
    assert!(any.is_some());
    assert!(any.is_some_and(|a| a.downcast_mut::<Counter>().is_some()));
    assert!(
        c.as_any_mut()
            .is_some_and(|a| a.downcast_mut::<Opaque>().is_none())
    );
}

/// An item the batch does not recognise is ticked through `instance.tick`.
#[test]
fn unrecognised_item_falls_back_to_instance_tick() {
    let (statuses, intents, _) = run(&|items| CounterBatch.tick_batch(items));
    assert!(statuses.iter().all(|s| s[1] == Status::Success));
    assert_eq!(intents[1], vec![Intent::Despawn; 4]);
}

#[test]
fn behavior_batch_default_is_none_and_override_groups_by_arc() {
    let shared: Arc<dyn BehaviorBatch> = Arc::new(CounterBatch);
    let id = BehaviorId::new("stub.behavior.batched").unwrap_or_else(|e| unreachable!("{e}"));
    let a = Batched(id.clone(), Arc::clone(&shared));
    let b = Batched(id, Arc::clone(&shared));
    let (ba, bb) = (a.batch(), b.batch());
    assert!(matches!((&ba, &bb), (Some(x), Some(y)) if Arc::ptr_eq(x, y)));
}
