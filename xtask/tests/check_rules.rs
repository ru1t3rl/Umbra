//! Unit-level tests of the pure `check()` function and the rule table.
//!
//! Covers M0 acceptance tests AT-2, AT-3, AT-4, AT-5 (synthetic graphs) and AT-7 (rule-table
//! self-tests). Requirements: REQ-A-1, REQ-A-2, REQ-A-3, section 4, ASM-005, ASM-006.

use std::collections::{BTreeMap, BTreeSet};

use xtask::deps::{DepEdge, DepKind, RULES, Violation, WorkspaceGraph, check};

fn edge(from: &str, to: &str, kind: DepKind) -> DepEdge {
    DepEdge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        target: None,
        is_workspace: to.starts_with("cgf-"),
    }
}

fn graph(edges: Vec<DepEdge>) -> WorkspaceGraph {
    let mut members: BTreeSet<String> = edges.iter().map(|e| e.from.clone()).collect();
    members.extend(
        edges
            .iter()
            .filter(|e| e.is_workspace)
            .map(|e| e.to.clone()),
    );
    WorkspaceGraph { members, edges }
}

fn forbidden(from: &str, to: &str, kind: DepKind, requirement: &'static str) -> Violation {
    Violation::ForbiddenWorkspaceDep {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        requirement,
    }
}

fn external(from: &str, to: &str, kind: DepKind) -> Violation {
    Violation::ExternalDepNotAllowed {
        from: from.to_string(),
        to: to.to_string(),
        kind,
    }
}

// ---------------------------------------------------------------- AT-2

/// REQ-A-1: `cgf-core` depends on no workspace crate.
#[test]
fn given_core_depends_on_kernel_when_checked_then_req_a1_violation() {
    let g = graph(vec![edge("cgf-core", "cgf-kernel", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "cgf-core",
        "cgf-kernel",
        DepKind::Normal,
        "REQ-A-1"
    )));
}

