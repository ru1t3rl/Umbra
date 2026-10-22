//! Dead reckoning: extrapolating a remote entity's state from its last reported basis
//! (REQ-N-6, ADR 0032).
//!
//! Shared by the publisher (to decide when an update is due) and the receiver (to move ghosts),
//! so both sides extrapolate identically. All inputs and outputs are SI and ECEF (D-15, REQ-C-4);
//! non-exact functions go through [`crate::math`] (ADR 0003).

use crate::geo::{Quaternion, Vec3};
use crate::motion::KinematicState;
use crate::units::{Radians, Seconds};

/// How a remote entity's state is extrapolated. Neutral names; each adapter maps them to its own
/// codes (ADR 0032).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeadReckoning {
    /// Does not move: the basis state is returned.
    Static,
    /// Constant velocity in ECEF.
    Velocity,
    /// Constant velocity, orientation turning at a constant body-axis rate.
    VelocityRotation,
    /// Constant acceleration in ECEF, orientation turning at a constant body-axis rate.
    AccelerationRotation,
    /// Constant acceleration in ECEF.
    Acceleration,
    /// Like [`Self::Velocity`] (body-frame form, ASM-229).
    BodyVelocity,
    /// Like [`Self::VelocityRotation`] (body-frame form, ASM-229).
    BodyVelocityRotation,
    /// Like [`Self::AccelerationRotation`], the acceleration given in body axes (ASM-229).
    BodyAccelerationRotation,
    /// Like [`Self::Acceleration`], the acceleration given in body axes (ASM-229).
    BodyAcceleration,
}

impl DeadReckoning {
    /// Whether the orientation turns with the angular velocity.
    fn rotates(self) -> bool {
        matches!(
            self,
            Self::VelocityRotation
                | Self::AccelerationRotation
                | Self::BodyVelocityRotation
                | Self::BodyAccelerationRotation
        )
    }

    /// Whether the acceleration is used.
    fn accelerates(self) -> bool {
        matches!(
            self,
            Self::AccelerationRotation
                | Self::Acceleration
                | Self::BodyAccelerationRotation
                | Self::BodyAcceleration
        )
    }

    /// Whether the acceleration is given in body axes.
    fn body_frame(self) -> bool {
        matches!(
            self,
            Self::BodyVelocity
                | Self::BodyVelocityRotation
                | Self::BodyAccelerationRotation
                | Self::BodyAcceleration
        )
    }
}

/// The last reported state of a remote entity and how to extrapolate it (ADR 0032).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemoteState {
    /// State at the basis time: ECEF position and velocity, body FRD to ECEF orientation.
    /// The velocity is in ECEF axes for every model; an adapter whose protocol reports a
    /// body-axis velocity rotates it into ECEF with the basis orientation before building this.
    pub state: KinematicState,
    /// Linear acceleration in m/s^2: ECEF axes, or body axes for the body-frame models.
    pub acceleration: Vec3,
    /// Angular velocity in rad/s about the body axes.
    pub angular_velocity: Vec3,
    /// The extrapolation model.
    pub model: DeadReckoning,
}

/// The state `elapsed` after the basis, extrapolated with the basis' model (ADR 0032).
///
/// A zero, negative or non-finite `elapsed` returns the basis state. The caller caps `elapsed`
/// (the ghost timeout). Position is `p + v t (+ a t^2 / 2)`, velocity `v (+ a t)`; for the
/// rotating models the orientation is turned by `angular_velocity * t` about the body axes,
/// exactly, and renormalized.
#[must_use]
pub fn extrapolate(basis: &RemoteState, elapsed: Seconds) -> KinematicState {
    let t = elapsed.0;
    let s = basis.state;
    if !(t.is_finite() && t > 0.0) || basis.model == DeadReckoning::Static {
        return s;
    }
    let mut out = s;
    out.position.0 = s.position.0 + s.velocity * t;
    if basis.model.accelerates() {
        let a = if basis.model.body_frame() {
            s.orientation.rotate(basis.acceleration)
        } else {
            basis.acceleration
        };
        out.position.0 = out.position.0 + a * (0.5 * t * t);
        out.velocity = s.velocity + a * t;
    }
    if basis.model.rotates() {
        out.orientation = rotate_body(s.orientation, basis.angular_velocity * t);
    }
    if is_finite(&out) { out } else { s }
}

/// Whether every component of `s` is finite.
fn is_finite(s: &KinematicState) -> bool {
    let (p, v, q) = (s.position.0, s.velocity, s.orientation);
    [p.x, p.y, p.z, v.x, v.y, v.z, q.w, q.x, q.y, q.z]
        .iter()
        .all(|c| c.is_finite())
}

/// `q` turned by the rotation vector `turn` (radians, body axes): `q * exp(turn)`.
fn rotate_body(q: Quaternion, turn: Vec3) -> Quaternion {
    let angle = turn.norm();
    let r = match Quaternion::from_axis_angle(turn, Radians(angle)) {
        Some(d) => q.mul(d),
        None => q,
    };
    r.normalized().unwrap_or(r)
}
