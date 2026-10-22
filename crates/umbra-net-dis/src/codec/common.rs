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
