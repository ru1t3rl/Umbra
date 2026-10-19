//! The process-wide image table (R-B5, ADR 0021 "One image per process" and "Image lifecycle states",
//! RR-M1, ASM-122).
//!
//! The operating system's loader shares one image of a library per process, so the host mirrors it: one
//! [`LoadedImage`] per image lifecycle, shared by `Arc` across hosts. The table is the **only persistent
//! process-global mutable state** of the host (an accepted exception to section 16, ASM-122); it never
//! affects simulation results (fault state is per host, ASM-118).
//!
//! Each slot has up to two keys, the canonical library path (before opening) and the address of
//! `umbra_plugin_entry` (after opening, **before** calling it), a state and a `Weak` to the image:
//!
//! - `Loading`: a thread is opening the library, calling the entry, validating or running `init`;
//! - `Live`: published; a load reuses it (no entry call) while the `Weak` upgrades;
//! - `Closing`: the last `Arc` is gone and `shutdown` / the close are running.
//!
//! A load that meets `Loading`, `Closing` or a `Live` slot whose image is already dropping waits on the
//! table's condvar and retries; when the slot is gone it starts a new lifecycle. Slots are removed when
//! their lifecycle ends (no dead entry outlives its image's `Drop`).
//!
//! Lock order (ADR 0019, RR-M2): (1) this table, (2) the per-image call lock, (3) per-host fault state,
//! (4) `PluginLog`. The table lock is never held while plugin code runs, and never while an
//! `Arc<LoadedImage>` may drop: it only upgrades `Weak`s and hands the `Arc` out.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

use crate::lifecycle::LoadedImage;
use crate::tokens;

/// Lifecycle state of a slot (RR-M1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Loading,
    Live,
    Closing,
}

/// One image lifecycle in the table.
struct Slot {
    id: u64,
    path: Option<PathBuf>,
    entry: Option<usize>,
    state: State,
    image: Weak<LoadedImage>,
}

/// The table: slots and the counters that name them.
struct Table {
    slots: Vec<Slot>,
    next_id: u64,
    generation: u32,
}

static TABLE: Mutex<Table> = Mutex::new(Table {
    slots: Vec::new(),
    next_id: 1,
    generation: 0,
});
/// Signalled whenever a slot is published or removed.
static CHANGED: Condvar = Condvar::new();

/// Locks the table. Nothing under the lock panics or runs foreign code, so a poisoned lock (impossible
/// in practice) still holds a consistent table and is recovered.
fn lock() -> MutexGuard<'static, Table> {
    TABLE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait(guard: MutexGuard<'static, Table>) -> MutexGuard<'static, Table> {
    CHANGED.wait(guard).unwrap_or_else(PoisonError::into_inner)
}

/// What a lookup found.
pub(crate) enum Claim {
    /// A live image to reuse (no entry call).
    Live(Arc<LoadedImage>),
    /// A new lifecycle: the caller owns the `Loading` slot.
    New(Reservation),
}

/// A `Loading` slot owned by the thread that creates the lifecycle. Dropping it before
/// [`Reservation::into_slot`] removes the slot and wakes waiters (a failed load).
pub(crate) struct Reservation {
    id: u64,
    generation: u32,
    armed: bool,
}

impl Reservation {
    /// The lifecycle generation (high half of `host_ctx`).
    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }

    /// Hands the slot to the image, whose `Drop` ends the lifecycle ([`closing`], [`closed`]).
    pub(crate) fn into_slot(mut self) -> u64 {
        self.armed = false;
        self.id
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.armed {
            closed(self.id);
        }
    }
}

impl Table {
    fn find(&self, key: Key<'_>, except: u64) -> Option<usize> {
        self.slots.iter().position(|s| {
            s.id != except
                && match key {
                    Key::Path(p) => s.path.as_deref() == Some(p),
                    Key::Entry(e) => s.entry == Some(e),
                }
        })
    }

