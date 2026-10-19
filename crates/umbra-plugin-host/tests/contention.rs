//! AT-4 `cross_thread_contention_never_faults` (RR-B1): two threads tick instances of the same image at
//! the same time. The per-image call lock is blocking, so nothing faults and results equal a serial run.
//! Own test binary: the raw plugin's concurrency counters are process-wide, and any other test ticking
//! the plugin would pollute them. Step 4 of M4: the bridge lands in WI-9.

#![expect(unsafe_code, reason = "raw_env! expands forged entry functions")]

#[macro_use]
mod support;

use std::sync::atomic::Ordering;
use std::thread;

use support::bridge::{Ent, tick_batched};
use support::raw;

#[test]
fn cross_thread_contention_never_faults() {
    let env = raw_env!();
    let b = env.behavior("spin");
    let batch = b.batch().expect("batch");
    let run = |first_id: u64| {
        let batch = batch.clone();
        let b = b.clone();
        thread::spawn(move || {
            let mut ents = vec![Ent::new(first_id, &b, 7), Ent::new(first_id + 1, &b, 7)];
            let mut trace = Vec::new();
            for tick in 0..200 {
                tick_batched(&batch, &mut ents, tick);
                trace.push(
                    ents.iter()
                        .map(|e| (e.status, e.buf.clone()))
                        .collect::<Vec<_>>(),
                );
            }
            trace
        })
    };
    let (a, c) = (run(1), run(3));
    let (trace_a, trace_c) = (a.join().expect("thread a"), c.join().expect("thread c"));

    assert_eq!(
        env.host.faults(),
        Vec::new(),
        "contention waits, it never faults"
    );
    assert_eq!(
        raw::MAX_CONCURRENT.load(Ordering::SeqCst),
        1,
        "calls into one image never overlap"
    );
    let mut serial_a = vec![Ent::new(1, &b, 7), Ent::new(2, &b, 7)];
    let mut serial = Vec::new();
    for tick in 0..200 {
        tick_batched(&batch, &mut serial_a, tick);
        serial.push(
            serial_a
                .iter()
                .map(|e| (e.status, e.buf.clone()))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(trace_a, serial, "results equal the serial run");
    assert_eq!(trace_c.len(), 200);
}
