//! The host side of the C ABI: the per-lifecycle `UmbraHostApi` and the host callbacks (REQ-P-5,
//! REQ-P-7, ADR 0019, ADR 0021).
//!
//! Every callback is `extern "C"`, catches panics (including from an embedder's [`PluginLog`]) and
//! turns them into an error code, checks its token against the calling thread's top call record
//! ([`crate::tokens`]), checks null pointers, lengths and UTF-8, and never calls into a plugin
//! (callbacks are leaves). The `UmbraHostApi` itself is leaked: one per image lifecycle, valid for
//! the process lifetime (ADR 0019 "Pointer lifetime"); a call through an old lifecycle's `host_ctx`
//! finds no record and is `UMBRA_ERR_STATE`.
//!
//! The host writes the struct in its own (newest) layout, today ABI minor 0: the plugin's declared minor
//! is known only from the descriptor the entry returns, after the struct was passed. A host of a later
//! minor writes its larger size, and plugins of an older minor read their prefix (ADR 0019 reader rule;
//! ASM-134).

#![expect(
    unsafe_code,
    reason = "the host side of the C ABI: callbacks reading plugin strings (ADR 0019)"
)]

use std::any::Any;
use std::mem::size_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE,
    UMBRA_MAX_ERROR_LEN, UMBRA_OK, UmbraCode, UmbraErrorSink, UmbraHostApi, UmbraLogLevel,
    UmbraStr, UmbraToken,
};

use umbra_core::RngStream;

use crate::log::LogLevel;
use crate::tokens::{self, Sink};

/// Leaks a fresh host API for an image lifecycle whose `host_ctx` is `host_ctx`.
pub(crate) fn leak(host_ctx: UmbraToken) -> &'static UmbraHostApi {
    Box::leak(Box::new(UmbraHostApi {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: abi_size::<UmbraHostApi>(),
        host_ctx,
        log: Some(host_log),
        rng_next_u64: Some(host_rng_next_u64),
        rng_next_f64: Some(host_rng_next_f64),
    }))
}

/// An error sink for a call whose error-sink token is `ctx`.
pub(crate) fn error_sink(ctx: UmbraToken) -> UmbraErrorSink {
    UmbraErrorSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: abi_size::<UmbraErrorSink>(),
        ctx,
        set: Some(host_error_set),
    }
}

/// `size_of::<T>()` as the ABI's `u32` `struct_size` (every ABI struct is far below 4 GiB).
pub(crate) const fn abi_size<T>() -> u32 {
    const { assert!(size_of::<T>() <= u32::MAX as usize) };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "checked by the const assert above"
    )]
    let size = size_of::<T>() as u32;
    size
}

/// Runs `f`, turning a panic into `on_panic`. The panic payload is dropped inside a second guard
/// and forgotten if its own `Drop` panics, so nothing unwinds out of a callback (ADR 0019, R-M1).
pub(crate) fn guarded<R>(on_panic: R, f: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            drop_payload(payload);
            on_panic
        }
    }
}

/// Drops a panic payload; a payload whose `Drop` panics is forgotten instead of unwinding.
pub(crate) fn drop_payload(payload: Box<dyn Any + Send>) {
    if let Err(inner) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        std::mem::forget(inner);
    }
}

/// Borrows the bytes of a plugin string: `None` for a null pointer with a non-zero length or a
/// length above `isize::MAX`.
///
/// # Safety
/// A non-null `s.ptr` must point to `s.len` readable bytes that stay unchanged for `'a` (the
/// plugin's promise for the duration of the call that passed them, ADR 0019).
pub(crate) unsafe fn bytes<'a>(s: UmbraStr) -> Option<&'a [u8]> {
    if s.len == 0 {
        return Some(&[]);
    }
    if s.ptr.is_null() || isize::try_from(s.len).is_err() {
        return None;
    }
    // SAFETY: non-null, length within `isize::MAX`, and readable for `'a` by the caller's contract;
    // `u8` has alignment 1.
    Some(unsafe { std::slice::from_raw_parts(s.ptr, s.len) })
}

