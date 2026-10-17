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

fn waypoint_text(d: &EntityDisplay) -> String {
    d.route_next
        .map_or_else(|| "-".to_string(), |(next, len)| format!("{next}/{len}"))
}

/// Writes one line per entity of `snapshot`.
pub fn write_entities(out: &mut impl Write, format: Format, snapshot: &Snapshot) -> io::Result<()> {
    let tick = snapshot.time.tick();
    let t = snapshot.time.as_seconds().0;
    for entity in &snapshot.entities {
        let d = EntityDisplay::from_snapshot(entity);
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
