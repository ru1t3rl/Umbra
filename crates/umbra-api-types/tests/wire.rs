//! Wire behavior of the `umbra-api-types` DTOs (D-12, ADR 0026, ADR 0027): serde round trips,
//! strict requests, the seed rules and the spellings clients depend on.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use umbra_api_types::*;

/// Serialises `value`, reads it back and asserts both the equality and that the JSON is stable.
fn round_trip<T>(value: &T) -> Value
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_value(value).expect("serialise");
    let back: T = serde_json::from_value(json.clone()).expect("deserialise");
    assert_eq!(&back, value);
    assert_eq!(serde_json::to_value(&back).expect("reserialise"), json);
    json
}

fn run_status() -> RunStatus {
    RunStatus {
        state: RunState::Running,
        stop_reason: None,
        fault: None,
        network_fault: None,
        clock: ClockSetting::Scaled { scale: 4.0 },
        session: 2,
        tick: Some(30),
        time_s: Some(1.0),
        step_pending: false,
        tick_rate_hz: Some(30),
        tick_rate_source: Some(TickRateSource::Scenario),
    }
}

fn summary() -> EntitySummary {
    EntitySummary {
        id: 7,
        name: Some("alpha".into()),
        template: Some("unit.walker".into()),
        group: None,
        force: Some("blue".into()),
        ownership: Ownership::Owned,
        remote_source: None,
        lat_deg: 52.0,
        lon_deg: 4.0,
        alt_m: 1.5,
        heading_deg: 90.0,
        speed_mps: 1.4,
        behavior: Some("behavior.patrol".into()),
        status: Some("running".into()),
        route: Some(RouteProgress { next: 1, len: 4 }),
    }
}

// ---- seed (D-c) ----

#[test]
fn seed_accepts_a_decimal_string_and_a_safe_integer() {
    let from_string: Seed = serde_json::from_str("\"42\"").expect("string");
    let from_integer: Seed = serde_json::from_str("42").expect("integer");
    assert_eq!(from_string, Seed(42));
    assert_eq!(from_integer, Seed(42));
    let top: Seed = serde_json::from_str("9007199254740991").expect("2^53 - 1");
    assert_eq!(top, Seed(9_007_199_254_740_991));
    assert_eq!(serde_json::from_str::<Seed>("0").expect("zero"), Seed(0));
}

#[test]
fn seed_integer_2_pow_53_is_rejected_but_the_same_value_as_a_string_is_accepted() {
    assert!(serde_json::from_str::<Seed>("9007199254740992").is_err());
    assert_eq!(
        serde_json::from_str::<Seed>("\"9007199254740992\"").expect("string"),
        Seed(9_007_199_254_740_992)
    );
}

#[test]
fn seed_u64_max_is_accepted_as_a_string_and_rejected_as_an_integer() {
    assert_eq!(
        serde_json::from_str::<Seed>("\"18446744073709551615\"").expect("string"),
        Seed(u64::MAX)
    );
    assert!(serde_json::from_str::<Seed>("18446744073709551615").is_err());
}

#[test]
fn seed_rejects_everything_else() {
    for bad in [
        "-1",
        "1.5",
        "1.0",
        "true",
        "null",
        "[]",
        "\"\"",
        "\"abc\"",
        "\"+5\"",
        "\"-5\"",
        "\" 5\"",
        "\"5 \"",
        "\"0x10\"",
        "\"1e3\"",
        "\"18446744073709551616\"",
        "\"000000000000000000001\"", // 21 digits
    ] {
        assert!(serde_json::from_str::<Seed>(bad).is_err(), "{bad}");
    }
    // 1 to 20 digits, leading zeros allowed (^[0-9]{1,20}$).
    assert_eq!(
        serde_json::from_str::<Seed>("\"00000000000000000007\"").expect("20 digits"),
        Seed(7)
    );
}

#[test]
fn seed_serialises_as_a_decimal_string() {
    assert_eq!(
        serde_json::to_string(&Seed(u64::MAX)).expect("serialise"),
        "\"18446744073709551615\""
    );
    assert_eq!(serde_json::to_string(&Seed(0)).expect("serialise"), "\"0\"");
}

