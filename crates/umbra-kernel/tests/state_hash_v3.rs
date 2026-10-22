//! AT-19 (kernel part, M7): state hash v3 is opt-in and v2 is untouched. Requirements: REQ-K-4,
//! D-30, ADR 0055, ASM-634.
//!
//! v3 = the v2 encoding with the version word 3 and, per entity after the RNG state: force (`u8` 0
//! none / 1 + string), remote track (`u8` 0 none / 1 + ...), behavior digest (`u8` 0 none / 1 + the
//! instance's digest bytes / 2 opaque). Written in M7 WI-1 against stubs: every row that needs
//! `StateHasher::with_version` or `Kernel::state_hash_with` is ignored until M7 WI-15; the row that
//! pins the default (v2) passes now.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::sync::Arc;

use umbra_core::dead_reckoning::{DeadReckoning, RemoteState};
use umbra_core::hash::fnv1a64;
use umbra_core::{
    BbValue, Behavior, BehaviorContext, BehaviorId, BehaviorInstance, BlackboardError,
    DigestSupport, EntityId, EntityView, Force, InstantiateError, IntentBuffer, KinematicState,
    RemoteSource, SimRng, StateHash, StateHashVersion, StateHasher, Status, StreamKey, TickRate,
    Vec3,
};
use umbra_kernel::{AgentInit, Kernel, RemoteInit};

use common::{Behavior as Motion, Stub, at, kernel, owned, state_at};

/// A behavior instance whose whole state is a counter it increments each tick and reports in its
/// digest. Its intents are none, so v2 (which does not see behavior state) cannot tell two
/// instances with different starts apart.
struct Counting {
    count: u64,
    written: bool,
}

impl BehaviorInstance for Counting {
    fn tick(
        &mut self,
        _: &mut BehaviorContext<'_>,
        _: &EntityView<'_>,
        _: &mut IntentBuffer<'_>,
    ) -> Status {
        self.count += 1;
        Status::Running
    }
    fn halt(&mut self) {}
    fn output(&self, _: &str) -> Option<BbValue> {
        None
    }
    fn set_input(&mut self, _: &str, _: Option<BbValue>) -> Result<(), BlackboardError> {
        Ok(())
    }
    fn digest(&self, hasher: &mut StateHasher) -> DigestSupport {
        if self.written {
            hasher.write_u64(self.count);
            DigestSupport::Written
        } else {
            DigestSupport::Opaque
        }
    }
}

struct CountingBehavior {
    id: BehaviorId,
    start: u64,
    written: bool,
}

impl Behavior for CountingBehavior {
    fn id(&self) -> &BehaviorId {
        &self.id
    }
    fn instantiate(&self, _: TickRate) -> Result<Box<dyn BehaviorInstance>, InstantiateError> {
        Ok(Box::new(Counting {
            count: self.start,
            written: self.written,
        }))
    }
}

fn counting(start: u64, written: bool) -> AgentInit {
    AgentInit {
        behavior: Some(Arc::new(CountingBehavior {
            id: BehaviorId::new("test.behavior.counting").expect("valid id"),
            start,
            written,
        })),
        ..AgentInit::default()
    }
}

fn one_entity(seed: u64, agent: AgentInit) -> Kernel {
    let mut k = kernel(seed);
    let m = Stub::new("test.stub.still", Motion::Stationary);
    k.spawn_with(owned(at(0.0), &m), agent).expect("spawn_with");
    k
}

/// ADR 0055: `StateHasher::new()` keeps meaning v2: it starts with the version word 2.
#[test]
fn given_the_default_hasher_then_it_starts_with_the_v2_version_word() {
    assert_eq!(
        StateHasher::new().finish(),
        StateHash(fnv1a64(&2u32.to_le_bytes()))
    );
}

/// ADR 0055: v3 starts with the version word 3.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_v3_hasher_then_it_starts_with_the_v3_version_word() {
    assert_eq!(
        StateHasher::with_version(StateHashVersion::V3).finish(),
        StateHash(fnv1a64(&3u32.to_le_bytes()))
    );
}

/// ADR 0055: `with_version(V2)` is the default hasher.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_v2_hasher_then_it_equals_the_default_hasher() {
    assert_eq!(
        StateHasher::with_version(StateHashVersion::V2),
        StateHasher::new()
    );
}

/// ADR 0055: `state_hash_with(V2)` is `state_hash()` bit for bit, so no golden can move.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_kernel_then_state_hash_with_v2_equals_state_hash_at_every_tick() {
    let mut k = one_entity(5, AgentInit::default());

    for _ in 0..5 {
        k.tick().expect("tick");
        assert_eq!(k.state_hash_with(StateHashVersion::V2), k.state_hash());
    }
}

