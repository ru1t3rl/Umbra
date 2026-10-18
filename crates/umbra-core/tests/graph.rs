//! M3 WI-4: graph data model, node-type table, catalog types.
//! Requirements: REQ-B-6, REQ-B-7, REQ-B-10, REQ-B-13, REQ-B-14, REQ-B-17, ADR 0011, 0012, 0017,
//! ASM-076.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde_json::json;
use umbra_core::{
    Access, BbType, BehaviorId, BlackboardParam, Catalog, CatalogEntry, CatalogError, CatalogKind,
    ComponentId, ComponentInfo, Composite, Decorator, ExitDef, FsmDef, GraphDef, Interface,
    LeafRef, NODE_TYPES, NodeDef, NodeId, NodeKind, NodeKindTag, OutputPort, ParallelDef,
    ParallelPolicy, ParamSchema, ParamTemplate, Provenance, RepeatDef, StateDef, Status,
    SubgraphRef, TimedDef, TransitionDef, TransitionTarget, Trigger, Version, VersionReq,
};

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("{e}"))
}

fn empty_interface() -> Interface {
    Interface::new(
        ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap_or_else(|e| panic!("{e}")),
    )
}

fn sub(id: &str, req: Option<&str>) -> NodeKind {
    let s = SubgraphRef::new(cid(id), ParamTemplate::new(json!({})));
    NodeKind::Subgraph(match req {
        Some(r) => s.with_version(VersionReq::parse(r).unwrap_or_else(|e| panic!("{e}"))),
        None => s,
    })
}

fn leaf(id: &str) -> LeafRef {
    LeafRef::new(cid(id), ParamTemplate::new(json!({})))
}

fn graph() -> GraphDef {
    let nodes = vec![
        NodeDef::new(
            "main",
            NodeKind::Sequence(Composite::new(vec![
                NodeId(1),
                NodeId(2),
                NodeId(3),
                NodeId(4),
            ])),
        ),
        NodeDef::new("a", sub("example.subgraph.b", Some("^1.0"))),
        NodeDef::new("c", NodeKind::Condition(leaf("builtin.cond.chance"))),
        NodeDef::new("d", sub("example.subgraph.a", None)),
        NodeDef::new("e", NodeKind::Action(leaf("builtin.action.wait"))),
    ];
    GraphDef::new(
        BehaviorId::new("example.behavior.g").unwrap_or_else(|e| panic!("{e}")),
        Version::new(1, 2, 3),
        Provenance::File,
        empty_interface(),
        NodeId(0),
        nodes,
    )
}

/// `node` looks up by index and is `None` out of range.
#[test]
fn given_graph_when_node_looked_up_then_index_semantics() {
    let g = graph();
    assert_eq!(g.node(NodeId(0)).map(|n| n.name.as_str()), Some("main"));
    assert_eq!(g.node(NodeId(4)).map(|n| n.name.as_str()), Some("e"));
    assert!(g.node(NodeId(5)).is_none());
}

/// `subgraph_refs` lists subgraph nodes in node order with their ranges.
#[test]
fn given_graph_when_subgraph_refs_listed_then_in_node_order() {
    let g = graph();
    let refs = g.subgraph_refs();

    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].0.as_str(), "example.subgraph.b");
    assert_eq!(refs[0].1.map(ToString::to_string).as_deref(), Some("^1.0"));
    assert_eq!(refs[1].0.as_str(), "example.subgraph.a");
    assert!(refs[1].1.is_none());
}

/// `leaf_refs` lists conditions and actions in node order with their tag.
#[test]
fn given_graph_when_leaf_refs_listed_then_tagged_in_node_order() {
    let g = graph();
    let leaves: Vec<_> = g
        .leaf_refs()
        .into_iter()
        .map(|(t, id)| (t, id.as_str().to_owned()))
        .collect();

    assert_eq!(
        leaves,
        vec![
            (NodeKindTag::Condition, "builtin.cond.chance".to_owned()),
            (NodeKindTag::Action, "builtin.action.wait".to_owned()),
        ]
    );
}

