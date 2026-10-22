//! AT-13 (WI-17): `umbra serve` as a process. Requirements: REQ-X-2, REQ-X-8, REQ-S-1, ADR 0026,
//! ADR 0029. Each test runs the built binary in its own working directory under
//! `CARGO_TARGET_TMPDIR`, with the working directory as the scenario root (the default) and an
//! example scenario copied into it. The server binds `127.0.0.1:0`; the test reads the
//! `listening on http://<addr>` line.
//!
//! `existing_commands_unchanged` pins the M2-M4 commands next to `serve`.

#![cfg(feature = "server")]
#![forbid(unsafe_code)]

mod common;
mod serve_support;

use serde_json::json;
use serve_support::{Served, Ws, WsIn, copy_example, run_to_exit, workdir, write_long_scenario};

const SKELETON: &str = "walking_skeleton/scenario.toml";

/// A working directory holding `walking_skeleton`.
fn skeleton_dir(tag: &str) -> std::path::PathBuf {
    let dir = workdir(tag);
    copy_example(&dir, "walking_skeleton");
    dir
}

#[test]
fn serve_prints_listening_address_and_answers_status() {
    let dir = skeleton_dir("listening");
    let mut served = Served::start(&dir, &[]);
    assert!(
        served
            .listening
            .starts_with("listening on http://127.0.0.1:"),
        "{}",
        served.listening
    );
    assert_ne!(served.addr.port(), 0, "the printed port is the bound one");
    let status = served.get("/api/v1/status").assert_json(200);
    assert_eq!(status["run"]["state"], "idle");
    assert_eq!(status["api_version"], "1.1.0");
    assert_eq!(served.terminate(), Some(0));
    assert_eq!(
        served.rest_of_stdout(),
        "",
        "exactly one stdout line: the listening line"
    );
}

#[test]
fn serve_binds_loopback_by_default() {
    // The default address is documented by the help text, and a flag-less start with a port-0 bind
    // from the config is loopback. (A real start on the default port 7878 would collide with other
    // processes; the default is checked in the help text instead, ASM-185.)
    let dir = skeleton_dir("default-bind");
    let help = run_to_exit(&dir, &["serve", "--help"]);
    assert_eq!(help.code, Some(0), "{}", help.stderr);
    assert!(help.stdout.contains("127.0.0.1:7878"), "{}", help.stdout);
    std::fs::write(dir.join("site.toml"), "[server]\nbind = \"127.0.0.1:0\"\n").expect("config");
    let mut served = Served::start_raw(&dir, &["--config", "site.toml", "serve"]);
    assert!(served.addr.ip().is_loopback());
    assert_eq!(served.terminate(), Some(0));
}

#[test]
fn serve_with_scenario_is_ready_and_with_start_is_running() {
    let dir = skeleton_dir("preload");
    let mut ready = Served::start(
        &dir,
        &["--scenario", SKELETON, "--clock", "stepped", "--seed", "5"],
    );
    let status = ready.get("/api/v1/status").assert_json(200);
    assert_eq!(status["run"]["state"], "ready");
    assert_eq!(status["run"]["clock"]["mode"], "stepped");
    let scenario = ready.get("/api/v1/scenario").assert_json(200);
    assert_eq!(scenario["name"], "walking_skeleton");
    assert_eq!(scenario["seed"], "5", "--seed replaces the scenario's seed");
    assert_eq!(scenario["path"], SKELETON);
    assert_eq!(ready.terminate(), Some(0));

    let mut running = Served::start(
        &dir,
        &["--scenario", SKELETON, "--clock", "stepped", "--start"],
    );
    let status = running.get("/api/v1/status").assert_json(200);
    assert_eq!(status["run"]["state"], "running");
    assert_eq!(running.terminate(), Some(0));
}

#[test]
fn serve_preload_outside_root_is_exit_1() {
    let dir = skeleton_dir("preload-outside");
    let outside =
        serve_support::repo_root().join("examples/scenarios/walking_skeleton/scenario.toml");
    let outside = outside.to_string_lossy().into_owned();
    for scenario in [outside.as_str(), "../elsewhere/scenario.toml"] {
        let out = run_to_exit(
            &dir,
            &["serve", "--bind", "127.0.0.1:0", "--scenario", scenario],
        );
        assert_eq!(out.code, Some(1), "{scenario}: {}", out.stderr);
        assert!(!out.stdout.contains("listening on"), "{}", out.stdout);
        assert!(!out.stderr.trim().is_empty(), "the reason is on stderr");
    }
}

