//! Guards the real repo against source replacement and patches (review of 6d8a8a2, ASM-006).
//!
//! `check-deps` cannot see first-party code served through a replaced source: it keeps the
//! crates.io source id, so `cargo deny` does not flag it either. The repo therefore must not
//! configure `[source]`, `[patch]` or `[replace]` at all. Fixtures are exempt (they live under
//! `xtask/tests/fixtures/` with their own config).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn top_level_keys(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let table: toml::Table = toml::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid TOML: {e}", path.display()));
    table.keys().cloned().collect()
}

fn forbidden_in(path: &Path, forbidden: &[&str]) -> Vec<String> {
    top_level_keys(path)
        .into_iter()
        .filter(|k| forbidden.contains(&k.as_str()))
        .collect()
}

/// ASM-006: no source replacement or patch in the repo's cargo config.
#[test]
fn repo_cargo_config_has_no_source_or_patch() {
    for name in ["config.toml", "config"] {
        let path = repo_root().join(".cargo").join(name);
        let found = forbidden_in(&path, &["source", "patch"]);
        assert!(
            found.is_empty(),
            "{}: forbidden tables {found:?}",
            path.display()
        );
    }
}

/// ASM-006: no `[patch]` or `[replace]` in the root manifest.
#[test]
fn root_manifest_has_no_patch_or_replace() {
    let path = repo_root().join("Cargo.toml");
    assert!(
        path.exists(),
        "root manifest not found at {}",
        path.display()
    );
    let found = forbidden_in(&path, &["patch", "replace"]);
    assert!(
        found.is_empty(),
        "{}: forbidden tables {found:?}",
        path.display()
    );
}