/// `Interface` helpers: required = no default, binding-only = entity or position.
#[test]
fn given_interface_when_queried_then_required_and_binding_only() {
    let i = empty_interface()
        .with_type("r", BbType::Number)
        .with_type("t", BbType::Entity)
        .with_type("p", BbType::Position)
        .with_type("s", BbType::String)
        .with_default("r", json!(1.0));

    assert!(!i.is_required("r"));
    assert!(i.is_required("t"));
    assert!(i.is_required("s"));
    assert!(!i.is_required("unknown"));
    assert!(i.is_binding_only("t"));
    assert!(i.is_binding_only("p"));
    assert!(!i.is_binding_only("r"));
    assert!(!i.is_binding_only("unknown"));
}

/// REQ-B-7: exactly these node types, in this order; every schema parses and has examples.
#[test]
fn given_node_types_when_listed_then_exactly_the_req_b_7_names() {
    let names: Vec<&str> = NODE_TYPES.iter().map(|n| n.name).collect();
    assert_eq!(
        names,
        [
            "sequence",
            "selector",
            "parallel",
            "inverter",
            "succeed_always",
            "repeat",
            "cooldown",
            "timeout",
            "condition",
            "action",
            "subgraph",
            "fsm",
        ]
    );
}

#[test]
fn given_node_type_schemas_when_parsed_then_each_is_an_object_with_examples() {
    for n in NODE_TYPES {
        assert!(!n.description.is_empty(), "{}", n.name);
        let schema = ParamSchema::from_json_str(n.settings_schema)
            .unwrap_or_else(|e| panic!("{}: {e}", n.name));
        let examples = schema.as_json()["examples"].as_array();
        assert!(
            examples.is_some_and(|e| !e.is_empty()),
            "{} has no examples",
            n.name
        );
        assert_eq!(schema.as_json()["type"], "object", "{}", n.name);
    }
}

/// Tag names match the node-type table.
#[test]
fn given_node_kind_tags_when_named_then_match_node_types() {
    let g = graph();
    let tags: Vec<&str> = g.nodes.iter().map(|n| n.kind.tag().name()).collect();
    assert_eq!(
        tags,
        ["sequence", "subgraph", "condition", "subgraph", "action"]
    );
    for t in tags {
        assert!(NODE_TYPES.iter().any(|n| n.name == t), "{t}");
    }
}

/// REQ-B-13: builtin and plugin are read-only, file graphs are editable.
#[test]
fn given_provenance_when_read_only_asked_then_file_is_the_only_writable() {
    assert!(Provenance::Builtin.read_only());
    assert!(Provenance::Plugin.read_only());
    assert!(!Provenance::File.read_only());
}

/// `ComponentInfo` builders accumulate in call order.
#[test]
fn given_component_info_builders_when_chained_then_fields_are_set() {
    let info = ComponentInfo::new("d", Provenance::Builtin, Version::new(0, 1, 0))
        .with_blackboard_param(BlackboardParam::new(
            "store_as",
            BbType::Entity,
            Access::Write,
        ))
        .with_param_type("target", BbType::Entity)
        .with_output("hit", BbType::Bool)
        .with_subgraph(cid("example.subgraph.a"), None)
        .with_result("Success when the target was hit; Running while aiming.");

    assert_eq!(info.description, "d");
    assert_eq!(info.blackboard_params.len(), 1);
    assert_eq!(info.outputs.get("hit"), Some(&BbType::Bool));
    assert_eq!(info.subgraphs.len(), 1);
    assert_eq!(info.param_types.get("target"), Some(&BbType::Entity));
    assert_eq!(
        info.result.as_deref(),
        Some("Success when the target was hit; Running while aiming.")
    );
}

fn entry(kind: CatalogKind, id: &str) -> CatalogEntry {
    CatalogEntry::new(
        kind,
        id,
        "",
        Provenance::File,
        Version::new(1, 0, 0),
        ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap_or_else(|e| panic!("{e}")),
    )
}

