//! AT-2 (syntax part) and AT-9 (`parse_never_panics`): the located TOML tree.
//! Requirements: REQ-T-2, ASM-054 rules 1 and 2, ASM-056, NFR-2.
//!
//! Every syntax error is reported with file, line and column. Expected locations of the fixtures
//! under `tests/fixtures/syntax/` were verified against `toml` 1.1.8 (see `toml_pin.rs`).

#![forbid(unsafe_code)]

mod common;

use proptest::prelude::*;

use cgf_templates::value::parse;
use cgf_templates::{Diagnostic, SourceFile};
use common::{fixture_path, fixture_source, row, rows};

fn syntax_rows(rel: &str) -> Vec<common::Row> {
    let (_, diagnostics) = parse(&fixture_source(rel));
    rows(&diagnostics)
}

fn syntax(rel: &str, at: (u32, u32)) -> common::Row {
    row("syntax", fixture_path(rel), at)
}

fn diagnostics_of(text: &str) -> Vec<Diagnostic> {
    parse(&SourceFile::new("a.toml", text)).1
}

#[test]
fn given_valid_file_then_no_diagnostics_and_a_table() {
    let (table, diagnostics) = parse(&SourceFile::new("a.toml", "a = 1\n[t]\nb = \"x\"\n"));

    assert_eq!(diagnostics, []);
    assert!(table.is_some_and(|t| t.get("a").is_some() && t.get("t").is_some()));
}

/// ASM-054 rule 1: several real errors on one line are all kept.
#[test]
fn given_two_bad_values_in_one_inline_table_then_both_reported() {
    let rel = "syntax/inline_two_bad_values.toml";

    assert_eq!(
        syntax_rows(rel),
        [syntax(rel, (9, 16)), syntax(rel, (9, 23))]
    );
}

/// ASM-054 rule 1.
#[test]
fn given_two_bad_array_elements_then_both_reported() {
    let rel = "syntax/array_two_bad_elements.toml";

    assert_eq!(
        syntax_rows(rel),
        [syntax(rel, (11, 16)), syntax(rel, (11, 22))]
    );
}

