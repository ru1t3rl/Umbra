//! The engine thread: the only code that touches an `Engine` (ADR 0024, REQ-X-1, REQ-K-3).
//!
//! One turn of the loop applies every queued command in arrival order (between ticks), serves the
//! priming marks, and then either asks the engine for one pacing decision or parks on the queue.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use umbra_core::{EntityId, Registry, SimEvent, SimTime};
use umbra_kernel::{ClockMode, TickReport, WallClock};
use umbra_plugin_host::PluginLog;

use super::shutdown::Shared;
use super::{Command, HandleError, Reply};
use super::{HandleConfig, SessionInfo, StepOutcome};
use crate::bus::{Bus, FrameCause, RunState, StopReason};
use crate::engine::Pumped;
use crate::error::EngineError;
use crate::plugins::PluginFault;
use crate::scenario::{EntityInfo, LoadSettings};
use crate::session::Session;

/// What each load needs, taken from [`HandleConfig`].
pub(super) struct Settings {
    pub(super) base: Registry,
    pub(super) load: LoadSettings,
    pub(super) wall: Arc<dyn Fn() -> Box<dyn WallClock> + Send + Sync>,
    pub(super) plugin_log: Option<Arc<dyn PluginLog>>,
    pub(super) max_wait: Duration,
}

/// A `step` waiting for its ticks.
pub(super) struct PendingStep {
    pub(super) reply: Reply<StepOutcome>,
    /// The ticks asked for.
    pub(super) requested: u64,
    /// The time when the step was applied.
    pub(super) applied_at: SimTime,
    /// The step is done once a tick reaches this number.
    pub(super) target_tick: u64,
}

/// How a pending step ends when its run does something else first (ADR 0026 "Step results").
#[derive(Clone, Copy)]
pub(super) enum StepEnd {
    /// `completed: true`: a tick reached the target.
    Complete,
    /// `completed: false`: a `stop` or the duration ended the run.
    Incomplete,
    /// `StepCancelled`: pause, clock change, fault, error or unload.
    Cancel,
}

/// A step taken out of its run, to be answered after the frame of the change that ended it
/// (publish, then reply).
pub(super) struct EndingStep {
    step: PendingStep,
    end: StepEnd,
    reached: SimTime,
}

/// The loaded scenario and where its run stands.
pub(super) struct Run {
    pub(super) session: Session,
    pub(super) info: SessionInfo,
    /// Shared between frames while the entity set is unchanged.
    pub(super) directory: Arc<BTreeMap<EntityId, EntityInfo>>,
    pub(super) state: RunState,
    pub(super) stop_reason: Option<StopReason>,
    pub(super) step: Option<PendingStep>,
    /// Events of ticks that built no frame, in order, for the next frame that is built (at most
    /// [`HELD_EVENTS_CAP`]; later ones are dropped).
    pub(super) held: Vec<SimEvent>,
    /// The tick of each of `held` (ASM-205).
    pub(super) held_ticks: Vec<u64>,
}

/// The most events kept for a frame that is not built yet.
const HELD_EVENTS_CAP: usize = 65_536;

impl Run {
    /// Takes the pending step, unanswered, with the time reached now. The ticks it still had
    /// queued in the clock are dropped, so a later `resume` does not run them.
    pub(super) fn take_step(&mut self, end: StepEnd) -> Option<EndingStep> {
        let step = self.step.take()?;
        self.session.engine_mut().cancel_steps();
        let reached = self.session.engine().now();
        Some(EndingStep { step, end, reached })
    }
}

/// The state of the engine thread.
pub(super) struct Worker {
    pub(super) settings: Settings,
    rx: Receiver<Command>,
    pub(super) bus: Arc<Bus>,
    pub(super) shared: Arc<Shared>,
    /// Frame counter, increasing over the thread's life.
    pub(super) seq: u64,
    /// Loads attempted so far; the session number of the current run.
    pub(super) sessions: u64,
    /// The clock reported while no scenario is loaded: that of the last session.
    pub(super) idle_clock: ClockMode,
    /// `None` is the `idle` state.
    pub(super) run: Option<Run>,
}

