//! Applying commands on the engine thread (ADR 0026 command x state matrix). A command that
//! changes state publishes a control frame first and replies after (publish, then reply).

use std::path::Path;
use std::sync::Arc;

use umbra_core::ControlAction;
use umbra_kernel::ClockMode;

use super::thread::{EndingStep, Run, StepEnd, Worker};
use super::{Command, HandleError, Reply, SessionInfo};
use crate::bus::{RunState, StopReason};
use crate::record::RecordOptions;
use crate::session::{Session, SessionOptions};

/// A state of the matrix in which `command` is refused with `invalid_state`.
pub(super) fn need(
    run: &Run,
    allowed: &[RunState],
    command: &'static str,
) -> Result<(), HandleError> {
    if allowed.contains(&run.state) {
        Ok(())
    } else {
        Err(HandleError::InvalidState {
            state: run.state,
            command,
        })
    }
}

impl Worker {
    /// Applies one command.
    pub(super) fn apply(&mut self, command: Command) {
        match command {
            Command::Load {
                path,
                seed,
                clock,
                reply,
            } => self.load(&path, seed, clock, reply),
            Command::Unload { reply } => {
                let done = self.unload();
                self.settle(done, reply, |_| ());
            }
            Command::Start { reply } => {
                let done = self.start();
                self.settle(done, reply, Worker::run_status);
            }
            Command::Pause { reply } => {
                let done = self.pause();
                self.settle(done, reply, Worker::run_status);
            }
            Command::Resume { reply } => {
                let done = self.resume();
                self.settle(done, reply, Worker::run_status);
            }
            Command::Stop { reply } => {
                let done = self.stop();
                self.settle(done, reply, Worker::run_status);
            }
            Command::SetClock { mode, reply } => {
                let done = self.set_clock(mode);
                self.settle(done, reply, Worker::run_status);
            }
            Command::Status { reply } => reply.complete(Ok(self.engine_status())),
            Command::Prime | Command::Shutdown => {}
            Command::Step { count, reply } => self.begin_step(count, reply),
            Command::Spawn { spec, reply } => self.spawn_entity(spec, reply),
            Command::Remove { id, reply } => self.remove_entity(id, reply),
            Command::Entities { query, reply } => reply.complete(self.entities(&query)),
            Command::Entity { id, reply } => reply.complete(self.entity(id)),
            Command::Templates { reply } => reply.complete(self.templates()),
            Command::Components { kind, reply } => reply.complete(Ok(self.components(kind))),
            Command::Catalog { reply } => reply.complete(Ok(self.catalog())),
            Command::Plugins { reply } => reply.complete(Ok(self.plugins())),
            Command::NetworkAdapters { reply } => reply.complete(self.network_adapters()),
            Command::SetAdapterEnabled {
                name,
                enabled,
                reply,
            } => reply.complete(self.set_adapter_enabled(&name, enabled)),
            Command::ValidateGraph {
                name,
                document,
                reply,
            } => reply.complete(self.validate_graph(&name, &document)),
        }
    }

    /// Publishes the control frame of a state change, then answers the step it ended (if any) and
    /// the command; an error answers alone.
    fn settle<T>(
        &mut self,
        done: Result<Option<EndingStep>, HandleError>,
        reply: Reply<T>,
        value: impl FnOnce(&Worker) -> T,
    ) {
        match done {
            Ok(ending) => {
                self.publish_control_then(Vec::new(), ending);
                reply.complete(Ok(value(self)));
            }
            Err(error) => reply.complete(Err(error)),
        }
    }

    pub(super) fn run_mut(&mut self) -> Result<&mut Run, HandleError> {
        self.run.as_mut().ok_or(HandleError::NoSession)
    }

    fn start(&mut self) -> Result<Option<EndingStep>, HandleError> {
        let run = self.run_mut()?;
        need(run, &[RunState::Ready], "start")?;
        run.session.engine_mut().resume();
        run.state = RunState::Running;
        Ok(self.control_or_stop(ControlAction::Start))
    }

    fn pause(&mut self) -> Result<Option<EndingStep>, HandleError> {
        let run = self.run_mut()?;
        need(run, &[RunState::Running], "pause")?;
        run.session.engine_mut().pause();
        run.state = RunState::Paused;
        let ending = run.take_step(StepEnd::Cancel);
        let stopped = self.control_or_stop(ControlAction::Pause);
        Ok(ending.or(stopped))
    }

