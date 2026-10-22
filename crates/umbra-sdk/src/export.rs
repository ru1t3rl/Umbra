//! `export_plugin!` and the machinery behind it (REQ-P-1, REQ-P-3, REQ-P-6, REQ-P-7, REQ-SDK-1,
//! ADR 0019, ADR 0020, ADR 0021).
//!
//! The macro emits one item, `pub unsafe extern "C" fn umbra_plugin_entry`, whose body is one
//! `unsafe` block calling the `unsafe fn` [`__private::entry`]; because both come from this crate's
//! macro, plugin crates keep `#![forbid(unsafe_code)]` (ASM-116, amended 2026-10-19 after the WI-7
//! review). Everything that needs `unsafe` lives here: the static descriptor and its capabilities and
//! vtables, and the generic `extern "C"` trampolines that turn each vtable call into a call of the safe
//! traits. Every trampoline runs its body under a panic guard (`guard::catch`) and validates every
//! input before any trait call. Every struct the SDK writes carries the minor of the capability kind it
//! belongs to (ADR 0033, `minor.rs`): a capability, its vtable and the structs of its calls the kind's
//! minor (behavior 0, network adapter 1), the descriptor the highest of its capabilities' minors, so a
//! behavior-only plugin built with this SDK still loads and runs in an ABI 0.0 host, and only a plugin
//! with a network adapter needs an ABI 0.1 host. The network adapter trampolines are in the child
//! module `network`.
//!
//! Handles: a configured behavior and an instance are each one `Box<Handle<T>>`, whose first field is
//! the `TypeId` of a per-capability tag. Every call checks the tag before touching the value, so a
//! handle of another capability, or a behavior passed as an instance, is refused
//! (`UMBRA_ERR_INVALID_ARGUMENT`) instead of being reinterpreted. Panics in calls without an error
//! sink (`halt`, `halt_running`, `output`, `destroy_*`, `shutdown`) are logged at error level through
//! the host API, so their message is not lost.
#![expect(unsafe_code, reason = "the C side of the plugin ABI (ADR 0019)")]

use std::any::TypeId;
use std::marker::PhantomData;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::abi::{
    UMBRA_CAPABILITY_BEHAVIOR, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_PANIC, UMBRA_OK,
    UmbraBehavior, UmbraBehaviorBatch, UmbraBehaviorVTable, UmbraCapability, UmbraCapabilityKind,
    UmbraCode, UmbraErrorSink, UmbraHostApi, UmbraInstance, UmbraNetworkVTable,
    UmbraPluginDescriptor, UmbraStr, UmbraValue, UmbraValueSink,
};
use crate::behavior::{
    Behavior, BehaviorFactory, BehaviorInstance, Contact, EntityView, Fault, Intents, Level,
    Params, TickContext,
};
use crate::convert::{batch_in, size_of_u32, str_from, umbra_str, value_in, value_sink, view_in};
use crate::error::Error;
use crate::guard::{catch, with_error_sink};
use crate::minor::{BEHAVIOR_MINOR, abi_version};

/// Support items the macro expands to; not public API.
#[doc(hidden)]
pub mod __private {
    use std::any::TypeId;

    use super::{Built, Cap, DESCRIPTOR, behavior_cap, network::network_cap};
    use crate::abi::{UmbraHostApi, UmbraPluginDescriptor};
    use crate::behavior::BehaviorFactory;
    use crate::error::Error;
    use crate::guard::catch;
    use crate::leaf::{Action, Condition};
    use crate::motion::MotionModel;
    use crate::network::NetworkAdapterFactory;
    use crate::sensor::Sensor;

