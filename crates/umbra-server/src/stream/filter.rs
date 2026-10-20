//! The subscription filter (REQ-X-6, ADR 0027): entity ids, a bounding box and event kinds.
//!
//! The bounding box test is the engine's [`GeoBox::contains`], the one implementation (ASM-173,
//! ASM-193); the filter only compiles the wire filter into engine types.

use std::collections::BTreeSet;

use umbra_api_types as api;
use umbra_engine::handle::GeoBox;

/// A wire filter, checked and ready to apply.
#[derive(Debug, Clone, Default)]
pub(crate) struct Filter {
    bbox: Option<GeoBox>,
    ids: Option<BTreeSet<u64>>,
    kinds: Option<BTreeSet<String>>,
}

impl Filter {
    /// The filter for `wire`, or the reason it is invalid.
    ///
    /// # Errors
    /// More than `max_ids` entity ids, or a bounding-box edge that is not a finite number.
    pub(crate) fn from_wire(
        wire: Option<&api::StreamFilter>,
        max_ids: usize,
    ) -> Result<Self, String> {
        let Some(wire) = wire else {
            return Ok(Self::default());
        };
        if let Some(ids) = &wire.entity_ids
            && ids.len() > max_ids
        {
            return Err(format!(
                "`filter.entity_ids` lists {} ids; at most {max_ids} are allowed",
                ids.len()
            ));
        }
        let bbox = wire
            .bbox
            .as_ref()
            .map(|b| {
                let edges = [b.min_lat_deg, b.min_lon_deg, b.max_lat_deg, b.max_lon_deg];
                if edges.iter().all(|edge| edge.is_finite()) {
                    Ok(GeoBox {
                        min_lat_deg: b.min_lat_deg,
                        min_lon_deg: b.min_lon_deg,
                        max_lat_deg: b.max_lat_deg,
                        max_lon_deg: b.max_lon_deg,
                    })
                } else {
                    Err("`filter.bbox` edges must be finite numbers".to_owned())
                }
            })
            .transpose()?;
        Ok(Self {
            bbox,
            ids: wire
                .entity_ids
                .as_ref()
                .map(|ids| ids.iter().copied().collect()),
            kinds: wire
                .event_kinds
                .as_ref()
                .map(|kinds| kinds.iter().cloned().collect()),
        })
    }

    /// Whether an entity passes the id filter.
    pub(crate) fn keeps_id(&self, id: u64) -> bool {
        self.ids.as_ref().is_none_or(|ids| ids.contains(&id))
    }

    /// Whether an entity at this position passes the bounding box.
    pub(crate) fn keeps_position(&self, lat_deg: f64, lon_deg: f64) -> bool {
        self.bbox.is_none_or(|bbox| bbox.contains(lat_deg, lon_deg))
    }

    /// Whether the box filter is set (events then need the positions of the frame's entities).
    pub(crate) fn has_bbox(&self) -> bool {
        self.bbox.is_some()
    }

    /// Whether an event of this kind passes the kind filter.
    pub(crate) fn keeps_kind(&self, kind: &str) -> bool {
        self.kinds.as_ref().is_none_or(|kinds| kinds.contains(kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(bbox: Option<(f64, f64, f64, f64)>, ids: Option<Vec<u64>>) -> api::StreamFilter {
        api::StreamFilter {
            bbox: bbox.map(|(a, b, c, d)| api::GeoBox {
                min_lat_deg: a,
                min_lon_deg: b,
                max_lat_deg: c,
                max_lon_deg: d,
            }),
            entity_ids: ids,
            event_kinds: None,
        }
    }

    #[test]
    fn no_filter_keeps_everything() {
        let filter = Filter::from_wire(None, 10).expect("filter");
        assert!(filter.keeps_id(7) && filter.keeps_position(0.0, 0.0) && filter.keeps_kind("fire"));
        assert!(!filter.has_bbox());
    }

    #[test]
    fn the_box_crosses_the_antimeridian_like_the_engines() {
        let filter = Filter::from_wire(Some(&wire(Some((-10.0, 170.0, 10.0, -170.0)), None)), 10)
            .expect("filter");
        assert!(filter.keeps_position(0.0, 175.0) && filter.keeps_position(0.0, -175.0));
        assert!(!filter.keeps_position(0.0, 0.0) && !filter.keeps_position(20.0, 175.0));
    }

    #[test]
    fn too_many_ids_and_non_finite_edges_are_refused() {
        let ids = wire(None, Some((1..=4).collect()));
        assert!(Filter::from_wire(Some(&ids), 3).is_err());
        assert!(Filter::from_wire(Some(&ids), 4).is_ok());
        let nan = wire(Some((f64::NAN, 0.0, 1.0, 1.0)), None);
        assert!(Filter::from_wire(Some(&nan), 3).is_err());
    }
}
