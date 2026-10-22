//! Conversions of the network adapter kind (ABI 0.1, ADR 0033) between the C ABI and the safe types
//! of [`crate::network`]. Host-written input (the `publish` batch, the `poll` sink) gets every reader
//! check of ADR 0019 plus the message rules of `umbra-plugin-api` `network.rs`: only the fields the
//! message's kind uses are read and checked (known codes, finite numbers, UTF-8 strings of at most
//! `UMBRA_MAX_ID_LEN` bytes checked before reading, non-zero subject keys, a non-empty `FIRE` weapon);
//! anything else is `UMBRA_ERR_INVALID_ARGUMENT` naming the field. Messages the SDK writes carry the
//! network kind's minor (`NETWORK_ADAPTER_MINOR`) and its size; unused fields are zero.
#![expect(
    unsafe_code,
    reason = "reads host-written network structs and calls the host's sink (ADR 0033)"
)]

use crate::abi::{
    UMBRA_DR_ACCELERATION, UMBRA_DR_ACCELERATION_ROTATION, UMBRA_DR_BODY_ACCELERATION,
    UMBRA_DR_BODY_ACCELERATION_ROTATION, UMBRA_DR_BODY_VELOCITY, UMBRA_DR_BODY_VELOCITY_ROTATION,
    UMBRA_DR_STATIC, UMBRA_DR_VELOCITY, UMBRA_DR_VELOCITY_ROTATION, UMBRA_MAX_ID_LEN,
    UMBRA_NET_CONTROL, UMBRA_NET_CONTROL_PAUSE, UMBRA_NET_CONTROL_RESUME, UMBRA_NET_CONTROL_START,
    UMBRA_NET_CONTROL_STOP, UMBRA_NET_DETONATION, UMBRA_NET_ENTITY_REMOVED, UMBRA_NET_ENTITY_STATE,
    UMBRA_NET_FIRE, UmbraCode, UmbraDeadReckoning, UmbraNetBatch, UmbraNetControl, UmbraNetMessage,
    UmbraNetSink, UmbraStr, UmbraToken,
};
use crate::error::Error;
use crate::minor::{NETWORK_ADAPTER_MINOR, abi_version};
use crate::network::{
    ControlAction, DeadReckoning, EntityState, Euler, NetKey, NetMessage, SimTime,
};

use super::{
    Headed, checked_ref, element_ok, malformed, size_of_u32, slice_from, str_from, umbra_str,
    umbra_vec3, vec3,
};

macro_rules! network_headed {
    ($($t:ident),* $(,)?) => {
        $(impl Headed for $t {
            const FIRST_MINOR: u32 = NETWORK_ADAPTER_MINOR;
        })*
    };
}
network_headed!(UmbraNetBatch, UmbraNetMessage, UmbraNetSink);

// ---------------------------------------------------------------- codes

/// The ABI code of a dead-reckoning model (ADR 0032 order).
pub(crate) fn dead_reckoning_code(model: DeadReckoning) -> UmbraDeadReckoning {
    match model {
        DeadReckoning::Static => UMBRA_DR_STATIC,
        DeadReckoning::Velocity => UMBRA_DR_VELOCITY,
        DeadReckoning::VelocityRotation => UMBRA_DR_VELOCITY_ROTATION,
        DeadReckoning::AccelerationRotation => UMBRA_DR_ACCELERATION_ROTATION,
        DeadReckoning::Acceleration => UMBRA_DR_ACCELERATION,
        DeadReckoning::BodyVelocity => UMBRA_DR_BODY_VELOCITY,
        DeadReckoning::BodyVelocityRotation => UMBRA_DR_BODY_VELOCITY_ROTATION,
        DeadReckoning::BodyAccelerationRotation => UMBRA_DR_BODY_ACCELERATION_ROTATION,
        DeadReckoning::BodyAcceleration => UMBRA_DR_BODY_ACCELERATION,
    }
}

