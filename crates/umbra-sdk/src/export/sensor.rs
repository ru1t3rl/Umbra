//! The sensor capability of `export_plugin!` (ABI 0.2, ADR 0050): the vtable and its `extern "C"`
//! trampolines over [`Sensor`].
//!
//! One `configure` makes one plugin-owned handle, a `Box<Handle<S>>` tagged per capability. `sense`
//! validates the whole call record (observer, candidates, terrain service) and the contact sink before
//! the trait runs; contacts go to the host as they are pushed, in the sensor kind's minor. The first
//! host refusal (a contact, an RNG draw, a terrain query) ends the call with the host's code, whatever
//! the sensor returns, and the host discards the call's contacts whole (ADR 0050).
#![expect(unsafe_code, reason = "the sensor vtable of the plugin ABI (ADR 0050)")]

use std::marker::PhantomData;

use super::{Cap, VTable, configure_into, destroy, handle, host_refusal};
use crate::abi::{
    UmbraCode, UmbraContactSink, UmbraErrorSink, UmbraSenseCall, UmbraSensor, UmbraSensorVTable,
    UmbraStr,
};
use crate::behavior::Fault;
use crate::convert::{ContactSink, sense_in, size_of_u32};
use crate::error::Error;
use crate::guard::with_error_sink;
use crate::minor::{SENSOR_MINOR, abi_version};
use crate::sensor::{Contacts, SenseCtx, Sensor};

/// The tag of sensor `S`'s handles.
struct SensorTag<S>(PhantomData<S>);

/// The capability of sensor `S`: every vtable entry is filled; the metadata is empty (ADR 0050).
pub(crate) fn sensor_cap<S: Sensor>() -> Cap {
    let vtable = UmbraSensorVTable {
        abi_version: abi_version(SENSOR_MINOR),
        struct_size: size_of_u32::<UmbraSensorVTable>(),
        configure: Some(configure::<S>),
        destroy: Some(destroy_sensor::<S>),
        sense: Some(sense::<S>),
    };
    Cap {
        minor: SENSOR_MINOR,
        id: S::ID,
        version: S::VERSION.unwrap_or(""),
        schema: S::SCHEMA,
        metadata: String::new(),
        vtable: VTable::Sensor(vtable),
    }
}

unsafe extern "C" fn configure<S: Sensor>(
    params: UmbraStr,
    out: *mut *mut UmbraSensor,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    // SAFETY: the vtable contract (ADR 0050): `params`, `out` and `err` are valid for this call.
    unsafe { configure_into::<SensorTag<S>, _, _>(params, out, err, S::configure) }
}

unsafe extern "C" fn destroy_sensor<S: Sensor>(sensor: *mut UmbraSensor) {
    // SAFETY: the vtable contract: a handle from `configure`, destroyed once.
    unsafe { destroy::<SensorTag<S>, S>(sensor.cast(), "destroy") };
}

unsafe extern "C" fn sense<S: Sensor>(
    sensor: *mut UmbraSensor,
    call: *const UmbraSenseCall,
    sink: *const UmbraContactSink,
    err: *const UmbraErrorSink,
) -> UmbraCode {
    let body = || {
        // SAFETY: the vtable contract: `call` and everything it points to are valid for this call.
        let c = unsafe { sense_in(call) }?;
        // SAFETY: the vtable contract: `sink` is the host's sink for this call.
        let sink = unsafe { ContactSink::from_raw(sink) }?;
        // SAFETY: the vtable contract: a live handle from `configure`; the reference ends with the call.
        let sensor = unsafe { handle::<SensorTag<S>, S>(sensor.cast()) }
            .ok_or_else(|| Error::invalid("sense: not a sensor of this capability"))?;
        let fault = Fault::default();
        let mut ctx = SenseCtx::new(
            c.tick,
            c.rate_hz,
            crate::host::current(),
            c.rng,
            c.terrain,
            &fault,
        );
        let mut out = Contacts::new(sink, &fault);
        let result = sensor
            .value
            .sense(&mut ctx, &c.observer, &c.candidates, &mut out);
        host_refusal(&fault).map_or(result, Err)
    };
    // SAFETY: the vtable contract: `err` is null or a sink valid for this call.
    unsafe { with_error_sink(err, body) }
}
