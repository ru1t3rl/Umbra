//! The M5 WI-6 surface of the snapshot bus and the engine handle (ADR 0024): the real types, their
//! defaults and `Reply`, plus the signatures the step-4 tests compile against. The stub bodies
//! (`todo!`) are exercised from WI-7 on.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use umbra_engine::bus::{
    Bus, Drained, Frame, FrameCause, RunState, RunStatus, SlotId, StopReason, SubscribeOptions,
    Subscription, SubscriptionStats,
};
use umbra_engine::handle::{
    Applied, Command, ComponentEntry, EngineHandle, EngineStatus, EngineThread, EntityPage,
    EntityQuery, EntityState, HandleConfig, HandleError, PluginReport, Reply, SessionInfo,
    SpawnError, StepOutcome,
};
use umbra_engine::{ClockMode, Registry, SimTime, TickRate};

fn registry() -> Registry {
    umbra_engine::builtins::builtin_registry().expect("builtin registry")
}

// ---- Reply ----

type Seen<T> = Arc<Mutex<Vec<Result<T, HandleError>>>>;

fn capture<T: Send + 'static>() -> (Reply<T>, Seen<T>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let reply = Reply::new(move |result| sink.lock().expect("lock").push(result));
    (reply, seen)
}

#[test]
fn reply_delivers_the_value_once() {
    let (reply, seen) = capture::<u32>();
    reply.complete(Ok(7));
    let seen = seen.lock().expect("lock");
    assert_eq!(seen.len(), 1, "exactly one completion, none from the drop");
    assert!(matches!(seen[0], Ok(7)));
}

#[test]
fn reply_delivers_an_error() {
    let (reply, seen) = capture::<u32>();
    reply.complete(Err(HandleError::NoSession));
    let seen = seen.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert!(matches!(seen[0], Err(HandleError::NoSession)));
}

#[test]
fn dropped_reply_reports_closed() {
    let (reply, seen) = capture::<u32>();
    assert!(
        seen.lock().expect("lock").is_empty(),
        "nothing before the drop"
    );
    drop(reply);
    let seen = seen.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert!(matches!(seen[0], Err(HandleError::Closed)), "{:?}", seen[0]);
}

#[test]
fn reply_inside_a_dropped_command_reports_closed() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let command = Command::Unload {
        reply: Reply::new(move |r: Result<(), HandleError>| {
            assert!(matches!(r, Err(HandleError::Closed)));
            counter.fetch_add(1, Ordering::SeqCst);
        }),
    };
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(command);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// ---- defaults ----

#[test]
fn subscribe_options_default_to_the_adr_values() {
    let options = SubscribeOptions::new();
    assert_eq!(options.capacity, NonZeroUsize::new(4).expect("4"));
    assert_eq!(options.min_tick_stride, NonZeroU64::new(1).expect("1"));
    assert_eq!(options.max_carry_events, 4096);
    assert!(!options.with_state_hash);
    assert!(options.notify.is_none());
    let same = SubscribeOptions::default();
    assert_eq!(same.capacity, options.capacity);
    assert_eq!(same.max_carry_events, options.max_carry_events);
}

#[test]
fn handle_config_defaults_to_the_adr_values() {
    let config = HandleConfig::new(registry());
    assert_eq!(config.command_capacity, 64);
    assert_eq!(config.stack_size, 8 * 1024 * 1024);
    assert_eq!(config.max_wait, Duration::from_millis(10));
    // The wall-clock factory yields a fresh clock per session.
    let first = (config.wall)();
    let second = (config.wall)();
    let _ = (first.now(), second.now());
}

#[test]
fn run_status_new_starts_without_optional_facts() {
    let status = RunStatus::new(RunState::Idle, ClockMode::AsFastAsPossible, 0);
    assert_eq!(status.state, RunState::Idle);
    assert_eq!(status.clock, ClockMode::AsFastAsPossible);
    assert_eq!(status.session, 0);
    assert!(status.stop_reason.is_none());
    assert!(status.time.is_none());
    assert!(!status.step_pending);
    assert!(status.tick_rate.is_none() && status.tick_rate_source.is_none());
    let time = SimTime::at_tick(TickRate::new(30).expect("rate"), 3);
    let mut running = RunStatus::new(RunState::Running, ClockMode::Stepped, 4);
    running.time = Some(time);
    running.stop_reason = Some(StopReason::Duration);
    assert_eq!(running.time.map(SimTime::tick), Some(3));
}

#[test]
fn frame_new_carries_its_parts() {
    let status = RunStatus::new(RunState::Ready, ClockMode::Stepped, 1);
    let frame = Frame::new(9, FrameCause::Control, status.clone());
    assert_eq!(frame.seq, 9);
    assert_eq!(frame.cause, FrameCause::Control);
    assert_eq!(frame.run, status);
    assert!(frame.snapshot.is_none() && frame.state_hash.is_none());
    assert!(frame.events.is_empty() && frame.directory.is_empty());
}