/// REQ-A-1: a clean `cgf-core` (external deps only) produces no violation.
#[test]
fn given_core_with_only_external_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("cgf-core", "thiserror", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-3

/// REQ-A-2: the kernel must not depend on the server.
#[test]
fn given_kernel_depends_on_server_when_checked_then_req_a2_violation() {
    let g = graph(vec![edge("cgf-kernel", "cgf-server", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "cgf-kernel",
        "cgf-server",
        DepKind::Normal,
        "REQ-A-2"
    )));
}

/// REQ-A-2: the kernel must not depend on the DIS adapter.
#[test]
fn given_kernel_depends_on_net_dis_when_checked_then_req_a2_violation() {
    let g = graph(vec![edge("cgf-kernel", "cgf-net-dis", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "cgf-kernel",
        "cgf-net-dis",
        DepKind::Normal,
        "REQ-A-2"
    )));
}

/// REQ-A-2: an allowed edge (kernel -> core) is not reported.
#[test]
fn given_kernel_depends_on_core_when_checked_then_no_violation() {
    let g = graph(vec![edge("cgf-kernel", "cgf-core", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-4

/// REQ-A-3: the SDK depends only on `cgf-plugin-api`.
#[test]
fn given_sdk_depends_on_core_when_checked_then_req_a3_violation() {
    let g = graph(vec![edge("cgf-sdk", "cgf-core", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "cgf-sdk",
        "cgf-core",
        DepKind::Normal,
        "REQ-A-3"
    )));
}

/// REQ-A-3: the SDK may depend on `cgf-plugin-api`.
#[test]
fn given_sdk_depends_on_plugin_api_when_checked_then_no_violation() {
    let g = graph(vec![edge("cgf-sdk", "cgf-plugin-api", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

/// Section 4 "nothing": `cgf-plugin-api` has no external normal dependency (ASM-009).
#[test]
fn given_plugin_api_has_external_dep_when_checked_then_violation() {
    let g = graph(vec![edge("cgf-plugin-api", "libc", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&external("cgf-plugin-api", "libc", DepKind::Normal)));
}

/// ASM-009: build dependencies count too for `cgf-plugin-api`.
#[test]
fn given_plugin_api_has_external_build_dep_when_checked_then_violation() {
    let g = graph(vec![edge("cgf-plugin-api", "cbindgen", DepKind::Build)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&external("cgf-plugin-api", "cbindgen", DepKind::Build)));
}

/// ASM-009 / plan: only normal and build externals are forbidden; dev externals are allowed.
#[test]
fn given_plugin_api_has_external_dev_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("cgf-plugin-api", "proptest", DepKind::Dev)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-5

/// ASM-005: dev dependencies are checked.
#[test]
fn given_forbidden_dev_dep_when_checked_then_violation_with_dev_kind() {
    let g = graph(vec![edge("cgf-core", "cgf-kernel", DepKind::Dev)]);

    assert!(check(&g, &RULES).contains(&forbidden(
        "cgf-core",
        "cgf-kernel",
        DepKind::Dev,
        "REQ-A-1"
    )));
}

/// ASM-005: build dependencies are checked.
#[test]
fn given_forbidden_build_dep_when_checked_then_violation_with_build_kind() {
    let g = graph(vec![edge("cgf-core", "cgf-kernel", DepKind::Build)]);

    assert!(check(&g, &RULES).contains(&forbidden(
        "cgf-core",
        "cgf-kernel",
        DepKind::Build,
        "REQ-A-1"
    )));
}

/// ASM-005: target-specific dependencies (`[target.'cfg(windows)'.dependencies]`) are checked.
#[test]
fn given_forbidden_target_specific_dep_when_checked_then_violation() {
    let mut e = edge("cgf-sdk", "cgf-core", DepKind::Normal);
    e.target = Some("cfg(windows)".to_string());

    let violations = check(&graph(vec![e]), &RULES);

    assert!(violations.contains(&forbidden(
        "cgf-sdk",
        "cgf-core",
        DepKind::Normal,
        "REQ-A-3"
    )));
}

/// ASM-005/006: a renamed dependency is matched on its real package name. The loader resolves
/// the rename, so `DepEdge::to` is already `cgf-kernel`; `check` must judge on `to`.
#[test]
fn given_renamed_dep_resolved_to_kernel_when_checked_then_violation_names_real_package() {
    let g = graph(vec![edge("cgf-motion", "cgf-kernel", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(
        violations
            .iter()
            .any(|v| matches!(v, Violation::ForbiddenWorkspaceDep { from, to, .. } if from == "cgf-motion" && to == "cgf-kernel"))
    );
}

/// ASM-005: a workspace crate that is missing from the rule table is a violation.
#[test]
fn given_unknown_workspace_crate_when_checked_then_unknown_crate_violation() {
    let g = graph(vec![edge("cgf-mystery", "cgf-core", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&Violation::UnknownCrate {
        name: "cgf-mystery".to_string()
    }));
}

/// ASM-005: an unknown member with no edges at all is still reported (membership, not edges).
#[test]
fn given_unknown_member_without_edges_when_checked_then_unknown_crate_violation() {
    let g = WorkspaceGraph {
        members: BTreeSet::from(["cgf-mystery".to_string()]),
        edges: Vec::new(),
    };

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::UnknownCrate {
            name: "cgf-mystery".to_string()
        }]
    );
}

/// ASM-006: external (non-member) dependencies are fine for crates that allow them.
#[test]
fn given_core_with_external_dev_build_and_normal_deps_when_checked_then_no_violation() {
    let g = graph(vec![
        edge("cgf-core", "thiserror", DepKind::Normal),
        edge("cgf-core", "proptest", DepKind::Dev),
        edge("cgf-core", "cc", DepKind::Build),
    ]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-7

fn table() -> BTreeMap<&'static str, &'static [&'static str]> {
    RULES
        .crates
        .iter()
        .map(|c| (c.name, c.allowed_workspace_deps))
        .collect()
}

fn closure(start: &str) -> BTreeSet<&'static str> {
    let t = table();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = t[start].to_vec();
    while let Some(n) = stack.pop() {
        if seen.insert(n) {
            stack.extend(t[n].iter().copied());
        }
    }
    seen
}

/// Plan table: every section 4 crate has exactly the allowed workspace deps from the plan.
#[test]
fn rules_table_matches_plan_table() {
    let expected: &[(&str, &[&str])] = &[
        ("cgf-core", &[]),
        ("cgf-kernel", &["cgf-core"]),
        ("cgf-motion", &["cgf-core"]),
        ("cgf-behavior", &["cgf-core"]),
        ("cgf-templates", &["cgf-core"]),
        ("cgf-net", &["cgf-core"]),
        ("cgf-plugin-api", &[]),
        ("cgf-plugin-host", &["cgf-core", "cgf-plugin-api"]),
        ("cgf-sdk", &["cgf-plugin-api"]),
        ("cgf-net-dis", &["cgf-sdk"]),
        ("cgf-net-hla", &["cgf-sdk"]),
        (
            "cgf-engine",
            &[
                "cgf-behavior",
                "cgf-core",
                "cgf-kernel",
                "cgf-motion",
                "cgf-net",
                "cgf-plugin-api",
                "cgf-plugin-host",
                "cgf-templates",
            ],
        ),
        ("cgf-api-types", &[]),
        ("cgf-server", &["cgf-api-types", "cgf-engine"]),
        ("cgf-cli", &["cgf-engine", "cgf-server"]),
        ("xtask", &[]),
    ];
    let actual: BTreeMap<&str, BTreeSet<&str>> = table()
        .into_iter()
        .map(|(k, v)| (k, v.iter().copied().collect()))
        .collect();
    let expected: BTreeMap<&str, BTreeSet<&str>> = expected
        .iter()
        .map(|(k, v)| (*k, v.iter().copied().collect()))
        .collect();

    assert_eq!(actual, expected);
}

/// REQ-A-2/3: only `cgf-plugin-api` forbids external dependencies; the requirement labels are set.
#[test]
fn rules_table_only_plugin_api_forbids_external_deps() {
    let forbidding: Vec<&str> = RULES
        .crates
        .iter()
        .filter(|c| !c.allow_external_deps)
        .map(|c| c.name)
        .collect();

    assert_eq!(forbidding, vec!["cgf-plugin-api"]);
}

/// AT-7: no cycles in the allowed graph.
#[test]
fn rules_table_is_acyclic_and_closed() {
    let t = table();
    assert!(!t.is_empty(), "rule table must not be empty");

    // closed: every allowed name is itself a key
    for (name, allowed) in &t {
        for dep in *allowed {
            assert!(t.contains_key(dep), "{name} allows unknown crate {dep}");
        }
    }
    // acyclic: no crate reaches itself
    for name in t.keys() {
        assert!(
            !closure(name).contains(name),
            "{name} is in its own closure"
        );
    }
}

/// REQ-A-2: the kernel's closure excludes the server, network adapters and API types.
#[test]
fn kernel_closure_excludes_server_net_and_api_types() {
    let c = closure("cgf-kernel");

    assert!(!c.contains("cgf-server"));
    assert!(!c.contains("cgf-api-types"));
    assert!(!c.iter().any(|n| n.starts_with("cgf-net")));
}

/// REQ-A-3: the SDK's closure is exactly `{cgf-plugin-api}`.
#[test]
fn sdk_closure_is_plugin_api_only() {
    assert_eq!(closure("cgf-sdk"), BTreeSet::from(["cgf-plugin-api"]));
}

/// REQ-A-1: the core's closure is empty.
#[test]
fn core_closure_is_empty() {
    assert!(closure("cgf-core").is_empty());
}

/// Determinism of the tool: output is sorted and identical across runs, whatever the edge order.
#[test]
fn check_output_is_sorted_and_stable() {
    let edges = || {
        vec![
            edge("cgf-sdk", "cgf-core", DepKind::Normal),
            edge("cgf-plugin-api", "libc", DepKind::Build),
            edge("cgf-kernel", "cgf-server", DepKind::Dev),
            edge("cgf-core", "cgf-kernel", DepKind::Normal),
            edge("cgf-mystery", "cgf-core", DepKind::Normal),
            edge("cgf-kernel", "cgf-net-dis", DepKind::Normal),
        ]
    };
    let mut reversed = edges();
    reversed.reverse();

    let first = check(&graph(edges()), &RULES);
    let second = check(&graph(edges()), &RULES);
    let from_reversed = check(&graph(reversed), &RULES);

    assert!(
        first.len() >= 6,
        "expected one violation per bad edge/crate, got {first:?}"
    );
    assert!(
        first.windows(2).all(|w| w[0] <= w[1]),
        "not sorted: {first:?}"
    );
    assert_eq!(first, second);
    assert_eq!(first, from_reversed);
}
