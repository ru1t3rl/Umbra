//! The network adapter bridge (ABI 0.1; REQ-N-1, REQ-P-6..8, REQ-P-11, D-8, ADR 0021, ADR 0030, ADR
//! 0033): a plugin network adapter capability as a core `Factory<dyn NetworkAdapterConfig>`, its
//! configuration as a [`NetworkAdapterConfig`] and the started adapter as a [`NetworkAdapter`].
//!
//! **Lifecycle** (`umbra-plugin-api` `network.rs`): `build` = `configure` (the ADR 0030 wrapper
//! `{"name", "params", "templates"}` as JSON text) then `key_space` (cached: `count >= 1`, `base +
//! count` must not overflow); `start` at most once per configuration (a second one is
//! [`NetError::AlreadyStarted`] without a plugin call); `publish`, `poll`, `counters`; `stop` once,
//! however often the core calls it; `destroy` exactly once, when the configuration and the adapter
//! are both gone (the plugin handle is shared by `Arc`), after `stop` if the adapter was started and
//! not stopped. Both still run for a faulted capability. Every holder keeps the capability and so
//! the image: the image cannot unload before `destroy` returned, and `destroy` joins the plugin's
//! threads (its contract), so no plugin code runs after the unload.
//!
//! **Errors and faults** (ADR 0021, ADR 0033, ASM-252, ASM-288): a non-OK `configure` or `key_space`
//! (a panic included) is a [`BuildError`] naming the capability, a non-OK `start` a
//! [`NetError::Start`]; neither faults the capability, like `configure` and `instantiate` of a
//! behavior (R-M9). A non-OK `publish`, `poll`, `counters` or `stop`, or a sink misuse (an invalid
//! message, more than `max` pushes, a stale token), faults the capability for this host under the
//! scenario's policy and returns [`NetError::Faulted`], so the manager stops and disables the adapter;
//! the messages of a faulted `poll` are discarded whole. A faulted capability gets no further calls
//! except `stop` and `destroy`; its other adapters fail their next call. Transient I/O problems are
//! the plugin's to count; it reports them through `counters` and returns `UMBRA_OK`.
//!
//! **Counters** are read from the plugin on the tick path only: after every clean `poll` and right
//! before `stop` (zero-filled first); [`NetworkAdapter::counters`] returns the cached value without a
//! plugin call, so a status query from another thread never calls into the plugin.
//!
//! **Threads and buffers.** Calls take the image's blocking call lock like every plugin call; the
//! bridge itself never blocks otherwise (the plugin's `publish` and `poll` move data to or from its
//! own bounded queues). The outbound messages are marshalled into a buffer reused across calls whose
//! string pointers borrow the core messages for the call only; the inbound messages are collected by
//! the sink into a vector moved into the call record during the call and back afterwards.

#![expect(
    unsafe_code,
    reason = "plugin network adapter handle and calls through the vtable"
)]

use std::sync::{Arc, Mutex, PoisonError};

use umbra_core::net::{
    AdapterCounters, AdapterStart, InboundEvent, KeySpace, NetError, NetworkAdapter,
    NetworkAdapterConfig, OutboundEvent,
};
use umbra_core::{BuildError, ComponentId, Factory, ParamSchema, ParamValue, SimTime};
use umbra_plugin_api::{
    UMBRA_MAX_NET_COUNTERS, UMBRA_MAX_NET_POLL, UmbraNetAdapter, UmbraNetBatch, UmbraNetMessage,
};

use super::batch::{PluginCapability, problem};
use super::factory::in_scope;
use super::marshal::{self, written_size, written_version};
use super::sinks;
use crate::api::HostMode;
use crate::descriptor::NetworkVTable;
use crate::log::LogLevel;
use crate::tokens::{self, PollCall, Sink};

/// A network adapter capability of one host.
pub(crate) type NetCapability = PluginCapability<NetworkVTable>;

/// How many counters this host reads, in the ABI order tx dropped, rx dropped, decode errors, I/O
/// errors (`AdapterCounters`).
const HOST_COUNTERS: usize = 4;
const _: () = assert!(HOST_COUNTERS <= UMBRA_MAX_NET_COUNTERS);

/// The registry factory of one network adapter capability of one host.
pub(crate) struct PluginNetworkFactory {
    capability: Arc<NetCapability>,
}

