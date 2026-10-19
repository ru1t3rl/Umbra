//! AT-4 (callback and token rows): tokens are opaque, generation-checked and resolved against the calling
//! thread's active call only; host callbacks never unwind. Requirements: REQ-P-5, REQ-P-7, REQ-K-4,
//! ADR 0019 (R-B4, RR-M2), ADR 0021 (host API v0). Raw-ABI plugin from `support/raw.rs`; the build helper
//! is a plan gap (`support::build_behavior`). Step 4 of M4: the bridge landed in WI-9.
#![expect(unsafe_code, reason = "calls the host API like a plugin would")]
#![expect(clippy::expect_used, reason = "helpers build known-valid fixtures")]

#[macro_use]
mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

use support::Recorder;
use support::bridge::{Ent, eid, tick_batched};
use support::raw;
use umbra_core::{
    Behavior, BehaviorBatch, Intent, MetersPerSecond, PluginId, SimRng, Status, StreamKey,
};
use umbra_plugin_api::{UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UmbraHostApi};
use umbra_plugin_host::{HostOptions, LogLevel, PluginLog};

fn batch_of(b: &Arc<dyn Behavior>) -> Arc<dyn BehaviorBatch> {
    b.batch().expect("batch")
}

fn host_api(env: &support::bridge::Env) -> UmbraHostApi {
    let p = env.f.host.load(Ordering::SeqCst) as *const UmbraHostApi;
    // SAFETY: the host API is a process-lifetime struct (ADR 0021).
    unsafe { std::ptr::read(p) }
}

fn draw(api: &UmbraHostApi, token: u64) -> i32 {
    let mut out = 0_u64;
    // SAFETY: valid API struct and out pointer.
    unsafe { (api.rng_next_u64.expect("rng"))(api.host_ctx, token, &raw mut out) }
}

/// R-B4: a token used after its call finished is refused (no call record on this thread).
#[test]
fn rng_draw_outside_a_call_is_err_state() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("stale");
    let mut ents = vec![Ent::new(1, &b, 3)];
    tick_batched(&batch_of(&b), &mut ents, 0);
    let (_, token) = raw::KEPT
        .with(std::cell::Cell::get)
        .expect("the plugin kept its token");

    let code = draw(&host_api(&env), token);

    assert_eq!(code, UMBRA_ERR_STATE);
}

/// RR-M2: callbacks from another thread find no call record: `UMBRA_ERR_STATE`.
#[test]
fn callback_from_another_thread_is_err_state() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("stale");
    let mut ents = vec![Ent::new(1, &b, 3)];
    tick_batched(&batch_of(&b), &mut ents, 0);
    let (_, token) = raw::KEPT.with(std::cell::Cell::get).expect("kept token");
    let api = host_api(&env);

    let code = thread::spawn(move || draw(&api, token))
        .join()
        .expect("thread");

    assert_eq!(code, UMBRA_ERR_STATE);
}

/// R-B4: a token kept across calls (and the sink ctx with it) is refused in the next call and faults the
/// capability; the batch is discarded.
#[test]
fn stale_token_in_a_later_call_is_a_fault() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("stale");
    let batch = batch_of(&b);
    let mut ents = vec![Ent::new(1, &b, 3)];
    tick_batched(&batch, &mut ents, 0);
    assert_eq!(env.host.faults(), Vec::new(), "the first call is clean");

    tick_batched(&batch, &mut ents, 1);

    let faults = env.host.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].call, "tick_batch");
    assert!(faults[0].message.contains("token"), "{}", faults[0].message);
    assert_eq!(ents[0].status, Some(Status::Failure));
    assert_eq!(ents[0].buf, Vec::new());
}

