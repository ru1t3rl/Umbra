//! AT-1, AT-2 (set level), AT-3 (loader rows), AT-4 (network) and AT-9 (order independence):
//! template sets. Requirements: REQ-T-1, REQ-T-2, REQ-T-3, REQ-T-4, REQ-K-4 (loader determinism),
//! ADR 0010 (merge rules), ASM-055, ASM-056.
//!
//! Registries hold stub factories (`stub.motion.alpha`: `a` number > 0 required; `beta`: `b`
//! integer; `gamma`: `x`, `y`, `z` numbers), because this crate may not use `cgf-motion`.

#![forbid(unsafe_code)]

mod common;

use serde_json::json;

use cgf_templates::{Diagnostic, Loader, SourceFile};
use common::{
    ALPHA_SCHEMA, alpha_template, cid, errors, lines, loader, ok, pos_of, registry, registry_with,
    row, rows, src, templates, tid,
};

const ALPHA_ONLY: [(&str, &str); 1] = [("stub.motion.alpha", ALPHA_SCHEMA)];

fn gamma(id: &str, extends: Option<&str>, motion: &[&str]) -> String {
    let mut head = vec!["schema_version = 1".to_string(), format!("id = \"{id}\"")];
    if let Some(e) = extends {
        head.push(format!("extends = \"{e}\""));
    }
    head.extend(motion.iter().map(|l| (*l).to_string()));
    let refs: Vec<&str> = head.iter().map(String::as_str).collect();
    lines(&refs)
}

// --- AT-1: components are named by ID and resolved through the registry (REQ-T-1) ---------------

#[test]
fn given_registered_motion_id_then_template_resolves_to_that_id_and_params() {
    let set = ok(templates(&[src(
        "templates/t.toml",
        &alpha_template("tank.base", None, "2.5"),
    )]));

    let t = set
        .get(&tid("tank.base"))
        .unwrap_or_else(|| panic!("template missing"));
    assert_eq!(t.motion.id, cid("stub.motion.alpha"));
    assert_eq!(t.motion.params, json!({ "a": 2.5 }));
    assert_eq!(t.chain, [tid("tank.base")]);
    assert_eq!(t.source, std::path::Path::new("templates/t.toml"));
    assert_eq!(t.description, None);
    assert!(t.network.is_empty());
}

/// The loader keeps no list of built-in IDs: with nothing registered, a built-in ID is unknown.
#[test]
fn given_empty_registry_then_even_builtin_ids_are_unknown() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"truck.cargo\"",
        "[motion]",
        "model = \"builtin.ground.kinematic\"",
        "params = { max_speed_mps = 18.0 }",
    ]);
    let (registry, _) = registry_with(&[], &[]);

    let found = errors(loader(&registry).templates_from_sources(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("unknown_id", "t.toml", (4, 9))]);
    assert!(
        found[0].message.contains("builtin.ground.kinematic"),
        "{}",
        found[0].message
    );
    let notes: Vec<&str> = found[0].notes.iter().map(|n| n.message.as_str()).collect();
    assert!(
        notes.iter().any(|n| n.contains("none are registered")),
        "{notes:?}"
    );
}

/// REQ-T-1: a factory named like a built-in and one named like a plugin are indistinguishable.
#[test]
fn given_two_stub_factories_then_loader_treats_them_identically() {
    let (registry, _) = registry_with(
        &[
            ("builtin.stub.motion", ALPHA_SCHEMA),
            ("acme.plugin.motion", ALPHA_SCHEMA),
        ],
        &[],
    );
    let text = |id: &str, model: &str| {
        lines(&[
            "schema_version = 1",
            &format!("id = \"{id}\""),
            "[motion]",
            &format!("model = \"{model}\""),
            "params = { a = 3.0 }",
        ])
    };
    let files = [
        src("a.toml", &text("one.builtin", "builtin.stub.motion")),
        src("b.toml", &text("one.plugin", "acme.plugin.motion")),
    ];

    let set = ok(loader(&registry).templates_from_sources(&files));

    let get = |id: &str| {
        set.get(&tid(id))
            .unwrap_or_else(|| panic!("missing {id}"))
            .clone()
    };
    let (a, b) = (get("one.builtin"), get("one.plugin"));
    assert_eq!(a.motion.params, b.motion.params);
    assert_eq!(a.motion.params, json!({ "a": 3.0 }));
    assert_eq!(a.motion.id, cid("builtin.stub.motion"));
    assert_eq!(b.motion.id, cid("acme.plugin.motion"));
}