fn dead_reckoning(code: UmbraDeadReckoning) -> Option<DeadReckoning> {
    Some(match code {
        UMBRA_DR_STATIC => DeadReckoning::Static,
        UMBRA_DR_VELOCITY => DeadReckoning::Velocity,
        UMBRA_DR_VELOCITY_ROTATION => DeadReckoning::VelocityRotation,
        UMBRA_DR_ACCELERATION_ROTATION => DeadReckoning::AccelerationRotation,
        UMBRA_DR_ACCELERATION => DeadReckoning::Acceleration,
        UMBRA_DR_BODY_VELOCITY => DeadReckoning::BodyVelocity,
        UMBRA_DR_BODY_VELOCITY_ROTATION => DeadReckoning::BodyVelocityRotation,
        UMBRA_DR_BODY_ACCELERATION_ROTATION => DeadReckoning::BodyAccelerationRotation,
        UMBRA_DR_BODY_ACCELERATION => DeadReckoning::BodyAcceleration,
        _ => return None,
    })
}

/// The ABI code of a run-control action.
pub(crate) fn control_code(action: ControlAction) -> UmbraNetControl {
    match action {
        ControlAction::Start => UMBRA_NET_CONTROL_START,
        ControlAction::Pause => UMBRA_NET_CONTROL_PAUSE,
        ControlAction::Resume => UMBRA_NET_CONTROL_RESUME,
        ControlAction::Stop => UMBRA_NET_CONTROL_STOP,
    }
}

fn control(code: UmbraNetControl) -> Option<ControlAction> {
    Some(match code {
        UMBRA_NET_CONTROL_START => ControlAction::Start,
        UMBRA_NET_CONTROL_PAUSE => ControlAction::Pause,
        UMBRA_NET_CONTROL_RESUME => ControlAction::Resume,
        UMBRA_NET_CONTROL_STOP => ControlAction::Stop,
        _ => return None,
    })
}

// ---------------------------------------------------------------- time

/// The simulation time of a call; `rate_hz` 0 is malformed.
pub(crate) fn sim_time(tick: u64, rate_hz: u32, what: &str) -> Result<SimTime, Error> {
    if rate_hz == 0 {
        return Err(malformed(&format!("{what}.rate_hz"), "0"));
    }
    Ok(SimTime::new(tick, rate_hz))
}

// ---------------------------------------------------------------- outbound (plugin -> host)

/// The ABI form of `message` in the network kind's minor, its strings borrowed from `message`.
/// `None` and absent keys are written as empty strings and 0; unused fields are 0. The message must
/// have passed [`NetMessage::validate`].
pub(crate) fn message_out(message: &NetMessage<'_>) -> UmbraNetMessage {
    let zero = umbra_vec3([0.0; 3]);
    let empty = umbra_str("");
    let key = |k: Option<NetKey>| k.map_or(0, |k| k.0);
    let text = |t: Option<&str>| t.map_or(empty, umbra_str);
    let mut out = UmbraNetMessage {
        abi_version: abi_version(NETWORK_ADAPTER_MINOR),
        struct_size: size_of_u32::<UmbraNetMessage>(),
        kind: 0,
        dead_reckoning: 0,
        key: 0,
        other_key: 0,
        position: zero,
        velocity: zero,
        acceleration: zero,
        angular_velocity: zero,
        orientation: zero,
        entity_type: empty,
        force: empty,
        name: empty,
        weapon: empty,
        control: 0,
        reserved_0: 0,
    };
    match *message {
        NetMessage::EntityState(ref s) => {
            out.kind = UMBRA_NET_ENTITY_STATE;
            out.dead_reckoning = dead_reckoning_code(s.dead_reckoning);
            out.key = s.key.0;
            out.position = umbra_vec3(s.position);
            out.velocity = umbra_vec3(s.velocity);
            out.acceleration = umbra_vec3(s.acceleration);
            out.angular_velocity = umbra_vec3(s.angular_velocity);
            out.orientation =
                umbra_vec3([s.orientation.yaw, s.orientation.pitch, s.orientation.roll]);
            out.entity_type = text(s.entity_type);
            out.force = text(s.force);
            out.name = text(s.name);
        }
        NetMessage::EntityRemoved { key } => {
            out.kind = UMBRA_NET_ENTITY_REMOVED;
            out.key = key.0;
        }
        NetMessage::Fire {
            shooter,
            target,
            weapon,
            position,
        } => {
            out.kind = UMBRA_NET_FIRE;
            out.key = shooter.0;
            out.other_key = key(target);
            out.weapon = umbra_str(weapon);
            out.position = umbra_vec3(position);
        }
        NetMessage::Detonation {
            shooter,
            target,
            position,
        } => {
            out.kind = UMBRA_NET_DETONATION;
            out.key = key(shooter);
            out.other_key = key(target);
            out.position = umbra_vec3(position);
        }
        NetMessage::SimulationControl(action) => {
            out.kind = UMBRA_NET_CONTROL;
            out.control = control_code(action);
        }
    }
    out
}

