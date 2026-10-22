//! The network adapter capabilities of the fixture (cargo features `network` and `network_peer`, M6
//! WI-12, ASM-407, ASM-410, ASM-414), built on `umbra-sdk` only, like everything in this crate.
//!
//! Every adapter is a **mirror**: `publish` keeps the entity states and removals it is handed, in
//! order, and the next `poll` pushes them back as remote entities with their key moved by
//! `remote_offset` (so they lie outside the adapter's own key space and are not echoes; a message
//! whose moved key would overflow `u64` is skipped), as many as
//! the inbox takes; the rest wait for the next poll. The ghosts it produces depend only on the run, so
//! a scenario using it is deterministic. Params (all optional): `key_base` (integer, the key space
//! is `(key_base, key_base + 100]`, default 10000) and `remote_offset` (integer, default 100000);
//! `net_panics_in_poll` adds `panic_at_tick` (integer, default 0). Every capability also takes
//! `detonate_at_tick` (integer, default none: no detonation, M6 WI-17): at that tick `poll` pushes two
//! `Detonation` messages at `detonate_position` (ECEF metres, an array of three numbers, default
//! `[6378237, 0, 0]`: latitude 0, longitude 0, 100 m above the ellipsoid), one whose shooter is the
//! mirrored key of the first entity the adapter was handed (so it maps to that entity's ghost) and one
//! without a shooter.
//!
//! | Capability (feature `network`) | Behavior |
//! |---|---|
//! | `test.network.net_ok` | the mirror |
//! | `test.network.net_panics_in_poll` | the mirror; `poll` panics from tick `panic_at_tick` on |
//! | `test.network.net_bad_message` | `poll` pushes an entity state whose force is not a force ID: the SDK passes it (it does not know the grammar), the host refuses it and faults the capability |
//! | `test.network.net_blocks_join` | the mirror; `start` starts an I/O thread that blocks until `stop` (or the drop) releases and joins it |
//!
//! With the feature `network_peer` the crate is plugin `test.plugin.net_peer` instead, with only
//! `test.network.peer_ok` and `test.network.peer_panics_in_poll` (a second, independent plugin image
//! for the several-plugins rows, ASM-410).
//!
//! Every adapter logs `net configure <name>`, `net start <name>`, `net stop <name>` and
//! `net destroy <name>` (the scenario's adapter name) through the host API, from the calls only:
//! the I/O thread of `net_blocks_join` never calls the host (ADR 0033).

use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use umbra_sdk::{
    ConfigureInput, DeadReckoning, EntityState, Error, Euler, Inbox, KeySpace, Level, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, NetworkAdapterFactory, SimTime, log,
};

/// What a network capability does (see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NetMode {
    Mirror,
    PanicsInPoll,
    BadMessage,
    BlocksJoin,
}

/// A received entity state, owned until the next `poll` pushes it back.
#[derive(Clone, Debug)]
struct Kept {
    key: NetKey,
    entity_type: Option<String>,
    force: Option<String>,
    name: Option<String>,
    position: [f64; 3],
    velocity: [f64; 3],
    acceleration: [f64; 3],
    angular_velocity: [f64; 3],
    orientation: Euler,
    dead_reckoning: DeadReckoning,
}

/// One message waiting for the next `poll`.
#[derive(Clone, Debug)]
enum Pending {
    State(Kept),
    Removed(NetKey),
}

/// A configured network adapter of any network capability.
pub(crate) struct NetFixture {
    mode: NetMode,
    name: String,
    keys: KeySpace,
    remote_offset: u64,
    panic_at_tick: u64,
    detonate_at_tick: Option<u64>,
    detonate_position: [f64; 3],
}