#[test]
fn load_request_applies_the_seed_rules() {
    let ok: LoadScenarioRequest =
        serde_json::from_value(json!({"path": "a.toml", "seed": "18446744073709551615"}))
            .expect("string seed");
    assert_eq!(ok.seed, Some(Seed(u64::MAX)));
    let absent: LoadScenarioRequest =
        serde_json::from_value(json!({"path": "a.toml"})).expect("no seed");
    assert_eq!(absent.seed, None);
    assert_eq!(absent.clock, None);
    let too_big = serde_json::from_str::<LoadScenarioRequest>(
        r#"{"path": "a.toml", "seed": 9007199254740992}"#,
    );
    assert!(too_big.is_err());
}

// ---- strict requests ----

#[test]
fn requests_reject_unknown_fields() {
    fn rejects<T: DeserializeOwned + std::fmt::Debug>(valid: Value, name: &str) {
        let parsed: Result<T, _> = serde_json::from_value(valid.clone());
        assert!(
            parsed.is_ok(),
            "{name}: the valid form must parse: {parsed:?}"
        );
        let mut with_extra = valid;
        with_extra["surprise"] = json!(1);
        assert!(
            serde_json::from_value::<T>(with_extra).is_err(),
            "{name} must refuse an unknown field"
        );
    }
    rejects::<LoadScenarioRequest>(json!({"path": "a.toml"}), "LoadScenarioRequest");
    // A tick-rate field is not part of the load request (ADR 0026).
    assert!(
        serde_json::from_value::<LoadScenarioRequest>(
            json!({"path": "a.toml", "tick_rate_hz": 30})
        )
        .is_err()
    );
    rejects::<StepRequest>(json!({"count": 5}), "StepRequest");
    rejects::<SpawnRequest>(
        json!({"template": "unit.walker", "lat_deg": 1.0, "lon_deg": 2.0}),
        "SpawnRequest",
    );
    rejects::<ValidateGraphRequest>(json!({"document": "x"}), "ValidateGraphRequest");
    rejects::<ClockSetting>(json!({"mode": "afap"}), "ClockSetting");
    rejects::<ClockSetting>(
        json!({"mode": "scaled", "scale": 2.0}),
        "ClockSetting scaled",
    );
    rejects::<Subscription>(json!({"max_rate_hz": 10.0}), "Subscription");
    rejects::<StreamFilter>(json!({"entity_ids": [1]}), "StreamFilter");
    rejects::<GeoBox>(
        json!({"min_lat_deg": 0.0, "min_lon_deg": 0.0, "max_lat_deg": 1.0, "max_lon_deg": 1.0}),
        "GeoBox",
    );
}

#[test]
fn responses_tolerate_unknown_fields() {
    // Clients and tests must ignore additive response fields (ADR 0026 versioning).
    let mut value = serde_json::to_value(run_status()).expect("serialise");
    value["added_in_1_1"] = json!(true);
    let parsed: RunStatus = serde_json::from_value(value).expect("unknown field ignored");
    assert_eq!(parsed, run_status());
}

#[test]
fn spawn_request_defaults_altitude_heading_and_speed() {
    let spawn: SpawnRequest =
        serde_json::from_value(json!({"template": "unit.walker", "lat_deg": 52.0, "lon_deg": 4.0}))
            .expect("minimal spawn");
    assert_eq!(spawn.alt_m, 0.0);
    assert_eq!(spawn.heading_deg, 0.0);
    assert_eq!(spawn.speed_mps, 0.0);
    assert_eq!((spawn.name, spawn.group, spawn.force), (None, None, None));
    assert!(
        serde_json::from_value::<SpawnRequest>(json!({"template": "t", "lat_deg": 1.0})).is_err(),
        "lon_deg is required"
    );
}

// ---- spellings ----

