//! AT-4 (engine handle core, M5 WI-8 and WI-9): thread, state machine,
//! command order, priming, publish-then-reply, load and unload, queue and panic policy.
//! Requirements: REQ-X-1, REQ-K-3, ADR 0024, ADR 0026 (command x state matrix), ASM-098.
//!
//! Engines run on a manual wall clock unless the test says
//! otherwise; a run that must hold its state uses the frozen wall clock of `handle_support`.
//! Steps, queries and shutdown are in `handle_steps.rs`, `handle_queries.rs`, `handle_stack.rs` and
//! `handle_shutdown.rs`.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(clippy::panic, reason = "tests fail by panicking")]

mod handle_support;

use handle_support::guarded_step;
mod plugin_support;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};

use handle_support::{
    GUARD, PluginRecorder, capture, collect_until, config, engine, examples, frozen_config, jam,
    load_stepped, lossless, plugin_lock, receive, scenario_copy, short_skeleton, start_engine,
    state_of, trace, until_stopped, walking_skeleton,
};
use plugin_support::{Tpl, write_scenario};
use umbra_core::{Geodetic, Position, StateHash, TemplateId, TickRate};
use umbra_engine::builtins::standard_registry;
use umbra_engine::bus::{Frame, FrameCause, RunState, RunStatus, StopReason, SubscribeOptions};
use umbra_engine::handle::{
    Applied, Command, EngineHandle, EngineStatus, EntityState, HandleError, SpawnError,
};
use umbra_engine::validate_path_with_plugins;
use umbra_engine::{
    ClockMode, EntityId, ScenarioError, SessionError, TemplateSpawn, TickRateSource,
};

fn template(id: &str) -> TemplateId {
    TemplateId::new(id).expect("valid template id")
}

fn spawn_spec(name: &str) -> TemplateSpawn {
    let at = Geodetic::from_degrees(52.001, 4.002, 0.0).expect("valid position");
    let mut spec = TemplateSpawn::new(template("ground_vehicle.base"), Position::Geodetic(at));
    spec.name = Some(name.to_owned());
    spec
}

fn tick_of(frame: &Frame) -> u64 {
    frame
        .snapshot
        .as_ref()
        .expect("a frame with a world")
        .time
        .tick()
}

fn last_tick_hash(frames: &[Arc<Frame>]) -> StateHash {
    frames
        .iter()
        .rev()
        .find(|f| f.cause == FrameCause::Tick)
        .and_then(|f| f.state_hash)
        .expect("a tick frame with a state hash")
}

// ---- the handle ----

#[test]
fn handle_is_send_sync_clone() {
    fn assert_all<T: Send + Sync + Clone>() {}
    assert_all::<EngineHandle>();
}

// ---- the command x state matrix (ADR 0026, m3) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Out {
    Ok,
    NoScenario,
    InvalidState,
    NotStepped,
}

impl Out {
    /// The spelling of the ADR table.
    fn word(self) -> &'static str {
        match self {
            Out::Ok => "ok",
            Out::NoScenario => "no_scenario",
            Out::InvalidState => "invalid_state",
            Out::NotStepped => "not_stepped",
        }
    }
}

fn out<T>(result: Result<T, HandleError>) -> Out {
    match result {
        Ok(_) => Out::Ok,
        Err(HandleError::NoSession) => Out::NoScenario,
        Err(HandleError::InvalidState { .. }) => Out::InvalidState,
        Err(HandleError::NotStepped) => Out::NotStepped,
        Err(other) => panic!("not a matrix cell: {other:?}"),
    }
}

const STATES: [RunState; 5] = [
    RunState::Idle,
    RunState::Ready,
    RunState::Running,
    RunState::Paused,
    RunState::Stopped,
];

fn state_word(state: RunState) -> &'static str {
    match state {
        RunState::Idle => "idle",
        RunState::Ready => "ready",
        RunState::Running => "running",
        RunState::Paused => "paused",
        _ => "stopped",
    }
}

/// A fresh engine whose run is in `state`, loaded with the stepped clock on a frozen wall clock, so
/// nothing advances and `running` holds.
fn reach(state: RunState, scenario: &Path) -> (EngineHandle, umbra_engine::handle::EngineThread) {
    let (handle, thread) = start_engine(frozen_config());
    if state != RunState::Idle {
        load_stepped(&handle, scenario.to_path_buf());
    }
    match state {
        RunState::Running | RunState::Paused | RunState::Stopped => {
            handle.start().expect("start");
        }
        _ => {}
    }
    match state {
        RunState::Paused => {
            handle.pause().expect("pause");
        }
        RunState::Stopped => {
            handle.stop().expect("stop");
        }
        _ => {}
    }
    assert_eq!(state_of(&handle), state, "set-up reached the state");
    (handle, thread)
}

