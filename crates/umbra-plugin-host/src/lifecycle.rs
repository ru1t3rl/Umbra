//! Image lifecycles (REQ-P-8, ADR 0021 "Lifecycle mapping", RR-M1, RR-m4) and the per-image call lock
//! (RR-B1).
//!
//! A lifecycle: load (open the library or take a linked-in entry; find or reserve the image-table slot),
//! describe (the entry is called **exactly twice** with the same `UmbraHostApi*` and must return the same
//! pointer; the descriptor is copied and validated completely), init (optional, once), calls, shutdown
//! (optional, once, only if `init` succeeded, after every handle was released), unload (close the library
//! unless `keep_loaded`). A host that finds the image `Live` reuses it and calls nothing. Every handle,
//! every registered factory and every host holds the image by `Arc`, so the last release runs
//! `shutdown` and the close, and every copied function pointer is gone before the library closes.
//!
//! **Call lock.** Every call into one image (`init`, `shutdown`, and from WI-9 every vtable call) goes
//! through [`LoadedImage::call`]: a call into an image that already has a record on the current thread's
//! call stack is re-entry and fails without touching the lock; otherwise the image's call lock is taken
//! **blocking** (contention from other threads waits and never faults, REQ-K-4), a call record is pushed
//! for the duration of the call ([`crate::tokens`]) and popped after it. A poisoned lock is reported.
//! The entry calls are not "calls" in this sense: they run before `init`, so the plugin's host API is not
//! usable yet (no record, `UMBRA_ERR_STATE`).

#![expect(
    unsafe_code,
    reason = "calls the plugin entry, init and shutdown (ADR 0021)"
)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use umbra_plugin_api::{
    UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_ERR_PARAMS,
    UMBRA_ERR_STATE, UMBRA_ERR_UNSUPPORTED, UMBRA_OK, UmbraCode, UmbraErrorSink, UmbraHostApi,
    UmbraPluginEntry, UmbraToken,
};

use crate::descriptor::{self, Descriptor};
use crate::error::PluginError;
use crate::host_api;
use crate::image::{self, Claim, Reservation};
use crate::library::Library;
use crate::log::{LogLevel, PluginLog};
use crate::tokens::{self, ActiveCall, CallRecord};

/// What a load asks of the image layer.
pub(crate) struct LoadRequest<'a> {
    /// Where the plugin came from (library path as discovered, or the `load_static` origin).
    pub(crate) origin: &'a str,
    /// The loading host's log (receives `init` messages).
    pub(crate) log: &'a Arc<dyn PluginLog>,
    /// The loading host's `keep_loaded` (sticky per image, RR-M3).
    pub(crate) keep_loaded: bool,
    /// Per-host checks run on the copied descriptor: before `init` for a new lifecycle, after the lookup
    /// for a reused image (manifest match, plugin id unique in the host).
    pub(crate) precheck: &'a dyn Fn(&Descriptor) -> Result<(), PluginError>,
}

/// Why a call into an image did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallError {
    /// The current thread is already inside a call into this image (RR-B1).
    Reentry,
    /// The image's call lock is poisoned (a host panic while it was held).
    Poisoned,
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Reentry => "re-entrant call into the plugin from inside one of its calls",
            Self::Poisoned => "the plugin's call lock is poisoned",
        })
    }
}

/// The call-scoped context handed to the code that calls the plugin.
pub(crate) struct CallScope {
    error_sink: UmbraErrorSink,
}

impl CallScope {
    /// The call's error sink, valid for the duration of the call.
    pub(crate) fn error_sink(&self) -> *const UmbraErrorSink {
        &raw const self.error_sink
    }
}

/// Per-image call state, guarded by the call lock.
struct CallState {
    /// The last call generation (high half of call-scoped tokens).
    generation: u32,
}

/// One image lifecycle (see the module docs).
pub(crate) struct LoadedImage {
    slot: u64,
    host_ctx: UmbraToken,
    api: &'static UmbraHostApi,
    descriptor: Descriptor,
    init_ok: bool,
    keep_loaded: AtomicBool,
    /// The log of the host that released the image most recently (receives `shutdown` messages).
    release_log: Mutex<Arc<dyn PluginLog>>,
    calls: Mutex<CallState>,
    /// Closed (or leaked) last, in `Drop`, after `shutdown`.
    library: Option<Library>,
}

impl LoadedImage {
    /// The copied descriptor.
    pub(crate) fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    /// Makes `keep_loaded` stick to this image (RR-M3).
    pub(crate) fn keep_loaded(&self) {
        self.keep_loaded.store(true, Ordering::SeqCst);
    }