/// The plan's error example, exactly (location: the `model` value; note lists the registered IDs).
#[test]
fn given_unknown_motion_model_id_then_unknown_id_error_at_model_value_with_registered_ids_note() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"truck.wheeled\"",
        "[motion]",
        "model = \"builtin.ground.wheeled\"",
        "params = { }",
    ]);
    let (registry, _) = registry_with(&ALPHA_ONLY, &[]);

    let found = errors(loader(&registry).templates_from_sources(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("unknown_id", "t.toml", (4, 9))]);
    assert_eq!(
        found[0].message,
        "motion model `builtin.ground.wheeled` is not registered"
    );
    assert_eq!(found[0].notes.len(), 1);
    assert_eq!(
        found[0].notes[0].message,
        "registered motion models: stub.motion.alpha"
    );
}

/// ASM-055: behaviors have no registry entries in M2, so every behavior ID is unknown, honestly.
#[test]
fn given_behavior_section_then_unknown_id_error_noting_none_registered() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "[behavior]",
        "ref = \"builtin.fsm.patrol_and_engage\"",
        "params = { aggression = 0.7 }",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("unknown_id", "t.toml", (7, 7))]);
    assert!(
        found[0].message.contains("builtin.fsm.patrol_and_engage"),
        "{}",
        found[0].message
    );
    assert!(
        found[0].message.contains("behavior"),
        "{}",
        found[0].message
    );
    let notes: Vec<&str> = found[0].notes.iter().map(|n| n.message.as_str()).collect();
    assert!(
        notes.iter().any(|n| n.contains("none are registered")),
        "{notes:?}"
    );
}

#[test]
fn given_sensor_kind_then_unknown_id_error() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "[[sensors]]",
        "kind = \"builtin.visual\"",
        "params = { range_m = 3000.0, fov_deg = 120.0 }",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("unknown_id", "t.toml", (7, 8))]);
    assert!(
        found[0].message.contains("builtin.visual"),
        "{}",
        found[0].message
    );
    assert!(found[0].message.contains("sensor"), "{}", found[0].message);
}

#[test]
fn given_two_unknown_sensor_kinds_then_each_is_reported() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
        "[[sensors]]",
        "kind = \"builtin.visual\"",
        "[[sensors]]",
        "kind = \"builtin.radar\"",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [
            row("unknown_id", "t.toml", (7, 8)),
            row("unknown_id", "t.toml", (9, 8))
        ]
    );
}

// --- AT-2: set-level errors ---------------------------------------------------------------------