/// A command of the table: its name and what it does to an engine.
type Case<'a> = (&'static str, &'a dyn Fn(&EngineHandle) -> Out);

/// One line per command: its name, then for each of idle, ready, running, paused and stopped the
/// outcome and the run state afterwards (`outcome/state`), as in the ADR table.
type Table = Vec<Vec<String>>;

/// Runs every command of `commands` in every state.
fn run_matrix(commands: &[Case<'_>]) -> Table {
    let scenario = short_skeleton();
    commands
        .iter()
        .map(|(name, command)| {
            let mut line = vec![(*name).to_owned()];
            for state in STATES {
                let (handle, _thread) = reach(state, &scenario);
                let outcome = command(&handle);
                line.push(format!(
                    "{}/{}",
                    outcome.word(),
                    state_word(state_of(&handle))
                ));
            }
            line
        })
        .collect()
}

/// Reads a table written as `name | cell | cell | cell | cell | cell` lines.
fn table(text: &str) -> Table {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split('|').map(|c| c.trim().to_owned()).collect())
        .collect()
}

use Out::{InvalidState as Inv, NoScenario as NoSc};
use RunState::{Idle, Paused, Ready, Running, Stopped};

/// ADR 0026, m3: the lifecycle rows of the command x state table.
#[test]
fn command_state_matrix() {
    let scenario = short_skeleton();
    let load =
        move |h: &EngineHandle| out(h.load(scenario.clone(), None, Some(ClockMode::Stepped)));

    let got = run_matrix(&[
        ("load", &load),
        ("unload", &|h| out(h.unload())),
        ("start", &|h| out(h.start())),
        ("pause", &|h| out(h.pause())),
        ("resume", &|h| out(h.resume())),
        ("stop", &|h| out(h.stop())),
        ("set clock", &|h| out(h.set_clock(ClockMode::RealTime))),
        ("status", &|h| out(h.status())),
    ]);

    let want = table(
        "
        load      | ok/ready              | ok/ready              | ok/ready              | ok/ready              | ok/ready
        unload    | no_scenario/idle      | ok/idle               | ok/idle               | ok/idle               | ok/idle
        start     | no_scenario/idle      | ok/running            | invalid_state/running | invalid_state/paused  | invalid_state/stopped
        pause     | no_scenario/idle      | invalid_state/ready   | ok/paused             | invalid_state/paused  | invalid_state/stopped
        resume    | no_scenario/idle      | invalid_state/ready   | invalid_state/running | ok/running            | invalid_state/stopped
        stop      | no_scenario/idle      | ok/stopped            | ok/stopped            | ok/stopped            | invalid_state/stopped
        set clock | no_scenario/idle      | ok/ready              | ok/running            | ok/paused             | invalid_state/stopped
        status    | ok/idle               | ok/ready              | ok/running            | ok/paused             | ok/stopped
        ",
    );
    assert_eq!(got, want);
}

/// ADR 0026, m3: the rows that need entities, steps and queries (WI-9), including `step` on a
/// stepped clock and the read-only queries that work in every state.
#[test]
fn command_state_matrix_entity_commands() {
    let one = EntityId::new(1).expect("non-zero");
    let stepped = |h: &EngineHandle| out(guarded_step(h, 1));
    let spawn = |h: &EngineHandle| out(h.spawn_entity(spawn_spec("matrix")));

    let got = run_matrix(&[
        ("step", &stepped),
        ("spawn", &spawn),
        ("remove", &|h| out(h.remove_entity(one))),
        ("entity", &|h| out(h.entity(one))),
        ("templates", &|h| out(h.templates())),
        ("catalog", &|h| out(h.catalog())),
        ("components", &|h| out(h.components(None))),
        ("plugins", &|h| out(h.plugins())),
        ("validate", &|h| {
            out(h.validate_graph("draft.behavior.toml".to_owned(), String::new()))
        }),
    ]);

    let want = table(
        "
        step       | no_scenario/idle | invalid_state/ready | ok/running | invalid_state/paused | invalid_state/stopped
        spawn      | no_scenario/idle | ok/ready            | ok/running | ok/paused            | invalid_state/stopped
        remove     | no_scenario/idle | ok/ready            | ok/running | ok/paused            | invalid_state/stopped
        entity     | no_scenario/idle | ok/ready            | ok/running | ok/paused            | ok/stopped
        templates  | no_scenario/idle | ok/ready            | ok/running | ok/paused            | ok/stopped
        catalog    | ok/idle          | ok/ready            | ok/running | ok/paused            | ok/stopped
        components | ok/idle          | ok/ready            | ok/running | ok/paused            | ok/stopped
        plugins    | ok/idle          | ok/ready            | ok/running | ok/paused            | ok/stopped
        validate   | ok/idle          | ok/ready            | ok/running | ok/paused            | ok/stopped
        ",
    );
    assert_eq!(got, want);
}

