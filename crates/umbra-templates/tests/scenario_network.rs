//! AT-15: the scenario key `[[network_adapters]]`. Requirements: REQ-T-2, REQ-T-4, REQ-T-5,
//! ADR 0010, ADR 0030, ASM-227, ASM-271.
//!
//! Written in step 4 of M6, run against the reader of WI-8. Diagnostics follow the existing M2
//! conventions: a value problem is located at the value. The warning channel (ASM-290) is
//! `Scenario::warnings` and `ValidationReport::warnings`.

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{KeySpace, NetEntityType, NetworkAdapterConfig};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, Registry, Seconds};
use umbra_templates::{Scenario, Severity, SourceFile};

use common::{
    alpha_template, cid, errors, fixture_path, lines, loader, ok, pos_after, pos_of, pos_of_nth,
    registry, row, rows, src, tid,
};

const SCN: &str = "scn/scenario.toml";

/// A network adapter factory that only has an id and a schema (`port` is an integer).
struct Fac {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn NetworkAdapterConfig> for Fac {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        if params["port"] == 13 {
            return Err(BuildError::new("port 13 is reserved"));
        }
        Ok(Arc::new(Cfg))
    }
}

struct Cfg;

impl NetworkAdapterConfig for Cfg {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 1 }
    }
    fn start(
        &self,
        _: &umbra_core::net::AdapterStart,
    ) -> Result<Box<dyn umbra_core::net::NetworkAdapter>, umbra_core::net::NetError> {
        Err(umbra_core::net::NetError::Start("never started".to_owned()))
    }
}

#[expect(clippy::expect_used, reason = "test helper")]
fn registry_with_adapter() -> Registry {
    let (mut registry, _) = registry();
    let schema = r#"{"type":"object","properties":{"port":{"type":"integer"}},"additionalProperties":false,"examples":[{}]}"#;
    registry
        .register_network_adapter(Arc::new(Fac {
            id: cid("test.net.loopback"),
            schema: ParamSchema::from_json_str(schema).expect("schema"),
        }))
        .expect("register");
    registry
}

fn templates() -> Vec<SourceFile> {
    vec![
        src(
            "scn/templates/veh.toml",
            &alpha_template("veh.base", None, "2.0"),
        ),
        src(
            "scn/templates/tank.toml",
            &alpha_template("tank.mbt", Some("veh.base"), "3.0"),
        ),
    ]
}

fn base() -> String {
    lines(&[
        "schema_version = 1",
        "",
        "[scenario]",
        "name = \"test\"",
        "seed = 7",
        "tick_rate_hz = 30",
        "clock = \"afap\"",
        "duration_s = 10.0",
        "template_paths = [\"templates\"]",
        "",
        "[[entities]]",
        "template = \"veh.base\"",
        "position = { lat_deg = 52.0, lon_deg = 4.0 }",
        "heading_deg = 90.0",
    ])
}

fn with(extra: &[&str]) -> String {
    let mut text = base();
    text.push('\n');
    text.push_str(&lines(extra));
    text
}

/// One minimal entry, with `extra` lines appended to the `[[network_adapters]]` table.
fn entry(extra: &[&str]) -> String {
    let mut table = vec![
        "[[network_adapters]]",
        "adapter = \"test.net.loopback\"",
        "name = \"lb\"",
    ];
    table.extend_from_slice(extra);
    with(&table)
}

fn load(text: &str) -> Result<Scenario, umbra_templates::Diagnostics> {
    let registry = registry_with_adapter();
    loader(&registry).scenario_from_sources(&src(SCN, text), &templates())
}

fn failing(text: &str) -> Vec<common::Row> {
    rows(&errors(load(text)))
}

/// The column just after `key` on the (unique) line `line`: the value, not the earlier
/// `[scenario] name = ` that `pos_after(text, "name = ")` would find.
fn value_after_key(text: &str, line: &str, key: &str) -> (u32, u32) {
    let (l, c) = pos_of(text, line);
    (l, c + u32::try_from(key.len()).unwrap_or(u32::MAX))
}

#[test]
fn a_scenario_without_the_key_has_an_empty_adapter_list() {
    let scenario = ok(load(&base()));

    assert_eq!(scenario.network_adapters, vec![]);
}

