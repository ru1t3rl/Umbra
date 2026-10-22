//! Entities: engine state to wire summaries and details, and a spawn request to a spawn (WI-13).
//! The stream maps its frame entities with [`summary_of`] too (WI-15), so a listing and a
//! frame agree bit for bit (REQ-K-5).

use umbra_api_types as api;
use umbra_engine::handle::{EntityPage, EntityState};
use umbra_engine::{
    BehaviorStatus, EntityDisplay, EntityId, EntityInfo, EntitySnapshot, Force, Geodetic,
    MetersPerSecond, Ownership, Position, Radians, TemplateId, TemplateSpawn,
};

use super::MapError;

fn ownership(owner: &Ownership) -> api::Ownership {
    match owner {
        Ownership::Owned => api::Ownership::Owned,
        Ownership::Remote { .. } => api::Ownership::Remote,
    }
}

/// The wire spelling of a behavior status.
fn behavior_status(status: BehaviorStatus) -> &'static str {
    match status {
        BehaviorStatus::Success => "success",
        BehaviorStatus::Failure => "failure",
        BehaviorStatus::Running => "running",
    }
}

/// The wire summary of an entity.
#[must_use]
pub fn entity_summary(entity: &EntityState) -> api::EntitySummary {
    summary_of(&entity.info, &entity.snapshot)
}

/// The wire summary of an entity from its directory entry and its snapshot: the one mapping behind
/// the listing and the stream frames.
#[must_use]
pub fn summary_of(info: &EntityInfo, snapshot: &EntitySnapshot) -> api::EntitySummary {
    let shown = EntityDisplay::from_snapshot(snapshot);
    api::EntitySummary {
        id: shown.id.get(),
        name: info.name.clone(),
        template: Some(info.template.as_str().to_owned()),
        group: info.group.clone(),
        force: snapshot.force.as_ref().map(|f| f.as_str().to_owned()),
        ownership: ownership(&snapshot.ownership),
        remote_source: match &snapshot.ownership {
            Ownership::Remote { source } => Some(source.as_str().to_owned()),
            Ownership::Owned => None,
        },
        lat_deg: shown.lat_deg,
        lon_deg: shown.lon_deg,
        alt_m: shown.height_m,
        heading_deg: shown.heading_deg,
        speed_mps: shown.speed_mps,
        behavior: snapshot.behavior.as_ref().map(|b| b.as_str().to_owned()),
        status: snapshot.status.map(|s| behavior_status(s).to_owned()),
        route: shown.route_next.map(|(next, len)| api::RouteProgress {
            next: u32::try_from(next).unwrap_or(u32::MAX),
            len: u32::try_from(len).unwrap_or(u32::MAX),
        }),
    }
}

/// The wire detail of an entity.
#[must_use]
pub fn entity_detail(entity: &EntityState) -> api::EntityDetail {
    api::EntityDetail {
        summary: entity_summary(entity),
        motion_model: entity.snapshot.motion_model.as_str().to_owned(),
        contacts: entity
            .snapshot
            .contacts
            .iter()
            .map(|c| c.entity.get())
            .collect(),
    }
}

/// The wire page of a listing.
#[must_use]
pub fn entity_page(page: &EntityPage) -> api::EntityPage {
    api::EntityPage {
        tick: page.tick,
        total: u64::try_from(page.total).unwrap_or(u64::MAX),
        items: page.items.iter().map(entity_summary).collect(),
        next_after_id: page.next_after_id.map(EntityId::get),
    }
}

/// The engine spawn for a wire request.
///
/// # Errors
/// [`MapError::InvalidId`] for a `template` or `force` that is not a valid id;
/// [`MapError::InvalidSpawn`] for a position that is not a valid WGS-84 position. A heading or speed
/// the engine refuses is refused by the engine (`invalid_spawn`).
pub fn spawn_spec(request: &api::SpawnRequest) -> Result<TemplateSpawn, MapError> {
    let template = TemplateId::new(&request.template).map_err(|error| {
        MapError::InvalidId(format!(
            "`template={}` is not a valid template id: {error}",
            request.template
        ))
    })?;
    let force = request
        .force
        .as_deref()
        .map(|force| {
            Force::new(force).map_err(|error| {
                MapError::InvalidId(format!("`force={force}` is not a valid force: {error}"))
            })
        })
        .transpose()?;
    let position = Geodetic::from_degrees(request.lat_deg, request.lon_deg, request.alt_m)
        .map_err(|error| {
            MapError::InvalidSpawn(format!(
                "the position ({}, {}, {}) is not valid: {error}",
                request.lat_deg, request.lon_deg, request.alt_m
            ))
        })?;
    let mut spawn = TemplateSpawn::new(template, Position::Geodetic(position));
    spawn.heading = Radians::from_degrees(request.heading_deg);
    spawn.speed = MetersPerSecond(request.speed_mps);
    spawn.name.clone_from(&request.name);
    spawn.group.clone_from(&request.group);
    spawn.force = force;
    Ok(spawn)
}