impl PluginNetworkFactory {
    /// The factory of `capability`.
    pub(crate) fn new(capability: Arc<NetCapability>) -> Self {
        Self { capability }
    }
}

impl Factory<dyn NetworkAdapterConfig> for PluginNetworkFactory {
    fn id(&self) -> &ComponentId {
        self.capability.id()
    }

    fn param_schema(&self) -> &ParamSchema {
        &self.capability.capability().schema
    }

    /// `configure` with `input` (the ADR 0030 wrapper) as JSON text, then `key_space`. In a run-mode
    /// host only inside the host's build scope (RR-m2), like behaviors.
    fn build(&self, input: &ParamValue) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        let cap = &self.capability;
        let fail = |message: String| {
            BuildError::new(format!("plugin network adapter {}: {message}", cap.id()))
        };
        if cap.host().mode == HostMode::Run && !in_scope(cap.host()) {
            return Err(fail("plugin host not attached to this engine".to_owned()));
        }
        if cap.faulted() {
            return Err(fail("the capability is faulted for this host".to_owned()));
        }
        let text = serde_json::to_string(input).map_err(|e| fail(e.to_string()))?;
        let configure = cap.vtable().configure;
        let mut out: *mut UmbraNetAdapter = std::ptr::null_mut();
        // SAFETY: `configure` is a required entry of the open image's vtable; `text`, `out` and the
        // error sink outlive the call; the call lock serialises calls.
        let outcome = cap.call(|scope, _| unsafe {
            configure(marshal::text(&text), &raw mut out, scope.error_sink())
        });
        let (code, finished) = outcome.map_err(|e| fail(format!("configure: {e}")))?;
        if let Some(problem) = problem(code, &finished) {
            return Err(fail(format!("configure {problem}")));
        }
        if out.is_null() {
            return Err(fail(
                "configure returned UMBRA_OK with a null handle".to_owned(),
            ));
        }
        // From here on the handle is destroyed when the last holder drops (also on the errors
        // below).
        let handle = Arc::new(AdapterHandle {
            capability: Arc::clone(cap),
            raw: out,
            life: Mutex::new(Life::default()),
        });
        let key_space = handle.key_space().map_err(fail)?;
        Ok(Arc::new(PluginNetworkConfig { handle, key_space }))
    }
}

/// Where an adapter handle is in its lifecycle.
#[derive(Default)]
struct Life {
    /// `start` was called (successfully or not).
    started: bool,
    /// `stop` was called.
    stopped: bool,
}

/// The plugin's configured adapter handle; destroyed (after `stop` when started and not stopped)
/// when the last holder (the configuration, the started adapter) drops.
struct AdapterHandle {
    capability: Arc<NetCapability>,
    raw: *mut UmbraNetAdapter,
    life: Mutex<Life>,
}

// SAFETY: the handle is an opaque plugin pointer the host never dereferences; it is only passed to
// calls into the plugin, which are serialised by the image's blocking call lock (ADR 0019 "Threads
// and re-entry"), so sharing and sending it between threads never lets the plugin see concurrent
// calls. The lifecycle flags are behind a mutex. Exercised by `network_bridge.rs`
// `a_started_adapter_moves_to_another_thread_and_is_used_and_dropped_there`.
unsafe impl Send for AdapterHandle {}
// SAFETY: as above; `&AdapterHandle` only reads the pointer value and locks the flags (the same
// test shares the configuration's handle between the two threads).
unsafe impl Sync for AdapterHandle {}

impl AdapterHandle {
    fn life(&self) -> std::sync::MutexGuard<'_, Life> {
        // Every update is a single flag write, so a poisoned lock holds consistent flags.
        self.life.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn log_error(&self, message: &str) {
        let cap = &self.capability;
        cap.host().log.log(
            cap.plugin(),
            LogLevel::Error,
            &format!("capability {}: {message}", cap.id()),
        );
    }

