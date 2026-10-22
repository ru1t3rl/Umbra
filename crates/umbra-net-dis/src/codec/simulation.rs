//! Simulation management PDUs: Start/Resume (type 13) and Stop/Freeze (type 14), family 5; identical
//! in v6 and v7 (`docs/net/dis.md`).

use super::{
    ClockTime, DecodeError, EntityId, PDU_TYPE_START_RESUME, PDU_TYPE_STOP_FREEZE,
    common::exact_len, reader::Reader,
};

/// Length of a Start/Resume PDU.
pub const START_RESUME_LEN: usize = 44;
/// Length of a Stop/Freeze PDU.
pub const STOP_FREEZE_LEN: usize = 40;

/// A Start/Resume PDU body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartResume {
    /// The sender.
    pub originating_entity: EntityId,
    /// The addressee.
    pub receiving_entity: EntityId,
    /// Real-world time at which to start or resume.
    pub real_world_time: ClockTime,
    /// Simulation time at which to start or resume.
    pub simulation_time: ClockTime,
    /// Request identifier.
    pub request_id: u32,
}

/// A Stop/Freeze PDU body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StopFreeze {
    /// The sender.
    pub originating_entity: EntityId,
    /// The addressee.
    pub receiving_entity: EntityId,
    /// Real-world time at which to stop or freeze.
    pub real_world_time: ClockTime,
    /// Reason code (SISO-REF-010; 2 recess, 3 termination).
    pub reason: u8,
    /// Frozen behavior bit field.
    pub frozen_behavior: u8,
    /// Request identifier.
    pub request_id: u32,
}

impl StartResume {
    pub(super) fn read(r: &mut Reader<'_>, length: u16) -> Result<Self, DecodeError> {
        exact_len(length, START_RESUME_LEN, PDU_TYPE_START_RESUME)?;
        Ok(StartResume {
            originating_entity: EntityId::read(r)?,
            receiving_entity: EntityId::read(r)?,
            real_world_time: ClockTime::read(r)?,
            simulation_time: ClockTime::read(r)?,
            request_id: r.u32()?,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.originating_entity.write(out);
        self.receiving_entity.write(out);
        self.real_world_time.write(out);
        self.simulation_time.write(out);
        out.extend_from_slice(&self.request_id.to_be_bytes());
    }
}

impl StopFreeze {
    pub(super) fn read(r: &mut Reader<'_>, length: u16) -> Result<Self, DecodeError> {
        exact_len(length, STOP_FREEZE_LEN, PDU_TYPE_STOP_FREEZE)?;
        let originating_entity = EntityId::read(r)?;
        let receiving_entity = EntityId::read(r)?;
        let real_world_time = ClockTime::read(r)?;
        let reason = r.u8()?;
        let frozen_behavior = r.u8()?;
        let _padding = r.u16()?;
        Ok(StopFreeze {
            originating_entity,
            receiving_entity,
            real_world_time,
            reason,
            frozen_behavior,
            request_id: r.u32()?,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.originating_entity.write(out);
        self.receiving_entity.write(out);
        self.real_world_time.write(out);
        out.push(self.reason);
        out.push(self.frozen_behavior);
        out.extend_from_slice(&[0, 0]); // padding
        out.extend_from_slice(&self.request_id.to_be_bytes());
    }
}