/// Logs a panic of the engine thread: the thread is not restarted, every later command is `Closed`.
struct PanicLog {
    bus: Arc<Bus>,
    shared: Arc<Shared>,
}

impl Drop for PanicLog {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::error!("the engine thread panicked; every command now fails with Closed");
        }
        // However the thread ended: say so, then wake the subscribers to look (ASM-206).
        self.shared.mark_closed();
        self.bus.wake_all();
    }
}

/// The body of the engine thread.
pub(super) fn run(config: HandleConfig, rx: Receiver<Command>, bus: Arc<Bus>, shared: Arc<Shared>) {
    let _log = PanicLog {
        bus: Arc::clone(&bus),
        shared: Arc::clone(&shared),
    };
    let HandleConfig {
        base_registry,
        load,
        max_wait,
        wall,
        plugin_log,
        ..
    } = config;
    let worker = Worker {
        settings: Settings {
            base: base_registry,
            load,
            wall,
            plugin_log,
            max_wait,
        },
        rx,
        bus,
        shared,
        seq: 0,
        sessions: 0,
        idle_clock: ClockMode::RealTime,
        run: None,
    };
    worker.run_loop();
}

impl Worker {
    fn run_loop(mut self) {
        while !self.shared.is_shutting_down() {
            if !self.drain_commands() {
                break;
            }
            // After the commands: a mark set while the queue was full is seen here, before parking.
            self.serve_primes();
            if !self.is_running() {
                if !self.wait_for_command() {
                    break;
                }
                continue;
            }
            if !self.pump_once() {
                break;
            }
        }
        self.shut_down();
    }

    /// The end of the thread: a step pending at shutdown is answered `Closed` (dropping its reply
    /// does that, review M12), then the session is dropped.
    fn shut_down(&mut self) {
        if let Some(run) = self.run.as_mut() {
            drop(run.step.take());
        }
        self.drop_session();
    }