/// ADR 0026, m3: `step` in `running` on a clock that is not stepped is `not_stepped`.
#[test]
fn step_on_a_clock_that_is_not_stepped_is_not_stepped() {
    // extra: the second `step` cell of the table.
    let (handle, _thread) = start_engine(frozen_config());
    handle
        .load(short_skeleton(), None, Some(ClockMode::RealTime))
        .expect("load");
    handle.start().expect("start");
    assert_eq!(state_of(&handle), Running);

    let outcome = out(guarded_step(&handle, 1));

    assert_eq!(outcome, Out::NotStepped);
    assert_eq!(state_of(&handle), Running);
}

#[test]
fn repeated_commands_are_not_idempotent() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    assert!(handle.start().is_ok());

    let second_start = handle.start();
    assert!(handle.unload().is_ok());
    let second_unload = handle.unload();

    assert_eq!(out(second_start), Inv);
    assert_eq!(out(second_unload), NoSc);
}

// ---- order ----

#[test]
fn commands_apply_between_ticks_in_arrival_order() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    handle.start().expect("start");
    guarded_step(&handle, 5).expect("step");
    let first = handle.spawn_entity(spawn_spec("a")).expect("spawn a");

    // Submitted without waiting, so they sit in the queue together: the name `a` is free for the
    // spawn only if the removal ran before it.
    let (remove_reply, removal) = capture::<Applied>();
    let (respawn_reply, respawn) = capture::<EntityState>();
    handle
        .submit(Command::Remove {
            id: first.snapshot.id,
            reply: remove_reply,
        })
        .expect("submit remove");
    handle
        .submit(Command::Spawn {
            spec: spawn_spec("a"),
            reply: respawn_reply,
        })
        .expect("submit spawn");
    let applied = receive(&removal).expect("remove applied");
    let second = receive(&respawn).expect("the respawn came after the removal");

    // The other way round: the spawn comes first and meets the name it now holds.
    let (clash_reply, clash) = capture::<EntityState>();
    let (last_remove_reply, last_removal) = capture::<Applied>();
    handle
        .submit(Command::Spawn {
            spec: spawn_spec("a"),
            reply: clash_reply,
        })
        .expect("submit spawn");
    handle
        .submit(Command::Remove {
            id: second.snapshot.id,
            reply: last_remove_reply,
        })
        .expect("submit remove");

    assert_eq!(applied.applied_at.tick(), 5, "applied between tick 5 and 6");
    assert!(matches!(
        receive(&clash),
        Err(HandleError::Session(SessionError::DuplicateName(_)))
    ));
    assert!(receive(&last_removal).is_ok());
}

#[test]
fn commands_from_two_clients_apply_in_arrival_order() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    handle.start().expect("start");
    let (sent_tx, sent_rx) = mpsc::channel::<()>();

    // Client B submits only after client A's submit returned, so A arrived first.
    let a = {
        let handle = handle.clone();
        std::thread::spawn(move || {
            let (reply, rx) = capture::<EntityState>();
            handle
                .submit(Command::Spawn {
                    spec: spawn_spec("same-name"),
                    reply,
                })
                .expect("client A submits");
            sent_tx.send(()).expect("signal");
            receive(&rx)
        })
    };
    let b = {
        let handle = handle.clone();
        std::thread::spawn(move || {
            sent_rx.recv_timeout(GUARD).expect("client A submitted");
            let (reply, rx) = capture::<EntityState>();
            handle
                .submit(Command::Spawn {
                    spec: spawn_spec("same-name"),
                    reply,
                })
                .expect("client B submits");
            receive(&rx)
        })
    };

    let (from_a, from_b) = (a.join().expect("client A"), b.join().expect("client B"));

    assert!(from_a.is_ok(), "the first to arrive wins the name");
    assert!(matches!(
        from_b,
        Err(HandleError::Session(SessionError::DuplicateName(_)))
    ));
}