#[test]
fn the_schema_version_is_unchanged() {
    assert_eq!(umbra_templates::SCHEMA_VERSION, 1);
}

/// FNV-1a 64 of `template.v1.schema.json` as of M5 (2152 bytes): M6 must not change it (ADR 0030).
/// To recompute after a deliberate change: fold the file's bytes with `h = (h ^ b) * 0x100000001b3`
/// from `h = 0xcbf29ce484222325` (wrapping), and update the length and hash below.
#[test]
fn the_template_file_schema_is_byte_identical_to_m5() {
    let hash = umbra_templates::TEMPLATE_FILE_SCHEMA
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });

    assert_eq!(
        (umbra_templates::TEMPLATE_FILE_SCHEMA.len(), hash),
        (2152, 0x5537_ea20_499f_506c)
    );
}

#[test]
fn the_scenario_file_schema_declares_the_optional_property() {
    let schema: serde_json::Value =
        serde_json::from_str(umbra_templates::SCENARIO_FILE_SCHEMA).expect("schema is json");

    assert!(schema["properties"]["network_adapters"].is_object());
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|k| k == "network_adapters")),
        "the property stays optional"
    );
}

#[test]
fn a_minimal_entry_gets_every_default() {
    let scenario = ok(load(&entry(&[])));

    assert_eq!(scenario.network_adapters.len(), 1);
    let spec = &scenario.network_adapters[0];
    assert_eq!(spec.adapter, cid("test.net.loopback"));
    assert_eq!(spec.name, "lb");
    assert!(spec.enabled);
    assert_eq!(spec.params, serde_json::json!({}));
    assert!((spec.publish.position_threshold_m - 1.0).abs() < f64::EPSILON);
    assert!((spec.publish.orientation_threshold.0 - 3.0_f64.to_radians()).abs() < 1e-12);
    assert_eq!(spec.publish.heartbeat, Seconds(5.0));
    assert_eq!(spec.publish.min_interval, Seconds(0.0));
    assert_eq!(spec.publish.model, DeadReckoning::Velocity);
    assert!(!spec.publish.control);
    assert_eq!(spec.ghosts.timeout, Seconds(12.0));
    assert_eq!(spec.ghosts.max, 10_000);
    assert_eq!(spec.ghosts.max_inbound_per_tick, 4096);
    assert_eq!(spec.ghosts.default_template, None);
    assert_eq!(spec.ghosts.type_map, vec![]);
}

#[test]
fn every_key_is_read() {
    let text = entry(&[
        "enabled = false",
        "params = { port = 7 }",
        "",
        "[network_adapters.publish]",
        "position_threshold_m = 2.5",
        "orientation_threshold_deg = 6.0",
        "heartbeat_s = 4.0",
        "min_interval_s = 0.5",
        "dead_reckoning = \"acceleration\"",
        "control = true",
        "",
        "[network_adapters.ghosts]",
        "timeout_s = 20.0",
        "max = 50",
        "max_inbound_per_tick = 100",
        "default_template = \"veh.base\"",
        "type_map = { \"1.1.225.1.1.1.0\" = \"tank.mbt\" }",
    ]);

    let scenario = ok(load(&text));

    let spec = &scenario.network_adapters[0];
    assert!(!spec.enabled);
    assert_eq!(spec.params, serde_json::json!({"port": 7}));
    assert!((spec.publish.position_threshold_m - 2.5).abs() < f64::EPSILON);
    assert!((spec.publish.orientation_threshold.0 - 6.0_f64.to_radians()).abs() < 1e-12);
    assert_eq!(spec.publish.heartbeat, Seconds(4.0));
    assert_eq!(spec.publish.min_interval, Seconds(0.5));
    assert_eq!(spec.publish.model, DeadReckoning::Acceleration);
    assert!(spec.publish.control);
    assert_eq!(spec.ghosts.timeout, Seconds(20.0));
    assert_eq!(spec.ghosts.max, 50);
    assert_eq!(spec.ghosts.max_inbound_per_tick, 100);
    assert_eq!(spec.ghosts.default_template, Some(tid("veh.base")));
    assert_eq!(
        spec.ghosts.type_map,
        vec![(NetEntityType::new("1.1.225.1.1.1.0"), tid("tank.mbt"))]
    );
}

