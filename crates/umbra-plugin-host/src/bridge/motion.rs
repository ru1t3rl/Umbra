//! The motion model bridge (ABI 0.2; REQ-P-4, REQ-P-5, REQ-P-7, REQ-M-1, REQ-M-4, REQ-C-6, D-8, ADR
//! 0021, ADR 0050, ASM-612, ASM-614, ASM-616, ASM-617): a plugin motion model capability as a core
//! `Factory<dyn MotionModel>`, and its configured model as a core [`MotionModel`], used by the kernel
//! exactly like a built-in.
//!
//! **Lifecycle.** `build` is the plugin's `configure` ([`configure_handle`]); every built model owns
//! one plugin handle, destroyed (`destroy`) when the model drops, also for a faulted capability. The
//! model holds its capability and so its image, so the image cannot unload before `destroy` ran.
//!
//! **A step** is one call per entity (not batched, ASM-612) under the image's blocking call lock:
//! the input state, the pre-filled `out` (the input state again), this tick's intents and the step
//! record are written in the plugin's declared minor (R-B3), with a terrain service whose token is
//! valid only in this call and answers from the `TerrainProvider` the core call received (ASM-617,
//! `bridge::services`). The core trait carries no simulation time, so the record's `tick` is 0 and
//! its `rate_hz` is `1 / dt` rounded to the nearest whole hertz, at least 1 (ASM-720): a plugin sees
//! exactly the inputs a built-in sees.
//!
//! **Faults** (ADR 0021, ADR 0050): a non-OK result (a panic included), a refused terrain query (a
//! breach on the call record) or a result that is not finite or whose orientation is not a unit
//! quaternion within 1e-9 (ASM-616) faults the capability for this host under the scenario's policy
//! (`abort` ends the run after the tick). The entity then keeps its position and orientation with
//! zero velocity, for that tick and, since a faulted capability is never called again, every later
//! one (ASM-614). A panic of the `TerrainProvider` during a query is the host's, not the plugin's:
//! the plugin's query gets `UMBRA_ERR_INTERNAL`, and after the plugin call the panic is logged and
//! resumed on the caller, as from a built-in, without faulting the plugin (ASM-722 b). A validation
//! host never steps a plugin model (the entity is held). A `dt` that is
//! not finite and positive returns the input state without a call (the core contract).

#![expect(
    unsafe_code,
    reason = "plugin motion model handle and calls through the vtable"
)]

use std::sync::Arc;

use umbra_core::{
    BuildError, ComponentId, Factory, Intent, KinematicState, MotionModel, MotionModelId,
    ParamSchema, ParamValue, Seconds, TerrainProvider, Vec3,
};
use umbra_plugin_api::{UmbraMotionModel, UmbraMotionStep};

use super::batch::{PluginCapability, problem};
use super::factory::configure_handle;
use super::marshal::{self, written_size, written_version};
use super::services;
use crate::api::HostMode;
use crate::descriptor::MotionVTable;
use crate::log::LogLevel;
use crate::tokens::{self, Sink, TerrainCall};

/// A motion model capability of one host.
pub(crate) type MotionCapability = PluginCapability<MotionVTable>;

/// The registry factory of one motion model capability of one host.
pub(crate) struct PluginMotionFactory {
    capability: Arc<MotionCapability>,
}

impl PluginMotionFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<MotionCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn MotionModel> for PluginMotionFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    /// `configure` with `params` as JSON text; in a run-mode host only inside the host's build scope
    /// (RR-m2), like behaviors.
    fn build(&self, params: &ParamValue) -> Result<Arc<dyn MotionModel>, BuildError> {
        let cap = &self.capability;
        let id = MotionModelId::new(cap.id().as_str())
            .map_err(|e| BuildError::new(format!("plugin motion model {}: {e}", cap.id())))?;
        let raw = configure_handle(cap, "motion model", params, cap.vtable().configure)?;
        Ok(Arc::new(PluginMotionModel {
            capability: Arc::clone(cap),
            raw,
            id,
        }))
    }
}

/// A configured plugin motion model (see the module docs).
pub(crate) struct PluginMotionModel {
    capability: Arc<MotionCapability>,
    raw: *mut UmbraMotionModel,
    id: MotionModelId,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which take the image's blocking call lock (ADR 0019 "Threads and re-entry":
// calls may come from different threads over time, plugins must not rely on thread-local state), so
// sending the model to another thread never lets the plugin see concurrent calls. Exercised by
// `motion_bridge.rs`
// `given_a_plugin_motion_model_shared_with_another_thread_then_it_steps_there_and_drops_last_there`.
unsafe impl Send for PluginMotionModel {}
// SAFETY: as above; `&PluginMotionModel` only reads the pointer value and calls through the lock
// (the same test steps one model from two threads).
unsafe impl Sync for PluginMotionModel {}

/// Why a step produced no result.
enum Refused {
    /// The capability was faulted during the call (already recorded).
    Faulted,
    /// Record this fault.
    Fault(String),
}

/// `rate_hz` of a step record: `1 / dt` rounded to the nearest whole hertz, at least 1 (ASM-720).
/// `dt` is finite and positive.
fn rate_of(dt: Seconds) -> u32 {
    let hz = (1.0 / dt.0).round();
    if hz >= f64::from(u32::MAX) {
        u32::MAX
    } else if hz >= 1.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number in [1, u32::MAX), checked above"
        )]
        let rate = hz as u32;
        rate
    } else {
        1
    }
}

