//! STUB (WI-7): the M1 intent subset (REQ-B-2, ASM-030).
#![expect(missing_docs, reason = "stub until WI-7: documented when implemented")]

use crate::coords::Ecef;
use crate::units::MetersPerSecond;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    MoveTo { destination: Ecef },
    SetSpeed(MetersPerSecond),
    Hold,
}
