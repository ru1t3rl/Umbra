//! M3 WI-3 follow-up: fallible `instantiate`, owned `output`, `InstantiateError`.
//! Requirements: REQ-B-4, REQ-B-5, ADR 0011, ADR 0013.

#![forbid(unsafe_code)]

use std::sync::Arc;

use umbra_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError, EntityView,
    InstantiateError, IntentBuffer, Status, TickRate,
};

struct Failing(BehaviorId);

impl Behavior for Failing {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, rate: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Err(InstantiateError::Duration {
            what: "cooldown".into(),
            hz: rate.hz(),
        })
    }
}

struct Works(BehaviorId);

struct Inst;

impl BehaviorInstance for Inst {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        _: &mut IntentBuffer<'_>,
    ) -> Status {
        Status::Success
    }
    fn halt(&mut self) {}
    fn output(&self, port: &str) -> Option<BbValue> {
        (port == "n").then_some(BbValue::Integer(3))
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
}

impl Behavior for Works {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Inst))
    }
}

fn id() -> BehaviorId {
    BehaviorId::new("stub.behavior.x").unwrap_or_else(|e| unreachable!("{e}"))
}

fn rate() -> TickRate {
    TickRate::new(30).unwrap_or_else(|| unreachable!("30 Hz is valid"))
}

/// REQ-B-4: a failing `instantiate` is reported through `Arc<dyn Behavior>`.
#[test]
fn given_failing_behavior_as_trait_object_when_instantiated_then_error() {
    let b: Arc<dyn Behavior> = Arc::new(Failing(id()));

    let err = b.instantiate(rate()).err();

    assert_eq!(
        err,
        Some(InstantiateError::Duration {
            what: "cooldown".into(),
            hz: 30
        })
    );
    assert_eq!(
        err.map(|e| e.to_string()).as_deref(),
        Some("cooldown: duration does not convert to ticks at 30 Hz")
    );
}

/// ADR 0013: a working behavior yields a boxed instance with owned outputs.
#[test]
fn given_working_behavior_when_instantiated_then_boxed_instance_with_owned_output() {
    let b: Arc<dyn Behavior> = Arc::new(Works(id()));

    let inst = b.instantiate(rate()).ok();

    let inst = inst.as_deref();
    assert_eq!(inst.and_then(|i| i.output("n")), Some(BbValue::Integer(3)));
    assert_eq!(inst.and_then(|i| i.output("missing")), None);
}

/// `InstantiateError::Subgraph` exposes the nested error as its source.
#[test]
fn given_nested_error_when_displayed_then_source_chain_visible() {
    let inner = InstantiateError::Other {
        message: "boom".into(),
    };
    let e = InstantiateError::Subgraph {
        id: id(),
        source: Box::new(inner.clone()),
    };

    assert_eq!(e.to_string(), "subgraph `stub.behavior.x`: boom");
    assert_eq!(
        std::error::Error::source(&e).map(ToString::to_string),
        Some("boom".to_owned())
    );
}
