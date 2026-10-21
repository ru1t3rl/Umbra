//! AT-15: the scenario key `[[network_adapters]]`. Requirements: REQ-T-2, REQ-T-4, REQ-T-5,
//! ADR 0010, ADR 0030, ASM-227, ASM-271.
//!
//! Step 4 of M6: written before WI-8. The types exist (`NetworkAdapterSpec`, `PublishSettings`,
//! `GhostSettings`, `Scenario::network_adapters`) and the field is always empty until WI-8 adds the
//! reader, so the rows that need the reader are ignored with that work item. Diagnostics follow the
//! existing M2 conventions: a value problem is located at the value (`pos_after(text, "key = ")`).

#![forbid(unsafe_code)]

mod common;

use std::sync::Arc;

use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{KeySpace, NetEntityType, NetworkAdapterConfig};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, Registry, Seconds};
use umbra_templates::{Scenario, SourceFile};

use common::{
    alpha_template, cid, errors, lines, loader, ok, pos_after, pos_of, pos_of_nth, registry, row,
    rows, src, tid,
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
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
fn a_timeout_not_above_the_heartbeat_is_a_warning_not_an_error() {
    let text = entry(&[
        "[network_adapters.publish]",
        "heartbeat_s = 5.0",
        "[network_adapters.ghosts]",
        "timeout_s = 1.0",
    ]);

    let scenario = ok(load(&text));

    assert_eq!(scenario.network_adapters[0].ghosts.timeout, Seconds(1.0));
    // TODO(WI-8): the loader has no warning severity yet. WI-8 chooses the channel (a `warnings`
    // list on the scenario, or a severity on `Diagnostic` shown by `validate`) and extends this row
    // to assert the warning's code, path, line and column at `timeout_s`; ASM-282.
}

#[test]
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
fn a_name_outside_the_grammar_is_invalid_value_at_the_value() {
    for bad in ["Dis", "1st", "a-b", ""] {
        let text = with(&[
            "[[network_adapters]]",
            "adapter = \"test.net.loopback\"",
            &format!("name = \"{bad}\""),
        ]);

        assert_eq!(
            failing(&text),
            vec![row("invalid_value", SCN, pos_after(&text, "name = "))],
            "{bad:?}"
        );
    }
}

#[test]
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
fn params_that_are_not_a_table_are_wrong_type() {
    let text = entry(&["params = 3"]);

    assert_eq!(
        failing(&text),
        vec![row("wrong_type", SCN, pos_after(&text, "params = "))]
    );
}

#[test]
#[ignore = "WI-8"]
fn params_are_checked_against_the_adapters_schema() {
    let text = entry(&["params = { port = \"seven\" }"]);

    assert_eq!(
        failing(&text),
        vec![row("param_wrong_type", SCN, pos_of(&text, "\"seven\""))]
    );
}

#[test]
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
#[ignore = "WI-8"]
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
            vec![row("wrong_type", SCN, pos_after(&text, key))],
            "{line}"
        );
    }
}
