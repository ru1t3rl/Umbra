//! Entities (REQ-X-5, REQ-K-5, ADR 0026, ADR 0027). Ids and ticks are JSON integers, exact below
//! 2^53; angles are degrees, lengths metres, speeds metres per second.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Who simulates an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    /// Simulated by this engine.
    Owned,
    /// Simulated elsewhere and mirrored here.
    Remote,
}

/// How far along its route an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RouteProgress {
    /// Index of the next waypoint.
    pub next: u32,
    /// Number of waypoints.
    pub len: u32,
}

/// An entity as listed by `GET /entities` and carried in stream frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EntitySummary {
    /// The entity id; never reused.
    pub id: u64,
    /// Its unique name, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Its template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Its group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Its force.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<String>,
    /// Owned or remote.
    pub ownership: Ownership,
    /// Latitude, degrees (WGS-84).
    pub lat_deg: f64,
    /// Longitude, degrees.
    pub lon_deg: f64,
    /// Altitude above the ellipsoid, metres.
    pub alt_m: f64,
    /// Heading, degrees clockwise from north.
    pub heading_deg: f64,
    /// Ground speed, metres per second.
    pub speed_mps: f64,
    /// The behavior id, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    /// The behavior's status in the last tick.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Route progress, if it has a route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteProgress>,
}

/// `GET /entities/{id}`: the summary plus detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EntityDetail {
    /// The same fields as in a listing.
    pub summary: EntitySummary,
    /// The motion model id.
    pub motion_model: String,
    /// Ids of the entities in contact, ascending.
    pub contacts: Vec<u64>,
}

/// One page of `GET /entities` (keyset paging by id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EntityPage {
    /// The tick this page was read at; pages of one listing may differ.
    pub tick: u64,
    /// Entities matching the filter, over all pages.
    pub total: u64,
    /// The entities, ascending by id.
    pub items: Vec<EntitySummary>,
    /// Pass as `after_id` for the next page; `null` on the last page.
    #[schema(required = true)]
    pub next_after_id: Option<u64>,
}

/// `POST /entities`: spawn from a template of the loaded scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SpawnRequest {
    /// The template id.
    pub template: String,
    /// Latitude, degrees.
    pub lat_deg: f64,
    /// Longitude, degrees.
    pub lon_deg: f64,
    /// Altitude above the ellipsoid, metres; default 0.
    #[serde(default)]
    pub alt_m: f64,
    /// Heading, degrees clockwise from north; default 0.
    #[serde(default)]
    pub heading_deg: f64,
    /// Initial speed, metres per second; default 0.
    #[serde(default)]
    pub speed_mps: f64,
    /// A unique name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A force.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<String>,
}

/// A bounding box in degrees. `min_lon_deg > max_lon_deg` crosses the antimeridian.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GeoBox {
    /// Southern edge.
    pub min_lat_deg: f64,
    /// Western edge.
    pub min_lon_deg: f64,
    /// Northern edge.
    pub max_lat_deg: f64,
    /// Eastern edge.
    pub max_lon_deg: f64,
}