#[test]
fn two_clients_racing_start_one_wins_one_409() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    let barrier = Arc::new(Barrier::new(2));

    let clients: Vec<_> = (0..2)
        .map(|_| {
            let (handle, barrier) = (handle.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                out(handle.start())
            })
        })
        .collect();
    let mut outcomes: Vec<Out> = clients
        .into_iter()
        .map(|c| c.join().expect("client"))
        .collect();
    outcomes.sort_by_key(|o| matches!(o, Out::InvalidState));

    assert_eq!(outcomes, vec![Out::Ok, Out::InvalidState]);
    assert_eq!(state_of(&handle), Running);
}

// ---- publish, then reply (M3) and priming (M1) ----

/// A state change: its name, the state it leads to, and how to ask for it.
type Change<'a> = (
    &'static str,
    RunState,
    &'a dyn Fn() -> Result<RunStatus, HandleError>,
);

#[test]
fn publish_then_reply_frame_is_in_every_subscription_when_reply_arrives() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    let subs: Vec<_> = (0..3).map(|_| handle.subscribe(lossless())).collect();
    for sub in &subs {
        drop(sub.wait(GUARD));
    }

    let changes: [Change<'_>; 4] = [
        ("start", Running, &|| handle.start()),
        ("pause", Paused, &|| handle.pause()),
        ("resume", Running, &|| handle.resume()),
        ("stop", Stopped, &|| handle.stop()),
    ];
    for (name, expected, command) in changes {
        command().unwrap_or_else(|e| panic!("{name}: {e}"));
        // No waiting: the frame must already be there when the reply is.
        for (i, sub) in subs.iter().enumerate() {
            let frames = sub.drain().frames;
            let last = frames.last().map(|f| f.run.state);
            assert_eq!(last, Some(expected), "{name}: subscription {i}");
        }
    }
}

#[test]
fn subscribe_while_paused_gets_a_current_frame() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    handle.start().expect("start");
    guarded_step(&handle, 3).expect("step");
    handle.pause().expect("pause");

    let sub = handle.subscribe(SubscribeOptions::new());
    let frames = sub.wait(GUARD).frames;

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].cause, FrameCause::Control);
    assert_eq!(frames[0].run.state, Paused);
    assert_eq!(tick_of(&frames[0]), 3);
}

#[test]
fn subscribe_while_idle_gets_an_idle_frame() {
    let (handle, _thread) = engine();

    let sub = handle.subscribe(SubscribeOptions::new());
    let frames = sub.wait(GUARD).frames;

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].cause, FrameCause::Control);
    assert_eq!(frames[0].run.state, Idle);
    assert!(frames[0].snapshot.is_none() && frames[0].directory.is_empty());
}

#[test]
fn subscribe_in_every_loaded_state_is_primed_with_that_state() {
    // extra: ready, running, paused and stopped are primed too (ADR 0024, M1).
    let scenario = short_skeleton();
    for state in [Ready, Running, Paused, Stopped] {
        let (handle, _thread) = reach(state, &scenario);

        let frames = handle.subscribe(SubscribeOptions::new()).wait(GUARD).frames;

        assert_eq!(frames.len(), 1, "{state:?}");
        assert_eq!(frames[0].cause, FrameCause::Control, "{state:?}");
        assert_eq!(frames[0].run.state, state);
        assert_eq!(tick_of(&frames[0]), 0, "{state:?}");
    }
}

#[test]
fn priming_reaches_only_the_new_subscription() {
    // extra (WI-8): a late subscriber's priming frame is not pushed to the earlier ones.
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    let early = handle.subscribe(SubscribeOptions::new());
    assert_eq!(early.wait(GUARD).frames.len(), 1, "its own priming frame");

    let late = handle.subscribe(SubscribeOptions::new());
    assert_eq!(late.wait(GUARD).frames.len(), 1);

    // A command round trip puts the thread past the priming turn.
    assert!(handle.status().is_ok());
    assert!(early.drain().frames.is_empty());
}

