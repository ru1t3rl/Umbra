//! What the thread reports: the run status, the engine status and the frames it publishes.

use std::sync::atomic::Ordering;

use umbra_core::SimEvent;

use super::EngineStatus;
use super::thread::Worker;
use crate::bus::{Frame, FrameCause, RunState, RunStatus};

impl Worker {
    /// The run status now.
    pub(super) fn run_status(&self) -> RunStatus {
        let Some(run) = &self.run else {
            return RunStatus::new(RunState::Idle, self.idle_clock, self.sessions);
        };
        let engine = run.session.engine();
        let mut status = RunStatus::new(run.state, engine.clock_mode(), self.sessions);
        status.stop_reason.clone_from(&run.stop_reason);
        status.time = Some(engine.now());
        status.step_pending = run.step.is_some();
        status.tick_rate = Some(run.info.tick_rate);
        status.tick_rate_source = Some(run.info.tick_rate_source);
        status
    }

    /// The status the `status` command answers.
    pub(super) fn engine_status(&self) -> EngineStatus {
        EngineStatus {
            run: self.run_status(),
            scenario: self.run.as_ref().map(|r| r.info.clone()),
        }
    }

    /// Builds one frame of the current state (no world while idle), counting the build.
    pub(super) fn frame(
        &mut self,
        cause: FrameCause,
        events: Vec<SimEvent>,
        ticks: Option<Vec<u64>>,
        with_hash: bool,
    ) -> Frame {
        let seq = self.seq;
        self.seq += 1;
        self.shared.builds.fetch_add(1, Ordering::Relaxed);
        let mut frame = Frame::new(seq, cause, self.run_status());
        if let Some(run) = &self.run {
            let engine = run.session.engine();
            frame.snapshot = Some(engine.snapshot());
            frame.directory = std::sync::Arc::clone(&run.directory);
            frame.state_hash = with_hash.then(|| engine.state_hash());
        }
        // Without ticks given, the events are those of a command, which happened between ticks, at
        // the tick now.
        frame.event_ticks = ticks.unwrap_or_else(|| {
            let now = frame.snapshot.as_ref().map_or(0, |s| s.time.tick());
            vec![now; events.len()]
        });
        debug_assert_eq!(events.len(), frame.event_ticks.len(), "a tick per event");
        frame.events = events;
        frame
    }

    /// Publishes a control frame of the current state, carrying `events` (those of the command that
    /// caused it), to every subscription, if there are any. The frame carries the state hash when a
    /// subscription asks for hashes (ASM-191 d, M5 WI-18), so a client reads the final hash of a run
    /// from its stop frame.
    pub(super) fn publish_control(&mut self, events: Vec<SimEvent>) {
        let (any, any_hash) = self.bus.interest();
        if any {
            let frame = self.frame(FrameCause::Control, events, None, any_hash);
            self.bus.publish(frame, None);
        }
    }
}
