//! AT-18 (CLI rows for `run`, `validate` and `list-plugins`, WI-3): the `umbra.toml` configuration
//! file. Requirements: REQ-X-2, REQ-T-2, REQ-P-10, ADR 0029, ASM-159, ASM-160, ASM-168.
//!
//! Every test runs in its own working directory under `CARGO_TARGET_TMPDIR`, never in the system
//! temporary directory. The serve rows belong to WI-17.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

#[path = "../../umbra-engine/tests/plugin_support/mod.rs"]
mod plugin_support;

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{example_dir, final_hash, json_of, stderr, stdout};

/// A fresh, empty working directory for one test.
fn workdir(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-config-{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Runs the binary with `dir` as working directory.
fn umbra(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbra"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run umbra")
}

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).expect("write");
}

/// Copies the example scenario into `dir`, without its `tick_rate_hz` line.
fn scenario_without_rate(dir: &Path) {
    let source = example_dir();
    fs::create_dir_all(dir.join("templates")).expect("mkdir");
    for entry in fs::read_dir(source.join("templates")).expect("templates") {
        let path = entry.expect("entry").path();
        let name = path.file_name().expect("name");
        fs::copy(&path, dir.join("templates").join(name)).expect("copy");
    }
    let text = fs::read_to_string(source.join("scenario.toml")).expect("scenario");
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.starts_with("tick_rate_hz"))
        .collect();
    write(dir, "scenario.toml", &(kept.join("\n") + "\n"));
}

fn notes(err: &str) -> Vec<&str> {
    err.lines().filter(|l| l.starts_with("note:")).collect()
}