    /// The plugin's key space: `count >= 1` and `base + count` within `u64`.
    fn key_space(&self) -> Result<KeySpace, String> {
        let cap = &self.capability;
        let entry = cap.vtable().key_space;
        let raw = self.raw;
        let (mut base, mut count) = (0_u64, 0_u64);
        // SAFETY: `key_space` is a required entry of the open image's vtable; `raw` is a live
        // configured adapter; both out pointers outlive the call; the call lock serialises calls.
        let outcome = cap.call(|_, _| unsafe { entry(raw, &raw mut base, &raw mut count) });
        let (code, finished) = outcome.map_err(|e| format!("key_space: {e}"))?;
        if let Some(problem) = problem(code, &finished) {
            return Err(format!("key_space {problem}"));
        }
        if count == 0 {
            return Err("key_space count is 0 (an adapter issues at least one key)".to_owned());
        }
        if base.checked_add(count).is_none() {
            return Err(format!(
                "key_space base {base} + count {count} overflows u64"
            ));
        }
        Ok(KeySpace { base, count })
    }

    /// Calls the plugin's `stop` once per handle (later calls do nothing); a non-OK result faults
    /// the capability. Called on the started adapter's `stop` and, for an adapter whose `start`
    /// failed, before `destroy`.
    fn stop(&self) {
        {
            let mut life = self.life();
            if life.stopped {
                return;
            }
            life.stopped = true;
        }
        let cap = &self.capability;
        let entry = cap.vtable().stop;
        let raw = self.raw;
        // SAFETY: `stop` is a required entry of the open image's vtable; `raw` is a live configured
        // adapter (destroyed only after this); the call lock serialises calls.
        match cap.call(|_, _| unsafe { entry(raw) }) {
            Ok((code, finished)) => {
                if let Some(problem) = problem(code, &finished) {
                    cap.fault("stop", problem);
                }
            }
            Err(e) => self.log_error(&format!("stop skipped: {e}")),
        }
    }
}

impl Drop for AdapterHandle {
    fn drop(&mut self) {
        let unstopped = {
            let life = self.life();
            life.started && !life.stopped
        };
        if unstopped {
            self.stop();
        }
        let cap = &self.capability;
        let destroy = cap.vtable().destroy;
        let raw = self.raw;
        // SAFETY: `destroy` is a required entry of the open image's vtable (the capability holds
        // the image); `raw` is a live configured adapter, destroyed exactly once, here, after every
        // other holder dropped; the call lock serialises calls.
        if let Err(e) = cap.call(|_, _| unsafe { destroy(raw) }) {
            self.log_error(&format!("destroy skipped: {e}"));
        }
    }
}

/// A configured plugin network adapter (the factory's product).
struct PluginNetworkConfig {
    handle: Arc<AdapterHandle>,
    key_space: KeySpace,
}

impl NetworkAdapterConfig for PluginNetworkConfig {
    fn key_space(&self) -> KeySpace {
        self.key_space
    }

    fn start(&self, ctx: &AdapterStart) -> Result<Box<dyn NetworkAdapter>, NetError> {
        let handle = &self.handle;
        let cap = &handle.capability;
        let fail = |message: String| {
            NetError::Start(format!("plugin network adapter {}: {message}", cap.id()))
        };
        // A start the host refuses claims nothing: the handle stays unstarted, so dropping it calls
        // only `destroy`.
        if cap.host().mode == HostMode::Validation {
            return Err(fail("a validation host never starts adapters".to_owned()));
        }
        if cap.faulted() {
            return Err(fail("the capability is faulted for this host".to_owned()));
        }
        {
            // Check and claim under one lock, right before the plugin call: a second `start` is
            // `AlreadyStarted` without a plugin call, and a failed plugin `start` still counts as
            // started (ASM-408(e): `stop` runs before `destroy`).
            let mut life = handle.life();
            if life.started {
                return Err(NetError::AlreadyStarted);
            }
            life.started = true;
        }
        let entry = cap.vtable().start;
        let raw = handle.raw;
        let (tick, rate_hz) = (ctx.now.tick(), ctx.now.rate().hz());
        // SAFETY: `start` is a required entry of the open image's vtable; `raw` is a live configured
        // adapter; the error sink outlives the call; the call lock serialises calls.
        let outcome = cap.call(|scope, _| unsafe { entry(raw, tick, rate_hz, scope.error_sink()) });
        let (code, finished) = outcome.map_err(|e| fail(format!("start: {e}")))?;
        if let Some(problem) = problem(code, &finished) {
            return Err(fail(format!("start {problem}")));
        }
        Ok(Box::new(PluginNetworkAdapter {
            handle: Arc::clone(handle),
            counters: AdapterCounters::default(),
            out: MessageBuf::default(),
            inbox: Vec::new(),
        }))
    }
}