    /// What `export_plugin!` declares about a plugin; implemented by the marker type the macro
    /// generates inside `umbra_plugin_entry`.
    pub trait Plugin: 'static {
        /// Plugin ID (`PluginId` grammar, ADR 0002).
        const ID: &'static str;
        /// Human-readable name.
        const NAME: &'static str;
        /// Semantic version.
        const VERSION: &'static str;
        /// Adds every capability, in declaration order.
        fn capabilities(caps: &mut Capabilities);
        /// The plugin's `init` (after the SDK stored the host API); default: nothing to do.
        ///
        /// # Errors
        /// The plugin cannot start; the load fails with this message.
        fn init() -> Result<(), Error> {
            Ok(())
        }
        /// The plugin's `shutdown`; default: nothing to do.
        fn shutdown() {}
    }

    /// The capability list a [`Plugin`] fills.
    #[derive(Default)]
    pub struct Capabilities {
        pub(super) caps: Vec<Cap>,
    }

    impl Capabilities {
        /// Adds the behavior capability of factory `F`.
        pub fn behavior<F: BehaviorFactory>(&mut self) {
            self.caps.push(behavior_cap::<F>());
        }

        /// Adds the network adapter capability of factory `F` (ABI 0.1, ADR 0033).
        pub fn network_adapter<F: NetworkAdapterFactory>(&mut self) {
            self.caps.push(network_cap::<F>());
        }

        /// Adds the motion model capability `M` (ABI 0.2, ADR 0050). M7 WI-2 stub.
        pub fn motion_model<M: MotionModel>(&mut self) {
            unimplemented!("M7 WI-10")
        }

        /// Adds the sensor capability `S` (ABI 0.2, ADR 0050). M7 WI-2 stub.
        pub fn sensor<S: Sensor>(&mut self) {
            unimplemented!("M7 WI-10")
        }

        /// Adds the condition capability `C` (ABI 0.2, ADR 0050). M7 WI-2 stub.
        pub fn condition<C: Condition>(&mut self) {
            unimplemented!("M7 WI-10")
        }

        /// Adds the action capability `A` (ABI 0.2, ADR 0050). M7 WI-2 stub.
        pub fn action<A: Action>(&mut self) {
            unimplemented!("M7 WI-10")
        }
    }

    /// A descriptor of `P` built like the one the entry returns, but owned by the caller and not
    /// stored: the entry keeps one descriptor per binary, so a test that inspects the descriptors of
    /// several plugin declarations (the required-minor rule, ADR 0033) builds them with this. The
    /// vtables call the same trampolines as the entry's. Not public API.
    pub struct OwnedDescriptor(Built);

    impl OwnedDescriptor {
        /// Builds `P`'s descriptor (a panic in a capability's declaration propagates).
        #[must_use]
        pub fn build<P: Plugin>() -> Self {
            Self(Built::new::<P>())
        }

        /// The descriptor; its pointers stay valid while `self` lives.
        #[must_use]
        pub fn descriptor(&self) -> &UmbraPluginDescriptor {
            &self.0.descriptor
        }
    }

    /// The body of the generated `umbra_plugin_entry` (REQ-P-1): validates the host API and stores it
    /// if this is the first entry call of a lifecycle, builds the static descriptor on the first call
    /// and returns it; every call returns the same pointer. Null when `host` is malformed, when
    /// building the descriptor panics (R-M1), or when another `Plugin` already built this binary's
    /// descriptor.
    ///
    /// # Safety
    /// `host` is null or points at memory readable for at least 8 bytes and, if its header passes
    /// the ADR 0019 checks, for `struct_size` bytes: the C contract of `umbra_plugin_entry`
    /// (ADR 0021, the host's process-lifetime `UmbraHostApi`). Only the host should call it.
    #[must_use]
    pub unsafe fn entry<P: Plugin>(host: *const UmbraHostApi) -> *const UmbraPluginDescriptor {
        let found = catch(|| {
            // SAFETY: forwarded from this function's contract; `read` checks null, alignment and
            // the header before reading the rest.
            let copy = unsafe { crate::host::read(host) }?;
            let built = DESCRIPTOR.get_or_init(Built::new::<P>);
            if built.plugin != TypeId::of::<P>() {
                return None;
            }
            crate::host::store_if_empty(copy);
            Some(&raw const built.descriptor)
        });
        found.ok().flatten().unwrap_or(std::ptr::null())
    }
}

use __private::{Capabilities, Plugin};

// ---------------------------------------------------------------- the static descriptor

