//! The network adapter capability of `export_plugin!` (ABI 0.1, ADR 0033): the vtable and its
//! `extern "C"` trampolines over [`NetworkAdapterFactory`], [`NetworkAdapterConfig`] and
//! [`NetworkAdapter`].
//!
//! One `configure` makes one plugin-owned handle, a `Box<Handle<NetState<F>>>` tagged per capability
//! like the behavior handles, which holds the configuration and, after `start`, the started adapter.
//! Every trampoline runs under a panic guard and validates every input before any trait call. The
//! lifecycle rules of `umbra-plugin-api` `network.rs` are enforced here, so a plugin author cannot
//! break them: `start` at most once (a second call is `UMBRA_ERR_STATE`), `publish` and `poll` only
//! between `start` and `stop` (`UMBRA_ERR_STATE` otherwise), `stop` idempotent and valid without
//! `start`, the adapter's `stop` called at most once and always before `destroy` drops it, and the
//! adapter dropped before its configuration. `key_space`, `stop`, `counters` and `destroy` have no
//! error sink: a panic in them is logged at error level through the host API.
#![expect(
    unsafe_code,
    reason = "the network adapter vtable of the plugin ABI (ADR 0033)"
)]

use std::marker::PhantomData;

use super::{Cap, VTable, configure_into, destroy, handle, without_sink};
use crate::abi::{
    UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UMBRA_MAX_NET_POLL, UMBRA_OK, UmbraCode,
    UmbraErrorSink, UmbraNetAdapter, UmbraNetBatch, UmbraNetSink, UmbraNetworkVTable, UmbraStr,
};
use crate::behavior::Fault;
use crate::convert::{net_batch_in, net_sink, sim_time, size_of_u32};
use crate::error::Error;
use crate::guard::with_error_sink;
use crate::minor::{NETWORK_ADAPTER_MINOR, abi_version};
use crate::network::{
    ConfigureInput, Inbox, NetworkAdapter, NetworkAdapterConfig, NetworkAdapterFactory,
};

/// The tag of factory `F`'s adapter handles.
struct NetTag<F>(PhantomData<F>);

type AdapterOf<F> = <<F as NetworkAdapterFactory>::Config as NetworkAdapterConfig>::Adapter;

/// What one network adapter handle holds. Fields drop in declaration order: the started adapter
/// before its configuration. It has no `Drop` of its own: `destroy` stops the adapter under its own
/// panic guard first, so dropping it runs only the plugin's `Drop` impls (which must not panic; one
/// panic is caught and logged, but a second while unwinding would abort the process).
struct NetState<F: NetworkAdapterFactory> {
    /// The started adapter (after a successful `start`).
    adapter: Option<AdapterOf<F>>,
    config: F::Config,
    /// `start` was called (successfully or not).
    started: bool,
    /// `stop` was called.
    stopped: bool,
    /// The tick rate `start` got (for the time of `poll`, which has no rate).
    rate_hz: u32,
}

impl<F: NetworkAdapterFactory> NetState<F> {
    /// Stops the adapter once; later calls (and a never-started handle) do nothing.
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Some(adapter) = self.adapter.as_mut() {
            adapter.stop();
        }
    }

    /// The started adapter, if `call` may use it now (after `start`, before `stop`).
    fn running(&mut self, call: &str) -> Result<&mut AdapterOf<F>, Error> {
        if self.stopped {
            return Err(Error::with_code(
                UMBRA_ERR_STATE,
                format!("{call}: after stop"),
            ));
        }
        self.adapter.as_mut().ok_or_else(|| {
            Error::with_code(
                UMBRA_ERR_STATE,
                format!("{call}: the adapter is not started"),
            )
        })
    }
}

/// The handle state behind `p`.
///
/// # Safety
/// As [`handle`].
unsafe fn state_of<'a, F: NetworkAdapterFactory>(
    p: *mut UmbraNetAdapter,
) -> Option<&'a mut NetState<F>> {
    // SAFETY: forwarded from this function's contract.
    unsafe { handle::<NetTag<F>, NetState<F>>(p.cast()) }.map(|h| &mut h.value)
}

/// The capability of network adapter factory `F`: every vtable entry is filled (`counters` reports
/// the trait's counters, all 0 by default); the metadata is empty (ABI 0.1 defines no keys).
pub(crate) fn network_cap<F: NetworkAdapterFactory>() -> Cap {
    let vtable = UmbraNetworkVTable {
        abi_version: abi_version(NETWORK_ADAPTER_MINOR),
        struct_size: size_of_u32::<UmbraNetworkVTable>(),
        configure: Some(configure::<F>),
        destroy: Some(destroy_adapter::<F>),
        key_space: Some(key_space::<F>),
        start: Some(start::<F>),
        publish: Some(publish::<F>),
        poll: Some(poll::<F>),
        stop: Some(stop::<F>),
        counters: Some(counters::<F>),
    };
    Cap {
        minor: NETWORK_ADAPTER_MINOR,
        id: F::ID,
        version: F::VERSION.unwrap_or(""),
        schema: F::PARAM_SCHEMA,
        metadata: String::new(),
        vtable: VTable::Network(vtable),
    }
}

unsafe extern "C" fn configure<F: NetworkAdapterFactory>(
    params: UmbraStr,
    out: *mut *mut UmbraNetAdapter,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the vtable contract (ADR 0033): `params`, `out` and `err` are valid for this call.
    unsafe {
        configure_into::<NetTag<F>, _, _>(params, out, err, |json| {
            let input = ConfigureInput::parse(json)?;
            Ok(NetState::<F> {
                adapter: None,
                config: F::configure(&input)?,
                started: false,
                stopped: false,
                rate_hz: 0,
            })
        })
    }
}

