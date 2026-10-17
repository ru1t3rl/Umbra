//! M3 WI-4: graph data model, node-type table, catalog types.
//! Requirements: REQ-B-6, REQ-B-7, REQ-B-10, REQ-B-13, REQ-B-14, REQ-B-17, ADR 0011, 0012, 0017,
//! ASM-076.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use cgf_core::{
    Access, BbType, BehaviorId, BlackboardParam, Catalog, CatalogEntry, CatalogKind, ComponentId,
    ComponentInfo, GraphDef, Interface, LeafRef, NODE_TYPES, NodeDef, NodeId, NodeKind,
    NodeKindTag, ParamSchema, ParamTemplate, Provenance, SubgraphRef, Version, VersionReq,
};
use serde_json::json;

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("{e}"))
}

fn empty_interface() -> Interface {
    Interface {
        params: ParamSchema::from_json_str(r#"{"type":"object"}"#)
            .unwrap_or_else(|e| panic!("{e}")),
        types: BTreeMap::new(),
        defaults: BTreeMap::new(),
        outputs: BTreeMap::new(),
        result: None,
    }
}

fn sub(id: &str, req: Option<&str>) -> NodeKind {
    NodeKind::Subgraph(SubgraphRef {
        id: cid(id),
        version: req.map(|r| VersionReq::parse(r).unwrap_or_else(|e| panic!("{e}"))),
        params: ParamTemplate::new(json!({})),
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
    })
}

fn leaf(id: &str) -> LeafRef {
    LeafRef {
        id: cid(id),
        params: ParamTemplate::new(json!({})),
    }
}

fn graph() -> GraphDef {
    let nodes = vec![
        NodeDef {
            name: "main".into(),
            kind: NodeKind::Sequence {
                children: vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)],
                memory: false,
            },
        },
        NodeDef {
            name: "a".into(),
            kind: sub("example.subgraph.b", Some("^1.0")),
        },
        NodeDef {
            name: "c".into(),
            kind: NodeKind::Condition(leaf("builtin.cond.chance")),
        },
        NodeDef {
            name: "d".into(),
            kind: sub("example.subgraph.a", None),
        },
        NodeDef {
            name: "e".into(),
            kind: NodeKind::Action(leaf("builtin.action.wait")),
        },
    ];
    GraphDef {
        id: BehaviorId::new("example.behavior.g").unwrap_or_else(|e| panic!("{e}")),
        version: Version::new(1, 2, 3),
        description: None,
        source: Provenance::File,
        origin: None,
        interface: empty_interface(),
        blackboard: BTreeMap::new(),
        root: NodeId(0),
        nodes,
    }
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
    let mut i = empty_interface();
    i.types.insert("r".into(), BbType::Number);
    i.types.insert("t".into(), BbType::Entity);
    i.types.insert("p".into(), BbType::Position);
    i.types.insert("s".into(), BbType::String);
    i.defaults.insert("r".into(), json!(1.0));

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
        .with_blackboard_param(BlackboardParam {
            param: "store_as".into(),
            ty: BbType::Entity,
            access: Access::Write,
        })
        .with_output("hit", BbType::Bool)
        .with_subgraph(cid("example.subgraph.a"), None)
        .with_result("hit");

    assert_eq!(info.description, "d");
    assert_eq!(info.blackboard_params.len(), 1);
    assert_eq!(info.outputs.get("hit"), Some(&BbType::Bool));
    assert_eq!(info.subgraphs.len(), 1);
    assert_eq!(info.result.as_deref(), Some("hit"));
}

fn entry(kind: CatalogKind, id: &str) -> CatalogEntry {
    CatalogEntry {
        kind,
        id: id.into(),
        description: String::new(),
        source: Provenance::File,
        version: Version::new(1, 0, 0),
        params: ParamSchema::from_json_str(r#"{"type":"object"}"#)
            .unwrap_or_else(|e| panic!("{e}")),
        outputs: BTreeMap::new(),
        read_only: false,
    }
}

/// The catalog is sorted by (kind, id) whatever the input order; `get` finds by both.
#[test]
fn given_unsorted_entries_when_catalog_built_then_sorted_by_kind_and_id() {
    let cat = Catalog::from_entries(vec![
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

/// The kind order is the documented one.
#[test]
fn given_catalog_kinds_when_compared_then_node_type_condition_action_behavior() {
    assert!(CatalogKind::NodeType < CatalogKind::Condition);
    assert!(CatalogKind::Condition < CatalogKind::Action);
    assert!(CatalogKind::Action < CatalogKind::Behavior);
}