#[test]
fn clock_setting_uses_the_scenario_file_spellings() {
    assert_eq!(round_trip(&ClockSetting::Afap), json!({"mode": "afap"}));
    assert_eq!(
        round_trip(&ClockSetting::Realtime),
        json!({"mode": "realtime"})
    );
    assert_eq!(
        round_trip(&ClockSetting::Stepped),
        json!({"mode": "stepped"})
    );
    assert_eq!(
        round_trip(&ClockSetting::Scaled { scale: 4.0 }),
        json!({"mode": "scaled", "scale": 4.0})
    );
    assert_eq!(
        ClockSetting::MODES,
        ["afap", "realtime", "stepped", "scaled"]
    );
    for bad in [
        json!({"mode": "scaled"}),
        json!({"mode": "afap", "scale": 2.0}),
        json!({"mode": "fast"}),
        json!({"mode": "AFAP"}),
        json!({"scale": 2.0}),
        json!("afap"),
    ] {
        assert!(
            serde_json::from_value::<ClockSetting>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn run_state_and_stop_reason_spellings() {
    for (state, text) in [
        (RunState::Idle, "idle"),
        (RunState::Ready, "ready"),
        (RunState::Running, "running"),
        (RunState::Paused, "paused"),
        (RunState::Stopped, "stopped"),
    ] {
        assert_eq!(round_trip(&state), json!(text));
    }
    for (reason, text) in [
        (StopReason::Command, "command"),
        (StopReason::Duration, "duration"),
        (StopReason::PluginFault, "plugin_fault"),
        (StopReason::Error, "error"),
    ] {
        assert_eq!(round_trip(&reason), json!(text));
    }
    assert_eq!(round_trip(&TickRateSource::Scenario), json!("scenario"));
    assert_eq!(round_trip(&TickRateSource::Default), json!("default"));
}

#[test]
fn state_hash_is_sixteen_lower_case_hex_digits() {
    assert_eq!(hex_state_hash(0), "0000000000000000");
    assert_eq!(hex_state_hash(0xABCD_EF01_2345_6789), "abcdef0123456789");
    assert_eq!(hex_state_hash(u64::MAX), "ffffffffffffffff");
}

// ---- round trips ----

#[test]
fn status_and_run_dtos_round_trip() {
    let status = run_status();
    let json = round_trip(&status);
    assert_eq!(json["state"], "running");
    assert_eq!(json["clock"], json!({"mode": "scaled", "scale": 4.0}));
    assert_eq!(json["tick"], 30);
    assert_eq!(json["tick_rate_source"], "scenario");
    // Absent optionals are left out, not null.
    let idle = RunStatus {
        state: RunState::Idle,
        stop_reason: None,
        fault: None,
        network_fault: None,
        clock: ClockSetting::Afap,
        session: 0,
        tick: None,
        time_s: None,
        step_pending: false,
        tick_rate_hz: None,
        tick_rate_source: None,
    };
    let idle_json = round_trip(&idle);
    for key in [
        "stop_reason",
        "fault",
        "tick",
        "time_s",
        "tick_rate_hz",
        "tick_rate_source",
    ] {
        assert!(
            idle_json.get(key).is_none(),
            "{key} must be omitted: {idle_json}"
        );
    }
    assert_eq!(idle_json["step_pending"], false);

    let stopped = RunStatus {
        state: RunState::Stopped,
        stop_reason: Some(StopReason::PluginFault),
        fault: Some(PluginFaultDto {
            plugin: "acme.wander".into(),
            capability: "acme.behavior.wander".into(),
            call: "tick_batch".into(),
            message: "bad intent".into(),
        }),
        ..status
    };
    let stopped_json = round_trip(&stopped);
    assert_eq!(stopped_json["fault"]["call"], "tick_batch");

    let scenario = ScenarioInfo {
        session: 2,
        name: "walking_skeleton".into(),
        path: "examples/walking_skeleton/scenario.toml".into(),
        seed: u64::MAX.to_string(),
        tick_rate_hz: 30,
        tick_rate_source: TickRateSource::Default,
        duration_ticks: 7200,
        entity_count: 3,
        warnings: Vec::new(),
    };
    let scenario_json = round_trip(&scenario);
    assert_eq!(scenario_json["seed"], "18446744073709551615");
    let engine = EngineStatus {
        api_version: API_VERSION.into(),
        run: stopped,
        scenario: Some(scenario),
    };
    round_trip(&engine);
    let idle_engine = EngineStatus {
        api_version: API_VERSION.into(),
        run: idle,
        scenario: None,
    };
    assert!(round_trip(&idle_engine).get("scenario").is_none());

    let step = StepResult {
        requested: 5,
        completed: true,
        reached: 5,
        time_s: 0.5,
        state_hash: hex_state_hash(0x1234),
        run: run_status(),
    };
    assert_eq!(round_trip(&step)["state_hash"], "0000000000001234");
    round_trip(&StepRequest {
        count: MAX_STEP_COUNT,
    });
    round_trip(&LoadScenarioRequest {
        path: "a.toml".into(),
        seed: Some(Seed(u64::MAX)),
        clock: Some(ClockSetting::Stepped),
    });
}

#[test]
fn step_count_range_is_one_to_one_million() {
    assert_eq!(MAX_STEP_COUNT, 1_000_000);
    assert!(!StepRequest { count: 0 }.in_range());
    assert!(StepRequest { count: 1 }.in_range());
    assert!(StepRequest { count: 1_000_000 }.in_range());
    assert!(!StepRequest { count: 1_000_001 }.in_range());
}

#[test]
fn entity_dtos_round_trip() {
    let json = round_trip(&summary());
    assert_eq!(json["id"], 7);
    assert_eq!(json["ownership"], "owned");
    assert!(json.get("group").is_none());
    assert_eq!(json["route"], json!({"next": 1, "len": 4}));
    let remote = EntitySummary {
        ownership: Ownership::Remote,
        ..summary()
    };
    assert_eq!(round_trip(&remote)["ownership"], "remote");
    round_trip(&EntityDetail {
        summary: summary(),
        motion_model: "builtin.motion.ground".into(),
        contacts: vec![1, 2, 9],
    });
    let page = EntityPage {
        tick: 12,
        total: 3,
        items: vec![summary()],
        next_after_id: None,
    };
    // The cursor is always present: null on the last page.
    assert_eq!(round_trip(&page)["next_after_id"], Value::Null);
    let more = EntityPage {
        next_after_id: Some(7),
        ..page
    };
    assert_eq!(round_trip(&more)["next_after_id"], 7);
    round_trip(&SpawnRequest {
        template: "unit.walker".into(),
        lat_deg: 52.0,
        lon_deg: 4.0,
        alt_m: 2.0,
        heading_deg: 45.0,
        speed_mps: 1.0,
        name: Some("bravo".into()),
        group: Some("g1".into()),
        force: Some("red".into()),
    });
}

#[test]
#[expect(clippy::too_many_lines, reason = "one table of DTO values")]
fn template_component_catalog_and_plugin_dtos_round_trip() {
    let reference = ComponentRef {
        id: "builtin.motion.ground".into(),
        params: json!({"max_speed_mps": 2.0, "nested": {"a": [1, 2]}}),
    };
    round_trip(&TemplateSummary {
        id: "unit.walker".into(),
        description: Some("a walker".into()),
        motion: "builtin.motion.ground".into(),
        behavior: None,
        source: "templates/walker.toml".into(),
    });
    let detail = TemplateDetail {
        id: "unit.walker".into(),
        chain: vec!["unit.walker".into(), "unit.base".into()],
        description: None,
        motion: reference.clone(),
        behavior: Some(reference.clone()),
        sensors: vec![reference],
        source: "templates/walker.toml".into(),
    };
    assert_eq!(
        round_trip(&detail)["motion"]["params"]["nested"]["a"],
        json!([1, 2])
    );
    let component = ComponentEntry {
        kind: ComponentKind::MotionModel,
        id: "builtin.motion.ground".into(),
        version: "1.0.0".into(),
        description: "ground".into(),
        provenance: Provenance::Builtin,
        params_schema: json!({"type": "object"}),
    };
    assert_eq!(round_trip(&component)["kind"], "motion_model");
    for (kind, text) in [
        (ComponentKind::Terrain, "terrain"),
        (ComponentKind::Behavior, "behavior"),
        (ComponentKind::Sensor, "sensor"),
        (ComponentKind::Condition, "condition"),
        (ComponentKind::Action, "action"),
    ] {
        assert_eq!(round_trip(&kind), json!(text));
    }
    for (p, text) in [(Provenance::File, "file"), (Provenance::Plugin, "plugin")] {
        assert_eq!(round_trip(&p), json!(text));
    }
    round_trip(&BehaviorCatalog {
        entries: vec![CatalogEntry {
            kind: CatalogKind::Behavior,
            id: "behavior.patrol".into(),
            description: "patrol".into(),
            provenance: Provenance::File,
            version: "0.1.0".into(),
            read_only: false,
            params_schema: json!({}),
            param_types: [("speed".to_string(), "number".to_string())].into(),
            outputs: std::collections::BTreeMap::default(),
            result: Some("success when done".into()),
        }],
    });
    for (kind, text) in [
        (CatalogKind::NodeType, "node_type"),
        (CatalogKind::Condition, "condition"),
        (CatalogKind::Action, "action"),
    ] {
        assert_eq!(round_trip(&kind), json!(text));
    }
    let diagnostic = Diagnostic {
        severity: Severity::Error,
        code: "unknown_component".into(),
        message: "no such action".into(),
        file: Some("graphs/patrol.toml".into()),
        line: Some(3),
        column: Some(5),
        notes: vec![DiagnosticNote {
            message: "defined here".into(),
            file: None,
            line: None,
            column: None,
        }],
    };
    round_trip(&ValidationResult {
        valid: false,
        diagnostics: vec![diagnostic],
    });
    round_trip(&ValidateGraphRequest {
        name: Some("patrol.toml".into()),
        document: "id = 'x'".into(),
    });
    round_trip(&PluginReport {
        plugins: vec![PluginDto {
            id: "acme.wander".into(),
            name: "Wander".into(),
            version: "1.0.0".into(),
            abi_major: 0,
            abi_minor: 0,
            origin: "libwander.so".into(),
            root: "plugins[0]".into(),
            capabilities: vec![CapabilityDto {
                kind: ComponentKind::Behavior,
                id: "acme.behavior.wander".into(),
                version: "1.0.0".into(),
                faulted: false,
            }],
        }],
        fault: None,
    });
}

// ---- problem details ----

#[test]
fn problem_carries_type_status_and_code_from_the_table() {
    let problem = Problem::new(ProblemCode::NoScenario, "load a scenario first");
    assert_eq!(problem.r#type, "urn:umbra:problem:no_scenario");
    assert_eq!(problem.status, 409);
    assert_eq!(problem.code, ProblemCode::NoScenario);
    assert_eq!(problem.title, ProblemCode::NoScenario.title());
    assert!(problem.diagnostics.is_none());
    let json = round_trip(&problem);
    assert_eq!(json["type"], "urn:umbra:problem:no_scenario");
    assert_eq!(json["code"], "no_scenario");
    assert_eq!(json["status"], 409);
    assert!(json.get("diagnostics").is_none());
    let with = problem.with_diagnostics(vec![Diagnostic {
        severity: Severity::Error,
        code: "io".into(),
        message: "cannot read".into(),
        file: None,
        line: None,
        column: None,
        notes: Vec::new(),
    }]);
    assert_eq!(round_trip(&with)["diagnostics"][0]["code"], "io");
}

#[test]
fn problem_code_text_matches_its_serde_spelling() {
    for code in ProblemCode::ALL {
        let text = code.as_str();
        assert_eq!(serde_json::to_value(code).expect("serialise"), json!(text));
        assert_eq!(
            serde_json::from_value::<ProblemCode>(json!(text)).expect("parse"),
            *code
        );
        assert_ne!(code.title(), "");
    }
}

// ---- stream messages (ADR 0027) ----

fn frame() -> Frame {
    Frame {
        kind: FrameKind::Snapshot,
        cause: FrameCause::Tick,
        session: 1,
        seq: 10,
        tick: 30,
        time_s: 1.0,
        run: run_status(),
        dropped: 2,
        entities: vec![summary()],
        events: vec![
            EventDto {
                tick: 30,
                kind: "fire".into(),
                entity_id: Some(7),
                target_id: Some(8),
                template: None,
                weapon: Some("gun".into()),
                detail: None,
            },
            EventDto {
                tick: 30,
                kind: "custom".into(),
                entity_id: None,
                target_id: None,
                template: None,
                weapon: None,
                detail: Some(json!({"x": 1})),
            },
        ],
        events_truncated: false,
        state_hash: Some(hex_state_hash(5)),
    }
}

#[test]
fn server_messages_are_tagged_by_type() {
    let hello = ServerMessage::Hello(Hello {
        protocol: STREAM_PROTOCOL.into(),
        api_version: API_VERSION.into(),
        encoding: "json".into(),
        settings: StreamSettings {
            default_rate_hz: 30.0,
            max_rate_hz: 60.0,
            queue_frames: 4,
            max_events_per_frame: 4096,
            send_timeout_s: 10.0,
            max_message_bytes: 65_536,
            max_entity_ids: 10_000,
        },
    });
    let json = round_trip(&hello);
    assert_eq!(json["type"], "hello");
    assert_eq!(json["protocol"], "umbra.stream.v1");
    assert_eq!(STREAM_PROTOCOL, "umbra.stream.v1");
    assert_eq!(json["settings"]["queue_frames"], 4);

    let frame_json = round_trip(&ServerMessage::Frame(frame()));
    assert_eq!(frame_json["type"], "frame");
    assert_eq!(frame_json["kind"], "snapshot");
    assert_eq!(frame_json["cause"], "tick");
    assert_eq!(frame_json["state_hash"], "0000000000000005");
    assert_eq!(frame_json["events"][0]["kind"], "fire");
    assert!(frame_json["events"][1].get("entity_id").is_none());

    let error = ServerMessage::Error(StreamError {
        code: StreamErrorCode::InvalidMessage,
        message: "not json".into(),
    });
    let error_json = round_trip(&error);
    assert_eq!(error_json["type"], "error");
    assert_eq!(error_json["code"], "invalid_message");
    assert_eq!(round_trip(&FrameCause::Control), json!("control"));
}

#[test]
fn client_subscribe_has_optional_filter_rate_and_hash() {
    let minimal: ClientMessage =
        serde_json::from_value(json!({"type": "subscribe"})).expect("minimal subscribe");
    let ClientMessage::Subscribe(sub) = minimal;
    assert_eq!(sub, Subscription::default());
    assert!(sub.filter.is_none() && sub.max_rate_hz.is_none() && sub.state_hash.is_none());

    let full = ClientMessage::Subscribe(Subscription {
        filter: Some(StreamFilter {
            bbox: Some(GeoBox {
                min_lat_deg: 51.0,
                min_lon_deg: 170.0,
                max_lat_deg: 53.0,
                max_lon_deg: -170.0,
            }),
            entity_ids: Some(vec![1, 2]),
            event_kinds: Some(vec!["fire".into()]),
        }),
        max_rate_hz: Some(10.0),
        state_hash: Some(true),
    });
    let json = round_trip(&full);
    assert_eq!(json["type"], "subscribe");
    assert_eq!(json["filter"]["bbox"]["max_lon_deg"], -170.0);
    assert!(
        serde_json::from_value::<ClientMessage>(json!({"type": "unsubscribe"})).is_err(),
        "there is no unsubscribe (ADR 0027)"
    );
    assert!(serde_json::from_value::<ClientMessage>(json!({"max_rate_hz": 1})).is_err());
    assert!(
        serde_json::from_value::<ClientMessage>(json!({"type": "subscribe", "extra": 1})).is_err()
    );
}

#[test]
fn stream_message_accepts_either_direction() {
    let client: StreamMessage =
        serde_json::from_value(json!({"type": "subscribe", "max_rate_hz": 5.0})).expect("client");
    assert!(matches!(client, StreamMessage::Client(_)));
    let server: StreamMessage = serde_json::from_value(
        serde_json::to_value(ServerMessage::Frame(frame())).expect("serialise"),
    )
    .expect("server");
    assert_eq!(server, StreamMessage::Server(ServerMessage::Frame(frame())));
    assert!(serde_json::from_value::<StreamMessage>(json!({"type": "nope"})).is_err());
}