/// ADR 0021: drawing with another item's token of the same call is a contract breach the host cannot
/// detect; it draws that item's stream, deterministically.
#[test]
fn rng_with_another_items_token_draws_that_items_stream() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("foreign_rng");
    let mut ents = vec![Ent::new(1, &b, 5), Ent::new(2, &b, 5)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    let mut second = SimRng::new(5).stream(StreamKey::entity(eid(2)));
    let (first_draw, second_draw) = (second.next_u64(), second.next_u64());
    assert_eq!(
        ents[0].buf,
        vec![Intent::SetSpeed(MetersPerSecond(raw::speed_of(first_draw)))]
    );
    assert_eq!(
        ents[1].buf,
        vec![Intent::SetSpeed(MetersPerSecond(raw::speed_of(
            second_draw
        )))]
    );
    let untouched = SimRng::new(5).stream(StreamKey::entity(eid(1)));
    assert_eq!(
        ents[0].rng.state(),
        untouched.state(),
        "item 1's own stream was not used"
    );
    assert_eq!(
        env.host.faults(),
        Vec::new(),
        "defined behavior, not a fault"
    );
}

/// ADR 0021: `log` routes to the host's `PluginLog` attributed to the plugin.
#[test]
fn log_during_a_call_is_routed_and_attributed() {
    raw::clear();
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    let env = raw_env_with!(options, true);
    let b = env.behavior("log");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    assert_eq!(raw::codes(), [0]);
    let got = recorder.all();
    assert!(
        got.iter()
            .any(|e| e.0 == "test.plugin.raw" && e.1 == LogLevel::Info && e.2 == "raw says hello"),
        "{got:?}"
    );
}

/// ADR 0021: an unknown log level is `UMBRA_ERR_INVALID_ARGUMENT` and nothing is logged.
#[test]
fn log_with_an_unknown_level_is_invalid_argument() {
    raw::clear();
    let recorder = Recorder::arc();
    let mut options = HostOptions::default();
    options.log = recorder.clone();
    let env = raw_env_with!(options, true);
    let b = env.behavior("log_bad_level");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    assert_eq!(raw::codes(), [UMBRA_ERR_INVALID_ARGUMENT]);
    assert!(recorder.all().iter().all(|e| e.2 != "raw says hello"));
}

/// ADR 0021: invalid UTF-8 in a log message is `UMBRA_ERR_INVALID_ARGUMENT`.
#[test]
fn log_with_invalid_utf8_is_invalid_argument() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("log_bad_utf8");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    assert_eq!(raw::codes(), [UMBRA_ERR_INVALID_ARGUMENT]);
}

/// A `PluginLog` that panics.
struct Boom;

impl PluginLog for Boom {
    fn log(&self, _: &PluginId, _: LogLevel, _: &str) {
        panic!("log sink boom");
    }
}

/// R-M1: host callbacks catch panics (here from the embedder's `PluginLog`); the plugin gets an error
/// code instead of an unwind through its frames.
#[test]
fn a_panicking_plugin_log_is_caught_at_the_boundary() {
    raw::clear();
    let mut options = HostOptions::default();
    options.log = Arc::new(Boom);
    let env = raw_env_with!(options, true);
    let b = env.behavior("log");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    let codes = raw::codes();
    assert_eq!(codes.len(), 1);
    assert_ne!(
        codes[0], 0,
        "the callback reported the panic as an error code"
    );
}

/// R-M2: an error message longer than 4 KiB is truncated at 4 KiB.
#[test]
fn error_sink_with_a_huge_length_is_truncated_to_4_kib() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("huge_error");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    let faults = env.host.faults();
    assert_eq!(faults.len(), 1);
    let message = &faults[0].message;
    assert!(
        message.contains(&"x".repeat(4096)) && !message.contains(&"x".repeat(4097)),
        "the plugin's text is cut at exactly 4096 bytes: {} bytes of message",
        message.len()
    );
}

/// R-M2: a null pointer with a non-zero length is refused without reading; the call still faults.
#[test]
fn error_sink_with_a_null_pointer_and_a_length_does_not_crash() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("null_error");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    assert_eq!(env.host.faults().len(), 1);
}

/// ADR 0019: the first error message of a call wins.
#[test]
fn second_error_message_is_ignored() {
    raw::clear();
    let env = raw_env!();
    let b = env.behavior("two_errors");
    let mut ents = vec![Ent::new(1, &b, 3)];

    tick_batched(&batch_of(&b), &mut ents, 0);

    let message = env.host.faults()[0].message.clone();
    assert!(
        message.contains("first message") && !message.contains("second message"),
        "{message}"
    );
}
