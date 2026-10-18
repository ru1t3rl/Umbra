//! M3 WI-3: `Force`, `hostile`, `Contact`, `Sensor` and the behavior view types.
//! Requirements: REQ-B-1, REQ-B-5, ADR 0013, ADR 0015, ASM-079.

#![forbid(unsafe_code)]

use std::sync::Arc;

use umbra_core::{
    BehaviorContext, ComponentId, Contact, ContactBuffer, Ecef, EntityId, EntityView, FlatTerrain,
    Force, ForceError, KinematicState, Meters, Observer, Ownership, Quaternion, RouteView,
    SenseContext, Sensor, SimRng, SimTime, StreamKey, Target, TickRate, Vec3, hostile,
};

fn force(s: &str) -> Force {
    Force::new(s).unwrap_or_else(|e| unreachable!("valid force {s}: {e}"))
}

fn entity(n: u64) -> EntityId {
    EntityId::new(n).unwrap_or_else(|| unreachable!("non-zero literal"))
}

fn state(x: f64) -> KinematicState {
    KinematicState {
        position: Ecef(Vec3 { x, y: 0.0, z: 0.0 }),
        velocity: Vec3::default(),
        orientation: Quaternion::IDENTITY,
    }
}

/// ASM-079: grammar `[a-z][a-z0-9_]*`.
#[test]
fn given_valid_names_when_force_new_then_accepted() {
    for ok in ["blue", "red", "a", "force_1", "x9_"] {
        assert_eq!(force(ok).as_str(), ok);
    }
}

/// ASM-079: invalid names are rejected with exact reasons and offsets.
#[test]
fn given_invalid_names_when_force_new_then_exact_error() {
    assert_eq!(Force::new(""), Err(ForceError::Empty));
    for bad in ["1red", "_red", "Red"] {
        assert_eq!(Force::new(bad), Err(ForceError::InvalidStart { index: 0 }));
    }
    assert_eq!(
        Force::new("re d"),
        Err(ForceError::InvalidChar { index: 2 })
    );
    assert_eq!(
        Force::new("rot\u{e9}"),
        Err(ForceError::InvalidChar { index: 3 })
    );
    assert_eq!(Force::new("a.b"), Err(ForceError::InvalidChar { index: 1 }));
}

/// ASM-079, ASM-025: over-long names are rejected with a bounded echo; 128 bytes is accepted.
#[test]
fn given_over_long_name_when_force_new_then_too_long_with_bounded_prefix() {
    assert!(Force::new(&"a".repeat(128)).is_ok());

    let Err(ForceError::TooLong { prefix, len }) = Force::new(&"a".repeat(129)) else {
        unreachable!("129 bytes must be too long");
    };
    assert_eq!((prefix.len(), len), (64, 129));

    // A multi-byte character straddling byte 64 is not split.
    let long = format!("{}{}", "a".repeat(63), "\u{e9}".repeat(40));
    let Err(ForceError::TooLong { prefix, .. }) = Force::new(&long) else {
        unreachable!("must be too long");
    };
    assert_eq!(prefix.len(), 63);
}

/// ASM-079: hostile iff both set and different.
#[test]
fn hostile_truth_table() {
    let (blue, red) = (force("blue"), force("red"));

    assert!(!hostile(None, None));
    assert!(!hostile(Some(&blue), None));
    assert!(!hostile(None, Some(&red)));
    assert!(!hostile(Some(&blue), Some(&blue)));
    assert!(hostile(Some(&blue), Some(&red)));
    assert!(hostile(Some(&red), Some(&blue)));
}

struct Everyone(ComponentId);