#[test]
fn handle_errors_have_distinct_messages() {
    let time = SimTime::start(TickRate::new(30).expect("rate"));
    let all = [
        HandleError::Busy,
        HandleError::Closed,
        HandleError::NoSession,
        HandleError::InvalidState {
            state: RunState::Paused,
            command: "start",
        },
        HandleError::NotStepped,
        HandleError::StepInProgress,
        HandleError::StepCancelled { reached: time },
    ];
    let messages: std::collections::BTreeSet<String> =
        all.iter().map(ToString::to_string).collect();
    assert_eq!(messages.len(), all.len(), "{messages:?}");
    assert!(all[3].to_string().contains("start"), "{}", all[3]);
    assert!(
        SpawnError::StackTooSmall {
            requested: 1,
            minimum: 4 << 20
        }
        .to_string()
        .contains('1')
    );
    assert_ne!(SpawnError::ZeroCapacity.to_string(), "");
}

#[test]
fn entity_query_defaults_list_everything_from_the_start() {
    let query = EntityQuery::default();
    assert_eq!(query.after_id, None);
    assert_eq!(query.limit, EntityQuery::DEFAULT_LIMIT);
    assert_eq!(EntityQuery::DEFAULT_LIMIT, 100);
    assert_eq!(EntityQuery::MAX_LIMIT, 1000);
    assert_eq!(query.ids, Vec::new());
    assert!(query.name.is_none() && query.force.is_none() && query.group.is_none());
    assert!(query.template.is_none() && query.ownership.is_none() && query.bbox.is_none());
}

// ---- signatures the step-4 tests compile against ----

#[test]
#[expect(clippy::type_complexity, reason = "the signatures are the point")]
fn signatures_compile() {
    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    assert_send_sync::<EngineHandle>();
    assert_clone::<EngineHandle>();
    assert_send_sync::<Subscription>();
    assert_send_sync::<Bus>();
    assert_send::<EngineThread>();
    assert_send::<Command>();
    assert_send_sync::<Frame>();

    // Never called: these only have to type-check.
    let _: fn(HandleConfig) -> Result<(EngineHandle, EngineThread), SpawnError> =
        EngineHandle::spawn;
    let _: fn(&EngineHandle, Command) -> Result<(), HandleError> = EngineHandle::submit;
    let _: fn(&EngineHandle, SubscribeOptions) -> Subscription = EngineHandle::subscribe;
    let _: fn(&EngineHandle) -> u64 = EngineHandle::snapshot_builds;
    let _: fn(EngineThread) -> Result<(), HandleError> = EngineThread::shutdown;
    let _: fn(&Subscription) -> Drained = Subscription::drain;
    let _: fn(&Subscription, Duration) -> Drained = Subscription::wait;
    let _: fn(&Subscription) -> SubscriptionStats = Subscription::stats;
    let _: fn(&Subscription) -> SlotId = Subscription::id;
    let _: fn(&Bus, u64) -> bool = Bus::wants_frame;
    let _: fn(&Bus, u64) -> bool = Bus::wants_state_hash;
    let _: fn(&Bus, Frame, Option<SlotId>) = Bus::publish;
    let _: fn(
        &EngineHandle,
        std::path::PathBuf,
        Option<u64>,
        Option<ClockMode>,
    ) -> Result<SessionInfo, HandleError> = EngineHandle::load;
    let _: fn(&EngineHandle, u64) -> Result<StepOutcome, HandleError> = EngineHandle::step;
    let _: fn(&EngineHandle) -> Result<RunStatus, HandleError> = EngineHandle::start;
    let _: fn(&EngineHandle) -> Result<(), HandleError> = EngineHandle::unload;
    let _: fn(&EngineHandle) -> Result<EngineStatus, HandleError> = EngineHandle::status;
    let _: fn(&EngineHandle, EntityQuery) -> Result<EntityPage, HandleError> =
        EngineHandle::entities;
    let _: fn(&EngineHandle, umbra_engine::EntityId) -> Result<EntityState, HandleError> =
        EngineHandle::entity;
    let _: fn(&EngineHandle, umbra_engine::EntityId) -> Result<Applied, HandleError> =
        EngineHandle::remove_entity;
    let _: fn(&EngineHandle) -> Result<PluginReport, HandleError> = EngineHandle::plugins;
    let _: fn(
        &EngineHandle,
        Option<umbra_engine::ComponentKind>,
    ) -> Result<Vec<ComponentEntry>, HandleError> = EngineHandle::components;
}