    fn is_running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.state == RunState::Running)
    }

    /// Applies what is queued, in arrival order; `false` when the thread must exit: every handle is
    /// gone, or the shutdown flag is set. The flag is re-read after the last receive, because the
    /// `Shutdown` wake-up may have been the command just taken and ignored.
    fn drain_commands(&mut self) -> bool {
        loop {
            if self.shared.is_shutting_down() {
                return false;
            }
            match self.rx.try_recv() {
                Ok(command) => self.apply(command),
                Err(TryRecvError::Empty) => return !self.shared.is_shutting_down(),
                Err(TryRecvError::Disconnected) => return false,
            }
        }
    }

    /// Parks until a command arrives and applies it; `false` when the thread must exit. The flag is
    /// checked first: a wake-up consumed earlier must not leave the thread parked.
    fn wait_for_command(&mut self) -> bool {
        if self.shared.is_shutting_down() {
            return false;
        }
        match self.rx.recv() {
            Ok(command) => {
                self.apply(command);
                true
            }
            Err(_) => false,
        }
    }

    /// One pacing decision of the running engine; `false` when parked on a queue that disconnected.
    fn pump_once(&mut self) -> bool {
        let max_wait = self.settings.max_wait;
        let Some(run) = self.run.as_mut() else {
            return true;
        };
        match run.session.engine_mut().pump(max_wait) {
            Ok(Pumped::Ticked(report)) => self.after_tick(report),
            Ok(Pumped::Aborted { report, fault }) => self.after_abort(report, &fault),
            Ok(Pumped::Waited) => {}
            Ok(Pumped::Idle) => return self.wait_for_command(),
            Err(error) => self.after_error(&error),
        }
        true
    }

    fn after_tick(&mut self, report: TickReport) {
        trace_tick(&report);
        let tick = report.time.tick();
        let stop_now = self
            .run
            .as_ref()
            .is_some_and(|r| tick >= r.info.duration_ticks);
        let step_done = self
            .run
            .as_ref()
            .and_then(|r| r.step.as_ref())
            .is_some_and(|s| tick >= s.target_tick);
        self.publish_tick(report, step_done || stop_now);
        if step_done {
            // Answered before the stop: a step that finishes on the duration tick completed.
            let done = self
                .run
                .as_mut()
                .and_then(|r| r.take_step(StepEnd::Complete));
            self.finish_step(done);
        }
        if stop_now {
            let ending = self.stop_with(StopReason::Duration, StepEnd::Incomplete);
            self.publish_control_then(Vec::new(), ending);
        }
    }

    /// A plugin fault under `abort` (ADR 0021): the faulting tick's frame is kept, then the run stops.
    fn after_abort(&mut self, report: TickReport, fault: &PluginFault) {
        trace_tick(&report);
        self.publish_tick(report, true);
        let ending = self.stop_with(
            self.shown_stop_reason(StopReason::PluginFault(fault.clone())),
            StepEnd::Cancel,
        );
        self.publish_control_then(Vec::new(), ending);
    }

    fn after_error(&mut self, error: &EngineError) {
        let ending = self.stop_with(
            self.shown_stop_reason(StopReason::Error(error.to_string())),
            StepEnd::Cancel,
        );
        self.publish_control_then(Vec::new(), ending);
    }

    /// Builds and publishes the frame of a tick. Built only if a subscription is due one (cause
    /// `Tick`), or if `forced` (a step completes, or the run ends) and anyone subscribes (cause
    /// `Control` when no subscription was due).
    fn publish_tick(&mut self, report: TickReport, forced: bool) {
        let tick = report.time.tick();
        let (due, hash_due) = self.bus.due(tick);
        let (build, hash) = if forced {
            let (any, any_hash) = self.bus.interest();
            (any, hash_due || any_hash)
        } else {
            (due, hash_due)
        };
        if build {
            // A frame built only because the step or the run ends here is a control frame (the
            // bus doc: a step completion, a pause, a stop); one that was due is a tick frame.
            let cause = if due {
                FrameCause::Tick
            } else {
                FrameCause::Control
            };
            let held = self.run.as_mut().map(|r| {
                (
                    std::mem::take(&mut r.held),
                    std::mem::take(&mut r.held_ticks),
                )
            });
            let (mut events, mut ticks) = held.unwrap_or_default();
            ticks.extend(std::iter::repeat_n(tick, report.events.len()));
            events.extend(report.events);
            let frame = self.frame(cause, events, Some(ticks), hash);
            self.bus.publish(frame, None);
        } else if let Some(run) = self.run.as_mut().filter(|_| self.bus.interest().0) {
            // Nobody is due a frame, but somebody is waiting for the events of this tick: they
            // join the next frame that is built.
            let room = HELD_EVENTS_CAP.saturating_sub(run.held.len());
            let kept = report.events.len().min(room);
            run.held_ticks.extend(std::iter::repeat_n(tick, kept));
            run.held.extend(report.events.into_iter().take(room));
        }
    }

    /// Ends the run in `stopped` and takes its pending step; the caller publishes the control frame
    /// and then answers the step ([`publish_control_then`](Self::publish_control_then)).
    pub(super) fn stop_with(&mut self, reason: StopReason, end: StepEnd) -> Option<EndingStep> {
        let run = self.run.as_mut()?;
        run.session.engine_mut().pause();
        run.state = RunState::Stopped;
        run.stop_reason = Some(reason);
        run.take_step(end)
    }

    /// Publishes a control frame (with the events of the command that caused it), then answers the
    /// step that change ended: publish, then reply.
    pub(super) fn publish_control_then(
        &mut self,
        events: Vec<SimEvent>,
        ending: Option<EndingStep>,
    ) {
        self.publish_control(events);
        self.finish_step(ending);
    }

    /// Answers a taken step: `completed` true or false with the state now, or `StepCancelled`.
    pub(super) fn finish_step(&mut self, ending: Option<EndingStep>) {
        let Some(EndingStep { step, end, reached }) = ending else {
            return;
        };
        match (end, self.run.as_ref()) {
            (StepEnd::Complete | StepEnd::Incomplete, Some(run)) => {
                let outcome = StepOutcome {
                    requested: step.requested,
                    completed: matches!(end, StepEnd::Complete),
                    applied_at: step.applied_at,
                    reached,
                    state_hash: run.session.engine().state_hash(),
                    status: self.run_status(),
                };
                step.reply.complete(Ok(outcome));
            }
            _ => step
                .reply
                .complete(Err(HandleError::StepCancelled { reached })),
        }
    }

    /// Drops the session on this thread, so plugin teardown runs here and before anything that
    /// follows (review D-a). A pending step must have been taken first.
    pub(super) fn drop_session(&mut self) {
        self.bus.set_tick_rate(None);
        if let Some(run) = self.run.take() {
            self.idle_clock = run.session.engine().clock_mode();
        }
    }
}

