//! AT-3 (validator rows): the JSON Schema subset of ADR 0009. Requirements: REQ-T-3, REQ-T-2,
//! ADR 0009.
//!
//! Each test validates the TOML value of `x = <value>` (so the value is at line 1, column 5)
//! against a schema and checks code, path, line and column of every violation.
//!
//! Code mapping used here (the plan lists the codes but not which keyword maps to which):
//! `type` is `param_wrong_type`; `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`,
//! `minLength`, `maxLength`, `minItems`, `maxItems` are `param_out_of_range`; `enum`, `const` and
//! `oneOf` are `param_invalid`; `required` is `missing_param`; `additionalProperties: false` is
//! `unknown_param`, located at the unknown key.

#![forbid(unsafe_code)]

mod common;

use common::{Row, pos_of, row, rows};
use umbra_core::ParamSchema;
use umbra_templates::value::parse;
use umbra_templates::{CompiledSchema, Diagnostic, SchemaError, SourceFile};

const CTX: &str = "motion model `builtin.ground.kinematic`";

fn compile(schema: &str) -> Result<CompiledSchema, SchemaError> {
    let parsed = ParamSchema::from_json_str(schema)
        .unwrap_or_else(|e| panic!("test schema is not JSON: {e}"));
    CompiledSchema::compile(&parsed)
}

fn violations(schema: &str, text: &str) -> Vec<Diagnostic> {
    let compiled = compile(schema).unwrap_or_else(|e| panic!("schema should compile: {e}"));
    let file = SourceFile::new("v.toml", text);
    let (table, syntax) = parse(&file);
    assert!(syntax.is_empty(), "test input must parse: {syntax:?}");
    let table = table.unwrap_or_else(|| panic!("no table"));
    let value = table
        .get("x")
        .unwrap_or_else(|| panic!("no `x` in {text:?}"));
    compiled.validate(value, CTX)
}

/// Rows of the violations of `x = <value>` against `schema`.
fn check(schema: &str, value: &str) -> Vec<Row> {
    rows(&violations(schema, &format!("x = {value}\n")))
}

fn at(code: &str, line: u32, col: u32) -> Row {
    row(code, "v.toml", (line, col))
}

// type ---------------------------------------------------------------------------------------