#[test]
fn subscribe_with_full_command_queue_is_primed_later() {
    let mut small = config();
    small.command_capacity = 2;
    let (handle, _thread) = start_engine(small);
    let held = jam(&handle);
    let (first, first_rx) = capture::<EngineStatus>();
    let (second, second_rx) = capture::<EngineStatus>();
    handle
        .submit(Command::Status { reply: first })
        .expect("fits");
    handle
        .submit(Command::Status { reply: second })
        .expect("fits");
    assert!(
        matches!(handle.status(), Err(HandleError::Busy)),
        "the queue is full"
    );

    let sub = handle.subscribe(SubscribeOptions::new());
    assert!(
        sub.drain().frames.is_empty(),
        "nothing can be primed while the thread is held"
    );
    held.release();

    let frames = sub.wait(GUARD).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].cause, FrameCause::Control);
    assert!(receive(&first_rx).is_ok() && receive(&second_rx).is_ok());
}

// ---- stopping, faults ----

#[test]
fn stops_at_duration_with_reason_and_final_frame() {
    let (handle, _thread) = engine();
    let info = handle.load(short_skeleton(), None, None).expect("load");
    let sub = handle.subscribe(lossless());

    handle.start().expect("start");
    let frames = until_stopped(&sub);

    let last = frames.last().expect("frames");
    assert_eq!(last.run.state, Stopped);
    assert_eq!(last.run.stop_reason, Some(StopReason::Duration));
    assert_eq!(
        tick_of(last),
        info.duration_ticks,
        "the final frame is the last tick's world"
    );
    let ticks: Vec<u64> = frames
        .iter()
        .filter(|f| f.cause == FrameCause::Tick)
        .map(|f| tick_of(f))
        .collect();
    assert_eq!(ticks, (1..=info.duration_ticks).collect::<Vec<_>>());
    let status = handle.status().expect("status").run;
    assert_eq!(status.state, Stopped);
    assert_eq!(status.stop_reason, Some(StopReason::Duration));
}

#[test]
fn plugin_fault_abort_publishes_the_faulting_tick_then_stops() {
    let _lock = plugin_lock();
    let mut with_fixture = config();
    with_fixture.load.extra_plugin_paths = vec![plugin_support::fixture(&[])];
    let (handle, _thread) = start_engine(with_fixture);
    let scenario =
        plugin_support::fault_scenario("core-abort", "abort", 2.0, &["healthy", "direct"]);
    handle.load(scenario, None, None).expect("load");
    let sub = handle.subscribe(lossless());

    handle.start().expect("start");
    let frames = until_stopped(&sub);

    let stop = frames.last().expect("frames");
    assert!(
        matches!(stop.run.stop_reason, Some(StopReason::PluginFault(_))),
        "{:?}",
        stop.run
    );
    let before_stop = &frames[frames.len() - 2];
    assert_eq!(
        before_stop.cause,
        FrameCause::Tick,
        "the faulting tick's own frame"
    );
    assert!(tick_of(before_stop) >= 1);
    assert_eq!(tick_of(stop), tick_of(before_stop));
}

#[test]
fn disable_capability_keeps_running() {
    let _lock = plugin_lock();
    let mut with_fixture = config();
    with_fixture.load.extra_plugin_paths = vec![plugin_support::fixture(&[])];
    let (handle, _thread) = start_engine(with_fixture);
    let scenario = plugin_support::fault_scenario(
        "core-disable",
        "disable_capability",
        2.0,
        &["healthy", "direct"],
    );
    let info = handle.load(scenario, None, None).expect("load");
    let sub = handle.subscribe(lossless());

    handle.start().expect("start");
    let frames = until_stopped(&sub);

    let stop = frames.last().expect("frames");
    assert_eq!(
        stop.run.stop_reason,
        Some(StopReason::Duration),
        "{:?}",
        stop.run
    );
    assert_eq!(tick_of(stop), info.duration_ticks);
}

// ---- load and unload ----

#[test]
fn failed_load_leaves_idle() {
    let (handle, _thread) = engine();
    handle.load(short_skeleton(), None, None).expect("load");
    assert_eq!(state_of(&handle), Ready);

    let failed = handle.load(PathBuf::from("no/such/scenario.toml"), None, None);

    assert!(failed.is_err());
    let status = handle.status().expect("status");
    assert_eq!(status.run.state, Idle, "the old session is gone too (D-a)");
    assert!(status.scenario.is_none());
    assert_eq!(out(handle.start()), NoSc);
}

