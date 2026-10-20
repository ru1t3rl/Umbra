//! AT-14 (WI-17, WI-18): end to end over REST and the stream against the real `umbra serve` process.
//! Requirements: M5 acceptance 2, REQ-K-4. The server runs in a temporary working directory with the
//! repository as its scenario root (`--scenario-root <repo>`), and the test drives it with a plain
//! `std::net` HTTP client and a minimal WebSocket client (`serve_support`).
//!
//! `serve` exists (WI-17), but both tests are ignored until WI-18: the frame they read at the end of
//! the run is a control frame, which carries no `state_hash` (`publish_control` builds it without one),
//! so their `state_hash` assertions see `null`. Whether control frames should carry the hash is
//! WI-18's decision (ASM-217).

#![cfg(feature = "server")]
#![forbid(unsafe_code)]

mod serve_support;

use serde_json::{Value, json};
use serve_support::{Served, Ws, repo_root, workdir};

const SKELETON: &str = "examples/scenarios/walking_skeleton/scenario.toml";
const PATROL: &str = "examples/scenarios/patrol_and_engage/scenario.toml";

fn start(tag: &str) -> Served {
    let dir = workdir(tag);
    let root = repo_root();
    Served::start(&dir, &["--scenario-root", &root.to_string_lossy()])
}

fn connect(served: &Served) -> Ws {
    let mut ws = Ws::connect(served.addr, &[]).unwrap_or_else(|r| panic!("{}", r.text()));
    ws.hello();
    ws
}

fn finish(mut served: Served) {
    assert_eq!(served.terminate(), Some(0), "{}", served.stderr_text());
}

#[test]
#[ignore = "WI-18"]
fn walking_skeleton_end_to_end_over_rest_and_stream() {
    let served = start("e2e-skeleton");
    served.get("/api/v1/status").assert_json(200);

    // 2. Load in stepped mode with a relative path.
    let info = served
        .post(
            "/api/v1/scenario",
            &json!({"path": SKELETON, "clock": {"mode": "stepped"}}),
        )
        .assert_json(201);
    // 5. The seed is a string and the tick rate comes from the scenario.
    assert_eq!(info["seed"], "42");
    assert_eq!(info["tick_rate_source"], "scenario");
    assert_eq!(info["path"], SKELETON);
    let duration = info["duration_ticks"].as_u64().expect("duration");
    assert_eq!(duration, 7200);

    // 3. Subscribe with state hashes: a primed control frame arrives at once.
    let mut ws = connect(&served);
    ws.subscribe(json!({"state_hash": true}));
    let primed = ws.next_json();
    assert_eq!(primed["type"], "frame");
    assert_eq!(primed["cause"], "control");
    assert_eq!(primed["tick"], 0);

    // Start, then step to the duration in chunks.
    served
        .request("POST", "/api/v1/run/start", None)
        .assert_json(200);
    let mut reached = 0;
    let mut last = Value::Null;
    while reached < duration {
        let result = served
            .post("/api/v1/run/step", &json!({"count": 600}))
            .assert_json(200);
        assert_eq!(result["requested"], 600);
        assert_eq!(result["completed"], true);
        reached += 600;
        assert_eq!(result["reached"], reached);
        last = result;
    }
    // 4. The final hash is the golden one, and the last frame equals `GET /entities`.
    assert_eq!(last["state_hash"], "849026656b7b42d2");
    let frame = ws.frame_until(|f| f["tick"] == duration);
    assert_eq!(frame["state_hash"], "849026656b7b42d2");
    let page = served.get("/api/v1/entities").assert_json(200);
    assert_eq!(page["tick"], duration);
    assert_eq!(
        frame["entities"], page["items"],
        "stream and REST agree exactly"
    );

    // 6. Spawn and remove in a fresh session (the finished run is stopped and takes no spawns).
    served
        .post(
            "/api/v1/scenario",
            &json!({"path": SKELETON, "clock": {"mode": "stepped"}}),
        )
        .assert_json(201);
    let spawned = served
        .post(
            "/api/v1/entities",
            &json!({"template": "truck.cargo", "lat_deg": 52.5, "lon_deg": 4.5, "name": "e2e-1"}),
        )
        .assert_json(201);
    assert_eq!(spawned["summary"]["id"], 4);
    let with_four = ws.frame_until(|f| f["entities"].as_array().is_some_and(|e| e.len() == 4));
    assert_eq!(with_four["cause"], "control");
    assert_eq!(
        served.request("DELETE", "/api/v1/entities/4", None).status,
        204
    );

    // 7. Unload, then shut down cleanly.
    assert_eq!(
        served.request("DELETE", "/api/v1/scenario", None).status,
        204
    );
    let status = served.get("/api/v1/status").assert_json(200);
    assert_eq!(status["run"]["state"], "idle");
    finish(served);
}

#[test]
#[ignore = "WI-18"]
fn patrol_and_engage_as_fast_as_possible_to_duration() {
    let served = start("e2e-patrol");
    let info = served
        .post(
            "/api/v1/scenario",
            &json!({"path": PATROL, "clock": {"mode": "afap"}}),
        )
        .assert_json(201);
    assert_eq!(info["duration_ticks"], 4200);
    let mut ws = connect(&served);
    ws.subscribe(json!({"state_hash": true}));
    ws.frame_until(|f| f["cause"] == "control");
    served
        .request("POST", "/api/v1/run/start", None)
        .assert_json(200);

    // The run ends by itself: the stream shows the stopped frame (no polling, no sleeping).
    let frame = ws.frame_until(|f| f["run"]["state"] == "stopped");
    assert_eq!(frame["run"]["stop_reason"], "duration");
    assert_eq!(frame["tick"], 4200);
    assert_eq!(frame["state_hash"], "3a7bfda0a76111b9");
    let status = served.get("/api/v1/status").assert_json(200);
    assert_eq!(status["run"]["state"], "stopped");
    assert_eq!(status["run"]["tick"], 4200);
    finish(served);
}
