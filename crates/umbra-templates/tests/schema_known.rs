//! `CompiledSchema::validate_known` (ASM-106 d): the schema check of an object some of whose
//! properties are not known yet. Only the root object is relaxed.

#![forbid(unsafe_code)]

use umbra_core::ParamSchema;
use umbra_templates::value::parse;
use umbra_templates::{CompiledSchema, LValue, SourceFile};

fn value(text: &str) -> LValue {
    let (table, _) = parse(&SourceFile::new("x.toml", format!("p = {text}\n")));
    table
        .and_then(|t| t.get("p").cloned())
        .unwrap_or_else(|| panic!("bad test value {text}"))
}

fn schema(json: &str) -> CompiledSchema {
    let parsed = ParamSchema::from_json_str(json).unwrap_or_else(|e| panic!("{e}"));
    CompiledSchema::compile(&parsed).unwrap_or_else(|e| panic!("{e}"))
}

fn codes(found: &[umbra_templates::Diagnostic]) -> Vec<&'static str> {
    found.iter().map(|d| d.code.as_str()).collect()
}

fn known(json: &str, text: &str) -> Vec<&'static str> {
    codes(&schema(json).validate_known(&value(text), "s"))
}

fn full(json: &str, text: &str) -> Vec<&'static str> {
    codes(&schema(json).validate(&value(text), "s"))
}

const REQ: &str = r#"{"type":"object","properties":{"a":{"type":"number","maximum":5},"b":{"type":"string","enum":["x","y"]}},"required":["a","b"],"additionalProperties":false}"#;

#[test]
fn root_required_is_relaxed_but_plain_validate_still_requires() {
    assert_eq!(known(REQ, "{ a = 1.0 }"), [] as [&str; 0]);
    assert_eq!(full(REQ, "{ a = 1.0 }"), ["missing_param"]);
}

#[test]
fn range_type_and_enum_errors_on_known_properties_are_reported() {
    assert_eq!(known(REQ, "{ a = 9.0 }"), ["param_out_of_range"]);
    assert_eq!(known(REQ, "{ a = \"s\" }"), ["param_wrong_type"]);
    assert_eq!(known(REQ, "{ b = \"z\" }"), ["param_invalid"]);
    assert_eq!(full(REQ, "{ a = 9.0, b = \"x\" }"), ["param_out_of_range"]);
}

#[test]
fn root_additional_properties_false_still_reports_unknown_keys() {
    assert_eq!(known(REQ, "{ c = 1 }"), ["unknown_param"]);
}

const ONE_OF: &str = r#"{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"string"}},"oneOf":[{"required":["a"]},{"required":["b"]}]}"#;

#[test]
fn root_one_of_passes_when_a_relaxed_branch_passes() {
    assert_eq!(known(ONE_OF, "{ }"), [] as [&str; 0]);
    assert_eq!(known(ONE_OF, "{ a = 1.0 }"), [] as [&str; 0]);
    assert_eq!(known(ONE_OF, "{ a = 1.0, b = \"s\" }"), [] as [&str; 0]);
    assert_eq!(full(ONE_OF, "{ }").len(), 1);
}

const TYPED_BRANCHES: &str = r#"{"type":"object","oneOf":[{"properties":{"a":{"type":"number"}}},{"properties":{"a":{"type":"number"}},"required":["q"]}]}"#;

#[test]
fn root_one_of_reports_the_errors_when_no_branch_passes() {
    assert_eq!(
        known(TYPED_BRANCHES, "{ a = \"str\" }"),
        ["param_wrong_type"]
    );
    assert_eq!(known(TYPED_BRANCHES, "{ a = 1.0 }"), [] as [&str; 0]);
}

const ROOT_REF: &str = r##"{"$defs":{"o":{"$ref":"#/$defs/p"},"p":{"type":"object","properties":{"k":{"type":"number"}},"required":["k"],"additionalProperties":false}},"$ref":"#/$defs/o"}"##;

#[test]
fn a_root_ref_chain_is_relaxed() {
    assert_eq!(known(ROOT_REF, "{ }"), [] as [&str; 0]);
    assert_eq!(known(ROOT_REF, "{ k = \"s\" }"), ["param_wrong_type"]);
    assert_eq!(known(ROOT_REF, "{ j = 1 }"), ["unknown_param"]);
    assert_eq!(full(ROOT_REF, "{ }"), ["missing_param"]);
}

const NESTED: &str = r#"{"type":"object","properties":{"n":{"type":"object","properties":{"z":{"type":"number"}},"required":["z"]}},"required":["n"]}"#;

#[test]
fn nested_required_is_still_enforced() {
    assert_eq!(known(NESTED, "{ }"), [] as [&str; 0]);
    assert_eq!(known(NESTED, "{ n = { } }"), ["missing_param"]);
}

const SHARED: &str = r##"{"$defs":{"o":{"type":"object","properties":{"k":{"type":"number"}},"required":["k"]}},"$ref":"#/$defs/o","properties":{"nested":{"$ref":"#/$defs/o"}}}"##;

#[test]
fn a_def_shared_by_the_root_and_a_property_is_relaxed_only_at_the_root() {
    assert_eq!(known(SHARED, "{ nested = { } }"), ["missing_param"]);
    assert_eq!(known(SHARED, "{ nested = { k = 1.0 } }"), [] as [&str; 0]);
    assert_eq!(known(SHARED, "{ }"), [] as [&str; 0]);
}