/// A validated copy of the host's message sink of one `poll` call.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NetSink {
    ctx: UmbraToken,
    push: unsafe extern "C" fn(UmbraToken, *const UmbraNetMessage) -> UmbraCode,
}

impl NetSink {
    /// Pushes `message` (validated by the caller) in the network kind's minor; returns the host's
    /// code.
    pub(crate) fn push(&self, message: &NetMessage<'_>) -> UmbraCode {
        let raw = message_out(message);
        // SAFETY: `push` comes from the sink validated by `net_sink` for the current `poll` call (the
        // `Inbox` holding this copy cannot outlive the call or leave its thread); `raw` and the
        // strings it borrows from `message` live across the call, and the host copies them; host
        // callbacks cannot unwind.
        unsafe { (self.push)(self.ctx, &raw const raw) }
    }
}

/// Validates the message sink of a `poll` call.
///
/// # Safety
/// As [`checked_ref`].
pub(crate) unsafe fn net_sink(sink: *const UmbraNetSink) -> Result<NetSink, Error> {
    // SAFETY: forwarded from this function's contract.
    let s = unsafe { checked_ref(sink) }.ok_or_else(|| malformed("sink", "header or pointer"))?;
    let push = s.push.ok_or_else(|| malformed("sink.push", "null"))?;
    Ok(NetSink { ctx: s.ctx, push })
}

// ---------------------------------------------------------------- inbound (host -> plugin)

/// A string of a host-written message: at most `UMBRA_MAX_ID_LEN` bytes (checked before reading),
/// UTF-8; empty is `None`.
///
/// # Safety
/// As [`str_from`].
unsafe fn optional_text<'a>(s: UmbraStr, what: &str) -> Result<Option<&'a str>, Error> {
    if s.len > UMBRA_MAX_ID_LEN {
        return Err(Error::invalid(format!(
            "{what} is {} bytes; max is {UMBRA_MAX_ID_LEN}",
            s.len
        )));
    }
    // SAFETY: forwarded from this function's contract.
    let t = unsafe { str_from(s) }.ok_or_else(|| malformed(what, "null or not UTF-8"))?;
    Ok((!t.is_empty()).then_some(t))
}

fn optional_key(key: u64) -> Option<NetKey> {
    (key != 0).then_some(NetKey(key))
}