#[test]
fn replace_with_same_plugin_without_unload() {
    // D-a: the old session is dropped (instances, behaviors, then the plugin's `shutdown`) before
    // the new load starts, so the second load is a new image lifecycle: `init` runs again. The log
    // is read through `HandleConfig::plugin_log`; the fixture logs `init` and `shutdown`.
    let _lock = plugin_lock();
    let (recorder, hook) = PluginRecorder::new();
    let mut with_fixture = config();
    with_fixture.load.extra_plugin_paths = vec![plugin_support::fixture(&[])];
    with_fixture.plugin_log = Some(hook);
    let (handle, _thread) = start_engine(with_fixture);
    let scenario = write_scenario(
        "replace-same",
        2.0,
        "",
        &[Tpl {
            id: "test.entity.life",
            behavior: "test.behavior.lifecycle",
            params: "",
        }],
        &[],
        &[("test.entity.life", "a")],
    );
    let first = handle
        .load(scenario.clone(), None, None)
        .expect("first load");
    assert_eq!(recorder.count("init"), 1, "one lifecycle so far");
    let mark = recorder.messages().len();

    let second = handle
        .load(scenario, None, None)
        .expect("second load, no unload");

    let window = recorder.messages().split_off(mark);
    let at = |what: &str| window.iter().position(|m| m == what);
    let destroy = at("destroy_instance").unwrap_or_else(|| panic!("no destroy in {window:?}"));
    let shutdown = at("shutdown").unwrap_or_else(|| panic!("no shutdown in {window:?}"));
    let init = at("init").unwrap_or_else(|| panic!("no init in {window:?}"));
    let create = at("create_behavior").unwrap_or_else(|| panic!("no create in {window:?}"));
    assert!(
        destroy < shutdown && shutdown < init && init < create,
        "old session first, then the new lifecycle: {window:?}"
    );
    assert_eq!(
        window.iter().filter(|m| *m == "init").count(),
        1,
        "{window:?}"
    );
    assert_eq!(
        window.iter().filter(|m| *m == "shutdown").count(),
        1,
        "{window:?}"
    );
    assert_eq!(
        recorder.count("init"),
        2,
        "init ran again, once per lifecycle"
    );
    assert_eq!(second.session, first.session + 1);
}

#[test]
fn validate_path_with_plugins_reuses_a_live_session_image_without_init() {
    // D-a: validation next to a live session shares its image: no `init`, no `shutdown`, no
    // lifecycle of its own. With no session, validation starts and ends its own: one `init`, one
    // `shutdown`. The session's messages come through `HandleConfig::plugin_log` (one `init`);
    // validation builds its own host, which logs to `tracing`, so its messages are read there.
    let _lock = plugin_lock();
    let capture_log = trace::install();
    let (recorder, hook) = PluginRecorder::new();
    let fixture = plugin_support::fixture(&[]);
    let mut with_fixture = config();
    with_fixture.load.extra_plugin_paths = vec![fixture.clone()];
    with_fixture.plugin_log = Some(hook);
    let (handle, _thread) = start_engine(with_fixture);
    let scenario = write_scenario(
        "validate-live",
        2.0,
        "",
        &[Tpl {
            id: "test.entity.life",
            behavior: "test.behavior.lifecycle",
            params: "",
        }],
        &[],
        &[("test.entity.life", "a")],
    );
    let registry = standard_registry().expect("registry");
    handle.load(scenario.clone(), None, None).expect("load");
    assert_eq!(
        recorder.count("init"),
        1,
        "the session started the lifecycle"
    );
    let live = capture_log.plugin_messages().len();

    let beside_session =
        validate_path_with_plugins(&registry, &scenario, std::slice::from_ref(&fixture));

    let beside = beside_session.expect("validate");
    assert!(beside.diagnostics.is_empty(), "{:?}", beside.diagnostics);
    let during = capture_log.plugin_messages().split_off(live);
    assert!(
        !during.iter().any(|m| m == "init" || m == "shutdown"),
        "image shared, no lifecycle of its own: {during:?}"
    );
    assert_eq!(
        recorder.count("init"),
        1,
        "the session's lifecycle is untouched"
    );
    assert_eq!(recorder.count("shutdown"), 0);
    assert_eq!(state_of(&handle), Ready, "the session is untouched");

    handle.unload().expect("unload");
    assert_eq!(
        recorder.count("shutdown"),
        1,
        "the unload ended the lifecycle"
    );
    let idle = capture_log.plugin_messages().len();
    let alone = validate_path_with_plugins(&registry, &scenario, std::slice::from_ref(&fixture));
    let alone = alone.expect("validate");
    assert!(alone.diagnostics.is_empty(), "{:?}", alone.diagnostics);
    let alone_log = capture_log.plugin_messages().split_off(idle);
    let count = |what: &str| alone_log.iter().filter(|m| *m == what).count();
    assert_eq!(
        (count("init"), count("shutdown")),
        (1, 1),
        "a lifecycle of its own: {alone_log:?}"
    );
}

