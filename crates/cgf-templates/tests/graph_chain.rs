//! WI-13 review follow-up: values supplied at a call site are followed down the chain of graphs
//! (REQ-B-10, ASM-076, ASM-106 d), mistyped references cause no cascade (ASM-056), and the place
//! of a cycle diagnostic (ASM-092 c).

#![forbid(unsafe_code)]

mod common;
mod graph_support;

use cgf_templates::{Diagnostic, SourceFile};
use common::{errors, lines, ok, pos_after, pos_of, row, rows, src};
use graph_support::graphs;

fn path(name: &str) -> String {
    format!("behaviors/{name}.behavior.toml")
}

fn found(items: &[(&str, &str)]) -> Vec<Diagnostic> {
    let files: Vec<SourceFile> = items.iter().map(|(n, t)| src(&path(n), t)).collect();
    errors(graphs(&files))
}

/// A graph `id` with an optional `[interface]` params line and the given node lines (root `r`).
fn graph(id: &str, params: &str, nodes: &[&str]) -> String {
    let mut all = vec![
        "schema_version = 1".to_string(),
        format!("id = \"{id}\""),
        "version = \"1.0.0\"".to_string(),
        "root = \"r\"".to_string(),
    ];
    if !params.is_empty() {
        all.push("[interface]".into());
        all.push(format!("params = {params}"));
    }
    all.extend(nodes.iter().map(|n| (*n).to_string()));
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    lines(&refs)
}

/// A graph whose root `r` calls `target` with `params`.
fn calls(id: &str, iface: &str, target: &str, params: &str) -> String {
    graph(
        id,
        iface,
        &[
            "[nodes.r]",
            "type = \"subgraph\"",
            &format!("ref = \"{target}\""),
            &format!("params = {params}"),
        ],
    )
}

const NOOP: [&str; 3] = [
    "[nodes.r]",
    "type = \"action\"",
    "ref = \"stub.action.noop\"",
];
const NUMBER: &str = "{ x = { type = \"number\" } }";
const MAX10: &str = "{ y = { type = \"number\", maximum = 10 } }";

/// A leaf `stub.cond.near` whose `range_m` is the param `p`.
fn near_of(id: &str, iface: &str, p: &str) -> String {
    graph(
        id,
        iface,
        &[
            "[nodes.r]",
            "type = \"condition\"",
            "ref = \"stub.cond.near\"",
            &format!("params = {{ range_m = {{ param = \"{p}\" }} }}"),
        ],
    )
}

