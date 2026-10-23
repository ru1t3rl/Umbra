//! The host's copying sinks for plugin output (ADR 0019 "Memory", ADR 0021 "Host-side guards"): the
//! intent sink of a `tick_batch` call, the value sink of an `output` call, the message sink of a
//! network adapter's `poll` call (ABI 0.1, ADR 0033) and the contact sink of a sensor's `sense` call
//! (ABI 0.2, ADR 0050).
//!
//! Both are `extern "C"` leaves: they catch panics, resolve their token against the calling
//! thread's top call record only, copy what the plugin passed and never call into a plugin. No
//! record is `UMBRA_ERR_STATE` without any lookup; with a record, every misuse records a breach on
//! it (the call then faults the capability) and returns `UMBRA_ERR_STATE` (a token of another call
//! or sink) or `UMBRA_ERR_INVALID_ARGUMENT` (a bad item index, intent, value, message or contact, a
//! contact naming no candidate, or a push beyond the poll's `max` or `UMBRA_MAX_CONTACTS_PER_SENSE`).

#![expect(
    unsafe_code,
    reason = "extern \"C\" sink callbacks reading plugin structs"
)]

use umbra_plugin_api::{
    UMBRA_ABI_VERSION, UMBRA_ERR_INTERNAL, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE,
    UMBRA_MAX_CONTACTS_PER_SENSE, UMBRA_MAX_INTENTS_PER_ITEM, UMBRA_OK, UmbraCode, UmbraContact,
    UmbraContactSink, UmbraIntent, UmbraIntentSink, UmbraNetMessage, UmbraNetSink, UmbraToken,
    UmbraValue, UmbraValueSink,
};

use super::marshal::{self, written_size, written_version};
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

/// The message sink of a network adapter's `poll` call whose sink token is `ctx`, written in the
/// plugin's declared minor `minor` (ABI 0.1).
pub(crate) fn net_sink(ctx: UmbraToken, minor: u32) -> UmbraNetSink {
    UmbraNetSink {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraNetSink>(minor),
        ctx,
        push: Some(net_push),
    }
}

/// The contact sink of a sensor's `sense` call whose sink token is `ctx`, written in the plugin's
/// declared minor `minor` (ABI 0.2).
pub(crate) fn contact_sink(ctx: UmbraToken, minor: u32) -> UmbraContactSink {
    UmbraContactSink {
        abi_version: written_version(minor),
        struct_size: written_size::<UmbraContactSink>(minor),
        ctx,
        push: Some(contact_push),
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

/// Appends a copy of `*message` to the current `poll` call (handed to the caller only after a clean
/// call), at most the call's `max` pushes. The message is read with the plugin's declared minor as
/// the ceiling (ADR 0033).
unsafe extern "C" fn net_push(ctx: UmbraToken, message: *const UmbraNetMessage) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        // Phase 1: the token and the cap, against the top record.
        let checked = tokens::with_top(|r| {
            let ours = r.sink(Sink::NetPoll) == ctx;
            let Some(poll) = r.poll.as_ref().filter(|_| ours) else {
                return refuse(
                    r,
                    UMBRA_ERR_STATE,
                    "message sink: stale or foreign token (not this call's message sink)"
                        .to_owned(),
                );
            };
            if poll.messages.len() >= poll.max {
                let max = poll.max;
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    format!("message sink: more than max = {max} messages in one poll"),
                );
            }
            Ok(r.minor)
        });
        let minor = match checked {
            None => return UMBRA_ERR_STATE,
            Some(Err(code)) => return code,
            Some(Ok(minor)) => minor,
        };
        // Phase 2: copy and check the message (no stack borrow while reading plugin memory).
        // SAFETY: the plugin passes the message (null is refused) and its strings borrowed for the
        // duration of this call (ADR 0019).
        let converted = unsafe { marshal::net_message_within(message, minor) };
        // Phase 3: store it.
        tokens::with_top(|r| match converted {
            Ok(m) => match r.poll.as_mut() {
                Some(poll) => {
                    poll.messages.push(m);
                    Ok(())
                }
                None => Err(UMBRA_ERR_STATE),
            },
            Err(why) => refuse(
                r,
                UMBRA_ERR_INVALID_ARGUMENT,
                format!("message sink: {why}"),
            ),
        })
        .map_or(UMBRA_ERR_STATE, |r| r.err().unwrap_or(UMBRA_OK))
    })
}

