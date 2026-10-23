//! M7 WI-11: the writer is lossless with backpressure, marks itself failed on a write error and
//! writes identical bytes for identical input (REQ-K-7, REQ-K-8, ASM-621, ASM-625).

#![forbid(unsafe_code)]
#![expect(
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "tests construct known-valid values; REQ-K-4: these tests observe a real thread blocking, not simulation logic"
)]

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{SharedBuf, every_record, header, read_bytes, write_bytes};
use umbra_core::StateHash;
use umbra_record::{
    DEFAULT_QUEUE, FaultRecord, MAX_LINE_BYTES, Reader, Record, RecordError, Writer,
};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("umbra-record-writer");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(name)
}

fn hash(tick: u64) -> Record {
    Record::Hash {
        tick,
        hash: StateHash(tick),
    }
}

/// Accepts the header write, then blocks every later write until the gate is closed.
struct StalledSink {
    inner: SharedBuf,
    gate: Receiver<()>,
    writes: usize,
}

impl Write for StalledSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writes += 1;
        if self.writes > 1 {
            let _ = self.gate.recv(); // returns when the test drops the sender
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Accepts the header write, then fails.
struct FailingSink {
    writes: usize,
}

impl Write for FailingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writes += 1;
        if self.writes > 1 {
            return Err(std::io::Error::other("disk full"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn given_a_stalled_sink_when_the_queue_is_full_then_push_blocks_and_nothing_is_dropped() {
    const TOTAL: u64 = 12;
    const QUEUE: usize = 2;
    let (open, gate) = channel();
    let buf = SharedBuf::default();
    let sink = StalledSink {
        inner: buf.clone(),
        gate,
        writes: 0,
    };
    let writer = Arc::new(Writer::from_sink(sink, &header(), QUEUE).expect("writer"));
    let pushed = Arc::new(AtomicUsize::new(0));
    let feeder = {
        let (writer, pushed) = (Arc::clone(&writer), Arc::clone(&pushed));
        std::thread::spawn(move || {
            for tick in 0..TOTAL {
                writer.push(hash(tick)).expect("push");
                pushed.fetch_add(1, Ordering::SeqCst);
            }
        })
    };

    std::thread::sleep(Duration::from_millis(400));
    let while_stalled = pushed.load(Ordering::SeqCst);
    // One line is in the stalled write, QUEUE are queued; the next push waits.
    assert!(
        (1..=QUEUE + 1).contains(&while_stalled),
        "push must block once the queue is full, {while_stalled} completed"
    );
    assert!(!feeder.is_finished());

    drop(open);
    feeder.join().expect("feeder");
    let writer = Arc::try_unwrap(writer).expect("sole owner");
    let summary = writer.finish().expect("finish");

    assert_eq!(summary.records, TOTAL);
    let (_, got) = read_bytes(buf.bytes()).expect("read");
    let expected: Vec<Record> = (0..TOTAL).map(hash).collect();
    assert_eq!(got, expected, "every record, in order");
}

#[test]
fn given_a_write_error_then_push_and_finish_report_the_recording_failed() {
    let writer = Writer::from_sink(FailingSink { writes: 0 }, &header(), 1).expect("writer");

    let deadline = Instant::now() + Duration::from_secs(10);
    let failure = loop {
        match writer.push(hash(1)) {
            Ok(()) => {}
            Err(e) => break e,
        }
        assert!(Instant::now() < deadline, "the failure must surface");
        std::thread::sleep(Duration::from_millis(2));
    };
    let again = writer.push(hash(2));
    let finished = writer.finish();

    assert!(
        matches!(&failure, RecordError::Failed(m) if m.contains("disk full")),
        "{failure:?}"
    );
    assert!(matches!(again, Err(RecordError::Failed(_))), "{again:?}");
    assert!(
        matches!(&finished, Err(RecordError::Failed(m)) if m.contains("disk full")),
        "{finished:?}"
    );
}

#[test]
fn given_a_sink_that_fails_on_the_header_then_create_is_an_io_error() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("no space"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let err = Writer::from_sink(Broken, &header(), 4).expect_err("must fail");

    assert!(matches!(err, RecordError::Io(_)), "{err:?}");
}

#[test]
fn given_the_same_inputs_when_written_to_files_twice_then_the_bytes_are_identical() {
    let write = |name: &str| {
        let path = scratch(name);
        let writer = Writer::create(&path, &header(), DEFAULT_QUEUE).expect("create");
        for r in every_record() {
            writer.push(r).expect("push");
        }
        let summary = writer.finish().expect("finish");
        let bytes = std::fs::read(&path).expect("read");
        std::fs::remove_file(&path).expect("remove");
        (bytes, summary)
    };

    let (a, summary_a) = write("identical-a.umbrarec.jsonl");
    let (b, _) = write("identical-b.umbrarec.jsonl");

    assert_eq!(a, b);
    assert_eq!(summary_a.bytes, a.len() as u64);
    assert_eq!(summary_a.records, every_record().len() as u64);
}

#[test]
fn given_a_tiny_queue_and_many_records_then_all_arrive_in_order() {
    let sink = SharedBuf::default();
    let writer = Writer::from_sink(sink.clone(), &header(), 1).expect("writer");
    for tick in 0..2000 {
        writer.push(hash(tick)).expect("push");
    }

    let summary = writer.finish().expect("finish");

    assert_eq!(summary.records, 2000);
    let (_, got) = read_bytes(sink.bytes()).expect("read");
    assert_eq!(got.len(), 2000);
    assert_eq!(got.last(), Some(&hash(1999)));
}

#[test]
fn given_a_queue_of_zero_then_the_writer_still_works() {
    let sink = SharedBuf::default();
    let writer = Writer::from_sink(sink.clone(), &header(), 0).expect("writer");
    writer.push(hash(1)).expect("push");

    writer.finish().expect("finish");

    assert_eq!(read_bytes(sink.bytes()).expect("read").1, vec![hash(1)]);
}

#[test]
fn given_a_file_recording_then_the_reader_opens_it_and_a_missing_directory_is_an_io_error() {
    let path = scratch("open.umbrarec.jsonl");
    let writer = Writer::create(&path, &header(), 4).expect("create");
    writer.push(hash(1)).expect("push");
    writer.finish().expect("finish");

    let (got_header, reader) = Reader::open(&path).expect("open");
    let got: Vec<_> = reader.collect::<Result<_, _>>().expect("records");
    std::fs::remove_file(&path).expect("remove");

    assert_eq!(got_header, header());
    assert_eq!(got, vec![hash(1)]);
    let err = Writer::create(&scratch("no-such-dir").join("x").join("y"), &header(), 4)
        .expect_err("must fail");
    assert!(matches!(err, RecordError::Io(_)), "{err:?}");
}

#[test]
fn given_the_writer_is_shared_across_threads_then_every_record_is_written() {
    let sink = SharedBuf::default();
    let writer = Arc::new(Writer::from_sink(sink.clone(), &header(), 4).expect("writer"));
    let lost = Arc::new(Mutex::new(0_u64));
    let threads: Vec<_> = (0..4_u64)
        .map(|t| {
            let (writer, lost) = (Arc::clone(&writer), Arc::clone(&lost));
            std::thread::spawn(move || {
                for i in 0..100 {
                    if writer.push(hash(t * 1000 + i)).is_err() {
                        *lost.lock().expect("lock") += 1;
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("thread");
    }

    let summary = Arc::try_unwrap(writer)
        .expect("sole owner")
        .finish()
        .expect("finish");

    assert_eq!(*lost.lock().expect("lock"), 0);
    assert_eq!(summary.records, 400);
}

#[test]
fn given_two_runs_through_memory_then_write_bytes_agree() {
    assert_eq!(
        write_bytes(&header(), &every_record()),
        write_bytes(&header(), &every_record())
    );
}

/// Records the length of the written bytes at each flush.
struct FlushCounter {
    inner: SharedBuf,
    flushes: Arc<Mutex<Vec<usize>>>,
}

impl Write for FlushCounter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let written = self.inner.bytes().len();
        self.flushes.lock().expect("flushes").push(written);
        Ok(())
    }
}

#[test]
fn given_an_idle_writer_then_it_flushes_after_the_header_and_the_record_without_finish() {
    let flushes = Arc::new(Mutex::new(Vec::new()));
    let inner = SharedBuf::default();
    let sink = FlushCounter {
        inner: inner.clone(),
        flushes: Arc::clone(&flushes),
    };
    let writer = Writer::from_sink(sink, &header(), 4).expect("writer");
    writer.push(hash(1)).expect("push");
    let after_record = common::write_bytes(&header(), &[hash(1)]).len();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !flushes
        .lock()
        .expect("flushes")
        .iter()
        .any(|&at| at >= after_record)
    {
        assert!(
            Instant::now() < deadline,
            "an idle writer must flush the record"
        );
        std::thread::sleep(Duration::from_millis(2));
    }

    writer.finish().expect("finish");
}

/// Writes slowly, so a writer dropped with a full queue still has lines to drain.
struct SlowSink(SharedBuf);

impl Write for SlowSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(3));
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn given_a_writer_dropped_without_finish_then_every_queued_line_is_written() {
    const N: u64 = 40;
    let buf = SharedBuf::default();
    let writer = Writer::from_sink(SlowSink(buf.clone()), &header(), 64).expect("writer");
    for tick in 0..N {
        writer.push(hash(tick)).expect("push");
    }

    drop(writer);

    let (_, got) = read_bytes(buf.bytes()).expect("read");
    assert_eq!(got.len() as u64, N, "drop joins the writer thread");
}

#[test]
fn given_a_record_whose_line_exceeds_the_reader_limit_then_push_refuses_it_and_the_writer_stays_usable()
 {
    let buf = SharedBuf::default();
    let writer = Writer::from_sink(buf.clone(), &header(), 4).expect("writer");
    let huge = Record::Fault(FaultRecord {
        tick: 1,
        adapter: "x".repeat(usize::try_from(MAX_LINE_BYTES).expect("limit")),
        call: "poll".to_owned(),
    });

    let refused = writer.push(huge);
    writer
        .push(hash(2))
        .expect("a normal record still goes through");
    let summary = writer.finish().expect("finish");

    assert!(
        matches!(refused, Err(RecordError::LineTooLong { .. })),
        "{refused:?}"
    );
    assert_eq!(summary.records, 1);
    let (_, got) = read_bytes(buf.bytes()).expect("read");
    assert_eq!(got, vec![hash(2)]);
}

/// Panics on the first write after the header.
struct PanickingSink(usize);

impl Write for PanickingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += 1;
        assert!(self.0 <= 1, "sink exploded");
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn given_a_sink_that_panics_then_push_and_finish_fail_without_hanging() {
    let writer = Writer::from_sink(PanickingSink(0), &header(), 1).expect("writer");

    let deadline = Instant::now() + Duration::from_secs(10);
    let failure = loop {
        if let Err(e) = writer.push(hash(1)) {
            break e;
        }
        assert!(Instant::now() < deadline, "the failure must surface");
        std::thread::sleep(Duration::from_millis(2));
    };
    let finished = writer.finish();

    assert!(matches!(failure, RecordError::Failed(_)), "{failure:?}");
    assert!(
        matches!(&finished, Err(RecordError::Failed(m)) if m.contains("panicked")),
        "{finished:?}"
    );
}

#[test]
fn given_a_line_of_exactly_the_limit_then_it_is_accepted_and_one_byte_more_is_refused() {
    let limit = usize::try_from(MAX_LINE_BYTES).expect("limit");
    let fault = |adapter_len: usize| {
        Record::Fault(FaultRecord {
            tick: 1,
            adapter: "x".repeat(adapter_len),
            call: "poll".to_owned(),
        })
    };
    let base = write_bytes(&header(), &[fault(0)]).len() - write_bytes(&header(), &[]).len() - 1;
    let buf = SharedBuf::default();
    let writer = Writer::from_sink(buf.clone(), &header(), 2).expect("writer");

    let exact = writer.push(fault(limit - base));
    let over = writer.push(fault(limit - base + 1));
    writer.finish().expect("finish");

    exact.expect("a line of exactly the limit is accepted");
    assert!(
        matches!(over, Err(RecordError::LineTooLong { bytes, limit: l }) if bytes == MAX_LINE_BYTES + 1 && l == MAX_LINE_BYTES),
        "{over:?}"
    );
    let (_, got) = read_bytes(buf.bytes()).expect("the reader accepts what the writer accepted");
    assert_eq!(got, vec![fault(limit - base)]);
}