/// The safe form of one host-written message (header already checked by the caller). This only
/// decodes (known codes, strings bounded before reading and UTF-8); the message rules themselves are
/// [`NetMessage::validate`], the one set of checks for both directions. Errors name the message
/// under `what`.
///
/// # Safety
/// The message's strings are valid for `'a` (the `publish` contract).
unsafe fn message_in<'a>(m: &UmbraNetMessage, what: &str) -> Result<NetMessage<'a>, Error> {
    let field = |f: &str| format!("{what}.{f}");
    let message = match m.kind {
        UMBRA_NET_ENTITY_STATE => {
            let o = vec3(m.orientation);
            // SAFETY: the message's strings are valid for `'a` (this function's contract).
            let (entity_type, force, name) = unsafe {
                (
                    optional_text(m.entity_type, &field("entity_type"))?,
                    optional_text(m.force, &field("force"))?,
                    optional_text(m.name, &field("name"))?,
                )
            };
            NetMessage::EntityState(EntityState {
                key: NetKey(m.key),
                entity_type,
                force,
                name,
                position: vec3(m.position),
                velocity: vec3(m.velocity),
                acceleration: vec3(m.acceleration),
                angular_velocity: vec3(m.angular_velocity),
                orientation: Euler {
                    yaw: o[0],
                    pitch: o[1],
                    roll: o[2],
                },
                dead_reckoning: dead_reckoning(m.dead_reckoning).ok_or_else(|| {
                    malformed(
                        &field("dead_reckoning"),
                        &format!("unknown model {}", m.dead_reckoning),
                    )
                })?,
            })
        }
        UMBRA_NET_ENTITY_REMOVED => NetMessage::EntityRemoved { key: NetKey(m.key) },
        UMBRA_NET_FIRE => NetMessage::Fire {
            shooter: NetKey(m.key),
            target: optional_key(m.other_key),
            // SAFETY: as above. Empty stays empty, so `validate` names the missing weapon.
            weapon: unsafe { optional_text(m.weapon, &field("weapon")) }?.unwrap_or(""),
            position: vec3(m.position),
        },
        UMBRA_NET_DETONATION => NetMessage::Detonation {
            shooter: optional_key(m.key),
            target: optional_key(m.other_key),
            position: vec3(m.position),
        },
        UMBRA_NET_CONTROL => {
            NetMessage::SimulationControl(control(m.control).ok_or_else(|| {
                malformed(&field("control"), &format!("unknown action {}", m.control))
            })?)
        }
        other => return Err(malformed(&field("kind"), &format!("unknown kind {other}"))),
    };
    message
        .validate()
        .map_err(|e| Error::invalid(format!("{what}: {}", e.message())))?;
    Ok(message)
}

/// Validates a `publish` batch completely before any trait call and returns its time and messages.
///
/// # Safety
/// `batch` is null or a host-written batch valid for the call `'a` (ADR 0033): its message array,
/// with the count it states, and every string in it readable for `'a`.
pub(crate) unsafe fn batch_in<'a>(
    batch: *const UmbraNetBatch,
) -> Result<(SimTime, Vec<NetMessage<'a>>), Error> {
    // SAFETY: forwarded from this function's contract.
    let b = unsafe { checked_ref(batch) }.ok_or_else(|| malformed("batch", "header or pointer"))?;
    let now = sim_time(b.tick, b.rate_hz, "batch")?;
    // SAFETY: `messages` holds `count` messages readable for `'a`; every bit pattern is valid.
    let raw = unsafe { slice_from(b.messages, b.count) }
        .ok_or_else(|| malformed("batch.messages", "null, misaligned or too long"))?;
    let mut out = Vec::with_capacity(raw.len());
    for (i, m) in raw.iter().enumerate() {
        let what = format!("batch.messages[{i}]");
        if !element_ok::<UmbraNetMessage>(m.abi_version, m.struct_size) {
            return Err(malformed(&what, "header"));
        }
        // SAFETY: the message's strings are valid for `'a` (this function's contract).
        out.push(unsafe { message_in(m, &what) }?);
    }
    Ok((now, out))
}

#[cfg(test)]
mod tests {
    use super::{batch_in, message_in, message_out};
    use crate::abi::{
        UMBRA_ABI_VERSION_MAJOR, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_MAX_ID_LEN, UmbraNetBatch,
        UmbraNetMessage, UmbraStr,
    };
    use crate::convert::size_of_u32;
    use crate::network::{ControlAction, DeadReckoning, EntityState, Euler, NetKey, NetMessage};

    static BAD: [u8; 2] = [0xFF, 0xFE];

    /// One edit of an invalid-message row.
    type Edit = Box<dyn Fn(&mut UmbraNetMessage)>;

