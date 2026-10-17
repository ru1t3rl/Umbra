//! Pins the behaviour of `toml` 1.1.8 that ASM-054 depends on. The syntax-error reader suppresses
//! cascades by matching `toml`'s message text and uses the spanned tree to find the opening bracket
//! and the template `id`; a `toml` upgrade that changes any of this must fail here, loudly, before
//! it silently changes diagnostics. These tests call `toml` directly and pass on the stubs.
//!
//! Provenance: the expected values were obtained by running `DeTable::parse_recoverable` from
//! `toml` 1.1.8 (features `parse` + `std`) on each fixture under `tests/fixtures/syntax/`.

#![forbid(unsafe_code)]

mod common;

use toml::de::{DeTable, DeValue};

use common::{pos_of, read_fixture};

/// `(line, column, message)` of every error, sorted by span start (the parser's own order is not
/// sorted: lexer-phase errors come after parser-phase errors).
fn errors_of(text: &str) -> Vec<(u32, u32, String)> {
    let (_, errors) = DeTable::parse_recoverable(text);
    let mut spans: Vec<(usize, String)> = errors
        .iter()
        .map(|e| (e.span().map_or(0, |s| s.start), e.message().to_string()))
        .collect();
    spans.sort();
    spans
        .into_iter()
        .map(|(start, message)| {
            let before = &text[..start];
            let line = u32::try_from(before.matches('\n').count() + 1).unwrap_or(u32::MAX);
            let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
            (line, u32::try_from(col).unwrap_or(u32::MAX), message)
        })
        .collect()
}

fn quoted(line: u32, col: u32) -> (u32, u32, String) {
    (
        line,
        col,
        "string values must be quoted, expected literal string".to_string(),
    )
}

#[test]
fn toml_reports_both_bad_values_of_an_inline_table() {
    let text = read_fixture("syntax/inline_two_bad_values.toml");

    assert_eq!(errors_of(&text), [quoted(9, 16), quoted(9, 23)]);
}

#[test]
fn toml_reports_both_bad_elements_of_an_array() {
    let text = read_fixture("syntax/array_two_bad_elements.toml");

    assert_eq!(errors_of(&text), [quoted(11, 16), quoted(11, 22)]);
}

#[test]
fn toml_reports_a_duplicate_key_and_a_bad_value_of_one_inline_table() {
    let text = read_fixture("syntax/duplicate_key_and_bad_value.toml");

    assert_eq!(
        errors_of(&text),
        [(8, 21, "duplicate key".to_string()), quoted(8, 34)]
    );
}

#[test]
fn toml_reports_an_unclosed_array_at_end_of_file_with_a_cascade_before_it() {
    let text = read_fixture("syntax/unclosed_array_then_tables.toml");

    let errors = errors_of(&text);

    // The independent error before the bracket is first.
    assert_eq!(errors[0], quoted(5, 13));
    // The unclosed-array error is the last one, at end of file (line 14 is the last line).
    let last = &errors[errors.len() - 1];
    assert_eq!(last.2, "unclosed array, expected `]`");
    assert_eq!(last.0, 14);
    // Everything between is follow-on noise: many errors, none of them the unclosed message.
    let cascade = &errors[1..errors.len() - 1];
    assert!(cascade.len() > 10, "cascade of {} errors", cascade.len());
    assert!(cascade.iter().all(|e| !e.2.starts_with("unclosed")));
    assert!(
        cascade.iter().all(|e| e.0 >= 6),
        "cascade starts after the bracket"
    );
}

#[test]
fn toml_locates_the_unclosed_array_in_the_tree_at_its_opening_bracket() {
    let text = read_fixture("syntax/unclosed_array_then_tables.toml");

    let (doc, _) = DeTable::parse_recoverable(&text);
    let tags = doc
        .get_ref()
        .get("tags")
        .unwrap_or_else(|| panic!("no `tags` in the partial tree"));

    assert!(matches!(tags.get_ref(), DeValue::Array(_)));
    let bracket = pos_of(&text, "[1, 2");
    assert_eq!(bracket, (6, 8));
    assert_eq!(tags.span().start, text.find("[1, 2").unwrap_or(usize::MAX));
}

