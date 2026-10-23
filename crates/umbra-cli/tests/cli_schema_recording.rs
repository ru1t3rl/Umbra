//! M7 WI-11: `umbra schema recording` prints the committed recording schema, run as a real
//! process. Requirements: REQ-K-7, D-14, ADR 0053.

#![forbid(unsafe_code)]

mod common;

use common::{cli, json_of, stderr};

#[test]
fn schema_recording_prints_the_published_schema() {
    let out = cli(&["schema", "recording"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = json_of(&out);
    assert_eq!(doc["$id"], "urn:umbra:schema:recording:v1");
    let committed = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../umbra-record/schemas/recording.v1.schema.json"),
    )
    .unwrap_or_else(|e| panic!("schema file: {e}"));
    let committed: serde_json::Value =
        serde_json::from_str(&committed).unwrap_or_else(|e| panic!("schema json: {e}"));
    assert_eq!(doc, committed);
}
