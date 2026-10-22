//! Translation between the SDK message mirror and DIS PDUs (REQ-N-5, REQ-N-6, ADR 0034,
//! `docs/net/dis.md`).
//!
//! Pure and per instance: a [`Translator`] owns the tables of one configured adapter (forces, weapons,
//! identity, counters for event and request IDs, the last state of each published entity) and nothing
//! is global (ASM-410). It never sees a socket or a clock: time arrives as a [`Stamp`] made from the
//! tick and rate of the call (REQ-K-4, ASM-237). The entity type token grammar, the key packing, the
//! dead reckoning table and the marking rules live here and nowhere else.

#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use umbra_sdk::{ControlAction, DeadReckoning, EntityState, Euler, KeySpace, NetKey, NetMessage};

use crate::codec::{self, EntityState as PduEntityState};
use crate::codec::{
    APPEARANCE_DEACTIVATED, Body, BurstDescriptor, ClockTime, DeadReckoningAlgorithm,
    DeadReckoningParameters, Descriptor, Detonation, EncodeError, EntityId, EntityType, EventId,
    Fire, Header, Marking, Pdu, StartResume, StopFreeze,
};
use crate::config::Config;
use crate::version::DisVersion;

/// Entity numbers 0 and 0xFFFF are reserved by the standard (none, all) and never name an entity.
const NO_ENTITY: u16 = 0;
/// "All sites", "all applications", "all entities".
const ALL: u16 = 0xFFFF;
/// Detonation result 5: detonation (SISO-REF-010).
const RESULT_DETONATION: u8 = 5;
/// Stop/Freeze reason 2: recess.
const REASON_RECESS: u8 = 2;
/// Stop/Freeze reason 3: termination.
const REASON_TERMINATION: u8 = 3;
/// Marking character set 1: ASCII.
const MARKING_ASCII: u8 = 1;
/// Characters in an entity marking.
const MARKING_LEN: usize = 11;
/// Units in one hour of a DIS timestamp (the 31 bits above the absolute/relative bit).
const UNITS_PER_HOUR: u128 = 1 << 31;
/// Seconds in the hour the timestamp counts within.
const SECONDS_PER_HOUR: u128 = 3600;

// ------------------------------------------------------------------------------------- time

/// The simulation time of a call: tick `tick` of a run at `rate_hz` (never a wall clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    /// Tick number.
    pub tick: u64,
    /// Tick rate (Hz, never 0).
    pub rate_hz: u32,
}

impl Stamp {
    /// Time units (3600 s / 2^31) past the hour at this stamp; 0 for a zero rate.
    fn units_past_hour(self) -> u32 {
        let per_hour = u128::from(self.rate_hz).saturating_mul(SECONDS_PER_HOUR);
        let units = u128::from(self.tick)
            .saturating_mul(UNITS_PER_HOUR)
            .checked_div(per_hour)
            .unwrap_or(0);
        u32::try_from(units.checked_rem(UNITS_PER_HOUR).unwrap_or(0)).unwrap_or(0)
    }

    /// The whole hours elapsed (saturating at `i32::MAX`).
    fn hours(self) -> i32 {
        let per_hour = u64::from(self.rate_hz).saturating_mul(3600);
        let hours = self.tick.checked_div(per_hour).unwrap_or(0);
        i32::try_from(hours).unwrap_or(i32::MAX)
    }

    /// The relative header timestamp: the units past the hour in the upper 31 bits, bit 0 = 0
    /// (relative, ASM-237).
    #[must_use]
    pub fn timestamp(self) -> u32 {
        self.units_past_hour() << 1
    }

    /// The clock time record of this stamp (hour and relative time past the hour).
    #[must_use]
    pub fn clock_time(self) -> ClockTime {
        ClockTime {
            hour: self.hours(),
            time_past_hour: self.timestamp(),
        }
    }
}

// ------------------------------------------------------------------------------ identity keys

/// `NetKey = site << 32 | application << 16 | entity` (ASM-225).
#[must_use]
pub fn pack_key(id: EntityId) -> NetKey {
    NetKey((u64::from(id.site) << 32) | (u64::from(id.application) << 16) | u64::from(id.entity))
}