/// Reused outbound marshalling buffer. Its messages hold string pointers into the core messages of
/// one `publish` call; it is cleared after every call.
#[derive(Default)]
struct MessageBuf(Vec<UmbraNetMessage>);

// SAFETY: the buffer's raw string pointers are written, used and cleared within one `publish` call
// on the calling thread (`PluginNetworkAdapter::publish` clears it on every path before returning),
// so between calls it holds no pointer and moving it to another thread moves no live pointer. The
// host never dereferences them. Exercised by `network_bridge.rs`
// `a_started_adapter_moves_to_another_thread_and_is_used_and_dropped_there`.
unsafe impl Send for MessageBuf {}

/// A started plugin network adapter.
struct PluginNetworkAdapter {
    handle: Arc<AdapterHandle>,
    /// The counters read after the last clean `poll` (or before `stop`).
    counters: AdapterCounters,
    out: MessageBuf,
    /// Inbound messages of the current `poll`, reused.
    inbox: Vec<InboundEvent>,
}

impl PluginNetworkAdapter {
    fn cap(&self) -> &Arc<NetCapability> {
        &self.handle.capability
    }

    /// `Err` when the adapter may not be called any more (stopped, or its capability faulted).
    fn usable(&self, call: &str) -> Result<(), NetError> {
        let id = self.cap().id();
        if self.handle.life().stopped {
            return Err(NetError::Faulted(format!(
                "plugin network adapter {id}: {call} after stop"
            )));
        }
        if self.cap().faulted() {
            return Err(NetError::Faulted(format!(
                "plugin network adapter {id}: the capability is faulted for this host"
            )));
        }
        Ok(())
    }

    /// Records a fault of `call` and returns the error the manager sees.
    fn fault(&self, call: &'static str, message: String) -> NetError {
        let cap = self.cap();
        let error = NetError::Faulted(format!(
            "plugin network adapter {}: {call} {message}",
            cap.id()
        ));
        cap.fault(call, message);
        error
    }

    /// Reads the plugin's counters into the cache (zero-filled first; a null entry means all 0). A
    /// non-OK result faults the capability.
    fn refresh_counters(&mut self) -> Result<(), NetError> {
        let Some(entry) = self.cap().vtable().counters else {
            return Ok(());
        };
        let raw = self.handle.raw;
        let mut values = [0_u64; HOST_COUNTERS];
        // SAFETY: `counters` is a non-null entry of the open image's vtable; `raw` is a live started
        // adapter; `values` has `HOST_COUNTERS` writable entries for the call; the call lock
        // serialises calls.
        let outcome = self
            .cap()
            .call(|_, _| unsafe { entry(raw, values.as_mut_ptr(), HOST_COUNTERS) });
        let (code, finished) = match outcome {
            Ok(done) => done,
            Err(e) => return Err(self.fault("counters", e.to_string())),
        };
        if let Some(problem) = problem(code, &finished) {
            return Err(self.fault("counters", problem));
        }
        let mut counters = AdapterCounters::default();
        counters.tx_dropped = values[0];
        counters.rx_dropped = values[1];
        counters.decode_errors = values[2];
        counters.io_errors = values[3];
        self.counters = counters;
        Ok(())
    }
}

impl NetworkAdapter for PluginNetworkAdapter {
    fn publish(&mut self, now: SimTime, out: &[OutboundEvent]) -> Result<(), NetError> {
        self.usable("publish")?;
        let minor = self.cap().minor();
        let mut buf = std::mem::take(&mut self.out);
        buf.0.clear();
        buf.0.extend(
            out.iter()
                .filter_map(|m| marshal::net_message_out(m, minor)),
        );
        let batch = UmbraNetBatch {
            abi_version: written_version(minor),
            struct_size: written_size::<UmbraNetBatch>(minor),
            tick: now.tick(),
            rate_hz: now.rate().hz(),
            reserved_0: 0,
            messages: if buf.0.is_empty() {
                std::ptr::null()
            } else {
                buf.0.as_ptr()
            },
            count: buf.0.len(),
        };
        let entry = self.cap().vtable().publish;
        let raw = self.handle.raw;
        // SAFETY: `publish` is a required entry of the open image's vtable; `raw` is a live started
        // adapter; `batch`, the message array it points to and the strings those point to (borrowed
        // from `out`) are neither moved nor touched until the call returns; the error sink lives
        // for the call; the call lock serialises calls.
        let outcome = self
            .cap()
            .call(|scope, _| unsafe { entry(raw, &raw const batch, scope.error_sink()) });
        buf.0.clear();
        self.out = buf;
        let (code, finished) = outcome.map_err(|e| self.fault("publish", e.to_string()))?;
        match problem(code, &finished) {
            Some(problem) => Err(self.fault("publish", problem)),
            None => Ok(()),
        }
    }