#[test]
fn given_unknown_extends_then_unknown_template_at_extends_value() {
    let text = alpha_template("tank.mbt", Some("ground_vehicle.bse"), "1.0");

    let found = errors(templates(&[src("examples/bad/tank.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("unknown_template", "examples/bad/tank.toml", (3, 11))]
    );
    assert_eq!(
        found[0].message,
        "template `ground_vehicle.bse` (extends) is not defined"
    );
}

#[test]
fn given_child_of_template_with_unknown_parent_then_no_cascade_error() {
    let orphan = alpha_template("tank.orphan", Some("nowhere.parent"), "1.0");
    let child = lines(&[
        "schema_version = 1",
        "id = \"tank.child\"",
        "extends = \"tank.orphan\"",
    ]);

    let found = errors(templates(&[src("a.toml", &orphan), src("b.toml", &child)]));

    assert_eq!(rows(&found), [row("unknown_template", "a.toml", (3, 11))]);
}

#[test]
fn given_self_extends_then_inheritance_cycle() {
    let text = alpha_template("loop.self", Some("loop.self"), "1.0");

    let found = errors(templates(&[src("s.toml", &text)]));

    assert_eq!(rows(&found), [row("inheritance_cycle", "s.toml", (3, 11))]);
    assert!(
        found[0].message.contains("`loop.self` -> `loop.self`"),
        "{}",
        found[0].message
    );
}

/// The plan's example, exactly: one error at the `extends` of the smallest ID, one note per other
/// edge of the cycle.
#[test]
fn given_two_template_cycle_then_one_cycle_error_with_note_per_edge() {
    let a = alpha_template("a.one", Some("b.two"), "1.0");
    let b = alpha_template("b.two", Some("a.one"), "1.0");

    let found = errors(templates(&[
        src("templates/b.toml", &b),
        src("templates/a.toml", &a),
    ]));

    assert_eq!(
        rows(&found),
        [row("inheritance_cycle", "templates/a.toml", (3, 11))]
    );
    assert_eq!(
        found[0].message,
        "template inheritance cycle: `a.one` -> `b.two` -> `a.one`"
    );
    assert_eq!(found[0].notes.len(), 1);
    assert_eq!(found[0].notes[0].message, "`b.two` extends `a.one`");
    let at = found[0].notes[0]
        .location
        .as_ref()
        .map(|l| (l.path.clone(), l.line, l.column));
    assert_eq!(at, Some(("templates/b.toml".into(), 3, 11)));
}

/// The plan's rendering, exactly (path:line:col: error[code]: message, then indented note lines).
#[test]
fn rendered_cycle_diagnostic_has_a_note_line_per_edge() {
    let a = alpha_template("a.one", Some("b.two"), "1.0");
    let b = alpha_template("b.two", Some("a.one"), "1.0");

    let found = errors(templates(&[
        src("templates/a.toml", &a),
        src("templates/b.toml", &b),
    ]));

    assert_eq!(
        found[0].to_string(),
        "templates/a.toml:3:11: error[inheritance_cycle]: template inheritance cycle: `a.one` -> `b.two` -> `a.one`\n  note: `b.two` extends `a.one` (templates/b.toml:3:11)"
    );
}

/// One diagnostic per cycle, at the smallest ID, whatever order the files arrive in.
#[test]
fn given_three_template_cycle_then_reported_once_at_smallest_id() {
    let a = alpha_template("a.one", Some("b.two"), "1.0");
    let b = alpha_template("b.two", Some("c.three"), "1.0");
    let c = alpha_template("c.three", Some("a.one"), "1.0");

    let found = errors(templates(&[
        src("templates/c.toml", &c),
        src("templates/a.toml", &a),
        src("templates/b.toml", &b),
    ]));

    assert_eq!(
        rows(&found),
        [row("inheritance_cycle", "templates/a.toml", (3, 11))]
    );
    assert_eq!(
        found[0].message,
        "template inheritance cycle: `a.one` -> `b.two` -> `c.three` -> `a.one`"
    );
    let notes: Vec<&str> = found[0].notes.iter().map(|n| n.message.as_str()).collect();
    assert_eq!(
        notes,
        ["`b.two` extends `c.three`", "`c.three` extends `a.one`"]
    );
    let note_paths: Vec<_> = found[0]
        .notes
        .iter()
        .map(|n| {
            n.location
                .as_ref()
                .map(|l| (l.path.clone(), l.line, l.column))
        })
        .collect();
    assert_eq!(
        note_paths,
        [
            Some(("templates/b.toml".into(), 3, 11)),
            Some(("templates/c.toml".into(), 3, 11)),
        ]
    );
}

#[test]
fn given_two_independent_cycles_then_one_error_each() {
    let files = [
        src("a.toml", &alpha_template("a.one", Some("b.two"), "1.0")),
        src("b.toml", &alpha_template("b.two", Some("a.one"), "1.0")),
        src("x.toml", &alpha_template("x.one", Some("y.two"), "1.0")),
        src("y.toml", &alpha_template("y.two", Some("x.one"), "1.0")),
    ];

    let found = errors(templates(&files));

    assert_eq!(
        rows(&found),
        [
            row("inheritance_cycle", "a.toml", (3, 11)),
            row("inheritance_cycle", "x.toml", (3, 11))
        ]
    );
}

/// REQ-T-2: the root cause is reported once; templates that depend on a cycle get no follow-on.
#[test]
fn given_child_of_cycle_then_no_cascade_error() {
    let files = [
        src("a.toml", &alpha_template("a.one", Some("b.two"), "1.0")),
        src("b.toml", &alpha_template("b.two", Some("a.one"), "1.0")),
        src(
            "d.toml",
            &lines(&[
                "schema_version = 1",
                "id = \"d.child\"",
                "extends = \"a.one\"",
            ]),
        ),
        src(
            "e.toml",
            &lines(&[
                "schema_version = 1",
                "id = \"e.grandchild\"",
                "extends = \"d.child\"",
            ]),
        ),
    ];

    let found = errors(templates(&files));

    assert_eq!(rows(&found), [row("inheritance_cycle", "a.toml", (3, 11))]);
}

#[test]
fn given_duplicate_template_id_in_two_files_then_duplicate_id_with_note_at_first() {
    let text = alpha_template("dup.one", None, "1.0");

    let found = errors(templates(&[src("b.toml", &text), src("a.toml", &text)]));

    assert_eq!(rows(&found), [row("duplicate_id", "b.toml", (2, 6))]);
    assert_eq!(found[0].notes.len(), 1);
    let at = found[0].notes[0]
        .location
        .as_ref()
        .map(|l| (l.path.clone(), l.line, l.column));
    assert_eq!(at, Some(("a.toml".into(), 2, 6)));
}

#[test]
fn given_child_of_duplicate_id_then_no_cascade_error() {
    let dup = alpha_template("dup.one", None, "1.0");
    let child = lines(&[
        "schema_version = 1",
        "id = \"dup.child\"",
        "extends = \"dup.one\"",
    ]);

    let found = errors(templates(&[
        src("a.toml", &dup),
        src("b.toml", &dup),
        src("c.toml", &child),
    ]));

    assert_eq!(rows(&found), [row("duplicate_id", "b.toml", (2, 6))]);
}

/// The plan's example, exactly.
#[test]
fn given_missing_required_param_then_missing_param_at_params_table() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { }",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("missing_param", "t.toml", (5, 10))]);
    assert_eq!(
        found[0].message,
        "motion model `stub.motion.alpha`: missing required parameter `a`"
    );
}

/// A header-style params table is located at its `[` (the plan's `6:1` example shape).
#[test]
fn given_header_style_params_table_then_missing_param_at_the_header_and_unknown_param_at_the_key() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "[motion.params]",
        "zzz = 1.0",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [
            row("missing_param", "t.toml", (5, 1)),
            row("unknown_param", "t.toml", (6, 1))
        ]
    );
}