    /// Records the log of a host that releases the image; the last one receives `shutdown` messages.
    pub(crate) fn released_by(&self, log: &Arc<dyn PluginLog>) {
        *self
            .release_log
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Arc::clone(log);
    }

    /// Runs `f` as one call into the plugin (see the module docs); returns its result and the first
    /// message the plugin wrote to the call's error sink.
    pub(crate) fn call<R>(
        &self,
        log: &Arc<dyn PluginLog>,
        f: impl FnOnce(&CallScope) -> R,
    ) -> Result<(R, Option<String>), CallError> {
        self.call_inner(log, false, f)
    }

    fn call_inner<R>(
        &self,
        log: &Arc<dyn PluginLog>,
        recover_poison: bool,
        f: impl FnOnce(&CallScope) -> R,
    ) -> Result<(R, Option<String>), CallError> {
        if tokens::is_active(self.host_ctx) {
            return Err(CallError::Reentry);
        }
        let mut state = match self.calls.lock() {
            Ok(state) => state,
            Err(poisoned) if recover_poison => poisoned.into_inner(),
            Err(_) => return Err(CallError::Poisoned),
        };
        state.generation = tokens::next_generation(state.generation);
        let generation = state.generation;
        let record = ActiveCall::push(CallRecord {
            image: self.host_ctx,
            generation,
            plugin: self.descriptor.id.clone(),
            log: Arc::clone(log),
            error: None,
        });
        let scope = CallScope {
            error_sink: host_api::error_sink(tokens::make(generation, tokens::ERROR_SINK_INDEX)),
        };
        let result = f(&scope);
        let error = record.finish();
        drop(state);
        Ok((result, error))
    }

    /// Runs the plugin's `init`, if any (ADR 0021: once per lifecycle, before any other call).
    fn init(&mut self, log: &Arc<dyn PluginLog>) -> Result<(), PluginError> {
        let Some(init) = self.descriptor.init else {
            self.init_ok = true;
            return Ok(());
        };
        let api: *const UmbraHostApi = self.api;
        let outcome = self.call(log, |scope| {
            // SAFETY: `init` is a non-null function pointer from the validated descriptor of this image,
            // whose library is open; `api` is the process-lifetime host API of this lifecycle and the
            // error sink lives for the call. A plugin cannot unwind out of an `extern "C"` function.
            unsafe { init(api, scope.error_sink()) }
        });
        let fail = |message: String| PluginError::Init {
            plugin: self.descriptor.id.clone(),
            message,
        };
        match outcome {
            Ok((UMBRA_OK, _)) => {
                self.init_ok = true;
                Ok(())
            }
            Ok((code, message)) => Err(fail(match message {
                Some(m) => format!("{}: {m}", describe(code)),
                None => describe(code),
            })),
            Err(e) => Err(fail(e.to_string())),
        }
    }
}

