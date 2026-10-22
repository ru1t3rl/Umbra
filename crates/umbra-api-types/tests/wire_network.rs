//! Wire shapes added in API 1.1.0 (M6 WI-17, ADR 0035): the network adapter, the network fault,
//! `severity`, `remote_source` and `warnings`. Absent optionals are left out; a client that sends
//! none of the new fields is understood.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use serde_json::{Value, json};
use umbra_api_types::*;

fn adapter() -> AdapterDto {
    AdapterDto {
        name: "left".into(),
        adapter: "test.network.net_ok".into(),
        plugin: Some("test.plugin.fixture".into()),
        enabled: false,
        faulted: true,
        fault_reason: Some("send broke".into()),
        key_space: KeySpaceDto {
            base: 3000,
            count: 100,
        },
        ghosts: 2,
        counters: AdapterCountersDto {
            published: 1,
            received: 2,
            echo_dropped: 3,
            unmapped: 4,
            keys_exhausted: 5,
            ghosts_capped: 6,
            control_ignored: 7,
            world_errors: 8,
            unsupported: 9,
            invalid_state: 10,
            tx_dropped: 11,
            rx_dropped: 12,
            decode_errors: 13,
            io_errors: 14,
        },
    }
}

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) -> Value {
    let json = serde_json::to_value(value).expect("serialise");
    let back: T = serde_json::from_value(json.clone()).expect("deserialise");
    assert_eq!(&back, value);
    json
}

#[test]
fn an_adapter_round_trips_with_every_field_named_as_the_adr_says() {
    let json = round_trip(&adapter());

    assert_eq!(json["key_space"], json!({"base": 3000, "count": 100}));
    assert_eq!(json["plugin"], "test.plugin.fixture");
    assert_eq!(json["fault_reason"], "send broke");
    let counters = json["counters"].as_object().expect("counters");
    for name in [
        "published",
        "received",
        "echo_dropped",
        "unmapped",
        "keys_exhausted",
        "ghosts_capped",
        "tx_dropped",
        "rx_dropped",
        "decode_errors",
        "io_errors",
    ] {
        assert!(counters.contains_key(name), "ADR 0035 names {name}");
    }
    assert_eq!(counters.len(), 14, "{counters:?}");
}

#[test]
fn a_built_in_healthy_adapter_leaves_out_plugin_and_fault_reason() {
    let healthy = AdapterDto {
        plugin: None,
        faulted: false,
        fault_reason: None,
        enabled: true,
        ..adapter()
    };
    let json = round_trip(&healthy);
    assert!(json.get("plugin").is_none() && json.get("fault_reason").is_none());
}

#[test]
fn severity_defaults_to_error_when_absent_and_is_always_written() {
    let old: Diagnostic = serde_json::from_value(json!({"code": "io", "message": "m"}))
        .expect("a 1.0 diagnostic still parses");
    assert_eq!(old.severity, Severity::Error);
    let json = round_trip(&Diagnostic {
        severity: Severity::Warning,
        ..old
    });
    assert_eq!(json["severity"], "warning");
    assert_eq!(
        serde_json::to_value(Severity::Error).expect("json"),
        json!("error")
    );
}

#[test]
fn network_fault_and_the_new_stop_reason_use_their_wire_spellings() {
    assert_eq!(
        serde_json::to_value(StopReason::NetworkFault).expect("json"),
        json!("network_fault")
    );
    let fault = NetworkFaultDto {
        adapter: "left".into(),
        reason: "send broke".into(),
    };
    assert_eq!(
        round_trip(&fault),
        json!({"adapter": "left", "reason": "send broke"})
    );
    // A 1.0 status (no `network_fault`) still parses.
    let status: RunStatus = serde_json::from_value(json!({
        "state": "idle", "clock": {"mode": "afap"}, "session": 0, "step_pending": false
    }))
    .expect("a 1.0 status parses");
    assert!(status.network_fault.is_none());
    assert!(
        serde_json::to_value(&status)
            .expect("json")
            .get("network_fault")
            .is_none()
    );
}

#[test]
fn remote_source_and_warnings_are_left_out_when_empty() {
    let summary: EntitySummary = serde_json::from_value(json!({
        "id": 1, "ownership": "owned", "lat_deg": 0.0, "lon_deg": 0.0, "alt_m": 0.0,
        "heading_deg": 0.0, "speed_mps": 0.0
    }))
    .expect("a 1.0 summary parses");
    assert!(summary.remote_source.is_none());
    assert!(
        serde_json::to_value(&summary)
            .expect("json")
            .get("remote_source")
            .is_none()
    );
    let remote = EntitySummary {
        ownership: Ownership::Remote,
        remote_source: Some("left".into()),
        ..summary
    };
    assert_eq!(round_trip(&remote)["remote_source"], "left");

    let scenario: ScenarioInfo = serde_json::from_value(json!({
        "session": 1, "name": "n", "path": "p", "seed": "1", "tick_rate_hz": 30,
        "tick_rate_source": "scenario", "duration_ticks": 1, "entity_count": 0
    }))
    .expect("a 1.0 scenario parses");
    assert_eq!(scenario.warnings, Vec::<Diagnostic>::new());
    assert!(
        serde_json::to_value(&scenario)
            .expect("json")
            .get("warnings")
            .is_none()
    );
}
