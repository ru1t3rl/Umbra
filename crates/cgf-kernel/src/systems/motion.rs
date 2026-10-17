//! The Motion phase system (REQ-K-2, REQ-K-5, REQ-M-1).

use cgf_core::{Intent, KinematicState};

use crate::pipeline::TickContext;
use crate::world::MotionRef;

/// Replaces `state` with the entity's motion model output for this tick.
pub(crate) fn step(
    model: &MotionRef,
    state: &mut KinematicState,
    intents: &[Intent],
    ctx: &TickContext<'_>,
) {
    *state = model.0.step(state, intents, ctx.dt, ctx.terrain);
}