#[test]
fn type_string_accepts_a_string() {
    assert_eq!(check(r#"{"type":"string"}"#, r#""hi""#), []);
}

#[test]
fn type_string_rejects_an_integer_at_the_value() {
    assert_eq!(
        check(r#"{"type":"string"}"#, "5"),
        [at("param_wrong_type", 1, 5)]
    );
}

#[test]
fn type_number_accepts_a_float() {
    assert_eq!(check(r#"{"type":"number"}"#, "1.5"), []);
}

#[test]
fn integer_satisfies_number() {
    assert_eq!(check(r#"{"type":"number"}"#, "18"), []);
}

#[test]
fn float_does_not_satisfy_integer() {
    assert_eq!(
        check(r#"{"type":"integer"}"#, "18.0"),
        [at("param_wrong_type", 1, 5)]
    );
}

#[test]
fn type_integer_accepts_an_integer() {
    assert_eq!(check(r#"{"type":"integer"}"#, "-3"), []);
}

#[test]
fn type_number_rejects_a_string() {
    assert_eq!(
        check(r#"{"type":"number"}"#, r#""fast""#),
        [at("param_wrong_type", 1, 5)]
    );
}

#[test]
fn type_boolean_accepts_and_rejects() {
    let schema = r#"{"type":"boolean"}"#;

    assert_eq!(check(schema, "true"), []);
    assert_eq!(check(schema, "1"), [at("param_wrong_type", 1, 5)]);
}

#[test]
fn type_array_accepts_and_rejects() {
    let schema = r#"{"type":"array"}"#;

    assert_eq!(check(schema, "[1, 2]"), []);
    assert_eq!(check(schema, "{ a = 1 }"), [at("param_wrong_type", 1, 5)]);
}

#[test]
fn type_object_accepts_and_rejects() {
    let schema = r#"{"type":"object"}"#;

    assert_eq!(check(schema, "{ a = 1 }"), []);
    assert_eq!(check(schema, "[1]"), [at("param_wrong_type", 1, 5)]);
}

#[test]
fn type_list_accepts_any_listed_type() {
    let schema = r#"{"type":["string","integer"]}"#;

    assert_eq!(check(schema, r#""s""#), []);
    assert_eq!(check(schema, "2"), []);
    assert_eq!(check(schema, "2.5"), [at("param_wrong_type", 1, 5)]);
}

/// The plan's error example, exactly (REQ-T-2).
#[test]
fn wrong_type_message_names_the_parameter_the_expected_type_and_the_found_type() {
    let schema = r#"{"type":"object","properties":{"max_speed_mps":{"type":"number"}}}"#;

    let d = violations(schema, "x = { max_speed_mps = \"fast\" }\n");

    assert_eq!(d.len(), 1);
    assert_eq!(
        d[0].message,
        "motion model `builtin.ground.kinematic`: parameter `max_speed_mps` must be a number, found string"
    );
    assert_eq!(rows(&d), [at("param_wrong_type", 1, 23)]);
}

// properties, required, additionalProperties -----------------------------------------------------

#[test]
fn properties_validate_each_named_value_at_its_own_location() {
    let schema = r#"{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"string"}}}"#;
    let value = "{ a = 1, b = 2 }";
    let text = format!("x = {value}\n");

    assert_eq!(
        rows(&violations(schema, &text)),
        [row("param_wrong_type", "v.toml", pos_of(&text, "2 }"))]
    );
}

#[test]
fn properties_not_present_in_the_value_are_not_an_error() {
    let schema = r#"{"type":"object","properties":{"a":{"type":"number"}}}"#;

    assert_eq!(check(schema, "{ }"), []);
}

#[test]
fn required_key_present_is_accepted() {
    let schema = r#"{"type":"object","required":["a"]}"#;

    assert_eq!(check(schema, "{ a = 1 }"), []);
}

#[test]
fn required_key_missing_is_reported_at_the_object() {
    let schema = r#"{"type":"object","required":["a"]}"#;

    assert_eq!(check(schema, "{ b = 1 }"), [at("missing_param", 1, 5)]);
}

/// The plan's error example, exactly.
#[test]
fn missing_param_message_names_the_parameter() {
    let schema = r#"{"type":"object","required":["max_accel_mps2"]}"#;

    let d = violations(schema, "x = { }\n");

    assert_eq!(d.len(), 1);
    assert_eq!(
        d[0].message,
        "motion model `builtin.ground.kinematic`: missing required parameter `max_accel_mps2`"
    );
}

#[test]
fn missing_key_in_a_header_table_is_reported_at_the_header() {
    let schema = r#"{"type":"object","required":["a"]}"#;
    let text = "[x]\nb = 1\n";
    let compiled = compile(schema).unwrap_or_else(|e| panic!("{e}"));
    let (table, _) = parse(&SourceFile::new("v.toml", text));
    let table = table.unwrap_or_else(|| panic!("no table"));

    let d = compiled.validate(table.get("x").unwrap_or_else(|| panic!("no x")), CTX);

    assert_eq!(rows(&d), [at("missing_param", 1, 1)]);
}

#[test]
fn additional_properties_false_reports_the_unknown_key() {
    let schema = r#"{"type":"object","properties":{"a":{}},"additionalProperties":false}"#;
    let text = "x = { a = 1, zzz = 2 }\n";

    assert_eq!(
        rows(&violations(schema, text)),
        [row("unknown_param", "v.toml", pos_of(text, "zzz"))]
    );
}

#[test]
fn additional_properties_false_accepts_only_declared_keys() {
    let schema = r#"{"type":"object","properties":{"a":{}},"additionalProperties":false}"#;

    assert_eq!(check(schema, "{ a = 1 }"), []);
}

#[test]
fn additional_properties_true_accepts_any_extra_key() {
    let schema = r#"{"type":"object","properties":{"a":{}},"additionalProperties":true}"#;

    assert_eq!(check(schema, "{ a = 1, b = \"x\" }"), []);
}

#[test]
fn additional_properties_schema_validates_extra_values() {
    let schema = r#"{"type":"object","additionalProperties":{"type":"number"}}"#;
    let text = "x = { a = 1, b = \"s\" }\n";

    assert_eq!(check(schema, "{ a = 1, b = 2.5 }"), []);
    assert_eq!(
        rows(&violations(schema, text)),
        [row("param_wrong_type", "v.toml", pos_of(text, "\"s\""))]
    );
}

#[test]
fn all_violations_in_one_object_reported() {
    let schema = r#"{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"string"}},"required":["c"],"additionalProperties":false}"#;
    let text = "x = { a = \"x\", b = 1, d = 2 }\n";

    assert_eq!(
        rows(&violations(schema, text)),
        [
            row("missing_param", "v.toml", (1, 5)),
            row("param_wrong_type", "v.toml", pos_of(text, "\"x\"")),
            row("param_wrong_type", "v.toml", pos_of(text, "1,")),
            row("unknown_param", "v.toml", pos_of(text, "d =")),
        ]
    );
}

#[test]
fn nested_objects_are_validated_at_their_own_location() {
    let schema = r#"{"type":"object","properties":{"p":{"type":"object","properties":{"q":{"type":"integer"}}}}}"#;
    let text = "x = { p = { q = 1.5 } }\n";

    assert_eq!(
        rows(&violations(schema, text)),
        [row("param_wrong_type", "v.toml", pos_of(text, "1.5"))]
    );
}

// items, minItems, maxItems -----------------------------------------------------------------------

#[test]
fn items_accepts_matching_elements() {
    assert_eq!(
        check(r#"{"type":"array","items":{"type":"number"}}"#, "[1, 2.5]"),
        []
    );
}

#[test]
fn items_rejects_each_bad_element_at_its_location() {
    let schema = r#"{"type":"array","items":{"type":"number"}}"#;
    let text = "x = [1, \"a\", 3, \"b\"]\n";

    assert_eq!(
        rows(&violations(schema, text)),
        [
            row("param_wrong_type", "v.toml", pos_of(text, "\"a\"")),
            row("param_wrong_type", "v.toml", pos_of(text, "\"b\"")),
        ]
    );
}

#[test]
fn min_items_accepts_at_the_bound_and_rejects_below_at_the_array() {
    let schema = r#"{"type":"array","minItems":3}"#;

    assert_eq!(check(schema, "[1, 2, 3]"), []);
    assert_eq!(check(schema, "[1, 2]"), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn max_items_accepts_at_the_bound_and_rejects_above_at_the_array() {
    let schema = r#"{"type":"array","maxItems":3}"#;

    assert_eq!(check(schema, "[1, 2, 3]"), []);
    assert_eq!(
        check(schema, "[1, 2, 3, 4]"),
        [at("param_out_of_range", 1, 5)]
    );
}

// enum, const -------------------------------------------------------------------------------------

#[test]
fn enum_accepts_a_listed_value_and_rejects_another_at_the_value() {
    let schema = r#"{"enum":["afap","realtime"]}"#;

    assert_eq!(check(schema, "\"afap\""), []);
    assert_eq!(check(schema, "\"warp\""), [at("param_invalid", 1, 5)]);
}

#[test]
fn enum_compares_numbers_by_value() {
    let schema = r#"{"enum":[1,2.5]}"#;

    assert_eq!(check(schema, "1"), []);
    assert_eq!(check(schema, "2.5"), []);
    assert_eq!(check(schema, "3"), [at("param_invalid", 1, 5)]);
}

#[test]
fn const_accepts_the_value_and_rejects_another() {
    let schema = r#"{"const":1}"#;

    assert_eq!(check(schema, "1"), []);
    assert_eq!(check(schema, "2"), [at("param_invalid", 1, 5)]);
}

// numeric bounds ----------------------------------------------------------------------------------

#[test]
fn minimum_accepts_the_bound_and_rejects_below() {
    let schema = r#"{"type":"number","minimum":0}"#;

    assert_eq!(check(schema, "0.0"), []);
    assert_eq!(check(schema, "-0.5"), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn maximum_accepts_the_bound_and_rejects_above() {
    let schema = r#"{"type":"number","maximum":10}"#;

    assert_eq!(check(schema, "10"), []);
    assert_eq!(check(schema, "10.5"), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn exclusive_minimum_rejects_the_bound_itself() {
    let schema = r#"{"type":"number","exclusiveMinimum":0}"#;

    assert_eq!(check(schema, "0.001"), []);
    assert_eq!(check(schema, "0.0"), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn exclusive_maximum_rejects_the_bound_itself() {
    let schema = r#"{"type":"number","exclusiveMaximum":5}"#;

    assert_eq!(check(schema, "4.999"), []);
    assert_eq!(check(schema, "5"), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn bounds_apply_to_integers_and_floats_alike() {
    let schema = r#"{"type":"number","minimum":1}"#;

    assert_eq!(check(schema, "1"), []);
    assert_eq!(check(schema, "0"), [at("param_out_of_range", 1, 5)]);
}

// string length -----------------------------------------------------------------------------------

#[test]
fn min_length_accepts_the_bound_and_rejects_shorter() {
    let schema = r#"{"type":"string","minLength":2}"#;

    assert_eq!(check(schema, "\"ab\""), []);
    assert_eq!(check(schema, "\"a\""), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn max_length_accepts_the_bound_and_rejects_longer() {
    let schema = r#"{"type":"string","maxLength":2}"#;

    assert_eq!(check(schema, "\"ab\""), []);
    assert_eq!(check(schema, "\"abc\""), [at("param_out_of_range", 1, 5)]);
}

#[test]
fn string_length_counts_unicode_scalar_values_not_bytes() {
    let schema = r#"{"type":"string","maxLength":2}"#;

    // Four bytes, two characters.
    assert_eq!(check(schema, "\"\u{e9}\u{e9}\""), []);
}

// oneOf -------------------------------------------------------------------------------------------

const POSITION_SCHEMA: &str = r#"{"oneOf":[
  {"type":"object","properties":{"ecef_m":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}},"required":["ecef_m"],"additionalProperties":false},
  {"type":"object","properties":{"lat_deg":{"type":"number"},"lon_deg":{"type":"number"}},"required":["lat_deg","lon_deg"],"additionalProperties":false}
]}"#;

#[test]
fn one_of_accepts_exactly_one_matching_branch() {
    assert_eq!(
        check(POSITION_SCHEMA, "{ lat_deg = 52.0, lon_deg = 4.0 }"),
        []
    );
    assert_eq!(check(POSITION_SCHEMA, "{ ecef_m = [1.0, 2.0, 3.0] }"), []);
}

#[test]
fn one_of_reports_closest_branch() {
    let text = "x = { lat_deg = 52.0, lon_deg = \"east\" }\n";

    // Branch 2 has one error; branch 1 has two (missing `ecef_m`, unknown keys), so branch 2 is
    // reported: the wrong type of `lon_deg` and nothing else.
    assert_eq!(
        rows(&violations(POSITION_SCHEMA, text)),
        [row("param_wrong_type", "v.toml", pos_of(text, "\"east\""))]
    );
}

#[test]
fn one_of_with_no_match_and_equal_error_counts_reports_the_first_branch() {
    let schema = r#"{"oneOf":[{"type":"string"},{"type":"integer"}]}"#;

    let d = violations(schema, "x = 1.5\n");

    assert_eq!(rows(&d), [at("param_wrong_type", 1, 5)]);
    assert!(d[0].message.contains("string"), "{}", d[0].message);
}

#[test]
fn one_of_matching_two_branches_is_invalid_at_the_value() {
    let schema = r#"{"oneOf":[{"type":"number"},{"type":"integer"}]}"#;

    assert_eq!(check(schema, "5"), [at("param_invalid", 1, 5)]);
}

// $defs and $ref ----------------------------------------------------------------------------------

#[test]
fn local_ref_resolves() {
    let schema = r##"{"$defs":{"pos":{"type":"number","minimum":0}},"type":"object","properties":{"a":{"$ref":"#/$defs/pos"}}}"##;
    let text = "x = { a = -1 }\n";

    assert_eq!(check(schema, "{ a = 1 }"), []);
    assert_eq!(
        rows(&violations(schema, text)),
        [row("param_out_of_range", "v.toml", pos_of(text, "-1"))]
    );
}

#[test]
fn ref_to_a_ref_resolves() {
    let schema =
        r##"{"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"type":"string"}},"$ref":"#/$defs/a"}"##;

    assert_eq!(check(schema, "\"s\""), []);
    assert_eq!(check(schema, "1"), [at("param_wrong_type", 1, 5)]);
}

#[test]
fn ref_cycle_is_compile_error() {
    let schema =
        r##"{"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"$ref":"#/$defs/a"}},"$ref":"#/$defs/a"}"##;

    let error = compile(schema).err();

    assert!(error.is_some(), "a $ref cycle must not compile");
}

#[test]
fn ref_to_a_missing_definition_is_compile_error() {
    let schema = r##"{"$ref":"#/$defs/nope"}"##;

    assert!(compile(schema).is_err());
}

#[test]
fn remote_ref_is_compile_error() {
    let schema = r#"{"$ref":"https://example.com/schema.json"}"#;

    assert!(compile(schema).is_err());
}

// fail-closed compilation -------------------------------------------------------------------------

#[test]
fn unsupported_keyword_is_compile_error_with_pointer() {
    let schema = r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^x"}}}"#;

    let error = compile(schema)
        .err()
        .unwrap_or_else(|| panic!("`pattern` must be rejected"));

    assert!(
        error.pointer.starts_with("/properties/a"),
        "{}",
        error.pointer
    );
    assert!(
        format!("{}{}", error.pointer, error.message).contains("pattern"),
        "{error}"
    );
}

#[test]
fn all_of_is_a_compile_error() {
    assert!(compile(r#"{"allOf":[{"type":"string"}]}"#).is_err());
}

#[test]
fn any_of_and_format_and_if_are_compile_errors() {
    assert!(compile(r#"{"anyOf":[{"type":"string"}]}"#).is_err());
    assert!(compile(r#"{"type":"string","format":"uri"}"#).is_err());
    assert!(compile(r#"{"if":{"type":"string"},"then":{}}"#).is_err());
}

#[test]
fn unknown_type_name_is_a_compile_error() {
    assert!(compile(r#"{"type":"strng"}"#).is_err());
}

#[test]
fn annotations_are_ignored() {
    let schema = r#"{
      "$schema":"https://json-schema.org/draft/2020-12/schema","$id":"urn:test","$comment":"c",
      "title":"t","description":"d","default":1,"examples":[1],"deprecated":false,
      "readOnly":false,"writeOnly":false,"type":"number"}"#;

    assert_eq!(check(schema, "5"), []);
}

#[test]
fn error_order_is_independent_of_schema_key_order() {
    let forward = r#"{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"},"c":{"type":"number"}},"required":["d"],"additionalProperties":false}"#;
    let reversed = r#"{"additionalProperties":false,"required":["d"],"properties":{"c":{"type":"number"},"b":{"type":"number"},"a":{"type":"number"}},"type":"object"}"#;
    let value = "{ c = \"1\", a = \"2\", b = \"3\", e = 4 }";

    let (f, r) = (check(forward, value), check(reversed, value));

    assert_eq!(f, r);
    let positions: Vec<(u32, u32)> = f.iter().map(|x| (x.2, x.3)).collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "violations are ordered by position");
}

// A realistic component schema --------------------------------------------------------------------

#[test]
fn ground_kinematic_style_schema_reports_every_problem_in_one_pass() {
    let schema = r#"{"type":"object","properties":{
      "max_speed_mps":{"type":"number","exclusiveMinimum":0},
      "max_turn_rate_dps":{"type":"number","exclusiveMinimum":0},
      "max_accel_mps2":{"type":"number","exclusiveMinimum":0}},
      "required":["max_speed_mps","max_turn_rate_dps","max_accel_mps2"],"additionalProperties":false}"#;
    let text = "x = { max_speed_mps = \"fast\", max_turn_rate_dps = 0.0, speed = 3 }\n";

    assert_eq!(
        rows(&violations(schema, text)),
        [
            row("missing_param", "v.toml", (1, 5)),
            row("param_wrong_type", "v.toml", pos_of(text, "\"fast\"")),
            row("param_out_of_range", "v.toml", pos_of(text, "0.0")),
            row("unknown_param", "v.toml", pos_of(text, "speed = 3")),
        ]
    );
}

// review fixes ----------------------------------------------------------------------------------

/// m4: a small schema whose `$ref`s fan out exponentially is rejected at compile time.
#[test]
fn exponentially_expanding_ref_chains_are_a_compile_error() {
    let mut defs = vec![r#""d0":{"type":"integer"}"#.to_string()];
    for i in 1..=20 {
        let p = i - 1;
        defs.push(format!(
            r##""d{i}":{{"oneOf":[{{"$ref":"#/$defs/d{p}"}},{{"$ref":"#/$defs/d{p}"}}]}}"##
        ));
    }
    let schema = format!(
        r##"{{"$defs":{{{}}},"$ref":"#/$defs/d20"}}"##,
        defs.join(",")
    );

    let error = compile(&schema).err();

    assert!(error.is_some_and(|e| e.message.contains("expands")));
}

#[test]
fn a_moderate_ref_fan_out_still_compiles() {
    let schema = r##"{"$defs":{"n":{"type":"number"}},"type":"object","properties":{"a":{"$ref":"#/$defs/n"},"b":{"$ref":"#/$defs/n"}}}"##;

    assert!(compile(schema).is_ok());
}

/// Nit: integers are compared exactly against integer bounds (beyond 2^53 f64 would conflate them).
#[test]
fn integer_bounds_are_compared_exactly() {
    let schema = r#"{"type":"integer","maximum":9007199254740992}"#;

    assert_eq!(check(schema, "9007199254740992"), []);
    assert_eq!(
        check(schema, "9007199254740993"),
        [at("param_out_of_range", 1, 5)]
    );
}

/// Nit: `~1` and `~0` in `$ref` names are decoded.
#[test]
fn ref_names_are_json_pointer_decoded() {
    let schema = r##"{"$defs":{"a/b":{"type":"string"}},"$ref":"#/$defs/a~1b"}"##;

    assert_eq!(check(schema, "\"s\""), []);
    assert_eq!(check(schema, "1"), [at("param_wrong_type", 1, 5)]);
}

/// m2: a value `parse` already rejected causes no schema error on top.
#[test]
fn unsupported_values_are_skipped_silently() {
    let compiled = compile(r#"{"type":"string","minLength":3}"#).unwrap_or_else(|e| panic!("{e}"));
    let file = SourceFile::new("v.toml", "x = 1979-05-27\n");
    let (table, diagnostics) = parse(&file);
    let table = table.unwrap_or_else(|| panic!("no table"));
    let value = table.get("x").unwrap_or_else(|| panic!("no x"));

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(compiled.validate(value, CTX), []);
}
