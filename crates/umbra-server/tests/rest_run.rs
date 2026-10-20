//! AT-6 (WI-11): scenario loading and run control over REST. Requirements: REQ-X-4, REQ-X-5,
//! REQ-X-9, ADR 0026 (amended: D-a, D-c, M10, m3). In-process through `Server::router()`.

#![forbid(unsafe_code)]

mod common;

use axum::http::Method;
use common::{Harness, copy_dir, write_long_scenario, write_scenario_variant};
use serde_json::json;
use umbra_api_types::{API_VERSION, ProblemCode};

const WS_PATH: &str = "walking_skeleton/scenario.toml";

#[tokio::test]
async fn status_when_idle() {
    let h = Harness::start("run-status-idle");
    let reply = h.get("/api/v1/status").await;
    let body = reply.assert_json(200);
    assert_eq!(body["api_version"], API_VERSION);
    assert_eq!(body["run"]["state"], "idle");
    assert_eq!(body["run"]["step_pending"], false);
    assert!(body.get("scenario").is_none_or(serde_json::Value::is_null));
}

#[tokio::test]
async fn load_returns_201_and_ready_with_tick_rate_source_and_seed_string() {
    let h = Harness::start("run-load");
    let body = h
        .load_body(&json!({"path": WS_PATH}))
        .await
        .assert_json(201);
    assert_eq!(body["name"], "walking_skeleton");
    assert_eq!(body["path"], WS_PATH);
    assert_eq!(body["seed"], json!("42"), "a response seed is a string");
    assert_eq!(body["tick_rate_hz"], 30);
    assert_eq!(body["tick_rate_source"], "scenario");
    assert_eq!(body["duration_ticks"], 7200);
    assert_eq!(body["entity_count"], 3);
    let status = h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(status["run"]["state"], "ready");
    assert_eq!(status["run"]["tick"], 0);
    assert_eq!(status["run"]["session"], body["session"]);
    assert_eq!(status["scenario"], body);
    let scenario = h.get("/api/v1/scenario").await.assert_json(200);
    assert_eq!(scenario, body);
}

#[tokio::test]
async fn seed_accepts_string_and_safe_integer() {
    let h = Harness::start("run-seed");
    // (request value, seed in the response)
    let cases = [
        (json!("7"), "7"),
        (json!(9_007_199_254_740_991_u64), "9007199254740991"),
        (json!("18446744073709551615"), "18446744073709551615"),
        (json!("007"), "7"),
    ];
    for (sent, expected) in cases {
        let body = h
            .load_body(&json!({"path": WS_PATH, "seed": sent}))
            .await
            .assert_json(201);
        assert_eq!(body["seed"], expected, "seed {sent}");
    }
}

#[tokio::test]
async fn seed_integer_above_2_pow_53_minus_1_is_422_validation_failed() {
    let h = Harness::start("run-seed-big");
    let reply = h
        .load_body(&json!({"path": WS_PATH, "seed": 9_007_199_254_740_992_u64}))
        .await;
    reply.assert_problem(ProblemCode::ValidationFailed);
}

#[tokio::test]
async fn load_relative_path_resolves_against_the_root() {
    let h = Harness::start("run-relative");
    // The process working directory is the crate directory, which has a `Cargo.toml`; a relative
    // path is looked up in the root, where there is none.
    h.load_body(&json!({"path": "Cargo.toml"}))
        .await
        .assert_problem(ProblemCode::ScenarioInvalid);
    let body = h
        .load_body(&json!({"path": "./walking_skeleton/../walking_skeleton/scenario.toml"}))
        .await
        .assert_json(201);
    assert_eq!(
        body["path"], WS_PATH,
        "the path is shown normalised, root relative"
    );
}

#[tokio::test]
async fn load_invalid_scenario_is_422_scenario_invalid_with_root_relative_paths() {
    let h = Harness::start("run-invalid");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../umbra-engine/tests/fixtures/bad_scenario");
    copy_dir(&fixture, &h.root.join("bad"));
    let problem = h
        .load_body(&json!({"path": "bad/scenario.toml"}))
        .await
        .assert_problem(ProblemCode::ScenarioInvalid);
    let diagnostics = problem.diagnostics.expect("located diagnostics");
    assert!(
        diagnostics.len() >= 2,
        "the fixture has two errors: {diagnostics:?}"
    );
    let root = h.root.to_string_lossy().into_owned();
    for diagnostic in &diagnostics {
        let file = diagnostic
            .file
            .as_deref()
            .expect("every diagnostic names a file");
        assert!(file.starts_with("bad/"), "root-relative: {file}");
        assert!(!file.contains(&root), "no absolute path: {file}");
    }
}

