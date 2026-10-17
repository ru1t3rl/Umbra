//! Shared helpers for the `cgf-templates` integration tests: stub component factories (the
//! templates crate may not use `cgf-motion`, ASM-005), in-memory sources and diagnostic summaries.
//!
//! Each test binary uses a subset of the helpers.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cgf_core::{
    BuildError, ComponentId, Factory, FlatTerrain, Intent, KinematicState, Meters, MotionModel,
    MotionModelId, ParamSchema, ParamValue, Registry, Seconds, TemplateId, TerrainProvider,
};
use cgf_templates::{Diagnostic, Diagnostics, Loader, Scenario, SourceFile, TemplateSet};

/// `a` is a number greater than zero, required; nothing else.
pub const ALPHA_SCHEMA: &str = r#"{"type":"object","properties":{"a":{"type":"number","exclusiveMinimum":0}},"required":["a"],"additionalProperties":false,"examples":[{"a":1.0}]}"#;
/// `b` is an integer; nothing required.
pub const BETA_SCHEMA: &str = r#"{"type":"object","properties":{"b":{"type":"integer"}},"additionalProperties":false,"examples":[{}]}"#;
/// `x`, `y`, `z` are numbers; nothing required (makes key-wise merging visible).
pub const GAMMA_SCHEMA: &str = r#"{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"z":{"type":"number"}},"additionalProperties":false,"examples":[{}]}"#;
/// `height_m` is a number, required.
pub const TERRAIN_SCHEMA: &str = r#"{"type":"object","properties":{"height_m":{"type":"number"}},"required":["height_m"],"additionalProperties":false,"examples":[{"height_m":0.0}]}"#;

pub fn cid(s: &str) -> ComponentId {
    ComponentId::new(s).unwrap_or_else(|e| panic!("bad component id {s:?}: {e}"))
}

pub fn tid(s: &str) -> TemplateId {
    TemplateId::new(s).unwrap_or_else(|e| panic!("bad template id {s:?}: {e}"))
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

/// A motion factory with a fixed schema that counts `build` calls and rejects `a > 1000`.
pub struct StubMotion {
    id: ComponentId,
    schema: ParamSchema,
    builds: Arc<AtomicUsize>,
}

impl StubMotion {
    pub fn new(id: &str, schema: &str, builds: &Arc<AtomicUsize>) -> Self {
        Self {
            id: cid(id),
            schema: ParamSchema::from_json_str(schema)
                .unwrap_or_else(|e| panic!("bad stub schema for {id}: {e}")),
            builds: Arc::clone(builds),
        }
    }
}

impl Factory<dyn MotionModel> for StubMotion {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        if params.get("a").and_then(ParamValue::as_f64) > Some(1000.0) {
            return Err(BuildError::new("`a` is unreasonably large").at("/a"));
        }
        let id = MotionModelId::new(self.id.as_str())
            .unwrap_or_else(|e| panic!("bad model id {}: {e}", self.id));
        Ok(Arc::new(StubModel(id)))
    }
}

/// A flat terrain factory with a fixed schema that counts `build` calls.
pub struct StubTerrain {
    id: ComponentId,
    schema: ParamSchema,
    builds: Arc<AtomicUsize>,
}

impl StubTerrain {
    pub fn new(id: &str, builds: &Arc<AtomicUsize>) -> Self {
        Self {
            id: cid(id),
            schema: ParamSchema::from_json_str(TERRAIN_SCHEMA)
                .unwrap_or_else(|e| panic!("bad terrain schema: {e}")),
            builds: Arc::clone(builds),
        }
    }
}

impl Factory<dyn TerrainProvider> for StubTerrain {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        let height = params
            .get("height_m")
            .and_then(ParamValue::as_f64)
            .unwrap_or(0.0);
        Ok(Arc::new(FlatTerrain {
            height: Meters(height),
        }))
    }
}

/// Counters of `Factory::build` calls.
pub struct Counters {
    pub motion: Arc<AtomicUsize>,
    pub terrain: Arc<AtomicUsize>,
}

impl Counters {
    pub fn motion(&self) -> usize {
        self.motion.load(Ordering::SeqCst)
    }
    pub fn terrain(&self) -> usize {
        self.terrain.load(Ordering::SeqCst)
    }
}

/// A registry with the given motion factories `(id, schema)` and terrain factories (ids).
pub fn registry_with(motion: &[(&str, &str)], terrain: &[&str]) -> (Registry, Counters) {
    let counters = Counters {
        motion: Arc::new(AtomicUsize::new(0)),
        terrain: Arc::new(AtomicUsize::new(0)),
    };
    let mut registry = Registry::new();
    for (id, schema) in motion {
        let f = StubMotion::new(id, schema, &counters.motion);
        registry
            .register_motion_model(Arc::new(f))
            .unwrap_or_else(|e| panic!("registering {id}: {e}"));
    }
    for id in terrain {
        let f = StubTerrain::new(id, &counters.terrain);
        registry
            .register_terrain(Arc::new(f))
            .unwrap_or_else(|e| panic!("registering {id}: {e}"));
    }
    (registry, counters)
}

