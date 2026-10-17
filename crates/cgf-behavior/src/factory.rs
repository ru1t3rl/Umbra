//! The factory that turns a [`GraphDef`] into a [`Behavior`] (ADR 0011, ADR 0014).
//!
//! M3 step 4: signatures only; the bodies are `todo!()` until WI-8 (interpreter, `build`)
//! and WI-10 (subgraph parameters).

use std::sync::Arc;

use cgf_core::{
    Behavior, BehaviorId, BuildError, ComponentId, Factory, GraphDef, ParamSchema, ParamValue,
    Registry, RegistryError,
};

/// Why a graph could not be registered or built into a behavior.
///
/// Inputs have passed the loader, so these are bugs or a registry that changed between load and
/// instantiate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GraphRegistrationError {
    /// The registry refused the graph (for example a duplicate ID).
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A leaf or subgraph the graph names is not registered.
    #[error("graph `{graph}`: `{id}` is not registered")]
    Unresolved {
        /// The graph with the dangling reference.
        graph: BehaviorId,
        /// The missing component.
        id: ComponentId,
    },
    /// The graph is part of a subgraph cycle.
    #[error("graph `{graph}` is part of a subgraph cycle")]
    Cycle {
        /// A graph on the cycle.
        graph: BehaviorId,
    },
    /// A leaf or child factory rejected the graph's parameters.
    #[error("graph `{graph}`: {source}")]
    Build {
        /// The graph being built.
        graph: BehaviorId,
        /// The factory's error.
        #[source]
        source: BuildError,
    },
}

/// Builds a [`Behavior`] from a [`GraphDef`]: merges defaults, substitutes `{ param }` references,
/// and builds leaves and child behaviors through the registry.
#[derive(Debug)]
pub struct GraphFactory(());

impl GraphFactory {
    /// A factory for `def`, with every leaf and subgraph resolved in `registry`.
    ///
    /// # Errors
    /// [`GraphRegistrationError::Unresolved`] for a reference the registry does not know.
    #[expect(clippy::todo, reason = "M3 WI-8 stub")]
    pub fn new(_def: Arc<GraphDef>, _registry: &Registry) -> Result<Self, GraphRegistrationError> {
        todo!("M3 WI-8")
    }
}

#[expect(clippy::todo, reason = "M3 WI-8 stub")]
impl Factory<dyn Behavior> for GraphFactory {
    fn id(&self) -> &ComponentId {
        todo!("M3 WI-8")
    }

    fn param_schema(&self) -> &ParamSchema {
        todo!("M3 WI-8")
    }

    fn build(&self, _params: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        todo!("M3 WI-8")
    }
}