fn note_text(d: &Diagnostic) -> String {
    d.notes
        .iter()
        .map(|n| n.message.clone())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// ASM-056 (M1 of the review): a `{ param }` reported as `invalid_binding` in the child is not
/// substituted again at the call site, so the caller gets no error.
#[test]
fn a_caller_of_a_graph_whose_leaf_has_a_mistyped_param_gets_no_error() {
    let child = near_of("test.c", "{ r = { type = \"string\" } }", "r");
    let parent = calls("test.p", "", "test.c", "{ r = \"x\" }");

    let d = found(&[("c", &child), ("p", &parent)]);

    assert_eq!(
        rows(&d),
        [row(
            "invalid_binding",
            path("c"),
            pos_of(&child, "{ param = \"r\" }")
        )]
    );
}

/// REQ-B-10: a value that reaches an interface through a `{ param }` of the middle graph is checked
/// against that interface, at the original call.
#[test]
fn value_passed_through_a_middle_graph_is_checked_against_the_last_interface() {
    let c = graph("test.c", MAX10, &NOOP);
    let b = calls("test.b", NUMBER, "test.c", "{ y = { param = \"x\" } }");
    let a = calls("test.a", "", "test.b", "{ x = 500.0 }");

    let d = found(&[("a", &a), ("b", &b), ("c", &c)]);

    assert_eq!(
        rows(&d),
        [row("param_out_of_range", path("a"), pos_after(&a, "x = "))]
    );
    assert!(
        note_text(&d[0]).contains("`test.b` -> `test.c`"),
        "{:?}",
        d[0].notes
    );
    let at = d[0].notes.iter().find_map(|n| n.location.as_ref());
    assert_eq!(
        at.map(|l| (l.path.clone(), (l.line, l.column))),
        Some((path("b").into(), pos_of(&b, "{ param = \"x\" }")))
    );
}

/// REQ-B-10, REQ-T-3: the same chain into a leaf is a located error at load.
#[test]
fn value_passed_through_a_middle_graph_is_checked_in_the_leaf() {
    let c = near_of("test.c", "{ y = { type = \"number\" } }", "y");
    let b = calls("test.b", NUMBER, "test.c", "{ y = { param = \"x\" } }");
    let a = calls("test.a", "", "test.b", "{ x = -5.0 }");

    let d = found(&[("a", &a), ("b", &b), ("c", &c)]);

    assert_eq!(
        rows(&d),
        [row("param_out_of_range", path("a"), pos_after(&a, "x = "))]
    );
}

/// A three-level chain: the value travels `a -> b -> c -> d`.
#[test]
fn value_is_followed_through_three_levels() {
    let d_graph = graph(
        "test.d",
        "{ z = { type = \"number\", maximum = 10 } }",
        &NOOP,
    );
    let c = calls(
        "test.c",
        "{ y = { type = \"number\" } }",
        "test.d",
        "{ z = { param = \"y\" } }",
    );
    let b = calls("test.b", NUMBER, "test.c", "{ y = { param = \"x\" } }");
    let a = calls("test.a", "", "test.b", "{ x = 500.0 }");
    let fine = calls("test.a", "", "test.b", "{ x = 5.0 }");

    let bad = found(&[("a", &a), ("b", &b), ("c", &c), ("d", &d_graph)]);
    let files: Vec<SourceFile> = [("a", &fine), ("b", &b), ("c", &c), ("d", &d_graph)]
        .iter()
        .map(|(n, t)| src(&path(n), t))
        .collect();

    assert_eq!(
        rows(&bad),
        [row("param_out_of_range", path("a"), pos_after(&a, "x = "))]
    );
    assert!(note_text(&bad[0]).contains("`test.b` -> `test.c` -> `test.d`"));
    assert_eq!(ok(graphs(&files)).len(), 4);
}

/// Into a registered behavior at the end of the chain (schema and dry run).
#[test]
fn value_is_followed_into_a_registered_behavior() {
    let b = calls(
        "test.b",
        NUMBER,
        "stub.behavior.plain",
        "{ speed = { param = \"x\" } }",
    );
    let a = calls("test.a", "", "test.b", "{ x = 60.0 }");

    let d = found(&[("a", &a), ("b", &b)]);

    assert_eq!(
        rows(&d),
        [row("param_out_of_range", path("a"), pos_after(&a, "x = "))]
    );
}

/// A param with no default and no supplied value is not known at load time: nothing is checked.
#[test]
fn param_without_default_or_value_is_unchecked() {
    let c = graph("test.c", MAX10, &NOOP);
    let b = calls("test.b", NUMBER, "test.c", "{ y = { param = \"x\" } }");

    assert_eq!(
        ok(graphs(&[src(&path("b"), &b), src(&path("c"), &c)])).len(),
        2
    );
}

/// ASM-056: values are not followed into a graph with a reader error (it is not usable), and a
/// cycle is entered once: no cascade, no hang.
#[test]
fn no_cascade_into_broken_or_cyclic_graphs() {
    let broken = format!("{}colour = 1\n", graph("test.c", MAX10, &NOOP));
    let b = calls("test.b", NUMBER, "test.c", "{ y = { param = \"x\" } }");
    let a = calls("test.a", "", "test.b", "{ x = 500.0 }");
    let loop_b = calls("test.b", NUMBER, "test.c", "{ x = { param = \"x\" } }");
    let loop_c = calls("test.c", NUMBER, "test.b", "{ x = { param = \"x\" } }");
    let entry = calls("test.a", "", "test.b", "{ x = 5.0 }");

    let into_broken = found(&[("a", &a), ("b", &b), ("c", &broken)]);
    let into_cycle = found(&[("a", &entry), ("b", &loop_b), ("c", &loop_c)]);

    assert_eq!(
        into_broken
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["unknown_key"]
    );
    assert_eq!(
        into_cycle
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["subgraph_cycle"]
    );
}

/// ASM-092 c: a cycle is reported at the first reference of the shortest cycle through the
/// smallest ID, which need not be that graph's first reference.
#[test]
fn cycle_is_reported_at_the_first_ref_of_the_shortest_cycle() {
    let two = |id: &str, a: &str, b: &str| {
        graph(
            id,
            "",
            &[
                "[nodes.r]",
                "type = \"sequence\"",
                "children = [\"c0\", \"c1\"]",
                "[nodes.c0]",
                "type = \"subgraph\"",
                &format!("ref = \"{a}\""),
                "[nodes.c1]",
                "type = \"subgraph\"",
                &format!("ref = \"{b}\""),
            ],
        )
    };
    let a = two("test.a", "test.x", "test.b");
    let b = calls("test.b", "", "test.a", "{ }");
    let x = calls("test.x", "", "stub.behavior.plain", "{ }");

    let d = found(&[("a", &a), ("b", &b), ("x", &x)]);

    assert_eq!(
        rows(&d),
        [row("subgraph_cycle", path("a"), pos_of(&a, "\"test.b\""))]
    );
    assert_eq!(
        d[0].message,
        "subgraph cycle: `test.a` -> `test.b` -> `test.a`"
    );
}

/// Cost: a ladder where every graph calls the next one twice with the same value is followed once
/// per (graph, value), not once per path (2^40 paths).
#[test]
fn a_ladder_of_double_calls_is_followed_once_per_graph() {
    let n = 40;
    let mut texts = Vec::new();
    for i in 0..n {
        let id = format!("test.l{i:02}");
        let text = if i + 1 == n {
            graph(&id, NUMBER, &NOOP)
        } else {
            let next = format!("test.l{:02}", i + 1);
            graph(
                &id,
                NUMBER,
                &[
                    "[nodes.r]",
                    "type = \"sequence\"",
                    "children = [\"a\", \"b\"]",
                    "[nodes.a]",
                    "type = \"subgraph\"",
                    &format!("ref = \"{next}\""),
                    "params = { x = { param = \"x\" } }",
                    "[nodes.b]",
                    "type = \"subgraph\"",
                    &format!("ref = \"{next}\""),
                    "params = { x = { param = \"x\" } }",
                ],
            )
        };
        texts.push((format!("l{i:02}"), text));
    }
    texts.push((
        "top".into(),
        calls("test.top", "", "test.l00", "{ x = 1.0 }"),
    ));
    let files: Vec<SourceFile> = texts.iter().map(|(n, t)| src(&path(n), t)).collect();

    let loaded = graph_support::graphs_with_depth(&files, 64);

    assert_eq!(ok(loaded).len(), 41);
}
