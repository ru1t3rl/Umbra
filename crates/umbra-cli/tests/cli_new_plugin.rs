//! AT-20 (M7): `umbra new-plugin <kind> <dir> [--id <plugin id>] [--sdk-path <dir>]` (REQ-SDK-4, REQ-X-2,
//! ADR 0056, ASM-638, ASM-671), as real processes. The kinds are `behavior`, `motion_model`, `sensor`,
//! `condition`, `action` and `network_adapter`.
//!
//! Contract (ADR 0056 and the choices recorded in ASM-671): the command writes `Cargo.toml`,
//! `src/lib.rs`, `plugin.toml`, `README.md` and the contract test `tests/contract.rs` into `<dir>`
//! (created if missing); it refuses a directory that is not empty (exit 1, nothing written or
//! changed); an unknown kind is a usage error (exit 2). The generated `Cargo.toml` is its own
//! workspace (`[workspace]`), depends on `umbra-sdk` only, by the `--sdk-path` directory when given,
//! and builds `cdylib` + `rlib`; `src/lib.rs` starts with `#![forbid(unsafe_code)]`; `plugin.toml`
//! carries `--id` (default `example.plugin.<directory name>`, dashes as underscores), version `0.1.0`
//! and the ABI of the kind: 0.0 for `behavior`, 0.1 for `network_adapter`, 0.2 for the rest (ADR 0033).
//!
//! Step 4 of M7 (WI-2): ignored until WI-20. Until the command exists every row ends at "unrecognized
//! subcommand" or its exit code. The building rows are slow (a nested `cargo` of a crate with its
//! own target directory under `CARGO_TARGET_TMPDIR`, offline): the one-kind row is for gate 3, the
//! all-kinds row for the evidence run (ASM-638; WI-20 decides how it is marked).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{assert_usage_error, cli, stderr, stdout};

/// Every kind, with the ABI minor of its generated `plugin.toml`.
const KINDS: [(&str, &str); 6] = [
    ("behavior", "0.0"),
    ("motion_model", "0.2"),
    ("sensor", "0.2"),
    ("condition", "0.2"),
    ("action", "0.2"),
    ("network_adapter", "0.1"),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sdk_path() -> String {
    workspace_root()
        .join("crates/umbra-sdk")
        .to_string_lossy()
        .into_owned()
}

/// A fresh, not yet existing path `<tmp>/m7-new-plugin/<pid>-<n>-<tag>`.
fn scratch(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "m7-new-plugin/{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

fn generate(kind: &str, dir: &Path, extra: &[&str]) -> std::process::Output {
    let (dir, sdk) = (dir.to_string_lossy().into_owned(), sdk_path());
    let mut args = vec!["new-plugin", kind, dir.as_str(), "--sdk-path", sdk.as_str()];
    args.extend_from_slice(extra);
    cli(&args)
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The string value of the first top-level-or-section line `key = "value"` of a TOML text (the
/// generated files are simple enough for a line scan; a TOML parser would be a new dependency).
fn string_of(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
    })
}

/// The lines of the `[section]` table of a TOML text, up to the next table header.
fn section<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|l| l.trim() != format!("[{name}]"))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect()
}

/// REQ-SDK-4: each kind writes the documented files.
#[test]
#[ignore = "M7 WI-20"]
fn each_kind_generates_the_documented_files() {
    for (kind, _) in KINDS {
        let dir = scratch(kind);

        let out = generate(kind, &dir, &[]);

        assert_eq!(out.status.code(), Some(0), "{kind}: {}", stderr(&out));
        for file in [
            "Cargo.toml",
            "src/lib.rs",
            "plugin.toml",
            "README.md",
            "tests/contract.rs",
        ] {
            assert!(dir.join(file).is_file(), "{kind}: {file} is missing");
        }
    }
}

/// ADR 0056, ADR 0023: a third-party-style crate: the SDK only, by the given path, no unsafe code.
#[test]
#[ignore = "M7 WI-20"]
fn the_generated_crate_depends_on_the_sdk_only_and_forbids_unsafe_code() {
    for (kind, _) in KINDS {
        let dir = scratch(kind);
        assert_eq!(generate(kind, &dir, &[]).status.code(), Some(0), "{kind}");

        let manifest = read(&dir, "Cargo.toml");

        let deps = section(&manifest, "dependencies");
        assert_eq!(deps.len(), 1, "{kind}: {deps:?}");
        assert!(
            deps[0].starts_with("umbra-sdk") && deps[0].contains(&sdk_path()),
            "{kind}: {deps:?}"
        );
        assert!(
            manifest.lines().any(|l| l.trim() == "[workspace]"),
            "{kind}: its own workspace"
        );
        let lib = read(&dir, "src/lib.rs");
        assert!(
            lib.lines().any(|l| l.trim() == "#![forbid(unsafe_code)]"),
            "{kind}: lib.rs forbids unsafe code"
        );
        assert!(lib.contains("export_plugin!"), "{kind}");
    }
}