/// Appends a copy of `*contact` to the current `sense` call (handed to the caller only after a clean
/// call), at most `UMBRA_MAX_CONTACTS_PER_SENSE` per call. The contact must name a candidate of the
/// call (ADR 0050); it is read with the plugin's declared minor as the ceiling.
unsafe extern "C" fn contact_push(ctx: UmbraToken, contact: *const UmbraContact) -> UmbraCode {
    guarded(UMBRA_ERR_INTERNAL, || {
        // Phase 1: the token and the cap, against the top record.
        let checked = tokens::with_top(|r| {
            let ours = r.sink(Sink::Contact) == ctx;
            let Some(sense) = r.sense.as_ref().filter(|_| ours) else {
                return refuse(
                    r,
                    UMBRA_ERR_STATE,
                    "contact sink: stale or foreign token (not this call's contact sink)"
                        .to_owned(),
                );
            };
            if sense.contacts.len() >= UMBRA_MAX_CONTACTS_PER_SENSE {
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    format!(
                        "contact sink: more than {UMBRA_MAX_CONTACTS_PER_SENSE} contacts in one \
                         sense call"
                    ),
                );
            }
            Ok(r.minor)
        });
        let minor = match checked {
            None => return UMBRA_ERR_STATE,
            Some(Err(code)) => return code,
            Some(Ok(minor)) => minor,
        };
        // Phase 2: copy and check the contact (no stack borrow while reading plugin memory).
        // SAFETY: the plugin passes the contact (null is refused) and its string borrowed for the
        // duration of this call (ADR 0019).
        let converted = unsafe { marshal::contact_within(contact, minor) };
        // Phase 3: check it names a candidate, then store it.
        tokens::with_top(|r| {
            let contact = match converted {
                Ok(contact) => contact,
                Err(why) => {
                    return refuse(
                        r,
                        UMBRA_ERR_INVALID_ARGUMENT,
                        format!("contact sink: {why}"),
                    );
                }
            };
            let Some(sense) = r.sense.as_mut() else {
                return Err(UMBRA_ERR_STATE);
            };
            let entity = contact.entity.get();
            if sense.candidates.binary_search(&entity).is_err() {
                return refuse(
                    r,
                    UMBRA_ERR_INVALID_ARGUMENT,
                    format!("contact sink: entity {entity} is not a candidate of this sense call"),
                );
            }
            sense.contacts.push(contact);
            Ok(())
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

    fn contact(entity: u64) -> UmbraContact {
        UmbraContact {
            abi_version: written_version(2),
            struct_size: crate::host_api::abi_size::<UmbraContact>(),
            entity,
            position: UmbraVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            velocity: UmbraVec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            force: UmbraStr {
                ptr: b"".as_ptr(),
                len: 0,
            },
            range_m: 1.0,
        }
    }

    /// ABI 0.2 (ADR 0050, ASM-719): the contact sink takes contacts of candidates only, with this
    /// call's token, up to the cap; the first misuse is the breach.
    #[test]
    fn the_contact_sink_takes_candidates_of_this_call_up_to_the_cap() {
        use crate::tokens::SenseCall;
        let image = tokens::make(202, tokens::HOST_CTX_INDEX);
        let active = call(image, 5);
        tokens::with_top(|r| {
            r.minor = 2;
            r.sense = Some(SenseCall {
                candidates: vec![2, 4],
                ..SenseCall::default()
            });
        });
        let sink = contact_sink(tokens::sink_token(image, 5, Sink::Contact), 2);
        let push = sink.push.unwrap_or_else(|| panic!("push"));
        let (four, three) = (contact(4), contact(3));
        // SAFETY: valid local contacts; the record is on this thread.
        unsafe {
            assert_eq!(push(sink.ctx, &raw const four), UMBRA_OK);
            assert_eq!(
                push(tokens::sink_token(image, 5, Sink::Terrain), &raw const four),
                UMBRA_ERR_STATE,
                "another service's token"
            );
            assert_eq!(
                push(tokens::sink_token(image, 4, Sink::Contact), &raw const four),
                UMBRA_ERR_STATE,
                "another call's token"
            );
            assert_eq!(push(sink.ctx, &raw const three), UMBRA_ERR_INVALID_ARGUMENT);
            assert_eq!(push(sink.ctx, std::ptr::null()), UMBRA_ERR_INVALID_ARGUMENT);
        }
        tokens::with_top(|r| {
            if let Some(sense) = r.sense.as_mut() {
                let kept = sense.contacts[0].clone();
                sense.contacts.resize(UMBRA_MAX_CONTACTS_PER_SENSE, kept);
            }
        });
        // SAFETY: as above.
        let over = unsafe { push(sink.ctx, &raw const four) };
        assert_eq!(over, UMBRA_ERR_INVALID_ARGUMENT, "the cap");
        let sense = tokens::with_top(|r| r.sense.take()).flatten();
        let sense = sense.unwrap_or_else(|| panic!("sense"));
        assert_eq!(sense.contacts.len(), UMBRA_MAX_CONTACTS_PER_SENSE);
        assert_eq!(sense.contacts[0].entity.get(), 4);
        assert!(
            active
                .finish()
                .breach
                .is_some_and(|b| b.contains("contact sink: stale or foreign"))
        );
        // SAFETY: without a record the callback refuses before reading.
        assert_eq!(unsafe { push(sink.ctx, &raw const four) }, UMBRA_ERR_STATE);
    }
}