#[test]
fn several_entries_keep_file_order() {
    let text = with(&[
        "[[network_adapters]]",
        "adapter = \"test.net.loopback\"",
        "name = \"zeta\"",
        "[[network_adapters]]",
        "adapter = \"test.net.loopback\"",
        "name = \"alpha\"",
    ]);

    let scenario = ok(load(&text));

    let names: Vec<&str> = scenario
        .network_adapters
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(names, ["zeta", "alpha"]);
}

#[test]
fn a_timeout_not_above_the_heartbeat_is_a_warning_not_an_error() {
    let text = entry(&[
        "[network_adapters.publish]",
        "heartbeat_s = 5.0",
        "[network_adapters.ghosts]",
        "timeout_s = 1.0",
    ]);

    let scenario = ok(load(&text));

    assert_eq!(scenario.network_adapters[0].ghosts.timeout, Seconds(1.0));
    // The warning is a `Severity::Warning` diagnostic on the scenario, at the value (ASM-290).
    assert_eq!(
        rows(&scenario.warnings),
        vec![row(
            "suspicious_value",
            SCN,
            pos_after(&text, "timeout_s = ")
        )]
    );
    assert_eq!(scenario.warnings[0].severity, Severity::Warning);
}

#[test]
fn unknown_keys_are_located_in_each_of_the_three_tables() {
    for (table, key) in [
        (None, "speed = 1"),
        (Some("[network_adapters.publish]"), "speed = 1"),
        (Some("[network_adapters.ghosts]"), "speed = 1"),
    ] {
        let mut extra = Vec::new();
        extra.extend(table);
        extra.push(key);
        let text = entry(&extra);

        assert_eq!(
            failing(&text),
            vec![row("unknown_key", SCN, pos_of(&text, "speed"))],
            "{table:?}"
        );
    }
}

#[test]
fn missing_adapter_or_name_is_missing_key_at_the_table() {
    let no_name = with(&["[[network_adapters]]", "adapter = \"test.net.loopback\""]);
    let no_adapter = with(&["[[network_adapters]]", "name = \"lb\""]);

    for text in [no_name, no_adapter] {
        // Located at the table header (the table's origin), like the other table-level rows.
        assert_eq!(
            failing(&text),
            vec![row(
                "missing_key",
                SCN,
                pos_of(&text, "[[network_adapters]]")
            )]
        );
    }
}

#[test]
fn a_duplicate_name_is_located_at_the_second_name() {
    let text = with(&[
        "[[network_adapters]]",
        "adapter = \"test.net.loopback\"",
        "name = \"lb\"",
        "[[network_adapters]]",
        "adapter = \"test.net.loopback\"",
        "name = \"lb\"",
    ]);

    assert_eq!(
        failing(&text),
        vec![row("duplicate_name", SCN, pos_of_nth(&text, "\"lb\"", 1))]
    );
}

#[test]
fn a_name_outside_the_grammar_is_invalid_value_at_the_value() {
    for bad in ["Dis", "1st", "a-b", ""] {
        let text = with(&[
            "[[network_adapters]]",
            "adapter = \"test.net.loopback\"",
            &format!("name = \"{bad}\""),
        ]);

        assert_eq!(
            failing(&text),
            vec![row(
                "invalid_value",
                SCN,
                value_after_key(&text, &format!("name = \"{bad}\""), "name = ")
            )],
            "{bad:?}"
        );
    }
}

#[test]
fn an_adapter_id_that_is_unknown_or_not_a_network_adapter_is_unknown_id() {
    for bad in ["test.net.missing", "stub.motion.alpha"] {
        let text = with(&[
            "[[network_adapters]]",
            &format!("adapter = \"{bad}\""),
            "name = \"lb\"",
        ]);

        assert_eq!(
            failing(&text),
            vec![row("unknown_id", SCN, pos_after(&text, "adapter = "))],
            "{bad}"
        );
    }
}

#[test]
fn params_that_are_not_a_table_are_wrong_type() {
    let text = entry(&["params = 3"]);

    assert_eq!(
        failing(&text),
        vec![row("wrong_type", SCN, pos_after(&text, "params = "))]
    );
}

