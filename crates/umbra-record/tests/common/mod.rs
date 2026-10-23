//! Shared fixtures of the `umbra-record` tests: one sample of every record kind and an in-memory
//! sink and source.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{KeySpace, NetEntityState, NetEntityType, NetKey, NetMessage};
use umbra_core::{
    ControlAction, Ecef, EntityId, Force, Geodetic, MetersPerSecond, Position, Radians, StateHash,
    TemplateId, Vec3, WeaponId, WorldEuler,
};
use umbra_record::{
    AdapterRef, CommandRecord, EndRecord, FaultRecord, FileDigest, Header, InboundRecord,
    PluginRef, Reader, Record, RecordError, RecordedCommand, ScenarioRef, SpawnRecord, Writer,
};

/// A header with every optional part filled and `u64::MAX` wherever a `u64` goes.
pub fn header() -> Header {
    Header {
        umbra: "0.1.0".to_owned(),
        scenario: ScenarioRef {
            path: PathBuf::from("/s/scenario.toml"),
            relative_path: Some(PathBuf::from("scenario.toml")),
            files: vec![
                FileDigest {
                    path: PathBuf::from("/s/scenario.toml"),
                    fnv1a64: u64::MAX,
                },
                FileDigest {
                    path: PathBuf::from("/s/t.toml"),
                    fnv1a64: 0,
                },
            ],
        },
        seed: u64::MAX,
        tick_rate_hz: 30,
        tick_rate_source: "scenario".to_owned(),
        state_hash_version: 2,
        hash_every: 1,
        on_plugin_fault: "disable_capability".to_owned(),
        plugins: vec![PluginRef {
            id: "acme.tools".to_owned(),
            version: "1.2.3".to_owned(),
            abi: (0, 2),
            library_fnv1a64: 0xdead_beef_0123_4567,
        }],
        network_adapters: vec![AdapterRef {
            name: "dis".to_owned(),
            adapter: "umbra.net.dis".to_owned(),
            key_space: KeySpace {
                base: u64::MAX - 10,
                count: 10,
            },
        }],
    }
}

pub fn vec3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

pub fn entity_state(model: DeadReckoning) -> NetEntityState {
    NetEntityState {
        key: NetKey(u64::MAX),
        entity_type: Some(NetEntityType::new("1.2.225.1.1.1.0")),
        force: Some(Force::new("red").expect("force")),
        name: Some(Arc::from("a \"quoted\"\n name \u{1F680}")),
        position: Ecef(vec3(f64::MIN_POSITIVE / 2.0, -0.0, 6_378_137.123_456_789)),
        velocity: vec3(5e-324, f64::MAX, -f64::MIN_POSITIVE),
        acceleration: vec3(0.1, 0.2, 0.3),
        angular_velocity: vec3(1e-300, 1e300, -1e-310),
        orientation: WorldEuler {
            yaw: Radians(std::f64::consts::PI),
            pitch: Radians(-std::f64::consts::FRAC_PI_2),
            roll: Radians(0.0),
        },
        dead_reckoning: model,
    }
}

/// Every dead reckoning model.
pub const MODELS: [DeadReckoning; 9] = [
    DeadReckoning::Static,
    DeadReckoning::Velocity,
    DeadReckoning::VelocityRotation,
    DeadReckoning::AccelerationRotation,
    DeadReckoning::Acceleration,
    DeadReckoning::BodyVelocity,
    DeadReckoning::BodyVelocityRotation,
    DeadReckoning::BodyAccelerationRotation,
    DeadReckoning::BodyAcceleration,
];

fn command(before_tick: u64, seq: u64, command: RecordedCommand) -> Record {
    Record::Command(CommandRecord {
        before_tick,
        seq,
        command,
    })
}

fn spawn(position: Position) -> RecordedCommand {
    RecordedCommand::Spawn {
        spec: SpawnRecord {
            template: TemplateId::new("tank.mbt_generic").expect("template"),
            position,
            heading: Radians(1.5),
            speed: MetersPerSecond(12.25),
            name: Some("alpha".to_owned()),
            group: None,
            force: Some(Force::new("blue").expect("force")),
        },
        result: EntityId::new(u64::MAX).expect("id"),
    }
}