/// The entity ID of `key`; `None` when the key does not fit 48 bits or its entity number is
/// reserved (0, 0xFFFF).
#[must_use]
pub fn unpack_key(key: NetKey) -> Option<EntityId> {
    let site = u16::try_from(key.0 >> 32).ok()?;
    let application = u16::try_from((key.0 >> 16) & 0xFFFF).ok()?;
    let entity = u16::try_from(key.0 & 0xFFFF).ok()?;
    valid_entity(entity).then_some(EntityId {
        site,
        application,
        entity,
    })
}

fn valid_entity(entity: u16) -> bool {
    entity != NO_ENTITY && entity != ALL
}

/// The key space of an adapter with this site and application: entities `1..=65534`.
///
/// # Errors
/// Never in practice (the range is non-empty and does not overflow); the SDK type is fallible.
pub fn key_space(site: u16, application: u16) -> Result<KeySpace, umbra_sdk::Error> {
    let base = (u64::from(site) << 32) | (u64::from(application) << 16);
    KeySpace::new(base, u64::from(ALL - 1))
}

// ---------------------------------------------------------------------------- entity type tokens

/// Why an entity type token is not a DIS entity type.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// Not seven dot-separated fields.
    #[error(
        "expected seven dot-separated decimal fields (kind.domain.country.category.subcategory.specific.extra), found {0}"
    )]
    Fields(usize),
    /// A field is empty or has a character other than a decimal digit.
    #[error("field {index} `{text}` is not a decimal number")]
    NotDecimal {
        /// 1-based field number.
        index: usize,
        /// The field text.
        text: String,
    },
    /// A field exceeds its width on the wire.
    #[error("field {index} `{text}` is above {max}")]
    Range {
        /// 1-based field number.
        index: usize,
        /// The field text.
        text: String,
        /// The largest value of the field.
        max: u32,
    },
}

/// Parses `kind.domain.country.category.subcategory.specific.extra` (decimal, no signs or spaces;
/// leading zeros are read).
///
/// # Errors
/// [`TokenError`].
pub fn parse_entity_type(token: &str) -> Result<EntityType, TokenError> {
    const MAX: [u32; 7] = [255, 255, 65_535, 255, 255, 255, 255];
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != MAX.len() {
        return Err(TokenError::Fields(parts.len()));
    }
    let mut values = [0_u32; 7];
    for (i, ((part, max), slot)) in parts.iter().zip(MAX).zip(values.iter_mut()).enumerate() {
        let index = i.saturating_add(1);
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(TokenError::NotDecimal {
                index,
                text: (*part).to_owned(),
            });
        }
        *slot = match part.parse::<u32>() {
            Ok(v) if v <= max => v,
            _ => {
                return Err(TokenError::Range {
                    index,
                    text: (*part).to_owned(),
                    max,
                });
            }
        };
    }
    let [
        kind,
        domain,
        country,
        category,
        subcategory,
        specific,
        extra,
    ] = values;
    let byte = |v: u32| u8::try_from(v).unwrap_or(u8::MAX);
    Ok(EntityType {
        kind: byte(kind),
        domain: byte(domain),
        country: u16::try_from(country).unwrap_or(u16::MAX),
        category: byte(category),
        subcategory: byte(subcategory),
        specific: byte(specific),
        extra: byte(extra),
    })
}

/// The normalised token of `t`: seven decimal fields without leading zeros.
#[must_use]
pub fn format_entity_type(t: EntityType) -> String {
    format!(
        "{}.{}.{}.{}.{}.{}.{}",
        t.kind, t.domain, t.country, t.category, t.subcategory, t.specific, t.extra
    )
}

fn is_none_type(t: EntityType) -> bool {
    t == EntityType::default()
}

// ------------------------------------------------------------------------------ dead reckoning

fn algorithm_of(dr: DeadReckoning) -> Option<DeadReckoningAlgorithm> {
    Some(match dr {
        DeadReckoning::Static => DeadReckoningAlgorithm::Static,
        DeadReckoning::Velocity => DeadReckoningAlgorithm::Fpw,
        DeadReckoning::VelocityRotation => DeadReckoningAlgorithm::Rpw,
        DeadReckoning::AccelerationRotation => DeadReckoningAlgorithm::Rvw,
        DeadReckoning::Acceleration => DeadReckoningAlgorithm::Fvw,
        DeadReckoning::BodyVelocity => DeadReckoningAlgorithm::Fpb,
        DeadReckoning::BodyVelocityRotation => DeadReckoningAlgorithm::Rpb,
        DeadReckoning::BodyAccelerationRotation => DeadReckoningAlgorithm::Rvb,
        DeadReckoning::BodyAcceleration => DeadReckoningAlgorithm::Fvb,
        _ => return None,
    })
}