#[test]
fn params_are_checked_against_the_adapters_schema() {
    let text = entry(&["params = { port = \"seven\" }"]);

    assert_eq!(
        failing(&text),
        vec![row("param_wrong_type", SCN, pos_of(&text, "\"seven\""))]
    );
}

#[test]
fn numbers_out_of_range_are_invalid_value_at_the_value() {
    let cases = [
        ("[network_adapters.publish]", "position_threshold_m = 0.0"),
        ("[network_adapters.publish]", "position_threshold_m = -1.0"),
        (
            "[network_adapters.publish]",
            "orientation_threshold_deg = 0.0",
        ),
        ("[network_adapters.publish]", "heartbeat_s = 0.0"),
        ("[network_adapters.publish]", "min_interval_s = -0.5"),
        ("[network_adapters.ghosts]", "timeout_s = 0.0"),
        ("[network_adapters.ghosts]", "max = 0"),
        ("[network_adapters.ghosts]", "max_inbound_per_tick = 0"),
    ];
    for (table, line) in cases {
        let text = entry(&[table, line]);
        let key = line.split('=').next().expect("key").trim();

        assert_eq!(
            failing(&text),
            vec![row(
                "invalid_value",
                SCN,
                pos_after(&text, &format!("{key} = "))
            )],
            "{line}"
        );
    }
}

#[test]
fn a_dead_reckoning_model_that_is_not_published_is_invalid_value() {
    for bad in ["velocity_rotation", "fast"] {
        let text = entry(&[
            "[network_adapters.publish]",
            &format!("dead_reckoning = \"{bad}\""),
        ]);

        assert_eq!(
            failing(&text),
            vec![row(
                "invalid_value",
                SCN,
                pos_after(&text, "dead_reckoning = ")
            )],
            "{bad}"
        );
    }
}

#[test]
fn unknown_template_ids_in_type_map_and_default_template_are_unknown_template() {
    let in_map = entry(&[
        "[network_adapters.ghosts]",
        "type_map = { \"1.1.1.1.1.1.1\" = \"no.such\" }",
    ]);
    let default = entry(&[
        "[network_adapters.ghosts]",
        "default_template = \"no.such\"",
    ]);

    assert_eq!(
        failing(&in_map),
        vec![row(
            "unknown_template",
            SCN,
            pos_of_nth(&in_map, "\"no.such\"", 0)
        )]
    );
    assert_eq!(
        failing(&default),
        vec![row(
            "unknown_template",
            SCN,
            pos_after(&default, "default_template = ")
        )]
    );
}

#[test]
fn wrong_typed_values_are_wrong_type_at_the_value() {
    for (table, line, key) in [
        (None, "name = 5", "name = "),
        (None, "enabled = \"yes\"", "enabled = "),
        (
            Some("[network_adapters.publish]"),
            "control = 1",
            "control = ",
        ),
        (Some("[network_adapters.ghosts]"), "max = 2.5", "max = "),
    ] {
        let mut extra = Vec::new();
        extra.extend(table);
        extra.push(line);
        let mut text = entry(&extra);
        if line.starts_with("name") {
            text = text.replacen("name = \"lb\"\n", "", 1);
        }

        assert_eq!(
            failing(&text),
            vec![row("wrong_type", SCN, value_after_key(&text, line, key))],
            "{line}"
        );
    }
}

#[test]
fn a_timeout_above_the_heartbeat_gives_no_warning() {
    let text = entry(&[
        "[network_adapters.publish]",
        "heartbeat_s = 5.0",
        "[network_adapters.ghosts]",
        "timeout_s = 5.5",
    ]);

    assert_eq!(ok(load(&text)).warnings, vec![]);
}

#[test]
fn a_timeout_equal_to_the_heartbeat_warns_and_names_the_written_heartbeat() {
    let text = entry(&[
        "[network_adapters.publish]",
        "heartbeat_s = 3",
        "[network_adapters.ghosts]",
        "timeout_s = 3",
    ]);

    let warnings = ok(load(&text)).warnings;

    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message.contains("`heartbeat_s` of 3"),
        "{}",
        warnings[0]
    );
}