/// The vtable of one capability, by kind.
pub(crate) enum VTable {
    /// `UMBRA_CAPABILITY_BEHAVIOR`.
    Behavior(UmbraBehaviorVTable),
    /// `UMBRA_CAPABILITY_NETWORK_ADAPTER` (ABI 0.1).
    Network(UmbraNetworkVTable),
}

impl VTable {
    /// The capability kind of this vtable.
    fn kind(&self) -> UmbraCapabilityKind {
        match self {
            Self::Behavior(_) => UMBRA_CAPABILITY_BEHAVIOR,
            Self::Network(_) => crate::abi::UMBRA_CAPABILITY_NETWORK_ADAPTER,
        }
    }

    /// The address of the vtable struct (stable while `self` does not move).
    fn as_ptr(&self) -> *const std::ffi::c_void {
        match self {
            Self::Behavior(v) => std::ptr::from_ref(v).cast(),
            Self::Network(v) => std::ptr::from_ref(v).cast(),
        }
    }
}

/// One capability before it is laid out: its strings, its vtable and the minor of its kind.
pub(crate) struct Cap {
    minor: u32,
    id: &'static str,
    version: &'static str,
    schema: &'static str,
    metadata: String,
    vtable: VTable,
}

/// The descriptor and everything it points to; built once per binary, never mutated or freed.
pub(crate) struct Built {
    /// The `Plugin` that built it.
    plugin: TypeId,
    descriptor: UmbraPluginDescriptor,
    // The arrays and texts the descriptor points into (heap blocks: stable when `Built` moves).
    _capabilities: Box<[UmbraCapability]>,
    _vtables: Box<[VTable]>,
    _metadata: Box<[String]>,
}

// SAFETY: `Built` holds raw pointers only into its own boxed slices and strings and into `'static`
// strings; none of them is ever written after `Built::new` returns, so shared access from any thread
// reads immutable data. The function pointers are plain code addresses.
unsafe impl Sync for Built {}
// SAFETY: as for `Sync`: no thread-affine state, the pointed-to data is owned by `Built` or static.
unsafe impl Send for Built {}

/// This binary's descriptor (one `export_plugin!` per binary: `umbra_plugin_entry` is one symbol).
static DESCRIPTOR: OnceLock<Built> = OnceLock::new();

impl Built {
    fn new<P: Plugin>() -> Self {
        let mut list = Capabilities::default();
        P::capabilities(&mut list);
        let (mut vtables, mut metadata, mut texts) = (Vec::new(), Vec::new(), Vec::new());
        // ADR 0033: the descriptor declares the lowest minor that defines every exported kind.
        let minor = list.caps.iter().map(|cap| cap.minor).max().unwrap_or(0);
        for cap in list.caps {
            vtables.push(cap.vtable);
            metadata.push(cap.metadata);
            texts.push((cap.minor, cap.id, cap.version, cap.schema));
        }
        let vtables = vtables.into_boxed_slice();
        let metadata = metadata.into_boxed_slice();
        let capabilities: Box<[UmbraCapability]> = texts
            .iter()
            .zip(vtables.iter().zip(metadata.iter()))
            .map(
                |(&(minor, id, version, schema), (vtable, meta))| UmbraCapability {
                    abi_version: abi_version(minor),
                    struct_size: size_of_u32::<UmbraCapability>(),
                    kind: vtable.kind(),
                    reserved_0: 0,
                    id: umbra_str(id),
                    version: umbra_str(version),
                    param_schema: umbra_str(schema),
                    metadata: umbra_str(meta),
                    vtable: vtable.as_ptr(),
                },
            )
            .collect();
        let descriptor = UmbraPluginDescriptor {
            abi_version: abi_version(minor),
            struct_size: size_of_u32::<UmbraPluginDescriptor>(),
            id: umbra_str(P::ID),
            name: umbra_str(P::NAME),
            version: umbra_str(P::VERSION),
            capabilities: if capabilities.is_empty() {
                std::ptr::null()
            } else {
                capabilities.as_ptr()
            },
            capability_count: capabilities.len(),
            init: Some(init::<P>),
            shutdown: Some(shutdown::<P>),
        };
        Self {
            plugin: TypeId::of::<P>(),
            descriptor,
            _capabilities: capabilities,
            _vtables: vtables,
            _metadata: metadata,
        }
    }
}

