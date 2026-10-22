//! The parameters of the DIS adapter (`[[network_adapters]] params`, `docs/net/dis.md`, ADR 0034).
//!
//! [`Config::parse`] checks everything the JSON schema ([`PARAM_SCHEMA`]) cannot (address syntax,
//! which keys belong to which mode, id grammars, token syntax, the `[network.<name>]` tables of the
//! templates) and names the offending key, value or template. No I/O happens here: a dry run builds
//! a [`Config`] and never starts it.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use umbra_sdk::serde_json::Value;
use umbra_sdk::{ConfigureInput, Error, MANAGER_TEMPLATE_KEYS};

use crate::codec::EntityType;
use crate::translate::parse_entity_type;
use crate::version::DisVersion;

/// JSON Schema of the params (ADR 0009 subset). Mode-dependent rules are in [`Config::parse`].
pub const PARAM_SCHEMA: &str = r#"{
  "type": "object",
  "description": "DIS (IEEE 1278.1) network adapter over UDP: unicast, broadcast or multicast, protocol versions 6 and 7",
  "properties": {
    "version": { "type": "integer", "enum": [6, 7], "description": "protocol version sent; both are accepted (default 7)" },
    "exercise": { "type": "integer", "minimum": 0, "maximum": 255, "description": "exercise ID sent and required (default 1)" },
    "site": { "type": "integer", "minimum": 1, "maximum": 65534, "description": "site ID of this adapter's key space" },
    "application": { "type": "integer", "minimum": 1, "maximum": 65534, "description": "application ID of this adapter's key space" },
    "mode": { "type": "string", "enum": ["unicast", "broadcast", "multicast"], "description": "default unicast" },
    "bind": { "type": "string", "description": "ip:port to receive on (default 127.0.0.1:3000; 0.0.0.0:3000 for broadcast and multicast)" },
    "destinations": { "type": "array", "items": { "type": "string" }, "description": "unicast targets, ip:port" },
    "broadcast_address": { "type": "string", "description": "broadcast mode: ip:port to send to" },
    "group": { "type": "string", "description": "multicast mode: IPv4 group address" },
    "interface": { "type": "string", "description": "multicast mode: IPv4 address of the interface (default 0.0.0.0)" },
    "ttl": { "type": "integer", "minimum": 0, "maximum": 255, "description": "multicast mode: time to live (default 1)" },
    "forces": { "type": "object", "additionalProperties": { "type": "integer", "minimum": 1, "maximum": 255 }, "description": "force name to DIS force ID (default friendly 1, opposing 2, neutral 3)" },
    "weapons": { "type": "object", "additionalProperties": { "type": "string" }, "description": "weapon ID to munition entity type token" },
    "queue": {
      "type": "object",
      "properties": {
        "send": { "type": "integer", "minimum": 1, "maximum": 1000000 },
        "receive": { "type": "integer", "minimum": 1, "maximum": 1000000 }
      },
      "additionalProperties": false,
      "description": "bounded queue sizes in datagrams (default 4096 each)"
    }
  },
  "required": ["site", "application"],
  "additionalProperties": false
}"#;

/// Default receive address of the unicast mode.
pub const DEFAULT_BIND: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3000);
/// Default receive address of the broadcast and multicast modes (they cannot receive on loopback only).
pub const DEFAULT_WIDE_BIND: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 3000);
/// Default size of each bounded queue, in datagrams.
pub const DEFAULT_QUEUE: usize = 4096;
/// Largest queue size accepted.
pub const MAX_QUEUE: usize = 1_000_000;
/// Default multicast time to live.
pub const DEFAULT_TTL: u32 = 1;

/// How datagrams are sent (the receive socket is the same in every mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// To each of the destinations (none: receive only).
    Unicast {
        /// Target addresses.
        destinations: Vec<SocketAddrV4>,
    },
    /// To one broadcast address.
    Broadcast {
        /// Target address.
        address: SocketAddrV4,
    },
    /// To a group, on the port of `bind`.
    Multicast {
        /// The IPv4 group.
        group: Ipv4Addr,
        /// The interface address that sends and joins (0.0.0.0: the system chooses).
        interface: Ipv4Addr,
        /// Time to live.
        ttl: u32,
    },
}

/// Sizes of the two bounded queues, in datagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Queues {
    /// Outbound queue.
    pub send: usize,
    /// Inbound queue.
    pub receive: usize,
}

