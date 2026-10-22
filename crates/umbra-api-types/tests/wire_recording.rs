//! AT-21: the recording status added in API 1.2.0 (M7 WI-3, ADR 0057). The field is absent unless a
//! recording exists, so a 1.1.0 client sees no change.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use serde_json::json;
use umbra_api_types::*;

fn idle() -> RunStatus {
    serde_json::from_value(json!({
        "state": "idle", "clock": {"mode": "afap"}, "session": 0, "step_pending": false
    }))
    .expect("a 1.1 status parses")
}

#[test]
fn the_api_version_is_1_2_0() {
    assert_eq!(API_VERSION, "1.2.0");
}

#[test]
fn a_status_without_a_recording_leaves_the_field_out() {
    let status = idle();
    assert!(status.recording.is_none());
    assert!(
        serde_json::to_value(&status)
            .expect("json")
            .get("recording")
            .is_none()
    );
}

#[test]
fn a_recording_round_trips_with_the_adr_field_names() {
    let mut status = idle();
    status.recording = Some(Box::new(RecordingDto {
        state: RecordingState::Recording,
        file: "session-3.umbrarec.jsonl".into(),
        records: 42,
        last_tick: 41,
    }));
    let json = serde_json::to_value(&status).expect("json");
    assert_eq!(
        json["recording"],
        json!({"state": "recording", "file": "session-3.umbrarec.jsonl", "records": 42, "last_tick": 41})
    );
    let back: RunStatus = serde_json::from_value(json).expect("parses");
    assert_eq!(back, status);
}

#[test]
fn the_recording_states_use_their_wire_spellings() {
    for (state, text) in [
        (RecordingState::Recording, "recording"),
        (RecordingState::Failed, "failed"),
        (RecordingState::Finished, "finished"),
    ] {
        assert_eq!(serde_json::to_value(state).expect("json"), json!(text));
    }
}
