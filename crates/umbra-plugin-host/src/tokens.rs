//! Opaque tokens and the thread-local call-record stack (ADR 0019 "Tokens instead of host
//! pointers", RR-M2, ASM-122, ASM-139).
//!
//! **Tokens.** Every host context a plugin can hold is a `u64`: generation in the high half, index
//! in the low half, 0 invalid. Tokens are checked, never dereferenced.
//! - `host_ctx`: the image lifecycle generation (from the image table) and index
//!   [`HOST_CTX_INDEX`].
//! - Sink tokens (error, intent and value sinks): the image's **call** generation, and an index
//!   that packs the low 16 bits of the image lifecycle generation (the image "tag") with the sink
//!   kind, so a sink token names its image as well as its call.
//! - RNG tokens: the call generation and `item + 1`; the image is named by the `host_ctx` argument
//!   of the RNG callback.
//!
//! Generations wrap; a wrapped generation can at worst misattribute a call to another call of the
//! same thread, never cause undefined behavior.
//!
//! **Call records.** Every call into a plugin pushes one [`CallRecord`] on a stack that is local to
//! the calling thread, and pops it when the call returns, so the stack is **call-scoped**: it is
//! empty whenever no plugin call is running on that thread, and it is not persistent global state
//! (ASM-122). A host callback resolves its token against the **top** record of the calling thread
//! only: no record is `UMBRA_ERR_STATE` without any other lookup (callbacks from another thread,
//! before `init`, after `shutdown`); a top record whose tokens do not match records a **breach** on
//! that record (the call then faults the capability). A call into an image that already has a
//! record anywhere on the current thread's stack is re-entry ([`is_active`]), found before the
//! image's call lock is taken (RR-B1).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::Arc;

use umbra_core::{BbValue, Intent, PluginId, RngStream};
use umbra_plugin_api::UmbraToken;

use crate::log::PluginLog;

/// Index (low half) of an image lifecycle's `host_ctx` token.
pub(crate) const HOST_CTX_INDEX: u32 = 1;

/// The kind of a call-scoped sink token (low 16 bits of its index).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sink {
    /// The error sink of any call.
    Error = 1,
    /// The intent sink of a `tick_batch` call.
    Intent = 2,
    /// The value sink of an `output` call.
    Value = 3,
}

/// Packs a token; `generation` must be non-zero for the token to be valid (callers skip 0).
pub(crate) const fn make(generation: u32, index: u32) -> UmbraToken {
    ((generation as u64) << 32) | index as u64
}

/// The generation (high half) of a token.
pub(crate) const fn generation_of(token: UmbraToken) -> u32 {
    (token >> 32) as u32
}

/// The index (low half) of a token.
pub(crate) const fn index_of(token: UmbraToken) -> u32 {
    (token & 0xFFFF_FFFF) as u32
}

/// The next generation after `generation`, skipping 0 (wraps).
pub(crate) const fn next_generation(generation: u32) -> u32 {
    match generation.wrapping_add(1) {
        0 => 1,
        g => g,
    }
}

/// The token of `sink` in call `generation` of the image lifecycle `image` (its `host_ctx`).
pub(crate) const fn sink_token(image: UmbraToken, generation: u32, sink: Sink) -> UmbraToken {
    let tag = generation_of(image) & 0xFFFF;
    make(generation, (tag << 16) | sink as u32)
}

/// The RNG token of batch item `item` in call `generation`.
pub(crate) fn rng_token(generation: u32, item: usize) -> UmbraToken {
    // Items beyond `u32::MAX - 1` cannot be named; the bridge never builds such a batch.
    make(generation, u32::try_from(item + 1).unwrap_or(0))
}

/// The per-call state of a `tick_batch` call, moved into the record for the duration of the call
/// and back into the image's scratch afterwards (no allocation after warm-up).
#[derive(Default)]
pub(crate) struct BatchCall {
    /// The entity stream of each item, pointing into the kernel's `BatchItem`s, valid only during
    /// the call (the bridge does not touch the items while the plugin runs).
    pub(crate) rngs: Vec<*mut RngStream>,
    /// Pushed intents (item index, intent), committed only after a clean call.
    pub(crate) intents: Vec<(usize, Intent)>,
    /// Intents pushed per item (the `UMBRA_MAX_INTENTS_PER_ITEM` cap).
    pub(crate) counts: Vec<usize>,
}

