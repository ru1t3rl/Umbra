//! Display conversion of snapshot entities (ASM-044).
//!
//! The simulation is ECEF-only; front ends (the CLI today, the API later) show lat/lon/alt in
//! degrees and meters, and a compass heading. This is the one place that conversion lives.

use cgf_core::{EntityId, LocalFrame};
use cgf_kernel::EntitySnapshot;

/// An entity in display units: degrees and meters, heading clockwise from true north.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityDisplay {
    /// The entity id.
    pub id: EntityId,
    /// WGS-84 latitude, degrees.
    pub lat_deg: f64,
    /// WGS-84 longitude, degrees.
    pub lon_deg: f64,
    /// Height above the WGS-84 ellipsoid, meters.
    pub height_m: f64,
    /// Heading in `[0, 360)` degrees, clockwise from true north.
    pub heading_deg: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// `(next, len)` of the entity's route, if any.
    pub route_next: Option<(usize, usize)>,
}

impl EntityDisplay {
    /// Converts a snapshot entity.
    pub fn from_snapshot(e: &EntitySnapshot) -> Self {
        let g = e.geodetic();
        let heading = LocalFrame::at(g)
            .euler_from_orientation(e.state.orientation)
            .heading;
        Self {
            id: e.id,
            lat_deg: g.lat().to_degrees(),
            lon_deg: g.lon().to_degrees(),
            height_m: g.height().0,
            heading_deg: heading.to_degrees().rem_euclid(360.0),
            speed_mps: e.state.velocity.norm(),
            route_next: e.route_next,
        }
    }
}
