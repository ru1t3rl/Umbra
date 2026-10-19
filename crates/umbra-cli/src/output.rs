//! Text and JSON Lines output of `run` (REQ-X-2, ASM-062) and the `validate` report.

use std::collections::BTreeMap;
use std::io::{self, Write};

use serde_json::json;
use umbra_engine::scenario::{Diagnostic, EntityInfo, Location};
use umbra_engine::{EntityDisplay, EntityId, PluginInfo, Snapshot, StateHash};

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
    if rounded >= 360.0 || rounded == 0.0 {
        0.0
    } else {
        rounded
    }
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

/// Entity names by id, for the state lines.
pub fn name_map(infos: &[EntityInfo]) -> BTreeMap<EntityId, String> {
    infos
        .iter()
        .filter_map(|i| i.name.clone().map(|n| (i.id, n)))
        .collect()
}

/// A name for a `key=value` text line: bare when it is plain, quoted (Rust escapes) otherwise.
fn text_name(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| !c.is_whitespace() && !c.is_control() && c != '=' && c != '"' && c != '\\');
    if plain {
        name.to_string()
    } else {
        format!("{name:?}")
    }
}

/// Writes one line per entity of `snapshot`; `names` supplies the entity names.
pub fn write_entities(
    out: &mut impl Write,
    format: Format,
    snapshot: &Snapshot,
    names: &BTreeMap<EntityId, String>,
) -> io::Result<()> {
    let tick = snapshot.time.tick();
    let t = snapshot.time.as_seconds().0;
    for entity in &snapshot.entities {
        let mut d = EntityDisplay::from_snapshot(entity);
        d.heading_deg = heading_for_display(d.heading_deg);
        d.height_m = height_for_display(d.height_m);
        let name = names.get(&d.id).map(String::as_str);
        match format {
            Format::Text => writeln!(
                out,
                "tick={tick} t={t:.3}s id={}{} lat={:.7} lon={:.7} h={:.3} heading_deg={:.2} speed_mps={:.2} waypoint={}",
                d.id,
                name.map_or_else(String::new, |n| format!(" name={}", text_name(n))),
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
                    "name": name,
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

/// Writes the state hash after `tick` ticks: `tick=N hash=<hex>` or `{"tick":N,"state_hash":".."}`.
pub fn write_tick_hash(
    out: &mut impl Write,
    format: Format,
    tick: u64,
    hash: StateHash,
) -> io::Result<()> {
    match format {
        Format::Text => writeln!(out, "tick={tick} hash={hash}"),
        Format::Json => writeln!(
            out,
            "{}",
            json!({"tick": tick, "state_hash": hash.to_string()})
        ),
    }
}

/// Writes the final line: the state hash after `tick` ticks (JSON: marked `"final": true`).
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
            json!({"tick": tick, "state_hash": hash.to_string(), "final": true})
        ),
    }
}

fn location_json(location: Option<&Location>) -> serde_json::Value {
    location.map_or_else(
        || json!({"path": null, "line": null, "column": null}),
        |l| json!({"path": l.path.display().to_string(), "line": l.line, "column": l.column}),
    )
}

fn diagnostic_json(d: &Diagnostic) -> serde_json::Value {
    let notes: Vec<serde_json::Value> = d
        .notes
        .iter()
        .map(|n| {
            let mut v = location_json(n.location.as_ref());
            v["message"] = json!(n.message);
            v
        })
        .collect();
    let mut v = location_json(d.location.as_ref());
    v["code"] = json!(d.code.as_str());
    v["message"] = json!(d.message);
    v["notes"] = json!(notes);
    v
}

/// The `validate --json` document: validity, checked files and every diagnostic.
pub fn validation_json(files: &[std::path::PathBuf], diagnostics: &[Diagnostic]) -> String {
    json!({
        "valid": diagnostics.is_empty(),
        "files": files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
    })
    .to_string()
}

/// The `validate` text summary: `ok: N files checked` or `N error(s) in M file(s)`.
pub fn validation_summary(files_checked: usize, diagnostics: &[Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return format!("ok: {files_checked} files checked");
    }
    let mut paths: Vec<&std::path::Path> = diagnostics
        .iter()
        .filter_map(|d| d.location.as_ref().map(|l| l.path.as_path()))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let unlocated = diagnostics.iter().filter(|d| d.location.is_none()).count();
    format!(
        "{} error(s) in {} file(s)",
        diagnostics.len(),
        paths.len() + unlocated
    )
}

/// The `list-plugins --json` document (ASM-144):
/// `{"plugins":[{"id","name","version","abi":"<major>.<minor>","path","capabilities":[{"kind","id","version"}]}]}`
/// in load order; `kind` is the component noun (`behavior`).
pub fn plugins_json(plugins: &[PluginInfo]) -> String {
    let plugins: Vec<_> = plugins
        .iter()
        .map(|p| {
            json!({
                "id": p.id.as_str(),
                "name": p.name,
                "version": p.version.to_string(),
                "abi": format!("{}.{}", p.abi.0, p.abi.1),
                "path": p.origin,
                "capabilities": p.capabilities.iter().map(|c| json!({
                    "kind": c.kind.to_string(),
                    "id": c.id.as_str(),
                    "version": c.version.to_string(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({ "plugins": plugins }).to_string()
}

/// The `list-plugins` text: one line per plugin, then one indented line per capability.
pub fn plugins_text(plugins: &[PluginInfo]) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    for p in plugins {
        // Writing to a String cannot fail.
        let _ = writeln!(
            text,
            "{} {} abi={}.{} path={}",
            p.id, p.version, p.abi.0, p.abi.1, p.origin
        );
        for c in &p.capabilities {
            let _ = writeln!(text, "  {} {} {}", c.kind, c.id, c.version);
        }
    }
    let _ = write!(text, "{} plugin(s)", plugins.len());
    text
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
        assert_eq!(format!("{:.2}", heading_for_display(-5.7e-11)), "0.00");
        assert_eq!(format!("{:.2}", heading_for_display(-1e-12)), "0.00");
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

#[cfg(test)]
mod name_tests {
    use super::text_name;

    #[test]
    fn plain_names_are_bare_and_odd_ones_are_quoted() {
        assert_eq!(text_name("base-1"), "base-1");
        assert_eq!(text_name("my tank"), "\"my tank\"");
        assert_eq!(text_name("a=b"), "\"a=b\"");
        assert_eq!(text_name("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(text_name(""), "\"\"");
    }
}
