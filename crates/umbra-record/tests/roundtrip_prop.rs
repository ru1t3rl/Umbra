//! M7 WI-11: property tests of the format. Any record round-trips; arbitrary or damaged bytes are
//! an error, never a panic (REQ-K-7, NFR-2, ASM-621, ADR 0053).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use common::{MODELS, every_record, header, read_bytes, write_bytes};
use proptest::prelude::*;
use umbra_core::net::{KeySpace, NetEntityState, NetEntityType, NetKey, NetMessage};
use umbra_core::{
    ControlAction, Ecef, EntityId, Force, Geodetic, MetersPerSecond, Position, Radians, StateHash,
    TemplateId, Vec3, WeaponId, WorldEuler,
};
use umbra_record::{
    AdapterRef, CommandRecord, EndRecord, FaultRecord, FileDigest, Header, InboundRecord,
    PluginRef, Reader, Record, RecordedCommand, ScenarioRef, SpawnRecord,
};

/// Any finite `f64`, by bit pattern (subnormals, huge, tiny, both zeros).
fn finite() -> impl Strategy<Value = f64> {
    any::<u64>()
        .prop_map(f64::from_bits)
        .prop_filter("finite", |x| x.is_finite())
}

fn vec3() -> impl Strategy<Value = Vec3> {
    (finite(), finite(), finite()).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn force() -> impl Strategy<Value = Option<Force>> {
    proptest::option::of("[a-z][a-z0-9_]{0,10}".prop_map(|s| Force::new(&s).expect("force")))
}

fn opt_string() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(any::<String>())
}

fn entity_state() -> impl Strategy<Value = NetEntityState> {
    (
        (
            any::<u64>(),
            proptest::option::of(any::<String>()),
            force(),
            opt_string(),
        ),
        (vec3(), vec3(), vec3(), vec3()),
        (finite(), finite(), finite(), 0..MODELS.len()),
    )
        .prop_map(
            |((key, ty, force, name), (p, v, a, w), (yaw, pitch, roll, m))| NetEntityState {
                key: NetKey(key),
                entity_type: ty.map(NetEntityType::new),
                force,
                name: name.map(|n| Arc::from(n.as_str())),
                position: Ecef(p),
                velocity: v,
                acceleration: a,
                angular_velocity: w,
                orientation: WorldEuler {
                    yaw: Radians(yaw),
                    pitch: Radians(pitch),
                    roll: Radians(roll),
                },
                dead_reckoning: MODELS[m],
            },
        )
}

fn key() -> impl Strategy<Value = NetKey> {
    any::<u64>().prop_map(NetKey)
}

fn message() -> impl Strategy<Value = NetMessage> {
    let weapon = "[a-z][a-z0-9_]{0,8}\\.[a-z][a-z0-9_]{0,8}";
    prop_oneof![
        entity_state().prop_map(NetMessage::EntityState),
        key().prop_map(|key| NetMessage::EntityRemoved { key }),
        (key(), proptest::option::of(key()), weapon, vec3()).prop_map(|(s, t, w, p)| {
            NetMessage::Fire {
                shooter: s,
                target: t,
                weapon: WeaponId::new(&w).expect("weapon"),
                position: Ecef(p),
            }
        }),
        (
            proptest::option::of(key()),
            proptest::option::of(key()),
            vec3()
        )
            .prop_map(|(shooter, target, p)| NetMessage::Detonation {
                shooter,
                target,
                position: Ecef(p),
            }),
        prop_oneof![
            Just(ControlAction::Start),
            Just(ControlAction::Pause),
            Just(ControlAction::Resume),
            Just(ControlAction::Stop),
            any::<u64>().prop_map(ControlAction::Step),
        ]
        .prop_map(NetMessage::SimulationControl),
    ]
}

fn position() -> impl Strategy<Value = Position> {
    prop_oneof![
        vec3().prop_map(|v| Position::Ecef(Ecef(v))),
        (-89.9..89.9_f64, -179.9..179.9_f64, -1.0e4..1.0e5_f64).prop_map(|(lat, lon, h)| {
            Position::Geodetic(Geodetic::from_degrees(lat, lon, h).expect("geodetic"))
        }),
    ]
}

fn command() -> impl Strategy<Value = RecordedCommand> {
    let spawn = (
        "[a-z][a-z0-9_]{0,8}\\.[a-z][a-z0-9_]{0,8}",
        position(),
        (finite(), finite()),
        (opt_string(), opt_string(), force()),
        1..=u64::MAX,
    )
        .prop_map(
            |(t, position, (h, s), (name, group, force), id)| RecordedCommand::Spawn {
                spec: SpawnRecord {
                    template: TemplateId::new(&t).expect("template"),
                    position,
                    heading: Radians(h),
                    speed: MetersPerSecond(s),
                    name,
                    group,
                    force,
                },
                result: EntityId::new(id).expect("id"),
            },
        );
    prop_oneof![
        spawn,
        (1..=u64::MAX).prop_map(|id| RecordedCommand::Remove {
            entity: EntityId::new(id).expect("id")
        }),
        (any::<String>(), any::<bool>())
            .prop_map(|(name, enabled)| RecordedCommand::SetAdapterEnabled { name, enabled }),
        Just(RecordedCommand::Start),
        Just(RecordedCommand::Pause),
        Just(RecordedCommand::Resume),
        any::<u64>().prop_map(RecordedCommand::Step),
        any::<String>().prop_map(RecordedCommand::SetClock),
        Just(RecordedCommand::Stop),
    ]
}