impl PluginMotionModel {
    /// One `step` call (under the call lock); the checked result or why there is none.
    fn call(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> Result<KinematicState, Refused> {
        let cap = &self.capability;
        let minor = cap.minor();
        let input = marshal::kinematic_out(state, minor);
        let mut out = marshal::kinematic_out(state, minor);
        let entry = cap.vtable().step;
        let raw = self.raw;
        let outcome = cap.call(|scope, scratch| {
            for intent in intents {
                let payload = match intent {
                    Intent::Custom { payload, .. } => {
                        // Practically unreachable: a core `Custom` payload is a JSON value that
                        // always serialises (only a non-string map key or a failing writer can
                        // fail, and neither exists here). If it ever fails, the step cannot be
                        // written in full, so it is not called and the capability faults under the
                        // policy, like any step that produced no result (ADR 0021), rather than
                        // silently dropping the intent.
                        let text = serde_json::to_string(payload)
                            .map_err(|e| format!("intent Custom payload: {e}"))?;
                        scratch.payloads.push(text);
                        scratch.payloads.last().map_or("", String::as_str)
                    }
                    _ => "",
                };
                // The payload's heap text does not move when `payloads` grows.
                scratch
                    .step_intents
                    .extend(marshal::intent_out(intent, payload, minor));
            }
            let terrain_service = services::terrain(scope.sink(Sink::Terrain), minor);
            let step = UmbraMotionStep {
                abi_version: written_version(minor),
                struct_size: written_size::<UmbraMotionStep>(minor),
                tick: 0,
                rate_hz: rate_of(dt),
                reserved_0: 0,
                dt: dt.0,
                state: &raw const input,
                intents: if scratch.step_intents.is_empty() {
                    std::ptr::null()
                } else {
                    scratch.step_intents.as_ptr()
                },
                intent_count: scratch.step_intents.len(),
                terrain: &raw const terrain_service,
            };
            let provider: &dyn TerrainProvider = terrain;
            if tokens::with_top(|r| r.terrain = Some(TerrainCall::new(&provider))).is_none() {
                return Err("no call record on this thread (thread-local storage)".to_owned());
            }
            // SAFETY: `step` is a required, non-null entry of this image's validated vtable and the
            // library is open (this model holds the capability, which holds the image). `raw` is a
            // live configured model; `step`, `input`, the intents (and the strings they borrow from
            // `intents` and the scratch), the terrain service, `out` and the error sink live on this
            // frame or in the scratch, unmoved until the call returns; the terrain state of the
            // record points at `provider` on this frame and is cleared below; calls into the image
            // are serialised by the call lock. A plugin cannot unwind.
            let code = unsafe { entry(raw, &raw const step, &raw mut out, scope.error_sink()) };
            let panic = tokens::with_top(|r| {
                r.terrain = None;
                r.provider_panic.take()
            })
            .flatten();
            Ok((code, panic))
        });
        let (code, finished) = outcome.map_err(|e| Refused::Fault(e.to_string()))?;
        let (code, panic) = code.map_err(Refused::Fault)?;
        // A provider panic is the host's: it resumes here, before any verdict on the plugin.
        services::resume_provider_panic(panic, cap.host().log.as_ref(), cap.plugin(), cap.id());
        if let Some(problem) = problem(code, &finished) {
            return Err(Refused::Fault(problem));
        }
        if cap.faulted() {
            return Err(Refused::Faulted);
        }
        marshal::kinematic_in(&out).map_err(Refused::Fault)
    }
}

impl MotionModel for PluginMotionModel {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    fn step(
        &self,
        state: &KinematicState,
        intents: &[Intent],
        dt: Seconds,
        terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        if !(dt.0.is_finite() && dt.0 > 0.0) {
            return *state;
        }
        let held = KinematicState {
            position: state.position,
            velocity: Vec3::ZERO,
            orientation: state.orientation,
        };
        let cap = &self.capability;
        if cap.host().mode == HostMode::Validation || cap.faulted() {
            return held;
        }
        match self.call(state, intents, dt, terrain) {
            Ok(next) => next,
            Err(Refused::Faulted) => held,
            Err(Refused::Fault(message)) => {
                cap.fault("step", message);
                held
            }
        }
    }
}

impl Drop for PluginMotionModel {
    /// `destroy`, also for a faulted capability (ADR 0021); a re-entrant drop leaks the handle (logged)
    /// rather than deadlock.
    fn drop(&mut self) {
        let cap = &self.capability;
        let destroy = cap.vtable().destroy;
        let raw = self.raw;
        // SAFETY: `destroy` is a required entry of the open image's vtable (the capability holds the
        // image); `raw` is a live configured model, destroyed exactly once, here; the call lock
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

    #[test]
    fn the_step_rate_is_one_over_dt_rounded_and_at_least_one() {
        assert_eq!(rate_of(Seconds(1.0 / 30.0)), 30);
        assert_eq!(rate_of(Seconds(1.0 / 45.0)), 45);
        assert_eq!(rate_of(Seconds(0.4)), 3, "2.5 rounds away from zero");
        assert_eq!(rate_of(Seconds(10.0)), 1, "a long step is at least 1 Hz");
        assert_eq!(rate_of(Seconds(1e-12)), u32::MAX, "saturates");
    }
}
