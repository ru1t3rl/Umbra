//! Per-call services of the ABI 0.2 kinds: terrain and blackboard access (ADR 0050, REQ-P-4,
//! REQ-P-11).
//!
//! A service exists only in the calls that carry it (a [`Terrain`] in a motion step or a sense call,
//! a [`BlackboardAccess`] in a condition check or an action tick) and is valid only for that call
//! (the host resolves every token against the call record, ADR 0021 RR-M2). The types borrow the
//! call and cannot leave its thread, so a plugin cannot keep one.
//!
//! A refused service call returns the host's code as an [`Error`] and also makes the whole call
//! (`step`, `sense`, `check`, `tick`) return that code, whether or not the plugin looks at the error:
//! the host has faulted the capability, so a result built after the refusal is never reported.

use crate::abi::UMBRA_ERR_INVALID_ARGUMENT;
use crate::behavior::{Fault, Value};
use crate::convert::{BlackboardSvc, GetError, TerrainSvc};
use crate::error::Error;

/// A point or vector in ECEF meters (the SDK mirrors the host's frame as plain arrays, like
/// [`crate::EntityView`]).
pub type Vec3 = [f64; 3];

/// The error of a host refusal (or malformed host answer) `code` of `call`: the one text of every
/// refusal of the ABI 0.2 kinds, used for the plugin's `Err` and for the call's result.
pub(crate) fn refusal(code: i32, call: &str) -> Error {
    Error::with_code(code, format!("{call} (host code {code})"))
}

/// Records `code` of `call` in `fault` (the first refusal of the call wins) and returns it as an
/// error.
pub(crate) fn refused(fault: &Fault, code: i32, call: &str) -> Error {
    fault.record(code, call);
    refusal(code, call)
}

/// Terrain queries of the current call (ADR 0050). Answers come from the `TerrainProvider` the host's
/// core call received, so a plugin sees what a built-in sees (LSP).
#[derive(Debug)]
pub struct Terrain<'a> {
    svc: TerrainSvc,
    fault: &'a Fault,
}

impl<'a> Terrain<'a> {
    /// The terrain service of one call.
    pub(crate) fn new(svc: TerrainSvc, fault: &'a Fault) -> Self {
        Self { svc, fault }
    }
}

impl Terrain<'_> {
    /// Terrain height above the WGS-84 ellipsoid below `p` (the point's own height is ignored). The
    /// answer is passed on as the host gives it (a provider may answer a non-finite height).
    ///
    /// # Errors
    /// The host refused the query (a non-finite point, or a stale or foreign token), which faults the
    /// capability.
    pub fn height_at(&self, p: Vec3) -> Result<f64, Error> {
        self.svc
            .height_at(p)
            .map_err(|code| refused(self.fault, code, "terrain height_at refused"))
    }

    /// Unit surface normal at `p` in the local ENU frame, as `[east, north, up]`.
    ///
    /// # Errors
    /// The host refused the query.
    pub fn normal_at(&self, p: Vec3) -> Result<Vec3, Error> {
        self.svc
            .normal_at(p)
            .map_err(|code| refused(self.fault, code, "terrain normal_at refused"))
    }

    /// Whether `b` is visible from `a` (ECEF points, already raised to eye height).
    ///
    /// # Errors
    /// The host refused the query, or answered neither 0 nor 1 (`UMBRA_ERR_INVALID_ARGUMENT`).
    pub fn line_of_sight(&self, a: Vec3, b: Vec3) -> Result<bool, Error> {
        match self.svc.line_of_sight(a, b) {
            Ok(Some(visible)) => Ok(visible),
            Ok(None) => Err(refused(
                self.fault,
                UMBRA_ERR_INVALID_ARGUMENT,
                "terrain line_of_sight answered neither 0 nor 1",
            )),
            Err(code) => Err(refused(self.fault, code, "terrain line_of_sight refused")),
        }
    }
}

/// Blackboard access of a condition check or an action tick, restricted to the keys of the leaf's
/// declared blackboard params with their access (ADR 0050, REQ-B-15). Any other key, a wrong type
/// or a non-finite value is refused by the host and faults the capability.
#[derive(Debug)]
pub struct BlackboardAccess<'a> {
    svc: BlackboardSvc,
    fault: &'a Fault,
}

impl<'a> BlackboardAccess<'a> {
    /// The blackboard service of one call.
    pub(crate) fn new(svc: BlackboardSvc, fault: &'a Fault) -> Self {
        Self { svc, fault }
    }
}

impl BlackboardAccess<'_> {
    /// Reads `key`; `Ok(None)` when it is unset.
    ///
    /// # Errors
    /// The key is not granted for reading, or the host refused the read, or answered it with a
    /// malformed value (`UMBRA_ERR_INVALID_ARGUMENT`); the error names the key.
    pub fn get(&mut self, key: &str) -> Result<Option<Value>, Error> {
        self.svc.get(key).map_err(|e| match e {
            GetError::Refused(code) => refused(
                self.fault,
                code,
                &format!("blackboard get of `{key}` refused"),
            ),
            GetError::Malformed(why) => refused(
                self.fault,
                UMBRA_ERR_INVALID_ARGUMENT,
                &format!("blackboard get of `{key}`: {why}"),
            ),
        })
    }

    /// Writes `key`; `None` unsets it.
    ///
    /// # Errors
    /// The key is not granted for writing, the value has the wrong type or is not finite; the error
    /// names the key.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the plan's signature (ASM-666): an owned value, like `BehaviorInstance::set_input`"
    )]
    pub fn set(&mut self, key: &str, value: Option<Value>) -> Result<(), Error> {
        let code = self.svc.set(key, value.as_ref());
        if code == crate::abi::UMBRA_OK {
            Ok(())
        } else {
            Err(refused(
                self.fault,
                code,
                &format!("blackboard set of `{key}` refused"),
            ))
        }
    }
}
