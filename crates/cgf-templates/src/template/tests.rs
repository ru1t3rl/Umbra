//! Unit tests of the template file reader (AT-2 file rows, AT-4 structure, AT-5 template rows).
//! The same behaviour through the loader is covered by `tests/template_file.rs` once WI-7 lands.

use super::*;
use proptest::prelude::*;

const PATH: &str = "t.toml";
const MOTION: &str = "[motion]\nmodel = \"stub.motion.alpha\"\nparams = { a = 1.0 }\n";

fn read(text: &str) -> TemplateRead {
    read_template(&SourceFile::new(PATH, text))
}

/// `(code, line, column)` of every diagnostic.
fn rows(text: &str) -> Vec<(&'static str, u32, u32)> {
    read(text)
        .diagnostics
        .iter()
        .map(|d| {
            let (line, column) = d.location.as_ref().map_or((0, 0), |l| (l.line, l.column));
            (d.code.as_str(), line, column)
        })
        .collect()
}

fn with(head: &str) -> String {
    format!("{head}{MOTION}")
}

#[test]
fn a_valid_template_is_read_completely() {
    let text = "schema_version = 1\nid = \"tank.base\"\nextends = \"vehicle.base\"\n\
                description = \"d\"\n[motion]\nmodel = \"stub.motion.alpha\"\n\
                [motion.params]\na = 2.0\n[network.dis]\nentity_type = \"1.1\"\n\
                [[sensors]]\nkind = \"builtin.visual\"\n";

    let read = read(text);

    let t = read
        .template
        .unwrap_or_else(|| panic!("{:?}", read.diagnostics));
    assert_eq!(t.id.0.as_str(), "tank.base");
    assert_eq!(
        t.extends.map(|e| e.0.as_str().to_string()).as_deref(),
        Some("vehicle.base")
    );
    assert_eq!(t.description.as_deref(), Some("d"));
    assert_eq!(
        t.motion
            .and_then(|m| m.id)
            .map(|i| i.0.as_str().to_string())
            .as_deref(),
        Some("stub.motion.alpha")
    );
    assert_eq!(t.sensors.map(|s| s.len()), Some(1));
    assert_eq!(t.network.keys().collect::<Vec<_>>(), ["dis"]);
}

#[test]
fn network_tables_are_kept_verbatim_as_json() {
    let text = with("schema_version = 1\nid = \"a.b\"\n")
        + "[network.dis]\nmixed = [1, \"two\", 3.0]\n[network.dis.nested]\ndeep = { k = \"v\" }\n";

    let t = read(&text).template;

    let dis = t.and_then(|t| t.network.get("dis").map(LTable::to_param));
    assert_eq!(
        dis,
        Some(serde_json::json!({"mixed": [1, "two", 3.0], "nested": {"deep": {"k": "v"}}}))
    );
}

#[test]
fn network_entry_that_is_not_a_table_is_wrong_type_at_the_value() {
    let text = with("schema_version = 1\nid = \"a.b\"\n") + "[network]\ndis = 5\n";

    assert_eq!(rows(&text), [("wrong_type", 7, 7)]);
}

#[test]
fn network_that_is_not_a_table_is_wrong_type() {
    assert_eq!(
        rows("schema_version = 1\nid = \"a.b\"\nnetwork = 5\n"),
        [("wrong_type", 3, 11)]
    );
}

#[test]
fn bad_adapter_name_is_invalid_value_at_the_key() {
    let text = with("schema_version = 1\nid = \"a.b\"\n") + "[network.Bad-Name]\nk = 1\n";

    assert_eq!(rows(&text), [("invalid_value", 6, 10)]);
}

#[test]
fn newer_schema_version_stops_the_file_at_the_value() {
    let read = read("schema_version = 2\ncolour = \"red\"\nid = 5\nwhen = 1979-05-27\n");

    let codes: Vec<_> = read.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["unsupported_schema_version"]);
    assert_eq!(
        read.diagnostics[0]
            .location
            .as_ref()
            .map(|l| (l.line, l.column)),
        Some((1, 18))
    );
}

#[test]
fn schema_version_problems_are_reported_where_they_are() {
    let rest = "id = \"a.b\"\n";
    assert_eq!(
        rows(&with(&format!("schema_version = 0\n{rest}"))),
        [("invalid_value", 1, 18)]
    );
    assert_eq!(
        rows(&with(&format!("schema_version = -1\n{rest}"))),
        [("invalid_value", 1, 18)]
    );
    assert_eq!(
        rows(&with(&format!("schema_version = \"1\"\n{rest}"))),
        [("wrong_type", 1, 18)]
    );
    assert_eq!(
        rows(&with(&format!("schema_version = 1.0\n{rest}"))),
        [("wrong_type", 1, 18)]
    );
    assert_eq!(rows(&with(rest)), [("missing_key", 1, 1)]);
}