#[test]
fn a_timeout_against_the_default_heartbeat_says_so() {
    let text = entry(&["[network_adapters.ghosts]", "timeout_s = 1.0"]);

    let warnings = ok(load(&text)).warnings;

    assert!(
        warnings[0].message.contains("default `heartbeat_s` of 5"),
        "{}",
        warnings[0]
    );
}

#[test]
fn a_warning_prints_as_a_warning_not_an_error() {
    let text = entry(&["[network_adapters.ghosts]", "timeout_s = 1.0"]);

    let shown = ok(load(&text)).warnings[0].to_string();

    assert!(shown.starts_with("scn/scenario.toml:"), "{shown}");
    assert!(shown.contains(": warning[suspicious_value]: "), "{shown}");
}

#[test]
fn errors_win_over_warnings_in_a_load() {
    let text = entry(&["[network_adapters.ghosts]", "timeout_s = 1.0", "max = 0"]);

    assert_eq!(
        failing(&text),
        vec![row("invalid_value", SCN, pos_after(&text, "max = "))]
    );
}

#[test]
fn params_absent_are_checked_as_an_empty_table() {
    // `port` is optional in the test schema, so an absent `params` loads as `{}`.
    let scenario = ok(load(&entry(&[])));

    assert_eq!(scenario.network_adapters[0].params, serde_json::json!({}));
}

#[test]
fn an_unknown_param_is_located_at_the_key() {
    let text = entry(&["params = { colour = 1 }"]);

    assert_eq!(
        failing(&text),
        vec![row("unknown_param", SCN, pos_of(&text, "colour"))]
    );
}

#[test]
fn a_type_map_template_that_is_defined_in_a_parent_chain_is_known() {
    let text = entry(&[
        "[network_adapters.ghosts]",
        "type_map = { \"9.9.9.9.9.9.9\" = \"tank.mbt\", \"1.1.1.1.1.1.1\" = \"veh.base\" }",
    ]);

    let scenario = ok(load(&text));

    // Ascending by token.
    let tokens: Vec<&str> = scenario.network_adapters[0]
        .ghosts
        .type_map
        .iter()
        .map(|(t, _)| t.as_str())
        .collect();
    assert_eq!(tokens, ["1.1.1.1.1.1.1", "9.9.9.9.9.9.9"]);
}

#[test]
fn the_scenario_file_schema_checks_the_network_entries() {
    let schema: serde_json::Value =
        serde_json::from_str(umbra_templates::SCENARIO_FILE_SCHEMA).expect("schema is json");
    let entry = &schema["$defs"]["network_adapter"];

    assert_eq!(entry["required"], serde_json::json!(["adapter", "name"]));
    assert_eq!(
        entry["properties"]["publish"]["properties"]["dead_reckoning"]["enum"],
        serde_json::json!(["static", "velocity", "acceleration"])
    );
    assert_eq!(entry["additionalProperties"], serde_json::json!(false));
}

fn validate_fixture(rel: &str) -> umbra_templates::ValidationReport {
    let registry = registry_with_adapter();
    loader(&registry).validate_path(&fixture_path(rel))
}

#[test]
fn validate_reports_the_warning_of_a_valid_scenario_file() {
    let report = validate_fixture("scenarios/network/warning/scenario.toml");

    assert_eq!(report.diagnostics, vec![]);
    assert_eq!(
        report
            .warnings
            .iter()
            .map(|w| (w.code.as_str(), w.location.as_ref().map(|l| l.line)))
            .collect::<Vec<_>>(),
        [("suspicious_value", Some(20))]
    );
}

#[test]
fn validate_reports_the_warning_for_a_directory_too() {
    let report = validate_fixture("scenarios/network");

    assert_eq!(report.diagnostics, vec![]);
    assert_eq!(report.warnings.len(), 1);
    let path = &report.warnings[0].location.as_ref().expect("located").path;
    assert!(
        path.ends_with("warning/scenario.toml"),
        "{}",
        path.display()
    );
}

