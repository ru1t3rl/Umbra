//! The pure DIS codec: [`Pdu`], [`encode`], [`decode`] (ADR 0034, REQ-N-6, NFR-2).
//!
//! No I/O, no clock, no allocation beyond the output vector. Big-endian per IEEE 1278.1. Input is
//! untrusted: every read is bounds-checked, every length is checked before use, and the module
//! denies indexing, unchecked arithmetic, `unwrap` and `expect` (the lints below), so no datagram
//! can make [`decode`] panic. Families: Entity State (WI-13); Fire, Detonation, Start/Resume and
//! Stop/Freeze (WI-14).
//!
//! One datagram is one PDU: the header length must equal the datagram length ([`DecodeError`]
//! distinguishes truncated and extended datagrams from bad structure).

#![deny(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod common;
mod detonation;
mod entity_state;
mod fire;
mod reader;
mod simulation;

use std::collections::BTreeMap;

pub use common::{
    BurstDescriptor, ClockTime, Descriptor, DescriptorForm, EntityId, EntityType, EventId,
};
pub use detonation::{DETONATION_LEN, Detonation};
pub use entity_state::{
    APPEARANCE_DEACTIVATED, DeadReckoningAlgorithm, DeadReckoningParameters, ENTITY_STATE_LEN,
    EntityState, Marking,
};
pub use fire::{FIRE_LEN, Fire};
pub use simulation::{START_RESUME_LEN, STOP_FREEZE_LEN, StartResume, StopFreeze};

use crate::version::DisVersion;
use common::{detonation_form, fire_form};
use reader::Reader;

/// Length of the common PDU header, both versions.
pub const HEADER_LEN: usize = 12;
/// PDU type of the Entity State PDU.
pub const PDU_TYPE_ENTITY_STATE: u8 = 1;
/// PDU type of the Fire PDU.
pub const PDU_TYPE_FIRE: u8 = 2;
/// PDU type of the Detonation PDU.
pub const PDU_TYPE_DETONATION: u8 = 3;
/// PDU type of the Start/Resume PDU.
pub const PDU_TYPE_START_RESUME: u8 = 13;
/// PDU type of the Stop/Freeze PDU.
pub const PDU_TYPE_STOP_FREEZE: u8 = 14;
/// Protocol family of the Entity Information/Interaction PDUs.
pub const FAMILY_ENTITY_INFORMATION: u8 = 1;
/// Protocol family of the Warfare PDUs.
pub const FAMILY_WARFARE: u8 = 2;
/// Protocol family of the Simulation Management PDUs.
pub const FAMILY_SIMULATION_MANAGEMENT: u8 = 5;

/// A supported PDU type with the protocol family it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PduKind {
    pdu_type: u8,
    family: u8,
}

/// Entity State.
const ENTITY_STATE: PduKind = PduKind {
    pdu_type: PDU_TYPE_ENTITY_STATE,
    family: FAMILY_ENTITY_INFORMATION,
};

/// Fire.
const FIRE: PduKind = PduKind {
    pdu_type: PDU_TYPE_FIRE,
    family: FAMILY_WARFARE,
};

/// Detonation.
const DETONATION: PduKind = PduKind {
    pdu_type: PDU_TYPE_DETONATION,
    family: FAMILY_WARFARE,
};

/// Start/Resume.
const START_RESUME: PduKind = PduKind {
    pdu_type: PDU_TYPE_START_RESUME,
    family: FAMILY_SIMULATION_MANAGEMENT,
};

/// Stop/Freeze.
const STOP_FREEZE: PduKind = PduKind {
    pdu_type: PDU_TYPE_STOP_FREEZE,
    family: FAMILY_SIMULATION_MANAGEMENT,
};

/// Every supported PDU type: the one table decoding looks types up in.
const KINDS: &[PduKind] = &[ENTITY_STATE, FIRE, DETONATION, START_RESUME, STOP_FREEZE];

/// Offset of the length field in the header.
const LENGTH_OFFSET: usize = 8;

