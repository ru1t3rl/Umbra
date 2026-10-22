//! The template keys the engine reads from `network.<adapter>` tables (ASM-228) and the list the SDK
//! offers adapters (`umbra_sdk::MANAGER_TEMPLATE_KEYS`, M6 WI-15 review) cannot drift apart. The
//! engine may not depend on the SDK, so the test reads the SDK's source for the constant.

#![forbid(unsafe_code)]

use std::path::Path;

#[test]
fn the_sdk_list_equals_the_keys_the_engine_reads() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../umbra-sdk/src/network.rs"),
    )
    .expect("read the SDK network module");
    let line = source
        .lines()
        .find(|l| l.contains("pub const MANAGER_TEMPLATE_KEYS"))
        .expect("the SDK declares the constant");
    let list = line
        .split_once("= &[")
        .and_then(|(_, r)| r.split_once(']'))
        .expect("a slice literal")
        .0;
    let sdk: Vec<&str> = list
        .split(',')
        .map(|k| k.trim().trim_matches('"'))
        .filter(|k| !k.is_empty())
        .collect();
    assert_eq!(sdk, umbra_engine::MANAGER_TEMPLATE_KEYS);
    assert_eq!(
        sdk,
        ["entity_type"],
        "a non-empty list: the scan found the keys"
    );
}
