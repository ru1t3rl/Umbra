//! The sensor bridge (ABI 0.2; REQ-P-4, REQ-P-5, REQ-P-7, REQ-B-5, REQ-K-4, REQ-C-6, D-8, ADR 0015,
//! ADR 0021, ADR 0050, ASM-612, ASM-613, ASM-615, ASM-617): a plugin sensor capability as a core
//! `Factory<dyn Sensor>`, and its configured sensor as a core [`Sensor`], used by the kernel exactly
//! like a built-in.
//!
//! **Lifecycle.** `build` is the plugin's `configure` ([`configure_handle`]); every built sensor owns
//! one plugin handle, destroyed (`destroy`) when the sensor drops, also for a faulted capability. The
//! sensor holds its capability and so its image.
//!
//! **A sense call** is one call per observer and sensor (not batched, ASM-612) under the image's
//! blocking call lock: the observer (`UMBRA_OWNERSHIP_OWNED`: the kernel senses for owned entities
//! only) and every candidate in the kernel's order with their pre-tick state are written in the
//! plugin's declared minor (O(n) per call, ASM-613), with the observer's RNG token (its own entity
//! stream, REQ-K-4), a terrain service valid only in this call (ASM-617, `bridge::services`) and a
//! copying contact sink (`bridge::sinks`). A contact must name a candidate; the kernel sorts and
//! deduplicates the contacts as for built-in sensors.
//!
//! **Faults** (ADR 0021, ADR 0050): a non-OK result (a panic included), a refused contact, RNG draw or
//! terrain query (a breach on the call record) faults the capability for this host under the
//! scenario's policy; the contacts of that call are discarded whole and a faulted sensor yields none
//! from then on (ASM-615). A panic of the `TerrainProvider` is the host's: logged and resumed on the
//! caller after the plugin call, the plugin not faulted (ASM-722 b). A validation host never calls a
//! plugin sensor.

#![expect(
    unsafe_code,
    reason = "plugin sensor handle and calls through the vtable"
)]

use std::sync::Arc;

use umbra_core::{
    BuildError, ComponentId, ContactBuffer, Factory, Observer, Ownership, ParamSchema, ParamValue,
    SenseContext, Sensor, Target, TerrainProvider,
};
use umbra_plugin_api::{
    UMBRA_OWNERSHIP_OWNED, UMBRA_OWNERSHIP_REMOTE, UmbraSenseCall, UmbraSensor,
};

use super::batch::{PluginCapability, problem};
use super::factory::configure_handle;
use super::marshal::{self, written_size, written_version};
use super::{services, sinks};
use crate::api::HostMode;
use crate::descriptor::SensorVTable;
use crate::log::LogLevel;
use crate::tokens::{self, Finished, Sink, TerrainCall};

/// A sensor capability of one host.
pub(crate) type SensorCapability = PluginCapability<SensorVTable>;

/// The registry factory of one sensor capability of one host.
pub(crate) struct PluginSensorFactory {
    capability: Arc<SensorCapability>,
}

impl PluginSensorFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<SensorCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn Sensor> for PluginSensorFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    /// `configure` with `params` as JSON text; in a run-mode host only inside the host's build scope
    /// (RR-m2), like behaviors.
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn Sensor>, BuildError> {
        let cap = &self.capability;
        let raw = configure_handle(cap, "sensor", params, cap.vtable().configure)?;
        Ok(Arc::new(PluginSensor {
            capability: Arc::clone(cap),
            raw,
        }))
    }
}

