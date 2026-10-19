//! The grouped Behavior phase's world-side half (REQ-B-4, ADR 0020).
//!
//! Hands every member of a batch group to `tick_batch` as one [`BatchItem`] over that entity's own
//! view, RNG stream, intent buffer and instance. All buffers live in [`BatchScratch`] and keep
//! their capacity between ticks. The items borrow from the world for one call only, so the stored
//! vectors are typed `'static` and re-typed to the tick's lifetime by [`relife`]; a vector is
//! always empty while it is stored, so no reference outlives its borrow.

use std::sync::Arc;

use umbra_core::{
    BatchItem, BehaviorBatch, BehaviorContext, BehaviorInstance, EntityId, EntityView,
    IntentBuffer, KinematicState, RouteView, SimTime, Status,
};

use super::{Contacts, EntityRng, ForceTag, Intents, Route, World};
use crate::systems::route;

/// One batched entity's components, borrowed for the call.
struct Row<'a> {
    id: EntityId,
    state: &'a KinematicState,
    route: Option<&'a Route>,
    contacts: Option<&'a Contacts>,
    force: Option<&'a ForceTag>,
    rng: &'a mut EntityRng,
    intents: &'a mut Intents,
}

/// A [`Row`] with its instance and group, before it becomes a [`BatchItem`].
struct Pending<'a> {
    group: usize,
    row: Row<'a>,
    instance: &'a mut Box<dyn BehaviorInstance>,
}

type BatchQuery<'a> = (
    &'a EntityId,
    &'a KinematicState,
    Option<&'a Route>,
    Option<&'a Contacts>,
    Option<&'a ForceTag>,
    &'a mut EntityRng,
    &'a mut Intents,
);

/// Buffers of the grouped Behavior phase, reused across ticks.
#[derive(Default)]
pub(crate) struct BatchScratch {
    /// The distinct batches of this tick, ordered by the first id of their members.
    pub(crate) groups: Vec<Arc<dyn BehaviorBatch>>,
    /// Batched entities in id order with their group index.
    pub(crate) batched: Vec<(EntityId, usize)>,
    counts: Vec<usize>,
    results: Vec<(EntityId, Status)>,
    rows: Vec<Row<'static>>,
    pending: Vec<Pending<'static>>,
    items: Vec<BatchItem<'static>>,
}

impl BatchScratch {
    /// Empties the per-tick lists (the group `Arc`s are released, capacities kept).
    pub(crate) fn begin_tick(&mut self) {
        self.groups.clear();
        self.batched.clear();
    }

    /// Adds a batched entity (visited in id order); finds its group with `Arc::ptr_eq`.
    pub(crate) fn add(&mut self, id: EntityId, batch: &Arc<dyn BehaviorBatch>) {
        let found = self.groups.iter().position(|g| Arc::ptr_eq(g, batch));
        let group = found.unwrap_or_else(|| {
            self.groups.push(Arc::clone(batch));
            self.groups.len() - 1
        });
        self.batched.push((id, group));
    }

    /// Total capacity, in elements, of all buffers (the AT-6 probe).
    pub(crate) fn capacity(&self) -> usize {
        self.groups.capacity()
            + self.batched.capacity()
            + self.counts.capacity()
            + self.results.capacity()
            + self.rows.capacity()
            + self.pending.capacity()
            + self.items.capacity()
    }
}

/// Defines `$name`, which re-types an empty vector to another lifetime and keeps its allocation
/// (an in-place `collect` of a same-layout iterator). It empties the vector first, so nothing
/// dangles.
macro_rules! relife {
    ($name:ident, $ty:ident) => {
        fn $name<'b>(mut v: Vec<$ty<'_>>) -> Vec<$ty<'b>> {
            v.clear();
            v.into_iter().filter_map(|_| None).collect()
        }
    };
}

relife!(relife_rows, Row);
relife!(relife_pending, Pending);
relife!(relife_items, BatchItem);

