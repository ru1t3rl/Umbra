//! What the condition and action bridges share (ABI 0.2; REQ-B-3, REQ-B-11, REQ-B-15, REQ-P-5,
//! REQ-P-7, ADR 0050, ASM-612, ASM-618, ASM-723): the grants a configured leaf resolves from its
//! params, and the one leaf call.
//!
//! **Grants** (ASM-618): at configure, every blackboard param the capability's metadata declares
//! names the key its param value holds (a string); that key, with the declared type and access, is
//! one grant of the configured leaf. A param that is absent or null names no key; any other value is
//! a build error. The blackboard service of every call of the leaf grants exactly these keys, so two
//! configured leaves of one capability never see each other's keys (REQ-B-11).
//!
//! **A leaf call** is one call per node tick (not batched, ASM-612) under the image's blocking call
//! lock: the entity view (in the plugin's declared minor, R-B3), the entity's RNG token (item 0), the
//! blackboard service (`bridge::services`) and, for an action, an intent sink of item 0 (the batch
//! state of a batch of one, so the intent sink and its cap are the behavior kind's). The call is clean
//! when the plugin returned `UMBRA_OK`, no host callback recorded a breach, the result the plugin
//! wrote is valid (`judge`) and the capability was not faulted during the call (re-entry); only then
//! are its intents handed on. Otherwise the call's intents are discarded and its blackboard writes
//! undone (ASM-723), and the caller faults the capability (unless it was faulted during the call,
//! already recorded).

use std::sync::Arc;

use umbra_core::{BehaviorContext, Blackboard, EntityView, IntentBuffer, ParamValue};
use umbra_plugin_api::{UmbraCode, UmbraErrorSink, UmbraIntentSink, UmbraLeafCall};

use super::batch::{PluginCapability, problem};
use super::marshal::{written_size, written_version};
use super::{services, sinks};
use crate::descriptor::Capability;
use crate::tokens::{self, Finished, Grant, Sink};

/// The grants of a leaf of `capability` configured with `params` (see the module docs); `Err` names
/// the param.
pub(crate) fn grants(capability: &Capability, params: &ParamValue) -> Result<Box<[Grant]>, String> {
    let mut out = Vec::with_capacity(capability.metadata.blackboard_params.len());
    for declared in &capability.metadata.blackboard_params {
        match params.get(&declared.param) {
            None | Some(ParamValue::Null) => {}
            Some(ParamValue::String(key)) => out.push(Grant {
                key: Arc::from(key.as_str()),
                ty: declared.ty,
                access: declared.access,
            }),
            Some(other) => {
                return Err(format!(
                    "param {:?} names a blackboard key, so it must be a string, not {other}",
                    declared.param
                ));
            }
        }
    }
    Ok(out.into_boxed_slice())
}

/// Why a leaf call produced no result.
pub(crate) enum Refused {
    /// The capability was faulted during the call (already recorded).
    Faulted,
    /// Record this fault.
    Fault(String),
}

/// The core inputs of one leaf call.
pub(crate) struct Inputs<'c, 'r, 'e, 'b> {
    /// Time and the entity's own RNG stream.
    pub(crate) ctx: &'c mut BehaviorContext<'r>,
    /// The entity being ticked.
    pub(crate) entity: &'e EntityView<'e>,
    /// The node's blackboard.
    pub(crate) bb: &'b mut Blackboard,
    /// The configured leaf's grants.
    pub(crate) grants: &'b [Grant],
}

