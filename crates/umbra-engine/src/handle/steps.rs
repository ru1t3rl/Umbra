//! `step` (ADR 0026 "Step results", review M10): run ticks of a stepped clock, one step pending
//! at a time. The step is answered by the code that ends it: a tick reaching the target
//! (`after_tick`), or a state change ([`Worker::publish_control_then`]).

use umbra_kernel::ClockMode;

use super::commands::need;
use super::thread::{PendingStep, StepEnd, Worker};
use super::{HandleError, Reply, StepOutcome};
use crate::bus::RunState;

impl Worker {
    /// Starts a step of `count` ticks, or refuses it. Valid only while `running` on the stepped
    /// clock with no step pending; the reply is kept until the step ends.
    pub(super) fn begin_step(&mut self, count: u64, reply: Reply<StepOutcome>) {
        match self.queue_step(count, reply) {
            // A step of no ticks is done as soon as it is applied.
            Ok(()) if count == 0 => {
                let done = self
                    .run
                    .as_mut()
                    .and_then(|r| r.take_step(StepEnd::Complete));
                self.finish_step(done);
            }
            Ok(()) => {}
            Err((error, reply)) => reply.complete(Err(error)),
        }
    }

    /// Checks the matrix cell and records the pending step; the reply comes back with the error.
    fn queue_step(
        &mut self,
        count: u64,
        reply: Reply<StepOutcome>,
    ) -> Result<(), (HandleError, Reply<StepOutcome>)> {
        let Some(run) = self.run.as_mut() else {
            return Err((HandleError::NoSession, reply));
        };
        let checked = need(run, &[RunState::Running], "step").and_then(|()| {
            if run.session.engine().clock_mode() != ClockMode::Stepped {
                Err(HandleError::NotStepped)
            } else if run.step.is_some() {
                Err(HandleError::StepInProgress)
            } else {
                Ok(())
            }
        });
        if let Err(error) = checked {
            return Err((error, reply));
        }
        let applied_at = run.session.engine().now();
        run.session.engine_mut().request_steps(count);
        run.step = Some(PendingStep {
            reply,
            requested: count,
            applied_at,
            target_tick: applied_at.tick().saturating_add(count),
        });
        Ok(())
    }
}