/// One record of every kind, command kind and message kind.
pub fn every_record() -> Vec<Record> {
    let geodetic = Geodetic::from_degrees(52.0, 5.0, 12.5).expect("geodetic");
    let mut records = vec![
        command(0, 0, spawn(Position::Ecef(Ecef(vec3(1.0, 2.0, 3.0))))),
        command(0, 1, spawn(Position::Geodetic(geodetic))),
        command(
            1,
            0,
            RecordedCommand::Remove {
                entity: EntityId::new(7).expect("id"),
            },
        ),
        command(
            1,
            1,
            RecordedCommand::SetAdapterEnabled {
                name: "dis".to_owned(),
                enabled: false,
            },
        ),
        command(2, 0, RecordedCommand::Start),
        command(2, 1, RecordedCommand::Pause),
        command(2, 2, RecordedCommand::Resume),
        command(2, 3, RecordedCommand::Step(u64::MAX)),
        command(
            2,
            4,
            RecordedCommand::SetClock("as_fast_as_possible".to_owned()),
        ),
        command(u64::MAX, u64::MAX, RecordedCommand::Stop),
    ];
    let mut messages: Vec<NetMessage> = MODELS
        .into_iter()
        .map(|m| NetMessage::EntityState(entity_state(m)))
        .collect();
    messages.extend([
        NetMessage::EntityRemoved {
            key: NetKey(u64::MAX),
        },
        NetMessage::Fire {
            shooter: NetKey(1),
            target: Some(NetKey(u64::MAX)),
            weapon: WeaponId::new("acme.cannon_120").expect("weapon"),
            position: Ecef(vec3(1e7, -1e7, 5e-324)),
        },
        NetMessage::Detonation {
            shooter: None,
            target: None,
            position: Ecef(vec3(0.0, 0.0, 0.0)),
        },
        NetMessage::SimulationControl(ControlAction::Start),
        NetMessage::SimulationControl(ControlAction::Pause),
        NetMessage::SimulationControl(ControlAction::Resume),
        NetMessage::SimulationControl(ControlAction::Stop),
        NetMessage::SimulationControl(ControlAction::Step(u64::MAX)),
    ]);
    records.push(Record::Inbound(InboundRecord {
        tick: 3,
        adapter: "dis".to_owned(),
        messages,
    }));
    records.push(Record::Inbound(InboundRecord {
        tick: 4,
        adapter: "dis".to_owned(),
        messages: Vec::new(),
    }));
    records.push(Record::Fault(FaultRecord {
        tick: 5,
        adapter: "dis".to_owned(),
        call: "poll".to_owned(),
    }));
    records.push(Record::Hash {
        tick: u64::MAX,
        hash: StateHash(u64::MAX),
    });
    records.push(Record::Hash {
        tick: 0,
        hash: StateHash(1),
    });
    records.push(Record::End(EndRecord {
        tick: 5,
        reason: "completed".to_owned(),
        hash: StateHash(0xabc),
    }));
    records
}

/// A `Write` sink that appends to a shared buffer.
#[derive(Clone, Default)]
pub struct SharedBuf(pub Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    pub fn bytes(&self) -> Vec<u8> {
        self.0.lock().expect("buffer").clone()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("buffer").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The bytes of a recording of `records`, through a real writer thread.
pub fn write_bytes(header: &Header, records: &[Record]) -> Vec<u8> {
    let sink = SharedBuf::default();
    let writer = Writer::from_sink(sink.clone(), header, 8).expect("writer");
    for r in records {
        writer.push(r.clone()).expect("push");
    }
    writer.finish().expect("finish");
    sink.bytes()
}

/// Reads a whole recording from memory.
pub fn read_bytes(bytes: Vec<u8>) -> Result<(Header, Vec<Record>), RecordError> {
    let (header, reader) = Reader::from_reader(Cursor::new(bytes))?;
    Ok((header, reader.collect::<Result<Vec<_>, _>>()?))
}
