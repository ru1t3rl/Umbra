//! The Entity State PDU (type 1; IEEE 1278.1-2012 and 1278.1a-1998, same layout; `docs/net/dis.md`).
//!
//! Fixed part 144 bytes after which `n` articulation parameters of 16 bytes may follow. The codec
//! validates them by length and drops their content (unsupported, `docs/net/dis.md`); it writes none.

use super::{DecodeError, EntityId, EntityType, PDU_TYPE_ENTITY_STATE, reader::Reader};

/// Length of an Entity State PDU without articulation parameters.
pub const ENTITY_STATE_LEN: usize = 144;
/// Length of one articulation parameter.
const ARTICULATION_PARAMETER_LEN: usize = 16;
/// Appearance bit 23: the entity state is "deactivated" (SISO-REF-010, platform appearance).
pub const APPEARANCE_DEACTIVATED: u32 = 1 << 23;

/// The dead reckoning algorithm codes (SISO-REF-010); other codes are kept as [`Self::Reserved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadReckoningAlgorithm {
    /// 0: other.
    Other,
    /// 1: static.
    Static,
    /// 2: fixed position, world coordinates (constant velocity).
    Fpw,
    /// 3: rotating position, world.
    Rpw,
    /// 4: rotating velocity, world.
    Rvw,
    /// 5: fixed velocity, world.
    Fvw,
    /// 6: fixed position, body.
    Fpb,
    /// 7: rotating position, body.
    Rpb,
    /// 8: rotating velocity, body.
    Rvb,
    /// 9: fixed velocity, body.
    Fvb,
    /// 10..=255: not defined by the supported versions.
    Reserved(u8),
}

impl DeadReckoningAlgorithm {
    /// The algorithm a wire code names.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Other,
            1 => Self::Static,
            2 => Self::Fpw,
            3 => Self::Rpw,
            4 => Self::Rvw,
            5 => Self::Fvw,
            6 => Self::Fpb,
            7 => Self::Rpb,
            8 => Self::Rvb,
            9 => Self::Fvb,
            other => Self::Reserved(other),
        }
    }

    /// The wire code.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Other => 0,
            Self::Static => 1,
            Self::Fpw => 2,
            Self::Rpw => 3,
            Self::Rvw => 4,
            Self::Fvw => 5,
            Self::Fpb => 6,
            Self::Rpb => 7,
            Self::Rvb => 8,
            Self::Fvb => 9,
            Self::Reserved(code) => code,
        }
    }
}

/// The 40-byte dead reckoning parameters block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeadReckoningParameters {
    /// Algorithm code (see [`DeadReckoningAlgorithm`]); kept raw so every byte round-trips.
    pub algorithm: u8,
    /// The 15 "other parameters" bytes.
    pub other: [u8; 15],
    /// Linear acceleration (m/s^2).
    pub linear_acceleration: [f32; 3],
    /// Angular velocity about the body axes (rad/s).
    pub angular_velocity: [f32; 3],
}

/// The entity marking: a character set and 11 characters, NUL padded (12 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marking {
    /// Character set (1 = ASCII, 0 = unused).
    pub character_set: u8,
    /// The 11 characters as sent, including padding.
    pub characters: [u8; 11],
}

/// An Entity State PDU body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityState {
    /// The entity.
    pub entity_id: EntityId,
    /// Force ID (0 other, 1 friendly, 2 opposing, 3 neutral).
    pub force_id: u8,
    /// Entity type.
    pub entity_type: EntityType,
    /// Alternative entity type.
    pub alternative_entity_type: EntityType,
    /// Linear velocity in world (ECEF) axes (m/s).
    pub linear_velocity: [f32; 3],
    /// Location in ECEF (m).
    pub location: [f64; 3],
    /// Orientation psi, theta, phi (rad).
    pub orientation: [f32; 3],
    /// Appearance bits.
    pub appearance: u32,
    /// Dead reckoning parameters.
    pub dead_reckoning: DeadReckoningParameters,
    /// Marking.
    pub marking: Marking,
    /// Capabilities bits.
    pub capabilities: u32,
}

impl EntityState {
    /// True if the appearance has the "deactivated" bit.
    #[must_use]
    pub fn is_deactivated(&self) -> bool {
        self.appearance & APPEARANCE_DEACTIVATED != 0
    }

    /// Reads the body after the 12-byte header; `length` is the PDU length from the header.
    pub(super) fn read(r: &mut Reader<'_>, length: u16) -> Result<Self, DecodeError> {
        let bad_length = DecodeError::BadLength {
            pdu_type: PDU_TYPE_ENTITY_STATE,
            length,
        };
        if usize::from(length) < ENTITY_STATE_LEN {
            return Err(bad_length);
        }
        let entity_id = EntityId::read(r)?;
        let force_id = r.u8()?;
        let articulations = usize::from(r.u8()?);
        let entity_type = EntityType::read(r)?;
        let alternative_entity_type = EntityType::read(r)?;
        let linear_velocity = r.f32x3()?;
        let location = r.f64x3()?;
        let orientation = r.f32x3()?;
        let appearance = r.u32()?;
        let dead_reckoning = DeadReckoningParameters {
            algorithm: r.u8()?,
            other: r.bytes::<15>()?,
            linear_acceleration: r.f32x3()?,
            angular_velocity: r.f32x3()?,
        };
        let marking = Marking {
            character_set: r.u8()?,
            characters: r.bytes::<11>()?,
        };
        let capabilities = r.u32()?;
        let extra = articulations.checked_mul(ARTICULATION_PARAMETER_LEN);
        if extra != Some(r.remaining()) {
            return Err(bad_length);
        }
        Ok(EntityState {
            entity_id,
            force_id,
            entity_type,
            alternative_entity_type,
            linear_velocity,
            location,
            orientation,
            appearance,
            dead_reckoning,
            marking,
            capabilities,
        })
    }

    /// Appends the 132 body bytes (no articulation parameters).
    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.entity_id.write(out);
        out.push(self.force_id);
        out.push(0); // number of articulation parameters
        self.entity_type.write(out);
        self.alternative_entity_type.write(out);
        put_f32s(out, &self.linear_velocity);
        for v in self.location {
            out.extend_from_slice(&v.to_be_bytes());
        }
        put_f32s(out, &self.orientation);
        out.extend_from_slice(&self.appearance.to_be_bytes());
        out.push(self.dead_reckoning.algorithm);
        out.extend_from_slice(&self.dead_reckoning.other);
        put_f32s(out, &self.dead_reckoning.linear_acceleration);
        put_f32s(out, &self.dead_reckoning.angular_velocity);
        out.push(self.marking.character_set);
        out.extend_from_slice(&self.marking.characters);
        out.extend_from_slice(&self.capabilities.to_be_bytes());
    }
}

fn put_f32s(out: &mut Vec<u8>, values: &[f32; 3]) {
    for v in values {
        out.extend_from_slice(&v.to_be_bytes());
    }
}
