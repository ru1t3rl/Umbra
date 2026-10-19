//! Opaque tokens and the thread-local call-record stack (ADR 0019 "Tokens instead of host pointers",
//! RR-M2, ASM-122).
//!
//! **Tokens.** Every host context a plugin can hold is a `u64`: generation in the high half, index in
//! the low half, 0 invalid. Tokens are checked, never dereferenced. `host_ctx` carries the image
//! lifecycle generation; call-scoped tokens (the error sink, and from WI-9 the batch sinks and RNG
//! tokens) carry the call generation of the image. Generations wrap; a wrapped generation can at worst
//! misattribute a call to another call of the same thread, never cause undefined behavior.
//!
//! **Call records.** Every call into a plugin pushes one [`CallRecord`] on a stack that is local to the
//! calling thread, and pops it when the call returns, so the stack is **call-scoped**: it is empty
//! whenever no plugin call is running on that thread, and it is not persistent global state (ASM-122).
//! A host callback resolves its token against the **top** record of the calling thread only: no record,
//! or a top record of another image or call, is refused without any other lookup (`UMBRA_ERR_STATE`).
//! That makes callbacks from another thread, before `init` and after `shutdown` refusals by
//! construction. A call into an image that already has a record anywhere on the current thread's stack
//! is re-entry ([`is_active`]), found before the image's call lock is taken (RR-B1).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::Arc;

use umbra_core::PluginId;
use umbra_plugin_api::UmbraToken;

use crate::log::PluginLog;

/// Index (low half) of an image lifecycle's `host_ctx` token.
pub(crate) const HOST_CTX_INDEX: u32 = 1;
/// Index (low half) of a call's error-sink token.
pub(crate) const ERROR_SINK_INDEX: u32 = 1;

/// Packs a token; `generation` must be non-zero for the token to be valid (callers skip 0).
pub(crate) const fn make(generation: u32, index: u32) -> UmbraToken {
    ((generation as u64) << 32) | index as u64
}

/// The next generation after `generation`, skipping 0 (wraps).
pub(crate) const fn next_generation(generation: u32) -> u32 {
    match generation.wrapping_add(1) {
        0 => 1,
        g => g,
    }
}

/// One in-progress call into a plugin on this thread.
pub(crate) struct CallRecord {
    /// The image lifecycle the call goes into (its `host_ctx`).
    pub(crate) image: UmbraToken,
    /// The image's call generation of this call (high half of its call-scoped tokens).
    pub(crate) generation: u32,
    /// The plugin messages are attributed to.
    pub(crate) plugin: PluginId,
    /// The log of the host that made the call.
    pub(crate) log: Arc<dyn PluginLog>,
    /// The first message the plugin wrote to the call's error sink (later ones are ignored).
    pub(crate) error: Option<String>,
}

impl CallRecord {
    /// The error-sink token of this call.
    pub(crate) const fn error_ctx(&self) -> UmbraToken {
        make(self.generation, ERROR_SINK_INDEX)
    }
}

thread_local! {
    /// The calling thread's in-progress plugin calls, innermost last. Call-scoped (see the module docs).
    static CALLS: RefCell<Vec<CallRecord>> = const { RefCell::new(Vec::new()) };
}

/// Whether the current thread is inside a call into `image` (re-entry check, RR-B1).
pub(crate) fn is_active(image: UmbraToken) -> bool {
    CALLS
        .try_with(|calls| {
            calls
                .try_borrow()
                .is_ok_and(|c| c.iter().any(|r| r.image == image))
        })
        .unwrap_or(false)
}

/// Runs `f` on the top record of the current thread; `None` without a record.
///
/// `f` must not call into a plugin or a [`PluginLog`] (the stack is borrowed while it runs); callers copy
/// what they need out and act after it returns.
pub(crate) fn with_top<R>(f: impl FnOnce(&mut CallRecord) -> R) -> Option<R> {
    CALLS
        .try_with(|calls| {
            let mut calls = calls.try_borrow_mut().ok()?;
            calls.last_mut().map(f)
        })
        .ok()
        .flatten()
}

/// A pushed [`CallRecord`]; popping it (by [`ActiveCall::finish`] or on drop, also during unwinding)
/// keeps the stack call-scoped. Not `Send`: it must be finished on the thread that pushed it.
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

    /// Pops the record and returns the first error message the plugin wrote during the call.
    pub(crate) fn finish(mut self) -> Option<String> {
        self.pop()
    }

    fn pop(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.pushed) {
            return None;
        }
        let (image, generation) = (self.image, self.generation);
        CALLS
            .try_with(|calls| {
                let mut calls = calls.try_borrow_mut().ok()?;
                // Always the top record (calls nest strictly); searched from the top for robustness.
                let at = calls
                    .iter()
                    .rposition(|r| r.image == image && r.generation == generation)?;
                calls.remove(at).error
            })
            .ok()
            .flatten()
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
        CallRecord {
            image,
            generation,
            plugin: PluginId::new("test.plugin.tokens").unwrap_or_else(|e| panic!("{e}")),
            log: Arc::new(TracingLog),
            error: None,
        }
    }

    #[test]
    fn tokens_pack_generation_high_and_index_low() {
        assert_eq!(make(1, 2), (1 << 32) | 2);
        assert_eq!(make(u32::MAX, u32::MAX), u64::MAX);
        assert_eq!(next_generation(1), 2);
        assert_eq!(next_generation(u32::MAX), 1, "wraps and skips 0");
    }

    #[test]
    fn records_are_call_scoped_and_thread_local() {
        let image = make(7, HOST_CTX_INDEX);
        assert!(!is_active(image));
        let outer = ActiveCall::push(record(image, 1));
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
        assert_eq!(outer.finish().as_deref(), Some("first"));
        assert!(!is_active(image));
        assert_eq!(with_top(|r| r.image), None, "empty after the last call");
    }
}