/// ASM-054 rule 1: a duplicate key is a syntax error, and the bad value after it is still reported.
#[test]
fn given_duplicate_key_and_bad_value_in_one_inline_table_then_both_reported() {
    let rel = "syntax/duplicate_key_and_bad_value.toml";

    let (_, diagnostics) = parse(&fixture_source(rel));

    assert_eq!(
        rows(&diagnostics),
        [syntax(rel, (8, 21)), syntax(rel, (8, 34))]
    );
    assert!(
        diagnostics[0].message.contains("duplicate key"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn given_duplicate_key_in_a_table_then_syntax_error_at_the_second_key() {
    let diagnostics = diagnostics_of("a = 1\nb = 2\na = 3\n");

    assert_eq!(rows(&diagnostics), [row("syntax", "a.toml", (3, 1))]);
}

/// ASM-054 rule 2: the unclosed array is reported once, at its opening bracket (6:8); the cascade
/// after it is suppressed; the independent error before it (5:13) is still reported, in span order
/// (the parser lists the unclosed error first).
#[test]
fn given_unclosed_array_followed_by_tables_then_one_error_at_the_opening_bracket() {
    let rel = "syntax/unclosed_array_then_tables.toml";

    let (_, diagnostics) = parse(&fixture_source(rel));

    assert_eq!(
        rows(&diagnostics),
        [syntax(rel, (5, 13)), syntax(rel, (6, 8))]
    );
    assert!(
        diagnostics[1].message.contains("unclosed array"),
        "{}",
        diagnostics[1].message
    );
}

/// ASM-054 rule 2 for inline tables: one error at the opening brace (5:9), nothing after it.
#[test]
fn given_unclosed_inline_table_followed_by_tables_then_one_error_at_the_opening_brace() {
    let rel = "syntax/unclosed_inline_table_then_tables.toml";

    let (_, diagnostics) = parse(&fixture_source(rel));

    assert_eq!(rows(&diagnostics), [syntax(rel, (5, 9))]);
    assert!(
        diagnostics[0].message.contains("unclosed inline table"),
        "{}",
        diagnostics[0].message
    );
}

/// ASM-054 rule 2: the rule is about the cascade, not about a second independent error elsewhere:
/// a bad value before an unclosed array on the same file stays (covered above); an unclosed array
/// alone yields exactly one error however many tables follow.
#[test]
fn given_unclosed_array_then_many_lines_then_only_one_error_regardless_of_length() {
    let tables: Vec<String> = (0..40).map(|i| format!("[t{i}]\nk{i} = {i}\n")).collect();
    let text = format!("tags = [1, 2\n{}", tables.concat());

    let diagnostics = diagnostics_of(&text);

    assert_eq!(rows(&diagnostics), [row("syntax", "a.toml", (1, 8))]);
}

/// B1: two unclosed containers each get their own error at their own opening bracket; neither
/// suppresses the other and the file is never accepted.
#[test]
fn given_unclosed_array_inside_unclosed_inline_table_then_both_reported_at_their_brackets() {
    let text = "x = { a = [1, 2\n[t]\nq = 1\n";

    assert_eq!(
        rows(&diagnostics_of(text)),
        [
            row("syntax", "a.toml", (1, 5)),
            row("syntax", "a.toml", (1, 11))
        ]
    );
}

#[test]
fn given_unclosed_inline_table_inside_unclosed_array_then_both_reported_at_their_brackets() {
    let text = "x = [1, { a = 1\nz = 2\n";

    assert_eq!(
        rows(&diagnostics_of(text)),
        [
            row("syntax", "a.toml", (1, 5)),
            row("syntax", "a.toml", (1, 9))
        ]
    );
}

#[test]
fn given_unclosed_array_closed_by_a_brace_inside_an_inline_table_then_both_reported() {
    let text = "a = {x = 1, y = [1, 2}\nb = 3\n";

    assert_eq!(
        rows(&diagnostics_of(text)),
        [
            row("syntax", "a.toml", (1, 5)),
            row("syntax", "a.toml", (1, 17))
        ]
    );
}

#[test]
fn given_two_unclosed_containers_after_a_valid_header_then_the_file_is_not_accepted() {
    let text = "[motion]\nmodel = \"m.n\"\nparams = { a = [1, 2\n";

    assert_eq!(
        rows(&diagnostics_of(text)),
        [
            row("syntax", "a.toml", (3, 10)),
            row("syntax", "a.toml", (3, 16))
        ]
    );
}

/// M1: the bracket is found even when the last item ends with its own closing bracket.
#[test]
fn given_unclosed_array_whose_last_item_is_closed_then_error_at_the_opening_bracket() {
    for text in ["x = [[1]\n", "x = [1, [2]\n", "x = {a = {b = 1}\n"] {
        assert_eq!(
            rows(&diagnostics_of(text)),
            [row("syntax", "a.toml", (1, 5))],
            "{text:?}"
        );
    }
}

#[test]
fn given_syntax_errors_in_two_places_then_sorted_by_position() {
    let diagnostics = diagnostics_of("a = $\nb = 1\nc = $\n");

    assert_eq!(
        rows(&diagnostics),
        [
            row("syntax", "a.toml", (1, 5)),
            row("syntax", "a.toml", (3, 5))
        ]
    );
}

#[test]
fn given_crlf_line_endings_then_error_line_and_column_are_correct() {
    let diagnostics = diagnostics_of("a = 1\r\nb = $\r\n");

    assert_eq!(rows(&diagnostics), [row("syntax", "a.toml", (2, 5))]);
}

/// ASM-056: columns count characters; the parser's byte offset 9 must become column 9, not 10.
#[test]
fn given_multibyte_text_before_the_error_then_column_counts_characters() {
    let diagnostics = diagnostics_of("x = \"\u{e9}\" $\ny = 1\n");

    assert_eq!(rows(&diagnostics), [row("syntax", "a.toml", (1, 9))]);
}

/// ASM-060: datetimes and non-finite floats cannot cross as JSON (REQ-P-6).
#[test]
fn given_datetime_value_then_unsupported_value_at_its_span() {
    let diagnostics = diagnostics_of("ok = 1\nwhen = 1979-05-27T07:32:00Z\n");

    assert_eq!(
        rows(&diagnostics),
        [row("unsupported_value", "a.toml", (2, 8))]
    );
}

#[test]
fn given_nan_then_unsupported_value_at_its_span() {
    let diagnostics = diagnostics_of("x = nan\n");

    assert_eq!(
        rows(&diagnostics),
        [row("unsupported_value", "a.toml", (1, 5))]
    );
}

#[test]
fn given_inf_and_negative_inf_then_unsupported_value_at_each() {
    let diagnostics = diagnostics_of("x = inf\ny = -inf\n");

    assert_eq!(
        rows(&diagnostics),
        [
            row("unsupported_value", "a.toml", (1, 5)),
            row("unsupported_value", "a.toml", (2, 5))
        ]
    );
}

#[test]
fn given_datetime_nested_in_an_array_and_a_table_then_each_is_reported() {
    let diagnostics = diagnostics_of("a = [1979-05-27, 2]\n[t]\nb = { c = 07:32:00 }\n");

    assert_eq!(
        rows(&diagnostics),
        [
            row("unsupported_value", "a.toml", (1, 6)),
            row("unsupported_value", "a.toml", (3, 11))
        ]
    );
}

/// NFR-2: nesting deeper than the parser allows is an error, not a stack overflow.
#[test]
fn given_very_deep_nesting_then_syntax_error_instead_of_stack_overflow() {
    let deep = format!("a = {}1{}\n", "[".repeat(5000), "]".repeat(5000));

    let diagnostics = diagnostics_of(&deep);

    assert_ne!(diagnostics.len(), 0);
    assert!(diagnostics.iter().all(|d| d.code.as_str() == "syntax"));
}

#[test]
fn given_empty_file_then_empty_table_and_no_diagnostics() {
    let (table, diagnostics) = parse(&SourceFile::new("a.toml", ""));

    assert_eq!(diagnostics, []);
    assert!(table.is_some());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// AT-9, NFR-2: arbitrary text never panics `parse`, and every diagnostic has a 1-based position.
    #[test]
    fn parse_never_panics(text in "\\PC{0,200}") {
        let (_, diagnostics) = parse(&SourceFile::new("a.toml", text));
        for d in &diagnostics {
            if let Some(l) = &d.location {
                prop_assert!(l.line >= 1 && l.column >= 1);
            }
        }
    }

    /// B1 invariant: if the parser reports any error there is at least one syntax diagnostic.
    #[test]
    fn toml_errors_imply_a_syntax_diagnostic(text in "[a-z0-9 =\\[\\]{}.,#\"'$:\\n+-]{0,120}") {
        let (_, errors) = toml::de::DeTable::parse_recoverable(&text);
        let (_, diagnostics) = parse(&SourceFile::new("a.toml", text.clone()));
        if !errors.is_empty() {
            prop_assert!(diagnostics.iter().any(|d| d.code.as_str() == "syntax"), "{text:?}");
        }
    }

    /// NFR-2 with TOML-shaped noise, which reaches far more parser states than random text.
    #[test]
    fn parse_never_panics_on_toml_shaped_text(text in "[a-z0-9 =\\[\\]{}.,#\"'$:\\n\\r+-]{0,160}") {
        let (_, diagnostics) = parse(&SourceFile::new("a.toml", text));
        for d in &diagnostics {
            if let Some(l) = &d.location {
                prop_assert!(l.line >= 1 && l.column >= 1);
            }
        }
    }
}

/// m1: a parser error without a span is not given an invented position (here: the helper that
/// builds the diagnostic is exercised through a recursion-limit input, whose error has a span; the
/// span-less path is covered by the unit test in `value.rs`).
#[test]
fn given_deep_nesting_every_syntax_diagnostic_has_a_real_position() {
    let deep = format!("a = {}1{}\n", "[".repeat(300), "]".repeat(300));

    let diagnostics = diagnostics_of(&deep);

    assert_ne!(diagnostics.len(), 0);
    assert!(
        diagnostics
            .iter()
            .all(|d| d.location.as_ref().is_some_and(|l| l.span.start > 0))
    );
}

/// m2: placeholders do not produce follow-on values.
#[test]
fn given_unsupported_values_then_they_are_placeholders_not_strings_or_zeros() {
    let (table, diagnostics) = parse(&SourceFile::new(
        "a.toml",
        "d = 1979-05-27\nbig = 99999999999999999999\nf = 1e999\n",
    ));

    assert_eq!(diagnostics.len(), 3);
    let table = table.unwrap_or_else(|| panic!("no table"));
    for key in ["d", "big", "f"] {
        let kind = table.get(key).map(|v| v.kind().type_name());
        assert_eq!(kind, Some("unsupported value"), "{key}");
    }
    assert!(
        diagnostics[2].message.contains("out of range"),
        "{}",
        diagnostics[2].message
    );
}
