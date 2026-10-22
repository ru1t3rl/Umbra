//! The Ingest system: dead reckoning of Remote entities (REQ-K-2, ADR 0032).

use crate::pipeline::TickContext;
use crate::world::World;

/// Moves every Remote entity with a track to `extrapolate(basis, now - basis time)` (ASM-267,
/// ASM-283). Owned entities are not touched, and a world without Remote entities does no work.
pub(crate) fn run(world: &mut World, ctx: &TickContext<'_>) {
    world.extrapolate_remotes(ctx.now);
}
