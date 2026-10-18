//! M3 WI-4: `ParamTemplate` build-time `{ param = .. }` references (ADR 0012, ASM-076).
//! Requirements: REQ-B-10, ADR 0014.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde_json::json;
use umbra_core::{ParamTemplate, ParamValue, ReservedForm, SubstituteError, reserved_form};

fn args(pairs: &[(&str, ParamValue)]) -> BTreeMap<String, ParamValue> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

/// References are found at any depth and reported as JSON pointers in traversal order.
#[test]
fn given_nested_references_when_listed_then_pointers_and_names_are_returned() {
    let t = ParamTemplate::new(json!({
        "range_m": { "param": "engage_range_m" },
        "nested": { "list": [1, { "param": "speed" }], "x": { "param": "a" } },
        "plain": 3.0
    }));

    assert_eq!(
        t.references(),
        vec![
            ("/nested/list/1".to_owned(), "speed"),
            ("/nested/x".to_owned(), "a"),
            ("/range_m".to_owned(), "engage_range_m"),
        ]
    );
}

/// A reference at the root has the empty pointer; special characters are escaped (RFC 6901).
#[test]
fn given_root_table_and_odd_keys_when_listed_then_root_is_literal_and_pointers_escaped() {
    // The root is the params table itself: a param literally named `param` (ASM-089 b).
    let root = ParamTemplate::new(json!({ "param": "p" }));
    assert_eq!(root.references().len(), 0);
    assert_eq!(
        root.substitute(&BTreeMap::new()),
        Ok(json!({ "param": "p" }))
    );

    let odd = ParamTemplate::new(json!({ "a/b": { "param": "p" }, "c~d": { "param": "q" } }));
    assert_eq!(
        odd.references(),
        vec![("/a~1b".to_owned(), "p"), ("/c~0d".to_owned(), "q")]
    );
}

/// Look-alikes (extra keys, non-string value) are literals.
#[test]
fn given_look_alike_tables_when_listed_then_they_are_not_references() {
    let t = ParamTemplate::new(json!({
        "a": { "param": "p", "extra": 1 },
        "b": { "param": 3 },
        "c": { "key": "k" },
        "d": {}
    }));
    assert_eq!(t.references().len(), 0);
}

/// Substitution replaces whole values and keeps everything else.
#[test]
fn given_bound_names_when_substituted_then_whole_values_are_replaced() {
    let t = ParamTemplate::new(json!({
        "range_m": { "param": "r" },
        "list": [{ "param": "s" }, 1],
        "lit": "x"
    }));
    let out = t.substitute(&args(&[
        ("r", json!(2000.0)),
        ("s", json!({ "k": [1, 2] })),
    ]));

    assert_eq!(
        out,
        Ok(json!({ "range_m": 2000.0, "list": [{ "k": [1, 2] }, 1], "lit": "x" }))
    );
}

/// Look-alike tables survive substitution untouched; substituted values are not re-scanned.
#[test]
fn given_look_alike_and_param_valued_argument_when_substituted_then_left_alone() {
    let t = ParamTemplate::new(json!({
        "a": { "param": "p", "extra": 1 },
        "b": { "param": "q" }
    }));
    let out = t.substitute(&args(&[("q", json!({ "param": "p" }))]));

    assert_eq!(
        out,
        Ok(json!({ "a": { "param": "p", "extra": 1 }, "b": { "param": "p" } }))
    );
}

/// An unbound name is reported with its pointer; the first in traversal order.
#[test]
fn given_unbound_name_when_substituted_then_error_names_it() {
    let t = ParamTemplate::new(json!({ "a": { "param": "x" }, "b": { "param": "y" } }));
    let err = t.substitute(&args(&[("y", json!(1))]));

    assert_eq!(
        err,
        Err(SubstituteError::Unbound {
            pointer: "/a".into(),
            name: "x".into()
        })
    );
    assert!(err.map_err(|e| e.to_string()).unwrap_err().contains('x'));
}

/// A template without references substitutes to itself, and equality is structural.
#[test]
fn given_literal_template_when_substituted_then_identity() {
    let v = json!({ "a": [1, 2, { "b": null }] });
    let t = ParamTemplate::new(v.clone());
    assert_eq!(t.substitute(&BTreeMap::new()), Ok(v));
    assert_eq!(t.references().len(), 0);
}

/// With two unbound references the first in traversal order is reported; pointers nest.
#[test]
fn given_two_unbound_when_substituted_then_first_in_traversal_order_is_reported() {
    let t = ParamTemplate::new(json!({ "b": [{ "param": "y" }], "a": { "param": "x" } }));
    assert_eq!(
        t.substitute(&BTreeMap::new()),
        Err(SubstituteError::Unbound {
            pointer: "/a".into(),
            name: "x".into()
        })
    );
    let nested = ParamTemplate::new(json!({ "b": [0, { "param": "y" }] }));
    assert_eq!(
        nested.substitute(&BTreeMap::new()),
        Err(SubstituteError::Unbound {
            pointer: "/b/1".into(),
            name: "y".into()
        })
    );
}

/// Children of look-alike tables are still scanned.
#[test]
fn given_look_alike_with_reference_child_when_listed_then_child_is_found() {
    let t = ParamTemplate::new(json!({ "v": { "param": "p", "extra": { "param": "x" } } }));
    assert_eq!(t.references(), vec![("/v/extra".to_owned(), "x")]);

    let t = ParamTemplate::new(json!({ "v": { "param": { "param": "x" } } }));
    assert_eq!(t.references(), vec![("/v/param".to_owned(), "x")]);
}

/// A `{ key }` binding is not a build-time value: `substitute` refuses it.
#[test]
fn given_key_binding_when_substituted_then_binding_error() {
    let t = ParamTemplate::new(json!({ "list": [{ "key": "target" }] }));
    assert_eq!(t.references().len(), 0);
    assert_eq!(
        t.substitute(&BTreeMap::new()),
        Err(SubstituteError::Binding {
            pointer: "/list/0".into(),
            key: "target".into()
        })
    );
}

/// Truth table of the single classifier; only the value itself is classified.
#[test]
fn given_values_when_classified_then_only_exact_one_key_string_forms_match() {
    assert_eq!(
        reserved_form(&json!({ "param": "x" })),
        Some(ReservedForm::Param("x"))
    );
    assert_eq!(
        reserved_form(&json!({ "key": "x" })),
        Some(ReservedForm::Key("x"))
    );
    assert_eq!(reserved_form(&json!({ "param": "x", "key": "y" })), None);
    assert_eq!(reserved_form(&json!({ "param": "x", "other": 1 })), None);
    assert_eq!(reserved_form(&json!({ "param": 1 })), None);
    assert_eq!(reserved_form(&json!({ "key": null })), None);
    assert_eq!(reserved_form(&json!({ "other": "x" })), None);
    assert_eq!(reserved_form(&json!({})), None);
    assert_eq!(reserved_form(&json!([{ "param": "x" }])), None);
    assert_eq!(reserved_form(&json!({ "a": { "param": "x" } })), None);
    assert_eq!(reserved_form(&json!("param")), None);
}
