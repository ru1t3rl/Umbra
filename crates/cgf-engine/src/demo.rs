//! The hard-coded M1 demo scenario (ASM-028). Replaced by a scenario file in M2.
//!
//! Three ground entities near 52 N 4 E on flat terrain at height 0. Each follows a square route
//! of four 500 m legs (arrival radius 5 m), placed and routed by lat/lon (ASM-044). Entity 1 is
//! "the hard-coded entity". Cruise speed is `max_speed * (0.8 + 0.2 * u)` with `u` drawn from the
//! scenario random stream of the entity's index (ADR 0006).

use std::sync::Arc;

use cgf_core::{
    EntityId, Enu, Geodetic, LocalFrame, Meters, MetersPerSecond, MetersPerSecondSquared,
    MotionModel, Ownership, Position, Radians, RadiansPerSecond, StreamKey,
};
use cgf_motion::{GroundKinematic, GroundKinematicParams};

use crate::engine::{Engine, RouteSpec, SpawnSpec};
use crate::error::EngineError;

/// Latitude and longitude, degrees, of the first entity's start corner.
pub const ORIGIN_DEG: (f64, f64) = (52.0, 4.0);

const ARRIVAL_RADIUS_M: f64 = 5.0;
const LEG_M: f64 = 500.0;
/// Start corners are this many degrees of latitude apart (about 2.2 km), so routes never meet.
const CORNER_SPACING_DEG: f64 = 0.02;

/// Ground model limits of the spec 7.1 example: 18 m/s, 40 deg/s, 2.5 m/s^2.
pub fn ground_params() -> GroundKinematicParams {
    GroundKinematicParams {
        max_speed: MetersPerSecond(18.0),
        max_turn_rate: RadiansPerSecond::from_degrees_per_second(40.0),
        max_accel: MetersPerSecondSquared(2.5),
    }
}

/// Route shape per entity as (east, north) offsets in meters from the start corner, in visiting
/// order, and the heading of the first leg in degrees.
fn shape(index: usize) -> ([(f64, f64); 4], f64) {
    match index % 3 {
        0 => ([(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)], 90.0),
        1 => ([(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)], 0.0),
        _ => ([(1.0, 0.0), (1.0, -1.0), (0.0, -1.0), (0.0, 0.0)], 90.0),
    }
}

fn corner(index: usize) -> Option<Geodetic> {
    // The index is below 3 in practice; the conversion cannot fail for such values.
    let step = f64::from(u32::try_from(index).ok()?);
    Geodetic::from_degrees(ORIGIN_DEG.0 + step * CORNER_SPACING_DEG, ORIGIN_DEG.1, 0.0).ok()
}

/// The start corner and the route waypoints of entity `index` (0-based), as geodetic positions
/// at height 0. The entity starts at the corner; the last waypoint is the corner again.
fn layout(index: usize) -> Option<(Geodetic, Vec<Geodetic>)> {
    let start = corner(index)?;
    let frame = LocalFrame::at(start);
    let waypoints = shape(index)
        .0
        .iter()
        .map(|&(e, n)| {
            let on_plane = frame
                .from_enu(Enu {
                    east: e * LEG_M,
                    north: n * LEG_M,
                    up: 0.0,
                })
                .to_geodetic();
            on_plane.with_height(Meters(0.0)).unwrap_or(on_plane)
        })
        .collect();
    Some((start, waypoints))
}

/// The waypoints of entity `index` (0-based) in visiting order. Empty for an index that has no
/// valid start corner.
pub fn route_of(index: usize) -> Vec<Geodetic> {
    layout(index).map(|(_, w)| w).unwrap_or_default()
}

/// Spawns the three demo entities with ids 1, 2 and 3, in order.
///
/// # Errors
/// [`EngineError`] if the model parameters or a spawn are rejected (not expected).
pub fn spawn_walking_skeleton(engine: &mut Engine) -> Result<Vec<EntityId>, EngineError> {
    let params = ground_params();
    let model: Arc<dyn MotionModel> = Arc::new(GroundKinematic::new(params)?);
    let mut ids = Vec::with_capacity(3);
    for i in 0..3_usize {
        let Some((start, waypoints)) = layout(i) else {
            return Err(EngineError::DemoLayout(i));
        };
        let index = u64::try_from(i).map_err(|_| EngineError::DemoLayout(i))?;
        let u = engine.rng().stream(StreamKey::scenario(index)).next_f64();
        let spec = SpawnSpec {
            position: Position::Geodetic(start),
            heading: Radians::from_degrees(shape(i).1),
            speed: MetersPerSecond(0.0),
            ownership: Ownership::Owned,
            motion: Arc::clone(&model),
            route: Some(RouteSpec {
                waypoints: waypoints.into_iter().map(Position::Geodetic).collect(),
                arrival_radius: Meters(ARRIVAL_RADIUS_M),
                cruise_speed: MetersPerSecond(params.max_speed.0 * (0.8 + 0.2 * u)),
            }),
        };
        ids.push(engine.spawn(spec)?);
    }
    Ok(ids)
}
