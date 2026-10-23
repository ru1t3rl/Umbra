//! Records to JSON lines: keys in a fixed order, shortest round-trip floats, no wall-clock data
//! (ASM-621). Hand-mapped because core types carry no serde derives (D-23, ADR 0053).

use std::path::Path;

use serde_json::Value;
use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{KeySpace, NetEntityState, NetKey, NetMessage};
use umbra_core::{ControlAction, Ecef, Position, StateHash, Vec3};

use crate::header::{AdapterRef, FileDigest, PluginRef, ScenarioRef};
use crate::records::{CommandRecord, EndRecord, FaultRecord, InboundRecord, SpawnRecord};
use crate::{FORMAT, FORMAT_VERSION, Header, Record, RecordError, RecordedCommand};

/// An object from already-encoded values; the order of `fields` is the order in the file.
fn object(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{}:{value}", text(key)))
        .collect();
    format!("{{{}}}", body.join(","))
}

fn array(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

/// A JSON string with the standard escapes.
fn text(s: &str) -> String {
    Value::String(s.to_owned()).to_string()
}

fn opt_text(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_owned(), text)
}

/// A finite float as its shortest round-trip decimal; a non-finite one as a string, because JSON
/// has no spelling for it (ASM-726).
fn float(x: f64) -> String {
    match serde_json::Number::from_f64(x) {
        Some(n) => n.to_string(),
        None if x.is_nan() => text("nan"),
        None if x > 0.0 => text("inf"),
        None => text("-inf"),
    }
}

/// A `u64` that may exceed 2^53 as a decimal string, safe for tools that read numbers as `f64`.
fn decimal(x: u64) -> String {
    text(&x.to_string())
}

/// A digest or hash as 16 lowercase hex digits in a string.
fn hex(x: u64) -> String {
    text(&format!("{x:016x}"))
}

fn path(p: &Path) -> String {
    text(&p.to_string_lossy())
}

fn vec3(v: Vec3) -> String {
    array([float(v.x), float(v.y), float(v.z)])
}

/// The header line, without the newline.
pub(crate) fn header(h: &Header) -> String {
    object(&[
        ("type", text("header")),
        ("format", text(FORMAT)),
        ("version", FORMAT_VERSION.to_string()),
        ("encoding", text("json")),
        ("umbra", text(&h.umbra)),
        ("scenario", scenario(&h.scenario)),
        ("seed", decimal(h.seed)),
        ("tick_rate_hz", h.tick_rate_hz.to_string()),
        ("tick_rate_source", text(&h.tick_rate_source)),
        ("state_hash_version", h.state_hash_version.to_string()),
        ("hash_every", h.hash_every.to_string()),
        ("on_plugin_fault", text(&h.on_plugin_fault)),
        ("plugins", array(h.plugins.iter().map(plugin))),
        (
            "network_adapters",
            array(h.network_adapters.iter().map(adapter)),
        ),
    ])
}

fn scenario(s: &ScenarioRef) -> String {
    object(&[
        ("path", path(&s.path)),
        (
            "relative_path",
            s.relative_path
                .as_deref()
                .map_or_else(|| "null".to_owned(), path),
        ),
        ("files", array(s.files.iter().map(file_digest))),
    ])
}

fn file_digest(f: &FileDigest) -> String {
    object(&[("path", path(&f.path)), ("fnv1a64", hex(f.fnv1a64))])
}

fn plugin(p: &PluginRef) -> String {
    object(&[
        ("id", text(&p.id)),
        ("version", text(&p.version)),
        ("abi", array([p.abi.0.to_string(), p.abi.1.to_string()])),
        ("library_fnv1a64", hex(p.library_fnv1a64)),
    ])
}

fn adapter(a: &AdapterRef) -> String {
    object(&[
        ("name", text(&a.name)),
        ("adapter", text(&a.adapter)),
        ("key_space", key_space(a.key_space)),
    ])
}

fn key_space(k: KeySpace) -> String {
    object(&[("base", decimal(k.base)), ("count", decimal(k.count))])
}