    fn resume(&mut self) -> Result<Option<EndingStep>, HandleError> {
        let run = self.run_mut()?;
        need(run, &[RunState::Paused], "resume")?;
        run.session.engine_mut().resume();
        run.state = RunState::Running;
        Ok(self.control_or_stop(ControlAction::Resume))
    }

    fn stop(&mut self) -> Result<Option<EndingStep>, HandleError> {
        let run = self.run_mut()?;
        need(
            run,
            &[RunState::Ready, RunState::Running, RunState::Paused],
            "stop",
        )?;
        Ok(self.stop_with(StopReason::Command, StepEnd::Incomplete))
    }

    fn set_clock(&mut self, mode: ClockMode) -> Result<Option<EndingStep>, HandleError> {
        let run = self.run_mut()?;
        need(
            run,
            &[RunState::Ready, RunState::Running, RunState::Paused],
            "set_clock",
        )?;
        run.session.engine_mut().set_clock(mode);
        Ok((mode != ClockMode::Stepped)
            .then(|| run.take_step(StepEnd::Cancel))
            .flatten())
    }

    fn unload(&mut self) -> Result<Option<EndingStep>, HandleError> {
        let ending = self.run_mut()?.take_step(StepEnd::Cancel);
        self.drop_session();
        Ok(ending)
    }

    /// Replaces the session (review D-a): the old one is dropped first, so a failed load leaves
    /// `idle` and a reload of the same plugin starts a new image lifecycle.
    fn load(
        &mut self,
        path: &Path,
        seed: Option<u64>,
        clock: Option<ClockMode>,
        reply: Reply<SessionInfo>,
    ) {
        // Cancel the pending step first, then drop the old session, then load.
        let pending = self.run.as_mut().and_then(|r| r.take_step(StepEnd::Cancel));
        self.finish_step(pending);
        self.drop_session();
        self.sessions += 1;
        let mut options = SessionOptions::new((self.settings.wall)());
        options.seed = seed;
        options.clock = clock;
        options.load = self.settings.load.clone();
        options.plugin_log.clone_from(&self.settings.plugin_log);
        let result = match Session::load(&self.settings.base, path, options) {
            Ok(mut session) => {
                self.start_recording(&mut session, seed);
                Ok(self.install(session, seed))
            }
            Err(error) => Err(HandleError::Session(self.shown_session_error(error))),
        };
        self.publish_control(Vec::new());
        reply.complete(result);
    }

    /// With a record directory configured, records this load to `session-<n>.umbrarec.jsonl`
    /// (ASM-632). A recording that cannot be started is logged and the session runs unrecorded.
    fn start_recording(&self, session: &mut Session, seed: Option<u64>) {
        let Some(dir) = &self.settings.record_dir else {
            return;
        };
        let file = dir.join(format!("session-{}.umbrarec.jsonl", self.sessions));
        let mut options = RecordOptions::new(&file);
        // Never truncate a recording under a record directory (ASM-736).
        options.create_new = true;
        if let Err(error) = session.start_recording(options, seed) {
            tracing::error!(file = %file.display(), "cannot record this session: {error}");
        }
    }

    /// Makes `session` the current run, `ready`, and describes it.
    fn install(&mut self, session: Session, seed: Option<u64>) -> SessionInfo {
        let scenario = session.scenario();
        let shown = self.shown_path(session.path());
        let info = SessionInfo {
            session: self.sessions,
            name: scenario.name.clone(),
            path: shown,
            seed: seed.unwrap_or(scenario.seed),
            tick_rate: scenario.tick_rate,
            tick_rate_source: scenario.tick_rate_source,
            duration_ticks: session.duration_ticks(),
            entity_count: session.entities().len(),
            warnings: scenario
                .warnings
                .iter()
                .map(|w| self.shown_diagnostic(w.clone()))
                .collect(),
        };
        self.bus.set_tick_rate(Some(info.tick_rate));
        self.run = Some(Run {
            directory: Arc::new(session.directory()),
            directory_ghosts: 0,
            directory_version: session.engine().directory_version(),
            session,
            info: info.clone(),
            state: RunState::Ready,
            stop_reason: None,
            step: None,
            held: Vec::new(),
            held_ticks: Vec::new(),
        });
        info
    }
}
