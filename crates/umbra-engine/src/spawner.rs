//! Applies the `Spawn` and `Despawn` intents of a tick after `Kernel::tick` (REQ-B-2, REQ-C-2,
//! REQ-K-5, D-46, ADR 0052, ASM-600..606, ASM-644, ASM-645).
//!
//! Status: M7 WI-1 stub. Implemented and wired into `Engine::tick_once` in M7 WI-4; the
//! acceptance tests (`tests/spawn_despawn.rs`, `tests/spawn_determinism.rs`, the kernel's
//! `tests/lifecycle_report.rs`) are ignored until then.

#![expect(dead_code, reason = "M7 WI-1 stub: wired into the tick loop in WI-4")]

use umbra_core::{EntityId, Intent, SimEvent};

use crate::engine::Engine;

/// Most spawns applied per tick per engine; the excess is dropped in order (ASM-603).
pub const MAX_SPAWNS_PER_TICK: usize = 256;
/// Most entities an engine holds; spawns beyond it are dropped (ASM-603).
pub const MAX_ENTITIES: usize = 100_000;

/// Counters of the lifecycle intents an engine has handled (ASM-603, ASM-604).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LifecycleCounters {
    /// Spawns applied.
    pub spawned: u64,
    /// Despawns applied.
    pub despawned: u64,
    /// Spawns rejected (unknown template or a failed build).
    pub rejected: u64,
    /// Spawns dropped by the per-tick or total caps.
    pub capped: u64,
}

/// Applies lifecycle intents against the scenario's templates and graphs.
pub(crate) struct Spawner {
    _private: (),
}

impl Spawner {
    /// Applies all spawns of `lifecycle` in order, then all despawns, and appends the resulting
    /// `EntityCreated` and `EntityRemoved` events to `events` (ADR 0052).
    pub(crate) fn apply(
        &mut self,
        engine: &mut Engine,
        lifecycle: &[(EntityId, Intent)],
        events: &mut Vec<SimEvent>,
    ) {
        let _ = (engine, lifecycle, events);
        unimplemented!("M7 WI-4")
    }
}

impl Engine {
    /// The lifecycle counters of this engine since it was built (ASM-603, ASM-604).
    ///
    /// M7 WI-1 stub: all zero, which is the truth until the spawner is wired in (M7 WI-4).
    #[must_use]
    pub fn lifecycle_counters(&self) -> LifecycleCounters {
        LifecycleCounters::default()
    }
}
