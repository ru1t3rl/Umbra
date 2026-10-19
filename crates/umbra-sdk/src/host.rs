//! The host API as the SDK holds it (REQ-P-5, ADR 0021, R-B4).
//!
//! The entry call and `init` validate the host's `UmbraHostApi` and store a **copy** of the fields the
//! SDK knows (`host_ctx` and the callbacks): every later use goes through that copy, so the SDK never
//! dereferences a host pointer outside the call that passed it (REQ-P-11). The first entry call of a
//! lifecycle (none stored: the first ever, or the first after a `shutdown`) stores the copy; later entry
//! calls of the same lifecycle keep it (ADR 0019, RR-M1); `init` always replaces it with its own `host`
//! (the same pointer the entry got), so a lifecycle that ended without `shutdown` (a failed `init`) is
//! reset there too; `shutdown` clears it, so [`log`] is a no-op outside a lifecycle. This copy is the
//! SDK's only global mutable state.
//!
//! Randomness reaches a plugin only through `rng_next_u64`/`rng_next_f64` with the batch item's own
//! token (REQ-K-4); the SDK has no RNG and no clock of its own.
#![expect(
    unsafe_code,
    reason = "reads the host API and calls its callbacks (ADR 0021)"
)]

use std::sync::{PoisonError, RwLock};

use crate::abi::{
    UMBRA_LOG_DEBUG, UMBRA_LOG_ERROR, UMBRA_LOG_INFO, UMBRA_LOG_TRACE, UMBRA_LOG_WARN, UMBRA_OK,
    UmbraCode, UmbraHostApi, UmbraLogLevel, UmbraToken,
};
use crate::behavior::Level;
use crate::convert::{checked_ref, umbra_str};

type LogFn = unsafe extern "C" fn(UmbraToken, UmbraLogLevel, crate::abi::UmbraStr) -> UmbraCode;
type U64Fn = unsafe extern "C" fn(UmbraToken, UmbraToken, *mut u64) -> UmbraCode;
type F64Fn = unsafe extern "C" fn(UmbraToken, UmbraToken, *mut f64) -> UmbraCode;

/// The minor-0 fields of the host API of the current lifecycle.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HostCopy {
    ctx: UmbraToken,
    log: Option<LogFn>,
    rng_u64: Option<U64Fn>,
    rng_f64: Option<F64Fn>,
}

/// The host API of the current lifecycle, if any.
static HOST: RwLock<Option<HostCopy>> = RwLock::new(None);

/// Validates `host` (non-null, aligned, header rule of ADR 0019) and copies its fields.
///
/// # Safety
/// `host` is null or points at a readable `UmbraHostApi` (the entry and `init` contract).
pub(crate) unsafe fn read(host: *const UmbraHostApi) -> Option<HostCopy> {
    // SAFETY: forwarded from this function's contract.
    let h = unsafe { checked_ref(host) }?;
    Some(HostCopy {
        ctx: h.host_ctx,
        log: h.log,
        rng_u64: h.rng_next_u64,
        rng_f64: h.rng_next_f64,
    })
}

/// Stores `host` if no host API is stored (the first entry call of a lifecycle).
pub(crate) fn store_if_empty(host: HostCopy) {
    let mut stored = HOST.write().unwrap_or_else(PoisonError::into_inner);
    if stored.is_none() {
        *stored = Some(host);
    }
}

/// Replaces (or with `None` clears) the stored host API.
pub(crate) fn store(host: Option<HostCopy>) {
    *HOST.write().unwrap_or_else(PoisonError::into_inner) = host;
}

/// The stored host API, if a lifecycle is active.
pub(crate) fn current() -> Option<HostCopy> {
    *HOST.read().unwrap_or_else(PoisonError::into_inner)
}

fn level_code(level: Level) -> UmbraLogLevel {
    match level {
        Level::Error => UMBRA_LOG_ERROR,
        Level::Warn => UMBRA_LOG_WARN,
        Level::Info => UMBRA_LOG_INFO,
        Level::Debug => UMBRA_LOG_DEBUG,
        Level::Trace => UMBRA_LOG_TRACE,
    }
}

/// Logs through the stored host API; a no-op without one (before entry, after `shutdown`) or when
/// the host has no `log` callback. The host's result code is ignored: logging cannot fail a call.
pub(crate) fn log(level: Level, message: &str) {
    let Some(HostCopy {
        ctx, log: Some(f), ..
    }) = current()
    else {
        return;
    };
    // SAFETY: `f` is the host's `log` callback of the current lifecycle (ADR 0021: valid for the
    // process lifetime, returns `UMBRA_ERR_STATE` outside a call); `message` is borrowed for the call
    // and copied by the host; host callbacks are `extern "C"` and cannot unwind.
    let _ = unsafe { f(ctx, level_code(level), umbra_str(message)) };
}

impl HostCopy {
    /// Draws a `u64` from the stream of `token`; `Err` is the host's code (or `UMBRA_ERR_STATE` when
    /// the host has no RNG callback).
    pub(crate) fn next_u64(&self, token: UmbraToken) -> Result<u64, UmbraCode> {
        let f = self.rng_u64.ok_or(crate::abi::UMBRA_ERR_STATE)?;
        let mut out = 0_u64;
        // SAFETY: `f` is the host's RNG callback of the current lifecycle; `out` is a live local the
        // host writes during the call; host callbacks cannot unwind.
        let code = unsafe { f(self.ctx, token, &raw mut out) };
        if code == UMBRA_OK { Ok(out) } else { Err(code) }
    }

    /// Draws an `f64` in `[0, 1)` from the stream of `token`; errors as [`HostCopy::next_u64`].
    pub(crate) fn next_f64(&self, token: UmbraToken) -> Result<f64, UmbraCode> {
        let f = self.rng_f64.ok_or(crate::abi::UMBRA_ERR_STATE)?;
        let mut out = 0.0_f64;
        // SAFETY: as in `next_u64`.
        let code = unsafe { f(self.ctx, token, &raw mut out) };
        if code == UMBRA_OK { Ok(out) } else { Err(code) }
    }
}
