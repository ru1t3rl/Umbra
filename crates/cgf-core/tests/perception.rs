//! M3 WI-3: `Force`, `hostile`, `Contact`, `Sensor` and the behavior view types.
//! Requirements: REQ-B-1, REQ-B-5, ADR 0013, ADR 0015, ASM-079.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cgf_core::{
    BehaviorContext, ComponentId, Contact, Ecef, EntityId, EntityView, FlatTerrain, Force,
    ForceError, KinematicState, Meters, Observer, Ownership, Quaternion, RouteView, Sensor, SimRng,
    SimTime, StreamKey, TerrainProvider, TickRate, Vec3, hostile,
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

/// ASM-079: invalid names are rejected with a reason.
#[test]
fn given_invalid_names_when_force_new_then_rejected() {
    assert_eq!(Force::new(""), Err(ForceError::Empty));
    for bad in ["1red", "_red", "Red", "re d", "re-d", "rot\u{e9}", "a.b"] {
        assert!(Force::new(bad).is_err(), "{bad} must be rejected");
    }
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
        observer: &Observer<'_>,
        candidates: &[cgf_core::Target<'_>],
        _terrain: &dyn TerrainProvider,
        out: &mut Vec<Contact>,
    ) {
        for t in candidates {
            out.push(Contact {
                entity: t.id,
                position: t.state.position,
                velocity: t.state.velocity,
                force: t.force.cloned(),
                range: observer.state.position.distance(t.state.position),
            });
        }
    }
}

/// ADR 0015: a sensor is an object-safe trait fed by `Observer` and `Target`.
#[test]
fn given_stub_sensor_as_trait_object_when_sensing_then_contacts_appended() {
    let id = ComponentId::new("stub.sensor.all").unwrap_or_else(|e| unreachable!("{e}"));
    let sensor: Arc<dyn Sensor> = Arc::new(Everyone(id.clone()));
    let (me, other) = (state(0.0), state(10.0));
    let blue = force("blue");
    let observer = Observer::new(entity(1), &me, None);
    let remote = Ownership::Remote {
        source: cgf_core::RemoteSource::new("x"),
    };
    let candidates = [cgf_core::Target::new(
        entity(2),
        &other,
        Some(&blue),
        &remote,
    )];
    let terrain = FlatTerrain {
        height: Meters(0.0),
    };
    let mut out = Vec::new();

    sensor.sense(&observer, &candidates, &terrain, &mut out);

    assert_eq!(sensor.id(), &id);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].entity, entity(2));
    assert_eq!(out[0].force, Some(blue));
    assert_eq!(out[0].range, Meters(10.0));
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
    let contacts = [Contact {
        entity: entity(9),
        position: s.position,
        velocity: s.velocity,
        force: None,
        range: Meters(0.0),
    }];

    let bare = EntityView::new(entity(1), &s);
    assert!(bare.force.is_none() && bare.route.is_none() && bare.contacts.is_empty());

    let view = EntityView::new(entity(1), &s)
        .with_force(Some(&blue))
        .with_route(RouteView {
            waypoints: &wps,
            next: 0,
            complete: false,
        })
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
