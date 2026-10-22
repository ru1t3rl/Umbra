//! Building blocks shared by several PDU families.

use super::{DecodeError, reader::Reader};

/// A DIS entity identifier: site, application, entity (6 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EntityId {
    /// Site number.
    pub site: u16,
    /// Application number.
    pub application: u16,
    /// Entity number.
    pub entity: u16,
}

impl EntityId {
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(EntityId {
            site: r.u16()?,
            application: r.u16()?,
            entity: r.u16()?,
        })
    }

    pub(super) fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.site.to_be_bytes());
        out.extend_from_slice(&self.application.to_be_bytes());
        out.extend_from_slice(&self.entity.to_be_bytes());
    }
}

/// A DIS entity type (8 bytes); all zero means "no type".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EntityType {
    /// Kind (platform, munition, ...).
    pub kind: u8,
    /// Domain (land, air, surface, ...).
    pub domain: u8,
    /// Country code.
    pub country: u16,
    /// Category.
    pub category: u8,
    /// Subcategory.
    pub subcategory: u8,
    /// Specific.
    pub specific: u8,
    /// Extra.
    pub extra: u8,
}

impl EntityType {
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(EntityType {
            kind: r.u8()?,
            domain: r.u8()?,
            country: r.u16()?,
            category: r.u8()?,
            subcategory: r.u8()?,
            specific: r.u8()?,
            extra: r.u8()?,
        })
    }

    pub(super) fn write(self, out: &mut Vec<u8>) {
        out.push(self.kind);
        out.push(self.domain);
        out.extend_from_slice(&self.country.to_be_bytes());
        out.push(self.category);
        out.push(self.subcategory);
        out.push(self.specific);
        out.push(self.extra);
    }
}

/// An event identifier: the issuing site and application and an event number (6 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventId {
    /// Site number of the issuer.
    pub site: u16,
    /// Application number of the issuer.
    pub application: u16,
    /// Event number.
    pub event: u16,
}

impl EventId {
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(EventId {
            site: r.u16()?,
            application: r.u16()?,
            event: r.u16()?,
        })
    }

    pub(super) fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.site.to_be_bytes());
        out.extend_from_slice(&self.application.to_be_bytes());
        out.extend_from_slice(&self.event.to_be_bytes());
    }
}

/// The munition form of the 16-byte descriptor of Fire and Detonation: the munition's entity type
/// and the warhead, fuse, quantity and rate codes (v6 has only this form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BurstDescriptor {
    /// Munition entity type.
    pub munition: EntityType,
    /// Warhead code.
    pub warhead: u16,
    /// Fuse code.
    pub fuse: u16,
    /// Quantity of munitions.
    pub quantity: u16,
    /// Rate of fire.
    pub rate: u16,
}

/// Which form a descriptor has; in v7 the PDU status bits say (Fire: bit 4, Fire Type Indicator;
/// Detonation: bits 4-5, Detonation Type Indicator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorForm {
    /// Indicator 0: munition.
    Munition,
    /// Indicator 1: expendable.
    Expendable,
    /// Detonation indicator 2: non-munition explosion.
    Explosion,
}

/// The 16-byte descriptor of Fire and Detonation (IEEE 1278.1-2012; v6 has only the munition form).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Descriptor {
    /// Munition: entity type, warhead, fuse, quantity, rate.
    Munition(BurstDescriptor),
    /// Expendable: entity type and 8 bytes of padding (written as zero, ignored on decode).
    Expendable {
        /// Expendable entity type.
        entity_type: EntityType,
    },
    /// Non-munition explosion (Detonation only): entity type, explosive material, 2 bytes of padding,
    /// explosive force.
    Explosion {
        /// Entity type of the explosion.
        entity_type: EntityType,
        /// Explosive material code.
        explosive_material: u16,
        /// Explosive force (kg of TNT equivalent).
        explosive_force: f32,
    },
}

impl Default for Descriptor {
    fn default() -> Self {
        Descriptor::Munition(BurstDescriptor::default())
    }
}