/// The canonical model of a received algorithm code and whether the code was reserved (10..=255,
/// treated as static and counted). Code 0 (other) is static too.
fn dead_reckoning_of(code: u8) -> (DeadReckoning, bool) {
    match DeadReckoningAlgorithm::from_code(code) {
        DeadReckoningAlgorithm::Other | DeadReckoningAlgorithm::Static => {
            (DeadReckoning::Static, false)
        }
        DeadReckoningAlgorithm::Fpw => (DeadReckoning::Velocity, false),
        DeadReckoningAlgorithm::Rpw => (DeadReckoning::VelocityRotation, false),
        DeadReckoningAlgorithm::Rvw => (DeadReckoning::AccelerationRotation, false),
        DeadReckoningAlgorithm::Fvw => (DeadReckoning::Acceleration, false),
        DeadReckoningAlgorithm::Fpb => (DeadReckoning::BodyVelocity, false),
        DeadReckoningAlgorithm::Rpb => (DeadReckoning::BodyVelocityRotation, false),
        DeadReckoningAlgorithm::Rvb => (DeadReckoning::BodyAccelerationRotation, false),
        DeadReckoningAlgorithm::Fvb => (DeadReckoning::BodyAcceleration, false),
        DeadReckoningAlgorithm::Reserved(_) => (DeadReckoning::Static, true),
    }
}

// ------------------------------------------------------------------------------------ marking

/// The 11 marking bytes for `name`: ASCII, anything else (non-ASCII and control characters)
/// replaced by `?`, cut at 11 characters, NUL padded.
#[must_use]
pub fn marking_of(name: Option<&str>) -> Marking {
    let mut characters = [0_u8; MARKING_LEN];
    for (slot, c) in characters.iter_mut().zip(name.unwrap_or("").chars()) {
        *slot = if c.is_ascii() && !c.is_ascii_control() {
            u8::try_from(c).unwrap_or(b'?')
        } else {
            b'?'
        };
    }
    Marking {
        character_set: MARKING_ASCII,
        characters,
    }
}

/// The name in `marking`: the text up to the first NUL, bytes that are not printable ASCII replaced
/// by `?`; `None` when empty.
#[must_use]
pub fn name_of(marking: &Marking) -> Option<String> {
    let text: String = marking
        .characters
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| {
            if b.is_ascii() && !b.is_ascii_control() {
                char::from(*b)
            } else {
                '?'
            }
        })
        .collect();
    (!text.is_empty()).then_some(text)
}

// -------------------------------------------------------------------------------- f32 conversion

#[expect(
    clippy::cast_possible_truncation,
    reason = "f64 to f32 is the precision of the wire; a result that is not finite is refused"
)]
fn to_f32(v: f64) -> Option<f32> {
    let f = v as f32;
    f.is_finite().then_some(f)
}