impl Sensor for Everyone {
    fn id(&self) -> &ComponentId {
        &self.0
    }
    fn sense(
        &self,
        ctx: &mut SenseContext<'_>,
        observer: &Observer<'_>,
        candidates: &[Target<'_>],
        out: &mut ContactBuffer<'_>,
    ) {
        // Prove the context is usable: time, rng and terrain.
        let _ = (ctx.now.tick(), ctx.rng.next_u64());
        for t in candidates {
            let visible = ctx
                .terrain
                .line_of_sight(observer.state.position, t.state.position);
            if visible {
                out.push(Contact::new(
                    t.id,
                    t.state.position,
                    t.state.velocity,
                    t.force.cloned(),
                    observer.state.position.distance(t.state.position),
                ));
            }
        }
    }
}

/// ADR 0015: a sensor is an object-safe trait fed by `SenseContext`, `Observer` and `Target`.
#[test]
fn given_stub_sensor_as_trait_object_when_sensing_then_contacts_appended() {
    let id = ComponentId::new("stub.sensor.all").unwrap_or_else(|e| unreachable!("{e}"));
    let sensor: Arc<dyn Sensor> = Arc::new(Everyone(id.clone()));
    let (me, other) = (state(0.0), state(10.0));
    let blue = force("blue");
    let observer = Observer::new(entity(1), &me, None);
    let remote = Ownership::Remote {
        source: umbra_core::RemoteSource::new("x"),
    };
    let candidates = [umbra_core::Target::new(
        entity(2),
        &other,
        Some(&blue),
        &remote,
    )];
    let terrain = FlatTerrain {
        height: Meters(-1.0e7),
    };
    let rate = TickRate::new(30).unwrap_or_else(|| unreachable!("30 Hz is valid"));
    let mut rng = SimRng::new(1).stream(StreamKey::entity(entity(1)));
    let mut untouched = rng.clone();
    let mut ctx = SenseContext::new(SimTime::at_tick(rate, 3), &mut rng, &terrain);
    let mut contacts = vec![Contact::new(
        entity(9),
        me.position,
        me.velocity,
        None,
        Meters(0.0),
    )];
    let mut out = ContactBuffer::new(&mut contacts);

    sensor.sense(&mut ctx, &observer, &candidates, &mut out);

    assert_eq!(sensor.id(), &id);
    assert_eq!(out.len(), 2);
    assert_eq!(contacts[0].entity, entity(9), "earlier contact kept");
    assert_eq!(contacts[1].entity, entity(2));
    assert_eq!(contacts[1].force, Some(blue));
    assert_eq!(contacts[1].range, Meters(10.0));
    assert_ne!(rng, untouched, "the sensor drew from the context stream");
    let _ = untouched.next_u64();
    assert_eq!(rng, untouched);
}

/// ADR 0015: `ContactBuffer` only appends and prints only its length.
#[test]
fn given_contact_buffer_when_pushing_then_appends_and_debug_shows_len() {
    let s = state(0.0);
    let c = |n| Contact::new(entity(n), s.position, s.velocity, None, Meters(0.0));
    let mut v = vec![c(1)];
    let mut buf = ContactBuffer::new(&mut v);
    assert!(!buf.is_empty());

    buf.push(c(2));

    assert_eq!(format!("{buf:?}"), "ContactBuffer { len: 2 }");
    assert_eq!(v, vec![c(1), c(2)]);
    assert!(ContactBuffer::new(&mut Vec::new()).is_empty());
}

/// ADR 0013: views and context are built with constructors (`non_exhaustive`).
#[test]
fn given_entity_view_builders_when_chained_then_fields_set() {
    let s = state(0.0);
    let blue = force("blue");
    let wps = [Ecef(Vec3 {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    })];
    let contacts = [Contact::new(
        entity(9),
        s.position,
        s.velocity,
        None,
        Meters(0.0),
    )];

    let bare = EntityView::new(entity(1), &s);
    assert!(bare.force.is_none() && bare.route.is_none() && bare.contacts.is_empty());

    let view = EntityView::new(entity(1), &s)
        .with_force(Some(&blue))
        .with_route(RouteView::new(&wps, 0, false))
        .with_contacts(&contacts);
    assert_eq!(view.id, entity(1));
    assert_eq!(view.force, Some(&blue));
    assert_eq!(view.route.map(|r| r.waypoints.len()), Some(1));
    assert_eq!(view.contacts.len(), 1);
}

/// ADR 0013: the context carries time and the entity's RNG stream.
#[test]
fn given_context_when_drawing_then_uses_the_given_stream() {
    let rate = TickRate::new(30).unwrap_or_else(|| unreachable!("30 Hz is valid"));
    let now = SimTime::at_tick(rate, 5);
    let mut a = SimRng::new(1).stream(StreamKey::entity(entity(1)));
    let mut b = a.clone();
    let expected = b.next_u64();

    let ctx = BehaviorContext::new(now, &mut a);

    assert_eq!(ctx.now, now);
    assert_eq!(ctx.rng.next_u64(), expected);
}