#[test]
fn load_unload_load_plugin_wander() {
    let _lock = plugin_lock();
    let mut with_wander = config();
    with_wander.load.extra_plugin_paths = vec![plugin_support::wander()];
    let (handle, _thread) = start_engine(with_wander);
    let scenario = scenario_copy(&examples().join("plugin_wander"), "wander", |text| {
        let edited = text.replace("duration_s = 60.0", "duration_s = 4.0");
        assert_ne!(edited, text, "the duration line was not found");
        edited
    });

    let mut finals = Vec::new();
    for round in 0..2 {
        let info = handle.load(scenario.clone(), None, None).expect("load");
        assert_eq!(state_of(&handle), Ready, "round {round}");
        let sub = handle.subscribe(lossless());
        handle.start().expect("start");
        let frames = until_stopped(&sub);
        finals.push((info.entity_count, last_tick_hash(&frames)));
        handle.unload().expect("unload");
        assert_eq!(state_of(&handle), Idle, "round {round}");
    }

    assert_eq!(
        finals[0], finals[1],
        "the same plugin scenario runs the same twice"
    );
}

#[test]
fn invalid_scenario_is_session_error_with_diagnostics() {
    let (handle, _thread) = engine();
    let bad =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bad_scenario/scenario.toml");

    let result = handle.load(bad, None, None);

    let Err(HandleError::Session(SessionError::Scenario(ScenarioError::Invalid(diagnostics)))) =
        result
    else {
        panic!("expected a session error with diagnostics, got {result:?}");
    };
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["invalid_value", "unknown_template"]);
    assert!(diagnostics.iter().all(|d| d.location.is_some()));
}

// ---- queue, capacity, replies, panic ----

#[test]
fn full_queue_is_busy() {
    let mut small = config();
    small.command_capacity = 2;
    let (handle, _thread) = start_engine(small);
    let held = jam(&handle);
    let (first, first_rx) = capture::<EngineStatus>();
    let (second, second_rx) = capture::<EngineStatus>();
    handle
        .submit(Command::Status { reply: first })
        .expect("fits");
    handle
        .submit(Command::Status { reply: second })
        .expect("fits");

    let (third, third_rx) = capture::<EngineStatus>();
    let refused = handle.submit(Command::Status { reply: third });

    assert!(matches!(refused, Err(HandleError::Busy)), "{refused:?}");
    assert!(
        matches!(receive(&third_rx), Err(HandleError::Closed)),
        "the refused command's reply is dropped uncompleted"
    );
    held.release();
    assert!(receive(&first_rx).is_ok() && receive(&second_rx).is_ok());
}

#[test]
fn zero_command_capacity_is_refused() {
    let mut none = config();
    none.command_capacity = 0;

    let result = umbra_engine::handle::EngineHandle::spawn(none);

    assert!(matches!(result, Err(SpawnError::ZeroCapacity)));
}

#[test]
fn dropped_reply_reports_closed() {
    let (handle, thread) = engine();
    thread.shutdown().expect("shutdown");
    let (reply, rx) = capture::<EngineStatus>();

    let submitted = handle.submit(Command::Status { reply });

    assert!(
        matches!(submitted, Err(HandleError::Closed)),
        "{submitted:?}"
    );
    assert!(matches!(receive(&rx), Err(HandleError::Closed)));
}

#[test]
fn engine_thread_panic_makes_every_command_closed() {
    let (handle, thread) = engine();
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    handle
        .submit(Command::Status {
            reply: umbra_engine::handle::Reply::new(move |_result| {
                let _ = entered_tx.send(());
                let _ = go_rx.recv();
                panic!("test: the engine thread panics on purpose");
            }),
        })
        .expect("submit");
    entered_rx
        .recv_timeout(GUARD)
        .expect("the thread is inside the callback");
    let (queued, queued_rx) = capture::<EngineStatus>();
    handle
        .submit(Command::Status { reply: queued })
        .expect("queued behind");

    go_tx.send(()).expect("release");

    assert!(
        matches!(receive(&queued_rx), Err(HandleError::Closed)),
        "a queued reply is answered Closed when the thread dies"
    );
    assert!(matches!(handle.status(), Err(HandleError::Closed)));
    assert!(matches!(handle.start(), Err(HandleError::Closed)));
    assert!(matches!(handle.unload(), Err(HandleError::Closed)));
    assert!(matches!(thread.shutdown(), Err(HandleError::Closed)));
}

