//! AT-10 (WI-16): a slow or stalled stream client never slows the tick. Requirements: REQ-X-7,
//! REQ-K-6, M5 acceptance 3, ADR 0027 (M4, M11, m9).
//!
//! The first two tests drive the generic connection loop `run_connection` with stub sinks under
//! `tokio::time::pause` (virtual time, no real waiting); the engine runs on its own thread with a
//! `ManualWallClock`. The last two use real loopback sockets.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use axum::extract::ws::Message;
use common::{Harness, WsClient, copy_dir, examples_dir};
use futures_util::{Sink, StreamExt};
use serde_json::{Value, json};
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{FrameCause, RunState, StopReason, SubscribeOptions};
use umbra_engine::{
    ClockMode, Config, ManualWallClock, RunUntil, Session, SessionOptions, SimTime, StreamConfig,
};
use umbra_server::{CloseReason, ConnectionParams, Encoding, run_connection};

/// The per-tick state hashes of `walking_skeleton` from a plain engine run (the reference).
fn reference_hashes() -> Vec<u64> {
    let path = examples_dir().join("walking_skeleton/scenario.toml");
    std::thread::Builder::new()
        .name("reference-run".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let registry = standard_registry().expect("registry");
            let mut options = SessionOptions::new(Box::new(ManualWallClock::new()));
            options.clock = Some(ClockMode::AsFastAsPossible);
            let mut session = Session::load(&registry, &path, options).expect("load");
            let duration = session.duration_ticks();
            let engine = session.engine_mut();
            engine.resume();
            let mut hashes = Vec::new();
            engine
                .run(RunUntil::Ticks(duration), |view| {
                    hashes.push(view.state_hash().0);
                    ControlFlow::Continue(())
                })
                .expect("reference run");
            hashes
        })
        .expect("spawn")
        .join()
        .expect("reference thread")
}

/// A sink that takes the first message (`hello`) and then never becomes ready again. The first
/// message passes so that the writer gets to register its engine subscription: a writer stuck in
/// `hello` would never subscribe and the bus would have nothing to drop (ASM-214).
#[derive(Default)]
struct StalledSink {
    started: usize,
}

