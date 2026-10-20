//! `umbra-net` scaffold smoke test (M6 WI-1): the crate builds, is named as the rule table expects
//! and keeps `#![forbid(unsafe_code)]` (CLAUDE.md rule 5). Real contract tests arrive with WI-3.

#![forbid(unsafe_code)]

#[test]
fn given_the_crate_when_built_then_it_is_named_as_the_rule_table_expects() {
    assert_eq!(umbra_net::NAME, env!("CARGO_PKG_NAME"));
    assert_eq!(umbra_net::NAME, "umbra-net");
}

#[test]
fn given_the_crate_root_when_read_then_it_forbids_unsafe_code() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    let text = std::fs::read_to_string(root).expect("read lib.rs");
    assert!(text.contains("#![forbid(unsafe_code)]"));
}
