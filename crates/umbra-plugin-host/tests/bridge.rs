//! AT-5: the host bridge between the core traits and the C ABI. Requirements: REQ-B-4, REQ-B-5,
//! REQ-B-9, REQ-B-10, REQ-K-4, ADR 0013 amendment, ADR 0020, ASM-086, ASM-096, R-B3.
//!
//! The tests use a raw-ABI plugin forged in `support/raw.rs` (host tests cannot use `umbra-behavior`,
//! ASM-005) and `support::build_behavior` for the engine-scope build (plan gap, see its doc). The
//! subgraph side of the null-entry fallback is the M3 test `subgraph_halt.rs` (children without an
//! override, ASM-096 d). Step 4 of M4: the host bridge lands in WI-9.
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[macro_use]
mod support;

use std::mem::size_of;
use std::sync::Arc;

use support::bridge::{Ent, eid, env, env_with, rate, state, tick_batched, tick_singly};
use support::raw::{self, Got};
use umbra_core::{
    BatchItem, BbValue, Behavior, BehaviorBatch, BehaviorContext, BehaviorInstance,
    BlackboardError, Contact, Ecef, EntityView, Force, InstantiateError, Intent, IntentBuffer,
    Meters, RouteView, SimTime, Status, Vec3,
};
use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UmbraBehaviorBatch, UmbraBehaviorItem, UmbraContact, UmbraEntityView,
};
use umbra_plugin_host::HostOptions;

fn batch_of(b: &Arc<dyn Behavior>) -> Arc<dyn BehaviorBatch> {
    b.batch().expect("a plugin behavior has a batch (ADR 0020)")
}

/// ADR 0020: a batch gives every item what its own `instance.tick` would: statuses, intents and RNG
/// states (here compared with `tick_each` over fresh, identical entities).
#[test]
#[ignore = "WI-9"]
fn batch_contract_equals_tick_each() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let batch = batch_of(&b);
    let mut batched: Vec<Ent> = (1..=4).map(|n| Ent::new(n, &b, 9)).collect();
    let mut single: Vec<Ent> = (1..=4).map(|n| Ent::new(n, &b, 9)).collect();

    for tick in 0..5 {
        tick_batched(&batch, &mut batched, tick);
        tick_singly(&mut single, tick);
        for (a, s) in batched.iter().zip(&single) {
            assert_eq!(a.status, s.status, "status of {:?} at tick {tick}", a.id);
            assert_eq!(a.buf, s.buf, "intents of {:?} at tick {tick}", a.id);
            assert_eq!(
                a.rng.state(),
                s.rng.state(),
                "rng of {:?} at tick {tick}",
                a.id
            );
        }
    }
}

/// REQ-B-4: the plugin sees the entity's state, force, route and contacts of the pre-tick view.
#[test]
#[ignore = "WI-9"]
fn views_carry_state_force_route_contacts() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let batch = batch_of(&b);
    let mut inst = b.instantiate(rate()).expect("instantiate");
    let st = state(7.0);
    let force = Force::new("blue").expect("force");
    let waypoints = [
        Ecef(Vec3 {
            x: 6_378_200.0,
            y: 10.0,
            z: 0.0,
        }),
        Ecef(Vec3 {
            x: 6_378_300.0,
            y: 20.0,
            z: 0.0,
        }),
    ];
    let other = state(50.0);
    let contacts = [
        Contact::new(
            eid(3),
            other.position,
            other.velocity,
            Some(Force::new("red").expect("force")),
            Meters(120.5),
        ),
        Contact::new(eid(9), other.position, other.velocity, None, Meters(300.0)),
    ];
    let mut rng = umbra_core::SimRng::new(1).stream(umbra_core::StreamKey::entity(eid(2)));
    let mut buf = Vec::new();
    let view = EntityView::new(eid(2), &st)
        .with_force(Some(&force))
        .with_route(RouteView::new(&waypoints, 1, false))
        .with_contacts(&contacts);
    let mut items = vec![BatchItem::new(
        BehaviorContext::new(SimTime::at_tick(rate(), 12), &mut rng),
        view,
        inst.as_mut(),
        IntentBuffer::new(&mut buf),
    )];

    batch.tick_batch(&mut items);

    let seen = raw::seen();
    assert_eq!(seen.len(), 1);
    let s = &seen[0];
    assert_eq!((s.id, s.tick, s.rate_hz), (2, 12, 30));
    assert_eq!(
        s.position,
        [st.position.0.x, st.position.0.y, st.position.0.z]
    );
    assert_eq!(s.velocity, [st.velocity.x, st.velocity.y, st.velocity.z]);
    assert_eq!(s.orientation, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(s.force, "blue");
    assert_eq!((s.has_route, s.route_complete, s.route_next), (1, 0, 1));
    assert_eq!(s.route.len(), 2);
    assert_eq!(s.route[1], [6_378_300.0, 20.0, 0.0]);
    assert_eq!(s.contacts.len(), 2);
    assert_eq!((s.contacts[0].0, s.contacts[0].3.as_str()), (3, "red"));
    assert!((s.contacts[0].2 - 120.5).abs() < f64::EPSILON);
    assert_eq!(
        (s.contacts[1].0, s.contacts[1].3.as_str()),
        (9, ""),
        "sorted by id, no force is empty"
    );
    assert_eq!(s.prefilled_status, 0, "statuses are pre-filled UNSET");
}

