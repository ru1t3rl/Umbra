//! Turning the frames a connection drained into one wire frame, and wire messages into text
//! (REQ-X-6, ADR 0027).
//!
//! Everything here is plain computation over owned or shared data, so the writer runs it on
//! `spawn_blocking` and no engine or bus lock is held (ASM-206).

use std::collections::BTreeSet;
use std::sync::Arc;

use umbra_api_types as api;
use umbra_engine::SimEvent;
use umbra_engine::bus::{Frame, FrameCause};

use super::filter::Filter;
use crate::map;

/// What one send is made of: the frames drained from the subscription and what the engine bus
/// dropped on the way.
pub(crate) struct Delivery {
    /// The queued frames, oldest first; only the last is sent as a snapshot (never empty).
    pub(crate) frames: Vec<Arc<Frame>>,
    /// Events of frames the bus dropped, to come first.
    pub(crate) carried_events: Vec<SimEvent>,
    /// The tick of each of `carried_events`.
    pub(crate) carried_ticks: Vec<u64>,
    /// Whether the bus lost carried events.
    pub(crate) bus_truncated: bool,
    /// Frames never sent since the previous frame: the bus's drops plus the queued frames skipped.
    pub(crate) dropped: u64,
    /// The frame's number in its subscription.
    pub(crate) seq: u64,
}

/// What the subscription asked for.
pub(crate) struct Wanted {
    pub(crate) filter: Arc<Filter>,
    pub(crate) max_events: usize,
    pub(crate) state_hash: bool,
}

fn wire_cause(cause: FrameCause) -> api::FrameCause {
    match cause {
        FrameCause::Tick => api::FrameCause::Tick,
        // `Control`, and any cause added later (the enum is non-exhaustive).
        _ => api::FrameCause::Control,
    }
}

/// The wire frame of `delivery`, or `None` if it holds no frame.
pub(crate) fn build_frame(delivery: &Delivery, wanted: &Wanted) -> Option<api::Frame> {
    let newest = delivery.frames.last()?;
    let (tick, time_s) = newest
        .snapshot
        .as_ref()
        .map_or((0, 0.0), |s| (s.time.tick(), s.time.as_seconds().0));
    let entities = entities_of(newest, &wanted.filter);
    let shown: BTreeSet<u64> = entities.iter().map(|e| e.id).collect();
    let (events, capped) = events_of(delivery, wanted, &shown);
    Some(api::Frame {
        kind: api::FrameKind::Snapshot,
        cause: wire_cause(newest.cause),
        session: newest.run.session,
        seq: delivery.seq,
        tick,
        time_s,
        run: map::run_status(&newest.run),
        dropped: delivery.dropped,
        entities,
        events,
        events_truncated: delivery.bus_truncated || capped,
        state_hash: wanted
            .state_hash
            .then(|| newest.state_hash.map(|hash| hash.to_string()))
            .flatten(),
    })
}

/// The frame's entities that pass the filter, ascending by id (the snapshot's order).
fn entities_of(frame: &Frame, filter: &Filter) -> Vec<api::EntitySummary> {
    let Some(snapshot) = &frame.snapshot else {
        return Vec::new();
    };
    snapshot
        .entities
        .iter()
        .filter(|e| filter.keeps_id(e.id.get()))
        // Entities of the snapshot without a directory entry are left out, as in the listing.
        .filter_map(|e| Some(map::summary_of(frame.directory.get(&e.id)?, e)))
        .filter(|summary| filter.keeps_position(summary.lat_deg, summary.lon_deg))
        .collect()
}

/// The events to send, in order: carried, then those of each queued frame. Those of kinds the
/// filter excludes, or of entities it excludes, are left out; an event of no entity is kept.
/// `true` if more than `max_events` events passed.
fn events_of(
    delivery: &Delivery,
    wanted: &Wanted,
    shown: &BTreeSet<u64>,
) -> (Vec<api::EventDto>, bool) {
    let carried = delivery
        .carried_events
        .iter()
        .enumerate()
        .map(|(i, event)| (event, delivery.carried_ticks.get(i).copied()));
    let queued =
        delivery.frames.iter().flat_map(|frame| {
            let at = frame.snapshot.as_ref().map_or(0, |s| s.time.tick());
            frame.events.iter().enumerate().map(move |(i, event)| {
                (event, Some(frame.event_ticks.get(i).copied().unwrap_or(at)))
            })
        });
    let mut out = Vec::new();
    for (event, tick) in carried.chain(queued) {
        if !wanted.filter.keeps_kind(event.kind_name()) || !concerns_shown(event, wanted, shown) {
            continue;
        }
        if out.len() == wanted.max_events {
            return (out, true);
        }
        out.push(map::event_dto(event, tick.unwrap_or(0)));
    }
    (out, false)
}

