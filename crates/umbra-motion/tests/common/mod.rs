//! Shared helpers for the M7 air and sea model tests (ADR 0051): start states, headings, heights,
//! a sloped test terrain and a step loop. Every state is near 52 N 4 E, as in the ground tests.
#![expect(
    dead_code,
    reason = "each test binary uses a subset of the shared helpers"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::f64::consts::{PI, TAU};

use umbra_core::{
    Ecef, Enu, Euler, FlatTerrain, Geodetic, Intent, KinematicState, LocalFrame, Meters,
    MotionModel, Radians, Seconds, TerrainProvider,
};

/// 30 Hz, the scenario default.
pub const HZ: f64 = 30.0;
/// One tick.
pub const DT: Seconds = Seconds(1.0 / HZ);

pub fn origin() -> Geodetic {
    Geodetic::from_degrees(52.0, 4.0, 0.0).expect("valid origin")
}

pub fn flat() -> FlatTerrain {
    FlatTerrain {
        height: Meters(0.0),
    }
}

/// Terrain whose height grows with the distance north of the origin by `slope` meters per meter
/// (a test oracle: the floor checks call the same `height_at` the model does).
pub struct SlopedTerrain {
    pub slope: f64,
}

impl TerrainProvider for SlopedTerrain {
    fn height_at(&self, at: Geodetic) -> Meters {
        let north_m = (at.lat().0 - origin().lat().0) * 6_367_000.0;
        Meters(self.slope * north_m)
    }

    fn surface_normal_at(&self, _at: Geodetic) -> Enu {
        Enu {
            east: 0.0,
            north: 0.0,
            up: 1.0,
        }
    }
}

/// An entity at the origin at `height` meters above the ellipsoid, flying or sailing along true
/// `heading` (radians) at `speed` m/s, level.
pub fn start(heading: f64, speed: f64, height: f64) -> KinematicState {
    let g = Geodetic::from_degrees(52.0, 4.0, height).expect("valid position");
    let frame = LocalFrame::at(g);
    let (s, c) = umbra_core::math::sin_cos(heading);
    KinematicState {
        position: g.to_ecef(),
        velocity: frame.vector_from_enu(Enu {
            east: speed * s,
            north: speed * c,
            up: 0.0,
        }),
        orientation: frame.orientation_from_euler(Euler {
            heading: Radians(heading),
            pitch: Radians(0.0),
            roll: Radians(0.0),
        }),
    }
}

/// A point `east`/`north` meters from the origin at `up` meters above the origin's height.
pub fn dest(east: f64, north: f64, up: f64) -> Ecef {
    LocalFrame::at(origin()).from_enu(Enu { east, north, up })
}

pub fn move_to(east: f64, north: f64, up: f64) -> Intent {
    Intent::MoveTo {
        destination: dest(east, north, up),
    }
}

pub fn speed_of(s: &KinematicState) -> f64 {
    s.velocity.norm()
}

pub fn euler_of(s: &KinematicState) -> Euler {
    LocalFrame::at(s.position.to_geodetic()).euler_from_orientation(s.orientation)
}

pub fn heading_of(s: &KinematicState) -> f64 {
    euler_of(s).heading.0
}

pub fn height_of(s: &KinematicState) -> f64 {
    s.position.to_geodetic().height().0
}

/// Signed smallest difference `a - b` wrapped to (-pi, pi].
pub fn angle_delta(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % TAU;
    if d > PI {
        d -= TAU;
    }
    if d <= -PI {
        d += TAU;
    }
    d
}

/// Vertical speed (m/s) in the local frame.
pub fn climb_rate_of(s: &KinematicState) -> f64 {
    LocalFrame::at(s.position.to_geodetic())
        .vector_to_enu(s.velocity)
        .up
}

/// The states after each of `steps` steps (the start state is not included).
pub fn run(
    m: &dyn MotionModel,
    terrain: &dyn TerrainProvider,
    from: KinematicState,
    intents: &[Intent],
    steps: usize,
) -> Vec<KinematicState> {
    let mut out = Vec::with_capacity(steps);
    let mut s = from;
    for _ in 0..steps {
        s = m.step(&s, intents, DT, terrain);
        out.push(s);
    }
    out
}

/// `from` followed by `states`, for pairwise comparisons.
pub fn with_start(from: KinematicState, states: &[KinematicState]) -> Vec<KinematicState> {
    let mut all = vec![from];
    all.extend_from_slice(states);
    all
}
