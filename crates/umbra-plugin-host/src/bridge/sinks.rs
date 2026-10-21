//! The host's copying sinks for plugin output (ADR 0019 "Memory", ADR 0021 "Host-side guards"): the
//! intent sink of a `tick_batch` call and the value sink of an `output` call.
//!
//! Both are `extern "C"` leaves: they catch panics, resolve their token against the calling
//! thread's top call record only, copy what the plugin passed and never call into a plugin. No
//! record is `UMBRA_ERR_STATE` without any lookup; with a record, every misuse records a breach on
//! it (the call then faults the capability) and returns `UMBRA_ERR_STATE` (a token of another call
//! or sink) or `UMBRA_ERR_INVALID_ARGUMENT` (a bad item index, intent or value).

#![expect(
    unsafe_code,
    reason = "extern \"C\" sink callbacks reading plugin structs"
)]

use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE,
    UMBRA_MAX_INTENTS_PER_ITEM, UMBRA_OK, UmbraCode, UmbraIntent, UmbraIntentSink, UmbraToken,
    UmbraValue, UmbraValueSink,
};

use super::marshal::{self, written_size};
use crate::host_api::guarded;
use crate::tokens::{self, CallRecord, Sink};

/// The intent sink of a `tick_batch` call whose sink token is `ctx`.
pub(crate) fn intent_sink(ctx: UmbraToken, minor: u32) -> UmbraIntentSink {
    UmbraIntentSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: written_size::<UmbraIntentSink>(minor),
        ctx,
        push: Some(intent_push),
    }
}

/// The value sink of an `output` call whose sink token is `ctx`.
pub(crate) fn value_sink(ctx: UmbraToken, minor: u32) -> UmbraValueSink {
    UmbraValueSink {
        abi_version: UMBRA_ABI_VERSION,
        struct_size: written_size::<UmbraValueSink>(minor),
        ctx,
        set: Some(value_set),
    }
}

/// The intent cap per item per call (ASM-119), short for messages.
const MAX: usize = UMBRA_MAX_INTENTS_PER_ITEM;

/// Refuses with `code` and records `why` as the call's breach.
fn refuse<T>(r: &mut CallRecord, code: UmbraCode, why: String) -> Result<T, UmbraCode> {
    r.breach(|| why);
    Err(code)
}

/// Appends `*intent` to item `item` of the current batch call (copied; committed only after a clean
/// call). At most `UMBRA_MAX_INTENTS_PER_ITEM` per item per call.
unsafe extern "C" fn intent_push(
    ctx: UmbraToken,
    item: usize,
    intent: *const UmbraIntent,
) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        // Phase 1: the token and the item index, against the top record.
        let checked = tokens::with_top(|r| {
            if r.sink(Sink::Intent) != ctx || r.batch.is_none() {
                return refuse(
                    r,
                    UMBRA_ERR_STATE,
                    "intent sink: stale or foreign token (not this call's intent sink)".to_owned(),
                );
            }
            let count = r.batch.as_ref().map_or(0, |b| b.rngs.len());
            if item >= count {
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    format!("intent sink: item index {item} >= count {count}"),
                );
            }
            Ok(r.minor)
        });
        let minor = match checked {
            None => return UMBRA_ERR_STATE,
            Some(Err(code)) => return code,
            Some(Ok(minor)) => minor,
        };
        // Phase 2: copy and check the intent (no stack borrow while reading plugin memory).
        // SAFETY: the plugin passes the intent (null is refused) and its strings borrowed for the
        // duration of this call (ADR 0019).
        let converted = unsafe { marshal::intent_within(intent, minor) };
        // Phase 3: store it.
        tokens::with_top(|r| {
            let intent = match converted {
                Ok(intent) => intent,
                Err(why) => {
                    return refuse(r, UMBRA_ERR_INVALID_ARGUMENT, format!("intent sink: {why}"));
                }
            };
            let Some(batch) = r.batch.as_mut() else {
                return Err(UMBRA_ERR_STATE);
            };
            let Some(pushed) = batch.counts.get_mut(item) else {
                return Err(UMBRA_ERR_STATE);
            };
            if *pushed >= UMBRA_MAX_INTENTS_PER_ITEM {
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    format!("intent sink: more than {MAX} intents for item {item}"),
                );
            }
            *pushed += 1;
            batch.intents.push((item, intent));
            Ok(())
        })
        .map_or(UMBRA_ERR_STATE, |r| r.err().unwrap_or(UMBRA_OK))
    })
}