/// Stops a started, unstopped adapter under its own panic guard (a panic is logged), then drops the
/// handle under another; so a panicking `stop` never runs inside a `Drop` (where a second panic
/// would abort the host).
unsafe extern "C" fn destroy_adapter<F: NetworkAdapterFactory>(adapter: *mut UmbraNetAdapter) {
    let _ = without_sink("destroy", || {
        // SAFETY: the vtable contract: a live handle from `configure` (or null); the reference ends
        // with this closure.
        if let Some(state) = unsafe { state_of::<F>(adapter) } {
            state.stop();
        }
        UMBRA_OK
    });
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once.
    unsafe { destroy::<NetTag<F>, NetState<F>>(adapter.cast(), "destroy") };
}

unsafe extern "C" fn key_space<F: NetworkAdapterFactory>(
    adapter: *mut UmbraNetAdapter,
    base: *mut u64,
    count: *mut u64,
) -> UmbraCode {
    without_sink("key_space", || {
        if base.is_null() || !base.is_aligned() || count.is_null() || !count.is_aligned() {
            return UMBRA_ERR_INVALID_ARGUMENT;
        }
        // SAFETY: the vtable contract: a live handle from `configure`.
        let Some(state) = (unsafe { state_of::<F>(adapter) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        // `KeySpace::new` already refused an empty or overflowing range.
        let ks = state.config.key_space();
        // SAFETY: both out-pointers are non-null and aligned (checked) and writable for the call
        // (the vtable contract).
        unsafe {
            base.write(ks.base());
            count.write(ks.count());
        }
        UMBRA_OK
    })
}

unsafe extern "C" fn start<F: NetworkAdapterFactory>(
    adapter: *mut UmbraNetAdapter,
    tick: u64,
    rate_hz: u32,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        let now = sim_time(tick, rate_hz, "start")?;
        // SAFETY: the vtable contract: a live handle from `configure`.
        let state = unsafe { state_of::<F>(adapter) }
            .ok_or_else(|| Error::invalid("start: not an adapter of this capability"))?;
        if state.started || state.stopped {
            return Err(Error::with_code(
                UMBRA_ERR_STATE,
                "start: called after start or stop (at most once per configuration)",
            ));
        }
        state.started = true;
        state.rate_hz = rate_hz;
        let started = state.config.start(now)?;
        state.adapter = Some(started);
        Ok(())
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn publish<F: NetworkAdapterFactory>(
    adapter: *mut UmbraNetAdapter,
    batch: *const UmbraNetBatch,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `batch`, its message array and every string in it are valid for this call.
        let (now, messages) = unsafe { net_batch_in(batch) }?;
        // SAFETY: the vtable contract: a live handle from `configure`.
        let state = unsafe { state_of::<F>(adapter) }
            .ok_or_else(|| Error::invalid("publish: not an adapter of this capability"))?;
        state.running("publish")?.publish(now, &messages)
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

/// Polls into an [`Inbox`] over the host's sink, capped at `min(max, UMBRA_MAX_NET_POLL)`. The first
/// host refusal of a pushed message ends the call with the host's code, whatever the adapter returns.
unsafe extern "C" fn poll<F: NetworkAdapterFactory>(
    adapter: *mut UmbraNetAdapter,
    tick: u64,
    max: usize,
    sink: *const UmbraNetSink,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `sink` is null or the host's sink for this call.
        let sink = unsafe { net_sink(sink) }?;
        // SAFETY: the vtable contract: a live handle from `configure`.
        let state = unsafe { state_of::<F>(adapter) }
            .ok_or_else(|| Error::invalid("poll: not an adapter of this capability"))?;
        let rate_hz = state.rate_hz;
        let running = state.running("poll")?;
        // Started, so `rate_hz` is the non-zero rate `start` checked.
        let now = sim_time(tick, rate_hz, "poll")?;
        let fault = Fault::default();
        let mut inbox = Inbox::new(sink, max.min(UMBRA_MAX_NET_POLL), &fault);
        let result = running.poll(now, &mut inbox);
        if let Some((code, call)) = fault.get() {
            return Err(Error::with_code(
                code,
                format!("{call} refused by the host with code {code}"),
            ));
        }
        result
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn stop<F: NetworkAdapterFactory>(adapter: *mut UmbraNetAdapter) -> UmbraCode {
    without_sink("stop", || {
        // SAFETY: the vtable contract: a live handle from `configure`.
        match unsafe { state_of::<F>(adapter) } {
            Some(state) => {
                state.stop();
                UMBRA_OK
            }
            None => UMBRA_ERR_INVALID_ARGUMENT,
        }
    })
}

/// Writes `min(n, 4)` counters (the four `Counters` knows); before `start` nothing is written (the
/// host zero-filled `out`).
unsafe extern "C" fn counters<F: NetworkAdapterFactory>(
    adapter: *mut UmbraNetAdapter,
    out: *mut u64,
    n: usize,
) -> UmbraCode {
    without_sink("counters", || {
        // SAFETY: the vtable contract: a live handle from `configure`.
        let Some(state) = (unsafe { state_of::<F>(adapter) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        let Some(adapter) = state.adapter.as_ref() else {
            return UMBRA_OK;
        };
        let values = adapter.counters().in_abi_order();
        let k = n.min(values.len());
        if k == 0 {
            return UMBRA_OK;
        }
        if out.is_null() || !out.is_aligned() {
            return UMBRA_ERR_INVALID_ARGUMENT;
        }
        for (i, v) in values.iter().take(k).enumerate() {
            // SAFETY: `out` is non-null and aligned (checked) and writable for `n >= k > i` entries
            // during the call (the vtable contract).
            unsafe { out.add(i).write(*v) };
        }
        UMBRA_OK
    })
}