    fn reserve(&mut self, path: Option<PathBuf>, entry: Option<usize>) -> Reservation {
        let id = self.next_id;
        self.next_id += 1;
        self.generation = tokens::next_generation(self.generation);
        self.slots.push(Slot {
            id,
            path,
            entry,
            state: State::Loading,
            image: Weak::new(),
        });
        Reservation {
            id,
            generation: self.generation,
            armed: true,
        }
    }
}

#[derive(Clone, Copy)]
enum Key<'a> {
    Path(&'a Path),
    Entry(usize),
}

/// Looks the canonical library `path` up: a live image to reuse, or a new `Loading` slot keyed by the
/// path. Waits while the slot is `Loading` or `Closing`.
pub(crate) fn claim_path(path: &Path) -> Claim {
    let mut table = lock();
    loop {
        match table.find(Key::Path(path), 0) {
            None => return Claim::New(table.reserve(Some(path.to_path_buf()), None)),
            Some(at) => {
                let slot = &table.slots[at];
                if slot.state == State::Live
                    && let Some(image) = slot.image.upgrade()
                {
                    return Claim::Live(image);
                }
            }
        }
        table = wait(table);
    }
}

/// Looks the entry address up, after opening and before calling the entry. With a reservation from
/// [`claim_path`], a live image found under the address replaces it (the reservation's slot is removed);
/// otherwise the address becomes a key of the reserved slot. Without one (`load_static`), a new `Loading`
/// slot keyed by the address is reserved. Waits while a slot with the address is `Loading` or `Closing`.
pub(crate) fn claim_entry(reservation: Option<Reservation>, entry: usize) -> Claim {
    let own = reservation.as_ref().map_or(0, |r| r.id);
    let mut table = lock();
    loop {
        match table.find(Key::Entry(entry), own) {
            None => {
                return Claim::New(match reservation {
                    Some(r) => {
                        if let Some(slot) = table.slots.iter_mut().find(|s| s.id == r.id) {
                            slot.entry = Some(entry);
                        }
                        r
                    }
                    None => table.reserve(None, Some(entry)),
                });
            }
            Some(at) => {
                let slot = &table.slots[at];
                if slot.state == State::Live
                    && let Some(image) = slot.image.upgrade()
                {
                    if let Some(mut r) = reservation {
                        table.slots.retain(|s| s.id != r.id);
                        r.armed = false;
                        CHANGED.notify_all();
                    }
                    return Claim::Live(image);
                }
            }
        }
        table = wait(table);
    }
}

/// Publishes the image of slot `id`: `Live`, reusable from now on.
pub(crate) fn publish(id: u64, image: &Arc<LoadedImage>) {
    let mut table = lock();
    if let Some(slot) = table.slots.iter_mut().find(|s| s.id == id) {
        slot.state = State::Live;
        slot.image = Arc::downgrade(image);
    }
    drop(table);
    CHANGED.notify_all();
}

/// Marks slot `id` `Closing` (its image is dropping).
pub(crate) fn closing(id: u64) {
    let mut table = lock();
    if let Some(slot) = table.slots.iter_mut().find(|s| s.id == id) {
        slot.state = State::Closing;
    }
}

/// Removes slot `id` (its lifecycle ended) and wakes waiters.
pub(crate) fn closed(id: u64) {
    let mut table = lock();
    table.slots.retain(|s| s.id != id);
    drop(table);
    CHANGED.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_reservation_frees_its_keys() {
        let path = Path::new("/nonexistent/unit-test/libfree.so");
        let Claim::New(first) = claim_path(path) else {
            panic!("fresh path");
        };
        assert_ne!(first.generation(), 0);
        let Claim::New(first) = claim_entry(Some(first), 0x1000_0001) else {
            panic!("fresh entry");
        };
        drop(first);
        let Claim::New(again) = claim_entry(None, 0x1000_0001) else {
            panic!("the dropped reservation's entry key is free");
        };
        drop(again);
        let Claim::New(again) = claim_path(path) else {
            panic!("the dropped reservation's path key is free");
        };
        drop(again);
    }
}