/// ADR 0055: v3 differs from v2 for the same state.
#[test]
#[ignore = "M7 WI-15"]
fn given_one_state_then_v3_and_v2_hashes_differ() {
    let k = one_entity(5, AgentInit::default());

    assert_ne!(k.state_hash_with(StateHashVersion::V3), k.state_hash());
}

/// How the behavior digest of one reference entity is encoded (ADR 0055, clarified 2026-10-22).
enum RefDigest {
    /// No behavior: tag 0.
    None,
    /// `Written`: tag 1, then the `finish()` of a fresh scratch v3 hasher that received `count`.
    Written(u64),
    /// `Opaque`: tag 2.
    Opaque,
}

/// A remote track as hashed: model index, basis state, acceleration, angular velocity, basis tick.
struct RefTrack {
    model: u32,
    state: KinematicState,
    acceleration: Vec3,
    angular_velocity: Vec3,
    tick: u64,
}

/// One entity of the hand-built v3 reference: the v2 fields (no route), then force, remote track
/// and behavior digest.
struct RefEntity<'a> {
    id: u64,
    remote_source: Option<&'a str>,
    model: &'a str,
    state: KinematicState,
    force: Option<&'a str>,
    track: Option<RefTrack>,
    digest: RefDigest,
}

/// The v3 hash of a kernel at tick 0 with these entities, built by hand from the ADR 0055 encoding.
fn reference_v3(seed: u64, entities: &[RefEntity<'_>]) -> StateHash {
    let mut h = StateHasher::with_version(StateHashVersion::V3);
    h.write_u64(0); // tick
    h.write_u32(30); // tick rate
    h.write_u64(entities.len() as u64);
    for e in entities {
        h.write_u64(e.id);
        match e.remote_source {
            None => h.write_u8(0),
            Some(source) => {
                h.write_u8(1);
                h.write_str(source);
            }
        }
        h.write_str(e.model);
        h.write_vec3(e.state.position.0);
        h.write_vec3(e.state.velocity);
        h.write_quaternion(e.state.orientation);
        h.write_u8(0); // no route
        let id = EntityId::new(e.id).expect("non-zero id");
        for word in SimRng::new(seed).stream(StreamKey::entity(id)).state() {
            h.write_u64(word);
        }
        match e.force {
            None => h.write_u8(0),
            Some(force) => {
                h.write_u8(1);
                h.write_str(force);
            }
        }
        match &e.track {
            None => h.write_u8(0),
            Some(t) => {
                h.write_u8(1);
                h.write_u32(t.model);
                h.write_vec3(t.state.position.0);
                h.write_vec3(t.state.velocity);
                h.write_quaternion(t.state.orientation);
                h.write_vec3(t.acceleration);
                h.write_vec3(t.angular_velocity);
                h.write_u64(t.tick);
            }
        }
        match e.digest {
            RefDigest::None => h.write_u8(0),
            RefDigest::Written(count) => {
                let mut scratch = StateHasher::with_version(StateHashVersion::V3);
                scratch.write_u64(count);
                h.write_u8(1);
                h.write_u64(scratch.finish().0);
            }
            RefDigest::Opaque => h.write_u8(2),
        }
    }
    h.finish()
}

fn owned_ref(digest: RefDigest, force: Option<&str>) -> RefEntity<'_> {
    RefEntity {
        id: 1,
        remote_source: None,
        model: "test.stub.still",
        state: state_at(at(0.0)),
        force,
        track: None,
        digest,
    }
}

/// ADR 0055: one Owned entity without force, remote track or behavior: the v2 fields under version
/// word 3, then the three absent tags.
#[test]
#[ignore = "M7 WI-15"]
fn given_an_entity_without_force_or_behavior_then_v3_equals_the_reference_encoding() {
    let k = one_entity(11, AgentInit::default());

    assert_eq!(
        k.state_hash_with(StateHashVersion::V3),
        reference_v3(11, &[owned_ref(RefDigest::None, None)])
    );
}

/// ADR 0055: a present force is tag 1 followed by the force string.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_force_then_v3_equals_the_reference_encoding_with_tag_1_and_the_string() {
    let agent = AgentInit {
        force: Some(Force::new("blue").expect("valid force")),
        ..AgentInit::default()
    };
    let k = one_entity(11, agent);

    assert_eq!(
        k.state_hash_with(StateHashVersion::V3),
        reference_v3(11, &[owned_ref(RefDigest::None, Some("blue"))])
    );
}

/// ADR 0055 (clarified): a `Written` digest is tag 1 plus the `finish()` of a fresh scratch hasher.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_written_digest_then_v3_equals_the_reference_encoding_with_tag_1_and_the_scratch_hash() {
    let k = one_entity(11, counting(7, true));

    assert_eq!(
        k.state_hash_with(StateHashVersion::V3),
        reference_v3(11, &[owned_ref(RefDigest::Written(7), None)])
    );
}

