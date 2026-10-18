//! M3 WI-5: behavior, condition, action and sensor kinds in the `Registry`, with metadata and
//! `Clone`. Requirements: D-8, REQ-T-1, REQ-B-9, ADR 0011, ASM-063, ASM-084.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use common::{
    ActionStub, BehaviorStub, ConditionStub, MotionStub, SensorStub, TerrainStub, cid,
    empty_registry_with_one_of_each, info,
};
use umbra_core::{
    BbType, ComponentId, ComponentInfo, ComponentKind, Provenance, Registry, RegistryError, Version,
};

/// Duplicate IDs are rejected across all six kinds (ASM-063): every ordered pair of kinds, first
/// kept.
#[test]
fn given_id_taken_by_any_kind_when_registered_under_any_kind_then_duplicate() {
    type Reg = fn(&mut Registry, &str) -> Result<(), RegistryError>;
    let regs: [(ComponentKind, Reg); 6] = [
        (ComponentKind::MotionModel, |r, id| {
            r.register_motion_model(MotionStub::arc(id))
        }),
        (ComponentKind::Terrain, |r, id| {
            r.register_terrain(TerrainStub::arc(id))
        }),
        (ComponentKind::Behavior, |r, id| {
            r.register_behavior(BehaviorStub::arc(id), info())
        }),
        (ComponentKind::Condition, |r, id| {
            r.register_condition(ConditionStub::arc(id), info())
        }),
        (ComponentKind::Action, |r, id| {
            r.register_action(ActionStub::arc(id), info())
        }),
        (ComponentKind::Sensor, |r, id| {
            r.register_sensor(SensorStub::arc(id))
        }),
    ];
    for (first_kind, first) in &regs {
        for (second_kind, second) in &regs {
            let mut r = Registry::new();
            first(&mut r, "a.b").unwrap_or_else(|e| panic!("{e}"));
            let err = second(&mut r, "a.b").err();
            assert_eq!(
                err,
                Some(RegistryError::Duplicate {
                    kind: *first_kind,
                    id: cid("a.b")
                }),
                "{first_kind} then {second_kind}"
            );
            assert_eq!(r.kind_of(&cid("a.b")), Some(*first_kind));
            assert_eq!(
                r.ids(*second_kind).len(),
                usize::from(first_kind == second_kind)
            );
        }
    }
}

/// `ids`, `param_schema`, `kind_of` and the typed getters answer per kind.
#[test]
fn given_one_of_each_when_queried_then_each_kind_answers_for_itself() {
    let r = empty_registry_with_one_of_each();
    let pairs = [
        (ComponentKind::Behavior, "t.behavior"),
        (ComponentKind::Condition, "t.cond"),
        (ComponentKind::Action, "t.action"),
        (ComponentKind::Sensor, "t.sensor"),
    ];
    for (kind, id) in pairs {
        let id = cid(id);
        assert_eq!(r.kind_of(&id), Some(kind));
        assert_eq!(r.ids(kind), vec![&id]);
        assert!(r.param_schema(kind, &id).is_some(), "{kind}");
        for other in ComponentKind::ALL.iter().filter(|k| **k != kind) {
            assert!(r.param_schema(*other, &id).is_none(), "{kind} vs {other}");
        }
    }
    assert!(r.behavior(&cid("t.behavior")).is_some());
    assert!(r.condition(&cid("t.cond")).is_some());
    assert!(r.action(&cid("t.action")).is_some());
    assert!(r.sensor(&cid("t.sensor")).is_some());
    assert!(r.behavior(&cid("t.cond")).is_none());
    assert!(r.sensor(&cid("t.action")).is_none());
}

/// IDs of each kind are sorted whatever the registration order.
#[test]
fn given_unsorted_registration_when_ids_listed_then_sorted() {
    let mut r = Registry::new();
    for id in ["z.c", "a.c", "m.c"] {
        r.register_condition(ConditionStub::arc(id), info())
            .unwrap_or_else(|e| panic!("{e}"));
    }
    let ids: Vec<&str> = r
        .ids(ComponentKind::Condition)
        .into_iter()
        .map(ComponentId::as_str)
        .collect();
    assert_eq!(ids, ["a.c", "m.c", "z.c"]);
}

/// `info` is stored for behaviors, conditions and actions; sensors and others have none.
#[test]
fn given_registrations_when_info_asked_then_stored_metadata_is_returned() {
    let mut r = Registry::new();
    let meta = ComponentInfo::new("does x", Provenance::Plugin, Version::new(1, 2, 0))
        .with_param_type("target", BbType::Entity)
        .with_output("done", BbType::Bool);
    r.register_behavior(BehaviorStub::arc("a.b"), meta.clone())
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_sensor(SensorStub::arc("a.s"))
        .unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(r.info(&cid("a.b")), Some(&meta));
    assert!(r.info(&cid("a.s")).is_none());
    assert!(r.info(&cid("x.unknown")).is_none());
}