/// The fields of the common header that are not implied by the body or the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    /// Exercise ID (byte 1).
    pub exercise_id: u8,
    /// Raw timestamp (bytes 4..8); the adapter writes relative timestamps (ASM-237).
    pub timestamp: u32,
    /// PDU status (v7 byte 10). Always 0 for v6, which has no such field.
    pub status: u8,
}

/// The body of a PDU, one variant per supported family.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// Entity State PDU (type 1).
    EntityState(EntityState),
    /// Fire PDU (type 2).
    Fire(Fire),
    /// Detonation PDU (type 3).
    Detonation(Detonation),
    /// Start/Resume PDU (type 13).
    StartResume(StartResume),
    /// Stop/Freeze PDU (type 14).
    StopFreeze(StopFreeze),
}

impl Body {
    /// The PDU type and protocol family of this body (an entry of [`KINDS`]).
    fn kind(&self) -> PduKind {
        match self {
            Body::EntityState(_) => ENTITY_STATE,
            Body::Fire(_) => FIRE,
            Body::Detonation(_) => DETONATION,
            Body::StartResume(_) => START_RESUME,
            Body::StopFreeze(_) => STOP_FREEZE,
        }
    }

    /// Reads the body of a PDU of type `pdu_type` after the header.
    fn read(
        pdu_type: u8,
        r: &mut Reader<'_>,
        length: u16,
        status: u8,
    ) -> Result<Self, DecodeError> {
        match pdu_type {
            PDU_TYPE_ENTITY_STATE => Ok(Body::EntityState(EntityState::read(r, length)?)),
            PDU_TYPE_FIRE => Ok(Body::Fire(Fire::read(r, length, status)?)),
            PDU_TYPE_DETONATION => Ok(Body::Detonation(Detonation::read(r, length, status)?)),
            PDU_TYPE_START_RESUME => Ok(Body::StartResume(StartResume::read(r, length)?)),
            PDU_TYPE_STOP_FREEZE => Ok(Body::StopFreeze(StopFreeze::read(r, length)?)),
            other => Err(DecodeError::UnsupportedPduType(other)),
        }
    }
}

/// A decoded or to-be-encoded PDU, independent of the protocol version it travels in.
#[derive(Debug, Clone, PartialEq)]
pub struct Pdu {
    /// Header fields.
    pub header: Header,
    /// Body.
    pub body: Body,
}

/// Why a datagram is not a PDU this codec can read. Never a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DecodeError {
    /// The datagram ends before a field that must be there.
    #[error("datagram truncated: needs {needed} bytes, has {got}")]
    Truncated {
        /// Bytes needed to read the field.
        needed: usize,
        /// Bytes in the datagram.
        got: usize,
    },
    /// The protocol version byte is neither 6 nor 7.
    #[error("unsupported DIS protocol version {0}")]
    UnsupportedVersion(u8),
    /// A PDU type outside the supported families; the transport skips and counts it.
    #[error("unsupported PDU type {0}")]
    UnsupportedPduType(u8),
    /// The protocol family byte does not belong to the PDU type.
    #[error("PDU type {pdu_type} cannot be in protocol family {family}")]
    WrongFamily {
        /// The PDU type byte.
        pdu_type: u8,
        /// The family byte.
        family: u8,
    },
    /// The header length differs from the datagram length (truncated or extended datagram).
    #[error("header declares {declared} bytes but the datagram has {actual}")]
    LengthMismatch {
        /// Length field of the header.
        declared: u16,
        /// Datagram length.
        actual: usize,
    },
    /// The v7 status bits name a reserved descriptor form (Detonation Type Indicator 3).
    #[error("PDU type {pdu_type} has the reserved descriptor indicator {indicator}")]
    ReservedDescriptorForm {
        /// The PDU type byte.
        pdu_type: u8,
        /// The indicator value.
        indicator: u8,
    },
    /// The length is consistent with the datagram but not valid for the PDU type.
    #[error("PDU type {pdu_type} cannot have length {length}")]
    BadLength {
        /// The PDU type byte.
        pdu_type: u8,
        /// The length field.
        length: u16,
    },
}