fn catalog(entries: Vec<CatalogEntry>) -> Catalog {
    Catalog::from_entries(entries).unwrap_or_else(|e| panic!("{e}"))
}

/// The catalog is sorted by (kind, id) whatever the input order; `get` finds by both.
#[test]
fn given_unsorted_entries_when_catalog_built_then_sorted_by_kind_and_id() {
    let cat = catalog(vec![
        entry(CatalogKind::Behavior, "b.x"),
        entry(CatalogKind::Action, "z.a"),
        entry(CatalogKind::NodeType, "sequence"),
        entry(CatalogKind::Action, "a.a"),
    ]);

    let order: Vec<_> = cat
        .entries()
        .iter()
        .map(|e| (e.kind, e.id.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            (CatalogKind::NodeType, "sequence"),
            (CatalogKind::Action, "a.a"),
            (CatalogKind::Action, "z.a"),
            (CatalogKind::Behavior, "b.x"),
        ]
    );
    assert!(cat.get(CatalogKind::Action, "z.a").is_some());
    assert!(cat.get(CatalogKind::Condition, "z.a").is_none());
}

#[test]
fn given_empty_catalog_when_get_then_none() {
    let cat = catalog(Vec::new());
    assert_eq!(cat.entries().len(), 0);
    assert!(cat.get(CatalogKind::Action, "x").is_none());
}

/// A duplicate (kind, id) is rejected; the same id under different kinds is fine.
#[test]
fn given_duplicate_entries_when_catalog_built_then_error_else_ok() {
    let dup = Catalog::from_entries(vec![
        entry(CatalogKind::Action, "a.a"),
        entry(CatalogKind::Action, "a.a"),
    ]);
    assert_eq!(
        dup.err(),
        Some(CatalogError::Duplicate {
            kind: CatalogKind::Action,
            id: "a.a".into()
        })
    );

    let ok = catalog(vec![
        entry(CatalogKind::Action, "a.a"),
        entry(CatalogKind::Condition, "a.a"),
    ]);
    assert_eq!(ok.entries().len(), 2);
}

/// The kind order is the documented one.
#[test]
fn given_catalog_kinds_when_compared_then_node_type_condition_action_behavior() {
    assert!(CatalogKind::NodeType < CatalogKind::Condition);
    assert!(CatalogKind::Condition < CatalogKind::Action);
    assert!(CatalogKind::Action < CatalogKind::Behavior);
}

/// `read_only` is derived from the source (REQ-B-13); builders round-trip.
#[test]
fn given_entry_when_source_varies_then_read_only_follows_and_builders_round_trip() {
    let base = entry(CatalogKind::Behavior, "b.x");
    assert!(!base.read_only());

    let mut types = BTreeMap::new();
    types.insert("t".to_owned(), BbType::Entity);
    let mut outs = BTreeMap::new();
    outs.insert("o".to_owned(), BbType::Bool);
    let e = CatalogEntry::new(
        CatalogKind::Behavior,
        "b.y",
        "d",
        Provenance::Plugin,
        Version::new(2, 0, 0),
        ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap_or_else(|e| panic!("{e}")),
    )
    .with_param_types(types.clone())
    .with_outputs(outs.clone())
    .with_result("Success when done.");

    assert!(e.read_only());
    assert_eq!(e.param_types, types);
    assert_eq!(e.outputs, outs);
    assert_eq!(e.result.as_deref(), Some("Success when done."));
    let mut e = e;
    e.source = Provenance::Builtin;
    assert!(e.read_only());
}

/// Every tag name is the matching `NODE_TYPES` name, in table order.
#[test]
fn given_all_tags_when_named_then_equal_node_types_in_order() {
    let tags = [
        NodeKindTag::Sequence,
        NodeKindTag::Selector,
        NodeKindTag::Parallel,
        NodeKindTag::Inverter,
        NodeKindTag::SucceedAlways,
        NodeKindTag::Repeat,
        NodeKindTag::Cooldown,
        NodeKindTag::Timeout,
        NodeKindTag::Condition,
        NodeKindTag::Action,
        NodeKindTag::Subgraph,
        NodeKindTag::Fsm,
    ];
    let names: Vec<&str> = tags.iter().map(|t| t.name()).collect();
    let table: Vec<&str> = NODE_TYPES.iter().map(|n| n.name).collect();
    assert_eq!(names, table);
}