/// The capability of behavior factory `F`: `set_input` only with declared params, `output` only with
/// declared outputs (ADR 0020); `halt_running` always (its trait default is `halt`).
fn behavior_cap<F: BehaviorFactory>() -> Cap {
    let interface = F::interface();
    let vtable = UmbraBehaviorVTable {
        abi_version: abi_version(BEHAVIOR_MINOR),
        struct_size: size_of_u32::<UmbraBehaviorVTable>(),
        configure: Some(configure::<F>),
        destroy_behavior: Some(destroy_behavior::<F>),
        instantiate: Some(instantiate::<F>),
        destroy_instance: Some(destroy_instance::<F>),
        tick_batch: Some(tick_batch::<F>),
        halt: Some(halt::<F>),
        halt_running: Some(halt_running::<F>),
        set_input: if interface.has_params() {
            Some(set_input::<F>)
        } else {
            None
        },
        output: if interface.has_outputs() {
            Some(output::<F>)
        } else {
            None
        },
    };
    Cap {
        minor: BEHAVIOR_MINOR,
        id: F::ID,
        version: F::VERSION.unwrap_or(""),
        schema: F::PARAM_SCHEMA,
        metadata: interface.metadata(),
        vtable: VTable::Behavior(vtable),
    }
}

// ---------------------------------------------------------------- lifecycle

/// `init`: stores the host API again (a new lifecycle resets the SDK's global state), then runs the
/// plugin's `init`.
unsafe extern "C" fn init<P: Plugin>(
    host: *const UmbraHostApi,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: the `init` contract: `host` is the entry's host API, readable for the process.
        let copy = unsafe { crate::host::read(host) }
            .ok_or_else(|| Error::invalid("init: malformed host API"))?;
        crate::host::store(Some(copy));
        P::init()
    };
    // SAFETY: the `init` contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

/// `shutdown`: runs the plugin's `shutdown` (a panic is logged, never propagated), then forgets the
/// host API, so `log` is a no-op until the next lifecycle.
unsafe extern "C" fn shutdown<P: Plugin>() {
    let _ = catch(|| {
        if let Err(panic) = catch(P::shutdown) {
            crate::host::log(Level::Error, &format!("shutdown: {panic}"));
        }
        crate::host::store(None);
    });
}

// ---------------------------------------------------------------- handles

/// The tag of factory `F`'s configured behaviors.
struct BehaviorTag<F>(PhantomData<F>);
/// The tag of factory `F`'s instances.
struct InstanceTag<F>(PhantomData<F>);

type InstanceOf<F> = <<F as BehaviorFactory>::Behavior as Behavior>::Instance;

/// A plugin-owned handle: the tag first (`repr(C)`: offset 0 in every handle type), then the value.
#[repr(C)]
struct Handle<T> {
    tag: TypeId,
    value: T,
}

/// A new handle for `value`, tagged with `Tag`.
fn new_handle<Tag: 'static, T>(value: T) -> *mut Handle<T> {
    Box::into_raw(Box::new(Handle {
        tag: TypeId::of::<Tag>(),
        value,
    }))
}

/// `p` as a handle of type `T` tagged `Tag`, or `None` if it is null, misaligned or tagged otherwise.
///
/// # Safety
/// `p` is null or a live handle created by this SDK (any `Handle<U>`) and not yet destroyed, which the
/// host guarantees for every handle it passes (ADR 0020), and no other reference to it is live.
unsafe fn handle<'a, Tag: 'static, T>(p: *mut Handle<T>) -> Option<&'a mut Handle<T>> {
    if p.is_null() || !p.cast::<TypeId>().is_aligned() {
        return None;
    }
    // SAFETY: a live SDK handle of any type starts with its `TypeId` tag at offset 0 (`repr(C)`), and
    // the pointer is aligned for `TypeId`; reading it reads initialised memory of that handle.
    let tag = unsafe { p.cast::<TypeId>().read() };
    if tag != TypeId::of::<Tag>() || !p.is_aligned() {
        return None;
    }
    // SAFETY: the tag proves `p` was made by `new_handle::<Tag, T>`, so it points at a live, aligned
    // `Handle<T>`. The host serialises every call into one image with its blocking per-image call
    // lock (ADR 0019, RR-B1) and passes each handle to one call at a time, and the SDK makes at most
    // one reference per handle per call, so this is the only reference while it lives.
    Some(unsafe { &mut *p })
}

