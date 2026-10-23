//! JSON lines to records. Total on arbitrary input: every failure is a [`RecordError`] naming the
//! line, never a panic. Unknown keys are ignored (additive keys keep version 1, ADR 0053); an
//! unknown record type, command kind or message kind is an error.

use std::path::PathBuf;

use serde_json::{Map, Value};
use umbra_core::net::{KeySpace, NetEntityState, NetEntityType, NetKey, NetMessage};
use umbra_core::{
    ControlAction, Ecef, EntityId, Force, Geodetic, MetersPerSecond, Position, Radians, StateHash,
    TemplateId, Vec3, WeaponId, WorldEuler,
};

use crate::encode::DEAD_RECKONING;
use crate::header::{AdapterRef, FileDigest, PluginRef, ScenarioRef};
use crate::records::{CommandRecord, EndRecord, FaultRecord, InboundRecord, SpawnRecord};
use crate::{FORMAT, FORMAT_VERSION, Header, Record, RecordError, RecordedCommand};

/// A decoding failure before it knows its line number.
type Res<T> = Result<T, String>;

/// The members of one JSON object, read by name.
struct Fields<'a>(&'a Map<String, Value>);

fn object<'a>(v: &'a Value, what: &str) -> Res<Fields<'a>> {
    v.as_object()
        .map(Fields)
        .ok_or_else(|| format!("{what} must be an object"))
}

fn within<T>(key: &str, r: Res<T>) -> Res<T> {
    r.map_err(|e| format!("`{key}`: {e}"))
}

impl<'a> Fields<'a> {
    fn get(&self, key: &str) -> Res<&'a Value> {
        self.0.get(key).ok_or_else(|| format!("missing `{key}`"))
    }

    fn str(&self, key: &str) -> Res<&'a str> {
        within(key, str_of(self.get(key)?))
    }

    fn string(&self, key: &str) -> Res<String> {
        self.str(key).map(str::to_owned)
    }

    fn u64(&self, key: &str) -> Res<u64> {
        within(key, u64_of(self.get(key)?))
    }

    fn u32(&self, key: &str) -> Res<u32> {
        let n = self.u64(key)?;
        u32::try_from(n).map_err(|_| format!("`{key}`: {n} does not fit 32 bits"))
    }

    fn bool(&self, key: &str) -> Res<bool> {
        self.get(key)?
            .as_bool()
            .ok_or_else(|| format!("`{key}` must be true or false"))
    }

    fn f64(&self, key: &str) -> Res<f64> {
        within(key, f64_of(self.get(key)?))
    }

    fn decimal(&self, key: &str) -> Res<u64> {
        within(key, decimal_of(self.get(key)?))
    }

    fn hex(&self, key: &str) -> Res<u64> {
        within(key, hex_of(self.get(key)?))
    }

    fn vec3(&self, key: &str) -> Res<Vec3> {
        within(key, vec3_of(self.get(key)?))
    }

    fn array(&self, key: &str) -> Res<&'a Vec<Value>> {
        self.get(key)?
            .as_array()
            .ok_or_else(|| format!("`{key}` must be an array"))
    }

    /// The value, or `None` for `null` or an absent key.
    fn optional(&self, key: &str) -> Option<&'a Value> {
        self.0.get(key).filter(|v| !v.is_null())
    }

    fn opt_str(&self, key: &str) -> Res<Option<&'a str>> {
        self.optional(key)
            .map(|v| within(key, str_of(v)))
            .transpose()
    }

    fn opt_key(&self, key: &str) -> Res<Option<NetKey>> {
        self.optional(key)
            .map(|v| within(key, decimal_of(v)).map(NetKey))
            .transpose()
    }

    fn key(&self, key: &str) -> Res<NetKey> {
        self.decimal(key).map(NetKey)
    }
}

fn str_of(v: &Value) -> Res<&str> {
    v.as_str().ok_or_else(|| "must be a string".to_owned())
}

fn u64_of(v: &Value) -> Res<u64> {
    v.as_u64()
        .ok_or_else(|| "must be a non-negative integer".to_owned())
}