/// Drift guard: description and title in each schema match the table entry.
#[test]
fn given_node_types_when_compared_to_schemas_then_description_and_title_match() {
    for n in NODE_TYPES {
        let schema = ParamSchema::from_json_str(n.settings_schema)
            .unwrap_or_else(|e| panic!("{}: {e}", n.name));
        let doc = schema.as_json();
        assert_eq!(doc["description"], n.description, "{}", n.name);
        assert_eq!(
            doc["title"],
            format!("{} node settings", n.name),
            "{}",
            n.name
        );
    }
}

/// Settings structs: constructors set defaults, builders override.
#[test]
fn given_settings_structs_when_built_then_defaults_and_builders() {
    let c = Composite::new(vec![NodeId(1)]);
    assert!(!c.memory);
    assert!(c.with_memory(true).memory);

    let p = ParallelDef::new(vec![NodeId(1)]);
    assert_eq!(p.policy, ParallelPolicy::All);
    assert_eq!(
        p.with_policy(ParallelPolicy::Any).policy,
        ParallelPolicy::Any
    );

    assert_eq!(Decorator::new(NodeId(2)).child, NodeId(2));
    let r = RepeatDef::new(NodeId(2));
    assert!(r.count.is_none());
    assert_eq!(
        r.with_count(std::num::NonZeroU32::MIN).count,
        Some(std::num::NonZeroU32::MIN)
    );
    let t = TimedDef::new(NodeId(2), umbra_core::Seconds(1.5));
    assert_eq!(t.duration, umbra_core::Seconds(1.5));
}

/// FSM builders keep file order; output port and subgraph builders accumulate.
#[test]
fn given_fsm_and_subgraph_builders_when_chained_then_order_is_kept() {
    let fsm = FsmDef::new(
        0,
        vec![StateDef::new("a", NodeId(1)), StateDef::new("b", NodeId(2))],
    )
    .with_exit(ExitDef::new("done", Status::Success))
    .with_transition(TransitionDef::new(
        0,
        TransitionTarget::State(1),
        Trigger::When(NodeId(3)),
    ))
    .with_transition(TransitionDef::new(
        1,
        TransitionTarget::Exit(0),
        Trigger::On(Status::Success),
    ));
    assert_eq!(fsm.states.len(), 2);
    assert_eq!(fsm.exits.len(), 1);
    assert_eq!(fsm.transitions[0].from, 0);
    assert_eq!(fsm.transitions[1].trigger, Trigger::On(Status::Success));

    let s = SubgraphRef::new(cid("a.b"), ParamTemplate::new(json!({})))
        .with_input("target", "enemy")
        .with_output("done", "finished");
    assert_eq!(s.inputs.get("target").map(String::as_str), Some("enemy"));
    assert_eq!(s.outputs.get("done").map(String::as_str), Some("finished"));

    let i = empty_interface().with_output("done", OutputPort::new(BbType::Bool, "finished"));
    assert_eq!(
        i.outputs.get("done").map(|o| o.from.as_str()),
        Some("finished")
    );
}

/// `GraphDef` builders.
#[test]
fn given_graph_builders_when_chained_then_fields_are_set() {
    let g = graph()
        .with_description("d")
        .with_origin("a/b.behavior.toml")
        .with_blackboard_key("target", BbType::Entity);
    assert_eq!(g.description.as_deref(), Some("d"));
    assert_eq!(
        g.origin.as_deref(),
        Some(std::path::Path::new("a/b.behavior.toml"))
    );
    assert_eq!(g.blackboard.get("target"), Some(&BbType::Entity));
}