impl Drop for LoadedImage {
    /// The end of a lifecycle: `Closing`, `shutdown` (only after a successful `init`), close (unless
    /// `keep_loaded`), then the slot is removed and waiting loads retry.
    fn drop(&mut self) {
        image::closing(self.slot);
        let log = Arc::clone(
            &self
                .release_log
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        if self.init_ok
            && let Some(shutdown) = self.descriptor.shutdown
        {
            // SAFETY: `shutdown` is a non-null function pointer from the validated descriptor; the
            // library is still open (closed below). Every handle of this image is gone (they hold the
            // image's `Arc`), so this is the last call (ADR 0021).
            let outcome = self.call_inner(&log, true, |_| unsafe { shutdown() });
            if let Err(e) = outcome {
                log.log(
                    &self.descriptor.id,
                    LogLevel::Error,
                    &format!("shutdown skipped: {e}"),
                );
            }
        }
        if let Some(library) = self.library.take() {
            if self.keep_loaded.load(Ordering::SeqCst) {
                library.leak();
            } else {
                drop(library);
            }
        }
        image::closed(self.slot);
    }
}

/// The name of an `UmbraCode`, for messages.
pub(crate) fn describe(code: UmbraCode) -> String {
    let name = match code {
        UMBRA_ERR_PANIC => "UMBRA_ERR_PANIC",
        UMBRA_ERR_INVALID_ARGUMENT => "UMBRA_ERR_INVALID_ARGUMENT",
        UMBRA_ERR_PARAMS => "UMBRA_ERR_PARAMS",
        UMBRA_ERR_UNSUPPORTED => "UMBRA_ERR_UNSUPPORTED",
        UMBRA_ERR_INTERNAL => "UMBRA_ERR_INTERNAL",
        UMBRA_ERR_STATE => "UMBRA_ERR_STATE",
        _ => return format!("returned unknown code {code}"),
    };
    format!("returned {name} ({code})")
}

/// Loads the library at the canonical `path` (ADR 0021, ADR 0022): reuse by path, open, reuse by entry
/// address, or a new lifecycle.
pub(crate) fn load_library(
    path: &Path,
    request: &LoadRequest<'_>,
) -> Result<Arc<LoadedImage>, PluginError> {
    let reservation = match image::claim_path(path) {
        Claim::Live(image) => return reuse(image, request),
        Claim::New(reservation) => reservation,
    };
    let library = Library::open(path).map_err(|message| PluginError::Open {
        path: path.to_path_buf(),
        message,
    })?;
    let entry = library.entry().ok_or_else(|| PluginError::MissingEntry {
        path: path.to_path_buf(),
    })?;
    match image::claim_entry(Some(reservation), entry as usize) {
        // Another path to an image that is already live (a hard link, for example): our handle is a
        // second reference to the same mapping and is closed here.
        Claim::Live(image) => {
            drop(library);
            reuse(image, request)
        }
        Claim::New(reservation) => start(
            Pending {
                library: Some(library),
                reservation,
            },
            entry,
            request,
        ),
    }
}

/// Loads an entry function linked into the process (`PluginHost::load_static`): keyed by its address.
pub(crate) fn load_static(
    entry: UmbraPluginEntry,
    request: &LoadRequest<'_>,
) -> Result<Arc<LoadedImage>, PluginError> {
    match image::claim_entry(None, entry as usize) {
        Claim::Live(image) => reuse(image, request),
        Claim::New(reservation) => start(
            Pending {
                library: None,
                reservation,
            },
            entry,
            request,
        ),
    }
}

/// A live image found by a host: per-host checks, then `keep_loaded`.
fn reuse(
    image: Arc<LoadedImage>,
    request: &LoadRequest<'_>,
) -> Result<Arc<LoadedImage>, PluginError> {
    (request.precheck)(image.descriptor())?;
    if request.keep_loaded {
        image.keep_loaded();
    }
    Ok(image)
}

/// The pieces of a lifecycle before its image exists. Field order is drop order: on a failed load the
/// library closes first, then the slot is freed and waiting loads retry.
struct Pending {
    library: Option<Library>,
    reservation: Reservation,
}

/// Starts a new lifecycle: host API, entry twice, copy and validate, per-host checks, `init`, publish.
fn start(
    pending: Pending,
    entry: UmbraPluginEntry,
    request: &LoadRequest<'_>,
) -> Result<Arc<LoadedImage>, PluginError> {
    let origin = request.origin;
    let host_ctx = tokens::make(pending.reservation.generation(), tokens::HOST_CTX_INDEX);
    let api = host_api::leak(host_ctx);
    // SAFETY: `entry` is the plugin's `umbra_plugin_entry` (a linked-in function, or the non-null symbol
    // of the library held open in `pending`); `api` is valid for the process lifetime. A plugin cannot
    // unwind out of an `extern "C"` function (the SDK returns null on a panic).
    let first = unsafe { entry(api) };
    if first.is_null() {
        return Err(PluginError::NullDescriptor {
            origin: origin.to_owned(),
        });
    }
    // SAFETY: as above; the second call of the lifecycle with the same pointer (RR-m4).
    let second = unsafe { entry(api) };
    if second != first {
        return Err(PluginError::UnstableEntry {
            origin: origin.to_owned(),
        });
    }
    // SAFETY: `first` is the non-null pointer the entry returned; the plugin keeps the descriptor valid
    // while its library is open (held in `pending`). `copy` checks alignment, sizes, caps and UTF-8.
    let descriptor = unsafe { descriptor::copy(first, origin) }?;
    (request.precheck)(&descriptor)?;
    let Pending {
        library,
        reservation,
    } = pending;
    let mut image = LoadedImage {
        slot: reservation.into_slot(),
        host_ctx,
        api,
        descriptor,
        init_ok: false,
        keep_loaded: AtomicBool::new(request.keep_loaded),
        release_log: Mutex::new(Arc::clone(request.log)),
        calls: Mutex::new(CallState { generation: 0 }),
        library,
    };
    // On failure `image` drops: no `shutdown` (init did not succeed), close, slot removed.
    image.init(request.log)?;
    let image = Arc::new(image);
    image::publish(image.slot, &image);
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_named() {
        assert_eq!(describe(UMBRA_ERR_PANIC), "returned UMBRA_ERR_PANIC (1)");
        assert_eq!(describe(UMBRA_ERR_STATE), "returned UMBRA_ERR_STATE (6)");
        assert_eq!(describe(42), "returned unknown code 42");
        assert!(CallError::Reentry.to_string().contains("re-entrant"));
        assert!(CallError::Poisoned.to_string().contains("poisoned"));
    }
}