/// ADR 0019 (R-B3): every struct the host passes is written in the plugin's declared minor.
#[test]
#[ignore = "WI-9"]
fn host_writes_structs_in_the_plugins_minor() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let batch = batch_of(&b);
    let mut ents = [Ent::new(1, &b, 1)];
    // A view with a contact so the contact struct is written too.
    let st = state(1.0);
    let contacts = [Contact::new(
        eid(5),
        st.position,
        st.velocity,
        None,
        Meters(1.0),
    )];
    let mut rng = umbra_core::SimRng::new(1).stream(umbra_core::StreamKey::entity(eid(1)));
    let mut buf = Vec::new();
    let mut items = vec![BatchItem::new(
        BehaviorContext::new(SimTime::at_tick(rate(), 0), &mut rng),
        EntityView::new(eid(1), &st).with_contacts(&contacts),
        ents[0].inst.as_mut(),
        IntentBuffer::new(&mut buf),
    )];

    batch.tick_batch(&mut items);

    let s = &raw::seen()[0];
    assert_eq!(
        (s.batch_version, s.batch_size as usize),
        (UMBRA_ABI_VERSION, size_of::<UmbraBehaviorBatch>())
    );
    assert_eq!(
        (s.view_version, s.view_size as usize),
        (UMBRA_ABI_VERSION, size_of::<UmbraEntityView>())
    );
    assert_eq!(s.item_size as usize, size_of::<UmbraBehaviorItem>());
    assert_eq!(s.contact_size as usize, size_of::<UmbraContact>());
}

/// ADR 0013 amendment: with a `halt_running` entry the host calls it for `halt_running`.
#[test]
#[ignore = "WI-9"]
fn halt_running_entry_used_when_present() {
    raw::clear();
    let env = env_with(HostOptions::default(), true);
    let b = env.behavior("ok");
    let mut inst = b.instantiate(rate()).expect("instantiate");

    inst.halt_running();

    let events = raw::events();
    assert_eq!(
        events.last().map(String::as_str),
        Some("halt_running"),
        "{events:?}"
    );
    assert!(!events.contains(&"halt".to_owned()));
}

/// ADR 0020: a null `halt_running` falls back to `halt` (the full reset).
#[test]
#[ignore = "WI-9"]
fn halt_used_when_halt_running_is_null() {
    raw::clear();
    let env = env_with(HostOptions::default(), false);
    let b = env.behavior("ok");
    let mut inst = b.instantiate(rate()).expect("instantiate");

    inst.halt_running();

    let events = raw::events();
    assert_eq!(
        events.last().map(String::as_str),
        Some("halt"),
        "{events:?}"
    );
    assert!(!events.contains(&"halt_running".to_owned()));
}

/// ASM-096 (c): halt is idempotent and safe on an instance that never ticked.
#[test]
#[ignore = "WI-9"]
fn halt_is_idempotent_and_safe_before_first_tick() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let mut inst = b.instantiate(rate()).expect("instantiate");

    inst.halt();
    inst.halt();

    let halts = raw::events().iter().filter(|e| *e == "halt").count();
    assert_eq!(halts, 2);
    assert_eq!(env.host.faults(), Vec::new());
}