fn record() -> impl Strategy<Value = Record> {
    prop_oneof![
        (any::<u64>(), any::<u64>(), command()).prop_map(|(before_tick, seq, command)| {
            Record::Command(CommandRecord {
                before_tick,
                seq,
                command,
            })
        }),
        (
            any::<u64>(),
            any::<String>(),
            proptest::collection::vec(message(), 0..4)
        )
            .prop_map(|(tick, adapter, messages)| Record::Inbound(InboundRecord {
                tick,
                adapter,
                messages,
            })),
        (any::<u64>(), any::<String>(), any::<String>()).prop_map(|(tick, adapter, call)| {
            Record::Fault(FaultRecord {
                tick,
                adapter,
                call,
            })
        }),
        (any::<u64>(), any::<u64>()).prop_map(|(tick, h)| Record::Hash {
            tick,
            hash: StateHash(h)
        }),
        (any::<u64>(), any::<String>(), any::<u64>()).prop_map(|(tick, reason, h)| {
            Record::End(EndRecord {
                tick,
                reason,
                hash: StateHash(h),
            })
        }),
    ]
}

proptest! {
    /// Write then read gives the same records, and writing those again gives the same bytes
    /// (which also pins every float to its bits).
    #[test]
    fn any_records_round_trip_bit_exactly(
        records in proptest::collection::vec(record(), 0..6),
        seed in any::<u64>(),
    ) {
        let mut h = header();
        h.seed = seed;

        let bytes = write_bytes(&h, &records);
        let (got_header, got) = read_bytes(bytes.clone()).expect("read");

        prop_assert_eq!(&got_header, &h);
        prop_assert_eq!(&got, &records);
        prop_assert_eq!(write_bytes(&got_header, &got), bytes);
    }

    /// NFR-2: arbitrary bytes are rejected or read, never a panic.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        drain(bytes);
    }

    /// Arbitrary text that looks like lines never panics either.
    #[test]
    fn arbitrary_lines_after_a_valid_header_never_panic(lines in proptest::collection::vec(any::<String>(), 0..8)) {
        let mut bytes = write_bytes(&header(), &[]);
        for l in lines {
            bytes.extend_from_slice(l.as_bytes());
            bytes.push(b'\n');
        }
        drain(bytes);
    }

    /// A valid file with a byte changed or the tail cut off is an error or a read, never a panic.
    #[test]
    fn damaged_recordings_never_panic(at in any::<prop::sample::Index>(), byte in any::<u8>(), cut in any::<prop::sample::Index>()) {
        let mut bytes = write_bytes(&header(), &every_record());
        let i = at.index(bytes.len());
        bytes[i] = byte;
        drain(bytes.clone());
        bytes.truncate(cut.index(bytes.len() + 1));
        drain(bytes);
    }
}

/// Reads everything; the outcome does not matter, only that it returns.
fn drain(bytes: Vec<u8>) {
    if let Ok((_, reader)) = Reader::from_reader(Cursor::new(bytes)) {
        for item in reader {
            let _ = item;
        }
    }
}

fn header_strategy() -> impl Strategy<Value = Header> {
    let digest = (any::<String>(), any::<u64>()).prop_map(|(p, d)| FileDigest {
        path: PathBuf::from(p),
        fnv1a64: d,
    });
    let plugin = (
        any::<String>(),
        any::<String>(),
        any::<(u32, u32)>(),
        any::<u64>(),
    )
        .prop_map(|(id, version, abi, library_fnv1a64)| PluginRef {
            id,
            version,
            abi,
            library_fnv1a64,
        });
    let adapter = (any::<String>(), any::<String>(), any::<u64>(), any::<u64>()).prop_map(
        |(name, adapter, base, count)| AdapterRef {
            name,
            adapter,
            key_space: KeySpace { base, count },
        },
    );
    (
        (
            any::<String>(),
            any::<String>(),
            proptest::option::of(any::<String>()),
        ),
        proptest::collection::vec(digest, 0..3),
        (
            any::<u64>(),
            any::<u32>(),
            any::<String>(),
            any::<u32>(),
            any::<u64>(),
        ),
        (any::<String>(), proptest::collection::vec(plugin, 0..3)),
        proptest::collection::vec(adapter, 0..3),
    )
        .prop_map(
            |(
                (umbra, path, rel),
                files,
                (seed, hz, source, svh, every),
                (fault, plugins),
                adapters,
            )| {
                Header {
                    umbra,
                    scenario: ScenarioRef {
                        path: PathBuf::from(path),
                        relative_path: rel.map(PathBuf::from),
                        files,
                    },
                    seed,
                    tick_rate_hz: hz,
                    tick_rate_source: source,
                    state_hash_version: svh,
                    hash_every: every,
                    on_plugin_fault: fault,
                    plugins,
                    network_adapters: adapters,
                }
            },
        )
}

proptest! {
    /// Any header round-trips, including `relative_path: None`, empty lists and arbitrary strings.
    #[test]
    fn any_header_round_trips(h in header_strategy()) {
        let bytes = write_bytes(&h, &[]);

        let (got, records) = read_bytes(bytes.clone()).expect("read");

        prop_assert_eq!(&got, &h);
        prop_assert!(records.is_empty());
        prop_assert_eq!(write_bytes(&got, &[]), bytes);
    }
}