/// A configured plugin sensor (see the module docs).
pub(crate) struct PluginSensor {
    capability: Arc<SensorCapability>,
    raw: *mut UmbraSensor,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which take the image's blocking call lock (ADR 0019 "Threads and re-entry"),
// so sending the sensor to another thread never lets the plugin see concurrent calls. Exercised by
// `sensor_bridge.rs`
// `given_a_plugin_sensor_shared_with_another_thread_then_it_senses_there_and_drops_last_there`.
unsafe impl Send for PluginSensor {}
// SAFETY: as above; `&PluginSensor` only reads the pointer value and calls through the lock (the same
// test senses with one sensor from two threads).
unsafe impl Sync for PluginSensor {}

/// The ABI code of an ownership.
fn ownership_code(ownership: &Ownership) -> u32 {
    match ownership {
        Ownership::Owned => UMBRA_OWNERSHIP_OWNED,
        Ownership::Remote { .. } => UMBRA_OWNERSHIP_REMOTE,
    }
}

impl Sensor for PluginSensor {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn sense(
        &self,
        ctx: &mut SenseContext<'_>,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        out: &mut ContactBuffer<'_>,
    ) {
        let cap = &self.capability;
        if cap.host().mode == HostMode::Validation || cap.faulted() {
            return;
        }
        let minor = cap.minor();
        let entry = cap.vtable().sense;
        let raw = self.raw;
        let (tick, rate_hz) = (ctx.now.tick(), ctx.now.rate().hz());
        let provider: &dyn TerrainProvider = ctx.terrain;
        let rng = std::ptr::from_mut(&mut *ctx.rng);
        let viewer = marshal::target_out(
            observer.id,
            observer.state,
            observer.force,
            UMBRA_OWNERSHIP_OWNED,
            minor,
        );
        let outcome = cap.call(|scope, scratch| {
            scratch.targets.extend(candidates.iter().map(|t| {
                marshal::target_out(t.id, t.state, t.force, ownership_code(t.ownership), minor)
            }));
            let mut state = std::mem::take(&mut scratch.sense);
            state.rng = Some(rng);
            state
                .candidates
                .extend(candidates.iter().map(|t| t.id.get()));
            state.candidates.sort_unstable();
            let terrain_service = services::terrain(scope.sink(Sink::Terrain), minor);
            let sink = sinks::contact_sink(scope.sink(Sink::Contact), minor);
            let call = UmbraSenseCall {
                abi_version: written_version(minor),
                struct_size: written_size::<UmbraSenseCall>(minor),
                tick,
                rate_hz,
                reserved_0: 0,
                observer: &raw const viewer,
                candidates: if scratch.targets.is_empty() {
                    std::ptr::null()
                } else {
                    scratch.targets.as_ptr()
                },
                candidate_count: scratch.targets.len(),
                rng: tokens::rng_token(scope.generation(), 0),
                terrain: &raw const terrain_service,
            };
            let ready = tokens::with_top(|r| {
                r.terrain = Some(TerrainCall::new(&provider));
                r.sense = Some(state);
            });
            if ready.is_none() {
                return (
                    Some("no call record on this thread (thread-local storage)".to_owned()),
                    None,
                );
            }
            // SAFETY: `sense` is a required, non-null entry of this image's validated vtable and the
            // library is open (this sensor holds the capability, which holds the image). `raw` is a
            // live configured sensor; the call record, the observer, the candidates (and the force
            // strings they borrow from the kernel), the terrain service, the sink and the error sink
            // live on this frame or in the scratch, unmoved until the call returns; the record's
            // terrain state points at `provider` on this frame and its RNG pointer at the context's
            // stream, which the host does not touch during the call; both are taken off the record
            // below. Calls into the image are serialised by the call lock. A plugin cannot unwind.
            let code = unsafe { entry(raw, &raw const call, &raw const sink, scope.error_sink()) };
            let (state, error, breach, panic) = tokens::with_top(|r| {
                r.terrain = None;
                (
                    r.sense.take(),
                    r.error.clone(),
                    r.breach.take(),
                    r.provider_panic.take(),
                )
            })
            .unwrap_or_default();
            let mut state = state.unwrap_or_default();
            let verdict = problem(code, &Finished { error, breach });
            // `panic.is_none()` is defensive: a provider panic resumes below and discards `out` anyway.
            if panic.is_none() && verdict.is_none() && !cap.faulted() {
                for contact in state.contacts.drain(..) {
                    out.push(contact);
                }
            }
            // Back into the scratch for reuse (cleared after the call).
            scratch.sense = state;
            (verdict, panic)
        });
        let problem = match outcome {
            Ok(((problem, panic), _)) => {
                // A provider panic is the host's: it resumes here, before any verdict on the plugin.
                services::resume_provider_panic(
                    panic,
                    cap.host().log.as_ref(),
                    cap.plugin(),
                    cap.id(),
                );
                problem
            }
            Err(e) => Some(e.to_string()),
        };
        if let Some(message) = problem {
            cap.fault("sense", message);
        }
    }
}

impl Drop for PluginSensor {
    /// `destroy`, also for a faulted capability (ADR 0021); a re-entrant drop leaks the handle (logged)
    /// rather than deadlock.
    fn drop(&mut self) {
        let cap = &self.capability;
        let destroy = cap.vtable().destroy;
        let raw = self.raw;
        // SAFETY: `destroy` is a required entry of the open image's vtable (the capability holds the
        // image); `raw` is a live configured sensor, destroyed exactly once, here; the call lock
        // serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(raw) }) {
            cap.host().log.log(
                cap.plugin(),
                LogLevel::Error,
                &format!("capability {}: destroy skipped: {e}", cap.id()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umbra_core::RemoteSource;

    #[test]
    fn ownership_codes_follow_the_abi() {
        assert_eq!(ownership_code(&Ownership::Owned), UMBRA_OWNERSHIP_OWNED);
        let remote = Ownership::Remote {
            source: RemoteSource::new("dis"),
        };
        assert_eq!(ownership_code(&remote), UMBRA_OWNERSHIP_REMOTE);
    }
}