/// The plan's example, exactly.
#[test]
fn given_string_for_number_param_then_param_wrong_type_at_value() {
    let text = alpha_template("tank.base", None, "\"fast\"");

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("param_wrong_type", "t.toml", pos_of(&text, "\"fast\""))]
    );
    assert_eq!(
        found[0].message,
        "motion model `stub.motion.alpha`: parameter `a` must be a number, found string"
    );
}

#[test]
fn given_zero_for_exclusive_minimum_then_param_out_of_range() {
    let text = alpha_template("tank.base", None, "0.0");

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("param_out_of_range", "t.toml", pos_of(&text, "0.0"))]
    );
}

#[test]
fn given_extra_param_then_unknown_param() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"tank.base\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0, zzz = 2 }",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("unknown_param", "t.toml", pos_of(&text, "zzz"))]
    );
    assert!(found[0].message.contains("`zzz`"), "{}", found[0].message);
}

/// REQ-T-2 dedup: a bad value inherited by two children is one error, located where it is written.
#[test]
fn given_inherited_bad_param_then_reported_once_at_parent_file() {
    let base = alpha_template("base.bad", None, "\"bad\"");
    let child = |id: &str| {
        lines(&[
            "schema_version = 1",
            &format!("id = \"{id}\""),
            "extends = \"base.bad\"",
        ])
    };

    let found = errors(templates(&[
        src("templates/base.toml", &base),
        src("templates/c1.toml", &child("kid.one")),
        src("templates/c2.toml", &child("kid.two")),
    ]));

    assert_eq!(
        rows(&found),
        [row(
            "param_wrong_type",
            "templates/base.toml",
            pos_of(&base, "\"bad\"")
        )]
    );
}

