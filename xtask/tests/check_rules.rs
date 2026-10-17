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
        is_workspace: to.starts_with("cgf-"),
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

// ---------------------------------------------------------------- review M1 / m3

fn path_edge(from: &str, to: &str, kind: DepKind) -> DepEdge {
    DepEdge {
        is_path: true,
        ..edge(from, to, kind)
    }
}

/// M1 (ASM-006): a path dependency outside the workspace could launder forbidden edges
/// (`cgf-sdk -> shim -> cgf-kernel`), so it is a violation unless allow-listed.
#[test]
fn given_non_member_path_dep_when_checked_then_violation() {
    let g = graph(vec![path_edge("cgf-sdk", "shim", DepKind::Normal)]);

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::NonMemberPathDep {
            from: "cgf-sdk".to_string(),
            to: "shim".to_string(),
            kind: DepKind::Normal
        }]
    );
}

/// M1: dev and build path dependencies are checked too.
#[test]
fn given_non_member_path_dev_and_build_deps_when_checked_then_violations() {
    let g = graph(vec![
        path_edge("cgf-core", "shim", DepKind::Dev),
        path_edge("cgf-core", "shim", DepKind::Build),
    ]);

    assert_eq!(check(&g, &RULES).len(), 2);
}

/// Finding 3 (ASM-006): there is no production allow-list for path dependencies, so a fixture
/// name such as `fake_external` is no back door.
#[test]
fn given_fake_external_path_dep_when_checked_then_violation() {
    let g = graph(vec![path_edge(
        "cgf-core",
        "fake_external",
        DepKind::Normal,
    )]);

    assert_eq!(
        check(&g, &RULES),
        vec![Violation::NonMemberPathDep {
            from: "cgf-core".to_string(),
            to: "fake_external".to_string(),
            kind: DepKind::Normal
        }]
    );
}

/// M1: registry dependencies (not path) stay fine.
#[test]
fn given_registry_dep_when_checked_then_no_violation() {
    let g = graph(vec![edge("cgf-core", "serde_json", DepKind::Normal)]);

    assert_eq!(check(&g, &RULES), Vec::<Violation>::new());
}

static CUSTOM: RuleSet = RuleSet {
    crates: &[CrateRule {
        name: "cgf-core",
        allowed_workspace_deps: &["cgf-odd"],
        allow_external_deps: false,
        requirement: "REQ-A-1",
    }],
};

/// m3: rendering uses the injected rule set, not the global table.
#[test]
fn given_custom_rules_when_rendered_then_allowed_list_comes_from_them() {
    let v = forbidden("cgf-core", "cgf-kernel", DepKind::Normal, "REQ-A-1");

    let text = v.render(&CUSTOM);

    assert_eq!(
        text,
        "REQ-A-1: cgf-core -> cgf-kernel (normal) not allowed; cgf-core may depend on: cgf-odd"
    );
}

