//! The Detonation PDU (type 3, family 2 Warfare; identical in v6 and v7; `docs/net/dis.md`),
//! 104 bytes plus 16 per articulation (v7: variable) parameter, which are validated by length and
//! dropped; the encoder writes none.

use super::{
    DecodeError, Descriptor, EntityId, EventId, PDU_TYPE_DETONATION,
    common::{detonation_form, put_f32s, put_f64s},
    reader::Reader,
};

/// Length of a Detonation PDU without parameter records.
pub const DETONATION_LEN: usize = 104;
/// Length of one parameter record.
const PARAMETER_LEN: usize = 16;

/// A Detonation PDU body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Detonation {
    /// The firing entity.
    pub firing_entity: EntityId,
    /// The target entity (all zero if none).
    pub target_entity: EntityId,
    /// The munition (v7: "exploding entity").
    pub munition: EntityId,
    /// The event, matching the Fire PDU.
    pub event: EventId,
    /// Velocity of the munition in world axes (m/s).
    pub velocity: [f32; 3],
    /// Burst point in ECEF (m).
    pub location: [f64; 3],
    /// Descriptor (v7: munition, expendable or explosion, as the status bits 4-5 say; v6: munition).
    pub descriptor: Descriptor,
    /// Burst point in the target entity's coordinates (m).
    pub location_in_entity: [f32; 3],
    /// Detonation result code (SISO-REF-010; 5 is "detonation").
    pub result: u8,
}

impl Detonation {
    /// `status` is the decoded PDU status (0 for v6).
    pub(super) fn read(r: &mut Reader<'_>, length: u16, status: u8) -> Result<Self, DecodeError> {
        let bad_length = DecodeError::BadLength {
            pdu_type: PDU_TYPE_DETONATION,
            length,
        };
        if usize::from(length) < DETONATION_LEN {
            return Err(bad_length);
        }
        let firing_entity = EntityId::read(r)?;
        let target_entity = EntityId::read(r)?;
        let munition = EntityId::read(r)?;
        let event = EventId::read(r)?;
        let velocity = r.f32x3()?;
        let location = r.f64x3()?;
        let descriptor = Descriptor::read(r, detonation_form(status, PDU_TYPE_DETONATION)?)?;
        let location_in_entity = r.f32x3()?;
        let result = r.u8()?;
        let records = usize::from(r.u8()?);
        let _padding = r.u16()?;
        if records.checked_mul(PARAMETER_LEN) != Some(r.remaining()) {
            return Err(bad_length);
        }
        Ok(Detonation {
            firing_entity,
            target_entity,
            munition,
            event,
            velocity,
            location,
            descriptor,
            location_in_entity,
            result,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.firing_entity.write(out);
        self.target_entity.write(out);
        self.munition.write(out);
        self.event.write(out);
        put_f32s(out, &self.velocity);
        put_f64s(out, &self.location);
        self.descriptor.write(out);
        put_f32s(out, &self.location_in_entity);
        out.push(self.result);
        out.push(0); // number of parameter records
        out.extend_from_slice(&[0, 0]); // padding
    }
}
