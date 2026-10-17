//! M3 WI-4: `ParamTemplate` build-time `{ param = .. }` references (ADR 0012, ASM-076).
//! Requirements: REQ-B-10, ADR 0014.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use cgf_core::{ParamTemplate, ParamValue};
use serde_json::json;

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
fn given_root_reference_and_odd_keys_when_listed_then_pointers_are_escaped() {
    let root = ParamTemplate::new(json!({ "param": "p" }));
    assert_eq!(root.references(), vec![(String::new(), "p")]);

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

    let e = err.expect_err("x is unbound");
    assert_eq!(e.name, "x");
    assert_eq!(e.pointer, "/a");
    assert!(e.to_string().contains('x'));
}

/// A template without references substitutes to itself, and equality is structural.
#[test]
fn given_literal_template_when_substituted_then_identity() {
    let v = json!({ "a": [1, 2, { "b": null }] });
    let t = ParamTemplate::new(v.clone());
    assert_eq!(t.substitute(&BTreeMap::new()), Ok(v));
    assert_eq!(t.references().len(), 0);
}
