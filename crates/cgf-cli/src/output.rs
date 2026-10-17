//! Text and JSON Lines output of `demo` (ASM-027). The format is unstable until M2.

use std::io::{self, Write};

use cgf_engine::{EntityDisplay, Snapshot, StateHash};
use serde_json::json;

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `key=value` text lines.
    Text,
    /// One JSON object per line.
    Json,
}

/// Heading rounded to the printed 0.01 degree and kept in `[0, 360)`: 359.999 prints as `0.00`.
fn heading_for_display(deg: f64) -> f64 {
    let rounded = (deg * 100.0).round() / 100.0;
    if rounded >= 360.0 { 0.0 } else { rounded }
}

/// Height rounded to the printed millimeter, with `-0.0` and tiny negatives shown as `0.0`.
fn height_for_display(m: f64) -> f64 {
    let rounded = (m * 1000.0).round() / 1000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn waypoint_text(d: &EntityDisplay) -> String {
    d.route_next
        .map_or_else(|| "-".to_string(), |(next, len)| format!("{next}/{len}"))
}

/// Writes one line per entity of `snapshot`.
pub fn write_entities(out: &mut impl Write, format: Format, snapshot: &Snapshot) -> io::Result<()> {
    let tick = snapshot.time.tick();
    let t = snapshot.time.as_seconds().0;
    for entity in &snapshot.entities {
        let mut d = EntityDisplay::from_snapshot(entity);
        d.heading_deg = heading_for_display(d.heading_deg);
        d.height_m = height_for_display(d.height_m);
        match format {
            Format::Text => writeln!(
                out,
                "tick={tick} t={t:.3}s id={} lat={:.7} lon={:.7} h={:.3} heading_deg={:.2} speed_mps={:.2} waypoint={}",
                d.id,
                d.lat_deg,
                d.lon_deg,
                d.height_m,
                d.heading_deg,
                d.speed_mps,
                waypoint_text(&d)
            )?,
            Format::Json => writeln!(
                out,
                "{}",
                json!({
                    "tick": tick,
                    "t": t,
                    "id": d.id.get(),
                    "lat_deg": d.lat_deg,
                    "lon_deg": d.lon_deg,
                    "height_m": d.height_m,
                    "heading_deg": d.heading_deg,
                    "speed_mps": d.speed_mps,
                    "waypoint": d.route_next.map(|(next, len)| json!({"next": next, "len": len})),
                })
            )?,
        }
    }
    Ok(())
}

/// Writes the final line: the state hash after `tick` ticks.
pub fn write_final(
    out: &mut impl Write,
    format: Format,
    tick: u64,
    hash: StateHash,
) -> io::Result<()> {
    match format {
        Format::Text => writeln!(out, "state_hash={hash}"),
        Format::Json => writeln!(
            out,
            "{}",
            json!({"tick": tick, "state_hash": hash.to_string()})
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{heading_for_display, height_for_display};

    /// A heading just below north prints as 0.00, never 360.00.
    #[test]
    fn heading_is_normalised_into_zero_to_360() {
        assert_eq!(heading_for_display(359.999_996), 0.0);
        assert_eq!(heading_for_display(360.0), 0.0);
        assert_eq!(heading_for_display(0.004), 0.0);
        assert_eq!(heading_for_display(90.004), 90.0);
        assert_eq!(heading_for_display(359.99), 359.99);
        assert_eq!(format!("{:.2}", heading_for_display(359.996)), "0.00");
    }

    /// A height of -1e-10 prints as 0.000, never -0.000.
    #[test]
    fn tiny_negative_height_prints_as_zero() {
        assert_eq!(format!("{:.3}", height_for_display(-1e-10)), "0.000");
        assert_eq!(format!("{:.3}", height_for_display(-0.0)), "0.000");
        assert_eq!(format!("{:.3}", height_for_display(-0.0004)), "0.000");
        assert_eq!(format!("{:.3}", height_for_display(-0.002)), "-0.002");
        assert_eq!(format!("{:.3}", height_for_display(120.0)), "120.000");
    }
}