#[test]
fn unknown_keys_and_wrong_types_are_all_reported_in_position_order() {
    let text = "schema_version = 1\nid = \"tank.mbT\"\ncolour = \"red\"\ndescription = 5\n\
                [motion]\nmodel = 5\nspeed = 3\n";

    assert_eq!(
        rows(text),
        [
            ("invalid_id", 2, 14),
            ("unknown_key", 3, 1),
            ("wrong_type", 4, 15),
            ("wrong_type", 6, 9),
            ("unknown_key", 7, 1)
        ]
    );
}

#[test]
fn id_problems_are_located_in_the_string() {
    let ns = with("schema_version = 1\nid = \"tank\"\n");
    let empty = with("schema_version = 1\nid = \"\"\n");
    let bad_extends = with("schema_version = 1\nid = \"a.b\"\nextends = \"bad id\"\n");

    assert_eq!(rows(&ns), [("invalid_id", 2, 6)]);
    assert_eq!(rows(&empty), [("invalid_id", 2, 6)]);
    assert_eq!(rows(&bad_extends), [("invalid_id", 3, 15)]);
}

#[test]
fn missing_id_is_missing_key_and_the_template_is_not_returned() {
    let read = read(&with("schema_version = 1\n"));

    assert!(read.template.is_none());
    assert_eq!(read.diagnostics.len(), 1);
    assert_eq!(read.diagnostics[0].code, DiagnosticCode::MissingKey);
}

#[test]
fn a_sensor_without_kind_is_missing_key_at_the_entry() {
    let text = with("schema_version = 1\nid = \"a.b\"\n") + "[[sensors]]\nparams = { }\n";

    assert_eq!(rows(&text), [("missing_key", 6, 1)]);
}

#[test]
fn syntax_errors_stop_semantic_checks_but_recover_the_id() {
    let read = read("schema_version = 1\nid = \"a.b\"\ncolour = 1\nx = $\n");

    assert!(read.syntax_errors);
    assert_eq!(read.diagnostics.len(), 1);
    assert_eq!(
        read.known_id.map(|i| i.as_str().to_string()).as_deref(),
        Some("a.b")
    );
}

#[test]
fn an_unrecoverable_id_is_none() {
    let read = read("schema_version = 1\nid = $\n");

    assert!(read.syntax_errors);
    assert_eq!(read.known_id, None);
}

#[test]
fn datetime_in_the_network_section_is_the_only_error() {
    let text = with("schema_version = 1\nid = \"a.b\"\n") + "[network.dis]\nwhen = 1979-05-27\n";

    assert_eq!(rows(&text), [("unsupported_value", 7, 8)]);
}

/// m2: a value that was already reported as unsupported causes no follow-on error.
#[test]
fn unsupported_values_cause_no_follow_on_errors() {
    let huge = with("schema_version = 99999999999999999999\nid = \"a.b\"\n");
    let datetime = with("schema_version = 1\nid = 1979-05-27\n");
    let sensor = with("schema_version = 1\nid = \"a.b\"\nsensors = 1979-05-27\n");

    assert_eq!(rows(&huge), [("unsupported_value", 1, 18)]);
    assert_eq!(rows(&datetime), [("unsupported_value", 2, 6)]);
    assert_eq!(rows(&sensor), [("unsupported_value", 3, 11)]);
}

/// m7: a bad version does not hide other errors in the file.
#[test]
fn schema_version_zero_and_an_unknown_key_are_both_reported() {
    let text = with("schema_version = 0\nid = \"a.b\"\ncolour = 1\n");

    assert_eq!(
        rows(&text),
        [("invalid_value", 1, 18), ("unknown_key", 3, 1)]
    );
}

#[test]
fn sensors_entries_are_named_by_index_in_messages() {
    let text = with("schema_version = 1\nid = \"a.b\"\n") + "[[sensors]]\nparams = { }\n";

    let message = read(&text).diagnostics[0].message.clone();

    assert!(message.contains("sensors[0]"), "{message}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// NFR-2: no file content makes the reader panic.
    #[test]
    fn read_template_never_panics(text in "\\PC{0,300}") {
        let _ = read(&text);
    }

    #[test]
    fn read_template_never_panics_on_template_shaped_text(
        text in "(schema_version|id|extends|motion|model|params|network|sensors|kind|[a-z.]{1,6}|\"[a-zA-Z.]{0,8}\"|[0-9]{1,3}|[=\\[\\]{}\\n ,.-])*"
    ) {
        let read = read(&text);
        prop_assert!(read.template.is_none() || read.diagnostics.is_empty());
    }

    #[test]
    fn read_template_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
        let _ = read(&String::from_utf8_lossy(&bytes));
    }
}