/// One record line, without the newline.
///
/// # Errors
/// [`RecordError::Unsupported`] for a value this format version cannot spell.
pub(crate) fn record(r: &Record) -> Result<String, RecordError> {
    Ok(match r {
        Record::Command(c) => command_record(c)?,
        Record::Inbound(i) => inbound_record(i)?,
        Record::Fault(f) => fault_record(f),
        Record::Hash { tick, hash } => object(&[
            ("type", text("hash")),
            ("tick", tick.to_string()),
            ("hash", state_hash(*hash)),
        ]),
        Record::End(e) => end_record(e),
    })
}

fn state_hash(h: StateHash) -> String {
    hex(h.0)
}

fn fault_record(f: &FaultRecord) -> String {
    object(&[
        ("type", text("fault")),
        ("tick", f.tick.to_string()),
        ("adapter", text(&f.adapter)),
        ("call", text(&f.call)),
    ])
}

fn end_record(e: &EndRecord) -> String {
    object(&[
        ("type", text("end")),
        ("tick", e.tick.to_string()),
        ("reason", text(&e.reason)),
        ("hash", state_hash(e.hash)),
    ])
}

fn inbound_record(i: &InboundRecord) -> Result<String, RecordError> {
    let messages = i
        .messages
        .iter()
        .map(message)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(object(&[
        ("type", text("inbound")),
        ("tick", i.tick.to_string()),
        ("adapter", text(&i.adapter)),
        ("messages", array(messages)),
    ]))
}

fn command_record(c: &CommandRecord) -> Result<String, RecordError> {
    Ok(object(&[
        ("type", text("command")),
        ("before_tick", c.before_tick.to_string()),
        ("seq", c.seq.to_string()),
        ("command", command(&c.command)?),
    ]))
}

fn command(c: &RecordedCommand) -> Result<String, RecordError> {
    let kind = |name: &str| ("kind", text(name));
    Ok(match c {
        RecordedCommand::Spawn { spec, result } => spawn(spec, result.get())?,
        RecordedCommand::Remove { entity } => {
            object(&[kind("remove"), ("entity", entity.get().to_string())])
        }
        RecordedCommand::SetAdapterEnabled { name, enabled } => object(&[
            kind("set_adapter_enabled"),
            ("name", text(name)),
            ("enabled", enabled.to_string()),
        ]),
        RecordedCommand::Start => object(&[kind("start")]),
        RecordedCommand::Pause => object(&[kind("pause")]),
        RecordedCommand::Resume => object(&[kind("resume")]),
        RecordedCommand::Step(n) => object(&[kind("step"), ("ticks", n.to_string())]),
        RecordedCommand::SetClock(mode) => object(&[kind("set_clock"), ("mode", text(mode))]),
        RecordedCommand::Stop => object(&[kind("stop")]),
    })
}

fn spawn(s: &SpawnRecord, result: u64) -> Result<String, RecordError> {
    Ok(object(&[
        ("kind", text("spawn")),
        ("template", text(s.template.as_ref())),
        ("position", position(s.position)?),
        ("heading", float(s.heading.0)),
        ("speed", float(s.speed.0)),
        ("name", opt_text(s.name.as_deref())),
        ("group", opt_text(s.group.as_deref())),
        (
            "force",
            opt_text(s.force.as_ref().map(umbra_core::Force::as_str)),
        ),
        ("result", result.to_string()),
    ]))
}

fn position(p: Position) -> Result<String, RecordError> {
    match p {
        Position::Ecef(e) => Ok(object(&[("ecef", vec3(e.0))])),
        Position::Geodetic(g) => Ok(object(&[(
            "geodetic",
            array([float(g.lat().0), float(g.lon().0), float(g.height().0)]),
        )])),
        _ => Err(unsupported("position frame")),
    }
}

fn unsupported(what: &str) -> RecordError {
    RecordError::Unsupported(format!("{what} is not part of recording format v1"))
}

