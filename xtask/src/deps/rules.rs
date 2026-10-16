//! Rule table (ASM-007). STUB: the table is empty until WI-3 fills it from the plan.

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

/// The full section 4 table. STUB: empty.
pub const RULES: RuleSet = RuleSet { crates: &[] };
