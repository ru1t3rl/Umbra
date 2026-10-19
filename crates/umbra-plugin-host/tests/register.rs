//! AT-4 (registration rows): capabilities become registry entries with `Provenance::Plugin`, metadata
//! mapped to `ComponentInfo`, all-or-nothing. Requirements: REQ-P-3, REQ-P-4, REQ-B-13, REQ-T-1, D-8,
//! ASM-091 (d), ADR 0020 (metadata). Step 4 of M4 (WI-8 stub).

#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]
#![expect(unsafe_code, reason = "forged entry functions are unsafe extern fns")]

#[macro_use]
mod support;

use std::sync::Arc;

use support::forge::{leak_str, parts};
use support::raw;
use umbra_core::{
    BbType, Behavior, BuildError, ComponentId, ComponentInfo, ComponentKind, Factory, ParamSchema,
    ParamValue, PluginId, Provenance, Registry, RegistryError, Version,
};
use umbra_plugin_host::{HostOptions, PluginError, PluginHost};

fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("{e}"))
}

/// A stand-in for an already registered (built-in) behavior.
struct Taken(ComponentId, ParamSchema);

impl Factory<dyn Behavior> for Taken {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.1
    }
    fn build(&self, _: &ParamValue) -> Result<Arc<dyn Behavior>, BuildError> {
        Err(BuildError::new("stand-in"))
    }
}

fn taken(id: &str) -> (Arc<dyn Factory<dyn Behavior>>, ComponentInfo) {
    let schema = ParamSchema::from_json_str(r#"{"type":"object"}"#).expect("schema");
    (
        Arc::new(Taken(cid(id), schema)),
        ComponentInfo::new("taken", Provenance::Builtin, Version::new(1, 0, 0)),
    )
}

fn host_with(plugins: Vec<support::forge::Parts>) -> PluginHost {
    let mut host = PluginHost::new(HostOptions::default());
    for (n, p) in plugins.into_iter().enumerate() {
        let f = forged!(p);
        host.load_static(&format!("register-{n}"), f.entry)
            .expect("load");
    }
    host
}

#[test]
#[ignore = "WI-8"]
fn register_maps_metadata_to_component_info() {
    let mut registry = Registry::new();
    let host = host_with(vec![raw::parts(true)]);

    host.register(&mut registry).expect("register");

    let id = cid(raw::CAPABILITY);
    assert_eq!(registry.kind_of(&id), Some(ComponentKind::Behavior));
    let info = registry.info(&id).expect("info");
    assert_eq!(info.source, Provenance::Plugin);
    assert!(
        info.source.read_only(),
        "plugin behaviors are read-only in the catalog (REQ-B-13)"
    );
    assert_eq!(
        info.description, "raw test behavior",
        "the schema's top-level description"
    );
    assert_eq!(
        info.version,
        Version::new(0, 1, 0),
        "an empty capability version is the plugin version"
    );
    assert_eq!(info.param_types.get("in_bool"), Some(&BbType::Bool));
    assert_eq!(info.param_types.get("in_entity"), Some(&BbType::Entity));
    assert_eq!(info.param_types.get("in_position"), Some(&BbType::Position));
    assert_eq!(info.outputs.get("out_integer"), Some(&BbType::Integer));
    assert_eq!(info.result.as_deref(), Some("raw result"));
    assert!(
        info.subgraphs.is_empty(),
        "v0 metadata has no subgraphs: depth counts one level (ASM-099 e)"
    );
}

#[test]
#[ignore = "WI-8"]
fn register_keeps_the_param_schema() {
    let mut registry = Registry::new();
    host_with(vec![raw::parts(true)])
        .register(&mut registry)
        .expect("register");

    let factory = registry.behavior(&cid(raw::CAPABILITY)).expect("factory");

    let schema = factory.param_schema().as_json();
    assert_eq!(schema["properties"]["mode"]["type"], "string");
}

#[test]
#[ignore = "WI-8"]
fn a_capability_version_overrides_the_plugin_version() {
    let mut p = parts();
    p.caps[0].version = leak_str("2.1.0");
    let mut registry = Registry::new();

    host_with(vec![p])
        .register(&mut registry)
        .expect("register");

    let info = registry.info(&cid("test.behavior.forge")).expect("info");
    assert_eq!(info.version, Version::new(2, 1, 0));
}

#[test]
#[ignore = "WI-8"]
fn registering_an_id_that_is_already_registered_is_a_registry_error() {
    let mut registry = Registry::new();
    let (f, info) = taken("test.behavior.forge");
    registry.register_behavior(f, info).expect("pre-register");

    let err = host_with(vec![parts()])
        .register(&mut registry)
        .expect_err("duplicate id");

    assert!(
        matches!(err, PluginError::Registry(RegistryError::Duplicate { .. })),
        "{err:?}"
    );
    assert!(err.to_string().contains("test.behavior.forge"), "{err}");
}

#[test]
#[ignore = "WI-8"]
fn register_is_all_or_nothing() {
    let mut first = parts();
    first.desc.id = leak_str("test.plugin.first");
    first.caps[0] = support::forge::capability("test.behavior.first");
    let mut second = parts();
    second.desc.id = leak_str("test.plugin.second");
    second.caps[0] = support::forge::capability("test.behavior.second");
    let mut registry = Registry::new();
    let (f, info) = taken("test.behavior.second");
    registry.register_behavior(f, info).expect("pre-register");
    let host = host_with(vec![first, second]);

    let result = host.register(&mut registry);

    assert!(result.is_err());
    assert!(
        registry.info(&cid("test.behavior.first")).is_none(),
        "the first plugin's capability must not stay registered when a later one fails"
    );
    let ids = registry.ids(ComponentKind::Behavior);
    assert_eq!(
        ids.len(),
        1,
        "only the pre-registered behavior remains: {ids:?}"
    );
    let _ = PluginId::new("test.plugin.first");
}
