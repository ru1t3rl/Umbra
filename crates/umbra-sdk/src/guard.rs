//! Panic guards and the error sink (REQ-P-7, ADR 0019 "Panics and unwinding", R-M1).
//!
//! Every exported function of the SDK runs its body inside [`catch`]: a panic becomes a message plus
//! `UMBRA_ERR_PANIC` and never unwinds across the C boundary. The panic payload is dropped inside a
//! nested `catch_unwind` and forgotten if its own `Drop` panics, so a hostile payload cannot escape
//! either. Rust's abort on a panic leaving an `extern "C"` function stays the backstop.
#![expect(
    unsafe_code,
    reason = "reads and calls the host's error sink (ADR 0019)"
)]

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::abi::{
    UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_MAX_ERROR_LEN, UMBRA_OK, UmbraCode,
    UmbraErrorSink, UmbraStr, UmbraToken,
};
use crate::convert::{checked_ref, umbra_str};
use crate::error::Error;

/// Runs `f`, turning a panic into its message. The payload is dropped inside a nested guard.
pub(crate) fn catch<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        let message = panic_message(payload.as_ref());
        drop_payload(payload);
        message
    })
}

/// The text of a `panic!` payload (`&str` or `String`), or a fixed text for any other payload.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    let text = if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    };
    format!("plugin panicked: {text}")
}

/// Drops a panic payload; if its `Drop` panics again, that second payload is forgotten (R-M1).
fn drop_payload(payload: Box<dyn Any + Send>) {
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        std::mem::forget(second);
    }
}

/// `text` cut to at most `max` bytes at a character boundary.
pub(crate) fn truncate(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A validated copy of the host's [`UmbraErrorSink`] for one call.
#[derive(Clone, Copy)]
pub(crate) struct ErrorSink {
    ctx: UmbraToken,
    set: unsafe extern "C" fn(UmbraToken, UmbraStr),
}

impl ErrorSink {
    /// Validates `err` (non-null, aligned, own major, `struct_size` in range and a multiple of 8, `set`
    /// non-null); `None` if it is malformed.
    ///
    /// # Safety
    /// `err` is null or points at memory the host keeps readable for the current call, at least
    /// 8 bytes, and `struct_size` bytes if the header says so (the ABI contract of every call).
    pub(crate) unsafe fn from_raw(err: *const UmbraErrorSink) -> Option<Self> {
        // SAFETY: forwarded from this function's contract.
        let sink = unsafe { checked_ref(err) }?;
        Some(Self {
            ctx: sink.ctx,
            set: sink.set?,
        })
    }

    /// Sends `message` (cut to `UMBRA_MAX_ERROR_LEN` bytes) to the host.
    pub(crate) fn report(self, message: &str) {
        let text = truncate(message, UMBRA_MAX_ERROR_LEN);
        // SAFETY: `set` came from a validated sink of the current call; the host copies the message
        // during the call and `text` outlives it. Host callbacks are `extern "C"` and cannot unwind.
        unsafe { (self.set)(self.ctx, umbra_str(text)) };
    }
}

/// The body of every fallible export that has an error sink: validates the sink (malformed is
/// `UMBRA_ERR_INVALID_ARGUMENT` with no message), runs `body` under a panic guard, and reports an
/// error or panic message through the sink.
///
/// # Safety
/// As [`ErrorSink::from_raw`] for `err`.
pub(crate) unsafe fn with_error_sink(
    err: *const UmbraErrorSink,
    body: impl FnOnce() -> Result<(), Error>,
) -> UmbraCode {
    catch(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(sink) = (unsafe { ErrorSink::from_raw(err) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        match catch(body) {
            Ok(Ok(())) => UMBRA_OK,
            Ok(Err(e)) => {
                sink.report(e.message());
                e.code()
            }
            Err(panic) => {
                sink.report(&panic);
                UMBRA_ERR_PANIC
            }
        }
    })
    .unwrap_or(UMBRA_ERR_PANIC)
}

#[cfg(test)]
mod tests {
    use super::{catch, truncate};

    #[test]
    fn truncate_keeps_short_text_and_cuts_at_a_char_boundary() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abc", 3), "abc");
        // "é" is two bytes: cutting at byte 2 would split it.
        assert_eq!(truncate("aé", 2), "a");
        assert_eq!(truncate("aéb", 3), "aé");
    }

    #[test]
    fn catch_reports_str_string_and_other_payloads() {
        assert_eq!(catch(|| 7), Ok(7));
        let e = catch(|| panic!("static text")).expect_err("panics");
        assert!(e.contains("static text"), "{e}");
        let n = 3;
        let e = catch(|| panic!("formatted {n}")).expect_err("panics");
        assert!(e.contains("formatted 3"), "{e}");
        let e = catch(|| std::panic::panic_any(42_u8)).expect_err("panics");
        assert!(e.contains("non-string"), "{e}");
    }
}
