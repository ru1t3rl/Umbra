//! Registration of the built-in conditions and actions and of graphs as behaviors.
//!
//! M3 step 4: signatures only; the bodies are `todo!()` until WI-10 and WI-11.

use std::sync::Arc;

use cgf_core::{GraphDef, Registry, RegistryError};

use crate::factory::GraphRegistrationError;

/// Registers the built-in conditions (`builtin.cond.{enemy_in_range, target_in_range,
/// timer_elapsed, chance}`) and actions (`builtin.action.{follow_route, move_to, wait, fire}`),
/// ASM-077.
///
/// # Errors
/// [`RegistryError::Duplicate`] if one of them is already registered.
#[expect(clippy::todo, reason = "M3 WI-11 stub")]
pub fn register_builtins(_registry: &mut Registry) -> Result<(), RegistryError> {
    todo!("M3 WI-11")
}

/// Adds graphs as behaviors, subgraphs first (dependency order, ties by ID), with
/// `ComponentInfo` from each graph (provenance, version, `param_types` from `Interface::types`,
/// outputs, subgraph references, result text).
///
/// # Errors
/// [`GraphRegistrationError`]: a duplicate ID, an unresolved reference, or a cycle.
#[expect(clippy::todo, reason = "M3 WI-10 stub")]
pub fn register_graphs<'a>(
    _registry: &mut Registry,
    _graphs: impl IntoIterator<Item = &'a Arc<GraphDef>>,
) -> Result<(), GraphRegistrationError> {
    todo!("M3 WI-10")
}