/// The host's `log` (ADR 0021): routed to the calling host's [`crate::PluginLog`], attributed to
/// the plugin of the current call. `UMBRA_ERR_STATE` outside a call into this image lifecycle
/// (another thread, before `init`, after `shutdown`, an old lifecycle's `host_ctx`);
/// `UMBRA_ERR_INVALID_ARGUMENT` for an unknown level, a null pointer with a length, or invalid
/// UTF-8.
unsafe extern "C" fn host_log(
    host_ctx: UmbraToken,
    level: UmbraLogLevel,
    message: UmbraStr,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let Some((plugin, log)) = tokens::with_top(|r| {
            (r.image == host_ctx).then(|| (r.plugin.clone(), Arc::clone(&r.log)))
        })
        .flatten() else {
            return UMBRA_ERR_STATE;
        };
        let Some(level) = LogLevel::from_abi(level) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        // SAFETY: the plugin passes the message borrowed for the duration of this call (ADR 0019).
        let Some(text) = (unsafe { bytes(message) }).and_then(|b| std::str::from_utf8(b).ok())
        else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        log.log(&plugin, level, text);
        UMBRA_OK
    })
}

/// The host's RNG draw (ADR 0021, R-B4): the next `u64` of the entity stream named by `rng`, the
/// RNG token of a batch item of the current `tick_batch` call or of the observer of the current
/// `sense` call (ABI 0.2). No call record on this thread is
/// `UMBRA_ERR_STATE` without any lookup (another thread, outside a call). With a record, every
/// misuse also records a breach that faults the capability: another image's `host_ctx` or a call
/// without RNG tokens or a token of another call (`UMBRA_ERR_STATE`), a token naming no item or a
/// null or misaligned `out` (`UMBRA_ERR_INVALID_ARGUMENT`). Another item's token of the same call
/// cannot be detected: it draws that item's stream (ADR 0021, documented contract breach).
unsafe extern "C" fn host_rng_next_u64(
    host_ctx: UmbraToken,
    rng: UmbraToken,
    out: *mut u64,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        match draw(host_ctx, rng, out.cast(), RngStream::next_u64) {
            Ok(value) => {
                // SAFETY: `draw` checked `out` non-null and 8-byte aligned; the plugin passes it
                // writable for the duration of this call.
                unsafe { out.write(value) };
                UMBRA_OK
            }
            Err(code) => code,
        }
    })
}

/// See [`host_rng_next_u64`]; draws an `f64` in `[0, 1)`.
unsafe extern "C" fn host_rng_next_f64(
    host_ctx: UmbraToken,
    rng: UmbraToken,
    out: *mut f64,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        match draw(host_ctx, rng, out.cast(), RngStream::next_f64) {
            Ok(value) => {
                // SAFETY: as in `host_rng_next_u64`.
                unsafe { out.write(value) };
                UMBRA_OK
            }
            Err(code) => code,
        }
    })
}

/// Resolves an RNG token against the calling thread's top record and draws from the item's stream.
fn draw<T>(
    host_ctx: UmbraToken,
    rng: UmbraToken,
    out: *mut u8,
    next: fn(&mut RngStream) -> T,
) -> Result<T, UmbraCode> {
    tokens::with_top(|r| {
        let refuse = |r: &mut tokens::CallRecord, code: UmbraCode, why: &str| {
            r.breach(|| format!("RNG draw refused: {why}"));
            Err(code)
        };
        if r.image != host_ctx {
            return refuse(r, UMBRA_ERR_STATE, "host_ctx of another image or lifecycle");
        }
        if !r.has_rng() {
            return refuse(
                r,
                UMBRA_ERR_STATE,
                "no RNG token is valid outside tick_batch, sense, check and tick calls",
            );
        }
        if tokens::generation_of(rng) != r.generation {
            return refuse(r, UMBRA_ERR_STATE, "stale RNG token of another call");
        }
        let Some(stream) = r.rng(tokens::index_of(rng)) else {
            return refuse(r, UMBRA_ERR_INVALID_ARGUMENT, "the RNG token names no item");
        };
        if out.is_null() || !(out as usize).is_multiple_of(8) {
            return refuse(
                r,
                UMBRA_ERR_INVALID_ARGUMENT,
                "null or misaligned out pointer",
            );
        }
        // SAFETY: the bridge set this pointer from the item's (or the sense or leaf call's)
        // `&mut RngStream` for the duration of the current call on this thread and does not touch it
        // while the plugin runs; the record (and so the pointer) is popped before the call returns to
        // the kernel.
        Ok(next(unsafe { &mut *stream }))
    })
    .unwrap_or(Err(UMBRA_ERR_STATE))
}