/// A finite number, or the strings `nan`, `inf`, `-inf` (ASM-726).
fn f64_of(v: &Value) -> Res<f64> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| "must be a number".to_owned()),
        Value::String(s) => match s.as_str() {
            "nan" => Ok(f64::NAN),
            "inf" => Ok(f64::INFINITY),
            "-inf" => Ok(f64::NEG_INFINITY),
            _ => Err("must be a number".to_owned()),
        },
        _ => Err("must be a number".to_owned()),
    }
}

/// ASCII digits only, no sign, no leading zero except `"0"` itself: one spelling per value.
fn decimal_of(v: &Value) -> Res<u64> {
    let text = str_of(v)?;
    let canonical =
        text == "0" || (!text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit()));
    canonical
        .then(|| text.parse().ok())
        .flatten()
        .ok_or_else(|| "must be a canonical decimal string of a 64-bit unsigned integer".to_owned())
}

fn hex_of(v: &Value) -> Res<u64> {
    let s = str_of(v)?;
    if s.len() == 16
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        u64::from_str_radix(s, 16).map_err(|e| e.to_string())
    } else {
        Err("must be 16 lowercase hex digits".to_owned())
    }
}

fn triple_of(v: &Value) -> Res<[f64; 3]> {
    match v.as_array().map(Vec::as_slice) {
        Some([x, y, z]) => Ok([f64_of(x)?, f64_of(y)?, f64_of(z)?]),
        _ => Err("must be an array of three numbers".to_owned()),
    }
}

fn vec3_of(v: &Value) -> Res<Vec3> {
    triple_of(v).map(|[x, y, z]| Vec3::new(x, y, z))
}

fn parse(line: &str) -> Res<Value> {
    serde_json::from_str(line).map_err(|e| format!("not valid JSON: {e}"))
}

fn at_line<T>(line: u64, r: Res<T>) -> Result<T, RecordError> {
    r.map_err(|message| RecordError::Syntax { line, message })
}

/// Parses the header line (line 1). A newer `version` is [`RecordError::Version`], checked before
/// any other member because a newer file may differ in them.
pub(crate) fn header(text: &str) -> Result<Header, RecordError> {
    let value = at_line(1, parse(text))?;
    let top = at_line(1, object(&value, "the header"))?;
    at_line(1, check_identity(&top))?;
    let version = at_line(1, top.u64("version"))?;
    if version > u64::from(FORMAT_VERSION) {
        return Err(RecordError::Version { found: version });
    }
    at_line(1, header_body(&top))
}

fn check_identity(top: &Fields<'_>) -> Res<()> {
    if top.str("type")? != "header" {
        return Err("the first line must be the header".to_owned());
    }
    match top.str("format")? {
        FORMAT => Ok(()),
        other => Err(format!("format {other:?} is not {FORMAT:?}")),
    }
}

fn header_body(top: &Fields<'_>) -> Res<Header> {
    if top.u64("version")? == 0 {
        return Err("`version` must be at least 1".to_owned());
    }
    match top.str("encoding")? {
        "json" => {}
        other => {
            return Err(format!(
                "encoding {other:?} is not supported (only \"json\")"
            ));
        }
    }
    Ok(Header {
        umbra: top.string("umbra")?,
        scenario: within("scenario", scenario(&object(top.get("scenario")?, "it")?))?,
        seed: top.decimal("seed")?,
        tick_rate_hz: top.u32("tick_rate_hz")?,
        tick_rate_source: top.string("tick_rate_source")?,
        state_hash_version: top.u32("state_hash_version")?,
        hash_every: top.u64("hash_every")?,
        on_plugin_fault: top.string("on_plugin_fault")?,
        plugins: list(top, "plugins", plugin)?,
        network_adapters: list(top, "network_adapters", adapter)?,
    })
}

/// Decodes every element of the array `key`, naming the index of a bad one.
fn list<T>(f: &Fields<'_>, key: &str, one: fn(&Fields<'_>) -> Res<T>) -> Res<Vec<T>> {
    f.array(key)?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            object(v, "it")
                .and_then(|o| one(&o))
                .map_err(|e| format!("`{key}`[{i}]: {e}"))
        })
        .collect()
}

