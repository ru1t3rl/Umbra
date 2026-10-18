//! Positions in scenario files: `{ lat_deg, lon_deg, alt_m? }` or `{ ecef_m = [x, y, z] }`
//! (ASM-044, ADR 0010). Converted once, at the edge.

use umbra_core::{Ecef, Geodetic, Position, Vec3};

use crate::diagnostic::DiagnosticCode;
use crate::reader::Reader;
use crate::value::{LTable, LValue};

const KEYS: [&str; 4] = ["lat_deg", "lon_deg", "alt_m", "ecef_m"];

/// Reads one position. Every problem is recorded; `None` when there was any.
pub(crate) fn read_position(reader: &mut Reader<'_>, value: &LValue) -> Option<Position> {
    let table = reader.table("position", value)?;
    reader.unknown_keys(table, &KEYS);
    let geodetic = ["lat_deg", "lon_deg", "alt_m"]
        .iter()
        .any(|k| table.get(k).is_some());
    match (geodetic, table.get("ecef_m")) {
        (true, Some(_)) | (false, None) => {
            let message = "a position needs exactly one form: `lat_deg` and `lon_deg` (with optional `alt_m`), or `ecef_m`".to_string();
            reader.error(DiagnosticCode::InvalidValue, message, table.origin());
            None
        }
        (false, Some(ecef)) => read_ecef(reader, ecef),
        (true, None) => read_geodetic(reader, table),
    }
}

fn read_ecef(reader: &mut Reader<'_>, value: &LValue) -> Option<Position> {
    let items = reader.array("ecef_m", value)?;
    if items.len() != 3 {
        let message = format!(
            "`ecef_m` needs exactly three numbers, found {}",
            items.len()
        );
        reader.error(DiagnosticCode::InvalidValue, message, value.origin());
        return None;
    }
    let numbers: Vec<Option<f64>> = items
        .iter()
        .map(|v| reader.number("ecef_m element", v))
        .collect();
    match numbers[..] {
        [Some(x), Some(y), Some(z)] => Some(Position::Ecef(Ecef(Vec3 { x, y, z }))),
        _ => None,
    }
}

fn read_geodetic(reader: &mut Reader<'_>, table: &LTable) -> Option<Position> {
    let mut field = |key: &str, default: Option<f64>| match table.get(key) {
        Some(v) => reader.number(key, v),
        None => default.or_else(|| {
            let message = format!("missing required key `{key}`");
            reader.error(DiagnosticCode::MissingKey, message, table.origin());
            None
        }),
    };
    let (lat, lon, alt) = (
        field("lat_deg", None),
        field("lon_deg", None),
        field("alt_m", Some(0.0)),
    );
    let (lat, lon, alt) = (lat?, lon?, alt?);
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        let message = format!(
            "latitude {lat} must be within [-90, 90] and longitude {lon} within [-180, 180] degrees"
        );
        reader.error(DiagnosticCode::InvalidValue, message, table.origin());
        return None;
    }
    match Geodetic::from_degrees(lat, lon, alt) {
        Ok(g) => Some(Position::Geodetic(g)),
        Err(error) => {
            reader.error(
                DiagnosticCode::InvalidValue,
                error.to_string(),
                table.origin(),
            );
            None
        }
    }
}
