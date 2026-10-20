//! State shared between the handles, the engine thread and the [`EngineThread`] owner, and the
//! shutdown protocol (ADR 0024, review M12).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use super::{Command, EngineThread, HandleError};
use crate::bus::SlotId;

/// What the threads share besides the command channel and the bus.
pub(super) struct Shared {
    /// Set by [`EngineThread`] before it wakes the thread; read at the top of every loop turn and
    /// between commands, so a full queue or a busy tick cannot hide it.
    pub(super) shutdown: AtomicBool,
    /// Frames the thread has built (`EngineHandle::snapshot_builds`).
    pub(super) builds: AtomicU64,
    /// Set when the engine thread has ended, however it ended (`EngineHandle::is_closed`).
    closed: AtomicBool,
    /// Whether `primes` may be non-empty; lets the loop skip the lock on almost every turn.
    needs_prime: AtomicBool,
    /// Subscriptions waiting for their priming frame.
    primes: Mutex<Vec<SlotId>>,
}

impl Shared {
    pub(super) fn new() -> Self {
        Self {
            shutdown: AtomicBool::new(false),
            builds: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            needs_prime: AtomicBool::new(false),
            primes: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub(super) fn is_shutting_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Marks `slot` as needing a priming frame.
    pub(super) fn mark_for_priming(&self, slot: SlotId) {
        self.primes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(slot);
        self.needs_prime.store(true, Ordering::Release);
    }

    /// The marked slots, in the order they were marked; clears the marks.
    pub(super) fn take_primes(&self) -> Vec<SlotId> {
        if !self.needs_prime.swap(false, Ordering::AcqRel) {
            return Vec::new();
        }
        std::mem::take(&mut *self.primes.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl EngineThread {
    /// Sets the flag, wakes the thread if it is parked, and joins it. `Err(Closed)` when the thread
    /// ended by a panic. Never waits for room in the command queue.
    pub(super) fn stop_and_join(&mut self) -> Result<(), HandleError> {
        let Some(join) = self.join.take() else {
            return Ok(());
        };
        self.shared.shutdown.store(true, Ordering::Release);
        if join.thread().id() == std::thread::current().id() {
            // Dropped from inside a reply callback, on the engine thread itself: it cannot join
            // itself. The flag ends it as soon as the callback returns.
            return Ok(());
        }
        // Full: the thread is iterating and sees the flag. Dropped senders: it has seen the
        // disconnect (or is about to).
        if let Some(commands) = self.wake.upgrade() {
            drop(commands.try_send(Command::Shutdown));
        }
        join.join().map_err(|_| HandleError::Closed)
    }
}

impl Drop for EngineThread {
    fn drop(&mut self) {
        // A panic of the engine thread was already logged there.
        drop(self.stop_and_join());
    }
}
