//! Shared harness of the M7 plugin-kind engine tests (WI-2, AT-1..AT-5): the example plugins as
//! packages, scenario directories written from text, the test terrain `test.terrain.relief` (a slope
//! and an optional wall) and a reference radar written against the core `Sensor` trait.
//!
//! The example plugins are built through `plugin_support::library` (a nested `cargo build` into
//! `CARGO_TARGET_TMPDIR`, honouring `CARGO_TARGET_DIR`), never read from `target/plugins`; each
//! package gets the `plugin.toml` the example ships, like `xtask build-plugins` packages it. The
//! including binary declares `mod plugin_support;` too. Each test binary uses a subset.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use umbra_core::{
    Access, BbType, BbValue, BehaviorContext, Blackboard, BlackboardParam, BuildError, ComponentId,
    ComponentInfo, Condition, Contact, ContactBuffer, Ecef, EntityView, Enu, Factory, Geodetic,
    Meters, Observer, ParamSchema, ParamValue, Provenance, Registry, SenseContext, Sensor, Target,
    TerrainProvider, Vec3, Version, math,
};
use umbra_engine::builtins::standard_registry;

/// North metres per degree of latitude and east metres per degree of longitude at 52 N (the
/// conversion the example scenarios use).
pub const M_PER_DEG_LAT: f64 = 111_267.0;
pub const M_PER_DEG_LON: f64 = 68_684.0;

/// The latitude of `north_m` metres north of the 52 N, 4 E origin, degrees.
pub fn lat(north_m: f64) -> f64 {
    52.0 + north_m / M_PER_DEG_LAT
}

/// The longitude of `east_m` metres east of the origin, degrees.
pub fn lon(east_m: f64) -> f64 {
    4.0 + east_m / M_PER_DEG_LON
}

pub use super::plugin_support::example_plugins;

/// Writes `files` (`(relative path, text)`) under a fresh directory and returns the directory.
pub fn write_dir(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "m7-kinds/{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create directories");
        std::fs::write(&path, text).expect("write a file");
    }
    dir
}

/// A fresh file path under the target tmp dir (not created).
pub fn scratch_file(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("m7-kinds-files");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!(
        "{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ))
}

// ------------------------------------------------------------------------------------- terrain

/// The longitude origin of the relief, degrees.
const RELIEF_LON0_DEG: f64 = 4.0;
/// Metres per radian of longitude at 52 N on the equatorial radius (the relief is a test function,
/// not geodesy: plugin and reference query the same function).
const RELIEF_M_PER_RAD: f64 = 6_378_137.0 * 0.615_661_475_325_658_3;

/// Terrain `test.terrain.relief`: height `slope * east_m` (east of 4 E) plus an optional wall of
/// `wall_height_m` for longitudes in `[wall_lon_lo_deg, wall_lon_hi_deg]`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Relief {
    pub slope: f64,
    pub wall: Option<(f64, f64, f64)>,
}

impl TerrainProvider for Relief {
    fn height_at(&self, at: Geodetic) -> Meters {
        let east_m = (at.lon().0 - RELIEF_LON0_DEG.to_radians()) * RELIEF_M_PER_RAD;
        let wall = self.wall.map_or(0.0, |(lo, hi, h)| {
            if (lo.to_radians()..=hi.to_radians()).contains(&at.lon().0) {
                h
            } else {
                0.0
            }
        });
        Meters(self.slope * east_m + wall)
    }

    fn surface_normal_at(&self, _: Geodetic) -> Enu {
        let norm = math::hypot(self.slope, 1.0);
        Enu {
            east: -self.slope / norm,
            north: 0.0,
            up: 1.0 / norm,
        }
    }
}

const RELIEF_ID: &str = "test.terrain.relief";
const RELIEF_SCHEMA: &str = r#"{"type":"object","description":"Test relief","properties":{
    "slope":{"type":"number"},
    "wall_lon_lo_deg":{"type":"number"},"wall_lon_hi_deg":{"type":"number"},
    "wall_height_m":{"type":"number"}},"additionalProperties":false}"#;

struct ReliefFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn TerrainProvider> for ReliefFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn TerrainProvider>, BuildError> {
        let number = |key: &str| params.get(key).and_then(ParamValue::as_f64);
        let wall = match (
            number("wall_lon_lo_deg"),
            number("wall_lon_hi_deg"),
            number("wall_height_m"),
        ) {
            (Some(lo), Some(hi), Some(h)) => Some((lo, hi, h)),
            _ => None,
        };
        Ok(Arc::new(Relief {
            slope: number("slope").unwrap_or(0.0),
            wall,
        }))
    }
}

/// The relief provider as the scenario builds it from its `[terrain] params` text.
pub fn relief(slope: f64, wall: Option<(f64, f64, f64)>) -> Relief {
    Relief { slope, wall }
}

/// The standard registry plus the relief terrain.
pub fn registry_with_relief() -> Registry {
    let mut registry = standard_registry().expect("the standard registry builds");
    registry
        .register_terrain(Arc::new(ReliefFactory {
            id: ComponentId::new(RELIEF_ID).expect("id"),
            schema: ParamSchema::from_json_str(RELIEF_SCHEMA).expect("schema"),
        }))
        .expect("register the relief terrain");
    registry
}

// ---------------------------------------------------------------------------- reference radar