/// One leaf call of `cap` (see the module docs). `invoke` calls the plugin with the call record, the
/// intent sink (null without `intents`) and the error sink, and returns its code and the result it
/// wrote; `judge` checks that result. With `intents`, the intents of a clean call are pushed to it.
pub(crate) fn call<V, T, J>(
    cap: &PluginCapability<V>,
    input: Inputs<'_, '_, '_, '_>,
    intents: Option<&mut IntentBuffer<'_>>,
    invoke: impl FnOnce(
        *const UmbraLeafCall,
        *const UmbraIntentSink,
        *const UmbraErrorSink,
    ) -> (UmbraCode, T),
    judge: impl FnOnce(T) -> Result<J, String>,
) -> Result<J, Refused> {
    let Inputs {
        ctx,
        entity,
        bb,
        grants,
    } = input;
    let minor = cap.minor();
    let (tick, rate_hz) = (ctx.now.tick(), ctx.now.rate().hz());
    let rng = std::ptr::from_mut(&mut *ctx.rng);
    let emits = intents.is_some();
    let mut intents = intents;
    let outcome = cap.call(|scope, scratch| {
        // Derived from the closure's own capture of `bb` (review M1): a pointer taken before the
        // closure would be invalidated by the closure's reborrow of `bb`. `bb` itself is next used
        // only after the plugin call returned and the record's copy of this pointer was taken off.
        let board = std::ptr::from_mut(&mut *bb);
        let span = scratch.push_entity_data(entity, minor);
        let view = scratch.view(entity, span, minor);
        let mut state = std::mem::take(&mut scratch.leaf);
        state.rng = Some(rng);
        state.board = Some(board);
        state.grants = Some(std::ptr::from_ref(grants));
        let batch = emits.then(|| {
            let mut batch = std::mem::take(&mut scratch.batch);
            batch.rngs.push(rng);
            batch.counts.push(0);
            batch
        });
        let ready = tokens::with_top(|r| {
            r.leaf = Some(state);
            r.batch = batch;
        });
        if ready.is_none() {
            return Err("no call record on this thread (thread-local storage)".to_owned());
        }
        let service = services::blackboard(scope.sink(Sink::Blackboard), minor);
        let sink = sinks::intent_sink(scope.sink(Sink::Intent), minor);
        let record = UmbraLeafCall {
            abi_version: written_version(minor),
            struct_size: written_size::<UmbraLeafCall>(minor),
            tick,
            rate_hz,
            reserved_0: 0,
            entity: &raw const view,
            rng: tokens::rng_token(scope.generation(), 0),
            blackboard: &raw const service,
        };
        let sink_ptr = if emits {
            &raw const sink
        } else {
            std::ptr::null()
        };
        // The record, the view (and the scratch arrays and kernel strings it points into), the
        // service and the sinks live on this frame or in the scratch, unmoved until `invoke` returns;
        // the record's RNG, blackboard and grants pointers are taken off it below, before this frame
        // ends.
        let (code, out) = invoke(&raw const record, sink_ptr, scope.error_sink());
        let (state, batch, error, breach) = tokens::with_top(|r| {
            (
                r.leaf.take(),
                r.batch.take(),
                r.error.clone(),
                r.breach.take(),
            )
        })
        .unwrap_or_default();
        let mut state = state.unwrap_or_default();
        let mut batch = batch.unwrap_or_default();
        let verdict = match problem(code, &Finished { error, breach }) {
            Some(problem) => Err(Refused::Fault(problem)),
            None => match judge(out) {
                Err(why) => Err(Refused::Fault(why)),
                Ok(_) if cap.faulted() => Err(Refused::Faulted),
                Ok(result) => Ok(result),
            },
        };
        match &verdict {
            Ok(_) => {
                // A clean call's writes stay; its undo log ends with the call (ASM-723 e).
                state.undo.clear();
                if let Some(out) = intents.as_mut() {
                    for (_, intent) in batch.intents.drain(..) {
                        out.push(intent);
                    }
                }
            }
            // The record's copy of `board` was taken off above and `board` is not used again, so
            // `bb` (its parent) is used directly.
            Err(_) => services::undo(bb, &mut state.undo),
        }
        // Back into the scratch for reuse (cleared after the call).
        scratch.leaf = state;
        if emits {
            scratch.batch = batch;
        }
        Ok(verdict)
    });
    match outcome {
        Ok((Ok(verdict), _)) => verdict,
        Ok((Err(why), _)) => Err(Refused::Fault(why)),
        Err(e) => Err(Refused::Fault(e.to_string())),
    }
}
