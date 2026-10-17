//! The single component registry (D-8, ADR 0008).
//!
//! Templates name components by [`ComponentId`] only; whether a factory is built in or comes from
//! a plugin is invisible to lookups (REQ-T-1). Iteration is in ID order (REQ-K-4).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::behavior::{Action, Behavior, Condition};
use crate::catalog::ComponentInfo;
use crate::id::ComponentId;
use crate::motion::MotionModel;
use crate::params::{ParamSchema, ParamValue};
use crate::perception::Sensor;
use crate::terrain::TerrainProvider;

/// The kinds of component a [`Registry`] can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ComponentKind {
    /// A [`MotionModel`].
    MotionModel,
    /// A [`TerrainProvider`].
    Terrain,
    /// A behavior (a graph or a plugin behavior).
    Behavior,
    /// A sensor.
    Sensor,
    /// A behavior-tree condition leaf (M3).
    Condition,
    /// A behavior-tree action leaf (M3).
    Action,
}

impl ComponentKind {
    /// Every kind, in declaration order.
    pub const ALL: &[ComponentKind] = &[
        Self::MotionModel,
        Self::Terrain,
        Self::Behavior,
        Self::Sensor,
        Self::Condition,
        Self::Action,
    ];

    /// Human-readable singular noun for messages.
    pub fn noun(self) -> &'static str {
        match self {
            Self::MotionModel => "motion model",
            Self::Terrain => "terrain provider",
            Self::Behavior => "behavior",
            Self::Sensor => "sensor",
            Self::Condition => "condition",
            Self::Action => "action",
        }
    }
}

impl fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.noun())
    }
}

/// A factory failed to build a component from its parameters.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
#[non_exhaustive]
pub struct BuildError {
    /// What is wrong.
    pub message: String,
    /// The parameter at fault as an RFC 6901 JSON pointer relative to the params root (for
    /// example `/max_speed_mps`), for error location.
    pub param: Option<String>,
}

impl BuildError {
    /// An error that is not tied to one parameter.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            param: None,
        }
    }

    /// Ties the error to the parameter at JSON pointer `pointer` (for example `/max_speed_mps`).
    #[must_use]
    pub fn at(mut self, pointer: impl Into<String>) -> Self {
        self.param = Some(pointer.into());
        self
    }
}

/// Builds one kind of component from parameters. Built-ins and plugin bridges implement it
/// identically (REQ-T-1). Called at load time only, never per tick.
///
/// The trait is object safe for unsized products:
///
/// ```
/// use std::sync::Arc;
/// use cgf_core::{Factory, MotionModel};
/// fn _accepts(_: Arc<dyn Factory<dyn MotionModel>>) {}
/// ```
pub trait Factory<T: ?Sized>: Send + Sync {
    /// The component's ID.
    fn id(&self) -> &ComponentId;

    /// The JSON Schema of the component's parameters (REQ-T-3).
    fn param_schema(&self) -> &ParamSchema;

    /// Builds the component. `params` has passed [`Factory::param_schema`]; the factory may
    /// still reject semantic combinations.
    ///
    /// Must not panic for any `params`: return a [`BuildError`] instead. The product's own ID
    /// must equal [`Factory::id`].
    fn build(&self, params: &ParamValue) -> Result<Arc<T>, BuildError>;
}

/// A registration was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// A component with this ID is already registered, under any kind (IDs are unique across
    /// kinds, ASM-063); the first one is kept.
    #[error("{kind} {id} is already registered")]
    Duplicate {
        /// Kind of the component already holding the ID.
        kind: ComponentKind,
        /// The duplicate ID.
        id: ComponentId,
    },
}

type MotionFactory = Arc<dyn Factory<dyn MotionModel>>;
type TerrainFactory = Arc<dyn Factory<dyn TerrainProvider>>;
type BehaviorFactory = Arc<dyn Factory<dyn Behavior>>;
type ConditionFactory = Arc<dyn Factory<dyn Condition>>;
type ActionFactory = Arc<dyn Factory<dyn Action>>;
/// A factory stored with its metadata, so one cannot exist without the other.
type WithInfo<F> = (F, ComponentInfo);
type SensorFactory = Arc<dyn Factory<dyn Sensor>>;

/// All registered component factories, looked up by kind and ID.
///
/// `Clone` copies the maps and shares the factories (they are `Arc`s), so the engine can give each
/// scenario its own registry with that scenario's file graphs added (ADR 0011).
#[derive(Default, Clone)]
pub struct Registry {
    kinds: BTreeMap<ComponentId, ComponentKind>,
    motion: BTreeMap<ComponentId, MotionFactory>,
    terrain: BTreeMap<ComponentId, TerrainFactory>,
    behavior: BTreeMap<ComponentId, WithInfo<BehaviorFactory>>,
    condition: BTreeMap<ComponentId, WithInfo<ConditionFactory>>,
    action: BTreeMap<ComponentId, WithInfo<ActionFactory>>,
    sensor: BTreeMap<ComponentId, SensorFactory>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Registry");
        for &kind in ComponentKind::ALL {
            d.field(kind.noun(), &self.ids(kind));
        }
        d.finish()
    }
}