#[test]
fn validate_of_a_clean_network_scenario_has_no_warnings() {
    let report = validate_fixture("scenarios/network/clean/scenario.toml");

    assert_eq!((report.diagnostics, report.warnings), (vec![], vec![]));
    assert_eq!(report.files_checked.len(), 2);
}

#[test]
fn a_factory_that_rejects_the_params_gives_param_invalid() {
    let text = entry(&["params = { port = 13 }"]);

    let found = failing(&text);

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "param_invalid");
    assert_eq!(found[0].1, std::path::PathBuf::from(SCN));
}

#[test]
fn an_empty_type_map_token_is_invalid_value_at_the_key() {
    let text = entry(&[
        "[network_adapters.ghosts]",
        "type_map = { \"\" = \"veh.base\" }",
    ]);

    assert_eq!(
        failing(&text),
        vec![row("invalid_value", SCN, pos_of(&text, "\"\" ="))]
    );
}

#[test]
fn network_adapters_that_is_not_an_array_of_tables_is_wrong_type() {
    // A top-level key must come before the first table.
    let scalar = base().replacen(
        "schema_version = 1\n",
        "schema_version = 1\nnetwork_adapters = 3\n",
        1,
    );
    let table = with(&["[network_adapters]", "adapter = \"test.net.loopback\""]);

    for (text, at) in [
        (&scalar, pos_after(&scalar, "network_adapters = ")),
        (&table, pos_of(&table, "[network_adapters]")),
    ] {
        let found = failing(text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, "wrong_type");
        assert_eq!((found[0].2, found[0].3), at, "{text}");
    }
}

#[test]
fn messages_name_the_key_the_user_wrote() {
    let default = entry(&["[network_adapters.ghosts]", "default_template = \"Bad Id\""]);
    let in_map = entry(&[
        "[network_adapters.ghosts]",
        "type_map = { \"1.1.1.1.1.1.1\" = \"Bad Id\" }",
    ]);

    let messages: Vec<String> = [default, in_map]
        .iter()
        .flat_map(|t| errors(load(t)))
        .map(|d| d.message)
        .collect();

    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(
        messages[0].contains("`default_template`"),
        "{}",
        messages[0]
    );
    assert!(
        messages[1].contains("`type_map.\"1.1.1.1.1.1.1\"`"),
        "{}",
        messages[1]
    );
}

#[test]
fn a_written_heartbeat_is_pointed_at_by_a_note_of_the_warning() {
    let text = entry(&[
        "[network_adapters.publish]",
        "heartbeat_s = 3",
        "[network_adapters.ghosts]",
        "timeout_s = 2",
    ]);

    let warning = &ok(load(&text)).warnings[0];

    assert_eq!(warning.notes.len(), 1);
    let at = warning.notes[0].location.as_ref().expect("located");
    assert_eq!((at.line, at.column), pos_after(&text, "heartbeat_s = "));
}

#[test]
fn the_schema_states_the_name_grammar_without_inventing_a_length_limit() {
    let schema: serde_json::Value =
        serde_json::from_str(umbra_templates::SCENARIO_FILE_SCHEMA).expect("schema is json");
    let name = &schema["$defs"]["network_adapter"]["properties"]["name"];

    assert_eq!(name["type"], "string");
    assert!(
        name.get("maxLength").is_none(),
        "the reader has no length limit"
    );
    assert!(
        name["description"]
            .as_str()
            .is_some_and(|d| d.contains("[a-z][a-z0-9_]*")),
        "{name}"
    );
}

#[test]
fn an_invalid_scenario_contributes_no_warnings_to_validate() {
    // One error next to a short timeout: errors win, in file and in directory mode. The scenario
    // itself is valid in `network_template_error` (the error is in its template set).
    for rel in [
        "scenarios/network_errors/scenario.toml",
        "scenarios/network_errors",
        "scenarios/network_template_error/scenario.toml",
        "scenarios/network_template_error",
    ] {
        let report = validate_fixture(rel);

        assert_eq!(
            report.diagnostics.len(),
            1,
            "{rel}: {:?}",
            report.diagnostics
        );
        assert!(
            ["invalid_value", "unknown_id"].contains(&report.diagnostics[0].code.as_str()),
            "{rel}"
        );
        assert_eq!(report.warnings, vec![], "{rel}");
    }
}
