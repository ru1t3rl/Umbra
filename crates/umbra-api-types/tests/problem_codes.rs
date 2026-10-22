//! `ProblemCode` -> HTTP status equals the table of ADR 0026 (amended), one status per code.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::{BTreeMap, BTreeSet};

use umbra_api_types::ProblemCode;

/// The `| status | codes |` rows of the "Error codes" table in the ADR, as `code -> status`.
fn adr_table() -> BTreeMap<String, u16> {
    let path = format!(
        "{}/../../docs/adr/0026-control-api-v1-versioning-errors-and-run-states.md",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("ADR 0026 is readable");
    let section = text
        .split("**Error codes.**")
        .nth(1)
        .expect("the Error codes section exists");
    let mut table = BTreeMap::new();
    for line in section.lines().skip(1) {
        if line.starts_with("**axum rejections") {
            break;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // `| 400 | `bad_request` (...), `invalid_body` (...) |`
        let Some(status) = cells.get(1).and_then(|c| c.parse::<u16>().ok()) else {
            continue;
        };
        let codes = cells.get(2).expect("a codes cell");
        // Codes are the back-quoted snake_case words that start an item.
        for item in codes.split('`').skip(1).step_by(2) {
            if item.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !item.is_empty() {
                let previous = table.insert(item.to_string(), status);
                assert!(previous.is_none(), "{item} appears twice in the ADR");
            }
        }
    }
    table
}

/// The codes added by API 1.1.0 (minor bump, ADR 0026): `adapter_not_found` and `adapter_faulted`
/// from ADR 0035, `entity_remote` for the removal of a ghost (ASM-400, ASM-447).
const ADDED_IN_1_1: [(&str, u16); 3] = [
    ("adapter_not_found", 404),
    ("adapter_faulted", 409),
    ("entity_remote", 409),
];

#[test]
fn the_status_table_equals_adr_0026() {
    let adr = adr_table();
    assert_eq!(adr.len(), 27, "the ADR lists 27 codes: {adr:?}");
    let ours: BTreeMap<String, u16> = ProblemCode::ALL
        .iter()
        .filter(|c| !ADDED_IN_1_1.iter().any(|(code, _)| *code == c.as_str()))
        .map(|c| (c.as_str().to_string(), c.status()))
        .collect();
    assert_eq!(ours, adr);
}

#[test]
fn the_codes_added_in_1_1_have_their_documented_status() {
    for (code, status) in ADDED_IN_1_1 {
        let found = ProblemCode::ALL.iter().find(|c| c.as_str() == code);
        assert_eq!(found.map(|c| c.status()), Some(status), "{code}");
    }
}

#[test]
fn no_code_is_listed_twice_or_mapped_to_two_statuses() {
    let names: BTreeSet<&str> = ProblemCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(names.len(), ProblemCode::ALL.len(), "duplicate code in ALL");
    // `status` is a function of the code, so a second status per code cannot exist; check that the
    // statuses fall in the documented set and every documented status is used.
    let statuses: BTreeSet<u16> = ProblemCode::ALL.iter().map(|c| c.status()).collect();
    assert_eq!(
        statuses,
        BTreeSet::from([400, 403, 404, 405, 409, 413, 415, 422, 500, 503])
    );
}

#[test]
fn every_variant_is_in_all() {
    // The exhaustive match fails to compile when a variant is added; ALL must then list it too.
    for code in ProblemCode::ALL {
        let _: () = match code {
            ProblemCode::BadRequest
            | ProblemCode::InvalidBody
            | ProblemCode::UnsupportedEncoding
            | ProblemCode::PathNotAllowed
            | ProblemCode::HostNotAllowed
            | ProblemCode::OriginNotAllowed
            | ProblemCode::NotFound
            | ProblemCode::AdapterNotFound
            | ProblemCode::EntityNotFound
            | ProblemCode::TemplateNotFound
            | ProblemCode::MethodNotAllowed
            | ProblemCode::NoScenario
            | ProblemCode::InvalidState
            | ProblemCode::NotStepped
            | ProblemCode::StepInProgress
            | ProblemCode::StepCancelled
            | ProblemCode::DuplicateName
            | ProblemCode::EntityRemote
            | ProblemCode::AdapterFaulted
            | ProblemCode::PayloadTooLarge
            | ProblemCode::UnsupportedMediaType
            | ProblemCode::ValidationFailed
            | ProblemCode::InvalidId
            | ProblemCode::InvalidSpawn
            | ProblemCode::ScenarioInvalid
            | ProblemCode::ScenarioLoadFailed
            | ProblemCode::Internal
            | ProblemCode::EngineBusy
            | ProblemCode::EngineUnavailable
            | ProblemCode::TooManyConnections => {}
        };
    }
    assert_eq!(ProblemCode::ALL.len(), 30);
}