/// The reference implementation of `example.sensor.radar` (its documented contract, written against
/// the core `Sensor` trait) registered as `test.sensor.radar_ref` by [`registry_with_reference_radar`].
/// The elevation gate and the antenna raise use the geocentric vertical, as the contract says.
struct RadarRef {
    id: ComponentId,
    range_m: f64,
    min_elevation_deg: f64,
    max_elevation_deg: f64,
    antenna_height_m: f64,
    detect_probability: f64,
}

impl RadarRef {
    /// `p` raised by the antenna height along its geocentric vertical.
    fn raised(&self, p: Ecef) -> Ecef {
        Ecef(p.0 + p.0.normalized().unwrap_or(Vec3::ZERO) * self.antenna_height_m)
    }
}

impl Sensor for RadarRef {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn sense(
        &self,
        ctx: &mut SenseContext<'_>,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        out: &mut ContactBuffer<'_>,
    ) {
        let from = observer.state.position;
        let up = from.0.normalized();
        for candidate in candidates {
            let to = candidate.state.position;
            let d = to - from;
            let range = d.norm();
            if range > self.range_m {
                continue;
            }
            let elevation = match up {
                Some(up) if range > 0.0 => math::asin(d.dot(up) / range).to_degrees(),
                _ => 0.0,
            };
            if elevation < self.min_elevation_deg || elevation > self.max_elevation_deg {
                continue;
            }
            if !ctx
                .terrain
                .line_of_sight(self.raised(from), self.raised(to))
            {
                continue;
            }
            if ctx.rng.next_f64() < self.detect_probability {
                out.push(Contact::new(
                    candidate.id,
                    to,
                    candidate.state.velocity,
                    candidate.force.cloned(),
                    Meters(range),
                ));
            }
        }
    }
}

struct RadarRefFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl Factory<dyn Sensor> for RadarRefFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Sensor>, BuildError> {
        let number = |key: &str, default: f64| {
            params
                .get(key)
                .and_then(ParamValue::as_f64)
                .unwrap_or(default)
        };
        Ok(Arc::new(RadarRef {
            id: self.id.clone(),
            range_m: number("range_m", 0.0),
            min_elevation_deg: number("min_elevation_deg", -90.0),
            max_elevation_deg: number("max_elevation_deg", 90.0),
            antenna_height_m: number("antenna_height_m", 2.0),
            detect_probability: number("detect_probability", 1.0),
        }))
    }
}

pub const RADAR_REF_ID: &str = "test.sensor.radar_ref";

/// [`registry_with_relief`] plus the reference radar.
pub fn registry_with_reference_radar() -> Registry {
    let mut registry = registry_with_relief();
    registry
        .register_sensor(Arc::new(RadarRefFactory {
            id: ComponentId::new(RADAR_REF_ID).expect("id"),
            schema: ParamSchema::from_json_str(
                r#"{"type":"object","description":"Reference radar","properties":{
                    "range_m":{"type":"number"},"min_elevation_deg":{"type":"number"},
                    "max_elevation_deg":{"type":"number"},"antenna_height_m":{"type":"number"},
                    "detect_probability":{"type":"number"}}}"#,
            )
            .expect("schema"),
        }))
        .expect("register the reference radar");
    registry
}

// -------------------------------------------------------------------------- probe condition

/// What `test.cond.probe` saw: the value of its `key` (a bool blackboard key) at each check, `None`
/// for unset.
pub type ProbeLog = Arc<std::sync::Mutex<Vec<Option<bool>>>>;

struct Probe {
    key: String,
    log: ProbeLog,
}

impl Condition for Probe {
    fn id(&self) -> &ComponentId {
        static ID: std::sync::LazyLock<ComponentId> =
            std::sync::LazyLock::new(|| ComponentId::new(PROBE_ID).expect("id"));
        &ID
    }

    fn check(&self, _: &mut BehaviorContext<'_>, _: &EntityView<'_>, bb: &mut Blackboard) -> bool {
        let seen = bb.get(&self.key).map(|v| matches!(v, BbValue::Bool(true)));
        self.log.lock().expect("probe log").push(seen);
        true
    }
}

struct ProbeFactory {
    id: ComponentId,
    schema: ParamSchema,
    log: ProbeLog,
}

impl Factory<dyn Condition> for ProbeFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Condition>, BuildError> {
        let key = params
            .get("key")
            .and_then(ParamValue::as_str)
            .ok_or_else(|| BuildError::new("`key` is required"))?;
        Ok(Arc::new(Probe {
            key: key.to_owned(),
            log: self.log.clone(),
        }))
    }
}

pub const PROBE_ID: &str = "test.cond.probe";

/// The standard registry plus the condition `test.cond.probe` (param `key`, a bool key it reads, always
/// true) and the log it fills.
pub fn registry_with_probe() -> (Registry, ProbeLog) {
    let log = ProbeLog::default();
    let mut registry = standard_registry().expect("the standard registry builds");
    registry
        .register_condition(
            Arc::new(ProbeFactory {
                id: ComponentId::new(PROBE_ID).expect("id"),
                schema: ParamSchema::from_json_str(
                    r#"{"type":"object","description":"Records a bool key","properties":{
                        "key":{"type":"string"}},"required":["key"],"additionalProperties":false}"#,
                )
                .expect("schema"),
                log: log.clone(),
            }),
            ComponentInfo::new("probe", Provenance::Plugin, Version::new(0, 1, 0))
                .with_blackboard_param(BlackboardParam::new("key", BbType::Bool, Access::Read)),
        )
        .expect("register the probe");
    (registry, log)
}