/// ADR 0055 (clarified): an `Opaque` instance contributes only tag 2, whatever it wrote.
#[test]
#[ignore = "M7 WI-15"]
fn given_an_opaque_instance_then_v3_equals_the_reference_encoding_with_tag_2() {
    let k = one_entity(11, counting(7, false));

    assert_eq!(
        k.state_hash_with(StateHashVersion::V3),
        reference_v3(11, &[owned_ref(RefDigest::Opaque, None)])
    );
}

/// ADR 0055: a Remote entity hashes its force and its dead-reckoning track (model index, basis
/// state, acceleration, angular velocity, basis tick) and has no behavior digest.
#[test]
#[ignore = "M7 WI-15"]
fn given_a_remote_entity_then_v3_equals_the_reference_encoding_with_the_track() {
    let mut k = kernel(11);
    let m = Stub::new("test.stub.still", Motion::Stationary);
    let basis = RemoteState {
        state: KinematicState {
            velocity: Vec3::new(1.5, -2.5, 0.25),
            ..state_at(at(40.0))
        },
        acceleration: Vec3::new(0.5, 0.25, -0.125),
        angular_velocity: Vec3::new(0.0, 0.0625, 0.03125),
        model: DeadReckoning::Velocity,
    };
    k.spawn_remote(RemoteInit {
        source: RemoteSource::new("dis.a"),
        motion: m,
        force: Some(Force::new("red").expect("valid force")),
        remote: basis,
    })
    .expect("spawn_remote");

    let want = reference_v3(
        11,
        &[RefEntity {
            id: 1,
            remote_source: Some("dis.a"),
            model: "test.stub.still",
            state: basis.state,
            force: Some("red"),
            track: Some(RefTrack {
                model: 1, // DeadReckoning::Velocity, declaration index
                state: basis.state,
                acceleration: basis.acceleration,
                angular_velocity: basis.angular_velocity,
                tick: 0,
            }),
            digest: RefDigest::None,
        }],
    );

    assert_eq!(k.state_hash_with(StateHashVersion::V3), want);
}

/// ADR 0055: v3 hashes the force; v2 does not.
#[test]
#[ignore = "M7 WI-15"]
fn given_two_forces_then_v3_differs_and_v2_does_not() {
    let with = |force: &str| {
        one_entity(
            5,
            AgentInit {
                force: Some(Force::new(force).expect("valid force")),
                ..AgentInit::default()
            },
        )
    };
    let (blue, red) = (with("blue"), with("red"));

    assert_ne!(
        blue.state_hash_with(StateHashVersion::V3),
        red.state_hash_with(StateHashVersion::V3)
    );
    assert_eq!(blue.state_hash(), red.state_hash());
}

/// ADR 0055, D-30: behavior state that v2 cannot see (a counter) is seen by v3 at the tick it
/// first differs, here tick 1.
#[test]
#[ignore = "M7 WI-15"]
fn given_instances_with_different_digest_state_then_v3_differs_at_the_first_tick_and_v2_never() {
    let mut a = one_entity(5, counting(0, true));
    let mut b = one_entity(5, counting(7, true));

    for tick in 1..=3_u64 {
        a.tick().expect("tick");
        b.tick().expect("tick");

        assert_eq!(a.state_hash(), b.state_hash(), "v2 is blind at tick {tick}");
        assert_ne!(
            a.state_hash_with(StateHashVersion::V3),
            b.state_hash_with(StateHashVersion::V3),
            "v3 sees it at tick {tick}"
        );
    }
}

/// ADR 0055: an opaque instance contributes a fixed tag, so two opaque instances with different
/// private state hash alike in v3 (plugin behaviors in M7).
#[test]
#[ignore = "M7 WI-15"]
fn given_opaque_instances_with_different_private_state_then_v3_hashes_alike() {
    let mut a = one_entity(5, counting(0, false));
    let mut b = one_entity(5, counting(7, false));

    a.tick().expect("tick");
    b.tick().expect("tick");

    assert_eq!(
        a.state_hash_with(StateHashVersion::V3),
        b.state_hash_with(StateHashVersion::V3)
    );
}

/// ADR 0055: v3 is a pure function of the state: two identical kernels agree tick by tick.
#[test]
#[ignore = "M7 WI-15"]
fn given_identical_kernels_then_v3_hashes_agree_on_every_tick() {
    let mut a = one_entity(9, counting(0, true));
    let mut b = one_entity(9, counting(0, true));

    for _ in 0..20 {
        a.tick().expect("tick");
        b.tick().expect("tick");
        assert_eq!(
            a.state_hash_with(StateHashVersion::V3),
            b.state_hash_with(StateHashVersion::V3)
        );
    }
}