#[test]
fn explicit_config_flag_is_used() {
    let dir = workdir("explicit");
    scenario_without_rate(&dir);
    write(&dir, "site.toml", "[simulation]\ntick_rate_hz = 20\n");

    let out = umbra(
        &dir,
        &[
            "--config",
            "site.toml",
            "run",
            "scenario.toml",
            "--ticks",
            "40",
            "--print-every",
            "40",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("t=2.000s"), "{}", stdout(&out));
    assert!(
        notes(&stderr(&out))
            .iter()
            .any(|n| n.contains("default of 20 Hz"))
    );
}

#[test]
fn missing_explicit_config_is_exit_1() {
    let dir = workdir("missing");
    scenario_without_rate(&dir);

    let out = umbra(&dir, &["run", "scenario.toml", "--config", "nope.toml"]);

    assert_eq!(out.status.code(), Some(1), "stdout: {}", stdout(&out));
    assert!(stderr(&out).contains("nope.toml"), "{}", stderr(&out));
    let out = umbra(
        &dir,
        &["validate", "scenario.toml", "--config", "nope.toml"],
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cwd_umbra_toml_is_discovered_with_a_stderr_note() {
    let dir = workdir("implicit");
    scenario_without_rate(&dir);
    write(&dir, "umbra.toml", "[simulation]\ntick_rate_hz = 20\n");

    let out = umbra(
        &dir,
        &[
            "run",
            "scenario.toml",
            "--ticks",
            "40",
            "--print-every",
            "40",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.lines()
            .any(|l| l == "note: using configuration ./umbra.toml"),
        "{err}"
    );
    assert!(stdout(&out).contains("t=2.000s"));
    assert!(
        !stdout(&out).contains("note:"),
        "the note must not reach stdout"
    );
    // Also with `--log-level off`, and for the other commands.
    let off = umbra(
        &dir,
        &["run", "scenario.toml", "--ticks", "1", "--log-level", "off"],
    );
    assert!(stderr(&off).contains("note: using configuration ./umbra.toml"));
    for args in [&["validate", "scenario.toml"][..], &["list-plugins"][..]] {
        let out = umbra(&dir, args);
        assert!(
            stderr(&out).contains("note: using configuration ./umbra.toml"),
            "{args:?}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn explicit_config_writes_no_implicit_note() {
    let dir = workdir("no-note");
    scenario_without_rate(&dir);
    write(&dir, "umbra.toml", "[simulation]\ntick_rate_hz = 20\n");
    write(&dir, "other.toml", "[simulation]\ntick_rate_hz = 10\n");

    let out = umbra(
        &dir,
        &[
            "run",
            "scenario.toml",
            "--ticks",
            "10",
            "--print-every",
            "10",
            "--config",
            "other.toml",
        ],
    );

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("using configuration"),
        "{}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("t=1.000s"),
        "the explicit file wins: {}",
        stdout(&out)
    );
}

#[test]
fn no_file_means_defaults() {
    let dir = workdir("none");
    scenario_without_rate(&dir);

    let out = umbra(&dir, &["run", "scenario.toml", "--ticks", "120"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert!(!stderr(&out).contains("using configuration"));
    assert!(
        notes(&stderr(&out))
            .iter()
            .any(|n| n.contains("default of 60 Hz"))
    );
    assert!(stdout(&out).contains("t=2.000s"));
}

#[test]
fn invalid_config_is_exit_1_with_located_diagnostics() {
    let dir = workdir("invalid");
    scenario_without_rate(&dir);
    write(
        &dir,
        "umbra.toml",
        "colour = 1\n[simulation]\ntick_rate_hz = 0\n[log]\nlevel = \"x\"\n",
    );

    for args in [
        &["run", "scenario.toml"][..],
        &["validate", "scenario.toml"][..],
        &["list-plugins"][..],
    ] {
        let out = umbra(&dir, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let err = stderr(&out);
        assert!(err.contains("umbra.toml:1:1: error[unknown_key]"), "{err}");
        assert!(
            err.contains("umbra.toml:3:16: error[invalid_value]"),
            "{err}"
        );
        assert!(
            err.contains("umbra.toml:5:9: error[invalid_value]"),
            "{err}"
        );
        assert_eq!(stdout(&out), "", "{args:?}");
    }
}

#[test]
fn config_default_tick_rate_reaches_run_and_validate() {
    let dir = workdir("rate");
    scenario_without_rate(&dir);
    write(&dir, "umbra.toml", "[simulation]\ntick_rate_hz = 20\n");
    write(&dir, "empty.toml", "");

    let validate = umbra(&dir, &["validate", "scenario.toml", "--json"]);
    let run = umbra(&dir, &["run", "scenario.toml", "--print-every", "0"]);

    assert_eq!(
        validate.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&validate)
    );
    assert_eq!(run.status.code(), Some(0), "stderr: {}", stderr(&run));
    for out in [&validate, &run] {
        let err = stderr(out);
        let found = notes(&err);
        assert!(
            found.iter().any(|l| l.contains("default of 20 Hz")),
            "{found:?}"
        );
    }
    assert_eq!(json_of(&validate)["valid"], true, "{}", stdout(&validate));
    // A different configured default changes the run (the duration in ticks, hence the hash).
    write(&dir, "umbra.toml", "[simulation]\ntick_rate_hz = 40\n");
    let faster = umbra(&dir, &["run", "scenario.toml", "--print-every", "0"]);
    assert_eq!(faster.status.code(), Some(0), "stderr: {}", stderr(&faster));
    assert!(final_hash(&stdout(&run)).is_some());
    assert_ne!(final_hash(&stdout(&run)), final_hash(&stdout(&faster)));
    // An explicit scenario value always wins over the configuration.
    let explicit = umbra(
        &dir,
        &[
            "run",
            &common::example(),
            "--print-every",
            "0",
            "--config",
            "umbra.toml",
        ],
    );
    let plain = umbra(
        &dir,
        &[
            "run",
            &common::example(),
            "--print-every",
            "0",
            "--config",
            "empty.toml",
        ],
    );
    for out in [&explicit, &plain] {
        assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(out));
    }
    assert!(final_hash(&stdout(&explicit)).is_some());
    assert_eq!(final_hash(&stdout(&explicit)), final_hash(&stdout(&plain)));
}

#[test]
fn plugin_path_flag_replaces_config_paths() {
    let dir = workdir("plugins");
    let plugins = plugin_support::fixture(&[]);
    write(
        &dir,
        "umbra.toml",
        &format!("[plugins]\npaths = [{:?}]\n", plugins.display().to_string()),
    );
    fs::create_dir_all(dir.join("empty")).expect("mkdir");

    let from_config = umbra(&dir, &["list-plugins"]);
    let replaced = umbra(&dir, &["list-plugins", "--plugin-path", "empty"]);

    assert_eq!(
        from_config.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&from_config)
    );
    assert!(
        stdout(&from_config).contains("test.plugin.fixture"),
        "{}",
        stdout(&from_config)
    );
    assert_eq!(
        replaced.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&replaced)
    );
    assert_eq!(stdout(&replaced), "0 plugin(s)\n");
}

#[test]
fn relative_plugin_paths_resolve_against_the_config_directory() {
    let dir = workdir("plugins-rel");
    fs::create_dir_all(dir.join("conf/none")).expect("mkdir");
    write(
        &dir.join("conf"),
        "site.toml",
        "[plugins]\npaths = [\"none\"]\n",
    );

    // Run from the parent; the relative path must be found next to the file.
    let out = umbra(&dir, &["list-plugins", "--config", "conf/site.toml"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "0 plugin(s)\n");
}

#[test]
fn log_level_flag_beats_config() {
    let dir = workdir("log");
    scenario_without_rate(&dir);
    write(&dir, "info.toml", "[log]\nlevel = \"info\"\n");

    let from_config = umbra(
        &dir,
        &[
            "run",
            "scenario.toml",
            "--ticks",
            "1",
            "--config",
            "info.toml",
        ],
    );
    let flag = umbra(
        &dir,
        &[
            "run",
            "scenario.toml",
            "--ticks",
            "1",
            "--config",
            "info.toml",
            "--log-level",
            "off",
        ],
    );
    let default = umbra(&dir, &["run", "scenario.toml", "--ticks", "1"]);

    let marker = "info: using configuration info.toml";
    assert!(
        stderr(&from_config).contains(marker),
        "{}",
        stderr(&from_config)
    );
    assert!(!stderr(&flag).contains(marker), "{}", stderr(&flag));
    assert!(!stderr(&default).contains("info:"), "{}", stderr(&default));
}

#[test]
fn schema_config_prints_the_published_schema() {
    let dir = workdir("schema");

    let out = umbra(&dir, &["schema", "config"]);

    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let doc = json_of(&out);
    assert_eq!(doc["$id"], "urn:umbra:schema:config:v1");
    assert_eq!(doc["additionalProperties"], false);
    let published = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../umbra-templates/schemas/config.v1.schema.json"),
    )
    .expect("schema file");
    assert_eq!(stdout(&out).trim_end(), published.trim_end());
}

#[test]
fn schema_with_a_named_config_checks_the_file() {
    let dir = workdir("schema-config");
    write(&dir, "bad.toml", "colour = 1\n");
    write(&dir, "ok.toml", "");

    let missing = umbra(&dir, &["schema", "config", "--config", "nope.toml"]);
    let bad = umbra(&dir, &["schema", "config", "--config", "bad.toml"]);
    let ok = umbra(&dir, &["schema", "config", "--config", "ok.toml"]);

    assert_eq!(missing.status.code(), Some(1));
    assert!(
        stderr(&missing).contains("nope.toml"),
        "{}",
        stderr(&missing)
    );
    assert_eq!(bad.status.code(), Some(1));
    assert!(stderr(&bad).contains("bad.toml:1:1: error[unknown_key]"));
    assert_eq!(ok.status.code(), Some(0), "stderr: {}", stderr(&ok));
}

#[test]
fn implicit_config_at_info_level_writes_the_note_but_not_the_info_line() {
    let dir = workdir("implicit-info");
    scenario_without_rate(&dir);
    write(&dir, "umbra.toml", "");

    let out = umbra(
        &dir,
        &[
            "run",
            "scenario.toml",
            "--ticks",
            "1",
            "--log-level",
            "info",
        ],
    );

    let err = stderr(&out);
    assert!(
        err.contains("note: using configuration ./umbra.toml"),
        "{err}"
    );
    assert!(!err.contains("info: using configuration"), "{err}");
}
