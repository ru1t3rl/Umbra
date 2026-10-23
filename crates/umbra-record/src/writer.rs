//! The recording writer: a bounded queue to a writer thread. Lossless, with backpressure
//! (ASM-625, ADR 0053).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::encode;
use crate::reader::MAX_LINE_BYTES;
use crate::{Header, Record, RecordError, WriteSummary};

/// The first write failure of the writer thread, shared with the pushing side.
type FailureSlot = Arc<Mutex<Option<String>>>;

/// Writes a recording through a bounded queue to a writer thread; lossless, with backpressure
/// (ASM-625).
///
/// Records are encoded on the calling thread and handed over as finished lines, so the bytes
/// depend only on the records pushed (ASM-621). When the queue is full `push` waits; it never
/// drops a record. A write error stops the thread, marks the writer failed and makes every later
/// `push` and `finish` return [`RecordError::Failed`]; the caller decides what a failed recording
/// means (REQ-K-8).
#[derive(Debug)]
pub struct Writer {
    lines: Option<SyncSender<String>>,
    failure: FailureSlot,
    thread: Option<JoinHandle<WriteSummary>>,
}

impl Writer {
    /// Creates the file and writes the header; `queue` is the number of records buffered (at
    /// least 1).
    ///
    /// # Errors
    /// [`RecordError::Io`] if the file cannot be created or the header cannot be written.
    pub fn create(path: &Path, header: &Header, queue: usize) -> Result<Self, RecordError> {
        Self::from_sink(BufWriter::new(File::create(path)?), header, queue)
    }

    /// Like [`Writer::create`] over any sink (an in-memory buffer, a socket, a test double).
    ///
    /// # Errors
    /// [`RecordError::Io`] if the header cannot be written.
    pub fn from_sink<W: Write + Send + 'static>(
        mut sink: W,
        header: &Header,
        queue: usize,
    ) -> Result<Self, RecordError> {
        let mut line = encode::header(header);
        line.push('\n');
        sink.write_all(line.as_bytes())?;
        let (lines, queued) = sync_channel(queue.max(1));
        let failure = FailureSlot::default();
        let slot = Arc::clone(&failure);
        let first = line.len() as u64;
        let thread = std::thread::Builder::new()
            .name("umbra-record-writer".to_owned())
            .spawn(move || drain(&queued, sink, first, &slot))?;
        Ok(Self {
            lines: Some(lines),
            failure,
            thread: Some(thread),
        })
    }

    /// Queues a record; blocks while the queue is full, never drops.
    ///
    /// A record whose encoded line exceeds [`MAX_LINE_BYTES`] would be rejected by the reader, so
    /// it is refused here and the writer stays usable: the recorder splits a large inbound batch
    /// into several records of the same tick.
    ///
    /// # Errors
    /// [`RecordError::Failed`] once the writer thread failed; [`RecordError::Unsupported`] for a
    /// value format v1 cannot spell and [`RecordError::LineTooLong`] for an oversize record
    /// (nothing is queued in either case).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the skeleton signature (ADR 0053) takes the record; callers build it per call"
    )]
    pub fn push(&self, record: Record) -> Result<(), RecordError> {
        let mut line = encode::record(&record)?;
        if line.len() as u64 > MAX_LINE_BYTES {
            return Err(RecordError::LineTooLong {
                bytes: line.len() as u64,
                limit: MAX_LINE_BYTES,
            });
        }
        line.push('\n');
        let closed = || {
            RecordError::Failed(
                failure_of(&self.failure).unwrap_or_else(|| "the writer thread ended".to_owned()),
            )
        };
        self.lines
            .as_ref()
            // unreachable: `lines` is `None` only after `finish` or `drop`, which consume the writer
            .ok_or_else(closed)?
            .send(line)
            .map_err(|_| closed())
    }

    /// Flushes everything and joins the writer thread.
    ///
    /// # Errors
    /// [`RecordError::Failed`] if any write failed.
    pub fn finish(mut self) -> Result<WriteSummary, RecordError> {
        drop(self.lines.take());
        let summary = self
            .thread
            .take()
            // unreachable: `finish` consumes the writer, so the handle is still there
            .ok_or_else(|| RecordError::Failed("the writer was already finished".to_owned()))?
            .join()
            .map_err(|_| RecordError::Failed("the writer thread panicked".to_owned()))?;
        match failure_of(&self.failure) {
            Some(message) => Err(RecordError::Failed(message)),
            None => Ok(summary),
        }
    }
}

/// Dropping a writer without `finish` still drains the queue and joins the thread, so an early
/// return in the caller cannot lose the tail of the recording. The drop blocks until the queue has
/// drained, also during unwinding. The result is ignored; call `finish` to learn whether the
/// recording is complete.
impl Drop for Writer {
    fn drop(&mut self) {
        drop(self.lines.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn failure_of(slot: &FailureSlot) -> Option<String> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// The writer thread: writes lines in order, flushing whenever the queue runs dry. On the first
/// error it records the message and stops, which closes the queue for the pushers.
fn drain<W: Write>(
    queued: &Receiver<String>,
    mut sink: W,
    header_bytes: u64,
    slot: &FailureSlot,
) -> WriteSummary {
    let mut summary = WriteSummary {
        records: 0,
        bytes: header_bytes,
    };
    let result = (|| -> std::io::Result<()> {
        loop {
            let line = match queued.try_recv() {
                Ok(line) => line,
                Err(TryRecvError::Empty) => {
                    sink.flush()?;
                    match queued.recv() {
                        Ok(line) => line,
                        Err(_) => break,
                    }
                }
                Err(TryRecvError::Disconnected) => break,
            };
            sink.write_all(line.as_bytes())?;
            summary.records += 1;
            summary.bytes += line.len() as u64;
        }
        sink.flush()
    })();
    if let Err(error) = result {
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(error.to_string());
    }
    summary
}