/// Records the value of the current `output` call (`value` null = unset), at most once per call.
unsafe extern "C" fn value_set(ctx: UmbraToken, value: *const UmbraValue) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        let checked = tokens::with_top(|r| {
            let ours = r.sink(Sink::Value) == ctx;
            let Some(output) = r.output.as_mut().filter(|_| ours) else {
                return refuse(
                    r,
                    UMBRA_ERR_STATE,
                    "value sink: stale or foreign token (not this call's value sink)".to_owned(),
                );
            };
            output.calls += 1;
            if output.calls > 1 {
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    "value sink called twice in one output call".to_owned(),
                );
            }
            Ok(r.minor)
        });
        let minor = match checked {
            None => return UMBRA_ERR_STATE,
            Some(Err(code)) => return code,
            Some(Ok(minor)) => minor,
        };
        // SAFETY: the plugin passes the value (or null) and its string borrowed for this call.
        let converted = unsafe { marshal::value_within(value, minor) };
        tokens::with_top(|r| match converted {
            Ok(v) => {
                if let Some(output) = r.output.as_mut() {
                    output.value = v;
                }
                Ok(())
            }
            Err(why) => refuse(
                r,
                UMBRA_ERR_INVALID_ARGUMENT,
                format!("output value: {why}"),
            ),
        })
        .map_or(UMBRA_ERR_STATE, |r| r.err().unwrap_or(UMBRA_OK))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::log::TracingLog;
    use crate::tokens::{ActiveCall, BatchCall, OutputCall};
    use umbra_core::{BbValue, Intent, PluginId};
    use umbra_plugin_api::{UMBRA_INTENT_HOLD, UMBRA_VALUE_INTEGER, UmbraStr, UmbraVec3};

    fn call(image: UmbraToken, generation: u32) -> ActiveCall {
        ActiveCall::push(CallRecord::new(
            image,
            generation,
            PluginId::new("test.plugin.sinks").unwrap_or_else(|e| panic!("{e}")),
            Arc::new(TracingLog),
        ))
    }

    fn hold() -> UmbraIntent {
        let empty = UmbraStr {
            ptr: b"".as_ptr(),
            len: 0,
        };
        UmbraIntent {
            abi_version: UMBRA_ABI_VERSION,
            struct_size: crate::host_api::abi_size::<UmbraIntent>(),
            kind: UMBRA_INTENT_HOLD,
            reserved_0: 0,
            position: UmbraVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            value: 0.0,
            entity: 0,
            id: empty,
            payload: empty,
        }
    }

    #[test]
    fn intents_are_buffered_per_item_and_capped() {
        let image = tokens::make(200, tokens::HOST_CTX_INDEX);
        let active = call(image, 3);
        tokens::with_top(|r| {
            r.batch = Some(BatchCall {
                rngs: vec![std::ptr::null_mut(); 2],
                intents: Vec::new(),
                counts: vec![0; 2],
            });
        });
        let sink = intent_sink(tokens::sink_token(image, 3, Sink::Intent), 0);
        let push = sink.push.unwrap_or_else(|| panic!("push"));
        let i = hold();
        // SAFETY: a valid local intent; the record is on this thread.
        unsafe {
            assert_eq!(push(sink.ctx, 1, &raw const i), UMBRA_OK);
            assert_eq!(push(sink.ctx + 1, 0, &raw const i), UMBRA_ERR_STATE);
            for _ in 0..UMBRA_MAX_INTENTS_PER_ITEM {
                assert_eq!(push(sink.ctx, 0, &raw const i), UMBRA_OK);
            }
            assert_eq!(push(sink.ctx, 0, &raw const i), UMBRA_ERR_INVALID_ARGUMENT);
            assert_eq!(push(sink.ctx, 2, &raw const i), UMBRA_ERR_INVALID_ARGUMENT);
        }
        let batch = tokens::with_top(|r| r.batch.take()).flatten();
        let batch = batch.unwrap_or_else(|| panic!("batch"));
        assert_eq!(batch.counts, [UMBRA_MAX_INTENTS_PER_ITEM, 1]);
        assert_eq!(batch.intents[0], (1, Intent::Hold));
        assert!(
            active
                .finish()
                .breach
                .is_some_and(|b| b.contains("stale or foreign"))
        );
        // SAFETY: without a record the callback refuses before reading.
        assert_eq!(unsafe { push(sink.ctx, 0, &raw const i) }, UMBRA_ERR_STATE);
    }

    #[test]
    fn the_value_sink_takes_one_value() {
        let image = tokens::make(201, tokens::HOST_CTX_INDEX);
        let active = call(image, 4);
        tokens::with_top(|r| r.output = Some(OutputCall::default()));
        let sink = value_sink(tokens::sink_token(image, 4, Sink::Value), 0);
        let set = sink.set.unwrap_or_else(|| panic!("set"));
        let mut v = marshal::value_out(&BbValue::Integer(5), 0);
        v.ty = UMBRA_VALUE_INTEGER;
        // SAFETY: a valid local value; the record is on this thread.
        unsafe {
            assert_eq!(set(sink.ctx, &raw const v), UMBRA_OK);
            assert_eq!(set(sink.ctx, &raw const v), UMBRA_ERR_INVALID_ARGUMENT);
        }
        let output = tokens::with_top(|r| r.output.take()).flatten();
        assert_eq!(output.and_then(|o| o.value), Some(BbValue::Integer(5)));
        assert!(active.finish().breach.is_some_and(|b| b.contains("twice")));
    }
}