    fn poll(
        &mut self,
        now: SimTime,
        max: usize,
        into: &mut Vec<InboundEvent>,
    ) -> Result<(), NetError> {
        self.usable("poll")?;
        let max = max.min(UMBRA_MAX_NET_POLL);
        let minor = self.cap().minor();
        let mut inbox = std::mem::take(&mut self.inbox);
        inbox.clear();
        let entry = self.cap().vtable().poll;
        let raw = self.handle.raw;
        let tick = now.tick();
        let outcome = self.cap().call(|scope, _| {
            let ready = tokens::with_top(|r| {
                r.poll = Some(PollCall {
                    max,
                    messages: inbox,
                });
            });
            if ready.is_none() {
                // No top record: the inbox (moved into the closure) is dropped and the next poll
                // starts cold with a fresh vector. Unreachable while a call is active, since
                // `call` pushes the record before running this.
                return (
                    Err("no call record on this thread (thread-local storage)"),
                    None,
                );
            }
            let sink = sinks::net_sink(scope.sink(Sink::NetPoll), minor);
            // SAFETY: `poll` is a required entry of the open image's vtable; `raw` is a live started
            // adapter; the sink and the error sink live on this frame for the call; the call lock
            // serialises calls. A plugin cannot unwind.
            let code = unsafe { entry(raw, tick, max, &raw const sink, scope.error_sink()) };
            (Ok(code), tokens::with_top(|r| r.poll.take()).flatten())
        });
        let ((code, state), finished) = outcome.map_err(|e| self.fault("poll", e.to_string()))?;
        let state = state.unwrap_or_default();
        self.inbox = state.messages;
        let code = code.map_err(|why| self.fault("poll", why.to_owned()))?;
        if let Some(problem) = problem(code, &finished) {
            // Discarded whole (ADR 0033).
            self.inbox.clear();
            return Err(self.fault("poll", problem));
        }
        if let Err(e) = self.refresh_counters() {
            self.inbox.clear();
            return Err(e);
        }
        into.append(&mut self.inbox);
        Ok(())
    }

    fn counters(&self) -> AdapterCounters {
        self.counters
    }

    fn stop(&mut self) {
        if self.handle.life().stopped {
            return;
        }
        if !self.cap().faulted() {
            // The last counters, read on the tick path; a failure faults the capability, and the
            // adapter is stopped anyway.
            let _ = self.refresh_counters();
        }
        self.handle.stop();
    }
}

