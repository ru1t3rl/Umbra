//! Steering helpers shared by `builtin.sea.kinematic` (WI-5) and `builtin.air.kinematic` (WI-6)
//! (ADR 0051, ASM-611). `builtin.ground.kinematic` keeps its own copy so its goldens stay byte-identical.
//!
//! Private to the crate: intent reduction by the ASM-030 / ADR 0016 rules and the horizontal
//! geometry to a destination. All trigonometry goes through `umbra_core::math` (ADR 0003).

use umbra_core::math;
use umbra_core::{Ecef, Intent, LocalFrame, Radians};

/// Smallest distance (meters) at which the arrival snap applies even for a stopped vessel.
pub(crate) const ARRIVAL_EPSILON_M: f64 = 1e-3;

/// Intents reduced by the ASM-030 rules; speed values are raw (not yet clamped).
pub(crate) struct Steering {
    /// `Hold` was present: it wins over everything (`destination` and `heading` are then `None`).
    pub hold: bool,
    /// The last steering intent was a (finite) `MoveTo`.
    pub destination: Option<Ecef>,
    /// The last steering intent was a (finite) `SetHeading`, radians.
    pub heading: Option<f64>,
    /// The last `SetSpeed`; a non-finite request means "stop" (ASM-048).
    pub set_speed: Option<f64>,
}

fn is_finite_ecef(p: Ecef) -> bool {
    p.0.x.is_finite() && p.0.y.is_finite() && p.0.z.is_finite()
}

/// Reduces `intents`: `Hold` wins; the last of `MoveTo` and `SetHeading` steers (ADR 0016);
/// the last `SetSpeed` wins; non-finite destinations and headings and unknown variants are ignored.
pub(crate) fn reduce(intents: &[Intent]) -> Steering {
    let mut s = Steering {
        hold: false,
        destination: None,
        heading: None,
        set_speed: None,
    };
    for intent in intents {
        match intent {
            Intent::Hold => s.hold = true,
            Intent::MoveTo { destination } if is_finite_ecef(*destination) => {
                s.destination = Some(*destination);
                s.heading = None;
            }
            Intent::SetHeading(h) if h.0.is_finite() => {
                s.heading = Some(h.0);
                s.destination = None;
            }
            Intent::SetSpeed(v) => s.set_speed = Some(if v.0.is_finite() { v.0 } else { 0.0 }),
            _ => {}
        }
    }
    if s.hold {
        s.destination = None;
        s.heading = None;
    }
    s
}

/// Horizontal geometry to a destination in the entity's local frame.
pub(crate) struct Approach {
    /// Meters east of the entity.
    pub east: f64,
    /// Meters north of the entity.
    pub north: f64,
    /// Horizontal distance, meters.
    pub distance: f64,
    /// Heading error, radians, in (-pi, pi].
    pub error: f64,
}

/// The approach from the entity (at the frame origin, on `heading`) to `destination`.
pub(crate) fn approach(frame: &LocalFrame, destination: Ecef, heading: f64) -> Approach {
    let offset = frame.to_enu(destination);
    let distance = math::hypot(offset.east, offset.north);
    let error = if distance <= 1e-12 {
        0.0
    } else {
        math::wrap_pi(Radians(math::atan2(offset.east, offset.north) - heading)).0
    };
    Approach {
        east: offset.east,
        north: offset.north,
        distance,
        error,
    }
}

/// The signed heading error to turn out: toward the destination, else toward the commanded
/// heading, else `None`.
pub(crate) fn turn_error(
    toward: Option<&Approach>,
    commanded: Option<f64>,
    heading: f64,
) -> Option<f64> {
    toward
        .map(|a| a.error)
        .or_else(|| commanded.map(|h| math::wrap_pi(Radians(h - heading)).0))
}

/// The largest speed from which repeated decelerations of `accel * dt` stop within `distance`:
/// `v = sqrt(h^2 + 2 a d) - h` with `h = a dt / 2`.
pub(crate) fn stopping_speed(accel: f64, dt: f64, distance: f64) -> f64 {
    let h = 0.5 * accel * dt;
    (h * h + 2.0 * accel * distance).sqrt() - h
}