/// n1: the external-dependency message names the rule and ASM-009.
#[test]
fn given_external_violation_when_rendered_then_message_is_precise() {
    let v = external("cgf-core", "libc", DepKind::Build);

    let text = v.render(&CUSTOM);

    assert_eq!(
        text,
        "REQ-A-1: cgf-core -> libc (build) not allowed; cgf-core may have no normal/build external dependencies (ASM-009)"
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

/// Finding 1 (REQ-A-3): `cgf-sdk -> shim -> cgf-kernel` through a non-member is reported.
#[test]
fn given_member_reached_through_non_member_when_resolved_checked_then_indirect_violation() {
    let g = resolved(
        &[
            ("sdk", "cgf-sdk", true),
            ("k", "cgf-kernel", true),
            ("s", "shim", false),
        ],
        &[("sdk", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![indirect("cgf-sdk", &["shim"], "cgf-kernel", "REQ-A-3")]
    );
}

/// The full non-member path is reported, shortest first.
#[test]
fn given_multi_hop_launder_when_resolved_checked_then_full_path_is_reported() {
    let g = resolved(
        &[
            ("core", "cgf-core", true),
            ("k", "cgf-kernel", true),
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
        vec![indirect("cgf-core", &["a", "b"], "cgf-kernel", "REQ-A-1")]
    );
}

/// A member reached through a non-member is fine when it is in the allowed closure
/// (`cgf-cli` may reach `cgf-kernel` through `cgf-engine`).
#[test]
fn given_member_in_allowed_closure_when_resolved_checked_then_no_violation() {
    let g = resolved(
        &[
            ("cli", "cgf-cli", true),
            ("k", "cgf-kernel", true),
            ("s", "shim", false),
        ],
        &[("cli", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// Members are not expanded: `cgf-plugin-api -> cgf-kernel` is reported for `cgf-plugin-api`
/// (direct edge), not again as an indirect edge of `cgf-sdk`.
#[test]
fn given_forbidden_edge_behind_a_member_when_resolved_checked_then_reported_once_at_its_source() {
    let g = resolved(
        &[
            ("sdk", "cgf-sdk", true),
            ("api", "cgf-plugin-api", true),
            ("k", "cgf-kernel", true),
        ],
        &[
            ("sdk", "api", DepKind::Normal),
            ("api", "k", DepKind::Normal),
        ],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![forbidden(
            "cgf-plugin-api",
            "cgf-kernel",
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
        &[("dis", "cgf-net-dis", true), ("k", "cgf-kernel", true)],
        &[("dis", "k", DepKind::Normal), ("dis", "k", DepKind::Dev)],
    );

    assert_eq!(
        check_resolved(&g, &RULES),
        vec![
            forbidden("cgf-net-dis", "cgf-kernel", DepKind::Normal, "section 4"),
            forbidden("cgf-net-dis", "cgf-kernel", DepKind::Dev, "section 4"),
        ]
    );
}

/// Finding 2: a non-member package with a member's name, or a rule-table name, is an impostor.
#[test]
fn given_non_member_with_reserved_name_when_resolved_checked_then_impostor() {
    let g = resolved(
        &[
            ("sdk", "cgf-sdk", true),
            ("api", "cgf-plugin-api", true),
            ("fake-api", "cgf-plugin-api", false),
            ("fake-kernel", "cgf-kernel", false),
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
        &[("core", "cgf-core", true), ("h", "helper", false)],
        &[("core", "h", DepKind::Dev), ("h", "core", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// Members missing from the rule table are skipped here (`check` reports `UnknownCrate`).
#[test]
fn given_unknown_member_when_resolved_checked_then_no_resolved_violation() {
    let g = resolved(
        &[
            ("m", "cgf-mystery", true),
            ("k", "cgf-kernel", true),
            ("s", "shim", false),
        ],
        &[("m", "s", DepKind::Normal), ("s", "k", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// The indirect message names the full path and the allowed closure.
#[test]
fn given_indirect_violation_when_rendered_then_message_names_path_and_closure() {
    let v = indirect("cgf-sdk", &["shim"], "cgf-kernel", "REQ-A-3");

    assert_eq!(
        v.render(&RULES),
        "REQ-A-3: cgf-sdk -> shim -> cgf-kernel (resolved) not allowed; cgf-sdk may reach: cgf-plugin-api"
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
        &[("core", "cgf-core", true), ("s", "shim", false)],
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
        &[("core", "cgf-core", true), ("t", "thiserror", false)],
        &[("core", "t", DepKind::Normal)],
    );

    assert_eq!(check_resolved(&g, &RULES), Vec::<Violation>::new());
}

/// m1 / ASM-006: a declared non-path dependency named like a workspace or rule-table crate (for
/// example `cgf-kernel = { git = ... }`) is reported by the declared pass.
#[test]
fn given_non_path_dep_with_reserved_name_when_checked_then_impostor_dep() {
    let mut e = edge("cgf-sdk", "cgf-kernel", DepKind::Normal);
    e.is_workspace = false;

    assert_eq!(
        check(&graph(vec![e]), &RULES),
        vec![Violation::ImpostorDep {
            from: "cgf-sdk".to_string(),
            to: "cgf-kernel".to_string(),
            kind: DepKind::Normal
        }]
    );
}
