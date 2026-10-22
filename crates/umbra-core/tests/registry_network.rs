//! WI-3: the `NetworkAdapter` registry kind (D-8, ADR 0008, ADR 0030).
#![forbid(unsafe_code)]
#![expect(clippy::unwrap_used, reason = "test code")]

use std::sync::Arc;

use umbra_core::net::{AdapterStart, KeySpace, NetError, NetworkAdapter, NetworkAdapterConfig};
use umbra_core::{
    BuildError, ComponentId, ComponentKind, EntityId, Factory, FlatTerrain, Meters, ParamSchema,
    ParamValue, Registry, RegistryError, SimEvent, TerrainProvider,
};
use umbra_core::{Ecef, Vec3};

struct Cfg;
impl NetworkAdapterConfig for Cfg {
    fn key_space(&self) -> KeySpace {
        KeySpace { base: 0, count: 1 }
    }
    fn start(&self, _: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        Err(NetError::Start("stub".to_owned()))
    }
}

struct Fac {
    id: ComponentId,
    schema: ParamSchema,
}

impl Fac {
    fn arc(id: &str) -> Arc<dyn Factory<dyn NetworkAdapterConfig>> {
        Arc::new(Self {
            id: ComponentId::new(id).unwrap(),
            schema: ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap(),
        })
    }
}

impl Factory<dyn NetworkAdapterConfig> for Fac {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        Ok(Arc::new(Cfg))
    }
}

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap()
}

#[test]
fn register_lookup_and_kind_of() {
    let mut r = Registry::new();
    let f = Fac::arc("builtin.net.loopback");
    r.register_network_adapter(f.clone()).unwrap();
    let id = cid("builtin.net.loopback");
    assert!(Arc::ptr_eq(r.network_adapter(&id).unwrap(), &f));
    assert_eq!(r.kind_of(&id), Some(ComponentKind::NetworkAdapter));
    assert!(r.param_schema(ComponentKind::NetworkAdapter, &id).is_some());
    assert!(r.param_schema(ComponentKind::Terrain, &id).is_none());
    assert_eq!(r.ids(ComponentKind::NetworkAdapter), vec![&id]);
    assert!(r.network_adapter(&cid("x.unknown")).is_none());
    assert_eq!(ComponentKind::NetworkAdapter.noun(), "network adapter");
}

#[test]
fn duplicate_network_adapter_is_refused() {
    let mut r = Registry::new();
    r.register_network_adapter(Fac::arc("a.b")).unwrap();
    let err = r.register_network_adapter(Fac::arc("a.b")).err().unwrap();
    assert_eq!(
        err,
        RegistryError::Duplicate {
            kind: ComponentKind::NetworkAdapter,
            id: cid("a.b")
        }
    );
    assert_eq!(err.to_string(), "network adapter a.b is already registered");
    assert_eq!(r.ids(ComponentKind::NetworkAdapter).len(), 1);
}

struct TerrainFac(ComponentId, ParamSchema);
impl Factory<dyn TerrainProvider> for TerrainFac {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.1
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
        Ok(Arc::new(FlatTerrain {
            height: Meters(0.0),
        }))
    }
}

fn terrain(id: &str) -> Arc<dyn Factory<dyn TerrainProvider>> {
    Arc::new(TerrainFac(
        cid(id),
        ParamSchema::from_json_str(r#"{"type":"object"}"#).unwrap(),
    ))
}

#[test]
fn network_adapter_id_taken_by_another_kind_is_refused() {
    let mut r = Registry::new();
    r.register_terrain(terrain("a.b")).unwrap();
    let err = r.register_network_adapter(Fac::arc("a.b")).err().unwrap();
    assert_eq!(
        err,
        RegistryError::Duplicate {
            kind: ComponentKind::Terrain,
            id: cid("a.b")
        }
    );
    assert!(r.network_adapter(&cid("a.b")).is_none());
    assert_eq!(r.kind_of(&cid("a.b")), Some(ComponentKind::Terrain));
}

#[test]
fn id_taken_by_a_network_adapter_is_refused_for_another_kind() {
    let mut r = Registry::new();
    r.register_network_adapter(Fac::arc("a.b")).unwrap();
    let err = r.register_terrain(terrain("a.b")).err().unwrap();
    assert_eq!(
        err,
        RegistryError::Duplicate {
            kind: ComponentKind::NetworkAdapter,
            id: cid("a.b")
        }
    );
    assert!(r.terrain(&cid("a.b")).is_none());
    assert!(r.network_adapter(&cid("a.b")).is_some());
}

#[test]
fn all_lists_network_adapter_once() {
    let n = ComponentKind::ALL
        .iter()
        .filter(|&&k| k == ComponentKind::NetworkAdapter)
        .count();
    assert_eq!(n, 1);
}

#[test]
fn detonation_kind_name() {
    let e = SimEvent::Detonation {
        shooter: Some(EntityId::new(1).unwrap()),
        target: None,
        position: Ecef(Vec3::ZERO),
    };
    assert_eq!(e.kind_name(), "detonation");
}