impl Sink<Message> for StalledSink {
    type Error = String;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        if self.started == 0 {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn start_send(mut self: Pin<&mut Self>, _item: Message) -> Result<(), String> {
        self.started += 1;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        if self.started <= 1 {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        Poll::Pending
    }
}

/// What a `GatedSink` saw.
#[derive(Default)]
struct GateState {
    open: bool,
    waker: Option<Waker>,
    /// A message was handed over with `start_send` and its flush has not completed.
    in_flight: bool,
    /// `start_send` was called while the previous message was still in flight.
    overlapped: bool,
    started: usize,
    delivered: Vec<String>,
    last_text: Option<String>,
    /// The code of a close frame handed over with `start_send`.
    close_code: Option<u16>,
}

/// A sink that accepts a message at once but completes its flush only while the gate is open.
struct GatedSink(Arc<Mutex<GateState>>);

impl Sink<Message> for GatedSink {
    type Error = String;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), String> {
        let mut state = self.0.lock().expect("lock");
        state.overlapped |= state.in_flight;
        state.in_flight = true;
        state.started += 1;
        if let Message::Close(Some(frame)) = &item {
            state.close_code = Some(frame.code);
        }
        state.last_text = match item {
            Message::Text(text) => Some(text.as_str().to_string()),
            _ => None,
        };

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        let mut state = self.0.lock().expect("lock");
        if !state.in_flight {
            return Poll::Ready(Ok(()));
        }
        if !state.open {
            state.waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        state.in_flight = false;
        if let Some(text) = state.last_text.take() {
            state.delivered.push(text);
        }
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), String>> {
        self.poll_flush(cx)
    }
}

/// The messages a client would have sent: one `subscribe`, then nothing and no close.
fn client_stream(
    subscribe: &Value,
) -> impl futures_util::Stream<Item = Result<Message, String>> + Unpin + Send + 'static {
    let first: Result<Message, String> = Ok(Message::Text(subscribe.to_string().into()));
    futures_util::stream::iter(vec![first]).chain(futures_util::stream::pending())
}

fn stream_settings(send_timeout: Duration) -> StreamConfig {
    let mut settings = Config::default().server.stream;
    settings.send_timeout = send_timeout;
    settings
}

fn tick_of(frame: &umbra_engine::bus::Frame) -> Option<u64> {
    frame.run.time.map(SimTime::tick)
}

async fn wait_until_stopped(h: &Harness) -> umbra_engine::handle::EngineStatus {
    loop {
        let status = h.handle.status().expect("status");
        if status.run.state == RunState::Stopped {
            return status;
        }
        // Yield only: virtual time must not advance while the engine runs.
        tokio::task::yield_now().await;
    }
}

/// A connection whose sink never becomes ready does not slow the tick: the run reaches
/// `stopped(duration)`, a lossless subscription sees the reference per-tick hashes, and the
/// connection ends `too_slow` after `send_timeout` of paused time, with its queue within
/// `queue_frames` and its drops counted (`run_connection` returns the counters and the close
/// reason, ASM-186, ASM-214).
#[tokio::test(start_paused = true)]
async fn stalled_sink_never_slows_the_tick() {
    let reference = reference_hashes();
    let h = Harness::start("slow-stalled");
    let timeout = Duration::from_secs(10);

    // A lossless subscription records every tick frame and its hash.
    let mut lossless_options = SubscribeOptions::new();
    lossless_options.capacity = NonZeroUsize::new(100_000).expect("non zero");
    lossless_options.with_state_hash = true;
    let lossless = h.handle.subscribe(lossless_options);

    let settings = stream_settings(timeout);
    let queue_frames = settings.queue_frames;
    let params = ConnectionParams::new(
        h.handle.clone(),
        settings,
        h.shutdown.clone(),
        Encoding::Json,
    );
    let connection = tokio::spawn(run_connection(
        StalledSink::default(),
        client_stream(&json!({"type": "subscribe"})),
        params,
    ));

    let path: PathBuf = h.root.join("walking_skeleton/scenario.toml");
    h.handle
        .load(path, None, Some(ClockMode::AsFastAsPossible))
        .expect("load");
    h.handle.start().expect("start");
    let status = wait_until_stopped(&h).await;

    // The run reached its duration although the connection never took a byte.
    assert_eq!(status.run.stop_reason, Some(StopReason::Duration));
    assert_eq!(status.run.time.map(SimTime::tick), Some(7200));
    assert!(!connection.is_finished(), "no virtual time has passed yet");
    let drained = lossless.drain();
    assert_eq!(drained.dropped, 0, "the lossless subscription lost nothing");
    let ticks: Vec<(u64, u64)> = drained
        .frames
        .iter()
        .filter(|f| f.cause == FrameCause::Tick)
        .map(|f| {
            (
                tick_of(f).expect("tick frames have a time"),
                f.state_hash.expect("hash requested").0,
            )
        })
        .collect();
    assert_eq!(ticks.len(), reference.len());
    for (tick, hash) in ticks {
        let index = usize::try_from(tick - 1).expect("index");
        assert_eq!(hash, reference[index], "state hash of tick {tick}");
    }

    // After `send_timeout` of paused time the stalled connection gives up.
    let started = tokio::time::Instant::now();
    let (counters, reason) = tokio::time::timeout(Duration::from_secs(120), connection)
        .await
        .expect("the stalled connection closes (virtual time)")
        .expect("connection task");
    assert!(
        started.elapsed() >= timeout,
        "closed no earlier than send_timeout"
    );
    assert_eq!(reason, CloseReason::TooSlow);
    assert!(
        counters.max_queue_len > 0,
        "frames were queued: {counters:?}"
    );
    assert!(
        counters.max_queue_len <= queue_frames,
        "the queue never exceeded queue_frames ({queue_frames}): {counters:?}"
    );
    assert!(
        counters.bus_dropped + counters.skipped > 0,
        "7200 ticks into a stalled connection drop frames: {counters:?}"
    );
    assert_eq!(counters.sent, 0, "nothing was ever sent: {counters:?}");
}

#[tokio::test(start_paused = true)]
async fn writer_never_cancels_a_send_in_progress() {
    let h = Harness::start("slow-no-cancel");
    h.load_body(&json!({"path": "walking_skeleton/scenario.toml", "clock": {"mode": "stepped"}}))
        .await
        .assert_json(201);
    h.run_command("start").await.assert_json(200);
    let gate = Arc::new(Mutex::new(GateState::default()));
    let params = ConnectionParams::new(
        h.handle.clone(),
        stream_settings(Duration::from_secs(3600)),
        h.shutdown.clone(),
        Encoding::Json,
    );
    let connection = tokio::spawn(run_connection(
        GatedSink(Arc::clone(&gate)),
        client_stream(&json!({"type": "subscribe"})),
        params,
    ));
    // The first message (`hello`) is handed to the sink and its flush stays pending.
    while gate.lock().expect("lock").started == 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // More frames are published while that send is in flight.
    for _ in 0..3 {
        h.handle.step(10).expect("step");
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    {
        let state = gate.lock().expect("lock");
        assert_eq!(
            state.started, 1,
            "no second send starts while the first is in flight"
        );
        assert!(!state.overlapped);
        assert_eq!(state.delivered, Vec::<String>::new());
    }
    // Open the gate: the first message completes, then the newest frame follows.
    {
        let mut state = gate.lock().expect("lock");
        state.open = true;
        if let Some(waker) = state.waker.take() {
            waker.wake();
        }
    }
    for _ in 0..200 {
        let done = gate
            .lock()
            .expect("lock")
            .delivered
            .iter()
            .filter_map(|m| serde_json::from_str::<Value>(m).ok())
            .any(|m| m["type"] == "frame" && m["tick"] == 30);
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let state = gate.lock().expect("lock");
    assert!(
        !state.overlapped,
        "a send was abandoned and another started"
    );
    let first: Value = serde_json::from_str(&state.delivered[0]).expect("JSON");
    assert_eq!(
        first["type"], "hello",
        "the in-flight message was completed first"
    );
    let last: Value = serde_json::from_str(state.delivered.last().expect("a frame")).expect("JSON");
    assert_eq!(last["type"], "frame");
    assert_eq!(
        last["tick"], 30,
        "the newest frame arrives after the release"
    );
    drop(state);
    connection.abort();
}

/// A send that times out makes one try to hand over the 1008 close frame without waiting: a sink
/// that takes a message at once (here: not flushing it) receives it, so a merely slow client can
/// see why it was dropped (ASM-214).
#[tokio::test(start_paused = true)]
async fn a_timed_out_send_tries_the_too_slow_close_once() {
    let h = Harness::start("slow-close-try");
    let gate = Arc::new(Mutex::new(GateState::default()));
    let params = ConnectionParams::new(
        h.handle.clone(),
        stream_settings(Duration::from_secs(10)),
        h.shutdown.clone(),
        Encoding::Json,
    );
    let (_stats, reason) = run_connection(
        GatedSink(Arc::clone(&gate)),
        client_stream(&json!({"type": "subscribe"})),
        params,
    )
    .await;
    assert_eq!(reason, CloseReason::TooSlow);
    let state = gate.lock().expect("lock");
    assert_eq!(state.started, 2, "hello, then the close frame");
    assert_eq!(state.close_code, Some(1008));
}

#[tokio::test]
async fn snapshot_builds_follow_the_stride() {
    let h = Harness::start("slow-stride");
    let mut options = SubscribeOptions::new();
    options.min_tick_stride = std::num::NonZeroU64::new(2).expect("non zero");
    options.capacity = NonZeroUsize::new(10_000).expect("non zero");
    let sub = h.handle.subscribe(options);
    h.handle
        .load(
            h.root.join("walking_skeleton/scenario.toml"),
            None,
            Some(ClockMode::Stepped),
        )
        .expect("load");
    h.handle.start().expect("start");
    let before = h.handle.snapshot_builds();
    let outcome = h.handle.step(100).expect("step");
    assert!(outcome.completed);
    h.handle.stop().expect("stop");
    let built = h.handle.snapshot_builds() - before;

    let drained = sub.drain();
    assert_eq!(drained.dropped, 0);
    let tick_frames: Vec<u64> = drained
        .frames
        .iter()
        .filter(|f| f.cause == FrameCause::Tick)
        .map(|f| tick_of(f).expect("time"))
        .collect();
    assert_eq!(tick_frames.len(), 50, "100 ticks at stride 2");
    assert!(tick_frames.iter().all(|t| t % 2 == 0), "{tick_frames:?}");
    // Every frame the thread built after `before` was delivered: 50 tick frames plus the control
    // and stop frames, and no tick frame was built only to be thrown away.
    let after_before = drained
        .frames
        .iter()
        .filter(|f| f.cause == FrameCause::Tick || tick_of(f).is_some_and(|t| t >= 100))
        .count();
    assert!(
        built >= 50 && built == u64::try_from(after_before).expect("count"),
        "built {built}"
    );
}

/// Writes `big/` into `root`: 2000 stationary entities on the skeleton's templates, 3 s long.
fn write_big_scenario(root: &std::path::Path) {
    let dir = root.join("big");
    copy_dir(
        &examples_dir().join("walking_skeleton/templates"),
        &dir.join("templates"),
    );
    let mut text = String::from(
        "schema_version = 1\n\n[scenario]\nname = \"big\"\nseed = 1\ntick_rate_hz = 30\nclock = \"afap\"\n\
         duration_s = 3.0\ntemplate_paths = [\"templates\"]\n",
    );
    for i in 0..2000_u32 {
        let lat = 52.0 + f64::from(i / 50) * 0.001;
        let lon = 4.0 + f64::from(i % 50) * 0.001;
        write!(
            text,
            "\n[[entities]]\ntemplate = \"truck.cargo\"\nname = \"e{i}\"\n\
             position = {{ lat_deg = {lat:.6}, lon_deg = {lon:.6} }}\nheading_deg = 90.0\nspeed_mps = 0.0\n"
        )
        .expect("write to a string");
    }
    std::fs::write(dir.join("scenario.toml"), text).expect("write big scenario");
}

#[tokio::test]
async fn real_socket_smoke_with_2000_entities() {
    let h = Harness::start("slow-real-socket");
    write_big_scenario(&h.root);
    let addr = h.serve_on_loopback().await;
    h.load_body(&json!({"path": "big/scenario.toml", "clock": {"mode": "afap"}}))
        .await
        .assert_json(201);

    // A client that subscribes at the highest rate and then never reads.
    let mut stalled = WsClient::connect(addr).await;
    stalled.hello().await;
    stalled.subscribe(json!({"max_rate_hz": 60.0})).await;

    h.run_command("start").await.assert_json(200);
    let status = wait_until_stopped(&h).await;
    assert_eq!(status.run.stop_reason, Some(StopReason::Duration));
    assert_eq!(status.run.time.map(SimTime::tick), Some(90));

    // A reading client still gets the final state.
    let mut reader = WsClient::connect(addr).await;
    reader.hello().await;
    reader.subscribe(json!({})).await;
    let frame = reader.next_frame().await;
    assert_eq!(frame["run"]["state"], "stopped");
    assert_eq!(frame["tick"], 90);
    assert_eq!(frame["entities"].as_array().expect("entities").len(), 2000);

    let conns = h.server.stream_stats();
    assert!(conns.len() >= 2, "both connections are listed: {conns:?}");
    for stat in &conns {
        assert!(
            stat.max_queue_len <= 4,
            "queue stayed within queue_frames: {stat:?}"
        );
    }
    // Connections are listed in connection order: the stalled client came first.
    assert!(
        conns[0].bus_dropped + conns[0].skipped > 0,
        "the stalled connection lost frames: {:?}",
        conns[0]
    );
    drop(stalled);
}

#[tokio::test]
async fn many_clients_mixed_speeds() {
    let h = Harness::start("slow-many");
    h.load_skeleton("afap").await;
    let addr = h.serve_on_loopback().await;

    // Readers at different rates, and one client that never reads.
    let rates = [1.0, 5.0, 15.0, 30.0, 60.0];
    let mut readers = Vec::new();
    for rate in rates {
        let mut ws = WsClient::connect(addr).await;
        ws.hello().await;
        ws.subscribe(json!({"max_rate_hz": rate})).await;
        let primed = ws.next_frame().await;
        assert_eq!(primed["cause"], "control");
        readers.push(tokio::spawn(async move {
            let mut seqs = vec![primed["seq"].as_u64().expect("seq")];
            loop {
                let frame = ws.next_frame().await;
                seqs.push(frame["seq"].as_u64().expect("seq"));
                if frame["run"]["state"] == "stopped" {
                    return (seqs, frame["tick"].as_u64().expect("tick"));
                }
            }
        }));
    }
    let mut silent = WsClient::connect(addr).await;
    silent.hello().await;
    silent.subscribe(json!({})).await;

    h.run_command("start").await.assert_json(200);
    for reader in readers {
        let (seqs, last_tick) = reader.await.expect("reader task");
        assert_eq!(last_tick, 7200, "every reader sees the final frame");
        assert!(
            seqs.windows(2).all(|w| w[0] < w[1]),
            "seq increases: {seqs:?}"
        );
    }
    drop(silent);
}