/// The host's error sink (ADR 0019 "Results"): the first message of a call wins, later ones and
/// messages for another call are ignored; the text is cut at `UMBRA_MAX_ERROR_LEN` bytes on a
/// character boundary and decoded lossily; a null pointer with a length records a placeholder
/// without reading.
unsafe extern "C" fn host_error_set(ctx: UmbraToken, message: UmbraStr) {
    guarded((), || {
        let wanted = tokens::with_top(|r| r.sink(Sink::Error) == ctx && r.error.is_none());
        if wanted != Some(true) {
            return;
        }
        // SAFETY: the plugin passes the message borrowed for the duration of this call; at most
        // `UMBRA_MAX_ERROR_LEN + 1` of its `len` bytes are read.
        let text = unsafe { truncated(message) };
        tokens::with_top(|r| {
            if r.sink(Sink::Error) == ctx && r.error.is_none() {
                r.error = Some(text);
            }
        });
    });
}

/// Decodes at most `UMBRA_MAX_ERROR_LEN` bytes of `s`, cut on a character boundary, lossily.
///
/// # Safety
/// As [`bytes`], for the first `min(s.len, UMBRA_MAX_ERROR_LEN + 1)` bytes.
pub(crate) unsafe fn truncated(s: UmbraStr) -> String {
    let head = UmbraStr {
        ptr: s.ptr,
        len: s.len.min(UMBRA_MAX_ERROR_LEN + 1),
    };
    // SAFETY: forwarded caller contract; `head.len <= s.len`.
    let Some(b) = (unsafe { bytes(head) }) else {
        return format!(
            "<invalid error message: null pointer with length {}>",
            s.len
        );
    };
    let mut end = b.len().min(UMBRA_MAX_ERROR_LEN);
    // Back off over UTF-8 continuation bytes so a character is never split (`b[end]` exists when
    // cut).
    while end > 0 && end < b.len() && (b[end] & 0xC0) == 0x80 {
        end -= 1;
    }
    String::from_utf8_lossy(&b[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::TracingLog;
    use crate::tokens::{ActiveCall, CallRecord};
    use umbra_core::PluginId;
    use umbra_plugin_api::UMBRA_LOG_INFO;

    fn s(text: &str) -> UmbraStr {
        UmbraStr {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    fn call(image: UmbraToken, generation: u32) -> ActiveCall {
        ActiveCall::push(CallRecord::new(
            image,
            generation,
            PluginId::new("test.plugin.api").unwrap_or_else(|e| panic!("{e}")),
            Arc::new(TracingLog),
        ))
    }

    #[test]
    fn bytes_refuses_null_with_a_length_and_accepts_empty() {
        // SAFETY: valid string, and two cases that must be refused before any read.
        unsafe {
            assert_eq!(bytes(s("abc")), Some(&b"abc"[..]));
            let null = UmbraStr {
                ptr: std::ptr::null(),
                len: 0,
            };
            assert_eq!(bytes(null), Some(&[][..]));
            let bad = UmbraStr {
                ptr: std::ptr::null(),
                len: 3,
            };
            assert_eq!(bytes(bad), None);
            let huge = UmbraStr {
                ptr: b"x".as_ptr(),
                len: usize::MAX,
            };
            assert_eq!(bytes(huge), None);
        }
    }

    #[test]
    fn truncation_keeps_whole_characters_and_at_most_the_cap() {
        let long = "é".repeat(UMBRA_MAX_ERROR_LEN);
        // SAFETY: valid string.
        let t = unsafe { truncated(s(&long)) };
        assert!(t.len() <= UMBRA_MAX_ERROR_LEN);
        assert_eq!(
            t.len(),
            UMBRA_MAX_ERROR_LEN,
            "2-byte chars fill the cap exactly"
        );
        assert!(t.chars().all(|c| c == 'é'));
        let odd = format!("x{}", "é".repeat(UMBRA_MAX_ERROR_LEN));
        // SAFETY: valid string.
        let t = unsafe { truncated(s(&odd)) };
        assert_eq!(
            t.len(),
            UMBRA_MAX_ERROR_LEN - 1,
            "a split character is dropped"
        );
        // SAFETY: refused before reading.
        let null = unsafe {
            truncated(UmbraStr {
                ptr: std::ptr::null(),
                len: 5,
            })
        };
        assert!(null.contains("null pointer"), "{null}");
    }

    #[test]
    fn callbacks_without_a_record_are_err_state_and_the_error_sink_ignores_them() {
        let api = leak(tokens::make(99, tokens::HOST_CTX_INDEX));
        let log = api.log.unwrap_or_else(|| panic!("log"));
        let draw = api.rng_next_u64.unwrap_or_else(|| panic!("rng"));
        let mut out = 0_u64;
        // SAFETY: valid string and out pointer; the callbacks must refuse without a record.
        unsafe {
            assert_eq!(log(api.host_ctx, UMBRA_LOG_INFO, s("x")), UMBRA_ERR_STATE);
            assert_eq!(draw(api.host_ctx, 1, &raw mut out), UMBRA_ERR_STATE);
            host_error_set(
                tokens::sink_token(api.host_ctx, 1, Sink::Error),
                s("ignored"),
            );
        }
    }

    #[test]
    fn callbacks_check_level_utf8_out_pointer_and_first_error_wins() {
        let image = tokens::make(100, tokens::HOST_CTX_INDEX);
        let api = leak(image);
        let log = api.log.unwrap_or_else(|| panic!("log"));
        let draw = api.rng_next_f64.unwrap_or_else(|| panic!("rng"));
        let active = call(image, 5);
        let sink = error_sink(tokens::sink_token(image, 5, Sink::Error));
        let set = sink.set.unwrap_or_else(|| panic!("set"));
        let bad = [0xFF_u8, 0xFE];
        let mut out = 0.0_f64;
        // SAFETY: valid strings and pointers for the duration of each call.
        unsafe {
            assert_eq!(log(image, UMBRA_LOG_INFO, s("hello")), UMBRA_OK);
            assert_eq!(log(image, 0, s("x")), UMBRA_ERR_INVALID_ARGUMENT);
            let bad = UmbraStr {
                ptr: bad.as_ptr(),
                len: bad.len(),
            };
            assert_eq!(log(image, UMBRA_LOG_INFO, bad), UMBRA_ERR_INVALID_ARGUMENT);
            assert_eq!(
                log(image + (1 << 32), UMBRA_LOG_INFO, s("x")),
                UMBRA_ERR_STATE,
                "another lifecycle's host_ctx"
            );
            assert_eq!(
                draw(image, tokens::rng_token(5, 0), &raw mut out),
                UMBRA_ERR_STATE,
                "no RNG tokens outside tick_batch"
            );
            set(tokens::sink_token(image, 6, Sink::Error), s("other call"));
            let other_image = tokens::make(101, tokens::HOST_CTX_INDEX);
            set(
                tokens::sink_token(other_image, 5, Sink::Error),
                s("other image"),
            );
            set(sink.ctx, s("first"));
            set(sink.ctx, s("second"));
        }
        let finished = active.finish();
        assert_eq!(finished.error.as_deref(), Some("first"));
        assert!(
            finished
                .breach
                .is_some_and(|b| b.contains("outside tick_batch")),
            "a draw without RNG tokens is a breach"
        );
    }

    #[test]
    fn rng_tokens_draw_the_items_stream_and_misuse_is_a_breach() {
        let image = tokens::make(102, tokens::HOST_CTX_INDEX);
        let api = leak(image);
        let next = api.rng_next_u64.unwrap_or_else(|| panic!("rng"));
        let mut stream = umbra_core::SimRng::new(4).stream(umbra_core::StreamKey::scenario(1));
        let mut reference = stream.clone();
        let active = call(image, 9);
        tokens::with_top(|r| {
            r.batch = Some(tokens::BatchCall {
                rngs: vec![&raw mut stream],
                ..tokens::BatchCall::default()
            });
        });
        let mut out = 0_u64;
        // SAFETY: valid out pointers; the stream outlives the record.
        unsafe {
            assert_eq!(next(image, tokens::rng_token(9, 0), &raw mut out), UMBRA_OK);
            assert_eq!(out, reference.next_u64(), "the item's own stream");
            assert_eq!(
                next(image, tokens::rng_token(9, 1), &raw mut out),
                UMBRA_ERR_INVALID_ARGUMENT,
                "no such item"
            );
            assert_eq!(
                next(image, tokens::rng_token(8, 0), &raw mut out),
                UMBRA_ERR_STATE,
                "a token of another call"
            );
            let misaligned = std::ptr::without_provenance_mut::<u64>(4);
            assert_eq!(
                next(image, tokens::rng_token(9, 0), misaligned),
                UMBRA_ERR_INVALID_ARGUMENT
            );
        }
        tokens::with_top(|r| r.batch = None);
        let finished = active.finish();
        assert!(
            finished.breach.is_some_and(|b| b.contains("names no item")),
            "the first misuse is the breach"
        );
        assert_eq!(stream.state(), reference.state(), "only the one draw");
    }

    #[test]
    fn guarded_turns_panics_into_the_fallback_and_reads_their_text() {
        assert_eq!(guarded(7, || panic!("boom")), 7);
        let result: std::thread::Result<()> = catch_unwind(|| panic!("payload"));
        if let Err(payload) = result {
            drop_payload(payload);
        }
    }
}