/// The per-call state of an `output` call.
#[derive(Default)]
pub(crate) struct OutputCall {
    /// How often the plugin called the value sink.
    pub(crate) calls: u32,
    /// The value it reported (`None` = unset).
    pub(crate) value: Option<BbValue>,
}

/// One in-progress call into a plugin on this thread.
pub(crate) struct CallRecord {
    /// The image lifecycle the call goes into (its `host_ctx`).
    pub(crate) image: UmbraToken,
    /// The image's call generation of this call (high half of its call-scoped tokens).
    pub(crate) generation: u32,
    /// The plugin's declared ABI minor: the ceiling for structs it writes during the call (ADR 0033).
    pub(crate) minor: u32,
    /// The plugin messages are attributed to.
    pub(crate) plugin: PluginId,
    /// The log of the host that made the call.
    pub(crate) log: Arc<dyn PluginLog>,
    /// The first message the plugin wrote to the call's error sink (later ones are ignored).
    pub(crate) error: Option<String>,
    /// The first contract breach a host callback found during the call (faults the capability).
    pub(crate) breach: Option<String>,
    /// Set during a `tick_batch` call.
    pub(crate) batch: Option<BatchCall>,
    /// Set during an `output` call.
    pub(crate) output: Option<OutputCall>,
}

impl CallRecord {
    /// A record with no messages, breaches or call state.
    pub(crate) fn new(
        image: UmbraToken,
        generation: u32,
        plugin: PluginId,
        log: Arc<dyn PluginLog>,
    ) -> Self {
        Self {
            image,
            generation,
            minor: umbra_plugin_api::UMBRA_ABI_VERSION_MINOR,
            plugin,
            log,
            error: None,
            breach: None,
            batch: None,
            output: None,
        }
    }

    /// The record with the plugin's declared minor (the ceiling for plugin-written structs; without
    /// it the host's own minor).
    pub(crate) const fn with_minor(mut self, minor: u32) -> Self {
        self.minor = minor;
        self
    }

    /// The token of `sink` in this call.
    pub(crate) const fn sink(&self, sink: Sink) -> UmbraToken {
        sink_token(self.image, self.generation, sink)
    }

    /// Records `message` as the call's breach unless one is recorded already (the first wins).
    pub(crate) fn breach(&mut self, message: impl FnOnce() -> String) {
        if self.breach.is_none() {
            self.breach = Some(message());
        }
    }
}

thread_local! {
    /// The calling thread's in-progress plugin calls, innermost last. Call-scoped (see the module
    /// docs).
    static CALLS: RefCell<Vec<CallRecord>> = const { RefCell::new(Vec::new()) };
}

/// Whether the current thread is inside a call into `image` (re-entry check, RR-B1). Fails closed:
/// if the stack cannot be read (thread-local storage torn down, or the stack borrowed), it answers
/// `true`, so the caller refuses the call instead of risking a deadlock on the image's call lock.
pub(crate) fn is_active(image: UmbraToken) -> bool {
    CALLS
        .try_with(|calls| {
            calls
                .try_borrow()
                .map_or(true, |c| c.iter().any(|r| r.image == image))
        })
        .unwrap_or(true)
}

/// Runs `f` on the top record of the current thread; `None` without a record.
///
/// `f` must not call into a plugin or a [`PluginLog`] (the stack is borrowed while it runs);
/// callers copy what they need out and act after it returns.
pub(crate) fn with_top<R>(f: impl FnOnce(&mut CallRecord) -> R) -> Option<R> {
    CALLS
        .try_with(|calls| {
            let mut calls = calls.try_borrow_mut().ok()?;
            calls.last_mut().map(f)
        })
        .ok()
        .flatten()
}

/// What a finished call left in its record.
#[derive(Debug, Default)]
pub(crate) struct Finished {
    /// The first error-sink message.
    pub(crate) error: Option<String>,
    /// The first breach.
    pub(crate) breach: Option<String>,
}

/// A pushed [`CallRecord`]; popping it (by [`ActiveCall::finish`] or on drop, also during
/// unwinding) keeps the stack call-scoped. Not `Send`: it must be finished on the thread that
/// pushed it.
pub(crate) struct ActiveCall {
    image: UmbraToken,
    generation: u32,
    pushed: bool,
    _thread_bound: PhantomData<*const ()>,
}

