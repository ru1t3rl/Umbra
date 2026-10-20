//! `graph_document_id`: the lenient top-level `id` of a behavior graph document (M5 WI-5, REQ-X-11).

#![forbid(unsafe_code)]

use umbra_templates::{SourceFile, graph_document_id};

fn id(text: &str) -> Option<String> {
    graph_document_id(&SourceFile::new("doc.behavior.toml", text))
}

#[test]
fn plain_id_is_found() {
    assert_eq!(
        id("schema_version = 1\nid = \"a.b.c\"\nroot = \"n\"\n"),
        Some("a.b.c".to_owned())
    );
}

#[test]
fn quoted_key_and_literal_string_are_read_by_the_parser() {
    assert_eq!(id("\"id\" = 'a.b.c'\n"), Some("a.b.c".to_owned()));
}

#[test]
fn escapes_are_decoded() {
    assert_eq!(id("id = \"a\\u002eb\"\n"), Some("a.b".to_owned()));
}

#[test]
fn an_id_line_inside_a_multiline_string_is_not_the_id() {
    let text = "description = \"\"\"\nid = \"fake.id\"\n\"\"\"\nid = \"real.id\"\n";
    assert_eq!(id(text), Some("real.id".to_owned()));
}

#[test]
fn a_table_key_named_id_is_not_the_id() {
    assert_eq!(id("[nodes.a]\nid = \"nope\"\n"), None);
}

#[test]
fn no_id_or_a_non_string_id_is_none() {
    assert_eq!(id("schema_version = 1\n"), None);
    assert_eq!(id("id = 5\n"), None);
    assert_eq!(id(""), None);
}

#[test]
fn a_syntax_error_after_the_id_does_not_hide_it() {
    let text = "id = \"a.b.c\"\nroot = = broken\n";
    assert_eq!(id(text), Some("a.b.c".to_owned()));
}