/// The configured behavior behind `p`.
///
/// # Safety
/// As [`handle`].
unsafe fn behavior_of<'a, F: BehaviorFactory>(p: *mut UmbraBehavior) -> Option<&'a F::Behavior> {
    // SAFETY: forwarded from this function's contract.
    unsafe { handle::<BehaviorTag<F>, F::Behavior>(p.cast()) }.map(|h| &h.value)
}

/// The instance behind `p`.
///
/// # Safety
/// As [`handle`].
unsafe fn instance_of<'a, F: BehaviorFactory>(
    p: *mut UmbraInstance,
) -> Option<&'a mut InstanceOf<F>> {
    // SAFETY: forwarded from this function's contract.
    unsafe { handle::<InstanceTag<F>, InstanceOf<F>>(p.cast()) }.map(|h| &mut h.value)
}

/// Writes `value` to the out-pointer of `configure`/`instantiate`.
///
/// # Safety
/// `out` is null or points at writable memory for one pointer for the call.
unsafe fn write_out<T>(out: *mut *mut T, value: *mut T) -> Result<(), Error> {
    if out.is_null() || !out.is_aligned() {
        return Err(Error::invalid("out: null or misaligned"));
    }
    // SAFETY: non-null and aligned; writable for the call by this function's contract.
    unsafe { out.write(value) };
    Ok(())
}

/// Destroys the handle `p` (null is a no-op); a handle of another capability is left alone and
/// logged; a panic in the value's `Drop` is logged.
///
/// # Safety
/// As [`handle`], and `p` is not used again.
unsafe fn destroy<Tag: 'static, T>(p: *mut Handle<T>, call: &str) {
    if p.is_null() {
        return;
    }
    let _ = catch(|| {
        // SAFETY: forwarded from this function's contract.
        let Some(h) = (unsafe { handle::<Tag, T>(p) }) else {
            crate::host::log(
                Level::Error,
                &format!("{call}: not a handle of this capability"),
            );
            return;
        };
        // SAFETY: `h` is a live `Box<Handle<T>>` made by `new_handle` and destroyed only here, once
        // (the host's contract); the reference `h` is not used after this point.
        let owned = unsafe { Box::from_raw(std::ptr::from_mut(h)) };
        if let Err(panic) = catch(move || drop(owned)) {
            crate::host::log(Level::Error, &format!("{call}: {panic}"));
        }
    });
}

/// Runs a call without an error sink: a panic is logged at error level and is `UMBRA_ERR_PANIC`.
fn without_sink(call: &str, body: impl FnOnce() -> UmbraCode) -> UmbraCode {
    catch(body).unwrap_or_else(|panic| {
        let _ = catch(|| crate::host::log(Level::Error, &format!("{call}: {panic}")));
        UMBRA_ERR_PANIC
    })
}

// ---------------------------------------------------------------- the behavior vtable

unsafe extern "C" fn configure<F: BehaviorFactory>(
    params: UmbraStr,
    out: *mut *mut UmbraBehavior,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `out` is the call's out-pointer.
        unsafe { write_out(out, std::ptr::null_mut()) }?;
        // SAFETY: `params` is borrowed for the call.
        let json = unsafe { str_from(params) }
            .ok_or_else(|| Error::invalid("configure: params are not UTF-8"))?;
        let behavior = F::configure(&Params::new(json))?;
        let handle = new_handle::<BehaviorTag<F>, _>(behavior).cast::<UmbraBehavior>();
        // SAFETY: as above (checked by the first write).
        unsafe { write_out(out, handle) }
    };
    // SAFETY: the vtable contract (ADR 0020): `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn destroy_behavior<F: BehaviorFactory>(behavior: *mut UmbraBehavior) {
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once, after its instances.
    unsafe { destroy::<BehaviorTag<F>, F::Behavior>(behavior.cast(), "destroy_behavior") };
}