fn to_f32s(v: [f64; 3], what: &'static str) -> Result<[f32; 3], Reject> {
    let [x, y, z] = v;
    match (to_f32(x), to_f32(y), to_f32(z)) {
        (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
        _ => Err(Reject::NotFinite(what)),
    }
}

fn widen(v: [f32; 3]) -> [f64; 3] {
    v.map(f64::from)
}

fn finite3(v: &[f64; 3]) -> bool {
    v.iter().all(|c| c.is_finite())
}

// ------------------------------------------------------------------------------------ outcomes

/// Why a message is not sent. Counted by the adapter (`tx_dropped`), never a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Reject {
    /// The key is not a DIS entity ID (above 48 bits or a reserved entity number).
    #[error("key {0:#x} is not a DIS entity ID (48 bits, entity number 1..=65534)")]
    Key(u64),
    /// An entity state or removal whose key lies outside this adapter's key space.
    #[error("key {0:#x} is outside this adapter's key space")]
    ForeignKey(u64),
    /// A value is not finite once converted to the 32-bit wire format.
    #[error("{0} is not finite as a 32-bit float")]
    NotFinite(&'static str),
    /// A message kind or dead reckoning model this adapter version cannot send.
    #[error("a message or dead reckoning model this adapter cannot send")]
    Unsupported,
    /// The codec refused the PDU.
    #[error(transparent)]
    Encode(#[from] EncodeError),
}

/// Why a received PDU is not turned into a message. Counted by the adapter (`decode_errors`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Skip {
    /// The PDU belongs to another exercise.
    #[error("PDU of another exercise")]
    Exercise,
    /// An entity ID with a reserved entity number where an entity is required.
    #[error("reserved entity ID")]
    EntityId,
    /// A number that is not finite.
    #[error("a value is not finite")]
    NotFinite,
    /// A Fire whose munition type is in no `weapons` entry.
    #[error("munition type in no `weapons` entry")]
    UnmappedWeapon,
    /// A Start/Resume or Stop/Freeze that this adapter's own site and application sent (the packet came
    /// back over broadcast or multicast).
    #[error("run control echoed from this adapter")]
    OwnControl,
    /// A message the SDK would refuse (cannot happen with the checks above; kept as a safety net).
    #[error("message refused by the SDK checks")]
    Invalid,
}

// ---------------------------------------------------------------------------- inbound messages

/// A received entity state with owned strings (the SDK message borrows them).
#[derive(Debug, Clone, PartialEq)]
pub struct InboundState {
    /// The entity's key.
    pub key: NetKey,
    /// The normalised entity type token, `None` for an all-zero type.
    pub entity_type: Option<String>,
    /// The force name of the `forces` table, `None` for an unknown id.
    pub force: Option<Arc<str>>,
    /// The marking text.
    pub name: Option<String>,
    /// ECEF position (m).
    pub position: [f64; 3],
    /// ECEF velocity (m/s).
    pub velocity: [f64; 3],
    /// Acceleration (m/s^2).
    pub acceleration: [f64; 3],
    /// Body angular velocity (rad/s).
    pub angular_velocity: [f64; 3],
    /// World-frame Euler angles.
    pub orientation: Euler,
    /// The dead reckoning model.
    pub dead_reckoning: DeadReckoning,
}

/// A message received from the network, owning its strings.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Inbound {
    /// An entity state.
    State(InboundState),
    /// An entity left (a deactivated Entity State).
    Removed(NetKey),
    /// A weapon was fired.
    Fire {
        /// The firing entity.
        shooter: NetKey,
        /// The target, if the PDU names one.
        target: Option<NetKey>,
        /// The weapon of the `weapons` table.
        weapon: String,
        /// The launch point (ECEF, m).
        position: [f64; 3],
    },
    /// A weapon effect.
    Detonation {
        /// The firing entity, if the PDU names one.
        shooter: Option<NetKey>,
        /// The target, if the PDU names one.
        target: Option<NetKey>,
        /// The burst point (ECEF, m).
        position: [f64; 3],
    },
    /// A run-control action.
    Control(ControlAction),
}

impl Inbound {
    /// The SDK message borrowing this value.
    #[must_use]
    pub fn message(&self) -> NetMessage<'_> {
        match self {
            Inbound::State(s) => NetMessage::EntityState(EntityState {
                key: s.key,
                entity_type: s.entity_type.as_deref(),
                force: s.force.as_deref(),
                name: s.name.as_deref(),
                position: s.position,
                velocity: s.velocity,
                acceleration: s.acceleration,
                angular_velocity: s.angular_velocity,
                orientation: s.orientation,
                dead_reckoning: s.dead_reckoning,
            }),
            Inbound::Removed(key) => NetMessage::EntityRemoved { key: *key },
            Inbound::Fire {
                shooter,
                target,
                weapon,
                position,
            } => NetMessage::Fire {
                shooter: *shooter,
                target: *target,
                weapon,
                position: *position,
            },
            Inbound::Detonation {
                shooter,
                target,
                position,
            } => NetMessage::Detonation {
                shooter: *shooter,
                target: *target,
                position: *position,
            },
            Inbound::Control(action) => NetMessage::SimulationControl(*action),
        }
    }
}

/// What the translator tallies while decoding (the adapter adds it to its counters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InboundTally {
    /// Entity States with a reserved dead reckoning code (treated as static).
    pub reserved_dead_reckoning: u64,
}

// ------------------------------------------------------------------------------------ translator

/// The translation state of one adapter instance.
#[derive(Debug)]
pub struct Translator {
    version: DisVersion,
    exercise: u8,
    site: u16,
    application: u16,
    forces: BTreeMap<String, u8>,
    force_names: BTreeMap<u8, Arc<str>>,
    weapons: BTreeMap<String, EntityType>,
    weapon_by_type: BTreeMap<EntityType, String>,
    next_event: u16,
    next_request: u32,
    last_state: BTreeMap<u64, PduEntityState>,
    tally: InboundTally,
}