#[test]
fn given_factory_build_rejects_then_param_invalid_at_that_param_value() {
    let text = alpha_template("tank.base", None, "5000.0");

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("param_invalid", "t.toml", pos_of(&text, "5000.0"))]
    );
    assert!(
        found[0].message.contains("unreasonably large"),
        "{}",
        found[0].message
    );
}

/// The `Factory::build` contract: `params` has passed the schema; a schema violation never reaches it.
#[test]
fn given_schema_violation_then_factory_build_is_not_called() {
    let (registry, counters) = registry();
    let text = alpha_template("tank.base", None, "\"fast\"");

    let result = loader(&registry).templates_from_sources(&[src("t.toml", &text)]);

    assert!(result.is_err());
    assert_eq!(counters.motion(), 0);
}

/// ASM-058: every template is dry-run through `Factory::build` once, so `validate` catches what
/// spawning would.
#[test]
fn given_parent_and_child_then_factory_build_is_called_once_per_template() {
    let (registry, counters) = registry();
    let parent = alpha_template("tank.base", None, "1.0");
    let child = lines(&[
        "schema_version = 1",
        "id = \"tank.child\"",
        "extends = \"tank.base\"",
    ]);

    let result =
        loader(&registry).templates_from_sources(&[src("a.toml", &parent), src("b.toml", &child)]);

    assert!(result.is_ok());
    assert_eq!(counters.motion(), 2);
}

/// REQ-T-2 headline: five independent errors in three files, mixed codes, all reported, in stable
/// order (path, then position).
#[test]
fn given_five_independent_errors_in_three_files_then_all_five_reported_in_stable_order() {
    let a = lines(&[
        "schema_version = 1",
        "id = \"five.a\"",
        "colour = \"red\"",
        "description = 5",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 1.0 }",
    ]);
    let b = alpha_template("five.b", Some("five.nope"), "1.0");
    let c = lines(&[
        "schema_version = 1",
        "id = \"five.c\"",
        "[motion]",
        "model = \"stub.motion.alpha\"",
        "params = { a = 0.0 }",
        "[behavior]",
        "ref = \"builtin.fsm.patrol_and_engage\"",
    ]);
    let files = [
        src("templates/c.toml", &c),
        src("templates/a.toml", &a),
        src("templates/b.toml", &b),
    ];

    let found = errors(templates(&files));

    assert_eq!(
        rows(&found),
        [
            row("unknown_key", "templates/a.toml", (3, 1)),
            row("wrong_type", "templates/a.toml", (4, 15)),
            row("unknown_template", "templates/b.toml", (3, 11)),
            row("param_out_of_range", "templates/c.toml", pos_of(&c, "0.0")),
            row("unknown_id", "templates/c.toml", (7, 7)),
        ]
    );
}

// --- AT-3: the loader refuses components whose schema it cannot evaluate -----------------------

#[test]
fn given_component_schema_outside_subset_then_loader_new_fails_naming_component() {
    let bad = r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^x"}}}"#;
    let (registry, _) = registry_with(&[("stub.motion.bad", bad)], &[]);

    let error = Loader::new(&registry).err();

    let error =
        error.unwrap_or_else(|| panic!("a schema outside the subset must fail Loader::new"));
    let text = error.to_string();
    assert!(text.contains("stub.motion.bad"), "{text}");
    assert!(text.contains("pattern"), "{text}");
}

// --- AT-4: opaque `network.<adapter>` sections (REQ-T-4) ---------------------------------------