unsafe extern "C" fn instantiate<F: BehaviorFactory>(
    behavior: *mut UmbraBehavior,
    rate_hz: u32,
    out: *mut *mut UmbraInstance,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `out` is the call's out-pointer.
        unsafe { write_out(out, std::ptr::null_mut()) }?;
        // SAFETY: `behavior` is a live handle from `configure` (the host's contract).
        let behavior = unsafe { behavior_of::<F>(behavior) }
            .ok_or_else(|| Error::invalid("instantiate: not a behavior of this capability"))?;
        let instance = behavior.instantiate(rate_hz)?;
        let handle = new_handle::<InstanceTag<F>, _>(instance).cast::<UmbraInstance>();
        // SAFETY: as above.
        unsafe { write_out(out, handle) }
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn destroy_instance<F: BehaviorFactory>(instance: *mut UmbraInstance) {
    // SAFETY: the vtable contract: a handle from `instantiate`, destroyed once.
    unsafe { destroy::<InstanceTag<F>, InstanceOf<F>>(instance.cast(), "destroy_instance") };
}

/// Ticks every item in order (REQ-B-4: batching is at the boundary). Pass 1 validates the whole batch
/// and every handle; pass 2 ticks and writes one status per item. The first host refusal (RNG draw or
/// intent) or panic ends the call with its code; the host then discards the whole batch (ADR 0020).
unsafe extern "C" fn tick_batch<F: BehaviorFactory>(
    batch: *const UmbraBehaviorBatch,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `batch` and everything it points to are valid for this call (ADR 0020).
        let b = unsafe { batch_in(batch) }?;
        for (i, item) in b.items.iter().enumerate() {
            // SAFETY: instances in a batch are live handles from `instantiate`; the reference
            // made by the check ends at once.
            if unsafe { instance_of::<F>(item.instance) }.is_none() {
                return Err(Error::invalid(format!(
                    "batch.items[{i}].instance: not an instance of this capability"
                )));
            }
        }
        let host = crate::host::current();
        let mut contacts = take_contacts();
        for (i, item) in b.items.iter().enumerate() {
            // SAFETY: the view was validated in pass 1 and stays valid for the call.
            let view = unsafe { view_in(item.entity) }?;
            view.contacts_into(&mut contacts);
            let entity = EntityView::new(&view, &contacts);
            let fault = Fault::default();
            let mut ctx = TickContext::new(b.tick, b.rate_hz, host, item.rng, &fault);
            let mut out = Intents::new(b.sink, i, &fault);
            // SAFETY: checked in pass 1; the only reference to this instance during its tick.
            let instance = unsafe { instance_of::<F>(item.instance) }
                .ok_or_else(|| Error::invalid("batch item instance"))?;
            let status = instance.tick(&mut ctx, &entity, &mut out);
            if let Some((code, call)) = fault.get() {
                return Err(Error::with_code(
                    code,
                    format!("{call} refused by the host with code {code} (item {i})"),
                ));
            }
            b.set_status(i, status);
        }
        put_contacts(contacts);
        Ok(())
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

/// The contacts scratch buffer, kept between `tick_batch` calls so a warm plugin does not allocate
/// per call (review of WI-7). A `static`, not a thread-local: a thread-local with a destructor in a
/// plugin `cdylib` is the `dlclose` risk the M4 plan lists. Uncontended, because the host serialises
/// calls into one image (RR-B1), and never locked while plugin code runs. Stored typed `'static` and
/// always empty; [`relife`] re-types it for one call. A call that ends early (fault or panic) drops
/// its buffer and the next call starts a new one.
static CONTACTS: Mutex<Vec<Contact<'static>>> = Mutex::new(Vec::new());

fn take_contacts<'a>() -> Vec<Contact<'a>> {
    relife(std::mem::take(
        &mut *CONTACTS.lock().unwrap_or_else(PoisonError::into_inner),
    ))
}

fn put_contacts(contacts: Vec<Contact<'_>>) {
    *CONTACTS.lock().unwrap_or_else(PoisonError::into_inner) = relife(contacts);
}

/// Empties `v` and changes the lifetime of its element type. `Contact<'a>` and `Contact<'b>` have one
/// layout, so the in-place `collect` keeps the allocation (current std behavior, not a documented
/// guarantee: the same precedent and the same kind of guard test as the kernel's `relife`, ASM-126 b).
/// If std ever stops doing this, the buffer is just reallocated: correctness does not depend on it.
#[expect(
    clippy::unnecessary_filter_map,
    reason = "dropping every element is the point: the in-place collect re-types the empty buffer"
)]
fn relife<'b>(mut v: Vec<Contact<'_>>) -> Vec<Contact<'b>> {
    v.clear();
    v.into_iter().filter_map(|_| None).collect()
}