#[test]
fn toml_reports_an_unclosed_inline_table_at_end_of_file_with_a_cascade_before_it() {
    let text = read_fixture("syntax/unclosed_inline_table_then_tables.toml");

    let errors = errors_of(&text);

    assert_eq!(errors.len(), 4, "{errors:?}");
    assert_eq!(errors[3].2, "unclosed inline table, expected `}`");
    assert!(errors[..3].iter().all(|e| !e.2.starts_with("unclosed")));
}

#[test]
fn toml_locates_the_unclosed_inline_table_in_the_tree_at_its_opening_brace() {
    let text = read_fixture("syntax/unclosed_inline_table_then_tables.toml");

    let (doc, _) = DeTable::parse_recoverable(&text);
    let extra = doc
        .get_ref()
        .get("extra")
        .unwrap_or_else(|| panic!("no `extra` in the partial tree"));

    assert!(matches!(extra.get_ref(), DeValue::Table(_)));
    assert_eq!(
        extra.span().start,
        text.find("{ x = 1").unwrap_or(usize::MAX)
    );
}

#[test]
fn toml_keeps_a_recoverable_top_level_id_with_a_real_span_in_a_broken_file() {
    let text = read_fixture("syntax/broken_with_id/broken.toml");

    let (doc, errors) = DeTable::parse_recoverable(&text);

    assert_eq!(errors.len(), 1);
    let id = doc
        .get_ref()
        .get("id")
        .unwrap_or_else(|| panic!("no `id` in the partial tree"));
    assert_eq!(id.get_ref().as_str(), Some("syntax.parent"));
    assert_eq!(
        id.span().start,
        text.find("\"syntax.parent\"").unwrap_or(usize::MAX)
    );
}

#[test]
fn toml_puts_the_bad_token_in_place_of_an_unrecoverable_id() {
    let text = read_fixture("syntax/broken_without_id/broken.toml");

    let (doc, errors) = DeTable::parse_recoverable(&text);

    assert_eq!(errors.len(), 1);
    // The partial tree holds the junk token `$` as a string: it fails the ID grammar, so the id is
    // not recovered (ASM-054 rule 3).
    let id = doc
        .get_ref()
        .get("id")
        .unwrap_or_else(|| panic!("no `id` in the partial tree"));
    assert_eq!(id.get_ref().as_str(), Some("$"));
}

#[test]
fn toml_counts_columns_after_multibyte_text_in_bytes_so_the_reader_must_convert() {
    let text = "x = \"\u{e9}\" $\ny = 1\n";

    let (_, errors) = DeTable::parse_recoverable(text);

    // Byte offset 9; the reader reports column 9 (characters), not 10.
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span().map(|s| s.start), Some(9));
    assert_eq!(
        errors[0].message(),
        "unexpected key or value, expected newline, `#`"
    );
}

#[test]
fn toml_limits_nesting_depth_with_an_error_instead_of_overflowing_the_stack() {
    let deep = format!("a = {}1{}", "[".repeat(1000), "]".repeat(1000));

    let (_, errors) = DeTable::parse_recoverable(&deep);

    assert_ne!(errors.len(), 0);
    assert!(
        errors
            .iter()
            .any(|e| e.message().contains("max recursion depth")),
        "{:?}",
        errors
            .iter()
            .map(|e| e.message().to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn toml_reports_datetimes_and_non_finite_floats_as_values_so_the_reader_must_reject_them() {
    let text = "d = 1979-05-27\nn = nan\ni = inf\n";

    let (doc, errors) = DeTable::parse_recoverable(text);

    assert!(
        errors.is_empty(),
        "toml accepts them; the loader rejects them"
    );
    let get = |k: &str| doc.get_ref().get(k).map(toml::Spanned::get_ref);
    assert!(matches!(get("d"), Some(DeValue::Datetime(_))));
    assert!(matches!(get("n"), Some(DeValue::Float(_))));
    assert!(matches!(get("i"), Some(DeValue::Float(_))));
}