fn scenario(f: &Fields<'_>) -> Res<ScenarioRef> {
    Ok(ScenarioRef {
        path: PathBuf::from(f.str("path")?),
        relative_path: f.opt_str("relative_path")?.map(PathBuf::from),
        files: list(f, "files", |o| {
            Ok(FileDigest {
                path: PathBuf::from(o.str("path")?),
                fnv1a64: o.hex("fnv1a64")?,
            })
        })?,
    })
}

fn plugin(f: &Fields<'_>) -> Res<PluginRef> {
    let abi = match f.array("abi")?.as_slice() {
        [major, minor] => (
            u32::try_from(u64_of(major)?).map_err(|_| "`abi`: does not fit 32 bits".to_owned())?,
            u32::try_from(u64_of(minor)?).map_err(|_| "`abi`: does not fit 32 bits".to_owned())?,
        ),
        _ => return Err("`abi` must be [major, minor]".to_owned()),
    };
    Ok(PluginRef {
        id: f.string("id")?,
        version: f.string("version")?,
        abi,
        library_fnv1a64: f.hex("library_fnv1a64")?,
    })
}

fn adapter(f: &Fields<'_>) -> Res<AdapterRef> {
    let space = object(f.get("key_space")?, "`key_space`")?;
    Ok(AdapterRef {
        name: f.string("name")?,
        adapter: f.string("adapter")?,
        key_space: KeySpace {
            base: space.decimal("base")?,
            count: space.decimal("count")?,
        },
    })
}

/// Parses one record line; `line` is its 1-based number for the error.
pub(crate) fn record(text: &str, line: u64) -> Result<Record, RecordError> {
    at_line(line, record_body(text))
}

fn record_body(text: &str) -> Res<Record> {
    let value = parse(text)?;
    let top = object(&value, "a record")?;
    match top.str("type")? {
        "command" => Ok(Record::Command(CommandRecord {
            before_tick: top.u64("before_tick")?,
            seq: top.u64("seq")?,
            command: within("command", command(&object(top.get("command")?, "it")?))?,
        })),
        "inbound" => Ok(Record::Inbound(InboundRecord {
            tick: top.u64("tick")?,
            adapter: top.string("adapter")?,
            messages: messages(&top)?,
        })),
        "fault" => Ok(Record::Fault(FaultRecord {
            tick: top.u64("tick")?,
            adapter: top.string("adapter")?,
            call: top.string("call")?,
        })),
        "hash" => Ok(Record::Hash {
            tick: top.u64("tick")?,
            hash: StateHash(top.hex("hash")?),
        }),
        "end" => Ok(Record::End(EndRecord {
            tick: top.u64("tick")?,
            reason: top.string("reason")?,
            hash: StateHash(top.hex("hash")?),
        })),
        "header" => Err("a header may only be line 1".to_owned()),
        other => Err(format!("unknown record type {other:?}")),
    }
}

fn command(f: &Fields<'_>) -> Res<RecordedCommand> {
    Ok(match f.str("kind")? {
        "spawn" => spawn(f)?,
        "remove" => RecordedCommand::Remove {
            entity: entity_id(f, "entity")?,
        },
        "set_adapter_enabled" => RecordedCommand::SetAdapterEnabled {
            name: f.string("name")?,
            enabled: f.bool("enabled")?,
        },
        "start" => RecordedCommand::Start,
        "pause" => RecordedCommand::Pause,
        "resume" => RecordedCommand::Resume,
        "step" => RecordedCommand::Step(f.u64("ticks")?),
        "set_clock" => RecordedCommand::SetClock(f.string("mode")?),
        "stop" => RecordedCommand::Stop,
        other => return Err(format!("unknown command kind {other:?}")),
    })
}

fn entity_id(f: &Fields<'_>, key: &str) -> Res<EntityId> {
    EntityId::new(f.u64(key)?).ok_or_else(|| format!("`{key}` must not be 0"))
}