impl NetFixture {
    pub(crate) fn configure(mode: NetMode, input: &ConfigureInput) -> Result<Self, Error> {
        let params = input.params();
        let integer = |key: &str, default: u64| -> Result<u64, Error> {
            match params.get(key) {
                None => Ok(default),
                Some(v) => v.as_u64().ok_or_else(|| {
                    Error::params(format!("`{key}` must be a non-negative integer"))
                }),
            }
        };
        let key_base = integer("key_base", 10_000)?;
        let remote_offset = integer("remote_offset", 100_000)?;
        if remote_offset <= 100 {
            return Err(Error::params(format!(
                "`remote_offset` {remote_offset} must exceed the key space size 100"
            )));
        }
        let detonate_position = match params.get("detonate_position") {
            None => [6_378_237.0, 0.0, 0.0],
            Some(v) => {
                let numbers: Option<Vec<f64>> = v
                    .as_array()
                    .map(|a| a.iter().map(|n| n.as_f64()).collect::<Option<_>>())
                    .unwrap_or(None);
                match numbers.as_deref() {
                    Some(&[x, y, z]) => [x, y, z],
                    _ => {
                        return Err(Error::params(
                            "`detonate_position` must be three numbers".to_owned(),
                        ));
                    }
                }
            }
        };
        let detonate_at_tick = match params.get("detonate_at_tick") {
            None => None,
            Some(v) => Some(v.as_u64().ok_or_else(|| {
                Error::params("`detonate_at_tick` must be a non-negative integer".to_owned())
            })?),
        };
        let fixture = Self {
            mode,
            name: input.name().to_owned(),
            keys: KeySpace::new(key_base, 100)?,
            remote_offset,
            panic_at_tick: integer("panic_at_tick", 0)?,
            detonate_at_tick,
            detonate_position,
        };
        log(Level::Info, &format!("net configure {}", fixture.name));
        Ok(fixture)
    }
}

impl Drop for NetFixture {
    fn drop(&mut self) {
        log(Level::Info, &format!("net destroy {}", self.name));
    }
}

impl NetworkAdapterConfig for NetFixture {
    type Adapter = NetAdapter;

    fn key_space(&self) -> KeySpace {
        self.keys
    }

    fn start(&mut self, _now: SimTime) -> Result<NetAdapter, Error> {
        log(Level::Info, &format!("net start {}", self.name));
        let io = (self.mode == NetMode::BlocksJoin).then(|| {
            let (release, wait) = channel::<()>();
            // The thread never calls the host API; it only waits until `stop` releases it.
            let thread = std::thread::spawn(move || {
                let _ = wait.recv();
            });
            (release, thread)
        });
        Ok(NetAdapter {
            mode: self.mode,
            name: self.name.clone(),
            remote_offset: self.remote_offset,
            panic_at_tick: self.panic_at_tick,
            detonate_at_tick: self.detonate_at_tick,
            detonate_position: self.detonate_position,
            first_key: None,
            pending: Vec::new(),
            io,
        })
    }
}

/// A started network adapter.
pub(crate) struct NetAdapter {
    mode: NetMode,
    name: String,
    remote_offset: u64,
    panic_at_tick: u64,
    detonate_at_tick: Option<u64>,
    detonate_position: [f64; 3],
    /// The key of the first entity state handed to `publish`.
    first_key: Option<NetKey>,
    pending: Vec<Pending>,
    /// `net_blocks_join`: the release channel and the I/O thread.
    io: Option<(Sender<()>, JoinHandle<()>)>,
}

impl NetAdapter {
    /// Releases and joins the I/O thread, if any.
    fn join(&mut self) -> bool {
        let Some((release, thread)) = self.io.take() else {
            return false;
        };
        drop(release);
        thread.join().is_ok()
    }

    /// `key` moved by `remote_offset`; `None` when that overflows `u64` (the message is skipped:
    /// a clamped key could collide with another entity's).
    fn remote(&self, key: NetKey) -> Option<NetKey> {
        key.0.checked_add(self.remote_offset).map(NetKey)
    }
}

