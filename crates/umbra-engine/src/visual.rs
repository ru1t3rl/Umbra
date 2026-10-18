//! `builtin.visual`, the first sensor (ADR 0015, ASM-080, ASM-108).
//!
//! Perfect detection. Per candidate the sensor checks the 3D range, then the horizontal field of
//! view, and only then asks the terrain for line of sight (the expensive call). It reads the
//! pre-tick states it is given and draws no random numbers.

use std::sync::Arc;

use umbra_core::{
    BuildError, ComponentId, Contact, ContactBuffer, Ecef, Factory, LocalFrame, Meters, Observer,
    ParamSchema, ParamValue, Radians, SenseContext, Sensor, Target, TerrainProvider, math,
};

/// ID of the visual sensor.
pub const VISUAL_SENSOR_ID: &str = "builtin.visual";

const SCHEMA_JSON: &str = include_str!("../schemas/builtin.visual.json");
/// Default `eye_height_m` (ADR 0015).
const DEFAULT_EYE_HEIGHT_M: f64 = 2.0;
/// Largest `range_m` (ADR 0015): 100 km.
const MAX_RANGE_M: f64 = 100_000.0;
/// Horizontal offsets below this have no meaningful bearing, meters (ASM-108).
const NO_BEARING_M: f64 = 1.0e-3;

/// Builds `builtin.visual` from `{ range_m, fov_deg, eye_height_m? }`.
#[derive(Debug)]
pub struct VisualSensorFactory {
    id: ComponentId,
    schema: ParamSchema,
}

impl VisualSensorFactory {
    /// The factory, with its schema from `schemas/builtin.visual.json`.
    ///
    /// # Panics
    /// Never in practice: the id constant and the embedded schema are valid (unit-tested).
    #[expect(
        clippy::expect_used,
        reason = "embedded constants, proven valid by tests"
    )]
    pub fn new() -> Self {
        Self {
            id: ComponentId::new(VISUAL_SENSOR_ID).expect("built-in id is valid"),
            schema: ParamSchema::from_json_str(SCHEMA_JSON).expect("embedded schema is valid"),
        }
    }
}

impl Default for VisualSensorFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Factory<dyn Sensor> for VisualSensorFactory {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.schema
    }

    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Sensor>, BuildError> {
        let range = number(params, "range_m", None)?;
        let fov = number(params, "fov_deg", None)?;
        let eye = number(params, "eye_height_m", Some(DEFAULT_EYE_HEIGHT_M))?;
        if !(range > 0.0 && range <= MAX_RANGE_M) {
            return Err(BuildError::new("`range_m` must be in (0, 100000]").at("/range_m"));
        }
        if !(fov > 0.0 && fov <= 360.0) {
            return Err(BuildError::new("`fov_deg` must be in (0, 360]").at("/fov_deg"));
        }
        if eye < 0.0 {
            return Err(BuildError::new("`eye_height_m` must be at least 0").at("/eye_height_m"));
        }
        Ok(Arc::new(VisualSensor {
            id: self.id.clone(),
            range: Meters(range),
            half_fov: Radians::from_degrees(fov / 2.0),
            eye: Meters(eye),
        }))
    }
}

/// A finite number parameter; `default` makes it optional.
fn number(params: &ParamValue, key: &str, default: Option<f64>) -> Result<f64, BuildError> {
    let at = format!("/{key}");
    match (params.get(key), default) {
        (None, Some(d)) => Ok(d),
        (None, None) => Err(BuildError::new(format!("missing parameter `{key}`")).at(at)),
        (Some(v), _) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| BuildError::new(format!("`{key}` must be a finite number")).at(at)),
    }
}

/// The built sensor.
struct VisualSensor {
    id: ComponentId,
    range: Meters,
    half_fov: Radians,
    eye: Meters,
}

impl VisualSensor {
    /// Whether the target lies within the horizontal field of view (edge inclusive). A target
    /// less than a millimetre away horizontally (straight above or below the observer) has no
    /// bearing and counts as inside (ASM-108).
    fn in_fov(&self, frame: &LocalFrame, heading: Radians, to: Ecef) -> bool {
        let enu = frame.to_enu(to);
        if math::hypot(enu.east, enu.north) < NO_BEARING_M {
            return true;
        }
        let bearing = Radians(math::atan2(enu.east, enu.north));
        math::wrap_pi(Radians(bearing.0 - heading.0)).0.abs() <= self.half_fov.0
    }

    /// Whether the terrain shows `to` from `from` (the observer's point, raised once per call)
    /// with `to` raised by the eye height along local up.
    fn has_line_of_sight(&self, terrain: &dyn TerrainProvider, from: Ecef, to: Ecef) -> bool {
        self.raised(to)
            .is_some_and(|to| terrain.line_of_sight(from, to))
    }

    fn raised(&self, p: Ecef) -> Option<Ecef> {
        let g = p.to_geodetic();
        let up = g.with_height(Meters(g.height().0 + self.eye.0)).ok()?;
        Some(up.to_ecef())
    }
}

impl Sensor for VisualSensor {
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
        let me = observer.state.position;
        let frame = LocalFrame::at(me.to_geodetic());
        // An observer whose raised point is not finite sees nothing.
        let Some(eyes) = self.raised(me) else {
            return;
        };
        let heading = frame
            .euler_from_orientation(observer.state.orientation)
            .heading;
        for target in candidates {
            let there = target.state.position;
            let range = me.distance(there);
            // NaN ranges fail this comparison and are skipped.
            let within_range = range.0 <= self.range.0;
            if !within_range
                || !self.in_fov(&frame, heading, there)
                || !self.has_line_of_sight(ctx.terrain, eyes, there)
            {
                continue;
            }
            out.push(Contact::new(
                target.id,
                there,
                target.state.velocity,
                target.force.cloned(),
                range,
            ));
        }
    }
}