impl Descriptor {
    /// The form of this descriptor.
    #[must_use]
    pub fn form(&self) -> DescriptorForm {
        match self {
            Descriptor::Munition(_) => DescriptorForm::Munition,
            Descriptor::Expendable { .. } => DescriptorForm::Expendable,
            Descriptor::Explosion { .. } => DescriptorForm::Explosion,
        }
    }

    pub(super) fn read(r: &mut Reader<'_>, form: DescriptorForm) -> Result<Self, DecodeError> {
        let entity_type = EntityType::read(r)?;
        Ok(match form {
            DescriptorForm::Munition => Descriptor::Munition(BurstDescriptor {
                munition: entity_type,
                warhead: r.u16()?,
                fuse: r.u16()?,
                quantity: r.u16()?,
                rate: r.u16()?,
            }),
            DescriptorForm::Expendable => {
                let _padding = r.bytes::<8>()?;
                Descriptor::Expendable { entity_type }
            }
            DescriptorForm::Explosion => {
                let explosive_material = r.u16()?;
                let _padding = r.u16()?;
                Descriptor::Explosion {
                    entity_type,
                    explosive_material,
                    explosive_force: r.f32()?,
                }
            }
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        match self {
            Descriptor::Munition(m) => {
                m.munition.write(out);
                out.extend_from_slice(&m.warhead.to_be_bytes());
                out.extend_from_slice(&m.fuse.to_be_bytes());
                out.extend_from_slice(&m.quantity.to_be_bytes());
                out.extend_from_slice(&m.rate.to_be_bytes());
            }
            Descriptor::Expendable { entity_type } => {
                entity_type.write(out);
                out.extend_from_slice(&[0; 8]);
            }
            Descriptor::Explosion {
                entity_type,
                explosive_material,
                explosive_force,
            } => {
                entity_type.write(out);
                out.extend_from_slice(&explosive_material.to_be_bytes());
                out.extend_from_slice(&[0, 0]);
                out.extend_from_slice(&explosive_force.to_be_bytes());
            }
        }
    }
}

/// The Fire Type Indicator of v7 status byte `status` (bit 4): the form of a Fire descriptor.
pub(super) fn fire_form(status: u8) -> DescriptorForm {
    if status & 0x10 == 0 {
        DescriptorForm::Munition
    } else {
        DescriptorForm::Expendable
    }
}

/// The Detonation Type Indicator of v7 status byte `status` (bits 4-5).
pub(super) fn detonation_form(status: u8, pdu_type: u8) -> Result<DescriptorForm, DecodeError> {
    match (status >> 4) & 0b11 {
        0 => Ok(DescriptorForm::Munition),
        1 => Ok(DescriptorForm::Expendable),
        2 => Ok(DescriptorForm::Explosion),
        indicator => Err(DecodeError::ReservedDescriptorForm {
            pdu_type,
            indicator,
        }),
    }
}

/// `length` must be exactly `expected` bytes for a fixed-length PDU of type `pdu_type`.
pub(super) fn exact_len(length: u16, expected: usize, pdu_type: u8) -> Result<(), DecodeError> {
    if usize::from(length) == expected {
        Ok(())
    } else {
        Err(DecodeError::BadLength { pdu_type, length })
    }
}

/// A clock time: whole hours (signed) and the time past the hour (8 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClockTime {
    /// Hours since the epoch of the clock.
    pub hour: i32,
    /// Time past the hour, in units of 3600/2^31 seconds.
    pub time_past_hour: u32,
}

impl ClockTime {
    pub(super) fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(ClockTime {
            hour: r.i32()?,
            time_past_hour: r.u32()?,
        })
    }

    pub(super) fn write(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hour.to_be_bytes());
        out.extend_from_slice(&self.time_past_hour.to_be_bytes());
    }
}

/// Writes three f32 (12 bytes).
pub(super) fn put_f32s(out: &mut Vec<u8>, values: &[f32; 3]) {
    for v in values {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

/// Writes three f64 (24 bytes).
pub(super) fn put_f64s(out: &mut Vec<u8>, values: &[f64; 3]) {
    for v in values {
        out.extend_from_slice(&v.to_be_bytes());
    }
}
