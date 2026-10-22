//! The Fire PDU (type 2, family 2 Warfare; identical in v6 and v7; `docs/net/dis.md`), 96 bytes.

use super::{
    DecodeError, Descriptor, EntityId, EventId, PDU_TYPE_FIRE,
    common::{exact_len, fire_form, put_f32s, put_f64s},
    reader::Reader,
};

/// Length of a Fire PDU.
pub const FIRE_LEN: usize = 96;

/// A Fire PDU body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fire {
    /// The firing entity.
    pub firing_entity: EntityId,
    /// The target entity (all zero if none).
    pub target_entity: EntityId,
    /// The munition (or expendable) entity.
    pub munition: EntityId,
    /// The event, matched by a later Detonation.
    pub event: EventId,
    /// Fire mission index.
    pub fire_mission_index: u32,
    /// Launch point in ECEF (m).
    pub location: [f64; 3],
    /// Descriptor (v7: munition or expendable, as the status bit 4 says; v6: munition).
    pub descriptor: Descriptor,
    /// Initial velocity in world axes (m/s).
    pub velocity: [f32; 3],
    /// Range to the target (m).
    pub range: f32,
}

impl Fire {
    /// `status` is the decoded PDU status (0 for v6).
    pub(super) fn read(r: &mut Reader<'_>, length: u16, status: u8) -> Result<Self, DecodeError> {
        exact_len(length, FIRE_LEN, PDU_TYPE_FIRE)?;
        Ok(Fire {
            firing_entity: EntityId::read(r)?,
            target_entity: EntityId::read(r)?,
            munition: EntityId::read(r)?,
            event: EventId::read(r)?,
            fire_mission_index: r.u32()?,
            location: r.f64x3()?,
            descriptor: Descriptor::read(r, fire_form(status))?,
            velocity: r.f32x3()?,
            range: r.f32()?,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.firing_entity.write(out);
        self.target_entity.write(out);
        self.munition.write(out);
        self.event.write(out);
        out.extend_from_slice(&self.fire_mission_index.to_be_bytes());
        put_f64s(out, &self.location);
        self.descriptor.write(out);
        put_f32s(out, &self.velocity);
        out.extend_from_slice(&self.range.to_be_bytes());
    }
}
