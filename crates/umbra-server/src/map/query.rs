//! The query string of `GET /entities` to an engine query (WI-13).

use umbra_api_types::ProblemCode;
use umbra_engine::EntityId;
use umbra_engine::handle::{EntityQuery, GeoBox, OwnershipFilter};

use crate::extract::ApiQuery;
use crate::problem::ApiError;

fn bad_request(detail: String) -> ApiError {
    ApiError::new(ProblemCode::BadRequest, detail)
}

/// The engine query for a parsed query string.
///
/// `ids` may be repeated or comma-separated and an id must be 1 or more; the four bounding-box
/// edges are given all or none and must be finite. Unknown parameters are ignored.
///
/// # Errors
/// `400 bad_request` for a value that does not parse, a repeated scalar, an incomplete box or an
/// unknown `ownership`; `422 validation_failed` for a `limit` out of range.
pub fn entity_query(query: &ApiQuery) -> Result<EntityQuery, ApiError> {
    let mut entities = EntityQuery::default();
    entities.limit = query.limit()?;
    entities.after_id = query.number::<u64>("after_id")?.and_then(EntityId::new);
    entities.ids = query
        .list("ids")
        .into_iter()
        .map(|text| {
            text.parse::<u64>()
                .ok()
                .and_then(EntityId::new)
                .ok_or_else(|| {
                    bad_request(format!(
                        "`ids` holds `{text}`, not an entity id (1 or more)"
                    ))
                })
        })
        .collect::<Result<_, _>>()?;
    entities.name = query.text("name")?.map(str::to_owned);
    entities.force = query.text("force")?.map(str::to_owned);
    entities.group = query.text("group")?.map(str::to_owned);
    entities.template = query.text("template")?.map(str::to_owned);
    entities.ownership = match query.text("ownership")? {
        None => None,
        Some("owned") => Some(OwnershipFilter::Owned),
        Some("remote") => Some(OwnershipFilter::Remote),
        Some(other) => {
            return Err(bad_request(format!(
                "`ownership={other}` is not `owned` or `remote`"
            )));
        }
    };
    entities.bbox = bbox(query)?;
    Ok(entities)
}

const BBOX_KEYS: [&str; 4] = ["min_lat_deg", "min_lon_deg", "max_lat_deg", "max_lon_deg"];

fn bbox(query: &ApiQuery) -> Result<Option<GeoBox>, ApiError> {
    let mut edges = [None; 4];
    for (edge, key) in edges.iter_mut().zip(BBOX_KEYS) {
        *edge = query.number::<f64>(key)?;
        if edge.is_some_and(|value| !value.is_finite()) {
            return Err(bad_request(format!("`{key}` must be a finite number")));
        }
    }
    match edges {
        [None, None, None, None] => Ok(None),
        [
            Some(min_lat_deg),
            Some(min_lon_deg),
            Some(max_lat_deg),
            Some(max_lon_deg),
        ] => Ok(Some(GeoBox {
            min_lat_deg,
            min_lon_deg,
            max_lat_deg,
            max_lon_deg,
        })),
        _ => Err(bad_request(format!(
            "a bounding box needs all of {}; got only {}",
            BBOX_KEYS.join(", "),
            BBOX_KEYS
                .iter()
                .zip(edges)
                .filter(|(_, edge)| edge.is_some())
                .map(|(key, _)| *key)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<EntityQuery, ApiError> {
        entity_query(&ApiQuery::parse(raw))
    }

    #[test]
    fn every_filter_reaches_the_engine_query() {
        let query = parse(
            "after_id=7&limit=5&ids=2,3&ids=9&name=n&force=f&group=g&template=t&ownership=remote\
             &min_lat_deg=1&min_lon_deg=2&max_lat_deg=3&max_lon_deg=4",
        )
        .expect("valid");
        assert_eq!(query.after_id, EntityId::new(7));
        assert_eq!(query.limit, 5);
        assert_eq!(query.ids, [2, 3, 9].map(|i| EntityId::new(i).expect("id")));
        assert_eq!(query.name.as_deref(), Some("n"));
        assert_eq!(query.force.as_deref(), Some("f"));
        assert_eq!(query.group.as_deref(), Some("g"));
        assert_eq!(query.template.as_deref(), Some("t"));
        assert_eq!(query.ownership, Some(OwnershipFilter::Remote));
        let bbox = query.bbox.expect("bbox");
        assert_eq!(
            (
                bbox.min_lat_deg,
                bbox.min_lon_deg,
                bbox.max_lat_deg,
                bbox.max_lon_deg
            ),
            (1.0, 2.0, 3.0, 4.0)
        );
    }

    #[test]
    fn no_parameters_is_the_default_query() {
        assert_eq!(parse("").expect("valid"), EntityQuery::default());
    }

    #[test]
    fn a_partial_box_names_what_was_given() {
        let error = parse("min_lat_deg=1&max_lon_deg=4").expect_err("partial");
        assert_eq!(error.code(), ProblemCode::BadRequest);
        let detail = error.into_problem().detail;
        assert!(detail.contains("only min_lat_deg, max_lon_deg"), "{detail}");
    }

    #[test]
    fn limit_out_of_range_stays_422() {
        let error = parse("limit=1001").expect_err("range");
        assert_eq!(error.code(), ProblemCode::ValidationFailed);
    }
}