/// Why a PDU cannot be encoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EncodeError {
    /// The PDU is longer than the 16-bit length field can state.
    #[error("PDU of {0} bytes does not fit the 16-bit length field")]
    TooLong(usize),
    /// A non-munition descriptor cannot be sent in v6, which has only the munition form.
    #[error("PDU type {pdu_type} has a non-munition descriptor, which needs DIS v7")]
    DescriptorNeedsV7 {
        /// The PDU type byte.
        pdu_type: u8,
    },
    /// The descriptor form disagrees with the type indicator bits of the PDU status.
    #[error("PDU type {pdu_type}: status {status:#04x} does not name the descriptor form")]
    DescriptorStatusMismatch {
        /// The PDU type byte.
        pdu_type: u8,
        /// The PDU status.
        status: u8,
    },
}

/// What happened to the datagrams an adapter instance received (WI-15 owns one per instance, ASM-410,
/// so nothing is global). Counts decoded PDUs and why the rest was dropped; PDU types this codec does
/// not know are skipped and counted per type (at most 256 keys).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecodeCounters {
    /// Datagrams decoded into a PDU.
    pub decoded: u64,
    /// The skipped datagrams by PDU type byte.
    pub skipped_by_type: BTreeMap<u8, u64>,
    /// Datagrams of an unsupported protocol version.
    pub unsupported_version: u64,
    /// Datagrams that are malformed in any other way (truncated, wrong length or family).
    pub malformed: u64,
}

impl DecodeCounters {
    /// Datagrams of an unsupported PDU type that were skipped (the sum of `skipped_by_type`).
    #[must_use]
    pub fn skipped_unsupported_type(&self) -> u64 {
        self.skipped_by_type
            .values()
            .fold(0, |sum, n| u64::saturating_add(sum, *n))
    }

    /// Decodes `datagram`, counts the outcome and returns the PDU if there is one.
    pub fn decode(&mut self, datagram: &[u8]) -> Option<(DisVersion, Pdu)> {
        match decode(datagram) {
            Ok(pdu) => {
                self.decoded = self.decoded.saturating_add(1);
                Some(pdu)
            }
            Err(DecodeError::UnsupportedPduType(t)) => {
                let n = self.skipped_by_type.entry(t).or_insert(0);
                *n = n.saturating_add(1);
                None
            }
            Err(DecodeError::UnsupportedVersion(_)) => {
                self.unsupported_version = self.unsupported_version.saturating_add(1);
                None
            }
            Err(
                DecodeError::Truncated { .. }
                | DecodeError::WrongFamily { .. }
                | DecodeError::LengthMismatch { .. }
                | DecodeError::ReservedDescriptorForm { .. }
                | DecodeError::BadLength { .. },
            ) => {
                self.malformed = self.malformed.saturating_add(1);
                None
            }
        }
    }
}

/// Appends the datagram for `pdu` in `version` to `out`. On error `out` is left as it was.
///
/// # Errors
/// [`EncodeError::TooLong`] if the PDU exceeds 65535 bytes (not reachable with the families so far);
/// [`EncodeError::DescriptorNeedsV7`] or [`EncodeError::DescriptorStatusMismatch`] if a Fire or
/// Detonation descriptor form cannot be sent in `version` with the PDU status of `pdu`.
pub fn encode(pdu: &Pdu, version: DisVersion, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    check_descriptor(pdu, version)?;
    let start = out.len();
    let kind = pdu.body.kind();
    let strategy = version.header();
    out.push(strategy.version().protocol_version());
    out.push(pdu.header.exercise_id);
    out.push(kind.pdu_type);
    out.push(kind.family);
    out.extend_from_slice(&pdu.header.timestamp.to_be_bytes());
    out.extend_from_slice(&[0, 0]); // length, patched below
    out.extend_from_slice(&strategy.encode_status(pdu.header.status));
    match &pdu.body {
        Body::EntityState(b) => b.write(out),
        Body::Fire(b) => b.write(out),
        Body::Detonation(b) => b.write(out),
        Body::StartResume(b) => b.write(out),
        Body::StopFreeze(b) => b.write(out),
    }
    patch_length(out, start)
}