/// The per-tick trace span of REQ-X-3 (ADR 0024, m16): one field per phase, in nanoseconds.
fn trace_tick(report: &TickReport) {
    let [ingest, perception, behavior, motion, events, publish] =
        report.phase_wall.map(|d| d.as_nanos());
    drop(
        tracing::trace_span!(
            "tick",
            tick = report.time.tick(),
            ingest_wall_ns = ingest,
            perception_wall_ns = perception,
            behavior_wall_ns = behavior,
            motion_wall_ns = motion,
            events_wall_ns = events,
            publish_wall_ns = publish,
        )
        .entered(),
    );
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::builtins::standard_registry;

    fn worker(rx: Receiver<Command>) -> Worker {
        Worker {
            settings: Settings {
                base: standard_registry().expect("registry"),
                load: LoadSettings::default(),
                wall: Arc::new(|| -> Box<dyn WallClock> {
                    Box::new(umbra_kernel::StdWallClock::new())
                }),
                plugin_log: None,
                max_wait: Duration::from_millis(1),
            },
            rx,
            bus: Arc::new(Bus::new()),
            shared: Arc::new(Shared::new()),
            seq: 0,
            sessions: 0,
            idle_clock: ClockMode::RealTime,
            run: None,
        }
    }

    /// B1: the `Shutdown` wake-up taken (and ignored) by the drain must not leave the thread
    /// parked: the drain reports exit because the flag was set before the wake-up was sent.
    #[test]
    fn drain_reports_exit_when_the_wake_up_was_consumed() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut worker = worker(rx);
        // The race: the flag is read false, then set, and `Shutdown` is sent; model the state after.
        worker
            .shared
            .shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        tx.send(Command::Shutdown).expect("queued");

        assert!(!worker.drain_commands());
        assert!(!worker.wait_for_command(), "and parking is refused");
    }

    /// B1, the second half: a consumed wake-up with the flag set after the last `try_recv` still
    /// ends the drain (no command left, flag set).
    #[test]
    fn drain_of_an_empty_queue_reports_exit_when_the_flag_is_set() {
        let (_tx, rx) = mpsc::sync_channel::<Command>(4);
        let mut worker = worker(rx);
        assert!(
            worker.drain_commands(),
            "no flag, nothing queued: keep going"
        );
        worker
            .shared
            .shutdown
            .store(true, std::sync::atomic::Ordering::Release);

        assert!(!worker.drain_commands());
    }

    // ---- step replies come after the frame of the change that ended them (M1, M2) ----

    type Seen = (Result<StepOutcome, HandleError>, Option<RunState>);

    /// A worker with `walking_skeleton` loaded (stepped) and running, a subscription, and a pending
    /// step whose reply records the newest frame's state at the moment it is called.
    fn running_with_step() -> (Worker, mpsc::Receiver<Seen>) {
        let (_tx, rx) = mpsc::sync_channel(4);
        let mut worker = worker(rx);
        let subscription = Arc::new(worker.bus.subscribe(crate::bus::SubscribeOptions::new()));
        let scenario = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scenarios/walking_skeleton/scenario.toml");
        let ignore = || Reply::new(|_: Result<SessionInfo, HandleError>| {});
        worker.apply(Command::Load {
            path: scenario,
            seed: None,
            clock: Some(ClockMode::Stepped),
            reply: ignore(),
        });
        worker.apply(Command::Start {
            reply: Reply::new(|_: Result<crate::bus::RunStatus, HandleError>| {}),
        });
        let (seen_tx, seen_rx) = mpsc::channel();
        let reply = Reply::new(move |result| {
            let state = subscription.drain().frames.last().map(|f| f.run.state);
            drop(seen_tx.send((result, state)));
        });
        let run = worker.run.as_mut().expect("loaded");
        let applied_at = run.session.engine().now();
        run.step = Some(PendingStep {
            reply,
            requested: 5,
            applied_at,
            target_tick: 5,
        });
        (worker, seen_rx)
    }

    fn the_step_ending(command: impl FnOnce(&mut Worker)) -> Seen {
        let (mut worker, seen) = running_with_step();
        command(&mut worker);
        seen.try_recv().expect("the step was answered")
    }

    fn ignore_status() -> Reply<crate::bus::RunStatus> {
        Reply::new(|_| {})
    }

    #[test]
    fn pause_cancels_the_step_after_its_frame() {
        let (result, state) = the_step_ending(|w| {
            w.apply(Command::Pause {
                reply: ignore_status(),
            });
        });

        assert!(matches!(result, Err(HandleError::StepCancelled { .. })));
        assert_eq!(state, Some(RunState::Paused), "the frame was already there");
    }

    #[test]
    fn set_clock_away_from_stepped_cancels_the_step_after_its_frame() {
        let (result, state) = the_step_ending(|w| {
            w.apply(Command::SetClock {
                mode: ClockMode::AsFastAsPossible,
                reply: ignore_status(),
            });
        });

        assert!(matches!(result, Err(HandleError::StepCancelled { .. })));
        assert_eq!(state, Some(RunState::Running));
    }

    #[test]
    fn stop_ends_the_step_incomplete_after_its_frame() {
        let (result, state) = the_step_ending(|w| {
            w.apply(Command::Stop {
                reply: ignore_status(),
            });
        });

        let outcome = result.expect("completed: false is an answer, not an error");
        assert!(!outcome.completed);
        assert_eq!(outcome.requested, 5);
        assert_eq!(outcome.status.state, RunState::Stopped);
        assert_eq!(state, Some(RunState::Stopped));
    }

    #[test]
    fn unload_cancels_the_step_after_its_idle_frame() {
        let (result, state) = the_step_ending(|w| {
            w.apply(Command::Unload {
                reply: Reply::new(|_| {}),
            });
        });

        assert!(matches!(result, Err(HandleError::StepCancelled { .. })));
        assert_eq!(state, Some(RunState::Idle));
    }

    /// ASM-212 (review M1): the bus learns the tick rate from load and unload even when nobody is
    /// subscribed, so a rate-following subscription is right before its priming frame is served.
    #[test]
    fn load_and_unload_set_the_bus_tick_rate_without_a_subscriber() {
        let (_tx, rx) = mpsc::sync_channel(4);
        let mut worker = worker(rx);
        let scenario = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scenarios/walking_skeleton/scenario.toml");
        worker.apply(Command::Load {
            path: scenario,
            seed: None,
            clock: Some(ClockMode::Stepped),
            reply: Reply::new(|_: Result<SessionInfo, HandleError>| {}),
        });
        let mut options = crate::bus::SubscribeOptions::new();
        options.max_rate_hz = Some(5.0);
        let subscription = worker.bus.subscribe(options);
        // The skeleton runs at 30 Hz: every 6th tick at 5 Hz, and no frame has been published.
        assert!(subscription.drain().frames.is_empty());
        assert!(!worker.bus.wants_frame(5) && worker.bus.wants_frame(6));
        worker.apply(Command::Unload {
            reply: Reply::new(|_| {}),
        });
        assert!(worker.bus.wants_frame(1), "unloaded: the fixed stride");
    }

    #[test]
    fn shutdown_answers_a_pending_step_closed() {
        let (mut worker, seen) = running_with_step();

        worker.shut_down();

        let (result, _) = seen.try_recv().expect("answered");
        assert!(matches!(result, Err(HandleError::Closed)), "{result:?}");
    }
}
