//! Rule table (ASM-007): the section 4 dependency rules as a type-checked constant.

/// Rule for one workspace crate.
#[derive(Debug)]
pub struct CrateRule {
    /// Crate name.
    pub name: &'static str,
    /// Allowed workspace dependencies.
    pub allowed_workspace_deps: &'static [&'static str],
    /// External deps allowed (false only for `cgf-plugin-api`, normal and build).
    pub allow_external_deps: bool,
    /// "REQ-A-1", "REQ-A-2", "REQ-A-3" or "section 4".
    pub requirement: &'static str,
}

/// All rules.
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
        rule("cgf-core", &[], "REQ-A-1"),
        rule("cgf-kernel", &["cgf-core"], "REQ-A-2"),
        rule("cgf-motion", &["cgf-core"], "section 4"),
        rule("cgf-behavior", &["cgf-core"], "section 4"),
        rule("cgf-templates", &["cgf-core"], "section 4"),
        rule("cgf-net", &["cgf-core"], "section 4"),
        CrateRule {
            name: "cgf-plugin-api",
            allowed_workspace_deps: &[],
            allow_external_deps: false,
            requirement: "section 4",
        },
        rule(
            "cgf-plugin-host",
            &["cgf-core", "cgf-plugin-api"],
            "section 4",
        ),
        rule("cgf-sdk", &["cgf-plugin-api"], "REQ-A-3"),
        rule("cgf-net-dis", &["cgf-sdk"], "section 4"),
        rule("cgf-net-hla", &["cgf-sdk"], "section 4"),
        rule(
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
            "section 4",
        ),
        rule("cgf-api-types", &[], "section 4"),
        rule("cgf-server", &["cgf-api-types", "cgf-engine"], "section 4"),
        rule("cgf-cli", &["cgf-engine", "cgf-server"], "section 4"),
        rule("xtask", &[], "section 4"),
    ],
};

/// Crates allowed to contain `unsafe` (CLAUDE.md rule 5, ASM-024).
pub const UNSAFE_ALLOWED: &[&str] = &["cgf-plugin-api", "cgf-plugin-host", "cgf-sdk"];