unsafe extern "C" fn halt<F: BehaviorFactory>(instance: *mut UmbraInstance) -> UmbraCode {
    without_sink("halt", || {
        // SAFETY: the vtable contract: a live handle from `instantiate`.
        match unsafe { instance_of::<F>(instance) } {
            Some(i) => {
                i.halt();
                UMBRA_OK
            }
            None => UMBRA_ERR_INVALID_ARGUMENT,
        }
    })
}

unsafe extern "C" fn halt_running<F: BehaviorFactory>(instance: *mut UmbraInstance) -> UmbraCode {
    without_sink("halt_running", || {
        // SAFETY: the vtable contract: a live handle from `instantiate`.
        match unsafe { instance_of::<F>(instance) } {
            Some(i) => {
                i.halt_running();
                UMBRA_OK
            }
            None => UMBRA_ERR_INVALID_ARGUMENT,
        }
    })
}

unsafe extern "C" fn set_input<F: BehaviorFactory>(
    instance: *mut UmbraInstance,
    param: UmbraStr,
    value: *const UmbraValue,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: `param` is borrowed for the call.
        let param = unsafe { str_from(param) }
            .ok_or_else(|| Error::invalid("set_input: param is not UTF-8"))?;
        // SAFETY: `value` is null or a host-written value valid for the call.
        let value = unsafe { value_in(value) }?;
        // SAFETY: the vtable contract: a live handle from `instantiate`.
        let instance = unsafe { instance_of::<F>(instance) }
            .ok_or_else(|| Error::invalid("set_input: not an instance of this capability"))?;
        instance.set_input(param, value)
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}