/// Whether an event passes the entity filters: it concerns no entity, or at least one that passes
/// (the id filter by id, the box by the frame's shown entities).
fn concerns_shown(event: &SimEvent, wanted: &Wanted, shown: &BTreeSet<u64>) -> bool {
    let entities = map::event_entities(event);
    if entities.is_empty() {
        return true;
    }
    entities.iter().any(|id| {
        wanted.filter.keeps_id(id.get()) && (!wanted.filter.has_bbox() || shown.contains(&id.get()))
    })
}

/// The JSON text of a server message.
///
/// # Errors
/// A serialisation error (not expected for these types).
pub(crate) fn to_text(message: &api::ServerMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use umbra_engine::bus::{RunState, RunStatus};
    use umbra_engine::{ClockMode, ComponentId, ControlAction, EntityId};

    use super::*;

    fn id(n: u64) -> EntityId {
        EntityId::new(n).expect("entity id")
    }

    fn custom(source: u64) -> SimEvent {
        SimEvent::Custom {
            source: Some(id(source)),
            kind: ComponentId::new("test.kind").expect("component id"),
            payload: json!({"n": source}),
        }
    }

    fn frame(events: Vec<SimEvent>, ticks: Vec<u64>) -> Arc<Frame> {
        let mut frame = Frame::new(
            0,
            FrameCause::Tick,
            RunStatus::new(RunState::Running, ClockMode::Stepped, 1),
        );
        frame.events = events;
        frame.event_ticks = ticks;
        Arc::new(frame)
    }

    fn delivery(frames: Vec<Arc<Frame>>) -> Delivery {
        Delivery {
            frames,
            carried_events: Vec::new(),
            carried_ticks: Vec::new(),
            bus_truncated: false,
            dropped: 0,
            seq: 0,
        }
    }

    fn wanted(filter: Filter, max_events: usize) -> Wanted {
        Wanted {
            filter: Arc::new(filter),
            max_events,
            state_hash: false,
        }
    }

    fn kinds_and_ticks(frame: &api::Frame) -> Vec<(String, u64)> {
        frame
            .events
            .iter()
            .map(|e| (e.kind.clone(), e.tick))
            .collect()
    }

    #[test]
    fn events_come_carried_first_then_queued_each_with_its_tick() {
        let mut d = delivery(vec![
            frame(vec![custom(1)], vec![30]),
            frame(vec![SimEvent::EntityRemoved { entity: id(2) }], vec![40]),
        ]);
        d.carried_events = vec![custom(3)];
        d.carried_ticks = vec![10];
        let out = build_frame(&d, &wanted(Filter::default(), 10)).expect("frame");
        let ticks: Vec<u64> = out.events.iter().map(|e| e.tick).collect();
        assert_eq!(ticks, [10, 30, 40]);
        assert_eq!(
            out.events.iter().map(|e| e.entity_id).collect::<Vec<_>>(),
            [Some(3), Some(1), Some(2)]
        );
        assert!(!out.events_truncated);
    }

    #[test]
    fn events_over_the_cap_are_cut_and_flagged_and_exactly_the_cap_is_not() {
        let many = |n| {
            delivery(vec![frame(
                (1..=n).map(custom).collect(),
                (1..=n).collect(),
            )])
        };
        let cut = build_frame(&many(5), &wanted(Filter::default(), 3)).expect("frame");
        assert_eq!(cut.events.len(), 3);
        assert_eq!(
            cut.events.iter().map(|e| e.tick).collect::<Vec<_>>(),
            [1, 2, 3],
            "the first events are kept"
        );
        assert!(cut.events_truncated);
        let exact = build_frame(&many(3), &wanted(Filter::default(), 3)).expect("frame");
        assert_eq!(exact.events.len(), 3);
        assert!(!exact.events_truncated, "nothing was lost");
        let mut lost = many(1);
        lost.bus_truncated = true;
        let flagged = build_frame(&lost, &wanted(Filter::default(), 3)).expect("frame");
        assert!(flagged.events_truncated, "the bus's flag is passed on");
    }

    #[test]
    fn event_filters_by_kind_and_by_entity_keep_events_of_no_entity() {
        let events = vec![
            custom(1),
            custom(2),
            SimEvent::EntityRemoved { entity: id(2) },
            SimEvent::SimulationControl(ControlAction::Pause),
        ];
        let d = delivery(vec![frame(events, vec![1, 2, 3, 4])]);
        let filter = |wire: api::StreamFilter| Filter::from_wire(Some(&wire), 10).expect("filter");
        let by_kind = filter(api::StreamFilter {
            event_kinds: Some(vec!["entity_removed".to_owned()]),
            ..api::StreamFilter::default()
        });
        let out = build_frame(&d, &wanted(by_kind, 10)).expect("frame");
        assert_eq!(kinds_and_ticks(&out), [("entity_removed".to_owned(), 3)]);
        let by_id = filter(api::StreamFilter {
            entity_ids: Some(vec![2]),
            ..api::StreamFilter::default()
        });
        let out = build_frame(&d, &wanted(by_id, 10)).expect("frame");
        assert_eq!(
            kinds_and_ticks(&out),
            [
                ("custom".to_owned(), 2),
                ("entity_removed".to_owned(), 3),
                ("simulation_control".to_owned(), 4)
            ],
            "entity 1's event is filtered out; an event of no entity stays"
        );
    }

    #[test]
    fn an_event_without_a_recorded_tick_takes_the_frames_tick() {
        let d = delivery(vec![frame(vec![custom(1)], Vec::new())]);
        let out = build_frame(&d, &wanted(Filter::default(), 10)).expect("frame");
        assert_eq!(out.events.len(), 1);
        assert_eq!(
            out.events[0].tick, 0,
            "a frame without a world is at tick 0"
        );
    }

    #[test]
    fn seq_dropped_and_the_idle_frame_pass_through() {
        let mut d = delivery(vec![frame(Vec::new(), Vec::new())]);
        d.seq = 9;
        d.dropped = 4;
        let out = build_frame(&d, &wanted(Filter::default(), 10)).expect("frame");
        assert_eq!((out.seq, out.dropped), (9, 4));
        assert!(out.entities.is_empty() && out.state_hash.is_none());
        assert!(build_frame(&delivery(Vec::new()), &wanted(Filter::default(), 1)).is_none());
    }

    fn detonation(shooter: Option<u64>, target: Option<u64>) -> SimEvent {
        let place = umbra_engine::Geodetic::from_degrees(52.0, 4.0, 10.0).expect("position");
        SimEvent::Detonation {
            shooter: shooter.map(id),
            target: target.map(id),
            position: place.to_ecef(),
        }
    }

    #[test]
    fn a_detonation_follows_the_kind_and_entity_filters() {
        let events = vec![
            detonation(Some(1), Some(2)),
            detonation(None, None),
            detonation(Some(3), None),
            custom(1),
        ];
        let d = delivery(vec![frame(events, vec![1, 2, 3, 4])]);
        let filter = |wire: api::StreamFilter| Filter::from_wire(Some(&wire), 10).expect("filter");
        let kind = |k: &str| {
            filter(api::StreamFilter {
                event_kinds: Some(vec![k.to_owned()]),
                ..api::StreamFilter::default()
            })
        };
        let out = build_frame(&d, &wanted(kind("detonation"), 10)).expect("frame");
        assert_eq!(
            kinds_and_ticks(&out),
            [
                ("detonation".to_owned(), 1),
                ("detonation".to_owned(), 2),
                ("detonation".to_owned(), 3)
            ]
        );
        let out = build_frame(&d, &wanted(kind("fire"), 10)).expect("frame");
        assert!(out.events.is_empty(), "other kinds do not pass");

        // The target counts as concerned, the unknown shooter and target of tick 2 keep the event.
        let by_id = filter(api::StreamFilter {
            entity_ids: Some(vec![2]),
            ..api::StreamFilter::default()
        });
        let out = build_frame(&d, &wanted(by_id, 10)).expect("frame");
        assert_eq!(
            kinds_and_ticks(&out),
            [("detonation".to_owned(), 1), ("detonation".to_owned(), 2)],
            "shooter 3 and source 1 are filtered out"
        );
    }
}
