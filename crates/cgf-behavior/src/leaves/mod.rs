//! The built-in conditions and actions (REQ-B-15, ASM-077) and what they share: a generic
//! factory, parameter parsing and the contact lookup.
//!
//! Every leaf reads state and perception and writes intents only (REQ-B-1). Time comes from the
//! tick's `SimTime`, randomness from the entity's own stream (REQ-B-16, REQ-K-4). No leaf
//! allocates while ticking.

mod chance;
mod enemy_in_range;
mod fire;
mod follow_route;
mod move_to;
mod target_in_range;
mod timer_elapsed;
mod wait;

use std::sync::Arc;

use cgf_core::{
    Action, BbValue, Blackboard, BuildError, ComponentId, ComponentInfo, Condition, Contact,
    EntityView, Factory, ParamSchema, ParamValue, Provenance, Registry, RegistryError, Version,
};

/// Registers all eight leaves.
pub(crate) fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    enemy_in_range::register(registry)?;
    target_in_range::register(registry)?;
    timer_elapsed::register(registry)?;
    chance::register(registry)?;
    follow_route::register(registry)?;
    move_to::register(registry)?;
    wait::register(registry)?;
    fire::register(registry)
}

/// Builds a leaf of kind `T` (`dyn Condition` or `dyn Action`) from its parameters.
type BuildFn<T> = fn(&ComponentId, &ParamValue) -> Result<Arc<T>, BuildError>;

/// One factory type for all built-in leaves: an ID, a schema and a build function.
pub(crate) struct LeafFactory<T: ?Sized> {
    id: ComponentId,
    schema: ParamSchema,
    build: BuildFn<T>,
}

impl<T: ?Sized> LeafFactory<T> {
    /// A factory for `id` with the embedded schema text.
    ///
    /// # Panics
    /// Never in practice: the ID and the schema are constants, proven valid by the unit tests
    /// and by `tests/leaves.rs`.
    #[expect(
        clippy::expect_used,
        reason = "embedded constants, proven valid by tests"
    )]
    pub(crate) fn new(id: &str, schema: &str, build: BuildFn<T>) -> Self {
        Self {
            id: ComponentId::new(id).expect("built-in leaf id is valid"),
            schema: ParamSchema::from_json_str(schema).expect("embedded leaf schema is valid"),
            build,
        }
    }
}

impl<T: ?Sized> Factory<T> for LeafFactory<T> {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<T>, BuildError> {
        (self.build)(&self.id, params)
    }
}

/// A factory for a condition.
pub(crate) type ConditionLeaf = LeafFactory<dyn Condition>;
/// A factory for an action.
pub(crate) type ActionLeaf = LeafFactory<dyn Action>;

/// Catalog metadata of a built-in leaf: provenance `Builtin`, the package version (ASM-077).
pub(crate) fn info(description: &str) -> ComponentInfo {
    let version =
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0));
    ComponentInfo::new(description, Provenance::Builtin, version)
}

// --- parameters ----------------------------------------------------------------------------------

/// Rejects a non-object and any key outside `allowed`, so a typo is an error, not ignored.
/// `Null` counts as an empty table.
pub(crate) fn check_keys(params: &ParamValue, allowed: &[&str]) -> Result<(), BuildError> {
    let Some(map) = params.as_object() else {
        return if params.is_null() {
            Ok(())
        } else {
            Err(BuildError::new("parameters must be a table").at(""))
        };
    };
    match map.keys().filter(|k| !allowed.contains(&k.as_str())).min() {
        Some(key) => Err(BuildError::new(format!("unknown parameter `{key}`")).at(pointer(key))),
        None => Ok(()),
    }
}

/// An RFC 6901 pointer to a top-level key (escapes `~` and `/`).
fn pointer(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn number(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let at = || pointer(key);
    let value = params
        .get(key)
        .ok_or_else(|| BuildError::new(format!("missing parameter `{key}`")).at(at()))?;
    match value.as_f64() {
        Some(n) if n.is_finite() => Ok(n),
        _ => Err(BuildError::new(format!("`{key}` must be a finite number")).at(at())),
    }
}

/// A required number greater than zero.
pub(crate) fn positive(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let n = number(params, key)?;
    if n > 0.0 {
        Ok(n)
    } else {
        Err(BuildError::new(format!("`{key}` must be greater than 0")).at(pointer(key)))
    }
}

/// A required number of at least zero.
pub(crate) fn non_negative(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let n = number(params, key)?;
    if n >= 0.0 {
        Ok(n)
    } else {
        Err(BuildError::new(format!("`{key}` must not be negative")).at(pointer(key)))
    }
}

/// A required number in `[0, 1]`.
pub(crate) fn probability(params: &ParamValue, key: &str) -> Result<f64, BuildError> {
    let n = number(params, key)?;
    if (0.0..=1.0).contains(&n) {
        Ok(n)
    } else {
        Err(BuildError::new(format!("`{key}` must be between 0 and 1")).at(pointer(key)))
    }
}

fn text<'a>(params: &'a ParamValue, key: &str) -> Result<&'a str, BuildError> {
    let value = params
        .get(key)
        .ok_or_else(|| BuildError::new(format!("missing parameter `{key}`")).at(pointer(key)))?;
    value
        .as_str()
        .ok_or_else(|| BuildError::new(format!("`{key}` must be a string")).at(pointer(key)))
}

/// A required blackboard key name (`[a-z][a-z0-9_]*`, the one segment grammar of ADR 0015).
pub(crate) fn key_name(params: &ParamValue, key: &str) -> Result<Box<str>, BuildError> {
    let name = text(params, key)?;
    cgf_core::Force::new(name)
        .map(|_| Box::from(name))
        .map_err(|e| {
            BuildError::new(format!("`{key}` is not a valid key name: {e}")).at(pointer(key))
        })
}

/// A required weapon label.
pub(crate) fn weapon(params: &ParamValue, key: &str) -> Result<cgf_core::WeaponId, BuildError> {
    cgf_core::WeaponId::new(text(params, key)?).map_err(|e| {
        BuildError::new(format!("`{key}` is not a valid weapon id: {e}")).at(pointer(key))
    })
}

// --- runtime -------------------------------------------------------------------------------------

/// The current contact for the entity in blackboard key `key`: `None` if the key is unset, holds
/// no entity, or the entity is not in this tick's contacts.
pub(crate) fn contact_in<'a>(
    entity: &EntityView<'a>,
    bb: &Blackboard,
    key: &str,
) -> Option<&'a Contact> {
    let Some(BbValue::Entity(id)) = bb.get(key) else {
        return None;
    };
    entity.contacts.iter().find(|c| c.entity == *id)
}
