//! AT-6 regression (WI-6 review B1): a Remote entity whose behavior has a batch shifts nothing.
//! Slots are paired with entities by id, so Owned members keep their own instance and status.
//! Requirements: REQ-K-5, REQ-B-4, ADR 0020.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use umbra_core::{
    BatchItem, BbValue, Behavior, BehaviorBatch, BehaviorContext, BehaviorId, BehaviorInstance,
    BlackboardError, EntityView, InstantiateError, Intent, IntentBuffer, MetersPerSecond,
    Ownership, RemoteSource, Status, TickRate, tick_each,
};
use umbra_kernel::{AgentInit, Kernel};

use common::{Behavior as Motion, Stub, at, kernel, owned};

struct PlainBatch;

impl BehaviorBatch for PlainBatch {
    fn tick_batch(&self, items: &mut [BatchItem<'_>]) {
        tick_each(items);
    }
}

/// Counts its own ticks; emits `SetSpeed(base * 1000 + count)` and a status that depends on `base`.
struct Counter {
    base: u64,
    count: u64,
    shared: Arc<AtomicU64>,
}

impl BehaviorInstance for Counter {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        self.count += 1;
        self.shared.fetch_add(1, Ordering::SeqCst);
        #[expect(clippy::cast_precision_loss, reason = "small numbers")]
        out.push(Intent::SetSpeed(MetersPerSecond(
            (self.base * 1000 + self.count) as f64,
        )));
        match self.base % 3 {
            0 => Status::Success,
            1 => Status::Failure,
            _ => Status::Running,
        }
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

struct Prog {
    id: BehaviorId,
    base: u64,
    shared: Arc<AtomicU64>,
    batch: Arc<dyn BehaviorBatch>,
}

impl Behavior for Prog {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Counter {
            base: self.base,
            count: 0,
            shared: Arc::clone(&self.shared),
        }))
    }
    fn batch(&self) -> Option<Arc<dyn BehaviorBatch>> {
        Some(Arc::clone(&self.batch))
    }
}

fn spawn(
    k: &mut Kernel,
    y: f64,
    remote: bool,
    base: u64,
    shared: &Arc<AtomicU64>,
    b: &Arc<dyn BehaviorBatch>,
) {
    let model = Stub::new("test.model.still", Motion::Stationary);
    let mut init = owned(at(y), &model);
    if remote {
        init.ownership = Ownership::Remote {
            source: RemoteSource::new("net.test"),
        };
    }
    let program: Arc<dyn Behavior> = Arc::new(Prog {
        id: BehaviorId::new("test.behavior.counter").expect("valid id"),
        base,
        shared: Arc::clone(shared),
        batch: Arc::clone(b),
    });
    k.spawn_with(
        init,
        AgentInit {
            behavior: Some(program),
            ..AgentInit::default()
        },
    )
    .expect("spawn_with");
}

#[test]
fn remote_batched_entity_before_owned_members_shifts_nothing() {
    let batch: Arc<dyn BehaviorBatch> = Arc::new(PlainBatch);
    let remote_ticks = Arc::new(AtomicU64::new(0));
    let owned_ticks = Arc::new(AtomicU64::new(0));
    let mut k = kernel(4);
    spawn(&mut k, 0.0, true, 9, &remote_ticks, &batch); // id 1, Remote
    spawn(&mut k, 20.0, false, 1, &owned_ticks, &batch); // id 2: Failure
    spawn(&mut k, 40.0, false, 2, &owned_ticks, &batch); // id 3: Running
    spawn(&mut k, 60.0, false, 3, &owned_ticks, &batch); // id 4: Success

    for n in 1..=5_u64 {
        k.tick().expect("tick");
        let snap = k.snapshot();
        let e = &snap.entities;
        assert_eq!(e[0].status, None, "remote status stays None");
        assert!(e[0].intents.is_empty(), "remote has no intents");
        for (entity, base, status) in [
            (&e[1], 1_u64, Status::Failure),
            (&e[2], 2, Status::Running),
            (&e[3], 3, Status::Success),
        ] {
            assert_eq!(
                entity.status,
                Some(status),
                "status of base {base}, tick {n}"
            );
            #[expect(clippy::cast_precision_loss, reason = "small numbers")]
            let want = Intent::SetSpeed(MetersPerSecond((base * 1000 + n) as f64));
            assert_eq!(
                entity.intents,
                vec![want],
                "intents of base {base}, tick {n}"
            );
        }
    }
    assert_eq!(
        remote_ticks.load(Ordering::SeqCst),
        0,
        "the remote instance never ticks"
    );
    assert_eq!(owned_ticks.load(Ordering::SeqCst), 15);
}
