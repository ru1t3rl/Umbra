//! The condition and action kinds (ABI 0.2, ADR 0050, REQ-P-4, REQ-B-3, REQ-B-15).
//!
//! Leaves of a behavior graph written as plugins: a [`Condition`] answers true or false; an
//! [`Action`] makes one [`ActionInstance`] per activation, which ticks and may emit intents over
//! several ticks. Both get a [`BlackboardAccess`] restricted to the keys their metadata declares
//! (`blackboard_params`), the entity view of the tick and the entity's RNG stream. Events are
//! emitted as `Intent::Custom` (ADR 0050, H2).
//!
//! The blackboard grants the keys the host resolved from the leaf's params at configure: a param named
//! in `blackboard_params` holds the key's name, so a leaf reads its own params in `configure` to know
//! which key to pass to [`BlackboardAccess::get`] and [`BlackboardAccess::set`].

use crate::abi::UmbraToken;
use crate::behavior::{EntityView, Fault, Intents, Rng, Status};
use crate::error::Error;
use crate::host::HostCopy;
use crate::services::BlackboardAccess;

/// Time and randomness of one leaf call.
#[derive(Debug)]
pub struct LeafCtx<'a> {
    tick: u64,
    rate_hz: u32,
    host: Option<HostCopy>,
    rng: UmbraToken,
    fault: &'a Fault,
}

impl<'a> LeafCtx<'a> {
    /// The context of one `check` or `tick` call.
    pub(crate) fn new(
        tick: u64,
        rate_hz: u32,
        host: Option<HostCopy>,
        rng: UmbraToken,
        fault: &'a Fault,
    ) -> Self {
        Self {
            tick,
            rate_hz,
            host,
            rng,
            fault,
        }
    }
}

impl LeafCtx<'_> {
    /// Simulation tick number.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Tick rate in Hz.
    #[must_use]
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The entity's own RNG stream through the host.
    pub fn rng(&mut self) -> Rng<'_> {
        Rng::new(self.host, self.rng, self.fault)
    }
}

/// A condition plugin capability (listed under `conditions:` of `export_plugin!`).
pub trait Condition: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset).
    const SCHEMA: &'static str;
    /// Metadata JSON: `{"blackboard_params": [{"param", "type", "access"}], "result"}` (ASM-643,
    /// ASM-709); empty when the condition uses no blackboard key and has no result text.
    const METADATA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;

    /// Builds the condition from the JSON params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &str) -> Result<Self, Error>;

    /// Whether the condition holds for `entity` now.
    ///
    /// # Errors
    /// The check failed (or a service call was refused); the host faults the capability and treats
    /// the result as false.
    fn check(
        &self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
    ) -> Result<bool, Error>;
}

/// An action plugin capability (listed under `actions:` of `export_plugin!`).
pub trait Action: Sized + Send + Sync + 'static {
    /// Component id (grammar of ADR 0002; `builtin.` is refused by the host).
    const ID: &'static str;
    /// JSON Schema of the params (ADR 0009 subset).
    const SCHEMA: &'static str;
    /// Metadata JSON, as for [`Condition::METADATA`].
    const METADATA: &'static str;
    /// Capability version; `None` means the plugin version.
    const VERSION: Option<&'static str> = None;
    /// The per-activation state.
    type Instance: ActionInstance;

    /// Builds the action from the JSON params.
    ///
    /// # Errors
    /// Bad params.
    fn configure(params: &str) -> Result<Self, Error>;

    /// Creates an instance at `rate_hz`.
    ///
    /// # Errors
    /// The action cannot run at this rate.
    fn instantiate(&self, rate_hz: u32) -> Result<Self::Instance, Error>;
}

/// One activation of an [`Action`].
pub trait ActionInstance: Send + 'static {
    /// One tick.
    ///
    /// # Errors
    /// The tick failed; the host reports Failure and discards this call's intents.
    fn tick(
        &mut self,
        ctx: &mut LeafCtx<'_>,
        entity: &EntityView<'_>,
        bb: &mut BlackboardAccess<'_>,
        out: &mut Intents<'_>,
    ) -> Result<Status, Error>;

    /// Halt: idempotent, resets the instance to its initial state.
    fn halt(&mut self);
}
