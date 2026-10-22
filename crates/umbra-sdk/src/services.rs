//! Per-call services of the ABI 0.2 kinds: terrain and blackboard access (ADR 0050, REQ-P-4,
//! REQ-P-11).
//!
//! A service exists only in the calls that carry it (a [`Terrain`] in a motion step or a sense call,
//! a [`BlackboardAccess`] in a condition check or an action tick) and is valid only for that call
//! (the host resolves every token against the call record, ADR 0021 RR-M2). The types borrow the
//! call, so a plugin cannot keep one.
//!
//! Status: M7 WI-2 stub. The signatures follow `docs/plans/M7.md` (section "SDK"); the behavior is
//! implemented in M7 WI-10 and the acceptance tests ignored until then.

use std::marker::PhantomData;

use crate::behavior::Value;
use crate::error::Error;

/// A point or vector in ECEF meters (the SDK mirrors the host's frame as plain arrays, like
/// [`crate::EntityView`]).
pub type Vec3 = [f64; 3];

/// Terrain queries of the current call (ADR 0050). Answers come from the `TerrainProvider` the host's
/// core call received, so a plugin sees what a built-in sees (LSP).
#[derive(Debug)]
pub struct Terrain<'a> {
    call: PhantomData<&'a ()>,
}

impl Terrain<'_> {
    /// Terrain height above the WGS-84 ellipsoid below `p` (the point's own height is ignored).
    ///
    /// # Errors
    /// The host refused the query (a stale or foreign token faults the capability).
    pub fn height_at(&self, p: Vec3) -> Result<f64, Error> {
        let _ = (&self.call, p);
        unimplemented!("M7 WI-10")
    }

    /// Unit surface normal at `p` in the local ENU frame.
    ///
    /// # Errors
    /// The host refused the query.
    pub fn normal_at(&self, p: Vec3) -> Result<Vec3, Error> {
        let _ = (&self.call, p);
        unimplemented!("M7 WI-10")
    }

    /// Whether `b` is visible from `a` (ECEF points, already raised to eye height).
    ///
    /// # Errors
    /// The host refused the query.
    pub fn line_of_sight(&self, a: Vec3, b: Vec3) -> Result<bool, Error> {
        let _ = (&self.call, a, b);
        unimplemented!("M7 WI-10")
    }
}

/// Blackboard access of a condition check or an action tick, restricted to the keys of the leaf's
/// declared blackboard params with their access (ADR 0050, REQ-B-15). Any other key, a wrong type
/// or a non-finite value is refused by the host and faults the capability.
#[derive(Debug)]
pub struct BlackboardAccess<'a> {
    call: PhantomData<&'a ()>,
}

impl BlackboardAccess<'_> {
    /// Reads `key`; `Ok(None)` when it is unset.
    ///
    /// # Errors
    /// The key is not granted for reading, or the host refused the read.
    pub fn get(&mut self, key: &str) -> Result<Option<Value>, Error> {
        let _ = (&self.call, key);
        unimplemented!("M7 WI-10")
    }

    /// Writes `key`; `None` unsets it.
    ///
    /// # Errors
    /// The key is not granted for writing, the value has the wrong type or is not finite.
    pub fn set(&mut self, key: &str, value: Option<Value>) -> Result<(), Error> {
        let _ = (&self.call, key, value);
        unimplemented!("M7 WI-10")
    }
}