impl Drop for PluginNetworkAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use umbra_core::dead_reckoning::DeadReckoning;
    use umbra_core::net::{NetEntityState, NetEntityType, NetKey, NetMessage};
    use umbra_core::{ControlAction, Ecef, Force, PluginId, Radians, Vec3, WeaponId, WorldEuler};
    use umbra_plugin_api::{
        UMBRA_ABI_VERSION, UMBRA_ERR_INVALID_ARGUMENT, UMBRA_ERR_STATE, UMBRA_MAX_ID_LEN,
        UMBRA_NET_CONTROL, UMBRA_NET_ENTITY_REMOVED, UMBRA_NET_FIRE, UMBRA_OK, UmbraNetMessage,
        UmbraStr,
    };

    use super::super::marshal::{self, NET_MESSAGE_SIZES};
    use super::super::sinks;
    use crate::log::TracingLog;
    use crate::tokens::{self, ActiveCall, CallRecord, PollCall, Sink};

    /// Bytes that are not UTF-8.
    static BAD: [u8; 3] = [0xFF, 0xFE, 0xFD];

    /// One edit of an invalid-message row.
    type Edit = Box<dyn Fn(&mut UmbraNetMessage)>;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    fn state() -> NetMessage {
        NetMessage::EntityState(NetEntityState {
            key: NetKey(42),
            entity_type: Some(NetEntityType::new("1.2.3.4.5.6.7")),
            force: Some(Force::new("blue").unwrap_or_else(|e| panic!("{e}"))),
            name: Some(Arc::from("alpha one")),
            position: Ecef(v(1.0, 2.0, 3.0)),
            velocity: v(4.0, 5.0, 6.0),
            acceleration: v(7.0, 8.0, 9.0),
            angular_velocity: v(0.1, 0.2, 0.3),
            orientation: WorldEuler {
                yaw: Radians(0.5),
                pitch: Radians(-0.25),
                roll: Radians(1.5),
            },
            dead_reckoning: DeadReckoning::BodyAccelerationRotation,
        })
    }

    fn every_kind() -> Vec<NetMessage> {
        vec![
            state(),
            NetMessage::EntityRemoved { key: NetKey(7) },
            NetMessage::Fire {
                shooter: NetKey(1),
                target: Some(NetKey(2)),
                weapon: WeaponId::new("test.weapon.gun").unwrap_or_else(|e| panic!("{e}")),
                position: Ecef(v(9.0, 8.0, 7.0)),
            },
            NetMessage::Fire {
                shooter: NetKey(1),
                target: None,
                weapon: WeaponId::new("test.weapon.gun").unwrap_or_else(|e| panic!("{e}")),
                position: Ecef(v(9.0, 8.0, 7.0)),
            },
            NetMessage::Detonation {
                shooter: None,
                target: Some(NetKey(3)),
                position: Ecef(v(-1.0, 0.0, 1.0)),
            },
            NetMessage::SimulationControl(ControlAction::Start),
            NetMessage::SimulationControl(ControlAction::Pause),
            NetMessage::SimulationControl(ControlAction::Resume),
            NetMessage::SimulationControl(ControlAction::Stop),
        ]
    }

    fn read(m: &UmbraNetMessage, ceiling: u32) -> Result<NetMessage, String> {
        // SAFETY: a valid local message whose strings borrow live core messages.
        unsafe { marshal::net_message_within(m, ceiling) }
    }

    #[test]
    fn every_message_kind_round_trips_through_the_abi() {
        for m in every_kind() {
            let raw = marshal::net_message_out(&m, 1).unwrap_or_else(|| panic!("{m:?}"));
            assert_eq!(raw.abi_version, UMBRA_ABI_VERSION, "written in minor 1");
            assert_eq!(read(&raw, 1), Ok(m));
        }
    }

    #[test]
    fn every_dead_reckoning_model_round_trips() {
        use DeadReckoning as D;
        let models = [
            D::Static,
            D::Velocity,
            D::VelocityRotation,
            D::AccelerationRotation,
            D::Acceleration,
            D::BodyVelocity,
            D::BodyVelocityRotation,
            D::BodyAccelerationRotation,
            D::BodyAcceleration,
        ];
        for (code, model) in (1_u32..).zip(models) {
            let NetMessage::EntityState(mut s) = state() else {
                panic!("state");
            };
            s.dead_reckoning = model;
            let m = NetMessage::EntityState(s);
            let raw = marshal::net_message_out(&m, 1).unwrap_or_else(|| panic!("{model:?}"));
            assert_eq!(raw.dead_reckoning, code, "ADR 0032 order");
            assert_eq!(read(&raw, 1), Ok(m));
        }
    }

    /// The `ENTITY_STATE` the host writes for `state()` with `name` and `entity_type`, read back.
    fn sent_state(name: &str, entity_type: &str) -> NetEntityState {
        let NetMessage::EntityState(mut s) = state() else {
            panic!("state");
        };
        s.name = Some(Arc::from(name));
        s.entity_type = Some(NetEntityType::new(entity_type));
        let raw = marshal::net_message_out(&NetMessage::EntityState(s), 1)
            .unwrap_or_else(|| panic!("out"));
        assert!(raw.name.len <= UMBRA_MAX_ID_LEN && raw.entity_type.len <= UMBRA_MAX_ID_LEN);
        match read(&raw, 1) {
            Ok(NetMessage::EntityState(s)) => s,
            other => panic!("{other:?}"),
        }
    }

    /// ASM-409: outbound strings respect the ABI bound. Rows: (name, entity type, the name read
    /// back, the entity type read back).
    #[test]
    fn outbound_strings_are_bounded_by_the_abi() {
        let max = UMBRA_MAX_ID_LEN;
        let a = |n: usize| "a".repeat(n);
        // 127 ASCII bytes then a 2-byte char straddling the limit (bytes 128..129).
        let straddle = format!("{}\u{e9}", a(max - 1));
        let rows: [(String, String, Option<String>, Option<String>); 4] = [
            (a(max), a(max), Some(a(max)), Some(a(max))),
            (a(max + 1), "t".into(), Some(a(max)), Some("t".into())),
            (straddle, "t".into(), Some(a(max - 1)), Some("t".into())),
            ("n".into(), a(max + 1), Some("n".into()), None),
        ];
        for (name, ty, want_name, want_ty) in rows {
            let got = sent_state(&name, &ty);
            assert_eq!(
                got.name.as_deref(),
                want_name.as_deref(),
                "name of {name:?}"
            );
            assert_eq!(
                got.entity_type.as_ref().map(NetEntityType::as_str),
                want_ty.as_deref(),
                "entity type of {ty:?}"
            );
        }
    }

    #[test]
    fn a_step_control_has_no_abi_form() {
        let step = NetMessage::SimulationControl(ControlAction::Step(3));
        assert!(marshal::net_message_out(&step, 1).is_none());
    }

    #[test]
    fn fields_the_kind_does_not_use_are_ignored() {
        let removed = NetMessage::EntityRemoved { key: NetKey(9) };
        let mut raw = marshal::net_message_out(&removed, 1).unwrap_or_else(|| panic!("out"));
        let bad = UmbraStr {
            ptr: BAD.as_ptr(),
            len: BAD.len(),
        };
        raw.velocity.x = f64::NAN;
        raw.dead_reckoning = 99;
        raw.control = 99;
        raw.name = bad;
        raw.weapon = bad;
        raw.other_key = 5;
        assert_eq!(read(&raw, 1), Ok(removed));
    }

    #[test]
    fn invalid_messages_name_the_problem() {
        let messages = every_kind();
        let out =
            |i: usize| marshal::net_message_out(&messages[i], 1).unwrap_or_else(|| panic!("out"));
        let long: &'static str = Box::leak(
            "x".repeat(umbra_plugin_api::UMBRA_MAX_ID_LEN + 1)
                .into_boxed_str(),
        );
        let rows: [(&str, Edit); 13] = [
            ("kind 0", Box::new(|m| m.kind = 0)),
            ("kind 6", Box::new(|m| m.kind = 6)),
            ("dead_reckoning", Box::new(|m| m.dead_reckoning = 0)),
            ("key is 0", Box::new(|m| m.key = 0)),
            ("position", Box::new(|m| m.position.y = f64::INFINITY)),
            ("orientation", Box::new(|m| m.orientation.z = f64::NAN)),
            (
                "angular_velocity",
                Box::new(|m| m.angular_velocity.x = f64::NAN),
            ),
            (
                "UTF-8",
                Box::new(|m| {
                    m.name = UmbraStr {
                        ptr: BAD.as_ptr(),
                        len: BAD.len(),
                    };
                }),
            ),
            (
                "max is 128",
                Box::new(move |m| {
                    m.entity_type = marshal::text(long);
                }),
            ),
            (
                "force",
                Box::new(|m| m.force = marshal::text("Not A Force")),
            ),
            ("struct_size", Box::new(|m| m.struct_size -= 8)),
            ("abi_version", Box::new(|m| m.abi_version = 2)),
            ("pointer is null", Box::new(|_| {})),
        ];
        for (needle, edit) in rows {
            let mut m = out(0);
            edit(&mut m);
            let got = if needle == "pointer is null" {
                // SAFETY: refused before any read.
                unsafe { marshal::net_message_within(std::ptr::null(), 1) }
            } else {
                read(&m, 1)
            };
            assert!(
                got.as_ref().is_err_and(|e| e.contains(needle)),
                "{needle}: {got:?}"
            );
        }
        let mut fire = out(2);
        fire.weapon = marshal::text("");
        assert!(read(&fire, 1).is_err_and(|e| e.contains("FIRE weapon is empty")));
        fire.kind = UMBRA_NET_FIRE;
        fire.weapon = marshal::text("test.weapon.gun");
        fire.key = 0;
        assert!(read(&fire, 1).is_err_and(|e| e.contains("FIRE key")));
        let mut control = out(5);
        control.kind = UMBRA_NET_CONTROL;
        control.control = 5;
        assert!(read(&control, 1).is_err_and(|e| e.contains("CONTROL control 5")));
        let mut removed = out(1);
        removed.kind = UMBRA_NET_ENTITY_REMOVED;
        removed.key = 0;
        assert!(read(&removed, 1).is_err_and(|e| e.contains("ENTITY_REMOVED key")));
    }

    #[test]
    fn the_declared_minor_is_the_ceiling_of_an_inbound_message() {
        let message = state();
        let raw = marshal::net_message_out(&message, 1).unwrap_or_else(|| panic!("out"));
        // A message declaring minor 1 from a plugin declaring minor 0 (the ceiling) is refused;
        // a message declaring minor 0 is older than the struct.
        assert!(read(&raw, 0).is_err_and(|e| e.contains("newer than the plugin's declared ABI")));
        let mut old = raw;
        old.abi_version = 0;
        assert!(read(&old, 1).is_err_and(|e| e.contains("which defines it")));
        assert_eq!(NET_MESSAGE_SIZES[1], std::mem::size_of::<UmbraNetMessage>());
    }

    fn call(image: u64, generation: u32, minor: u32) -> ActiveCall {
        ActiveCall::push(
            CallRecord::new(
                image,
                generation,
                PluginId::new("test.plugin.netsink").unwrap_or_else(|e| panic!("{e}")),
                Arc::new(TracingLog),
            )
            .with_minor(minor),
        )
    }

    #[test]
    fn the_message_sink_copies_up_to_max_and_refuses_misuse() {
        let image = tokens::make(300, tokens::HOST_CTX_INDEX);
        let active = call(image, 9, 1);
        tokens::with_top(|r| {
            r.poll = Some(PollCall {
                max: 2,
                messages: Vec::new(),
            });
        });
        let sink = sinks::net_sink(tokens::sink_token(image, 9, Sink::NetPoll), 1);
        let push = sink.push.unwrap_or_else(|| panic!("push"));
        let m = marshal::net_message_out(&NetMessage::EntityRemoved { key: NetKey(3) }, 1)
            .unwrap_or_else(|| panic!("out"));
        // SAFETY: a valid local message; the record is on this thread.
        unsafe {
            assert_eq!(push(sink.ctx, &raw const m), UMBRA_OK);
            assert_eq!(push(sink.ctx ^ 1, &raw const m), UMBRA_ERR_STATE);
            assert_eq!(push(sink.ctx, &raw const m), UMBRA_OK);
            assert_eq!(push(sink.ctx, &raw const m), UMBRA_ERR_INVALID_ARGUMENT);
        }
        let poll = tokens::with_top(|r| r.poll.take()).flatten();
        assert_eq!(
            poll.map(|p| p.messages),
            Some(vec![NetMessage::EntityRemoved { key: NetKey(3) }; 2])
        );
        assert!(
            active
                .finish()
                .breach
                .is_some_and(|b| b.contains("stale or foreign"))
        );
        // SAFETY: without a record the callback refuses before reading.
        assert_eq!(unsafe { push(sink.ctx, &raw const m) }, UMBRA_ERR_STATE);
    }

    #[test]
    fn the_message_sink_reads_with_the_records_minor_as_the_ceiling() {
        let image = tokens::make(301, tokens::HOST_CTX_INDEX);
        let active = call(image, 4, 0);
        tokens::with_top(|r| {
            r.poll = Some(PollCall {
                max: 8,
                messages: Vec::new(),
            });
        });
        let sink = sinks::net_sink(tokens::sink_token(image, 4, Sink::NetPoll), 1);
        let push = sink.push.unwrap_or_else(|| panic!("push"));
        let m = marshal::net_message_out(&NetMessage::EntityRemoved { key: NetKey(3) }, 1)
            .unwrap_or_else(|| panic!("out"));
        // SAFETY: a valid local message; the record is on this thread.
        let code = unsafe { push(sink.ctx, &raw const m) };
        assert_eq!(code, UMBRA_ERR_INVALID_ARGUMENT);
        assert!(
            active
                .finish()
                .breach
                .is_some_and(|b| b.contains("newer than the plugin's declared ABI"))
        );
    }
}