unsafe extern "C" fn output<F: BehaviorFactory>(
    instance: *mut UmbraInstance,
    port: UmbraStr,
    sink: *const UmbraValueSink,
) -> UmbraCode {
    without_sink("output", || {
        // SAFETY: the vtable contract: `sink` is null or a sink valid for this call.
        let Some(sink) = (unsafe { value_sink(sink) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        // SAFETY: `port` is borrowed for the call.
        let Some(port) = (unsafe { str_from(port) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        // SAFETY: the vtable contract: a live handle from `instantiate`.
        let Some(instance) = (unsafe { instance_of::<F>(instance) }) else {
            return UMBRA_ERR_INVALID_ARGUMENT;
        };
        sink.set(instance.output(port).as_ref())
    })
}

// ---------------------------------------------------------------- the network adapter vtable

mod network;

// ---------------------------------------------------------------- the macro

/// Exports a plugin (REQ-P-1): generates exactly one item, `#[unsafe(no_mangle)] pub unsafe extern
/// "C" fn umbra_plugin_entry`, whose body is one `unsafe` block calling the SDK, and no lint
/// attribute. The `unsafe` comes from this external macro, so the plugin crate keeps
/// `#![forbid(unsafe_code)]` (ASM-116, amended 2026-10-19 after the WI-7 review; pinned by
/// `xtask/tests/lint_facts.rs`). A private marker type inside the function lists the factories;
/// the descriptor, the vtables and every other `unsafe` stay in the SDK.
///
/// `init` (optional) is a `fn() -> Result<(), umbra_sdk::Error>` run once per lifecycle after the SDK
/// stored the host API; reset any global state there (ADR 0021). `shutdown` (optional) is a `fn()`
/// run once at the end of a lifecycle in which `init` succeeded; `log` still works inside it.
///
/// `behaviors` and `network_adapters` (ABI 0.1, ADR 0033) are each optional, in this order; the
/// capabilities are declared in list order, behaviors first. The descriptor declares the lowest ABI
/// minor that defines every listed kind: 0 for behaviors only, 1 with a network adapter.
///
/// Syntax (a complete plugin is in the crate docs):
///
/// ```text
/// umbra_sdk::export_plugin! {
///     id: "example.plugin.wander",
///     name: "Wander example",
///     version: "0.1.0",
///     behaviors: [WanderFactory],          // optional
///     network_adapters: [MyNetFactory],    // optional (ABI 0.1)
///     motion_models: [MyMotion],           // optional (ABI 0.2, ADR 0050)
///     sensors: [MySensor],                 // optional (ABI 0.2)
///     conditions: [MyCondition],           // optional (ABI 0.2)
///     actions: [MyAction],                 // optional (ABI 0.2)
///     init: my_init,                       // optional
///     shutdown: my_shutdown,               // optional
/// }
/// ```
#[macro_export]
macro_rules! export_plugin {
    (
        id: $id:literal, name: $name:literal, version: $version:literal
        $(, behaviors: [$($factory:ty),* $(,)?])?
        $(, network_adapters: [$($net:ty),* $(,)?])?
        $(, motion_models: [$($motion:ty),* $(,)?])?
        $(, sensors: [$($sensor:ty),* $(,)?])?
        $(, conditions: [$($condition:ty),* $(,)?])?
        $(, actions: [$($action:ty),* $(,)?])?
        $(, init: $init:path)? $(, shutdown: $shutdown:path)? $(,)?
    ) => {
        /// The plugin entry point (REQ-P-1), generated by `umbra_sdk::export_plugin!`.
        ///
        /// # Safety
        /// Called by the Umbra host only: `host` is null or points at its `UmbraHostApi` (ADR 0021).
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn umbra_plugin_entry(
            host: *const $crate::abi::UmbraHostApi,
        ) -> *const $crate::abi::UmbraPluginDescriptor {
            struct __UmbraPlugin;
            impl $crate::__private::Plugin for __UmbraPlugin {
                const ID: &'static str = $id;
                const NAME: &'static str = $name;
                const VERSION: &'static str = $version;
                fn capabilities(caps: &mut $crate::__private::Capabilities) {
                    let _ = &caps;
                    $($(caps.behavior::<$factory>();)*)?
                    $($(caps.network_adapter::<$net>();)*)?
                    $($(caps.motion_model::<$motion>();)*)?
                    $($(caps.sensor::<$sensor>();)*)?
                    $($(caps.condition::<$condition>();)*)?
                    $($(caps.action::<$action>();)*)?
                }
                $(fn init() -> ::core::result::Result<(), $crate::Error> {
                    $init()
                })?
                $(fn shutdown() {
                    $shutdown()
                })?
            }
            // SAFETY: `host` is passed on unchanged under this function's own contract, which is
            // `__private::entry`'s.
            unsafe { $crate::__private::entry::<__UmbraPlugin>(host) }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{put_contacts, relife, take_contacts};
    use crate::behavior::Contact;

    fn contact(force: &str) -> Contact<'_> {
        Contact {
            entity: 1,
            position: [0.0; 3],
            velocity: [0.0; 3],
            force: Some(force),
            range_m: 1.0,
        }
    }

    /// The guard of the contacts scratch buffer (review of WI-7): `relife` empties the vector and keeps
    /// its allocation, and the buffer survives a `take`/`put` round.
    #[test]
    fn relife_keeps_the_allocation_and_the_scratch_is_reused() {
        let text = String::from("blue");
        let mut v = Vec::with_capacity(16);
        v.push(contact(&text));
        let (ptr, cap) = (v.as_ptr().cast::<u8>(), v.capacity());

        let back: Vec<Contact<'static>> = relife(v);

        assert_eq!(back.len(), 0);
        assert_eq!((back.as_ptr().cast::<u8>(), back.capacity()), (ptr, cap));

        put_contacts(back);
        let again = take_contacts();
        assert_eq!((again.as_ptr().cast::<u8>(), again.capacity()), (ptr, cap));
        put_contacts(again);
    }
}