impl ActiveCall {
    /// Pushes `record` on the current thread's stack.
    pub(crate) fn push(record: CallRecord) -> Self {
        let (image, generation) = (record.image, record.generation);
        let pushed = CALLS
            .try_with(|calls| calls.try_borrow_mut().map(|mut c| c.push(record)).is_ok())
            .unwrap_or(false);
        Self {
            image,
            generation,
            pushed,
            _thread_bound: PhantomData,
        }
    }

    /// Whether the record is on the stack (it is not if thread-local storage was unusable; then
    /// every callback of the call is refused).
    #[cfg(test)]
    pub(crate) fn is_pushed(&self) -> bool {
        self.pushed
    }

    /// Pops the record and returns what the call left in it.
    pub(crate) fn finish(mut self) -> Finished {
        self.pop()
    }

    fn pop(&mut self) -> Finished {
        if !std::mem::take(&mut self.pushed) {
            return Finished::default();
        }
        let (image, generation) = (self.image, self.generation);
        CALLS
            .try_with(|calls| {
                let mut calls = calls.try_borrow_mut().ok()?;
                // Always the top record (calls nest strictly); searched from the top for
                // robustness.
                let at = calls
                    .iter()
                    .rposition(|r| r.image == image && r.generation == generation)?;
                let record = calls.remove(at);
                Some(Finished {
                    error: record.error,
                    breach: record.breach,
                })
            })
            .ok()
            .flatten()
            .unwrap_or_default()
    }
}

impl Drop for ActiveCall {
    fn drop(&mut self) {
        self.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::TracingLog;

    fn record(image: UmbraToken, generation: u32) -> CallRecord {
        CallRecord::new(
            image,
            generation,
            PluginId::new("test.plugin.tokens").unwrap_or_else(|e| panic!("{e}")),
            Arc::new(TracingLog),
        )
    }

    #[test]
    fn tokens_pack_generation_high_and_index_low() {
        assert_eq!(make(1, 2), (1 << 32) | 2);
        assert_eq!(make(u32::MAX, u32::MAX), u64::MAX);
        assert_eq!(generation_of(make(7, 9)), 7);
        assert_eq!(index_of(make(7, 9)), 9);
        assert_eq!(next_generation(1), 2);
        assert_eq!(next_generation(u32::MAX), 1, "wraps and skips 0");
        assert_eq!(rng_token(3, 0), make(3, 1));
    }

    #[test]
    fn sink_tokens_name_the_image_the_call_and_the_kind() {
        let a = make(0x0001_0005, HOST_CTX_INDEX);
        let b = make(0x0001_0006, HOST_CTX_INDEX);
        assert_ne!(sink_token(a, 9, Sink::Error), sink_token(b, 9, Sink::Error));
        assert_ne!(
            sink_token(a, 9, Sink::Error),
            sink_token(a, 10, Sink::Error)
        );
        assert_ne!(
            sink_token(a, 9, Sink::Error),
            sink_token(a, 9, Sink::Intent)
        );
        assert_eq!(index_of(sink_token(a, 9, Sink::Value)), (5 << 16) | 3);
    }

    #[test]
    fn records_are_call_scoped_and_thread_local() {
        let image = make(7, HOST_CTX_INDEX);
        assert!(!is_active(image));
        let outer = ActiveCall::push(record(image, 1));
        assert!(outer.is_pushed());
        assert!(is_active(image));
        assert!(
            !std::thread::spawn(move || is_active(image))
                .join()
                .unwrap_or(true),
            "another thread sees no record"
        );
        let inner = ActiveCall::push(record(make(8, HOST_CTX_INDEX), 1));
        assert_eq!(with_top(|r| r.image), Some(make(8, HOST_CTX_INDEX)));
        drop(inner);
        assert_eq!(with_top(|r| r.image), Some(image));
        with_top(|r| r.error = Some("first".to_owned()));
        with_top(|r| r.breach(|| "breach one".to_owned()));
        with_top(|r| r.breach(|| "breach two".to_owned()));
        let finished = outer.finish();
        assert_eq!(finished.error.as_deref(), Some("first"));
        assert_eq!(finished.breach.as_deref(), Some("breach one"));
        assert!(!is_active(image));
        assert_eq!(with_top(|r| r.image), None, "empty after the last call");
    }

    #[test]
    fn is_active_fails_closed_while_the_stack_is_borrowed() {
        let image = make(11, HOST_CTX_INDEX);
        let _call = ActiveCall::push(record(make(12, HOST_CTX_INDEX), 1));
        let inside = with_top(|_| is_active(image));
        assert_eq!(inside, Some(true), "a borrowed stack counts as active");
    }
}
