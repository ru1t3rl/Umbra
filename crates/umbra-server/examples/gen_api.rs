//! Prints the contract documents as one JSON object `{"<file name>": "<rendered text>"}`, for
//! `cargo xtask gen-api` (REQ-X-4, REQ-X-6, gate 10, ADR 0025). Nothing is written here.

#![forbid(unsafe_code)]
#![expect(clippy::print_stdout, reason = "the output is the example's interface")]

use serde_json::{Map, Value};

fn main() -> Result<(), umbra_server::contract::ContractError> {
    let files: Map<String, Value> = umbra_server::contract::documents()?
        .files()?
        .into_iter()
        .map(|(name, text)| (name.to_string(), Value::String(text)))
        .collect();
    println!("{}", Value::Object(files));
    Ok(())
}