fn spawn(f: &Fields<'_>) -> Res<RecordedCommand> {
    let force = f
        .opt_str("force")?
        .map(|s| Force::new(s).map_err(|e| format!("`force`: {e}")))
        .transpose()?;
    Ok(RecordedCommand::Spawn {
        spec: SpawnRecord {
            template: TemplateId::new(f.str("template")?)
                .map_err(|e| format!("`template`: {e}"))?,
            position: within("position", position(f.get("position")?))?,
            heading: Radians(f.f64("heading")?),
            speed: MetersPerSecond(f.f64("speed")?),
            name: f.opt_str("name")?.map(str::to_owned),
            group: f.opt_str("group")?.map(str::to_owned),
            force,
        },
        result: entity_id(f, "result")?,
    })
}

fn position(value: &Value) -> Res<Position> {
    let f = object(value, "it")?;
    match (f.optional("ecef"), f.optional("geodetic")) {
        (Some(e), None) => within("ecef", vec3_of(e)).map(|v| Position::Ecef(Ecef(v))),
        (None, Some(g)) => {
            let [lat, lon, height] = within("geodetic", triple_of(g))?;
            Geodetic::new(Radians(lat), Radians(lon), umbra_core::Meters(height))
                .map(Position::Geodetic)
                .map_err(|e| format!("`geodetic`: {e}"))
        }
        _ => Err("must have exactly one of ecef or geodetic".to_owned()),
    }
}

fn messages(top: &Fields<'_>) -> Res<Vec<NetMessage>> {
    let items = top.array("messages")?;
    items
        .iter()
        .enumerate()
        .map(|(i, v)| {
            object(v, "it")
                .and_then(|o| message(&o))
                .map_err(|e| format!("`messages`[{i}]: {e}"))
        })
        .collect()
}

fn message(f: &Fields<'_>) -> Res<NetMessage> {
    Ok(match f.str("kind")? {
        "entity_state" => NetMessage::EntityState(entity_state(f)?),
        "entity_removed" => NetMessage::EntityRemoved { key: f.key("key")? },
        "fire" => NetMessage::Fire {
            shooter: f.key("shooter")?,
            target: f.opt_key("target")?,
            weapon: WeaponId::new(f.str("weapon")?).map_err(|e| format!("`weapon`: {e}"))?,
            position: Ecef(f.vec3("position")?),
        },
        "detonation" => NetMessage::Detonation {
            shooter: f.opt_key("shooter")?,
            target: f.opt_key("target")?,
            position: Ecef(f.vec3("position")?),
        },
        "simulation_control" => NetMessage::SimulationControl(control(f)?),
        other => return Err(format!("unknown message kind {other:?}")),
    })
}

fn control(f: &Fields<'_>) -> Res<ControlAction> {
    Ok(match f.str("action")? {
        "start" => ControlAction::Start,
        "pause" => ControlAction::Pause,
        "resume" => ControlAction::Resume,
        "stop" => ControlAction::Stop,
        "step" => ControlAction::Step(f.u64("ticks")?),
        other => return Err(format!("unknown control action {other:?}")),
    })
}

fn entity_state(f: &Fields<'_>) -> Res<NetEntityState> {
    let [yaw, pitch, roll] = within("orientation", triple_of(f.get("orientation")?))?;
    let model = f.str("dead_reckoning")?;
    let dead_reckoning = DEAD_RECKONING
        .iter()
        .find(|(name, _)| *name == model)
        .map(|(_, m)| *m)
        .ok_or_else(|| format!("unknown dead reckoning model {model:?}"))?;
    let force = f
        .opt_str("force")?
        .map(|s| Force::new(s).map_err(|e| format!("`force`: {e}")))
        .transpose()?;
    Ok(NetEntityState {
        key: f.key("key")?,
        entity_type: f.opt_str("entity_type")?.map(NetEntityType::new),
        force,
        name: f.opt_str("name")?.map(std::sync::Arc::from),
        position: Ecef(f.vec3("position")?),
        velocity: f.vec3("velocity")?,
        acceleration: f.vec3("acceleration")?,
        angular_velocity: f.vec3("angular_velocity")?,
        orientation: WorldEuler {
            yaw: Radians(yaw),
            pitch: Radians(pitch),
            roll: Radians(roll),
        },
        dead_reckoning,
    })
}