fn network_template(id: &str, extends: Option<&str>, model: &[&str], network: &[&str]) -> String {
    let mut body: Vec<&str> = model.to_vec();
    body.extend_from_slice(network);
    gamma(id, extends, &body)
}

const GAMMA_MOTION: [&str; 3] = [
    "[motion]",
    "model = \"stub.motion.gamma\"",
    "params = { x = 1.0 }",
];

#[test]
fn network_section_is_preserved_verbatim_and_not_validated() {
    let text = network_template(
        "net.parent",
        None,
        &GAMMA_MOTION,
        &[
            "[network.dis]",
            "entity_type = \"1.1.225.1.1.1.0\"",
            "anything_goes = true",
            "count = 5",
            "negative = -7",
            "ratio = 2.5",
            "tags = [\"a\", 1, 2.5]",
            "[network.dis.nested]",
            "deep = { k = \"v\" }",
            "[network.hla]",
            "fom_module = \"x.xml\"",
        ],
    );

    let set = ok(templates(&[src("t.toml", &text)]));

    let t = set
        .get(&tid("net.parent"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(
        t.network.get("dis"),
        Some(&json!({
            "entity_type": "1.1.225.1.1.1.0",
            "anything_goes": true,
            "count": 5,
            "negative": -7,
            "ratio": 2.5,
            "tags": ["a", 1, 2.5],
            "nested": { "deep": { "k": "v" } }
        }))
    );
    assert_eq!(
        t.network.get("hla"),
        Some(&json!({ "fom_module": "x.xml" }))
    );
    assert_eq!(t.network.len(), 2);
}

#[test]
fn child_network_adapter_table_replaces_parent_table_whole() {
    let parent = network_template(
        "net.parent",
        None,
        &GAMMA_MOTION,
        &["[network.dis]", "a = 1", "b = 2"],
    );
    let child = network_template(
        "net.child",
        Some("net.parent"),
        &[],
        &["[network.dis]", "b = 3"],
    );

    let set = ok(templates(&[src("a.toml", &parent), src("b.toml", &child)]));

    let t = set
        .get(&tid("net.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.network.get("dis"), Some(&json!({ "b": 3 })));
}

#[test]
fn other_adapters_are_inherited() {
    let parent = network_template(
        "net.parent",
        None,
        &GAMMA_MOTION,
        &["[network.dis]", "a = 1", "[network.hla]", "fom = \"x\""],
    );
    let child = network_template(
        "net.child",
        Some("net.parent"),
        &[],
        &["[network.dis]", "b = 3"],
    );

    let set = ok(templates(&[src("a.toml", &parent), src("b.toml", &child)]));

    let t = set
        .get(&tid("net.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.network.get("dis"), Some(&json!({ "b": 3 })));
    assert_eq!(t.network.get("hla"), Some(&json!({ "fom": "x" })));
}

#[test]
fn child_without_network_inherits_every_adapter() {
    let parent = network_template(
        "net.parent",
        None,
        &GAMMA_MOTION,
        &["[network.dis]", "a = 1"],
    );
    let child = network_template("net.child", Some("net.parent"), &[], &[]);

    let set = ok(templates(&[src("a.toml", &parent), src("b.toml", &child)]));

    let t = set
        .get(&tid("net.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.network.get("dis"), Some(&json!({ "a": 1 })));
}

#[test]
fn network_entry_must_be_a_table() {
    let text = network_template("net.bad", None, &GAMMA_MOTION, &["[network]", "dis = 5"]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("wrong_type", "t.toml", pos_of(&text, "5"))]
    );
}

#[test]
fn network_section_must_be_a_table() {
    let text = lines(&[
        "schema_version = 1",
        "id = \"net.bad\"",
        "network = 5",
        "[motion]",
        "model = \"stub.motion.gamma\"",
    ]);

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(rows(&found), [row("wrong_type", "t.toml", (3, 11))]);
}

#[test]
fn datetime_in_network_section_is_unsupported_value() {
    let text = network_template(
        "net.bad",
        None,
        &GAMMA_MOTION,
        &["[network.dis]", "when = 1979-05-27"],
    );

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("unsupported_value", "t.toml", pos_of(&text, "1979"))]
    );
}

/// Adapter names are `[a-z][a-z0-9_]*`; the error is at the name (key).
#[test]
fn invalid_adapter_name_is_invalid_value() {
    let text = network_template(
        "net.bad",
        None,
        &GAMMA_MOTION,
        &["[network.Bad-Name]", "k = 1"],
    );

    let found = errors(templates(&[src("t.toml", &text)]));

    assert_eq!(
        rows(&found),
        [row("invalid_value", "t.toml", pos_of(&text, "Bad-Name"))]
    );
}

#[test]
fn adapter_name_with_digits_and_underscore_is_accepted() {
    let text = network_template("net.ok", None, &GAMMA_MOTION, &["[network.hla_2]", "k = 1"]);

    let set = ok(templates(&[src("t.toml", &text)]));

    let t = set.get(&tid("net.ok")).unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.network.get("hla_2"), Some(&json!({ "k": 1 })));
}

// --- ADR 0010: merge rules ---------------------------------------------------------------------

const PARENT_PARAMS: [&str; 4] = [
    "description = \"Parent\"",
    "[motion]",
    "model = \"stub.motion.gamma\"",
    "params = { x = 1.0, y = 2.0 }",
];

fn parent_text() -> String {
    gamma("veh.parent", None, &PARENT_PARAMS)
}

#[test]
fn child_params_merge_key_wise_with_parent_params() {
    let child = gamma(
        "veh.child",
        Some("veh.parent"),
        &["[motion]", "params = { y = 5.0, z = 7.0 }"],
    );

    let set = ok(templates(&[
        src("a.toml", &parent_text()),
        src("b.toml", &child),
    ]));

    let t = set
        .get(&tid("veh.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.motion.id, cid("stub.motion.gamma"));
    assert_eq!(t.motion.params, json!({ "x": 1.0, "y": 5.0, "z": 7.0 }));
}

#[test]
fn child_without_motion_section_inherits_model_and_params() {
    let child = gamma("veh.child", Some("veh.parent"), &[]);

    let set = ok(templates(&[
        src("a.toml", &parent_text()),
        src("b.toml", &child),
    ]));

    let t = set
        .get(&tid("veh.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.motion.id, cid("stub.motion.gamma"));
    assert_eq!(t.motion.params, json!({ "x": 1.0, "y": 2.0 }));
}

/// Naming a different model drops the parent's params (they belong to the parent's model).
#[test]
fn child_naming_a_different_model_drops_the_parents_params() {
    let child = gamma(
        "veh.child",
        Some("veh.parent"),
        &["[motion]", "model = \"stub.motion.beta\""],
    );

    let set = ok(templates(&[
        src("a.toml", &parent_text()),
        src("b.toml", &child),
    ]));

    let t = set
        .get(&tid("veh.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.motion.id, cid("stub.motion.beta"));
    assert_eq!(t.motion.params, json!({}));
}

#[test]
fn child_naming_the_same_model_again_keeps_merging_params() {
    let child = gamma(
        "veh.child",
        Some("veh.parent"),
        &[
            "[motion]",
            "model = \"stub.motion.gamma\"",
            "params = { z = 3.0 }",
        ],
    );

    let set = ok(templates(&[
        src("a.toml", &parent_text()),
        src("b.toml", &child),
    ]));

    let t = set
        .get(&tid("veh.child"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(t.motion.params, json!({ "x": 1.0, "y": 2.0, "z": 3.0 }));
}

#[test]
fn description_and_id_are_not_inherited() {
    let child = gamma("veh.child", Some("veh.parent"), &[]);
    let described = gamma("veh.other", Some("veh.parent"), &["description = \"Mine\""]);

    let set = ok(templates(&[
        src("a.toml", &parent_text()),
        src("b.toml", &child),
        src("c.toml", &described),
    ]));

    let get = |id: &str| {
        set.get(&tid(id))
            .unwrap_or_else(|| panic!("missing {id}"))
            .clone()
    };
    assert_eq!(get("veh.parent").description.as_deref(), Some("Parent"));
    assert_eq!(get("veh.child").description, None);
    assert_eq!(get("veh.other").description.as_deref(), Some("Mine"));
    assert_eq!(get("veh.child").id, tid("veh.child"));
}

#[test]
fn chain_lists_self_first_and_root_last_and_params_fold_root_first() {
    let mid = gamma(
        "veh.mid",
        Some("veh.parent"),
        &["[motion]", "params = { y = 20.0 }"],
    );
    let leaf = gamma(
        "veh.leaf",
        Some("veh.mid"),
        &["[motion]", "params = { x = 100.0 }"],
    );

    let set = ok(templates(&[
        src("c.toml", &leaf),
        src("a.toml", &parent_text()),
        src("b.toml", &mid),
    ]));

    let t = set
        .get(&tid("veh.leaf"))
        .unwrap_or_else(|| panic!("missing"));
    assert_eq!(
        t.chain,
        [tid("veh.leaf"), tid("veh.mid"), tid("veh.parent")]
    );
    assert_eq!(t.motion.params, json!({ "x": 100.0, "y": 20.0 }));
    assert_eq!(set.len(), 3);
}

#[test]
fn template_set_iterates_in_id_order() {
    let files = [
        src("1.toml", &gamma("zeta.one", None, &GAMMA_MOTION)),
        src("2.toml", &gamma("alpha.one", None, &GAMMA_MOTION)),
        src("3.toml", &gamma("mid.one", None, &GAMMA_MOTION)),
    ];

    let set = ok(templates(&files));

    let ids: Vec<String> = set.iter().map(|t| t.id.to_string()).collect();
    assert_eq!(ids, ["alpha.one", "mid.one", "zeta.one"]);
}

// --- AT-9: the result does not depend on the order files arrive in -----------------------------

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn go(items: &mut Vec<usize>, k: usize, out: &mut Vec<Vec<usize>>) {
        if k == items.len() {
            out.push(items.clone());
            return;
        }
        for i in k..items.len() {
            items.swap(k, i);
            go(items, k + 1, out);
            items.swap(k, i);
        }
    }
    let mut out = Vec::new();
    go(&mut (0..n).collect(), 0, &mut out);
    out
}

#[test]
fn shuffled_source_order_gives_identical_template_set_and_diagnostics() {
    let valid = [
        src("a.toml", &gamma("veh.a", None, &GAMMA_MOTION)),
        src(
            "b.toml",
            &gamma(
                "veh.b",
                Some("veh.a"),
                &["[motion]", "params = { y = 2.0 }"],
            ),
        ),
        src(
            "c.toml",
            &gamma(
                "veh.c",
                Some("veh.b"),
                &["[motion]", "params = { z = 3.0 }"],
            ),
        ),
        src("d.toml", &gamma("veh.d", Some("veh.a"), &[])),
    ];
    let broken = [
        src("a.toml", &alpha_template("e.one", Some("e.two"), "1.0")),
        src("b.toml", &alpha_template("e.two", Some("e.one"), "1.0")),
        src(
            "c.toml",
            &alpha_template("e.three", Some("e.nope"), "\"bad\""),
        ),
        src("d.toml", &alpha_template("e.four", None, "0.0")),
    ];
    let reference_set = ok(templates(&valid));
    let reference_errors = errors(templates(&broken));

    for order in permutations(4) {
        let pick = |files: &[SourceFile]| -> Vec<SourceFile> {
            order.iter().map(|&i| files[i].clone()).collect()
        };

        assert_eq!(
            ok(templates(&pick(&valid))),
            reference_set,
            "order {order:?}"
        );
        let got: Vec<Diagnostic> = errors(templates(&pick(&broken)));
        assert_eq!(got, reference_errors, "order {order:?}");
    }
}

#[test]
fn loading_the_same_files_twice_gives_equal_results() {
    let files = [
        src("a.toml", &gamma("veh.a", None, &GAMMA_MOTION)),
        src("b.toml", &gamma("veh.b", Some("veh.a"), &[])),
    ];

    assert_eq!(ok(templates(&files)), ok(templates(&files)));
}
