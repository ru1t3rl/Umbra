//! ASM-248 at engine level (M6 WI-9): the ghost events of a tick (`EntityCreated`, `EntityRemoved`)
//! come before the events the kernel produced in the same tick.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod net_support;

use std::sync::Arc;

use umbra_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError, BuildError,
    ComponentId, ComponentInfo, EntityView, Factory, InstantiateError, Intent, IntentBuffer,
    ParamSchema, ParamValue, Provenance, Registry, SimEvent, Status, TickRate, Version,
};
use umbra_net::LoopbackHub;

use net_support::{Peer, id_of, registry_with, start};

const TICKER: &str = "test.behavior.ticker";

struct Ticker;

impl BehaviorInstance for Ticker {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        out.push(Intent::Custom {
            kind: ComponentId::new(TICKER).expect("id"),
            payload: ParamValue::Null,
        });
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

struct TickerBehavior(BehaviorId);

impl Behavior for TickerBehavior {
    fn id(&self) -> &BehaviorId {
        &self.0
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Ticker))
    }
}

struct TickerFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn Behavior> for TickerFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        let id = BehaviorId::new(TICKER).map_err(|e| BuildError::new(e.to_string()))?;
        Ok(Arc::new(TickerBehavior(id)))
    }
}

fn with_ticker(mut registry: Registry) -> Registry {
    registry
        .register_behavior(
            Arc::new(TickerFactory {
                id: ComponentId::new(TICKER).expect("id"),
                schema: ParamSchema::from_json_str(r#"{"type":"object","examples":[{}]}"#)
                    .expect("schema"),
            }),
            ComponentInfo::new("ticker", Provenance::Plugin, Version::new(0, 1, 0)),
        )
        .expect("register the ticker");
    registry
}

fn is_custom(e: &SimEvent) -> bool {
    matches!(e, SimEvent::Custom { .. })
}

#[test]
fn ghost_events_precede_kernel_events_in_the_tick_report() {
    let hub = LoopbackHub::new(4096);
    let (mut o, _) = Peer::new(start(
        "order",
        &with_ticker(registry_with(&[("test.net.loopback", &hub, 1000)])),
        None,
    ));
    let (mut b, b_infos) = Peer::new(start(
        "b",
        &registry_with(&[("test.net.loopback", &hub, 2000)]),
        None,
    ));

    for _ in 0..5 {
        b.tick();
        o.tick();
    }
    let ghost = o.ghosts()[0].id;
    let appear = o
        .events
        .iter()
        .find(|tick| {
            tick.iter()
                .any(|e| matches!(e, SimEvent::EntityCreated { entity, .. } if *entity == ghost))
        })
        .expect("a tick created the ghost");
    assert!(
        is_custom(appear.last().expect("events")),
        "the kernel's event is in the same tick: {appear:?}"
    );
    assert!(
        matches!(appear.first(), Some(SimEvent::EntityCreated { entity, .. }) if *entity == ghost),
        "the ghost event comes first: {appear:?}"
    );

    b.engine.remove(id_of(&b_infos, "b-tank")).expect("remove");
    let before = o.events.len();
    for _ in 0..3 {
        b.tick();
        o.tick();
    }
    let gone = o.events[before..]
        .iter()
        .find(|tick| {
            tick.iter()
                .any(|e| matches!(e, SimEvent::EntityRemoved { entity } if *entity == ghost))
        })
        .expect("a tick removed the ghost");
    assert!(
        matches!(gone.first(), Some(SimEvent::EntityRemoved { entity }) if *entity == ghost),
        "the removal comes first: {gone:?}"
    );
    assert!(is_custom(gone.last().expect("events")), "{gone:?}");
}
