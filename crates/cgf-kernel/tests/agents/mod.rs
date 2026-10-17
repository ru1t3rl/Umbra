//! Stub behaviors and sensors for the M3 kernel tests (WI-6). The kernel may not use
//! `cgf-behavior` (ASM-005), so behaviors here are closures and sensors are range checks.
//!
//! New in M3; the M1 helpers in `tests/common/mod.rs` are reused unchanged.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::sync::{Arc, Mutex};

use cgf_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError, ComponentId,
    Contact, ContactBuffer, EntityId, EntityView, InstantiateError, IntentBuffer, Meters, Observer,
    SenseContext, Sensor, Status, Target, TickRate,
};

/// The per-instance tick function of a [`Scripted`] behavior.
pub type TickFn = Box<
    dyn FnMut(&mut BehaviorContext<'_>, &EntityView<'_>, &mut IntentBuffer<'_>) -> Status + Send,
>;

/// A behavior whose instances run a closure made by `make`; `make` runs once per `instantiate`.
pub struct Scripted {
    id: BehaviorId,
    make: Box<dyn Fn() -> TickFn + Send + Sync>,
    fail: bool,
}

impl Scripted {
    pub fn arc(id: &str, make: impl Fn() -> TickFn + Send + Sync + 'static) -> Arc<dyn Behavior> {
        Arc::new(Self {
            id: BehaviorId::new(id).expect("valid id"),
            make: Box::new(make),
            fail: false,
        })
    }

    /// A behavior whose `instantiate` always fails.
    pub fn failing(id: &str) -> Arc<dyn Behavior> {
        Arc::new(Self {
            id: BehaviorId::new(id).expect("valid id"),
            make: Box::new(|| Box::new(|_, _, _| Status::Failure)),
            fail: true,
        })
    }
}

struct ScriptedInstance(TickFn);

impl BehaviorInstance for ScriptedInstance {
    fn tick(
        &mut self,
        ctx: &mut BehaviorContext<'_>,
        entity: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        (self.0)(ctx, entity, out)
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

impl Behavior for Scripted {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        if self.fail {
            return Err(InstantiateError::Duration {
                what: "stub".into(),
                hz: rate.hz(),
            });
        }
        Ok(Box::new(ScriptedInstance((self.make)())))
    }
}

/// What one sensor call saw.
#[derive(Debug, Clone, PartialEq)]
pub struct SenseRecord {
    pub observer: EntityId,
    /// Candidate ids in the order given.
    pub candidates: Vec<EntityId>,
    /// The first draw from the stream the sensor was given, if it draws.
    pub draw: Option<u64>,
}

/// A sensor that detects every candidate within `range_m` (3D, ECEF) and reports `tag` as the
/// contact range, so tests can tell sensors apart. It can draw from the observer's RNG stream and
/// logs every call.
pub struct RangeSensor {
    id: ComponentId,
    pub range_m: f64,
    pub tag: f64,
    pub draws: bool,
    pub log: Arc<Mutex<Vec<SenseRecord>>>,
}

impl RangeSensor {
    pub fn new(id: &str, range_m: f64) -> Self {
        Self {
            id: ComponentId::new(id).expect("valid id"),
            range_m,
            tag: range_m,
            draws: false,
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn tagged(mut self, tag: f64) -> Self {
        self.tag = tag;
        self
    }

    #[must_use]
    pub fn drawing(mut self) -> Self {
        self.draws = true;
        self
    }

    pub fn records(&self) -> Vec<SenseRecord> {
        self.log.lock().expect("log lock").clone()
    }
}

impl Sensor for RangeSensor {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn sense(
        &self,
        ctx: &mut SenseContext<'_>,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        out: &mut ContactBuffer<'_>,
    ) {
        let draw = self.draws.then(|| ctx.rng.next_u64());
        self.log.lock().expect("log lock").push(SenseRecord {
            observer: observer.id,
            candidates: candidates.iter().map(|t| t.id).collect(),
            draw,
        });
        for t in candidates {
            let d = (t.state.position - observer.state.position).norm();
            if d <= self.range_m {
                out.push(Contact::new(
                    t.id,
                    t.state.position,
                    t.state.velocity,
                    t.force.cloned(),
                    Meters(self.tag),
                ));
            }
        }
    }
}