    fn every_kind<'a>() -> Vec<NetMessage<'a>> {
        let models = [
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
        let mut out: Vec<NetMessage<'a>> = models
            .into_iter()
            .map(|dead_reckoning| {
                NetMessage::EntityState(EntityState {
                    key: NetKey(5),
                    entity_type: Some("1.2.3"),
                    force: Some("blue"),
                    name: Some("alpha"),
                    position: [1.0, 2.0, 3.0],
                    velocity: [4.0, 5.0, 6.0],
                    acceleration: [7.0, 8.0, 9.0],
                    angular_velocity: [0.1, 0.2, 0.3],
                    orientation: Euler {
                        yaw: 0.5,
                        pitch: -0.25,
                        roll: 1.5,
                    },
                    dead_reckoning,
                })
            })
            .collect();
        out.extend([
            NetMessage::EntityRemoved { key: NetKey(9) },
            NetMessage::Fire {
                shooter: NetKey(1),
                target: Some(NetKey(2)),
                weapon: "test.weapon.gun",
                position: [9.0, 8.0, 7.0],
            },
            NetMessage::Detonation {
                shooter: None,
                target: Some(NetKey(3)),
                position: [-1.0, 0.0, 1.0],
            },
        ]);
        out.extend(
            [
                ControlAction::Start,
                ControlAction::Pause,
                ControlAction::Resume,
                ControlAction::Stop,
            ]
            .map(NetMessage::SimulationControl),
        );
        out
    }

    fn read(m: &UmbraNetMessage) -> Result<NetMessage<'_>, crate::Error> {
        // SAFETY: a local message whose strings borrow live `'static` text or `BAD`.
        unsafe { message_in(m, "m") }
    }

    #[test]
    fn every_kind_round_trips_in_the_network_minor() {
        for m in every_kind() {
            let raw = message_out(&m);
            assert_eq!(
                raw.abi_version,
                (UMBRA_ABI_VERSION_MAJOR << 16) | 1,
                "minor 1"
            );
            assert_eq!(raw.struct_size, size_of_u32::<UmbraNetMessage>());
            assert_eq!(read(&raw).expect("reads back"), m);
        }
    }

