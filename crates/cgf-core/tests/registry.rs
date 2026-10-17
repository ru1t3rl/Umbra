//! Public-API tests of the component registry (REQ-T-1, REQ-T-3, ADR 0008).
#![forbid(unsafe_code)]
#![expect(clippy::unwrap_used, reason = "test code")]

use std::sync::Arc;

use cgf_core::{
    BuildError, ComponentId, ComponentKind, Factory, FlatTerrain, Intent, KinematicState, Meters,
    MotionModel, MotionModelId, ParamSchema, ParamValue, Registry, RegistryError, Seconds,
    TerrainProvider,
};

struct Stub {
    id: ComponentId,
    model_id: MotionModelId,
    schema: ParamSchema,
}

struct StubModel(MotionModelId);

impl MotionModel for StubModel {
    fn id(&self) -> &MotionModelId {
        &self.0
    }
    fn step(
        &self,
        s: &KinematicState,
        _: &[Intent],
        _: Seconds,
        _: &dyn TerrainProvider,
    ) -> KinematicState {
        *s
    }
}

impl Stub {
    fn arc(id: &str) -> Arc<dyn Factory<dyn MotionModel>> {
        Arc::new(Self {
            id: ComponentId::new(id).unwrap(),
            model_id: MotionModelId::new(id).unwrap(),
            schema: ParamSchema::from_json_str(r#"{"type":"object","examples":[{}]}"#).unwrap(),
        })
    }
}

impl Factory<dyn MotionModel> for Stub {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, p: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
        if p.get("bad").is_some() {
            return Err(BuildError {
                message: "bad".into(),
                param: Some("bad".into()),
            });
        }
        Ok(Arc::new(StubModel(self.model_id.clone())))
    }
}

#[test]
fn motion_factory_registers_looks_up_and_builds() {
    let mut r = Registry::new();
    r.register_motion_model(Stub::arc("acme.ground.fast"))
        .unwrap();
    let id = ComponentId::new("acme.ground.fast").unwrap();
    let f = r.motion_model(&id).unwrap();
    assert_eq!(
        f.build(&ParamValue::Null).unwrap().id().as_str(),
        "acme.ground.fast"
    );
    assert_eq!(
        f.build(&serde_json::json!({"bad": 1}))
            .err()
            .unwrap()
            .param
            .as_deref(),
        Some("bad")
    );
    assert_eq!(
        r.param_schema(ComponentKind::MotionModel, &id)
            .unwrap()
            .as_json()["examples"][0],
        serde_json::json!({})
    );
}

#[test]
fn duplicate_motion_model_leaves_first_and_ids_sorted() {
    let mut r = Registry::new();
    for id in ["b.two", "a.one", "c.three"] {
        r.register_motion_model(Stub::arc(id)).unwrap();
    }
    let err = r.register_motion_model(Stub::arc("a.one")).err().unwrap();
    assert!(matches!(
        err,
        RegistryError::Duplicate {
            kind: "motion model",
            ..
        }
    ));
    let ids: Vec<_> = r
        .ids(ComponentKind::MotionModel)
        .iter()
        .map(|i| i.as_str())
        .collect();
    assert_eq!(ids, ["a.one", "b.two", "c.three"]);
    assert_eq!(r.ids(ComponentKind::Terrain).len(), 0);
}

#[test]
fn component_id_follows_the_adr_0002_grammar() {
    for ok in ["builtin.ground.kinematic", "a.b", "a1.b_2"] {
        assert!(ComponentId::new(ok).is_ok(), "{ok}");
    }
    for bad in ["", "single", "A.b", "a..b", ".a.b", "a.b.", "a.1b", "a b.c"] {
        assert!(ComponentId::new(bad).is_err(), "{bad}");
    }
    assert!(ComponentId::new(&format!("a.{}", "b".repeat(128))).is_err());
}

#[test]
fn arc_flat_terrain_is_a_terrain_provider() {
    fn takes(_: Box<dyn TerrainProvider>) {}
    takes(Box::new(Arc::new(FlatTerrain {
        height: Meters(0.0),
    })));
    let dynamic: Arc<dyn TerrainProvider> = Arc::new(FlatTerrain {
        height: Meters(1.0),
    });
    takes(Box::new(dynamic));
}