#[tokio::test]
async fn load_missing_plugin_dir_is_422_scenario_load_failed() {
    let h = Harness::start("run-missing-plugins");
    let path = write_scenario_variant(
        &h.root,
        "withplugins",
        "template_paths = [\"templates\"]",
        "template_paths = [\"templates\"]\nplugin_paths = [\"no_such_plugins\"]",
    );
    h.load_body(&json!({"path": path}))
        .await
        .assert_problem(ProblemCode::ScenarioLoadFailed);
}

#[tokio::test]
async fn load_failure_leaves_idle_and_old_session_gone() {
    let h = Harness::start("run-load-failure");
    let first = h.load_skeleton("stepped").await;
    assert_eq!(first["name"], "walking_skeleton");
    h.load_body(&json!({"path": "missing/scenario.toml"}))
        .await
        .assert_problem(ProblemCode::ScenarioInvalid);
    let status = h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(status["run"]["state"], "idle");
    h.get("/api/v1/scenario")
        .await
        .assert_problem(ProblemCode::NoScenario);
    h.entities("").await.assert_problem(ProblemCode::NoScenario);
}

#[tokio::test]
async fn load_request_with_tick_rate_field_is_422_validation_failed() {
    let h = Harness::start("run-tick-rate-field");
    h.load_body(&json!({"path": WS_PATH, "tick_rate_hz": 10}))
        .await
        .assert_problem(ProblemCode::ValidationFailed);
    let status = h.get("/api/v1/status").await.assert_json(200);
    assert_eq!(status["run"]["state"], "idle", "nothing was loaded");
}

/// What a command answers in a state: a success status, or a problem code.
#[derive(Clone, Copy, Debug)]
enum Expect {
    Ok(u16),
    Problem(ProblemCode),
}

use Expect::{Ok as Okay, Problem as Prob};
use ProblemCode::{InvalidState, NoScenario};

/// Drives a fresh stepped-clock session into `state`.
async fn reach(h: &Harness, state: &str) {
    if state != "idle" {
        h.load_skeleton("stepped").await;
    }
    for command in match state {
        "running" => &["start"][..],
        "paused" => &["start", "pause"][..],
        "stopped" => &["stop"][..],
        _ => &[][..],
    } {
        h.run_command(command).await.assert_json(200);
    }
}

/// Runs one command and returns its reply.
async fn apply(h: &Harness, command: &str) -> common::Reply {
    match command {
        "unload" => h.send(Method::DELETE, "/api/v1/scenario").await,
        "set_clock" => {
            h.send_json(
                Method::PUT,
                "/api/v1/run/clock",
                &json!({"mode": "realtime"}),
            )
            .await
        }
        other => h.run_command(other).await,
    }
}

#[tokio::test]
async fn run_control_follows_the_matrix() {
    // The ADR 0026 command x state table (the REST-reachable commands).
    let matrix: [(&str, [(&str, Expect); 6]); 5] = [
        (
            "idle",
            [
                ("start", Prob(NoScenario)),
                ("pause", Prob(NoScenario)),
                ("resume", Prob(NoScenario)),
                ("stop", Prob(NoScenario)),
                ("unload", Prob(NoScenario)),
                ("set_clock", Prob(NoScenario)),
            ],
        ),
        (
            "ready",
            [
                ("start", Okay(200)),
                ("pause", Prob(InvalidState)),
                ("resume", Prob(InvalidState)),
                ("stop", Okay(200)),
                ("unload", Okay(204)),
                ("set_clock", Okay(200)),
            ],
        ),
        (
            "running",
            [
                ("start", Prob(InvalidState)),
                ("pause", Okay(200)),
                ("resume", Prob(InvalidState)),
                ("stop", Okay(200)),
                ("unload", Okay(204)),
                ("set_clock", Okay(200)),
            ],
        ),
        (
            "paused",
            [
                ("start", Prob(InvalidState)),
                ("pause", Prob(InvalidState)),
                ("resume", Okay(200)),
                ("stop", Okay(200)),
                ("unload", Okay(204)),
                ("set_clock", Okay(200)),
            ],
        ),
        (
            "stopped",
            [
                ("start", Prob(InvalidState)),
                ("pause", Prob(InvalidState)),
                ("resume", Prob(InvalidState)),
                ("stop", Prob(InvalidState)),
                ("unload", Okay(204)),
                ("set_clock", Prob(InvalidState)),
            ],
        ),
    ];
    let h = Harness::start("run-matrix");
    for (state, row) in matrix {
        for (command, expected) in row {
            let _ = h.send(Method::DELETE, "/api/v1/scenario").await;
            reach(&h, state).await;
            let reply = apply(&h, command).await;
            match expected {
                Okay(status) => assert_eq!(
                    reply.status,
                    status,
                    "{command} in {state}: {}",
                    reply.text()
                ),
                Prob(code) => {
                    reply.assert_problem(code);
                }
            }
        }
    }
}