/// The standard test registry: motion models `stub.motion.{alpha,beta,gamma}` and terrain
/// providers `stub.terrain.flat` and `builtin.terrain.flat`. The scenario format defaults the
/// terrain to the ID `builtin.terrain.flat`, so any registry used with scenarios must hold it.
pub fn registry() -> (Registry, Counters) {
    registry_with(
        &[
            ("stub.motion.alpha", ALPHA_SCHEMA),
            ("stub.motion.beta", BETA_SCHEMA),
            ("stub.motion.gamma", GAMMA_SCHEMA),
        ],
        &["stub.terrain.flat", "builtin.terrain.flat"],
    )
}

pub fn loader(registry: &Registry) -> Loader<'_> {
    Loader::new(registry).unwrap_or_else(|e| panic!("loader: {e}"))
}

pub fn src(path: &str, text: &str) -> SourceFile {
    SourceFile::new(path, text)
}

/// Joins lines with LF and a trailing LF, so line numbers are `index + 1`.
pub fn lines(l: &[&str]) -> String {
    let mut s = l.join("\n");
    s.push('\n');
    s
}

/// Loads templates with the standard registry.
pub fn templates(files: &[SourceFile]) -> Result<TemplateSet, Diagnostics> {
    let (registry, _) = registry();
    loader(&registry).templates_from_sources(files)
}

/// Loads a scenario and templates with the standard registry.
pub fn scenario(sc: &SourceFile, files: &[SourceFile]) -> Result<Scenario, Diagnostics> {
    let (registry, _) = registry();
    loader(&registry).scenario_from_sources(sc, files)
}

/// The diagnostics of a failed load; panics if the load succeeded.
pub fn errors<T>(result: Result<T, Diagnostics>) -> Vec<Diagnostic> {
    match result {
        Ok(_) => panic!("expected diagnostics, but the load succeeded"),
        Err(d) => d.iter().cloned().collect(),
    }
}

/// The value of a successful load; panics with the rendered diagnostics otherwise.
pub fn ok<T>(result: Result<T, Diagnostics>) -> T {
    match result {
        Ok(v) => v,
        Err(d) => {
            let rendered: Vec<String> = d.iter().map(ToString::to_string).collect();
            panic!("expected success, got:\n{}", rendered.join("\n"))
        }
    }
}

/// `(code, path, line, column)` of a diagnostic.
pub type Row = (String, PathBuf, u32, u32);

pub fn row(code: &str, path: impl AsRef<Path>, at: (u32, u32)) -> Row {
    (code.to_string(), path.as_ref().to_path_buf(), at.0, at.1)
}

/// Rows of diagnostics; a diagnostic without location becomes line 0, column 0 on an empty path.
pub fn rows(diagnostics: &[Diagnostic]) -> Vec<Row> {
    diagnostics
        .iter()
        .map(|d| match &d.location {
            Some(l) => (
                d.code.as_str().to_string(),
                l.path.clone(),
                l.line,
                l.column,
            ),
            None => (d.code.as_str().to_string(), PathBuf::new(), 0, 0),
        })
        .collect()
}

/// 1-based line and column (Unicode scalar values) of the first occurrence of `needle`.
pub fn pos_of(text: &str, needle: &str) -> (u32, u32) {
    pos_of_nth(text, needle, 0)
}

/// Like [`pos_of`] for the `n`-th (0-based) occurrence.
pub fn pos_of_nth(text: &str, needle: &str, n: usize) -> (u32, u32) {
    let at = text.match_indices(needle).nth(n).map_or_else(
        || panic!("occurrence {n} of {needle:?} not found in:\n{text}"),
        |(i, _)| i,
    );
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}

/// Line and column just after the first occurrence of a single-line `needle` (the start of the
/// value in `key = value`).
pub fn pos_after(text: &str, needle: &str) -> (u32, u32) {
    let (line, col) = pos_of(text, needle);
    let width = u32::try_from(needle.chars().count()).unwrap_or(u32::MAX);
    (line, col + width)
}

/// `<crate>/tests/fixtures/<rel>`.
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

pub fn read_fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// A fixture as a source whose path is its real path.
pub fn fixture_source(rel: &str) -> SourceFile {
    SourceFile::new(fixture_path(rel), read_fixture(rel))
}

/// A minimal valid template using `stub.motion.alpha`.
pub fn alpha_template(id: &str, extends: Option<&str>, a: &str) -> String {
    let extends_line = extends.map_or_else(String::new, |e| format!("extends = \"{e}\"\n"));
    format!(
        "schema_version = 1\nid = \"{id}\"\n{extends_line}[motion]\nmodel = \"stub.motion.alpha\"\nparams = {{ a = {a} }}\n"
    )
}