#[test]
fn serve_preload_failure_is_exit_1() {
    let dir = skeleton_dir("preload-failure");
    let bad = serve_support::repo_root().join("crates/umbra-engine/tests/fixtures/bad_scenario");
    serve_support::copy_dir(&bad, &dir.join("bad"));
    for scenario in ["missing/scenario.toml", "bad/scenario.toml"] {
        let out = run_to_exit(
            &dir,
            &["serve", "--bind", "127.0.0.1:0", "--scenario", scenario],
        );
        assert_eq!(out.code, Some(1), "{scenario}: {}", out.stderr);
        assert!(!out.stdout.contains("listening on"), "{}", out.stdout);
    }
}

#[test]
fn non_loopback_bind_logs_a_warning() {
    let dir = skeleton_dir("non-loopback");
    let mut open = Served::start_raw(&dir, &["serve", "--bind", "0.0.0.0:0"]);
    assert_eq!(open.terminate(), Some(0));
    let stderr = open.stderr_text().to_lowercase();
    assert!(
        stderr.contains("warn") && stderr.contains("authentication"),
        "a warning that the API has no authentication: {stderr}"
    );
    // A loopback bind writes no such warning.
    let mut local = Served::start(&dir, &[]);
    assert_eq!(local.terminate(), Some(0));
    assert!(
        !local
            .stderr_text()
            .to_lowercase()
            .contains("authentication")
    );
}

#[cfg(unix)]
#[test]
fn sigterm_shuts_down_cleanly_exit_0() {
    let dir = skeleton_dir("sigterm");
    let mut served = Served::start(&dir, &["--scenario", SKELETON, "--start"]);
    served.get("/api/v1/status").assert_json(200);
    assert_eq!(served.terminate(), Some(0), "{}", served.stderr_text());
}

#[cfg(unix)]
#[test]
fn sigterm_with_open_stream_and_pending_step_exits_0() {
    let dir = workdir("sigterm-busy");
    let long = write_long_scenario(&dir);
    let mut served = Served::start(&dir, &["--scenario", long, "--clock", "stepped", "--start"]);
    let mut ws = Ws::connect(served.addr, &[]).unwrap_or_else(|r| panic!("{}", r.text()));
    ws.hello();
    ws.subscribe(json!({}));
    ws.frame_until(|f| f["cause"] == "control");

    // The largest step keeps a request pending while the engine works on it.
    let addr = served.addr;
    let step = std::thread::spawn(move || {
        serve_support::http_request(
            addr,
            "POST",
            "/api/v1/run/step",
            &[],
            Some(&json!({"count": 1_000_000}).to_string()),
        )
    });
    loop {
        let status = served.get("/api/v1/status").assert_json(200);
        if status["run"]["step_pending"] == true {
            break;
        }
        assert!(
            !step.is_finished(),
            "the step ended before it was seen pending"
        );
    }
    assert_eq!(served.terminate(), Some(0), "{}", served.stderr_text());

    let reply = step.join().expect("step thread");
    reply.assert_problem(503, "engine_unavailable");
    loop {
        match ws.recv() {
            WsIn::Close(code) => {
                assert_eq!(code, 1001);
                break;
            }
            WsIn::Json(_) => {}
            WsIn::Eof => panic!("the stream ended without a 1001 close"),
        }
    }
}

#[test]
fn existing_commands_unchanged() {
    // The M2-M4 commands print what they printed before `serve` existed.
    let run = common::cli(&["run", &common::example(), "--print-every", "0"]);
    assert_eq!(run.status.code(), Some(0), "{}", common::stderr(&run));
    assert_eq!(
        common::final_hash(&common::stdout(&run)).as_deref(),
        Some("849026656b7b42d2"),
        "the walking_skeleton golden hash"
    );
    let validate = common::cli(&["validate", &common::example()]);
    assert_eq!(
        validate.status.code(),
        Some(0),
        "{}",
        common::stderr(&validate)
    );
    let schema = common::cli(&["schema", "scenario"]);
    assert_eq!(schema.status.code(), Some(0));
    assert!(common::json_of(&schema).is_object());
}
