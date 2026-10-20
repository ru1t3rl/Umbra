//! The M5 WI-6 surface of `umbra-server`: settings, shutdown signal, clock mapping and problem
//! construction are real; the router, the stream loop and the handle-error mapping are stubs
//! that WI-11 onward implement.

#![forbid(unsafe_code)]

use std::time::Duration;

use umbra_api_types::{ClockSetting, ProblemCode};
use umbra_engine::{ClockMode, Config, SCENARIO_FILE_SCHEMA};
use umbra_server::map::{MapError, clock_mode, clock_setting};
use umbra_server::{ApiError, Encoding, ServerSettings, Shutdown};

#[test]
fn clock_setting_spellings_equal_the_scenario_files() {
    // The schema's `clock` enum is the file's spelling (the reader accepts exactly these).
    let schema: serde_json::Value = serde_json::from_str(SCENARIO_FILE_SCHEMA).expect("schema");
    let in_file: Vec<String> = schema["properties"]["scenario"]["properties"]["clock"]["enum"]
        .as_array()
        .expect("the scenario clock enum")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect();
    assert_eq!(in_file, ClockSetting::MODES);
    // Every wire spelling maps to a different engine mode and back.
    let settings = [
        ClockSetting::Afap,
        ClockSetting::Realtime,
        ClockSetting::Stepped,
        ClockSetting::Scaled { scale: 4.0 },
    ];
    let mut modes = Vec::new();
    for (setting, spelling) in settings.iter().zip(in_file.iter()) {
        let json = serde_json::to_value(setting).expect("serialise");
        assert_eq!(json["mode"], spelling.as_str());
        let mode = clock_mode(setting).expect("maps");
        assert_eq!(&clock_setting(mode), setting);
        modes.push(mode);
    }
    for (i, a) in modes.iter().enumerate() {
        for b in &modes[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_eq!(modes[0], ClockMode::AsFastAsPossible);
    assert_eq!(modes[1], ClockMode::RealTime);
    assert_eq!(modes[2], ClockMode::Stepped);
}

#[test]
fn clock_spellings_parse_in_a_real_scenario_file() {
    // Load a scenario with every spelling and compare with the setting the wire form maps to.
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("wi6-clock-spellings");
    std::fs::create_dir_all(&dir).expect("dir");
    let base = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/scenarios/walking_skeleton/scenario.toml"
    ))
    .expect("example scenario");
    assert!(base.contains("clock = \"afap\""));
    let templates = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/scenarios/walking_skeleton/templates"
    );
    let base = base.replace(
        "template_paths = [\"templates\"]",
        &format!("template_paths = [{templates:?}]"),
    );
    let registry = umbra_engine::builtins::standard_registry().expect("registry");
    for (spelling, extra, expected) in [
        ("afap", "", ClockMode::AsFastAsPossible),
        ("realtime", "", ClockMode::RealTime),
        ("stepped", "", ClockMode::Stepped),
        (
            "scaled",
            "time_scale = 4.0\n",
            clock_mode(&ClockSetting::Scaled { scale: 4.0 }).expect("maps"),
        ),
    ] {
        let text = base.replace(
            "clock = \"afap\"",
            &format!("clock = \"{spelling}\"\n{extra}"),
        );
        let path = dir.join(format!("{spelling}.toml"));
        std::fs::write(&path, text).expect("write");
        let loaded = umbra_engine::scenario::load_scenario_with_settings(
            &registry,
            &path,
            &umbra_engine::LoadSettings::default(),
        )
        .unwrap_or_else(|e| panic!("{spelling}: {e}"));
        let wire: ClockSetting = serde_json::from_value(if spelling == "scaled" {
            serde_json::json!({"mode": spelling, "scale": 4.0})
        } else {
            serde_json::json!({"mode": spelling})
        })
        .expect("wire form");
        assert_eq!(clock_mode(&wire).expect("maps"), expected, "{spelling}");
        let from_file = match loaded.clock {
            umbra_engine::ClockSetting::AsFastAsPossible => ClockSetting::Afap,
            umbra_engine::ClockSetting::RealTime => ClockSetting::Realtime,
            umbra_engine::ClockSetting::Stepped => ClockSetting::Stepped,
            umbra_engine::ClockSetting::Scaled(scale) => ClockSetting::Scaled { scale },
        };
        assert_eq!(from_file, wire, "{spelling}");
    }
}

#[test]
fn scaled_clock_outside_the_kernel_range_is_refused() {
    for scale in [0.0, -1.0, 0.0001, 2_000_000.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            clock_mode(&ClockSetting::Scaled { scale }),
            Err(MapError::InvalidScale),
            "{scale}"
        );
    }
    assert!(clock_mode(&ClockSetting::Scaled { scale: 0.001 }).is_ok());
    assert!(clock_mode(&ClockSetting::Scaled { scale: 1_000_000.0 }).is_ok());
}

