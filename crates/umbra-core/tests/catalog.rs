//! M3 WI-5: `Catalog::from_registry`. Requirements: REQ-B-13, REQ-B-17, ADR 0011, ASM-084.

#![forbid(unsafe_code)]

mod common;

use common::{ActionStub, BehaviorStub, ConditionStub, MotionStub, SensorStub, TerrainStub, cid};
use umbra_core::{
    BbType, Catalog, CatalogKind, ComponentInfo, NODE_TYPES, Provenance, Registry, Version,
};

fn registry() -> Registry {
    let mut r = Registry::new();
    let behavior = ComponentInfo::new("patrols", Provenance::File, Version::new(1, 2, 3))
        .with_param_type("target", BbType::Entity)
        .with_param_type("range_m", BbType::Number)
        .with_output("done", BbType::Bool)
        .with_result("Success when the patrol is complete.");
    let leaf = |p| ComponentInfo::new("leaf", p, Version::new(0, 4, 0));
    r.register_behavior(BehaviorStub::arc("z.behavior"), behavior)
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_condition(ConditionStub::arc("m.cond"), leaf(Provenance::Builtin))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_condition(ConditionStub::arc("a.cond"), leaf(Provenance::Plugin))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_action(ActionStub::arc("a.action"), leaf(Provenance::Builtin))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_sensor(SensorStub::arc("a.sensor"))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_motion_model(MotionStub::arc("a.motion"))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_terrain(TerrainStub::arc("a.terrain"))
        .unwrap_or_else(|e| panic!("{e}"));
    r
}

/// Sorted by (kind, id); node types first, then conditions, actions, behaviors.
#[test]
fn given_registry_when_catalog_built_then_sorted_by_kind_then_id() {
    let cat = Catalog::from_registry(&registry());
    let order: Vec<(CatalogKind, &str)> = cat
        .entries()
        .iter()
        .map(|e| (e.kind, e.id.as_str()))
        .collect();

    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);

    let tail: Vec<_> = order
        .iter()
        .filter(|(k, _)| *k != CatalogKind::NodeType)
        .collect();
    assert_eq!(
        tail,
        [
            &(CatalogKind::Condition, "a.cond"),
            &(CatalogKind::Condition, "m.cond"),
            &(CatalogKind::Action, "a.action"),
            &(CatalogKind::Behavior, "z.behavior"),
        ]
    );
}

/// Node types: all twelve, built in, read-only, with their settings schema.
#[test]
fn given_catalog_when_node_types_read_then_all_present_builtin_and_read_only() {
    let cat = Catalog::from_registry(&Registry::new());
    let nodes: Vec<_> = cat
        .entries()
        .iter()
        .filter(|e| e.kind == CatalogKind::NodeType)
        .collect();

    assert_eq!(nodes.len(), NODE_TYPES.len());
    assert_eq!(cat.entries().len(), NODE_TYPES.len());
    let ids: Vec<&str> = nodes.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "node types are sorted by id");
    for t in NODE_TYPES {
        let e = cat
            .get(CatalogKind::NodeType, t.name)
            .unwrap_or_else(|| panic!("{} missing", t.name));
        assert_eq!(e.description, t.description);
        assert_eq!(e.source, Provenance::Builtin);
        assert!(e.read_only());
        assert_eq!(e.version.to_string(), env!("CARGO_PKG_VERSION"));
        assert!(e.params.as_json()["examples"].is_array());
    }
}

/// Provenance, version, description, outputs, result and read-only flow from the metadata.
#[test]
fn given_registered_metadata_when_catalog_built_then_entries_mirror_it() {
    let cat = Catalog::from_registry(&registry());

    let b = cat
        .get(CatalogKind::Behavior, "z.behavior")
        .unwrap_or_else(|| panic!("behavior entry"));
    assert_eq!(b.description, "patrols");
    assert_eq!(b.source, Provenance::File);
    assert!(!b.read_only());
    assert_eq!(b.version, Version::new(1, 2, 3));
    assert_eq!(b.outputs.get("done"), Some(&BbType::Bool));
    assert_eq!(
        b.result.as_deref(),
        Some("Success when the patrol is complete.")
    );
    assert_eq!(b.params.as_json()["type"], "object");

    let p = cat
        .get(CatalogKind::Condition, "a.cond")
        .unwrap_or_else(|| panic!("condition entry"));
    assert_eq!(p.source, Provenance::Plugin);
    assert!(p.read_only());
    let c = cat
        .get(CatalogKind::Condition, "m.cond")
        .unwrap_or_else(|| panic!("condition entry"));
    assert!(c.read_only());
}

/// A behavior entry's `param_types` equals its `ComponentInfo::param_types` (binding-only types
/// reach editors).
#[test]
fn given_behavior_with_entity_param_when_catalog_built_then_param_types_match_info() {
    let r = registry();
    let cat = Catalog::from_registry(&r);
    let entry = cat
        .get(CatalogKind::Behavior, "z.behavior")
        .unwrap_or_else(|| panic!("behavior entry"));
    let info = r.info(&cid("z.behavior")).unwrap_or_else(|| panic!("info"));

    assert_eq!(entry.param_types, info.param_types);
    assert_eq!(entry.param_types.get("target"), Some(&BbType::Entity));
}

/// Motion models, terrain and sensors are not catalog material.
#[test]
fn given_sensor_motion_terrain_registered_when_catalog_built_then_they_are_absent() {
    let cat = Catalog::from_registry(&registry());
    for id in ["a.sensor", "a.motion", "a.terrain"] {
        assert!(cat.entries().iter().all(|e| e.id != id), "{id}");
    }
    assert_eq!(cat.entries().len(), NODE_TYPES.len() + 4);
}

/// The catalog does not depend on registration order.
#[test]
fn given_other_registration_order_when_catalog_built_then_equal() {
    let mut r = Registry::new();
    let leaf = |p| ComponentInfo::new("leaf", p, Version::new(0, 4, 0));
    r.register_action(ActionStub::arc("a.action"), leaf(Provenance::Builtin))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_condition(ConditionStub::arc("a.cond"), leaf(Provenance::Plugin))
        .unwrap_or_else(|e| panic!("{e}"));
    r.register_condition(ConditionStub::arc("m.cond"), leaf(Provenance::Builtin))
        .unwrap_or_else(|e| panic!("{e}"));
    let behavior = ComponentInfo::new("patrols", Provenance::File, Version::new(1, 2, 3))
        .with_param_type("target", BbType::Entity)
        .with_param_type("range_m", BbType::Number)
        .with_output("done", BbType::Bool)
        .with_result("Success when the patrol is complete.");
    r.register_behavior(BehaviorStub::arc("z.behavior"), behavior)
        .unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(
        Catalog::from_registry(&r),
        Catalog::from_registry(&registry())
    );
}