/// Checks that the descriptor form of a Fire or Detonation can be sent in `version` with the PDU
/// status of `pdu`: v6 has only the munition form, v7 names the form in the status bits.
fn check_descriptor(pdu: &Pdu, version: DisVersion) -> Result<(), EncodeError> {
    let status = pdu.header.status;
    let (pdu_type, form, named) = match &pdu.body {
        Body::Fire(f) => (PDU_TYPE_FIRE, f.descriptor.form(), Some(fire_form(status))),
        Body::Detonation(d) => (
            PDU_TYPE_DETONATION,
            d.descriptor.form(),
            detonation_form(status, PDU_TYPE_DETONATION).ok(),
        ),
        _ => return Ok(()),
    };
    match version {
        DisVersion::V6 if form != DescriptorForm::Munition => {
            Err(EncodeError::DescriptorNeedsV7 { pdu_type })
        }
        DisVersion::V7 if named != Some(form) => {
            Err(EncodeError::DescriptorStatusMismatch { pdu_type, status })
        }
        _ => Ok(()),
    }
}

/// Writes the PDU length (everything appended since `start`) into the header, or rolls back.
fn patch_length(out: &mut Vec<u8>, start: usize) -> Result<(), EncodeError> {
    let length = out.len().saturating_sub(start);
    let Ok(field) = u16::try_from(length) else {
        out.truncate(start);
        return Err(EncodeError::TooLong(length));
    };
    let from = start.saturating_add(LENGTH_OFFSET);
    if let Some(slot) = out.get_mut(from..from.saturating_add(2)) {
        slot.copy_from_slice(&field.to_be_bytes());
    }
    Ok(())
}

/// Decodes one datagram. Accepts v6 and v7 alike and reports which one it was (ASM-249).
///
/// # Errors
/// A [`DecodeError`] for anything that is not exactly one supported PDU.
pub fn decode(datagram: &[u8]) -> Result<(DisVersion, Pdu), DecodeError> {
    let mut r = Reader::new(datagram);
    let version_byte = r.u8()?;
    let exercise_id = r.u8()?;
    let pdu_type = r.u8()?;
    let family = r.u8()?;
    let timestamp = r.u32()?;
    let length = r.u16()?;
    let status_bytes = r.bytes::<2>()?;

    let version = DisVersion::from_protocol_version(version_byte)
        .ok_or(DecodeError::UnsupportedVersion(version_byte))?;
    let kind = KINDS
        .iter()
        .find(|k| k.pdu_type == pdu_type)
        .ok_or(DecodeError::UnsupportedPduType(pdu_type))?;
    if family != kind.family {
        return Err(DecodeError::WrongFamily { pdu_type, family });
    }
    if usize::from(length) != datagram.len() {
        return Err(DecodeError::LengthMismatch {
            declared: length,
            actual: datagram.len(),
        });
    }
    let header = Header {
        exercise_id,
        timestamp,
        status: version.header().decode_status(status_bytes),
    };
    let body = Body::read(pdu_type, &mut r, length, header.status)?;
    Ok((version, Pdu { header, body }))
}

#[cfg(test)]
mod tests {
    use super::patch_length;

    /// The 16-bit length field cannot state 65536 bytes: the encoder refuses and rolls back.
    #[test]
    fn patch_length_refuses_a_pdu_over_65535_bytes_and_rolls_back() {
        let mut out = vec![9, 9];
        out.extend(std::iter::repeat_n(0, 65_536));
        assert_eq!(
            patch_length(&mut out, 2),
            Err(super::EncodeError::TooLong(65_536))
        );
        assert_eq!(out, vec![9, 9]);
    }

    /// The largest length that fits is written big-endian at bytes 8..10 of the PDU.
    #[test]
    fn patch_length_writes_the_length_at_offset_8() {
        let mut out = vec![0; 65_535];
        patch_length(&mut out, 0).unwrap();
        assert_eq!(out.get(8..10), Some(&[0xFF, 0xFF][..]));
    }
}