/// REQ-B-10: every value type crosses `set_input` and `output`; `None` unsets.
#[test]
#[ignore = "WI-9"]
fn set_input_and_output_every_value_type() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let mut inst = b.instantiate(rate()).expect("instantiate");
    let position = Ecef(Vec3 {
        x: 1.0,
        y: -2.0,
        z: 3.5,
    });
    let cases: [(&str, &str, BbValue); 6] = [
        ("in_bool", "out_bool", BbValue::Bool(true)),
        ("in_number", "out_number", BbValue::Number(-2.5)),
        ("in_integer", "out_integer", BbValue::Integer(i64::MIN)),
        (
            "in_string",
            "out_string",
            BbValue::String("h\u{e9}llo".into()),
        ),
        ("in_entity", "out_entity", BbValue::Entity(eid(42))),
        ("in_position", "out_position", BbValue::Position(position)),
    ];

    for (input, output, value) in cases {
        inst.set_input(input, Some(value.clone()))
            .expect("set_input");
        assert_eq!(inst.output(output), Some(value), "{input} -> {output}");
    }
    inst.set_input("in_number", None).expect("unset");

    assert_eq!(inst.output("out_number"), None);
    assert!(
        raw::inputs()
            .iter()
            .any(|(n, g)| n == "in_bool" && *g == Got::Bool(1))
    );
    assert!(
        raw::inputs()
            .iter()
            .any(|(n, g)| n == "in_number" && *g == Got::Unset)
    );
}

/// R-B4, REQ-K-4: the plugin's draws come from the entity's own stream, and that stream advances.
#[test]
#[ignore = "WI-9"]
fn rng_token_draws_the_entity_stream() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let batch = batch_of(&b);
    let mut ents: Vec<Ent> = (1..=3).map(|n| Ent::new(n, &b, 77)).collect();

    tick_batched(&batch, &mut ents, 0);

    for (n, e) in ents.iter().enumerate() {
        let n = n as u64 + 1;
        assert_eq!(
            e.buf,
            vec![Intent::SetSpeed(umbra_core::MetersPerSecond(
                Ent::expected_speed(n, 77)
            ))],
            "entity {n}"
        );
        let mut reference =
            umbra_core::SimRng::new(77).stream(umbra_core::StreamKey::entity(eid(n)));
        reference.next_u64();
        assert_eq!(
            e.rng.state(),
            reference.state(),
            "entity {n}: exactly one draw consumed"
        );
    }
}

/// A core instance that is not a plugin instance (default `as_any_mut`).
struct Foreign;

impl BehaviorInstance for Foreign {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        out: &mut IntentBuffer<'_>,
    ) -> Status {
        out.push(Intent::Hold);
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

/// ADR 0020 / ASM-123: an item the bridge does not recognise is ticked through `instance.tick`, never a
/// panic; recognised items in the same slice are still batched.
#[test]
#[ignore = "WI-9"]
fn downcast_failure_falls_back_to_instance_tick() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let batch = batch_of(&b);
    let own = Ent::new(1, &b, 5);
    let mut foreign = Ent::new(2, &b, 5);
    foreign.inst = Box::new(Foreign);
    let mut ents = [own, foreign];

    tick_batched(&batch, &mut ents, 0);

    assert_eq!(ents[0].status, Some(Status::Running));
    assert_eq!(
        ents[1].buf,
        vec![Intent::Hold],
        "the foreign instance ran its own tick"
    );
    assert_eq!(ents[1].status, Some(Status::Success));
    assert_eq!(env.host.faults(), Vec::new(), "not a fault");
}

/// ADR 0019: handles are destroyed instance first, then the behavior.
#[test]
#[ignore = "WI-9"]
fn instances_are_destroyed_before_their_behavior() {
    raw::clear();
    let env = env();
    let b = env.behavior("ok");
    let inst = b.instantiate(rate()).expect("instantiate");

    drop(b);
    assert!(
        !raw::events().contains(&"destroy_behavior".to_owned()),
        "an instance keeps its behavior alive"
    );
    drop(inst);

    let events = raw::events();
    let tail: Vec<&str> = events.iter().rev().take(2).map(String::as_str).collect();
    assert_eq!(tail, ["destroy_behavior", "destroy_instance"]);
    let _ = InstantiateError::Duration {
        what: String::new(),
        hz: 0,
    };
}
