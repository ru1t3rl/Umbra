//! The M1 route-following stand-in for the Behavior phase (REQ-K-2, ASM-029).
//!
//! It stays in M3 (ASM-082, ADR 0016): it is the default for Owned entities without a behavior
//! and the expansion of the `FollowRoute` intent. It reads the entity's position and writes
//! intents; it never touches entity state (REQ-B-1).

use cgf_core::{Ecef, Intent, LocalFrame, MetersPerSecond, math};

use crate::world::Route;

/// Horizontal distance from `position` to `target`, in the local frame at `position`.
fn horizontal_distance(position: Ecef, target: Ecef) -> f64 {
    let offset = LocalFrame::at(position.to_geodetic()).to_enu(target);
    math::hypot(offset.east, offset.north)
}

/// Advances the route when the next waypoint is reached and writes this tick's intents:
/// `[SetSpeed(cruise), MoveTo(next)]`, or `[Hold]` once the last waypoint is reached.
pub(crate) fn steer(route: &mut Route, position: Ecef, intents: &mut Vec<Intent>) {
    let last = route.waypoints.len().saturating_sub(1);
    let Some(target) = route.waypoints.get(route.next).copied() else {
        intents.push(Intent::Hold);
        return;
    };
    if horizontal_distance(position, target) <= route.arrival_radius {
        if route.next >= last {
            intents.push(Intent::Hold);
            return;
        }
        route.next += 1;
    }
    let destination = route.waypoints.get(route.next).copied().unwrap_or(target);
    intents.push(Intent::SetSpeed(MetersPerSecond(route.cruise)));
    intents.push(Intent::MoveTo { destination });
}

/// Whether the route is at its last waypoint and within the arrival radius of it, where
/// [`steer`] holds.
pub(crate) fn is_complete(route: &Route, position: Ecef) -> bool {
    let last = route.waypoints.len().saturating_sub(1);
    route.next >= last
        && route
            .waypoints
            .get(last)
            .is_none_or(|w| horizontal_distance(position, *w) <= route.arrival_radius)
}