fn key(k: NetKey) -> String {
    decimal(k.0)
}

fn opt_key(k: Option<NetKey>) -> String {
    k.map_or_else(|| "null".to_owned(), key)
}

fn ecef(e: Ecef) -> String {
    vec3(e.0)
}

fn message(m: &NetMessage) -> Result<String, RecordError> {
    let kind = |name: &str| ("kind", text(name));
    Ok(match m {
        NetMessage::EntityState(s) => entity_state(s)?,
        NetMessage::EntityRemoved { key: k } => object(&[kind("entity_removed"), ("key", key(*k))]),
        NetMessage::Fire {
            shooter,
            target,
            weapon,
            position,
        } => object(&[
            kind("fire"),
            ("shooter", key(*shooter)),
            ("target", opt_key(*target)),
            ("weapon", text(weapon.as_ref())),
            ("position", ecef(*position)),
        ]),
        NetMessage::Detonation {
            shooter,
            target,
            position,
        } => object(&[
            kind("detonation"),
            ("shooter", opt_key(*shooter)),
            ("target", opt_key(*target)),
            ("position", ecef(*position)),
        ]),
        NetMessage::SimulationControl(action) => control(*action)?,
        _ => return Err(unsupported("network message kind")),
    })
}

fn control(action: ControlAction) -> Result<String, RecordError> {
    let kind = ("kind", text("simulation_control"));
    let named = |name: &str| object(&[kind.clone(), ("action", text(name))]);
    Ok(match action {
        ControlAction::Start => named("start"),
        ControlAction::Pause => named("pause"),
        ControlAction::Resume => named("resume"),
        ControlAction::Stop => named("stop"),
        ControlAction::Step(n) => object(&[
            kind.clone(),
            ("action", text("step")),
            ("ticks", n.to_string()),
        ]),
        _ => return Err(unsupported("control action")),
    })
}

fn entity_state(s: &NetEntityState) -> Result<String, RecordError> {
    let orientation = array([
        float(s.orientation.yaw.0),
        float(s.orientation.pitch.0),
        float(s.orientation.roll.0),
    ]);
    Ok(object(&[
        ("kind", text("entity_state")),
        ("key", key(s.key)),
        (
            "entity_type",
            opt_text(
                s.entity_type
                    .as_ref()
                    .map(umbra_core::net::NetEntityType::as_str),
            ),
        ),
        (
            "force",
            opt_text(s.force.as_ref().map(umbra_core::Force::as_str)),
        ),
        ("name", opt_text(s.name.as_deref())),
        ("position", ecef(s.position)),
        ("velocity", vec3(s.velocity)),
        ("acceleration", vec3(s.acceleration)),
        ("angular_velocity", vec3(s.angular_velocity)),
        ("orientation", orientation),
        ("dead_reckoning", text(dead_reckoning(s.dead_reckoning)?)),
    ]))
}

/// The stable name of a dead reckoning model; `decode` is its inverse.
pub(crate) const DEAD_RECKONING: [(&str, DeadReckoning); 9] = [
    ("static", DeadReckoning::Static),
    ("velocity", DeadReckoning::Velocity),
    ("velocity_rotation", DeadReckoning::VelocityRotation),
    ("acceleration_rotation", DeadReckoning::AccelerationRotation),
    ("acceleration", DeadReckoning::Acceleration),
    ("body_velocity", DeadReckoning::BodyVelocity),
    (
        "body_velocity_rotation",
        DeadReckoning::BodyVelocityRotation,
    ),
    (
        "body_acceleration_rotation",
        DeadReckoning::BodyAccelerationRotation,
    ),
    ("body_acceleration", DeadReckoning::BodyAcceleration),
];

fn dead_reckoning(model: DeadReckoning) -> Result<&'static str, RecordError> {
    DEAD_RECKONING
        .iter()
        .find(|(_, m)| *m == model)
        .map(|(name, _)| *name)
        .ok_or_else(|| unsupported("dead reckoning model"))
}