impl Translator {
    /// The translator of `config` (the force and weapon tables are copied).
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let force_names = config
            .forces
            .iter()
            .map(|(name, id)| (*id, Arc::from(name.as_str())))
            .collect();
        // Several weapons with one munition type: the smallest weapon ID wins (BTreeMap order).
        let mut weapon_by_type = BTreeMap::new();
        for (weapon, munition) in &config.weapons {
            weapon_by_type
                .entry(*munition)
                .or_insert_with(|| weapon.clone());
        }
        Self {
            version: config.version,
            exercise: config.exercise,
            site: config.site,
            application: config.application,
            forces: config.forces.clone(),
            force_names,
            weapons: config.weapons.clone(),
            weapon_by_type,
            next_event: 1,
            next_request: 1,
            last_state: BTreeMap::new(),
            tally: InboundTally::default(),
        }
    }

    /// What the inbound side tallied so far.
    #[must_use]
    pub fn tally(&self) -> InboundTally {
        self.tally
    }

    fn header(&self, at: Stamp) -> Header {
        Header {
            exercise_id: self.exercise,
            timestamp: at.timestamp(),
            status: 0,
        }
    }

    fn own_id(&self, entity: u16) -> EntityId {
        EntityId {
            site: self.site,
            application: self.application,
            entity,
        }
    }

    fn event_id(&mut self) -> EventId {
        let event = self.next_event;
        // 0 is not issued: the counter wraps from 65535 to 1.
        self.next_event = if event == u16::MAX {
            1
        } else {
            event.saturating_add(1)
        };
        EventId {
            site: self.site,
            application: self.application,
            event,
        }
    }

    fn request_id(&mut self) -> u32 {
        let id = self.next_request;
        self.next_request = id.wrapping_add(1).max(1);
        id
    }

    // ------------------------------------------------------------------------------ outbound

    /// The datagram for `message` at time `at`.
    ///
    /// # Errors
    /// [`Reject`] when the message cannot be sent (counted by the caller).
    pub fn outbound(&mut self, at: Stamp, message: &NetMessage<'_>) -> Result<Vec<u8>, Reject> {
        let pdu = match message {
            NetMessage::EntityState(s) => self.entity_state(at, s)?,
            NetMessage::EntityRemoved { key } => self.removal(at, *key)?,
            NetMessage::Fire {
                shooter,
                target,
                weapon,
                position,
            } => self.fire(at, *shooter, *target, weapon, position)?,
            NetMessage::Detonation {
                shooter,
                target,
                position,
            } => self.detonation(at, *shooter, *target, position)?,
            NetMessage::SimulationControl(action) => self.control(at, *action),
            _ => return Err(Reject::Unsupported),
        };
        let mut out = Vec::new();
        codec::encode(&pdu, self.version, &mut out)?;
        Ok(out)
    }

    fn is_own(&self, id: EntityId) -> bool {
        id.site == self.site && id.application == self.application
    }

    fn own_entity(&self, key: NetKey) -> Result<EntityId, Reject> {
        let id = unpack_key(key).ok_or(Reject::Key(key.0))?;
        if id.site == self.site && id.application == self.application {
            Ok(id)
        } else {
            Err(Reject::ForeignKey(key.0))
        }
    }

    fn entity_state(&mut self, at: Stamp, s: &EntityState<'_>) -> Result<Pdu, Reject> {
        let entity_id = self.own_entity(s.key)?;
        let algorithm = algorithm_of(s.dead_reckoning).ok_or(Reject::Unsupported)?;
        let entity_type = match s.entity_type {
            // A token that does not parse cannot come from a validated template; refuse it.
            Some(token) => parse_entity_type(token).map_err(|_| Reject::Unsupported)?,
            None => EntityType::default(),
        };
        let [yaw, pitch, roll] = to_f32s(
            [s.orientation.yaw, s.orientation.pitch, s.orientation.roll],
            "orientation",
        )?;
        let state = PduEntityState {
            entity_id,
            force_id: s
                .force
                .and_then(|name| self.forces.get(name).copied())
                .unwrap_or(0),
            entity_type,
            alternative_entity_type: EntityType::default(),
            linear_velocity: to_f32s(s.velocity, "velocity")?,
            location: s.position,
            orientation: [yaw, pitch, roll],
            appearance: 0,
            dead_reckoning: DeadReckoningParameters {
                algorithm: algorithm.code(),
                other: [0; 15],
                linear_acceleration: to_f32s(s.acceleration, "acceleration")?,
                angular_velocity: to_f32s(s.angular_velocity, "angular velocity")?,
            },
            marking: marking_of(s.name),
            capabilities: 0,
        };
        if !finite3(&state.location) {
            return Err(Reject::NotFinite("position"));
        }
        // At most one state per key of this adapter's key space (65534), so the map is bounded.
        self.last_state.insert(s.key.0, state.clone());
        Ok(Pdu {
            header: self.header(at),
            body: Body::EntityState(state),
        })
    }

    /// The final Entity State of a removed entity: its last published state with the deactivated
    /// appearance bit; for an entity never published, an otherwise empty one.
    fn removal(&mut self, at: Stamp, key: NetKey) -> Result<Pdu, Reject> {
        let entity_id = self.own_entity(key)?;
        let mut state = self
            .last_state
            .remove(&key.0)
            .unwrap_or_else(|| PduEntityState {
                entity_id,
                marking: marking_of(None),
                ..PduEntityState::default()
            });
        state.appearance |= APPEARANCE_DEACTIVATED;
        Ok(Pdu {
            header: self.header(at),
            body: Body::EntityState(state),
        })
    }

    fn fire(
        &mut self,
        at: Stamp,
        shooter: NetKey,
        target: Option<NetKey>,
        weapon: &str,
        position: &[f64; 3],
    ) -> Result<Pdu, Reject> {
        let firing_entity = self.own_entity(shooter)?;
        // A weapon with no `weapons` entry is sent with no munition type (zero, "other").
        let munition = self.weapons.get(weapon).copied().unwrap_or_default();
        let event = self.event_id();
        Ok(Pdu {
            header: self.header(at),
            body: Body::Fire(Fire {
                firing_entity,
                target_entity: target.and_then(unpack_key).unwrap_or_default(),
                munition: EntityId::default(),
                event,
                fire_mission_index: 0,
                location: *position,
                descriptor: Descriptor::Munition(BurstDescriptor {
                    munition,
                    warhead: 0,
                    fuse: 0,
                    quantity: 1,
                    rate: 0,
                }),
                velocity: [0.0; 3],
                range: 0.0,
            }),
        })
    }

    fn detonation(
        &mut self,
        at: Stamp,
        shooter: Option<NetKey>,
        target: Option<NetKey>,
        position: &[f64; 3],
    ) -> Result<Pdu, Reject> {
        let firing_entity = match shooter {
            Some(key) => self.own_entity(key)?,
            None => EntityId::default(),
        };
        let event = self.event_id();
        Ok(Pdu {
            header: self.header(at),
            body: Body::Detonation(Detonation {
                firing_entity,
                target_entity: target.and_then(unpack_key).unwrap_or_default(),
                munition: EntityId::default(),
                event,
                velocity: [0.0; 3],
                location: *position,
                // The canonical Detonation carries no weapon: the munition is unknown (zero type,
                // warhead and fuse "other"), the result is a detonation.
                descriptor: Descriptor::Munition(BurstDescriptor::default()),
                location_in_entity: [0.0; 3],
                result: RESULT_DETONATION,
            }),
        })
    }

    fn control(&mut self, at: Stamp, action: ControlAction) -> Pdu {
        let originating_entity = self.own_id(NO_ENTITY);
        let receiving_entity = EntityId {
            site: ALL,
            application: ALL,
            entity: ALL,
        };
        let request_id = self.request_id();
        let body = match action {
            ControlAction::Pause | ControlAction::Stop => Body::StopFreeze(StopFreeze {
                originating_entity,
                receiving_entity,
                real_world_time: at.clock_time(),
                reason: if action == ControlAction::Pause {
                    REASON_RECESS
                } else {
                    REASON_TERMINATION
                },
                frozen_behavior: 0,
                request_id,
            }),
            // `ControlAction` is non-exhaustive: Start, Resume and anything added later start.
            _ => Body::StartResume(StartResume {
                originating_entity,
                receiving_entity,
                real_world_time: at.clock_time(),
                simulation_time: at.clock_time(),
                request_id,
            }),
        };
        Pdu {
            header: self.header(at),
            body,
        }
    }

    // ------------------------------------------------------------------------------- inbound

    /// The message a received PDU stands for.
    ///
    /// # Errors
    /// [`Skip`] when the PDU is dropped (counted by the caller).
    pub fn inbound(&mut self, pdu: &Pdu) -> Result<Inbound, Skip> {
        if pdu.header.exercise_id != self.exercise {
            return Err(Skip::Exercise);
        }
        let inbound = match &pdu.body {
            Body::EntityState(s) => self.inbound_state(s)?,
            Body::Fire(f) => self.inbound_fire(f)?,
            Body::Detonation(d) => Self::inbound_detonation(d)?,
            Body::StartResume(s) if self.is_own(s.originating_entity) => {
                return Err(Skip::OwnControl);
            }
            Body::StopFreeze(s) if self.is_own(s.originating_entity) => {
                return Err(Skip::OwnControl);
            }
            Body::StartResume(_) => Inbound::Control(ControlAction::Start),
            Body::StopFreeze(s) => Inbound::Control(if s.reason == REASON_RECESS {
                ControlAction::Pause
            } else {
                ControlAction::Stop
            }),
        };
        if inbound.message().validate().is_err() {
            return Err(Skip::Invalid);
        }
        Ok(inbound)
    }

    fn inbound_state(&mut self, s: &PduEntityState) -> Result<Inbound, Skip> {
        if !valid_entity(s.entity_id.entity) {
            return Err(Skip::EntityId);
        }
        let key = pack_key(s.entity_id);
        if s.is_deactivated() {
            return Ok(Inbound::Removed(key));
        }
        let velocity = widen(s.linear_velocity);
        let acceleration = widen(s.dead_reckoning.linear_acceleration);
        let angular_velocity = widen(s.dead_reckoning.angular_velocity);
        let [yaw, pitch, roll] = widen(s.orientation);
        let all_finite = [
            &s.location,
            &velocity,
            &acceleration,
            &angular_velocity,
            &[yaw, pitch, roll],
        ]
        .into_iter()
        .all(finite3);
        if !all_finite {
            return Err(Skip::NotFinite);
        }
        let (dead_reckoning, reserved) = dead_reckoning_of(s.dead_reckoning.algorithm);
        if reserved {
            self.tally.reserved_dead_reckoning =
                self.tally.reserved_dead_reckoning.saturating_add(1);
        }
        Ok(Inbound::State(InboundState {
            key,
            entity_type: (!is_none_type(s.entity_type)).then(|| format_entity_type(s.entity_type)),
            force: self.force_names.get(&s.force_id).cloned(),
            name: name_of(&s.marking),
            position: s.location,
            velocity,
            acceleration,
            angular_velocity,
            orientation: Euler { yaw, pitch, roll },
            dead_reckoning,
        }))
    }

    fn inbound_fire(&self, f: &Fire) -> Result<Inbound, Skip> {
        if !valid_entity(f.firing_entity.entity) {
            return Err(Skip::EntityId);
        }
        if !finite3(&f.location) {
            return Err(Skip::NotFinite);
        }
        // Munition and expendable descriptors name the weapon by their entity type.
        let weapon = self
            .weapon_by_type
            .get(&descriptor_type(&f.descriptor))
            .ok_or(Skip::UnmappedWeapon)?;
        Ok(Inbound::Fire {
            shooter: pack_key(f.firing_entity),
            target: optional_key(f.target_entity),
            weapon: weapon.clone(),
            position: f.location,
        })
    }

    /// A Detonation of any descriptor form (munition, expendable, explosion) is a weapon effect at
    /// its location; the warhead, fuse and explosive fields have no canonical counterpart and are
    /// not read, so an explosion or expendable never yields a made-up warhead or fuse.
    fn inbound_detonation(d: &Detonation) -> Result<Inbound, Skip> {
        if !finite3(&d.location) {
            return Err(Skip::NotFinite);
        }
        Ok(Inbound::Detonation {
            shooter: optional_key(d.firing_entity),
            target: optional_key(d.target_entity),
            position: d.location,
        })
    }
}

/// The key of an optional entity reference: `None` for the reserved entity numbers.
fn optional_key(id: EntityId) -> Option<NetKey> {
    valid_entity(id.entity).then(|| pack_key(id))
}

fn descriptor_type(d: &Descriptor) -> EntityType {
    match d {
        Descriptor::Munition(m) => m.munition,
        Descriptor::Expendable { entity_type } | Descriptor::Explosion { entity_type, .. } => {
            *entity_type
        }
    }
}