/// A rejected duplicate leaves the first entry and its info untouched.
#[test]
fn given_duplicate_when_rejected_then_first_factory_and_info_are_kept() {
    let mut r = Registry::new();
    let first = BehaviorStub::arc("a.b");
    let one = ComponentInfo::new("first", Provenance::File, Version::new(1, 0, 0));
    r.register_behavior(first.clone(), one.clone())
        .unwrap_or_else(|e| panic!("{e}"));
    let two = ComponentInfo::new("second", Provenance::File, Version::new(2, 0, 0));
    assert!(r.register_behavior(BehaviorStub::arc("a.b"), two).is_err());

    assert!(
        r.behavior(&cid("a.b"))
            .is_some_and(|f| Arc::ptr_eq(f, &first))
    );
    assert_eq!(r.info(&cid("a.b")), Some(&one));
}

/// Clone shares the factories (ADR 0011) and later registrations do not leak back.
#[test]
fn given_registry_when_cloned_then_factories_are_shared_and_clones_are_independent() {
    let mut base = Registry::new();
    let f = BehaviorStub::arc("a.b");
    base.register_behavior(f.clone(), info())
        .unwrap_or_else(|e| panic!("{e}"));
    let c = ConditionStub::arc("a.c");
    base.register_condition(c.clone(), info())
        .unwrap_or_else(|e| panic!("{e}"));
    let act = ActionStub::arc("a.act");
    base.register_action(act.clone(), info())
        .unwrap_or_else(|e| panic!("{e}"));
    let sen = SensorStub::arc("a.sen");
    base.register_sensor(sen.clone())
        .unwrap_or_else(|e| panic!("{e}"));

    let mut copy = base.clone();
    assert!(
        copy.behavior(&cid("a.b"))
            .is_some_and(|x| Arc::ptr_eq(x, &f))
    );
    assert!(
        copy.condition(&cid("a.c"))
            .is_some_and(|x| Arc::ptr_eq(x, &c))
    );
    assert!(
        copy.action(&cid("a.act"))
            .is_some_and(|x| Arc::ptr_eq(x, &act))
    );
    assert!(
        copy.sensor(&cid("a.sen"))
            .is_some_and(|x| Arc::ptr_eq(x, &sen))
    );
    assert_eq!(copy.info(&cid("a.b")), base.info(&cid("a.b")));

    copy.register_action(ActionStub::arc("a.new"), info())
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(base.action(&cid("a.new")).is_none());
    assert_eq!(base.kind_of(&cid("a.new")), None);
    assert_eq!(copy.kind_of(&cid("a.new")), Some(ComponentKind::Action));
}

fn check<T: Send + Sync + Clone>() {}

/// New kinds have the documented nouns and the registry stays `Send + Sync`.
#[test]
fn given_new_kinds_when_displayed_then_nouns_match() {
    assert_eq!(ComponentKind::Condition.to_string(), "condition");
    assert_eq!(ComponentKind::Action.to_string(), "action");
    check::<Registry>();
}

/// REQ-B-12, ASM-099 e: the longest chain below a registered behavior counts one level per
/// behavior, 1 without references, and stops `limit` levels down (also for looping metadata).
#[test]
fn longest_subgraph_chain_counts_registered_levels() {
    let mut r = Registry::new();
    let mut add = |id: &str, calls: &[&str]| {
        let mut described = info();
        for c in calls {
            described = described.with_subgraph(cid(c), None);
        }
        r.register_behavior(BehaviorStub::arc(id), described)
            .unwrap_or_else(|e| panic!("{e}"));
    };
    add("t.c", &[]);
    add("t.b", &["t.c"]);
    add("t.a", &["t.b", "t.c", "t.missing"]);
    add("t.loop", &["t.loop"]);

    let ids = |chain: Option<Vec<ComponentId>>| -> Vec<String> {
        chain
            .unwrap_or_default()
            .iter()
            .map(|i| i.as_str().to_string())
            .collect()
    };
    assert_eq!(ids(r.longest_subgraph_chain(&cid("t.c"), 16)), ["t.c"]);
    assert_eq!(
        ids(r.longest_subgraph_chain(&cid("t.a"), 16)),
        ["t.a", "t.b", "t.c"]
    );
    assert_eq!(ids(r.longest_subgraph_chain(&cid("t.loop"), 3)).len(), 4);
    assert!(r.longest_subgraph_chain(&cid("t.missing"), 16).is_none());
}