impl NetworkAdapter for NetAdapter {
    fn publish(&mut self, _now: SimTime, out: &[NetMessage<'_>]) -> Result<(), Error> {
        for m in out {
            match m {
                NetMessage::EntityState(s) => {
                    self.first_key.get_or_insert(s.key);
                    self.pending.push(Pending::State(Kept {
                        key: s.key,
                        entity_type: s.entity_type.map(str::to_owned),
                        force: s.force.map(str::to_owned),
                        name: s.name.map(str::to_owned),
                        position: s.position,
                        velocity: s.velocity,
                        acceleration: s.acceleration,
                        angular_velocity: s.angular_velocity,
                        orientation: s.orientation,
                        dead_reckoning: s.dead_reckoning,
                    }));
                }
                NetMessage::EntityRemoved { key } => self.pending.push(Pending::Removed(*key)),
                _ => {}
            }
        }
        Ok(())
    }

    fn poll(&mut self, now: SimTime, inbox: &mut Inbox<'_>) -> Result<(), Error> {
        match self.mode {
            NetMode::PanicsInPoll => assert!(
                now.tick() < self.panic_at_tick,
                "net_panics_in_poll: poll panics on purpose at tick {}",
                now.tick()
            ),
            NetMode::BadMessage => {
                let Some(key) = self.remote(NetKey(1)) else {
                    return Ok(());
                };
                let bad = EntityState {
                    key,
                    entity_type: Some("tank"),
                    force: Some("Not A Force"),
                    name: None,
                    position: [3_900_000.0, 300_000.0, 5_000_000.0],
                    velocity: [0.0; 3],
                    acceleration: [0.0; 3],
                    angular_velocity: [0.0; 3],
                    orientation: Euler::default(),
                    dead_reckoning: DeadReckoning::Static,
                };
                return inbox.push(&NetMessage::EntityState(bad));
            }
            NetMode::Mirror | NetMode::BlocksJoin => {}
        }
        let take = inbox.remaining().min(self.pending.len());
        let batch: Vec<Pending> = self.pending.drain(..take).collect();
        for p in &batch {
            let key = match p {
                Pending::State(k) => k.key,
                Pending::Removed(key) => *key,
            };
            let Some(remote) = self.remote(key) else {
                continue;
            };
            match p {
                Pending::State(k) => inbox.push(&NetMessage::EntityState(EntityState {
                    key: remote,
                    entity_type: k.entity_type.as_deref(),
                    force: k.force.as_deref(),
                    name: k.name.as_deref(),
                    position: k.position,
                    velocity: k.velocity,
                    acceleration: k.acceleration,
                    angular_velocity: k.angular_velocity,
                    orientation: k.orientation,
                    dead_reckoning: k.dead_reckoning,
                }))?,
                Pending::Removed(_) => inbox.push(&NetMessage::EntityRemoved { key: remote })?,
            }
        }
        if self.detonate_at_tick == Some(now.tick()) {
            let shooter = self.first_key.and_then(|k| self.remote(k));
            for shooter in [shooter, None] {
                inbox.push(&NetMessage::Detonation {
                    shooter,
                    target: None,
                    position: self.detonate_position,
                })?;
            }
        }
        Ok(())
    }

    fn stop(&mut self) {
        if self.join() {
            log(Level::Info, &format!("net io thread joined {}", self.name));
        }
        log(Level::Info, &format!("net stop {}", self.name));
    }
}

impl Drop for NetAdapter {
    fn drop(&mut self) {
        // The SDK calls `stop` before the drop; this only covers a thread `stop` did not join.
        self.join();
    }
}

/// Declares one network capability type of `mode`.
macro_rules! network_capability {
    ($ty:ident, $id:literal, $mode:expr, $what:literal) => {
        #[doc = concat!("Network capability `", $id, "`.")]
        pub(crate) struct $ty;

        impl NetworkAdapterFactory for $ty {
            const ID: &'static str = $id;
            const PARAM_SCHEMA: &'static str = concat!(
                r#"{"type":"object","description":""#,
                $what,
                r#"","properties":{"key_base":{"type":"integer","minimum":0},"remote_offset":{"type":"integer","minimum":0},"panic_at_tick":{"type":"integer","minimum":0},"detonate_at_tick":{"type":"integer","minimum":0},"detonate_position":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}},"additionalProperties":false}"#
            );
            type Config = NetFixture;

            fn configure(input: &ConfigureInput) -> Result<NetFixture, Error> {
                NetFixture::configure($mode, input)
            }
        }
    };
}

#[cfg(not(feature = "network_peer"))]
network_capability!(
    NetOk,
    "test.network.net_ok",
    NetMode::Mirror,
    "Mirrors what it publishes"
);
#[cfg(not(feature = "network_peer"))]
network_capability!(
    NetPanicsInPoll,
    "test.network.net_panics_in_poll",
    NetMode::PanicsInPoll,
    "Panics in poll from panic_at_tick on"
);
#[cfg(not(feature = "network_peer"))]
network_capability!(
    NetBadMessage,
    "test.network.net_bad_message",
    NetMode::BadMessage,
    "Pushes a message the host refuses"
);
#[cfg(not(feature = "network_peer"))]
network_capability!(
    NetBlocksJoin,
    "test.network.net_blocks_join",
    NetMode::BlocksJoin,
    "Runs an I/O thread joined by stop"
);
#[cfg(feature = "network_peer")]
network_capability!(
    PeerOk,
    "test.network.peer_ok",
    NetMode::Mirror,
    "Mirrors what it publishes"
);
#[cfg(feature = "network_peer")]
network_capability!(
    PeerPanicsInPoll,
    "test.network.peer_panics_in_poll",
    NetMode::PanicsInPoll,
    "Panics in poll from panic_at_tick on"
);