/// A validated configuration of one adapter instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The adapter's name in the scenario.
    pub name: String,
    /// Version written (both are read).
    pub version: DisVersion,
    /// Exercise ID sent and required.
    pub exercise: u8,
    /// Site of the key space.
    pub site: u16,
    /// Application of the key space.
    pub application: u16,
    /// Sending mode.
    pub mode: Mode,
    /// Receive address.
    pub bind: SocketAddrV4,
    /// Queue sizes.
    pub queues: Queues,
    /// Force name to DIS force ID (IDs are unique and non-zero).
    pub forces: BTreeMap<String, u8>,
    /// Weapon ID to munition entity type.
    pub weapons: BTreeMap<String, EntityType>,
}

/// Keys of a template's `[network.<name>]` table that only the DIS adapter reads (beyond the keys the
/// engine reads, ASM-228). None yet.
const OWN_TEMPLATE_KEYS: &[&str] = &[];

const KNOWN_KEYS: [&str; 14] = [
    "version",
    "exercise",
    "site",
    "application",
    "mode",
    "bind",
    "destinations",
    "broadcast_address",
    "group",
    "interface",
    "ttl",
    "forces",
    "weapons",
    "queue",
];

/// `[a-z][a-z0-9_]*` and at most 128 bytes: one ID segment (ADR 0002).
fn is_segment(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A force ID of the engine: one segment.
fn is_force_id(s: &str) -> bool {
    s.len() <= umbra_sdk::abi::UMBRA_MAX_ID_LEN && is_segment(s)
}

/// A weapon ID of the engine: at least two dot-separated segments.
fn is_weapon_id(s: &str) -> bool {
    s.len() <= umbra_sdk::abi::UMBRA_MAX_ID_LEN && s.contains('.') && s.split('.').all(is_segment)
}

fn bad(what: &str, why: impl std::fmt::Display) -> Error {
    Error::params(format!("{what}: {why}"))
}

fn uint(v: &Value, key: &str, min: u64, max: u64) -> Result<u64, Error> {
    match v.as_u64() {
        Some(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(bad(
            &format!("`{key}`"),
            format!("must be an integer from {min} to {max}, got {v}"),
        )),
    }
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.as_str()
        .ok_or_else(|| bad(&format!("`{key}`"), format!("must be a string, got {v}")))
}

fn v4_addr(v: &Value, key: &str) -> Result<SocketAddrV4, Error> {
    let s = text(v, key)?;
    match s.parse::<SocketAddr>() {
        Ok(SocketAddr::V4(a)) => Ok(a),
        Ok(SocketAddr::V6(_)) => Err(bad(
            &format!("`{key}`"),
            format!("`{s}` is an IPv6 address; the DIS adapter is IPv4 only"),
        )),
        Err(_) => Err(bad(
            &format!("`{key}`"),
            format!("`{s}` is not an `ip:port` address"),
        )),
    }
}

fn v4_ip(v: &Value, key: &str) -> Result<Ipv4Addr, Error> {
    let s = text(v, key)?;
    s.parse::<Ipv4Addr>()
        .map_err(|_| bad(&format!("`{key}`"), format!("`{s}` is not an IPv4 address")))
}

fn nonzero_port(addr: SocketAddrV4, key: &str) -> Result<SocketAddrV4, Error> {
    if addr.port() == 0 {
        Err(bad(&format!("`{key}`"), format!("`{addr}` has port 0")))
    } else {
        Ok(addr)
    }
}

impl Config {
    /// The configuration of the adapter described by `input` (the ADR 0030 wrapper).
    ///
    /// # Errors
    /// [`Error::params`] naming the key, value or template at fault.
    pub fn from_input(input: &ConfigureInput) -> Result<Self, Error> {
        Self::parse(input.name(), input.params(), input.templates())
    }

    /// The configuration for `params` of the adapter `name`, with each template's
    /// `[network.<name>]` table in `templates`.
    ///
    /// # Errors
    /// [`Error::params`] naming the key, value or template at fault.
    pub fn parse(
        name: &str,
        params: &Value,
        templates: &BTreeMap<String, Value>,
    ) -> Result<Self, Error> {
        let table = params
            .as_object()
            .ok_or_else(|| bad("params", "must be a table"))?;
        if let Some(unknown) = table.keys().find(|k| !KNOWN_KEYS.contains(&k.as_str())) {
            return Err(bad(
                &format!("`{unknown}`"),
                format!(
                    "is not a DIS adapter parameter (known: {})",
                    KNOWN_KEYS.join(", ")
                ),
            ));
        }
        let get = |key: &str| params.get(key);
        let version = match get("version") {
            None => DisVersion::V7,
            Some(v) => match v.as_u64() {
                Some(6) => DisVersion::V6,
                Some(7) => DisVersion::V7,
                _ => return Err(bad("`version`", format!("must be 6 or 7, got {v}"))),
            },
        };
        let exercise = match get("exercise") {
            None => 1,
            Some(v) => u8::try_from(uint(v, "exercise", 0, 255)?).unwrap_or(1),
        };
        let id = |key: &str| -> Result<u16, Error> {
            let v = get(key).ok_or_else(|| bad(&format!("`{key}`"), "is required"))?;
            Ok(u16::try_from(uint(v, key, 1, 65_534)?).unwrap_or(1))
        };
        let site = id("site")?;
        let application = id("application")?;
        let mode = Self::mode(params)?;
        let bind = match get("bind") {
            Some(v) => v4_addr(v, "bind")?,
            None if matches!(mode, Mode::Unicast { .. }) => DEFAULT_BIND,
            None => DEFAULT_WIDE_BIND,
        };
        if !matches!(mode, Mode::Unicast { .. }) && bind.ip().is_loopback() {
            return Err(bad(
                &format!("adapter `{name}` `bind`"),
                format!(
                    "`{bind}` is a loopback address, which never receives broadcast or multicast packets; \
                     use 0.0.0.0 or the address of an interface (or mode `unicast`)"
                ),
            ));
        }
        let queues = Self::queues(get("queue"))?;
        let forces = Self::forces(get("forces"))?;
        let weapons = Self::weapons(get("weapons"))?;
        Self::check_templates(name, templates)?;
        Ok(Self {
            name: name.to_owned(),
            version,
            exercise,
            site,
            application,
            mode,
            bind,
            queues,
            forces,
            weapons,
        })
    }

    fn mode(params: &Value) -> Result<Mode, Error> {
        let get = |key: &str| params.get(key);
        let name = match get("mode") {
            None => "unicast",
            Some(v) => text(v, "mode")?,
        };
        // Keys of the other modes are a mistake, not something to ignore silently.
        let only = |keys: &[&str], mode: &str| -> Result<(), Error> {
            for key in keys {
                if get(key).is_some() {
                    return Err(bad(
                        &format!("`{key}`"),
                        format!("only applies to mode `{mode}`, but the mode is `{name}`"),
                    ));
                }
            }
            Ok(())
        };
        let required = |key: &str| -> Result<&Value, Error> {
            get(key)
                .ok_or_else(|| bad(&format!("`{key}`"), format!("is required in mode `{name}`")))
        };
        match name {
            "unicast" => {
                only(&["broadcast_address"], "broadcast")?;
                only(&["group", "interface", "ttl"], "multicast")?;
                let destinations = match get("destinations") {
                    None => Vec::new(),
                    Some(v) => v
                        .as_array()
                        .ok_or_else(|| bad("`destinations`", format!("must be a list, got {v}")))?
                        .iter()
                        .map(|d| nonzero_port(v4_addr(d, "destinations")?, "destinations"))
                        .collect::<Result<Vec<_>, _>>()?,
                };
                Ok(Mode::Unicast { destinations })
            }
            "broadcast" => {
                only(&["destinations"], "unicast")?;
                only(&["group", "interface", "ttl"], "multicast")?;
                let address = nonzero_port(
                    v4_addr(required("broadcast_address")?, "broadcast_address")?,
                    "broadcast_address",
                )?;
                Ok(Mode::Broadcast { address })
            }
            "multicast" => {
                only(&["destinations"], "unicast")?;
                only(&["broadcast_address"], "broadcast")?;
                let group = v4_ip(required("group")?, "group")?;
                if !group.is_multicast() {
                    return Err(bad(
                        "`group`",
                        format!("`{group}` is not a multicast address"),
                    ));
                }
                let interface = match get("interface") {
                    None => Ipv4Addr::UNSPECIFIED,
                    Some(v) => v4_ip(v, "interface")?,
                };
                let ttl = match get("ttl") {
                    None => DEFAULT_TTL,
                    Some(v) => u32::try_from(uint(v, "ttl", 0, 255)?).unwrap_or(DEFAULT_TTL),
                };
                Ok(Mode::Multicast {
                    group,
                    interface,
                    ttl,
                })
            }
            other => Err(bad(
                "`mode`",
                format!("`{other}` is not unicast, broadcast or multicast"),
            )),
        }
    }

    fn queues(v: Option<&Value>) -> Result<Queues, Error> {
        let mut queues = Queues {
            send: DEFAULT_QUEUE,
            receive: DEFAULT_QUEUE,
        };
        let Some(v) = v else { return Ok(queues) };
        let table = v
            .as_object()
            .ok_or_else(|| bad("`queue`", format!("must be a table, got {v}")))?;
        let max = u64::try_from(MAX_QUEUE).unwrap_or(u64::MAX);
        for (key, value) in table {
            let size = match key.as_str() {
                "send" | "receive" => uint(value, &format!("queue.{key}"), 1, max)?,
                other => {
                    return Err(bad(
                        &format!("`queue.{other}`"),
                        "is not a queue (send, receive)",
                    ));
                }
            };
            let size = usize::try_from(size).unwrap_or(DEFAULT_QUEUE);
            if key == "send" {
                queues.send = size;
            } else {
                queues.receive = size;
            }
        }
        Ok(queues)
    }

    fn forces(v: Option<&Value>) -> Result<BTreeMap<String, u8>, Error> {
        let Some(v) = v else {
            return Ok(BTreeMap::from([
                ("friendly".to_owned(), 1),
                ("opposing".to_owned(), 2),
                ("neutral".to_owned(), 3),
            ]));
        };
        let table = v
            .as_object()
            .ok_or_else(|| bad("`forces`", format!("must be a table, got {v}")))?;
        let mut forces = BTreeMap::new();
        let mut owners: BTreeMap<u8, &str> = BTreeMap::new();
        for (name, value) in table {
            let what = format!("forces.{name}");
            if !is_force_id(name) {
                return Err(bad(
                    &format!("`{what}`"),
                    "is not a force ID ([a-z][a-z0-9_]*, at most 128 bytes)",
                ));
            }
            let id = u8::try_from(uint(value, &what, 1, 255)?).unwrap_or(1);
            if let Some(other) = owners.insert(id, name) {
                return Err(bad(
                    &format!("`{what}`"),
                    format!("force ID {id} is already used by `{other}`"),
                ));
            }
            forces.insert(name.clone(), id);
        }
        Ok(forces)
    }

    fn weapons(v: Option<&Value>) -> Result<BTreeMap<String, EntityType>, Error> {
        let Some(v) = v else {
            return Ok(BTreeMap::new());
        };
        let table = v
            .as_object()
            .ok_or_else(|| bad("`weapons`", format!("must be a table, got {v}")))?;
        let mut weapons = BTreeMap::new();
        for (weapon, value) in table {
            let what = format!("weapons.{weapon}");
            if !is_weapon_id(weapon) {
                return Err(bad(
                    &format!("`{what}`"),
                    "is not a weapon ID (at least two dot-separated segments [a-z][a-z0-9_]*)",
                ));
            }
            let token = text(value, &what)?;
            let munition = parse_entity_type(token)
                .map_err(|e| bad(&format!("`{what}`"), format!("`{token}`: {e}")))?;
            weapons.insert(weapon.clone(), munition);
        }
        Ok(weapons)
    }

    /// Each template's `[network.<name>]` table may hold the keys the engine reads
    /// ([`MANAGER_TEMPLATE_KEYS`], `entity_type`) and the keys of [`OWN_TEMPLATE_KEYS`] (none yet);
    /// `entity_type` must be a token.
    fn check_templates(name: &str, templates: &BTreeMap<String, Value>) -> Result<(), Error> {
        for (template, table) in templates {
            let here = format!("template `{template}`: [network.{name}]");
            let table = table
                .as_object()
                .ok_or_else(|| Error::params(format!("{here} must be a table")))?;
            for (key, value) in table {
                if !MANAGER_TEMPLATE_KEYS.contains(&key.as_str())
                    && !OWN_TEMPLATE_KEYS.contains(&key.as_str())
                {
                    return Err(Error::params(format!(
                        "{here} has the unknown key `{key}` (the DIS adapter reads `entity_type`)"
                    )));
                }
                if key == "entity_type" {
                    let token = value.as_str().ok_or_else(|| {
                        Error::params(format!("{here} entity_type must be a string, got {value}"))
                    })?;
                    parse_entity_type(token)
                        .map_err(|e| Error::params(format!("{here} entity_type `{token}`: {e}")))?;
                }
            }
        }
        Ok(())
    }

    /// The warning to log when this configuration listens to or talks to a network
    /// ("accepts unauthenticated DIS traffic from the network", NFR-4): a bind address that is not
    /// loopback, broadcast mode or multicast mode. `None` for a loopback unicast adapter.
    #[must_use]
    pub fn security_warning(&self) -> Option<String> {
        let why = match &self.mode {
            Mode::Broadcast { .. } => "broadcast mode",
            Mode::Multicast { .. } => "multicast mode",
            Mode::Unicast { .. } if !self.bind.ip().is_loopback() => "bind address is not loopback",
            Mode::Unicast { .. } => return None,
        };
        Some(format!(
            "DIS adapter `{}` accepts unauthenticated DIS traffic from the network ({why}, bind {})",
            self.name, self.bind
        ))
    }
}
