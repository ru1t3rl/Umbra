//! `docs/api.md` against the committed contract (M5 WI-19, NFR-5, REQ-X-4).
//!
//! One rule, documented at the end of `docs/api.md`: in the section of a named `## ` heading, every
//! table row whose first cell is a single code span is read. The sets read from the page must equal the
//! sets in the committed files, in both directions:
//! - `## Endpoints`: `METHOD /path` and the operation id against `api/openapi.json`;
//! - `## Error codes`: code and status against `ProblemCode::ALL`;
//! - `## Run states`: states against the `RunState` enum of `api/openapi.json`;
//! - `## Configuration reference`: `[table] key` against the leaves of `config.v1.schema.json`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests read known files")]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use umbra_api_types::ProblemCode;

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read(relative: &str) -> String {
    fs::read_to_string(repo_file(relative)).expect("read file")
}

fn json(relative: &str) -> Value {
    serde_json::from_str(&read(relative)).expect("parse JSON")
}

/// The rows of the `## heading` section whose first cell is one code span: `(code span, other cells)`.
fn rows(page: &str, heading: &str) -> Vec<(String, Vec<String>)> {
    let wanted = format!("## {heading}");
    let mut in_section = false;
    let mut out = Vec::new();
    for line in page.lines() {
        if line.starts_with("## ") {
            in_section = line.trim_end() == wanted;
            continue;
        }
        if !in_section || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let first = cells[0];
        let Some(span) = first
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
            .filter(|span| !span.contains('`'))
        else {
            continue;
        };
        out.push((
            span.to_string(),
            cells[1..]
                .iter()
                .map(|c| c.trim_matches('`').to_string())
                .collect(),
        ));
    }
    out
}

/// `(METHOD path) -> operation id` of the page.
fn page_endpoints(page: &str) -> BTreeMap<String, String> {
    rows(page, "Endpoints")
        .into_iter()
        .map(|(span, cells)| (span, cells[0].clone()))
        .collect()
}

/// `(METHOD path) -> operation id` of the contract.
fn contract_endpoints(openapi: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (path, item) in openapi["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("path item") {
            out.insert(
                format!("{} {path}", method.to_uppercase()),
                operation["operationId"]
                    .as_str()
                    .expect("operationId")
                    .to_string(),
            );
        }
    }
    out
}

fn page_codes(page: &str) -> BTreeMap<String, u16> {
    rows(page, "Error codes")
        .into_iter()
        .map(|(code, cells)| (code, cells[0].parse().expect("status number")))
        .collect()
}

fn contract_codes() -> BTreeMap<String, u16> {
    ProblemCode::ALL
        .iter()
        .map(|code| (code.as_str().to_string(), code.status()))
        .collect()
}

fn page_states(page: &str) -> BTreeSet<String> {
    rows(page, "Run states")
        .into_iter()
        .map(|(state, _)| state)
        .collect()
}

fn contract_states(openapi: &Value) -> BTreeSet<String> {
    openapi["components"]["schemas"]["RunState"]["enum"]
        .as_array()
        .expect("RunState enum")
        .iter()
        .map(|v| v.as_str().expect("state").to_string())
        .collect()
}

fn page_keys(page: &str) -> BTreeSet<String> {
    rows(page, "Configuration reference")
        .into_iter()
        .map(|(span, _)| span)
        .collect()
}

/// Leaf keys of the config schema as the page spells them: `key` at the top level, `[a.b] key` below.
fn schema_keys(schema: &Value) -> BTreeSet<String> {
    fn walk(node: &Value, table: &str, out: &mut BTreeSet<String>) {
        let Some(props) = node["properties"].as_object() else {
            return;
        };
        for (key, child) in props {
            if child["type"] == "object" && child["properties"].is_object() {
                let next = if table.is_empty() {
                    key.clone()
                } else {
                    format!("{table}.{key}")
                };
                walk(child, &next, out);
            } else if table.is_empty() {
                out.insert(key.clone());
            } else {
                out.insert(format!("[{table}] {key}"));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(schema, "", &mut out);
    out
}

/// A readable two-way difference, empty when the sets are equal.
fn difference<T: Ord + std::fmt::Debug + Clone>(
    page: &BTreeSet<T>,
    contract: &BTreeSet<T>,
) -> String {
    let only_page: Vec<&T> = page.difference(contract).collect();
    let only_contract: Vec<&T> = contract.difference(page).collect();
    if only_page.is_empty() && only_contract.is_empty() {
        String::new()
    } else {
        format!("only in docs/api.md: {only_page:?}; only in the contract: {only_contract:?}")
    }
}

#[test]
fn every_endpoint_and_operation_id_matches_the_openapi_file_both_ways() {
    let page = page_endpoints(&read("docs/api.md"));
    let contract = contract_endpoints(&json("api/openapi.json"));
    assert!(
        contract.len() >= 20,
        "the contract lists its routes: {contract:?}"
    );
    assert_eq!(
        difference(
            &page.keys().cloned().collect(),
            &contract.keys().cloned().collect()
        ),
        "",
        "endpoints"
    );
    assert_eq!(page, contract, "operation ids");
}

#[test]
fn every_problem_code_and_status_matches_problem_code_all_both_ways() {
    let page = page_codes(&read("docs/api.md"));
    let contract = contract_codes();
    assert!(
        contract.len() >= 20,
        "the contract lists its codes: {contract:?}"
    );
    assert_eq!(
        difference(
            &page.keys().cloned().collect(),
            &contract.keys().cloned().collect()
        ),
        "",
        "codes"
    );
    assert_eq!(page, contract, "statuses");
}

#[test]
fn every_run_state_matches_the_openapi_enum_both_ways() {
    let page = page_states(&read("docs/api.md"));
    let contract = contract_states(&json("api/openapi.json"));
    assert_eq!(contract.len(), 5, "{contract:?}");
    assert_eq!(difference(&page, &contract), "", "run states");
}

#[test]
fn every_configuration_key_matches_the_config_schema_both_ways() {
    let page = page_keys(&read("docs/api.md"));
    let contract = schema_keys(&json(
        "crates/umbra-templates/schemas/config.v1.schema.json",
    ));
    assert!(
        contract.contains("[server.stream] max_connections"),
        "{contract:?}"
    );
    assert_eq!(difference(&page, &contract), "", "configuration keys");
}

#[test]
fn the_row_reader_ignores_other_sections_and_non_code_rows() {
    let page = "## A\n| `x` | 1 |\n| plain | 2 |\n## B\n| `y` | 2 |\n| `a` `b` | 3 |\n";
    let read_rows = rows(page, "A");
    assert_eq!(read_rows, vec![("x".to_string(), vec!["1".to_string()])]);
    assert_eq!(rows(page, "B").len(), 1);
    assert_eq!(rows(page, "C").len(), 0);
}