#[test]
fn queries_work_when_stopped() {
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    handle.stop().expect("stop");
    assert_eq!(state_of(&handle), Stopped);

    let page = handle.entities(umbra_engine::handle::EntityQuery::default());
    let entity = handle.entity(EntityId::new(1).expect("non-zero"));
    let templates = handle.templates();

    assert_eq!(page.expect("entities").total, 3);
    assert!(entity.is_ok());
    let templates = templates.expect("templates");
    assert!(!templates.is_empty(), "the scenario has templates");
}

#[test]
fn handle_default_tick_rate_applies_only_to_scenarios_without_one() {
    let mut other_default = config();
    other_default.load.default_tick_rate = TickRate::new(20).expect("rate");
    let (handle, _thread) = start_engine(other_default);
    let without = scenario_copy(&examples().join("walking_skeleton"), "no-rate", |text| {
        let edited = text.replace("tick_rate_hz = 30\n", "");
        assert_ne!(edited, text, "the rate line was not found");
        edited
    });

    let defaulted = handle
        .load(without, None, None)
        .expect("load without a rate");
    let explicit = handle
        .load(walking_skeleton(), None, None)
        .expect("load with a rate");

    assert_eq!(defaulted.tick_rate, TickRate::new(20).expect("rate"));
    assert_eq!(defaulted.tick_rate_source, TickRateSource::Default);
    assert_eq!(explicit.tick_rate, TickRate::new(30).expect("rate"));
    assert_eq!(explicit.tick_rate_source, TickRateSource::Scenario);
}

/// The fields of the engine thread's `tick` span (ADR 0024, m16): the tick and one wall time in
/// nanoseconds per phase, in phase order (ASM-177, ASM-187).
const TICK_SPAN_FIELDS: [&str; 7] = [
    "tick",
    "ingest_wall_ns",
    "perception_wall_ns",
    "behavior_wall_ns",
    "motion_wall_ns",
    "events_wall_ns",
    "publish_wall_ns",
];

fn assert_tick_spans(spans: &[trace::SpanSeen], at_least: usize) {
    assert!(
        spans.len() >= at_least,
        "one per tick (other tests' ticks may add more): {spans:?}"
    );
    for span in spans {
        assert_eq!(span.name, "tick");
        assert_eq!(span.level, tracing::Level::TRACE);
        assert_eq!(span.fields, TICK_SPAN_FIELDS, "{span:?}");
    }
}

#[test]
fn tick_trace_span_carries_phase_wall() {
    // m16, ASM-177, ASM-187: a TRACE-level span named `tick` per tick with the fields above.
    let capture_log = trace::install();
    let before = capture_log.spans().len();
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    handle.start().expect("start");

    guarded_step(&handle, 5).expect("step");

    assert_tick_spans(&capture_log.spans().split_off(before), 5);
}

#[test]
fn tick_trace_span_is_emitted_for_every_tick_of_a_running_run() {
    // extra (WI-8): the same span check on a run that needs no `step`.
    let capture_log = trace::install();
    let before = capture_log.spans().len();
    let (handle, _thread) = engine();
    let info = handle.load(short_skeleton(), None, None).expect("load");
    let sub = handle.subscribe(lossless());

    handle.start().expect("start");
    until_stopped(&sub);

    let spans = capture_log.spans().split_off(before);
    assert_tick_spans(&spans, usize::try_from(info.duration_ticks).expect("fits"));
}

#[test]
fn collect_until_sees_a_stop_frame_after_stop() {
    // extra: the shared wait helper used above terminates on the stop frame (guards the helper).
    let (handle, _thread) = engine();
    load_stepped(&handle, short_skeleton());
    let sub = handle.subscribe(lossless());
    handle.start().expect("start");
    handle.stop().expect("stop");

    let frames = collect_until(&sub, |f| f.run.state == Stopped);

    assert_eq!(
        frames.last().map(|f| f.run.stop_reason.clone()),
        Some(Some(StopReason::Command))
    );
}
