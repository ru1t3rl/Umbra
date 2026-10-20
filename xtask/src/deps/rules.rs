//! Rule table (ASM-007): the section 4 dependency rules as a type-checked constant.

use std::collections::BTreeSet;

/// Rule for one workspace crate.
#[derive(Debug)]
pub struct CrateRule {
    /// Crate name.
    pub name: &'static str,
    /// Allowed workspace dependencies.
    pub allowed_workspace_deps: &'static [&'static str],
    /// External deps allowed (false only for `umbra-plugin-api`, normal and build).
    pub allow_external_deps: bool,
    /// "REQ-A-1", "REQ-A-2", "REQ-A-3", "REQ-SDK-3" or "section 4".
    pub requirement: &'static str,
}

/// All rules. There is deliberately no allow-list for non-member path dependencies (ASM-006):
/// every such dependency is a violation.
#[derive(Debug)]
pub struct RuleSet {
    /// One entry per crate.
    pub crates: &'static [CrateRule],
}

impl RuleSet {
    /// Finds the rule for a crate by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&CrateRule> {
        self.crates.iter().find(|c| c.name == name)
    }

    /// Transitive closure of the allowed workspace deps of `name` (empty for unknown crates).
    #[must_use]
    pub fn closure(&self, name: &str) -> BTreeSet<&'static str> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<&'static str> = self
            .get(name)
            .map(|r| r.allowed_workspace_deps.to_vec())
            .unwrap_or_default();
        while let Some(n) = stack.pop() {
            if seen.insert(n)
                && let Some(r) = self.get(n)
            {
                stack.extend(r.allowed_workspace_deps);
            }
        }
        seen
    }
}

const fn rule(
    name: &'static str,
    allowed_workspace_deps: &'static [&'static str],
    requirement: &'static str,
) -> CrateRule {
    CrateRule {
        name,
        allowed_workspace_deps,
        allow_external_deps: true,
        requirement,
    }
}

/// The full section 4 table (REQ-A-1..3, ASM-005..009). Crates that do not exist yet are listed
/// so they are covered the day they appear.
pub const RULES: RuleSet = RuleSet {
    crates: &[
        rule("umbra-core", &[], "REQ-A-1"),
        rule("umbra-kernel", &["umbra-core"], "REQ-A-2"),
        rule("umbra-motion", &["umbra-core"], "section 4"),
        rule("umbra-behavior", &["umbra-core"], "section 4"),
        rule("umbra-templates", &["umbra-core"], "section 4"),
        rule("umbra-net", &["umbra-core"], "section 4"),
        CrateRule {
            name: "umbra-plugin-api",
            allowed_workspace_deps: &[],
            allow_external_deps: false,
            requirement: "section 4",
        },
        rule(
            "umbra-plugin-host",
            &["umbra-core", "umbra-plugin-api"],
            "section 4",
        ),
        rule("umbra-sdk", &["umbra-plugin-api"], "REQ-A-3"),
        rule("umbra-net-dis", &["umbra-sdk"], "section 4"),
        rule("umbra-net-hla", &["umbra-sdk"], "section 4"),
        rule(
            "umbra-engine",
            &[
                "umbra-behavior",
                "umbra-core",
                "umbra-kernel",
                "umbra-motion",
                "umbra-net",
                "umbra-plugin-api",
                "umbra-plugin-host",
                "umbra-templates",
            ],
            "section 4",
        ),
        rule("umbra-api-types", &[], "section 4"),
        rule(
            "umbra-server",
            &["umbra-api-types", "umbra-engine"],
            "section 4",
        ),
        rule("umbra-cli", &["umbra-engine", "umbra-server"], "section 4"),
        rule("xtask", &[], "section 4"),
        // Plugin crates (M4, ADR 0023 item 4): built with the SDK only (REQ-SDK-3). They are not
        // FFI crates, so they keep `#![forbid(unsafe_code)]` (ASM-024, ASM-116).
        rule("example-wander", &["umbra-sdk"], "REQ-SDK-3"),
        rule("umbra-test-plugin", &["umbra-sdk"], "REQ-SDK-3"),
    ],
};

/// Crates allowed to contain `unsafe` (CLAUDE.md rule 5, ASM-024).
pub const UNSAFE_ALLOWED: &[&str] = &["umbra-plugin-api", "umbra-plugin-host", "umbra-sdk"];

/// Protocol-neutral crates (ADR 0030, ASM-226, ASM-255): the engine side knows no protocol, so none
/// of them, nor any workspace crate they may depend on ([`protocol_scope`], ASM-256), may depend,
/// in any kind or target, on a crate whose name matches [`is_protocol_crate`]. Sorted. Workspace edges to adapter crates are already forbidden by
/// [`RULES`]; this entry covers external (registry, git, `[patch]`) protocol crates.
pub const PROTOCOL_NEUTRAL: &[&str] = &["umbra-core", "umbra-engine", "umbra-kernel", "umbra-net"];

/// Anchored name patterns of protocol crates (ASM-226): `-` and `_` are the same character and
/// case is ignored, so `redis` or `discord` never match.
#[must_use]
pub fn is_protocol_crate(name: &str) -> bool {
    const EXACT: &[&str] = &["dis-rs", "dis", "rti", "hla"];
    const PREFIX: &[&str] = &["open-dis", "opendis", "portico", "rti-", "hla-", "dis-"];
    const SUFFIX: &[&str] = &["-dis", "-hla"];
    let n = name.to_ascii_lowercase().replace('_', "-");
    EXACT.contains(&n.as_str())
        || PREFIX.iter().any(|p| n.starts_with(p))
        || n.contains("-dis-")
        || SUFFIX.iter().any(|s| n.ends_with(s))
}

/// Crates that must not depend on protocol crates: [`PROTOCOL_NEUTRAL`] plus the transitive closure
/// of their allowed workspace dependencies in `rules` (ASM-256). A protocol crate cannot enter
/// through an allowed member such as `umbra-templates`.
#[must_use]
pub fn protocol_scope(rules: &RuleSet) -> BTreeSet<&'static str> {
    let mut scope: BTreeSet<&'static str> = PROTOCOL_NEUTRAL.iter().copied().collect();
    for name in PROTOCOL_NEUTRAL {
        scope.extend(rules.closure(name));
    }
    scope
}