    #[test]
    fn unused_fields_are_written_zero_and_ignored_when_read() {
        let removed = NetMessage::EntityRemoved { key: NetKey(9) };
        let mut raw = message_out(&removed);
        assert_eq!(
            (
                raw.other_key,
                raw.dead_reckoning,
                raw.control,
                raw.weapon.len
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(raw.position.x.to_bits(), 0);
        raw.velocity.x = f64::NAN;
        raw.dead_reckoning = 99;
        raw.control = 99;
        raw.name = UmbraStr {
            ptr: BAD.as_ptr(),
            len: BAD.len(),
        };
        raw.other_key = 4;
        assert_eq!(read(&raw).expect("unused fields ignored"), removed);
    }

    #[test]
    fn invalid_host_messages_are_refused_naming_the_field() {
        let messages = every_kind();
        let long = "x".repeat(UMBRA_MAX_ID_LEN + 1);
        let rows: Vec<(&str, Edit)> = vec![
            (
                "m.kind: malformed (unknown kind 0)",
                Box::new(|m| m.kind = 0),
            ),
            (
                "m.kind: malformed (unknown kind 6)",
                Box::new(|m| m.kind = 6),
            ),
            ("m.dead_reckoning", Box::new(|m| m.dead_reckoning = 10)),
            ("m: ENTITY_STATE key is 0", Box::new(|m| m.key = 0)),
            (
                "m: ENTITY_STATE position is not finite",
                Box::new(|m| m.position.z = f64::NAN),
            ),
            (
                "m: ENTITY_STATE orientation is not finite",
                Box::new(|m| m.orientation.x = f64::INFINITY),
            ),
            (
                "m: ENTITY_STATE angular_velocity is not finite",
                Box::new(|m| m.angular_velocity.y = f64::NAN),
            ),
            (
                "m.force: malformed (null or not UTF-8)",
                Box::new(|m| {
                    m.force = UmbraStr {
                        ptr: BAD.as_ptr(),
                        len: BAD.len(),
                    };
                }),
            ),
            (
                "m.name is 129 bytes; max is 128",
                Box::new(move |m| {
                    m.name = UmbraStr {
                        ptr: long.as_ptr(),
                        len: long.len(),
                    };
                }),
            ),
            (
                "m.entity_type: malformed (null or not UTF-8)",
                Box::new(|m| {
                    m.entity_type = UmbraStr {
                        ptr: std::ptr::null(),
                        len: 3,
                    };
                }),
            ),
        ];
        for (needle, edit) in rows {
            let mut m = message_out(&messages[0]);
            edit(&mut m);
            let e = read(&m).expect_err(needle);
            assert_eq!(e.code(), UMBRA_ERR_INVALID_ARGUMENT, "{needle}");
            assert!(e.message().contains(needle), "{needle}: {}", e.message());
        }
        let mut fire = message_out(&every_kind()[10]);
        fire.weapon = UmbraStr {
            ptr: std::ptr::null(),
            len: 0,
        };
        assert!(read(&fire).is_err_and(|e| e.message().contains("m: FIRE weapon is empty")));
        fire.weapon = message_out(&every_kind()[10]).weapon;
        fire.key = 0;
        assert!(read(&fire).is_err_and(|e| e.message().contains("m: FIRE shooter is 0")));
        let mut control = message_out(&every_kind()[12]);
        control.control = 5;
        assert!(read(&control).is_err_and(|e| e.message().contains("m.control")));
    }

    fn batch(messages: &[UmbraNetMessage], rate_hz: u32) -> UmbraNetBatch {
        UmbraNetBatch {
            abi_version: (UMBRA_ABI_VERSION_MAJOR << 16) | 1,
            struct_size: size_of_u32::<UmbraNetBatch>(),
            tick: 3,
            rate_hz,
            reserved_0: 0,
            messages: messages.as_ptr(),
            count: messages.len(),
        }
    }

    #[test]
    fn a_batch_is_validated_whole_and_names_the_message() {
        let all = every_kind();
        let raw = || -> Vec<UmbraNetMessage> { all.iter().map(message_out).collect() };
        let good = raw();
        let b = batch(&good, 30);
        // SAFETY: a local batch over a live array whose strings are `'static`.
        let (now, got) = unsafe { batch_in(&raw const b) }.expect("valid batch");
        assert_eq!((now.tick(), now.rate_hz()), (3, 30));
        assert_eq!(got, every_kind());

        let zero_rate = batch(&good, 0);
        // SAFETY: as above.
        let e = unsafe { batch_in(&raw const zero_rate) }.expect_err("rate 0");
        assert!(e.message().contains("batch.rate_hz"), "{}", e.message());

        let mut bad = raw();
        bad[2].struct_size += 8;
        let b = batch(&bad, 30);
        // SAFETY: as above.
        let e = unsafe { batch_in(&raw const b) }.expect_err("element size");
        assert!(
            e.message()
                .contains("batch.messages[2]: malformed (header)"),
            "{}",
            e.message()
        );

        let mut bad = raw();
        bad[1].abi_version = UMBRA_ABI_VERSION_MAJOR << 16;
        let b = batch(&bad, 30);
        // SAFETY: as above.
        let e = unsafe { batch_in(&raw const b) }.expect_err("older than the struct");
        assert!(e.message().contains("batch.messages[1]"), "{}", e.message());

        let mut null = batch(&good, 30);
        null.messages = std::ptr::null();
        // SAFETY: a null array with a count is refused before reading.
        let e = unsafe { batch_in(&raw const null) }.expect_err("null array");
        assert!(e.message().contains("batch.messages"), "{}", e.message());

        let mut empty = batch(&[], 30);
        empty.messages = std::ptr::null();
        // SAFETY: count 0, never read.
        assert!(unsafe { batch_in(&raw const empty) }.is_ok_and(|(_, m)| m.is_empty()));
    }
}
