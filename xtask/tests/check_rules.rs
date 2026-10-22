//! Unit-level tests of the pure `check()` function and the rule table.
//!
//! Covers M0 acceptance tests AT-2, AT-3, AT-4, AT-5 (synthetic graphs) and AT-7 (rule-table
//! self-tests). Requirements: REQ-A-1, REQ-A-2, REQ-A-3, section 4, ASM-005, ASM-006.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use xtask::deps::{
    CrateRule, DepEdge, DepKind, RULES, ResolvedDep, ResolvedGraph, ResolvedPackage, RuleSet,
    Violation, WorkspaceGraph, check, check_resolved,
};

fn edge(from: &str, to: &str, kind: DepKind) -> DepEdge {
    DepEdge {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        target: None,
        is_workspace: to.starts_with("umbra-"),
        is_path: false,
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

/// REQ-A-1: `umbra-core` depends on no workspace crate.
#[test]
fn given_core_depends_on_kernel_when_checked_then_req_a1_violation() {
    let g = graph(vec![edge("umbra-core", "umbra-kernel", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "umbra-core",
        "umbra-kernel",
        DepKind::Normal,
        "REQ-A-1"
    )));
}

/// REQ-A-1: a clean `umbra-core` (external deps only) produces no violation.
#[test]
fn given_core_with_only_external_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("umbra-core", "thiserror", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-3

/// REQ-A-2: the kernel must not depend on the server.
#[test]
fn given_kernel_depends_on_server_when_checked_then_req_a2_violation() {
    let g = graph(vec![edge("umbra-kernel", "umbra-server", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "umbra-kernel",
        "umbra-server",
        DepKind::Normal,
        "REQ-A-2"
    )));
}

/// REQ-A-2: the kernel must not depend on the DIS adapter.
#[test]
fn given_kernel_depends_on_net_dis_when_checked_then_req_a2_violation() {
    let g = graph(vec![edge("umbra-kernel", "umbra-net-dis", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "umbra-kernel",
        "umbra-net-dis",
        DepKind::Normal,
        "REQ-A-2"
    )));
}

/// REQ-A-2: an allowed edge (kernel -> core) is not reported.
#[test]
fn given_kernel_depends_on_core_when_checked_then_no_violation() {
    let g = graph(vec![edge("umbra-kernel", "umbra-core", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-4

/// REQ-A-3: the SDK depends only on `umbra-plugin-api`.
#[test]
fn given_sdk_depends_on_core_when_checked_then_req_a3_violation() {
    let g = graph(vec![edge("umbra-sdk", "umbra-core", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&forbidden(
        "umbra-sdk",
        "umbra-core",
        DepKind::Normal,
        "REQ-A-3"
    )));
}

/// REQ-A-3: the SDK may depend on `umbra-plugin-api`.
#[test]
fn given_sdk_depends_on_plugin_api_when_checked_then_no_violation() {
    let g = graph(vec![edge("umbra-sdk", "umbra-plugin-api", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

/// Section 4 "nothing": `umbra-plugin-api` has no external normal dependency (ASM-009).
#[test]
fn given_plugin_api_has_external_dep_when_checked_then_violation() {
    let g = graph(vec![edge("umbra-plugin-api", "libc", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&external("umbra-plugin-api", "libc", DepKind::Normal)));
}

/// ASM-009: build dependencies count too for `umbra-plugin-api`.
#[test]
fn given_plugin_api_has_external_build_dep_when_checked_then_violation() {
    let g = graph(vec![edge("umbra-plugin-api", "cbindgen", DepKind::Build)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&external("umbra-plugin-api", "cbindgen", DepKind::Build)));
}

/// ASM-009 / plan: only normal and build externals are forbidden; dev externals are allowed.
#[test]
fn given_plugin_api_has_external_dev_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("umbra-plugin-api", "proptest", DepKind::Dev)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- AT-5

/// ASM-005: dev dependencies are checked.
#[test]
fn given_forbidden_dev_dep_when_checked_then_violation_with_dev_kind() {
    let g = graph(vec![edge("umbra-core", "umbra-kernel", DepKind::Dev)]);

    assert!(check(&g, &RULES).contains(&forbidden(
        "umbra-core",
        "umbra-kernel",
        DepKind::Dev,
        "REQ-A-1"
    )));
}

/// ASM-005: build dependencies are checked.
#[test]
fn given_forbidden_build_dep_when_checked_then_violation_with_build_kind() {
    let g = graph(vec![edge("umbra-core", "umbra-kernel", DepKind::Build)]);

    assert!(check(&g, &RULES).contains(&forbidden(
        "umbra-core",
        "umbra-kernel",
        DepKind::Build,
        "REQ-A-1"
    )));
}

/// ASM-005: target-specific dependencies (`[target.'cfg(windows)'.dependencies]`) are checked.
#[test]
fn given_forbidden_target_specific_dep_when_checked_then_violation() {
    let mut e = edge("umbra-sdk", "umbra-core", DepKind::Normal);
    e.target = Some("cfg(windows)".to_string());

    let violations = check(&graph(vec![e]), &RULES);

    assert!(violations.contains(&forbidden(
        "umbra-sdk",
        "umbra-core",
        DepKind::Normal,
        "REQ-A-3"
    )));
}

/// ASM-005/006: a renamed dependency is matched on its real package name. The loader resolves
/// the rename, so `DepEdge::to` is already `umbra-kernel`; `check` must judge on `to`.
#[test]
fn given_renamed_dep_resolved_to_kernel_when_checked_then_violation_names_real_package() {
    let g = graph(vec![edge("umbra-motion", "umbra-kernel", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(
        violations
            .iter()
            .any(|v| matches!(v, Violation::ForbiddenWorkspaceDep { from, to, .. } if from == "umbra-motion" && to == "umbra-kernel"))
    );
}

/// ASM-005: a workspace crate that is missing from the rule table is a violation.
#[test]
fn given_unknown_workspace_crate_when_checked_then_unknown_crate_violation() {
    let g = graph(vec![edge("umbra-mystery", "umbra-core", DepKind::Normal)]);

    let violations = check(&g, &RULES);

    assert!(violations.contains(&Violation::UnknownCrate {
        name: "umbra-mystery".to_string()
    }));
}

/// ASM-005: an unknown member with no edges at all is still reported (membership, not edges).
#[test]
fn given_unknown_member_without_edges_when_checked_then_unknown_crate_violation() {
    let g = WorkspaceGraph {
        members: BTreeSet::from(["umbra-mystery".to_string()]),
        edges: Vec::new(),
    };

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::UnknownCrate {
            name: "umbra-mystery".to_string()
        }]
    );
}

/// ASM-006: external (non-member) dependencies are fine for crates that allow them.
#[test]
fn given_core_with_external_dev_build_and_normal_deps_when_checked_then_no_violation() {
    let g = graph(vec![
        edge("umbra-core", "thiserror", DepKind::Normal),
        edge("umbra-core", "proptest", DepKind::Dev),
        edge("umbra-core", "cc", DepKind::Build),
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
        ("umbra-core", &[]),
        ("umbra-kernel", &["umbra-core"]),
        ("umbra-motion", &["umbra-core"]),
        ("umbra-behavior", &["umbra-core"]),
        ("umbra-templates", &["umbra-core"]),
        ("umbra-net", &["umbra-core"]),
        // M7 WI-1 (ADR 0053, ASM-635): the recording format depends on core only.
        ("umbra-record", &["umbra-core"]),
        ("umbra-plugin-api", &[]),
        ("umbra-plugin-host", &["umbra-core", "umbra-plugin-api"]),
        ("umbra-sdk", &["umbra-plugin-api"]),
        ("umbra-net-dis", &["umbra-sdk"]),
        ("umbra-net-hla", &["umbra-sdk"]),
        (
            "umbra-engine",
            &[
                "umbra-behavior",
                "umbra-core",
                "umbra-kernel",
                "umbra-motion",
                "umbra-net",
                "umbra-plugin-api",
                "umbra-plugin-host",
                "umbra-record",
                "umbra-templates",
            ],
        ),
        ("umbra-api-types", &[]),
        ("umbra-server", &["umbra-api-types", "umbra-engine"]),
        ("umbra-cli", &["umbra-engine", "umbra-server"]),
        ("xtask", &[]),
        // M4 WI-1 (REQ-SDK-3, ADR 0023 item 4): plugin crates depend on the SDK only.
        ("example-wander", &["umbra-sdk"]),
        ("umbra-test-plugin", &["umbra-sdk"]),
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

/// REQ-A-2/3: only `umbra-plugin-api` forbids external dependencies; the requirement labels are set.
#[test]
fn rules_table_only_plugin_api_forbids_external_deps() {
    let forbidding: Vec<&str> = RULES
        .crates
        .iter()
        .filter(|c| !c.allow_external_deps)
        .map(|c| c.name)
        .collect();

    assert_eq!(forbidding, vec!["umbra-plugin-api"]);
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
    let c = closure("umbra-kernel");

    assert!(!c.contains("umbra-server"));
    assert!(!c.contains("umbra-api-types"));
    assert!(!c.iter().any(|n| n.starts_with("umbra-net")));
}

/// REQ-A-3: the SDK's closure is exactly `{umbra-plugin-api}`.
#[test]
fn sdk_closure_is_plugin_api_only() {
    assert_eq!(closure("umbra-sdk"), BTreeSet::from(["umbra-plugin-api"]));
}

/// REQ-A-1: the core's closure is empty.
#[test]
fn core_closure_is_empty() {
    assert!(closure("umbra-core").is_empty());
}

/// Determinism of the tool: output is sorted and identical across runs, whatever the edge order.
#[test]
fn check_output_is_sorted_and_stable() {
    let edges = || {
        vec![
            edge("umbra-sdk", "umbra-core", DepKind::Normal),
            edge("umbra-plugin-api", "libc", DepKind::Build),
            edge("umbra-kernel", "umbra-server", DepKind::Dev),
            edge("umbra-core", "umbra-kernel", DepKind::Normal),
            edge("umbra-mystery", "umbra-core", DepKind::Normal),
            edge("umbra-kernel", "umbra-net-dis", DepKind::Normal),
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

// ---------------------------------------------------------------- review M1 / m3

fn path_edge(from: &str, to: &str, kind: DepKind) -> DepEdge {
    DepEdge {
        is_path: true,
        ..edge(from, to, kind)
    }
}

/// M1 (ASM-006): a path dependency outside the workspace could launder forbidden edges
/// (`umbra-sdk -> shim -> umbra-kernel`), so it is a violation unless allow-listed.
#[test]
fn given_non_member_path_dep_when_checked_then_violation() {
    let g = graph(vec![path_edge("umbra-sdk", "shim", DepKind::Normal)]);

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::NonMemberPathDep {
            from: "umbra-sdk".to_string(),
            to: "shim".to_string(),
            kind: DepKind::Normal
        }]
    );
}

/// M1: dev and build path dependencies are checked too.
#[test]
fn given_non_member_path_dev_and_build_deps_when_checked_then_violations() {
    let g = graph(vec![
        path_edge("umbra-core", "shim", DepKind::Dev),
        path_edge("umbra-core", "shim", DepKind::Build),
    ]);

    assert_eq!(check(&g, &RULES).len(), 2);
}

/// Finding 3 (ASM-006): there is no production allow-list for path dependencies, so a fixture
/// name such as `fake_external` is no back door.
#[test]
fn given_fake_external_path_dep_when_checked_then_violation() {
    let g = graph(vec![path_edge(
        "umbra-core",
        "fake_external",
        DepKind::Normal,
    )]);

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::NonMemberPathDep {
            from: "umbra-core".to_string(),
            to: "fake_external".to_string(),
            kind: DepKind::Normal
        }]
    );
}

/// M1: registry dependencies (not path) stay fine.
#[test]
fn given_registry_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("umbra-core", "serde_json", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

static CUSTOM: RuleSet = RuleSet {
    crates: &[CrateRule {
        name: "umbra-core",
        allowed_workspace_deps: &["umbra-odd"],
        allow_external_deps: false,
        requirement: "REQ-A-1",
    }],
};

/// m3: rendering uses the injected rule set, not the global table.
#[test]
fn given_custom_rules_when_rendered_then_allowed_list_comes_from_them() {
    let v = forbidden("umbra-core", "umbra-kernel", DepKind::Normal, "REQ-A-1");

    let text = v.render(&CUSTOM);

    assert_eq!(
        text,
        "REQ-A-1: umbra-core -> umbra-kernel (normal) not allowed; umbra-core may depend on: umbra-odd"
    );
}

/// n1: the external-dependency message names the rule and ASM-009.
#[test]
fn given_external_violation_when_rendered_then_message_is_precise() {
    let v = external("umbra-core", "libc", DepKind::Build);

    let text = v.render(&CUSTOM);

    assert_eq!(
        text,
        "REQ-A-1: umbra-core -> libc (build) not allowed; umbra-core may have no normal/build external dependencies (ASM-009)"
    );
}

// ---------------------------------------------------------------- review of 0a25b26: resolved pass

/// Synthetic resolved graph. Nodes are `(id, name, is_member)`; edges are `(from_id, to_id, kind)`.
fn resolved(nodes: &[(&str, &str, bool)], edges: &[(&str, &str, DepKind)]) -> ResolvedGraph {
    let mut g = ResolvedGraph::default();
    for (id, name, member) in nodes {
        g.packages.insert(
            (*id).to_string(),
            ResolvedPackage {
                name: (*name).to_string(),
                version: "0.1.0".to_string(),
                // Members are path packages; synthetic non-members come from a registry.
                source: (!*member).then(|| "registry+https://example.invalid/index".to_string()),
            },
        );
        if *member {
            g.members.insert((*id).to_string());
        }
        g.deps.insert((*id).to_string(), Vec::new());
    }
    for (from, to, kind) in edges {
        let deps = g.deps.entry((*from).to_string()).or_default();
        if let Some(d) = deps.iter_mut().find(|d| d.id == *to) {
            d.kinds.insert(*kind);
        } else {
            deps.push(ResolvedDep {
                id: (*to).to_string(),
                kinds: BTreeSet::from([*kind]),
            });
        }
        deps.sort_by(|a, b| a.id.cmp(&b.id));
    }
    g
}

fn indirect(from: &str, via: &[&str], to: &str, requirement: &'static str) -> Violation {
    Violation::IndirectWorkspaceDep {
        from: from.to_string(),
        via: via.iter().map(|s| (*s).to_string()).collect(),
        to: to.to_string(),
        requirement,
    }
}

/// Finding 1 (REQ-A-3): `umbra-sdk -> shim -> umbra-kernel` through a non-member is reported.
#[test]
fn given_member_reached_through_non_member_when_resolved_checked_then_indirect_violation() {
    let g = resolved(
        &[
            ("sdk", "umbra-sdk", true),
            ("k", "umbra-kernel", true),
            ("s", "shim", false),
        ],
        &[("sdk", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![indirect("umbra-sdk", &["shim"], "umbra-kernel", "REQ-A-3")]
    );
}

/// The full non-member path is reported, shortest first.
#[test]
fn given_multi_hop_launder_when_resolved_checked_then_full_path_is_reported() {
    let g = resolved(
        &[
            ("core", "umbra-core", true),
            ("k", "umbra-kernel", true),
            ("a", "a", false),
            ("b", "b", false),
        ],
        &[
            ("core", "a", DepKind::Dev),
            ("a", "b", DepKind::Normal),
            ("b", "k", DepKind::Build),
        ],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![indirect(
            "umbra-core",
            &["a", "b"],
            "umbra-kernel",
            "REQ-A-1"
        )]
    );
}

/// A member reached through a non-member is fine when it is in the allowed closure
/// (`umbra-cli` may reach `umbra-kernel` through `umbra-engine`).
#[test]
fn given_member_in_allowed_closure_when_resolved_checked_then_no_violation() {
    let g = resolved(
        &[
            ("cli", "umbra-cli", true),
            ("k", "umbra-kernel", true),
            ("s", "shim", false),
        ],
        &[("cli", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// Members are not expanded: `umbra-plugin-api -> umbra-kernel` is reported for `umbra-plugin-api`
/// (direct edge), not again as an indirect edge of `umbra-sdk`.
#[test]
fn given_forbidden_edge_behind_a_member_when_resolved_checked_then_reported_once_at_its_source() {
    let g = resolved(
        &[
            ("sdk", "umbra-sdk", true),
            ("api", "umbra-plugin-api", true),
            ("k", "umbra-kernel", true),
        ],
        &[
            ("sdk", "api", DepKind::Normal),
            ("api", "k", DepKind::Normal),
        ],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![forbidden(
            "umbra-plugin-api",
            "umbra-kernel",
            DepKind::Normal,
            "section 4"
        )]
    );
}

/// Finding 1: a direct edge to a member by package id is judged against the direct allow-list,
/// with every kind it has (this catches a registry dependency patched to a member).
#[test]
fn given_direct_member_edge_by_id_when_resolved_checked_then_forbidden_for_each_kind() {
    let g = resolved(
        &[("dis", "umbra-net-dis", true), ("k", "umbra-kernel", true)],
        &[("dis", "k", DepKind::Normal), ("dis", "k", DepKind::Dev)],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![
            forbidden(
                "umbra-net-dis",
                "umbra-kernel",
                DepKind::Normal,
                "section 4"
            ),
            forbidden("umbra-net-dis", "umbra-kernel", DepKind::Dev, "section 4"),
        ]
    );
}

/// Finding 2: a non-member package with a member's name, or a rule-table name, is an impostor.
#[test]
fn given_non_member_with_reserved_name_when_resolved_checked_then_impostor() {
    let g = resolved(
        &[
            ("sdk", "umbra-sdk", true),
            ("api", "umbra-plugin-api", true),
            ("fake-api", "umbra-plugin-api", false),
            ("fake-kernel", "umbra-kernel", false),
        ],
        &[
            ("sdk", "fake-api", DepKind::Normal),
            ("sdk", "fake-kernel", DepKind::Normal),
        ],
    );

    let v = check_resolved(&g, &RULES);

    let impostors: Vec<&str> = v
        .iter()
        .filter_map(|v| match v {
            Violation::ImpostorPackage { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        impostors,
        ["fake-kernel", "fake-api"],
        "sorted by name: {v:?}"
    );
}

/// A dev-dependency cycle back to the member itself (allowed by cargo) is not a violation.
#[test]
fn given_dev_cycle_back_to_self_when_resolved_checked_then_no_violation() {
    let g = resolved(
        &[("core", "umbra-core", true), ("h", "helper", false)],
        &[("core", "h", DepKind::Dev), ("h", "core", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// Members missing from the rule table are skipped here (`check` reports `UnknownCrate`).
#[test]
fn given_unknown_member_when_resolved_checked_then_no_resolved_violation() {
    let g = resolved(
        &[
            ("m", "umbra-mystery", true),
            ("k", "umbra-kernel", true),
            ("s", "shim", false),
        ],
        &[("m", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// The indirect message names the full path and the allowed closure.
#[test]
fn given_indirect_violation_when_rendered_then_message_names_path_and_closure() {
    let v = indirect("umbra-sdk", &["shim"], "umbra-kernel", "REQ-A-3");

    assert_eq!(
        v.render(&RULES),
        "REQ-A-3: umbra-sdk -> shim -> umbra-kernel (resolved) not allowed; umbra-sdk may reach: umbra-plugin-api"
    );
}

/// `RuleSet::closure` matches the self-test closure for every crate.
#[test]
fn rules_closure_matches_table_closure() {
    for c in RULES.crates {
        assert_eq!(RULES.closure(c.name), closure(c.name), "{}", c.name);
    }
    assert!(RULES.closure("not-a-crate").is_empty());
}

// ---------------------------------------------------------------- re-review of 341eca7

/// M1 (ASM-006, NFR-2): a non-member path package in the resolved graph is reported even when no
/// rule is broken otherwise (it arrived through `[patch]`, so the declared pass saw a registry
/// dependency).
#[test]
fn given_non_member_path_package_when_resolved_checked_then_violation() {
    let mut g = resolved(
        &[("core", "umbra-core", true), ("s", "shim", false)],
        &[("core", "s", DepKind::Normal)],
    );
    if let Some(p) = g.packages.get_mut("s") {
        p.source = None;
    }

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![Violation::NonMemberPathPackage {
            name: "shim".to_string(),
            version: "0.1.0".to_string(),
            id: "s".to_string()
        }]
    );
}

/// M1: registry and git packages (with a source) are not path packages.
#[test]
fn given_non_member_with_source_when_resolved_checked_then_no_violation() {
    let g = resolved(
        &[("core", "umbra-core", true), ("t", "thiserror", false)],
        &[("core", "t", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// m1 / ASM-006: a declared non-path dependency named like a workspace or rule-table crate (for
/// example `umbra-kernel = { git = ... }`) is reported by the declared pass.
#[test]
fn given_non_path_dep_with_reserved_name_when_checked_then_impostor_dep() {
    let mut e = edge("umbra-sdk", "umbra-kernel", DepKind::Normal);
    e.is_workspace = false;

    assert_eq!(
        check(&graph(vec![e]), &RULES),
        vec![Violation::ImpostorDep {
            from: "umbra-sdk".to_string(),
            to: "umbra-kernel".to_string(),
            kind: DepKind::Normal
        }]
    );
}

// ---------------------------------------------------------------- M5 WI-1 (REQ-X-7, ASM-007, ASM-008)

/// The edges the M5 plan forbids (and two more for ASM-008) are each reported with the real rule table, and the allowed
/// server edges are not. (The same edges were seeded into the real workspace by hand in WI-1:
/// `check-deps` exited 1 for the first and third; cargo rejects the second as a cycle.)
#[test]
fn given_m5_forbidden_edges_when_checked_then_each_is_a_violation() {
    for (from, to) in [
        ("umbra-server", "umbra-kernel"),
        ("umbra-server", "umbra-core"),
        ("umbra-engine", "umbra-server"),
        ("umbra-api-types", "umbra-core"),
        ("umbra-api-types", "umbra-engine"),
    ] {
        let g = graph(vec![edge(from, to, DepKind::Normal)]);
        let v = check(&g, &RULES);
        assert!(
            v.iter().any(|x| matches!(
                x,
                Violation::ForbiddenWorkspaceDep { from: f, to: t, .. } if f == from && t == to
            )),
            "{from} -> {to} not reported: {v:?}"
        );
    }
}

/// The allowed edges of the new crates produce no violation.
#[test]
fn given_m5_allowed_edges_when_checked_then_no_violation() {
    let g = graph(vec![
        edge("umbra-server", "umbra-engine", DepKind::Normal),
        edge("umbra-server", "umbra-api-types", DepKind::Normal),
        edge("umbra-cli", "umbra-server", DepKind::Normal),
        edge("umbra-cli", "umbra-engine", DepKind::Normal),
    ]);
    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

// ---------------------------------------------------------------- M6 WI-1 (ADR 0030, ASM-226, ASM-255)

/// Protocol-neutrality rule: the engine-side crates must not depend on protocol crates, by name
/// pattern, whatever the dependency is called. Fixtures: `tests/fixtures/protocol_*`.
mod protocol_neutrality {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use xtask::deps::{
        DepEdge, DepKind, PROTOCOL_NEUTRAL, RULES, Violation, Workspace, WorkspaceGraph, check,
        check_workspace, is_protocol_crate, protocol_scope,
    };

    fn edge(from: &str, to: &str, kind: DepKind) -> DepEdge {
        DepEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            target: None,
            is_workspace: to.starts_with("umbra-"),
            is_path: false,
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

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
            .join("Cargo.toml")
    }

    #[expect(
        clippy::expect_used,
        reason = "test helper: a spawn failure is a test error"
    )]
    fn run_check_deps(manifest: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("check-deps")
            .arg("--manifest-path")
            .arg(manifest)
            .output()
            .expect("spawn xtask binary")
    }

    #[expect(
        clippy::expect_used,
        reason = "test helper: a fixture that fails to load is a test error"
    )]
    fn violations(name: &str) -> Vec<Violation> {
        let mut v = check_workspace(
            &Workspace::load(Some(&fixture(name))).expect("load"),
            &RULES,
        );
        for x in &mut v {
            if let Violation::NonMemberPathPackage { id, .. } = x {
                *id = "<id>".to_string();
            }
        }
        v.sort();
        v
    }

    fn forbidden(from: &str, to: &str, requirement: &'static str) -> Violation {
        Violation::ForbiddenWorkspaceDep {
            from: from.into(),
            to: to.into(),
            kind: DepKind::Normal,
            requirement,
        }
    }

    /// The name patterns match protocol crates and not look-alikes (ASM-226: anchored).
    #[test]
    fn given_crate_names_when_classified_then_only_protocol_crates_match() {
        for yes in [
            "dis-rs",
            "dis_rs",
            "dis",
            "DIS",
            "open-dis-rust",
            "opendis",
            "umbra-net-dis",
            "my_dis",
            "rti",
            "rti-sys",
            "portico",
            "portico-rti",
            "hla",
            "hla-fom",
            "umbra-net-hla",
            "dis-codec",
            "foo-dis-bar",
            "Dis_Codec",
        ] {
            assert!(is_protocol_crate(yes), "{yes} should match");
        }
        for no in [
            "redis",
            "discord",
            "radis",
            "disk-utils",
            "dispatch",
            "display-info",
            "distributed",
            "disk",
            "umbra-net",
            "umbra-core",
            "serde",
            "pedis",
            "critic",
            "hlas",
            "rtic",
            "umbra-engine",
        ] {
            assert!(!is_protocol_crate(no), "{no} should not match");
        }
    }

    /// The neutral crates are exactly the four of ASM-226.
    #[test]
    fn neutral_crates_are_core_kernel_net_engine() {
        assert_eq!(
            PROTOCOL_NEUTRAL,
            &["umbra-core", "umbra-engine", "umbra-kernel", "umbra-net"]
        );
    }

    /// An external protocol crate on a neutral crate is reported for every edge kind; the same
    /// dependency on an adapter crate (not neutral) is allowed; a look-alike is allowed.
    #[test]
    fn given_external_protocol_dep_when_checked_then_only_neutral_crates_are_flagged() {
        for kind in [DepKind::Normal, DepKind::Build, DepKind::Dev] {
            let v = check(&graph(vec![edge("umbra-net", "dis-rs", kind)]), &RULES);
            assert_eq!(
                v,
                vec![Violation::ProtocolDep {
                    from: "umbra-net".into(),
                    to: "dis-rs".into(),
                    kind
                }]
            );
        }
        let ok = graph(vec![
            edge("umbra-net-dis", "dis-rs", DepKind::Normal),
            edge("umbra-net", "redis", DepKind::Normal),
        ]);
        assert_eq!(check(&ok, &RULES), Vec::<Violation>::new());
    }

    /// The message names the rule, the edge and the neutral crate.
    #[test]
    fn given_protocol_violation_when_rendered_then_message_is_precise() {
        let v = Violation::ProtocolDep {
            from: "umbra-net".into(),
            to: "dis-rs".into(),
            kind: DepKind::Dev,
        };
        assert_eq!(
            v.render(&RULES),
            "ADR 0030: umbra-net -> dis-rs (dev) not allowed; umbra-net is protocol-neutral and must not depend on protocol crates (name pattern)"
        );
        let i = Violation::IndirectProtocolDep {
            from: "umbra-engine".into(),
            via: vec!["shim".into()],
            to: "dis-rs".into(),
        };
        assert_eq!(
            i.render(&RULES),
            "ADR 0030: umbra-engine -> shim -> dis-rs (resolved) not allowed; umbra-engine is protocol-neutral and must not reach protocol crates (name pattern)"
        );
    }

    /// The five declared violations of the plan, in one fixture, exactly (sorted, one each).
    #[test]
    fn given_protocol_fixture_when_checked_then_each_seeded_violation_is_found() {
        let mut expected = vec![
            forbidden("umbra-kernel", "umbra-net", "REQ-A-2"),
            forbidden("umbra-net", "umbra-net-dis", "section 4"),
            forbidden("umbra-engine", "umbra-net-dis", "section 4"),
            forbidden("umbra-engine", "umbra-net-hla", "section 4"),
            Violation::ProtocolDep {
                from: "umbra-net".into(),
                to: "dis-rs".into(),
                kind: DepKind::Normal,
            },
            Violation::ProtocolDep {
                from: "umbra-templates".into(),
                to: "dis-rs".into(),
                kind: DepKind::Normal,
            },
        ];
        expected.sort();
        assert_eq!(violations("protocol_violating"), expected);
    }

    /// The binary exits 1 and prints one line per violation.
    #[test]
    fn given_protocol_fixture_when_check_deps_runs_then_exit_1_with_one_line_each() {
        let out = run_check_deps(&fixture("protocol_violating"));
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "output:\n{text}");
        for needle in [
            "REQ-A-2: umbra-kernel -> umbra-net (normal) not allowed",
            "section 4: umbra-net -> umbra-net-dis (normal) not allowed",
            "section 4: umbra-engine -> umbra-net-dis (normal) not allowed",
            "section 4: umbra-engine -> umbra-net-hla (normal) not allowed",
            "ADR 0030: umbra-net -> dis-rs (normal) not allowed",
            "ADR 0030: umbra-templates -> dis-rs (normal) not allowed",
        ] {
            assert_eq!(
                text.lines().filter(|l| l.contains(needle)).count(),
                1,
                "{needle}\n{text}"
            );
        }
    }

    /// Laundering: `umbra-engine -> shim -> dis-rs` with `shim` supplied by `[patch]` is found in
    /// the resolved graph (the declared edge `umbra-engine -> shim` looks harmless).
    #[test]
    fn given_dis_rs_laundered_through_patch_when_checked_then_indirect_violation() {
        assert_eq!(
            violations("protocol_launder_patch"),
            vec![
                Violation::NonMemberPathPackage {
                    name: "shim".into(),
                    version: "0.1.0".into(),
                    id: "<id>".into(),
                },
                Violation::IndirectProtocolDep {
                    from: "umbra-engine".into(),
                    via: vec!["shim".into()],
                    to: "dis-rs".into(),
                },
            ]
        );
    }

    #[expect(
        clippy::expect_used,
        reason = "test helper: a workspace that fails to load is a test error"
    )]
    fn load(manifest: &Path) -> Workspace {
        Workspace::load(Some(manifest)).expect("load")
    }

    /// The real workspace has no protocol dependency on a neutral crate.
    #[test]
    fn given_real_workspace_when_checked_then_no_protocol_violation() {
        let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let ws = load(&real);
        let bad: Vec<_> = check_workspace(&ws, &RULES)
            .into_iter()
            .filter(|v| {
                matches!(
                    v,
                    Violation::ProtocolDep { .. } | Violation::IndirectProtocolDep { .. }
                )
            })
            .collect();
        assert_eq!(bad, Vec::<Violation>::new());
    }

    /// A protocol crate that enters through an allowed workspace member (`umbra-engine ->
    /// umbra-templates -> dis-rs`) is reported at the member that declares it: the scope is the
    /// neutral crates plus their allowed workspace closure.
    #[test]
    fn given_protocol_dep_on_an_allowed_member_when_checked_then_reported() {
        let scope = protocol_scope(&RULES);
        for c in [
            "umbra-engine",
            "umbra-templates",
            "umbra-motion",
            "umbra-behavior",
            "umbra-plugin-host",
        ] {
            assert!(scope.contains(c), "{c} should be in scope");
        }
        for c in ["umbra-net-dis", "umbra-server", "umbra-sdk", "umbra-cli"] {
            assert!(!scope.contains(c), "{c} should be out of scope");
        }
        let v = check(
            &graph(vec![edge("umbra-templates", "dis-rs", DepKind::Normal)]),
            &RULES,
        );
        assert_eq!(
            v,
            vec![Violation::ProtocolDep {
                from: "umbra-templates".into(),
                to: "dis-rs".into(),
                kind: DepKind::Normal
            }]
        );
    }

    /// `PROTOCOL_NEUTRAL` and the source scanner's crate list are the same set.
    #[test]
    fn scanner_crate_list_matches_protocol_neutral() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/umbra-net/tests/protocol_neutral.rs");
        let text = std::fs::read_to_string(src).expect("read protocol_neutral.rs");
        let decl = text
            .split("const NEUTRAL_CRATES")
            .nth(1)
            .and_then(|t| t.split(';').next())
            .unwrap_or_default();
        let listed: BTreeSet<&str> = decl.split('"').skip(1).step_by(2).collect();
        let expected: BTreeSet<&str> = PROTOCOL_NEUTRAL.iter().copied().collect();
        assert_eq!(listed, expected);
    }
}