impl World {
    /// Ticks every group once, in group order, members in id order, and writes each member's
    /// `last` status (ADR 0020). `scratch.batched` lists the entities, built by the first pass.
    pub(crate) fn tick_batches(&mut self, scratch: &mut BatchScratch, now: SimTime) {
        let BatchScratch {
            groups,
            batched,
            counts,
            results,
            rows,
            pending,
            items,
        } = scratch;
        let mut rows_t = relife_rows(std::mem::take(rows));
        for (id, state, route, contacts, force, rng, intents) in
            self.ecs.query_mut::<BatchQuery<'_>>()
        {
            if batched.binary_search_by_key(id, |b| b.0).is_ok() {
                rows_t.push(Row {
                    id: *id,
                    state,
                    route,
                    contacts,
                    force,
                    rng,
                    intents,
                });
            }
        }
        rows_t.sort_unstable_by_key(|r| r.id);
        let mut pending_t = relife_pending(std::mem::take(pending));
        let slots = self.behaviors.values_mut().filter(|s| s.batch.is_some());
        for ((row, slot), &(id, group)) in rows_t.drain(..).zip(slots).zip(batched.iter()) {
            debug_assert_eq!(row.id, id, "rows and slots cover the same entities");
            pending_t.push(Pending {
                group,
                row,
                instance: &mut slot.instance,
            });
        }
        *rows = relife_rows(rows_t);
        pending_t.sort_unstable_by_key(|p| (p.group, p.row.id));
        let mut items_t = relife_items(std::mem::take(items));
        items_t.extend(pending_t.drain(..).map(|p| make_item(p, now)));
        *pending = relife_pending(pending_t);
        counts.clear();
        counts.resize(groups.len(), 0);
        for (_, group) in batched.iter() {
            if let Some(n) = counts.get_mut(*group) {
                *n += 1;
            }
        }
        results.clear();
        let mut start = 0;
        for (group, count) in groups.iter().zip(counts.iter()) {
            let Some(members) = items_t.get_mut(start..start + count) else {
                debug_assert!(false, "item count matches group sizes");
                break;
            };
            group.tick_batch(members);
            results.extend(members.iter().map(|i| (i.entity.id, i.status)));
            start += count;
        }
        *items = relife_items(items_t);
        results.sort_unstable_by_key(|r| r.0);
        let slots = self.behaviors.values_mut().filter(|s| s.batch.is_some());
        for (slot, (_, status)) in slots.zip(results.iter()) {
            slot.last = Some(*status);
        }
    }

    /// Visits the batched entities in id order for post-processing (the intents, the route and
    /// the pre-tick state).
    pub(crate) fn for_each_batched_mut(
        &mut self,
        batched: &[(EntityId, usize)],
        mut f: impl FnMut(&mut Intents, Option<&mut Route>, &KinematicState),
    ) {
        for (id, _) in batched {
            let Some(entity) = self.index.get(id) else {
                debug_assert!(false, "a batched entity is indexed");
                continue;
            };
            let Ok((state, intents, route)) =
                self.ecs
                    .query_one_mut::<(&KinematicState, &mut Intents, Option<&mut Route>)>(*entity)
            else {
                debug_assert!(false, "every indexed entity has all kernel components");
                continue;
            };
            f(intents, route, state);
        }
    }
}

fn make_item(p: Pending<'_>, now: SimTime) -> BatchItem<'_> {
    let Row {
        id,
        state,
        route: r,
        contacts,
        force,
        rng,
        intents,
    } = p.row;
    let mut view = EntityView::new(id, state).with_force(force.map(|f| &f.0));
    if let Some(r) = r {
        view = view.with_route(RouteView::new(
            &r.waypoints,
            r.next,
            route::is_complete(r, state.position),
        ));
    }
    if let Some(c) = contacts {
        view = view.with_contacts(&c.0);
    }
    BatchItem::new(
        BehaviorContext::new(now, &mut rng.0),
        view,
        &mut **p.instance,
        IntentBuffer::new(&mut intents.0),
    )
}
