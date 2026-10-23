//! The records after the header (ADR 0053, ASM-626, ASM-627).

use umbra_core::net::NetMessage;
use umbra_core::{EntityId, Force, MetersPerSecond, Position, Radians, StateHash, TemplateId};

/// One line after the header.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Record {
    /// A command applied between two ticks.
    Command(CommandRecord),
    /// Messages one adapter delivered in one tick.
    Inbound(InboundRecord),
    /// A network adapter fault seen live.
    Fault(FaultRecord),
    /// The state hash after a tick.
    Hash {
        /// The tick that just ran.
        tick: u64,
        /// The hash, in the header's version.
        hash: StateHash,
    },
    /// The end of the run.
    End(EndRecord),
}

/// A command and where it was applied.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord {
    /// Ticks completed when the command was applied.
    pub before_tick: u64,
    /// Application order within one boundary.
    pub seq: u64,
    /// The command.
    pub command: RecordedCommand,
}

/// A spawn as recorded: the full template spawn (ASM-626).
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnRecord {
    /// The template.
    pub template: TemplateId,
    /// Where the entity started.
    pub position: Position,
    /// Initial heading.
    pub heading: Radians,
    /// Initial speed.
    pub speed: MetersPerSecond,
    /// Optional unique name.
    pub name: Option<String>,
    /// Optional group.
    pub group: Option<String>,
    /// Optional force.
    pub force: Option<Force>,
}

/// A recorded command (ASM-626): the first three change state, the rest are informational.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RecordedCommand {
    /// Spawn from a template; `result` is the id the live run assigned.
    Spawn {
        /// What was spawned.
        spec: SpawnRecord,
        /// The id it received.
        result: EntityId,
    },
    /// Remove an entity.
    Remove {
        /// The entity.
        entity: EntityId,
    },
    /// Enable or disable a network adapter.
    SetAdapterEnabled {
        /// The adapter's scenario name.
        name: String,
        /// The new state.
        enabled: bool,
    },
    /// Informational: the run started.
    Start,
    /// Informational: the run paused.
    Pause,
    /// Informational: the run resumed.
    Resume,
    /// Informational: stepped this many ticks.
    Step(u64),
    /// Informational: the clock mode changed.
    SetClock(String),
    /// Informational: the run stopped.
    Stop,
}

/// Messages one adapter delivered in one tick, exactly as polled (ASM-627).
#[derive(Debug, Clone, PartialEq)]
pub struct InboundRecord {
    /// The tick about to run.
    pub tick: u64,
    /// The adapter's scenario name.
    pub adapter: String,
    /// The canonical messages.
    pub messages: Vec<NetMessage>,
}

/// An adapter fault seen live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultRecord {
    /// The tick about to run.
    pub tick: u64,
    /// The adapter's scenario name.
    pub adapter: String,
    /// The adapter call that failed (`poll` or `publish`).
    pub call: String,
}

/// The last record of a complete recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndRecord {
    /// The last tick that ran.
    pub tick: u64,
    /// Why the run ended (`completed`, `stopped`, `plugin_fault`, ...).
    pub reason: String,
    /// The final state hash.
    pub hash: StateHash,
}