/// Registers `value` under `id` in `map` unless the ID is taken under any kind.
fn insert<V>(
    kinds: &mut BTreeMap<ComponentId, ComponentKind>,
    map: &mut BTreeMap<ComponentId, V>,
    kind: ComponentKind,
    id: &ComponentId,
    value: V,
) -> Result<(), RegistryError> {
    use std::collections::btree_map::Entry;
    match kinds.entry(id.clone()) {
        Entry::Occupied(e) => Err(RegistryError::Duplicate {
            kind: *e.get(),
            id: e.key().clone(),
        }),
        Entry::Vacant(e) => {
            map.insert(e.key().clone(), value);
            e.insert(kind);
            Ok(())
        }
    }
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a motion model factory under its own ID.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_motion_model(&mut self, f: MotionFactory) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.motion,
            ComponentKind::MotionModel,
            &id,
            f,
        )
    }

    /// Registers a terrain provider factory under its own ID.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_terrain(&mut self, f: TerrainFactory) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.terrain,
            ComponentKind::Terrain,
            &id,
            f,
        )
    }

    /// Registers a behavior factory under its own ID with its metadata.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_behavior(
        &mut self,
        f: BehaviorFactory,
        info: ComponentInfo,
    ) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.behavior,
            ComponentKind::Behavior,
            &id,
            (f, info),
        )
    }

    /// Registers a condition factory under its own ID with its metadata.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_condition(
        &mut self,
        f: ConditionFactory,
        info: ComponentInfo,
    ) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.condition,
            ComponentKind::Condition,
            &id,
            (f, info),
        )
    }

    /// Registers an action factory under its own ID with its metadata.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_action(
        &mut self,
        f: ActionFactory,
        info: ComponentInfo,
    ) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.action,
            ComponentKind::Action,
            &id,
            (f, info),
        )
    }

    /// Registers a sensor factory under its own ID. Sensors carry no catalog metadata.
    ///
    /// # Errors
    /// [`RegistryError::Duplicate`] if the ID is taken; the registry is unchanged.
    pub fn register_sensor(&mut self, f: SensorFactory) -> Result<(), RegistryError> {
        let id = f.id().clone();
        insert(
            &mut self.kinds,
            &mut self.sensor,
            ComponentKind::Sensor,
            &id,
            f,
        )
    }

    /// The behavior factory with this ID.
    pub fn behavior(&self, id: &ComponentId) -> Option<&BehaviorFactory> {
        self.behavior.get(id).map(|(f, _)| f)
    }

    /// The condition factory with this ID.
    pub fn condition(&self, id: &ComponentId) -> Option<&ConditionFactory> {
        self.condition.get(id).map(|(f, _)| f)
    }

    /// The action factory with this ID.
    pub fn action(&self, id: &ComponentId) -> Option<&ActionFactory> {
        self.action.get(id).map(|(f, _)| f)
    }

    /// The sensor factory with this ID.
    pub fn sensor(&self, id: &ComponentId) -> Option<&SensorFactory> {
        self.sensor.get(id)
    }

    /// The metadata of a behavior, condition or action (`None` for other kinds and unknown IDs).
    pub fn info(&self, id: &ComponentId) -> Option<&ComponentInfo> {
        match self.kinds.get(id)? {
            ComponentKind::Behavior => self.behavior.get(id).map(|(_, i)| i),
            ComponentKind::Condition => self.condition.get(id).map(|(_, i)| i),
            ComponentKind::Action => self.action.get(id).map(|(_, i)| i),
            _ => None,
        }
    }

    /// Every behavior, condition or action of `kind` with its schema and metadata, in ID order.
    /// Empty for other kinds. The factory and the metadata live in one map entry, so none can
    /// be missing (used by `Catalog::from_registry`).
    pub(crate) fn described(
        &self,
        kind: ComponentKind,
    ) -> Vec<(&ComponentId, &ParamSchema, &ComponentInfo)> {
        fn rows<F: ?Sized>(
            map: &BTreeMap<ComponentId, WithInfo<Arc<dyn Factory<F>>>>,
        ) -> Vec<(&ComponentId, &ParamSchema, &ComponentInfo)> {
            map.iter()
                .map(|(id, (f, info))| (id, f.param_schema(), info))
                .collect()
        }
        match kind {
            ComponentKind::Behavior => rows(&self.behavior),
            ComponentKind::Condition => rows(&self.condition),
            ComponentKind::Action => rows(&self.action),
            _ => Vec::new(),
        }
    }

    /// The motion model factory with this ID.
    pub fn motion_model(&self, id: &ComponentId) -> Option<&MotionFactory> {
        self.motion.get(id)
    }

    /// The terrain provider factory with this ID.
    pub fn terrain(&self, id: &ComponentId) -> Option<&TerrainFactory> {
        self.terrain.get(id)
    }

    /// The parameter schema of a component, for any kind.
    pub fn param_schema(&self, kind: ComponentKind, id: &ComponentId) -> Option<&ParamSchema> {
        match kind {
            ComponentKind::MotionModel => self.motion.get(id).map(|f| f.param_schema()),
            ComponentKind::Terrain => self.terrain.get(id).map(|f| f.param_schema()),
            ComponentKind::Behavior => self.behavior.get(id).map(|(f, _)| f.param_schema()),
            ComponentKind::Sensor => self.sensor.get(id).map(|f| f.param_schema()),
            ComponentKind::Condition => self.condition.get(id).map(|(f, _)| f.param_schema()),
            ComponentKind::Action => self.action.get(id).map(|(f, _)| f.param_schema()),
        }
    }

    /// The kind an ID is registered under, if any (IDs are global across kinds, ASM-063).
    pub fn kind_of(&self, id: &ComponentId) -> Option<ComponentKind> {
        self.kinds.get(id).copied()
    }

    /// Registered IDs of a kind in sorted order.
    pub fn ids(&self, kind: ComponentKind) -> Vec<&ComponentId> {
        match kind {
            ComponentKind::MotionModel => self.motion.keys().collect(),
            ComponentKind::Terrain => self.terrain.keys().collect(),
            ComponentKind::Behavior => self.behavior.keys().collect(),
            ComponentKind::Sensor => self.sensor.keys().collect(),
            ComponentKind::Condition => self.condition.keys().collect(),
            ComponentKind::Action => self.action.keys().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::FlatTerrain;
    use crate::units::Meters;

    struct FlatFactory {
        id: ComponentId,
        schema: ParamSchema,
    }

    impl FlatFactory {
        fn arc(id: &str) -> Arc<dyn Factory<dyn TerrainProvider>> {
            Arc::new(Self {
                id: ComponentId::new(id).unwrap(),
                schema: ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap(),
            })
        }
    }

    impl Factory<dyn TerrainProvider> for FlatFactory {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn param_schema(&self) -> &ParamSchema {
            &self.schema
        }
        fn build(&self, _: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
            Ok(Arc::new(FlatTerrain {
                height: Meters(0.0),
            }))
        }
    }

    fn cid(s: &str) -> ComponentId {
        ComponentId::new(s).unwrap()
    }

    #[test]
    fn registry_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Registry>();
    }

    #[test]
    fn lookup_by_kind_and_id() {
        let mut r = Registry::new();
        r.register_terrain(FlatFactory::arc("builtin.terrain.flat"))
            .unwrap();
        let id = cid("builtin.terrain.flat");
        assert!(r.terrain(&id).is_some());
        assert!(r.motion_model(&id).is_none(), "same id, other kind");
        assert!(r.terrain(&cid("x.unknown")).is_none());
        assert!(r.param_schema(ComponentKind::Terrain, &id).is_some());
        assert!(r.param_schema(ComponentKind::MotionModel, &id).is_none());
        assert!(r.param_schema(ComponentKind::Behavior, &id).is_none());
        assert!(r.param_schema(ComponentKind::Sensor, &id).is_none());
        assert_eq!(r.ids(ComponentKind::Behavior).len(), 0);
    }

    #[test]
    fn duplicate_is_an_error_and_keeps_the_first() {
        let mut r = Registry::new();
        let first = FlatFactory::arc("a.b");
        r.register_terrain(first.clone()).unwrap();
        let err = r.register_terrain(FlatFactory::arc("a.b")).err().unwrap();
        assert_eq!(
            err,
            RegistryError::Duplicate {
                kind: ComponentKind::Terrain,
                id: cid("a.b")
            }
        );
        assert_eq!(
            err.to_string(),
            "terrain provider a.b is already registered"
        );
        assert!(Arc::ptr_eq(r.terrain(&cid("a.b")).unwrap(), &first));
    }

    #[test]
    fn ids_are_sorted_regardless_of_registration_order() {
        let mut r = Registry::new();
        for id in ["z.last", "a.first", "m.middle"] {
            r.register_terrain(FlatFactory::arc(id)).unwrap();
        }
        let ids: Vec<&str> = r
            .ids(ComponentKind::Terrain)
            .into_iter()
            .map(ComponentId::as_str)
            .collect();
        assert_eq!(ids, ["a.first", "m.middle", "z.last"]);
    }

    #[test]
    fn factory_builds_through_a_trait_object() {
        let f = FlatFactory::arc("a.b");
        let t = f.build(&ParamValue::Null).unwrap();
        assert_eq!(
            t.surface_normal_at(
                crate::coords::Geodetic::new(
                    crate::units::Radians(0.0),
                    crate::units::Radians(0.0),
                    Meters(0.0)
                )
                .unwrap()
            )
            .up,
            1.0
        );
    }

    #[test]
    fn kind_nouns() {
        assert_eq!(ComponentKind::MotionModel.noun(), "motion model");
        assert_eq!(ComponentKind::Sensor.noun(), "sensor");
    }

    #[test]
    fn all_lists_every_kind_once() {
        // The exhaustive match fails to compile when a variant is added without updating it;
        // `COUNT` sits next to it and must grow with it, so dropping a kind from `ALL` fails.
        const COUNT: usize = 6;
        let index = |k: ComponentKind| match k {
            ComponentKind::MotionModel => 0,
            ComponentKind::Terrain => 1,
            ComponentKind::Behavior => 2,
            ComponentKind::Sensor => 3,
            ComponentKind::Condition => 4,
            ComponentKind::Action => 5,
        };
        let mut seen: Vec<usize> = ComponentKind::ALL.iter().map(|&k| index(k)).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..COUNT).collect::<Vec<_>>());
        assert_eq!(ComponentKind::Terrain.to_string(), "terrain provider");
    }

    #[test]
    fn kind_of_reports_the_kind_an_id_is_registered_under() {
        let mut r = Registry::new();
        r.register_terrain(FlatFactory::arc("a.b")).unwrap();
        let id = |s: &str| ComponentId::new(s).unwrap();

        assert_eq!(r.kind_of(&id("a.b")), Some(ComponentKind::Terrain));
        assert_eq!(r.kind_of(&id("c.d")), None);
    }

    #[test]
    fn debug_lists_ids_per_kind() {
        let mut r = Registry::new();
        r.register_terrain(FlatFactory::arc("a.b")).unwrap();
        let text = format!("{r:?}");
        assert!(
            text.contains("terrain provider: [ComponentId(\"a.b\")]"),
            "{text}"
        );
        assert!(text.contains("motion model: []"), "{text}");
    }

    #[test]
    fn id_is_unique_across_kinds() {
        let mut r = Registry::new();
        r.register_terrain(FlatFactory::arc("a.b")).unwrap();
        let err = r
            .register_motion_model(MotionStub::arc("a.b"))
            .err()
            .unwrap();
        assert_eq!(
            err,
            RegistryError::Duplicate {
                kind: ComponentKind::Terrain,
                id: cid("a.b")
            }
        );
        assert!(r.motion_model(&cid("a.b")).is_none());
        assert_eq!(r.ids(ComponentKind::MotionModel).len(), 0);
        assert!(r.terrain(&cid("a.b")).is_some());
    }

    #[test]
    fn registering_a_motion_model_leaves_behavior_and_sensor_empty() {
        let mut r = Registry::new();
        r.register_motion_model(MotionStub::arc("a.motion"))
            .unwrap();
        let id = cid("a.motion");
        assert_eq!(r.ids(ComponentKind::Behavior).len(), 0);
        assert_eq!(r.ids(ComponentKind::Sensor).len(), 0);
        assert!(r.param_schema(ComponentKind::Behavior, &id).is_none());
        assert!(r.param_schema(ComponentKind::Sensor, &id).is_none());
        assert!(r.param_schema(ComponentKind::MotionModel, &id).is_some());
    }

    #[test]
    fn build_error_constructors() {
        let e = BuildError::new("nope");
        assert_eq!((e.message.as_str(), e.param.as_deref()), ("nope", None));
        let e = e.at("/max_speed_mps");
        assert_eq!(e.param.as_deref(), Some("/max_speed_mps"));
        assert_eq!(e.to_string(), "nope");
    }

    struct MotionStub {
        id: ComponentId,
        schema: ParamSchema,
    }

    impl MotionStub {
        fn arc(id: &str) -> Arc<dyn Factory<dyn MotionModel>> {
            Arc::new(Self {
                id: cid(id),
                schema: ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap(),
            })
        }
    }

    impl Factory<dyn MotionModel> for MotionStub {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn param_schema(&self) -> &ParamSchema {
            &self.schema
        }
        fn build(&self, _: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
            Err(BuildError::new("stub"))
        }
    }
}