#[tokio::test]
async fn run_commands_report_the_new_state() {
    // Extra: the success bodies are `RunStatus` and show the transition.
    let h = Harness::start("run-states");
    h.load_skeleton("stepped").await;
    let started = h.run_command("start").await.assert_json(200);
    assert_eq!(started["state"], "running");
    let paused = h.run_command("pause").await.assert_json(200);
    assert_eq!(paused["state"], "paused");
    let resumed = h.run_command("resume").await.assert_json(200);
    assert_eq!(resumed["state"], "running");
    let stopped = h.run_command("stop").await.assert_json(200);
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["stop_reason"], "command");
}

#[tokio::test]
async fn step_without_stepped_clock_is_409_not_stepped() {
    let h = Harness::start("run-not-stepped");
    // With the manual wall clock a real-time run never advances, so the state stays `running`.
    h.load_skeleton("realtime").await;
    h.run_command("start").await.assert_json(200);
    h.step(1).await.assert_problem(ProblemCode::NotStepped);
}

#[tokio::test]
async fn step_returns_requested_completed_reached_hash() {
    let h = Harness::start("run-step");
    h.load_skeleton("stepped").await;
    h.run_command("start").await.assert_json(200);
    let first = h.step(10).await.assert_json(200);
    assert_eq!(first["requested"], 10);
    assert_eq!(first["completed"], true);
    assert_eq!(first["reached"], 10);
    assert_eq!(first["run"]["state"], "running");
    assert_eq!(first["run"]["tick"], 10);
    let hash = first["state_hash"].as_str().expect("hash string");
    assert!(
        hash.len() == 16
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "16 lower-case hex digits: {hash}"
    );
    let second = h.step(5).await.assert_json(200);
    assert_eq!(second["reached"], 15);
    assert_ne!(second["state_hash"], first["state_hash"], "the state moved");
}

#[tokio::test]
async fn step_count_out_of_range_is_422_validation_failed() {
    // Extra: the bounds of `StepRequest`.
    let h = Harness::start("run-step-range");
    h.load_skeleton("stepped").await;
    h.run_command("start").await.assert_json(200);
    for count in [0_u64, 1_000_001] {
        h.step(count)
            .await
            .assert_problem(ProblemCode::ValidationFailed);
    }
}

#[tokio::test]
async fn second_step_is_409_step_in_progress() {
    let h = Harness::start("run-step-in-progress");
    let path = write_long_scenario(&h.root);
    h.load_body(&json!({"path": path, "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    h.run_command("start").await.assert_json(200);
    // The largest step takes the engine a while; `step_pending` in the status says it has begun.
    let pending = h.send_json_detached(
        Method::POST,
        "/api/v1/run/step",
        json!({"count": 1_000_000}),
    );
    loop {
        let status = h.get("/api/v1/status").await.assert_json(200);
        if status["run"]["step_pending"] == true {
            break;
        }
        assert!(
            !pending.is_finished(),
            "the first step ended before it was seen pending"
        );
        tokio::task::yield_now().await;
    }
    h.step(1).await.assert_problem(ProblemCode::StepInProgress);
    // Pausing cancels the pending step, and its request answers `step_cancelled`.
    h.run_command("pause").await.assert_json(200);
    let first = pending.await.expect("join");
    first.assert_problem(ProblemCode::StepCancelled);
}

#[tokio::test]
async fn set_clock_scaled_out_of_range_is_422() {
    let h = Harness::start("run-clock-range");
    h.load_skeleton("stepped").await;
    for scale in [0.0, 0.0005, 2_000_000.0, -1.0] {
        h.send_json(
            Method::PUT,
            "/api/v1/run/clock",
            &json!({"mode": "scaled", "scale": scale}),
        )
        .await
        .assert_problem(ProblemCode::ValidationFailed);
    }
    let ok = h
        .send_json(
            Method::PUT,
            "/api/v1/run/clock",
            &json!({"mode": "scaled", "scale": 4.0}),
        )
        .await
        .assert_json(200);
    assert_eq!(ok["clock"], json!({"mode": "scaled", "scale": 4.0}));
}

#[tokio::test]
async fn unload_then_entities_is_409_no_scenario() {
    let h = Harness::start("run-unload");
    h.load_skeleton("stepped").await;
    assert_eq!(h.entities("").await.status, 200);
    let unloaded = h.send(Method::DELETE, "/api/v1/scenario").await;
    assert_eq!(unloaded.status, 204);
    assert_eq!(unloaded.body, Vec::<u8>::new());
    h.entities("").await.assert_problem(ProblemCode::NoScenario);
    h.send(Method::DELETE, "/api/v1/scenario")
        .await
        .assert_problem(ProblemCode::NoScenario);
}

#[tokio::test]
async fn engine_thread_dead_is_503_engine_unavailable() {
    let mut h = Harness::start("run-engine-dead");
    h.load_skeleton("stepped").await;
    h.stop_engine();
    for uri in ["/api/v1/status", "/api/v1/entities"] {
        h.get(uri)
            .await
            .assert_problem(ProblemCode::EngineUnavailable);
    }
}