#[test]
fn settings_come_from_the_configuration_file_values() {
    let defaults = ServerSettings::from_config(&Config::default());
    assert_eq!(defaults.server.bind.to_string(), "127.0.0.1:7878");
    assert!(defaults.server.bind.ip().is_loopback());
    assert_eq!(defaults.server.body_limit_bytes, 1_048_576);
    assert_eq!(defaults.server.stream.queue_frames, 4);
    assert_eq!(defaults.server.shutdown_grace, Duration::from_secs(5));
    assert_eq!(defaults.default_tick_rate.hz(), 60);
    assert_eq!(defaults.plugin_paths, Vec::<std::path::PathBuf>::new());

    let mut config = Config::default();
    config.tick_rate = Some(umbra_engine::TickRate::new(30).expect("rate"));
    config.plugin_paths = Some(vec!["plugins".into()]);
    config.server.body_limit_bytes = 10;
    let custom = ServerSettings::from_config(&config);
    assert_eq!(custom.default_tick_rate.hz(), 30);
    assert_eq!(
        custom.plugin_paths,
        vec![std::path::PathBuf::from("plugins")]
    );
    assert_eq!(custom.server.body_limit_bytes, 10);
}

#[test]
fn encoding_parses_json_and_names_the_unsupported() {
    assert_eq!(Encoding::parse(None), Ok(Encoding::Json));
    assert_eq!(Encoding::parse(Some("json")), Ok(Encoding::Json));
    assert_eq!(Encoding::Json.as_str(), "json");
    for other in ["msgpack", "JSON", "", "cbor"] {
        assert_eq!(
            Encoding::parse(Some(other)),
            Err(other.to_string()),
            "{other}"
        );
    }
}

#[test]
fn api_error_takes_status_and_problem_from_its_code() {
    let error = ApiError::new(ProblemCode::StepCancelled, "paused");
    assert_eq!(error.status(), 409);
    let problem = error.into_problem();
    assert_eq!(problem.code, ProblemCode::StepCancelled);
    assert_eq!(problem.status, 409);
    assert_eq!(problem.detail, "paused");
}

#[tokio::test]
async fn shutdown_signal_wakes_every_waiter_once_triggered() {
    let shutdown = Shutdown::new();
    assert!(!shutdown.is_triggered());
    let mut waiters = Vec::new();
    for _ in 0..3 {
        let clone = shutdown.clone();
        waiters.push(tokio::spawn(async move { clone.triggered().await }));
    }
    tokio::task::yield_now().await;
    assert!(
        waiters.iter().all(|w| !w.is_finished()),
        "nobody wakes early"
    );
    shutdown.trigger();
    assert!(shutdown.is_triggered());
    for waiter in waiters {
        waiter.await.expect("waiter");
    }
    // A late waiter returns at once; triggering twice is harmless.
    shutdown.trigger();
    shutdown.clone().triggered().await;
}

/// Never called (the stubs are `todo!`): it only has to type-check from outside the crate.
#[expect(dead_code, reason = "type-check only")]
async fn stub_signatures_compile(
    handle: umbra_engine::handle::EngineHandle,
    listener: tokio::net::TcpListener,
) {
    use umbra_server::{ConnectionParams, ConnectionStats, Server};

    // `ConnectionParams` can be built from outside the crate (the slow-client test needs it).
    let params = ConnectionParams::new(
        handle.clone(),
        umbra_engine::StreamConfig::default(),
        Shutdown::new(),
        Encoding::Json,
    );
    // A clone keeps `stream_stats` available while the original serves.
    let server = Server::new(
        handle,
        ServerSettings::from_config(&Config::default()),
        Shutdown::new(),
    );
    let probe: Server = server.clone();
    let _router: axum::Router = server.router();
    let serving: std::io::Result<()> = server.serve(listener).await;
    let _stats: Vec<ConnectionStats> = probe.stream_stats();
    let _ = (params, serving);
}

/// `run_connection` accepts any sink whose errors can be displayed and a stream whose errors convert
/// into `ReadFailure`, and returns the counters and the close reason.
#[expect(dead_code, reason = "type-check only")]
async fn run_connection_bounds<Si, St>(sink: Si, stream: St, params: umbra_server::ConnectionParams)
where
    Si: futures_util::Sink<axum::extract::ws::Message> + Unpin + Send + 'static,
    Si::Error: std::fmt::Display + Send,
    St: futures_util::Stream<Item = Result<axum::extract::ws::Message, String>>
        + Unpin
        + Send
        + 'static,
{
    let (_stats, _reason): (umbra_server::ConnectionStats, umbra_server::CloseReason) =
        umbra_server::run_connection(sink, stream, params).await;
}