/// ADR 0033: the manifest names the minor that defines the kind, and `--id` is used.
#[test]
#[ignore = "M7 WI-20"]
fn the_generated_manifest_has_the_id_and_the_abi_of_the_kind() {
    for (kind, abi) in KINDS {
        let dir = scratch(kind);

        let out = generate(kind, &dir, &["--id", "acme.plugin.demo"]);

        assert_eq!(out.status.code(), Some(0), "{kind}: {}", stderr(&out));
        let manifest = read(&dir, "plugin.toml");
        let field = |key| string_of(&manifest, key);
        assert_eq!(field("id").as_deref(), Some("acme.plugin.demo"), "{kind}");
        assert_eq!(field("version").as_deref(), Some("0.1.0"), "{kind}");
        assert_eq!(field("abi").as_deref(), Some(abi), "{kind}");
        assert_eq!(field("schema_version").as_deref(), Some("1"), "{kind}");
    }
}

#[test]
#[ignore = "M7 WI-20"]
fn without_id_the_plugin_id_comes_from_the_directory_name() {
    let dir = scratch("my-demo-plugin");

    let out = generate("sensor", &dir, &[]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        string_of(&read(&dir, "plugin.toml"), "id").as_deref(),
        Some("example.plugin.my_demo_plugin")
    );
}

/// The command refuses to write into a directory that has content, exit 1, and changes nothing.
#[test]
#[ignore = "M7 WI-20"]
fn a_non_empty_directory_is_refused_and_left_unchanged() {
    let dir = scratch("occupied");
    std::fs::create_dir_all(&dir).expect("create the directory");
    std::fs::write(dir.join("notes.txt"), "mine").expect("write a file");

    let out = generate("behavior", &dir, &[]);

    assert_eq!(out.status.code(), Some(1), "stdout: {}", stdout(&out));
    assert!(
        stderr(&out).contains(&*dir.to_string_lossy()),
        "names the directory: {}",
        stderr(&out)
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .expect("read the directory")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["notes.txt"]);
    assert_eq!(read(&dir, "notes.txt"), "mine");
}

#[test]
#[ignore = "M7 WI-20"]
fn an_empty_existing_directory_is_accepted() {
    let dir = scratch("empty");
    std::fs::create_dir_all(&dir).expect("create the directory");

    let out = generate("action", &dir, &[]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(dir.join("src/lib.rs").is_file());
}

#[test]
#[ignore = "M7 WI-20"]
fn an_unknown_kind_is_a_usage_error() {
    let dir = scratch("unknown-kind");

    let out = generate("component", &dir, &[]);

    assert_usage_error(&out, "component");
    assert!(!dir.exists(), "nothing was written");
}

/// Runs `cargo test --offline` in the generated crate with a target directory of its own, with the
/// workspace lock file next to it so that offline resolution picks versions the cache has.
fn cargo_test_in(dir: &Path) -> std::process::Output {
    std::fs::copy(workspace_root().join("Cargo.lock"), dir.join("Cargo.lock"))
        .expect("copy the lock file");
    Command::new(env!("CARGO"))
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .args(["test", "--offline"])
        .output()
        .expect("run cargo")
}

fn assert_generated_crate_passes(kind: &str) {
    let dir = scratch(&format!("build-{kind}"));
    let generated = generate(kind, &dir, &[]);
    assert_eq!(
        generated.status.code(),
        Some(0),
        "{kind}: {}",
        stderr(&generated)
    );

    let built = cargo_test_in(&dir);

    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(built.status.success(), "{kind}:\n{output}");
    assert!(output.contains("test result: ok"), "{kind}:\n{output}");
}

/// ADR 0056: the generated crate builds offline against the workspace SDK and its contract test
/// passes. One kind, the cheapest, for gate 3.
#[test]
#[ignore = "M7 WI-20"]
fn the_generated_behavior_crate_builds_offline_and_its_contract_test_passes() {
    assert_generated_crate_passes("behavior");
}

/// The same for every kind (the evidence run, ASM-638).
#[test]
#[ignore = "M7 WI-20"]
fn every_generated_crate_builds_offline_and_its_contract_test_passes() {
    for (kind, _) in KINDS {
        assert_generated_crate_passes(kind);
    }
}
