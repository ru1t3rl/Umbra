//! The DIS adapter through the real plugin (M6 WI-15; REQ-N-1, REQ-N-6, REQ-SDK-3, ADR 0033, ADR 0034,
//! ASM-410, ASM-427..): the library built from `crates/umbra-net-dis` and its shipped `plugin.toml` are
//! loaded by `umbra-plugin-host`, and the core adapter contract suite (AT-21) runs against it, fed
//! over UDP. Also: one plugin image configured twice under two scenario names exchanging unicast on
//! `127.0.0.1`, DIS next to the loopback in one process without any crosstalk, the security warning in
//! the host log, and a bad template token as a build error naming the template.
//!
//! The kernel and the engine never depend on `umbra-net-dis` (ADR 0030): the library is built by a
//! nested `cargo build`, like every plugin of the M4 tests. Ports come from binding `127.0.0.1:0`
//! (ASM-246).

#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_methods,
    reason = "bounded wait-until loops on real sockets need the wall clock and short sleeps (REQ-K-4 governs simulation logic)"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod plugin_support;

use std::cell::RefCell;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use umbra_core::dead_reckoning::DeadReckoning;
use umbra_core::net::{
    AdapterCounters, AdapterStart, KeySpace, NetEntityState, NetEntityType, NetError, NetKey,
    NetMessage, NetworkAdapter, NetworkAdapterConfig, configure_input,
};
use umbra_core::{
    BuildError, ComponentId, ControlAction, Ecef, Force, ParamValue, PluginId, Radians, Registry,
    SimTime, TemplateId, TickRate, Vec3, WorldEuler,
};
use umbra_net::LoopbackHub;
use umbra_plugin_host::{HostOptions, LogLevel, PluginHost, PluginLog};

const CAPABILITY: &str = "umbra.net.dis";
const TIMEOUT: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------------------- the plugin

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A plugin directory holding package `dis`: the built library and the manifest the crate ships.
fn package_dir() -> PathBuf {
    let lib = plugin_support::library("umbra-net-dis", &[]);
    let dir = plugin_support::plugins_dir(&[("dis", &lib)]);
    std::fs::copy(
        workspace_root().join("crates/umbra-net-dis/plugin.toml"),
        dir.join("dis/plugin.toml"),
    )
    .expect("copy the shipped plugin.toml");
    dir
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(LogLevel, String)>>);

impl PluginLog for Recorder {
    fn log(&self, _: &PluginId, level: LogLevel, message: &str) {
        self.0
            .lock()
            .expect("log lock")
            .push((level, message.to_owned()));
    }
}

impl Recorder {
    fn lines(&self) -> Vec<(LogLevel, String)> {
        self.0.lock().expect("log lock").clone()
    }
}

/// A host with the DIS package loaded and registered.
struct Env {
    host: PluginHost,
    registry: Registry,
    log: Arc<Recorder>,
}

fn load() -> Env {
    let log = Arc::new(Recorder::default());
    let mut options = HostOptions::default();
    options.log = log.clone();
    let mut host = PluginHost::new(options);
    for package in PluginHost::discover(&[package_dir()]).expect("discover the dis package") {
        host.load(&package).expect("load the dis package");
    }
    let mut registry = Registry::new();
    host.register(&mut registry)
        .expect("register the dis package");
    Env {
        host,
        registry,
        log,
    }
}

impl Env {
    fn try_build(
        &self,
        name: &str,
        params: &Value,
        templates: &[(&str, Value)],
    ) -> Result<Arc<dyn NetworkAdapterConfig>, BuildError> {
        let id = ComponentId::new(CAPABILITY).expect("valid id");
        let factory = self
            .registry
            .network_adapter(&id)
            .expect("the dis adapter is registered");
        let ids: Vec<TemplateId> = templates
            .iter()
            .map(|(id, _)| TemplateId::new(id).expect("template id"))
            .collect();
        let tables: Vec<(&TemplateId, &ParamValue)> =
            ids.iter().zip(templates.iter().map(|(_, t)| t)).collect();
        let _scope = self.host.build_scope();
        factory.build(&configure_input(name, params, tables))
    }

    fn build(&self, name: &str, params: &Value) -> Arc<dyn NetworkAdapterConfig> {
        self.try_build(name, params, &[])
            .expect("the dis adapter builds")
    }
}

fn at(tick: u64) -> SimTime {
    SimTime::at_tick(TickRate::new(30).expect("30 Hz"), tick)
}

fn start(config: &dyn NetworkAdapterConfig) -> Box<dyn NetworkAdapter> {
    config.start(&AdapterStart { now: at(0) }).expect("start")
}

/// `n` distinct free loopback ports, held until all are chosen (ASM-246).
fn ports(n: usize) -> Vec<u16> {
    let sockets: Vec<UdpSocket> = (0..n)
        .map(|_| UdpSocket::bind("127.0.0.1:0").expect("ephemeral port"))
        .collect();
    sockets
        .iter()
        .map(|s| s.local_addr().expect("address").port())
        .collect()
}

fn wait_until<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let begin = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(begin.elapsed() < TIMEOUT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Polls until `n` messages arrived; returns them in arrival order.
fn collect(adapter: &mut dyn NetworkAdapter, n: usize) -> Vec<NetMessage> {
    let mut got = Vec::new();
    wait_until(&format!("{n} messages"), || {
        adapter.poll(at(1), n - got.len(), &mut got).expect("poll");
        (got.len() >= n).then_some(())
    });
    got
}

fn params(app: u16, bind_port: u16, peers: &[u16]) -> Value {
    json!({
        "site": 1,
        "application": app,
        "bind": format!("127.0.0.1:{bind_port}"),
        "destinations": peers.iter().map(|p| format!("127.0.0.1:{p}")).collect::<Vec<_>>(),
    })
}

fn tank(key: NetKey) -> NetMessage {
    NetMessage::EntityState(NetEntityState {
        key,
        entity_type: Some(NetEntityType::new("1.1.225.1.1.0.0")),
        force: Some(Force::new("friendly").expect("force")),
        name: Some(Arc::from("Alpha-1")),
        position: Ecef(Vec3::new(3_900_000.0, 400_000.0, 5_000_000.0)),
        velocity: Vec3::new(10.0, -5.0, 2.0),
        acceleration: Vec3::new(0.0, 0.0, 0.0),
        angular_velocity: Vec3::new(0.0, 0.0, 0.0),
        orientation: WorldEuler {
            yaw: Radians(0.5),
            pitch: Radians(-0.25),
            roll: Radians(0.125),
        },
        dead_reckoning: DeadReckoning::Velocity,
    })
}

// ------------------------------------------------------------------------------- loading

#[test]
fn the_package_loads_at_abi_0_1_and_registers_the_dis_adapter() {
    let env = load();
    let id = ComponentId::new(CAPABILITY).expect("valid id");
    let factory = env.registry.network_adapter(&id).expect("registered");
    let schema = factory.param_schema().as_json();
    assert_eq!(schema["required"], json!(["site", "application"]));
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(env.host.faults().len(), 0);
}

#[test]
fn the_key_space_is_the_entities_of_the_configured_site_and_application() {
    let env = load();
    let config = env.build(
        "dis",
        &json!({"site": 7, "application": 9, "bind": "127.0.0.1:0"}),
    );
    assert_eq!(
        config.key_space(),
        KeySpace {
            base: 7 << 32 | 9 << 16,
            count: 65_534
        }
    );
}

#[test]
fn bad_params_and_template_tables_are_build_errors_naming_the_cause() {
    let env = load();
    let e = env
        .try_build("dis", &json!({"application": 4}), &[])
        .err()
        .expect("site is required");
    assert!(e.to_string().contains("`site`"), "{e}");
    let ok = json!({"site": 1, "application": 4, "bind": "127.0.0.1:0"});
    let e = env
        .try_build(
            "dis",
            &ok,
            &[
                ("blue.tank", json!({"entity_type": "1.1.225.1.1.0.0"})),
                ("red.truck", json!({"entity_type": "1.1.225"})),
            ],
        )
        .err()
        .expect("the token is bad");
    let m = e.to_string();
    assert!(
        m.contains("red.truck") && m.contains("[network.dis]"),
        "{m}"
    );
    assert!(!m.contains("blue.tank"), "{m}");
    // A valid table builds, and building opens nothing: the port stays free.
    let [port] = ports(1)[..] else {
        panic!("one port")
    };
    let bound = json!({"site": 1, "application": 4, "bind": format!("127.0.0.1:{port}")});
    let _config = env
        .try_build(
            "dis",
            &bound,
            &[("blue.tank", json!({"entity_type": "01.1.225.1.1.0.0"}))],
        )
        .expect("a valid table builds");
    UdpSocket::bind(("127.0.0.1", port)).expect("a configuration holds no socket until it starts");
}

// ---------------------------------------------------------------------------- the warning

#[test]
fn the_security_warning_is_logged_by_start_exactly_for_wide_exposure() {
    let env = load();
    let warnings = |env: &Env| -> Vec<String> {
        env.log
            .lines()
            .into_iter()
            .filter(|(level, m)| *level == LogLevel::Warn && m.contains("unauthenticated"))
            .map(|(_, m)| m)
            .collect()
    };
    // Building alone (a dry run) logs nothing, whatever the exposure.
    let wide = env.build(
        "wide",
        &json!({"site": 1, "application": 1, "bind": "0.0.0.0:0"}),
    );
    assert!(
        warnings(&env).is_empty(),
        "configure must not warn: {:?}",
        warnings(&env)
    );

    // A loopback unicast adapter starts silently.
    let local = env.build(
        "local",
        &json!({"site": 1, "application": 2, "bind": "127.0.0.1:0"}),
    );
    let mut a = start(local.as_ref());
    assert_eq!(warnings(&env).len(), 0);
    a.stop();

    // A non-loopback bind warns once, at start, naming the adapter.
    let mut b = start(wide.as_ref());
    let lines = warnings(&env);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("accepts unauthenticated DIS traffic from the network"));
    assert!(lines[0].contains("`wide`"), "{}", lines[0]);
    b.stop();

    // Multicast warns too (it cannot bind to loopback, so it always listens wider).
    let multicast = env.build(
        "mcast",
        &json!({
            "site": 1, "application": 3, "mode": "multicast", "group": "239.255.42.18",
            "interface": "127.0.0.1", "bind": "0.0.0.0:0",
        }),
    );
    let mut c = start(multicast.as_ref());
    assert_eq!(warnings(&env).len(), 2);
    c.stop();
}

// ------------------------------------------------------- one image, two names, over UDP

#[test]
fn one_image_configured_twice_exchanges_unicast_on_loopback() {
    let env = load();
    let [pa, pb] = ports(2)[..] else {
        panic!("two ports")
    };
    let ca = env.build("a", &params(1, pa, &[pb]));
    let cb = env.build("b", &params(2, pb, &[pa]));
    assert_ne!(
        ca.key_space(),
        cb.key_space(),
        "distinct application, distinct keys"
    );
    let (ka, kb) = (ca.key_space(), cb.key_space());
    let mut a = start(ca.as_ref());
    let mut b = start(cb.as_ref());

    let from_a = NetKey(ka.base + 1);
    let from_b = NetKey(kb.base + 7);
    a.publish(at(1), &[tank(from_a)]).expect("a publishes");
    b.publish(at(1), &[tank(from_b)]).expect("b publishes");

    let got = collect(b.as_mut(), 1);
    let NetMessage::EntityState(s) = &got[0] else {
        panic!("a state, got {:?}", got[0])
    };
    assert_eq!(s.key, from_a);
    assert_eq!(
        s.entity_type.as_ref().map(NetEntityType::as_str),
        Some("1.1.225.1.1.0.0")
    );
    assert_eq!(s.force.as_ref().map(Force::as_str), Some("friendly"));
    assert_eq!(s.name.as_deref(), Some("Alpha-1"));
    assert_eq!(
        s.position,
        Ecef(Vec3::new(3_900_000.0, 400_000.0, 5_000_000.0))
    );
    assert_eq!(s.velocity, Vec3::new(10.0, -5.0, 2.0));
    assert_eq!(s.dead_reckoning, DeadReckoning::Velocity);
    assert_eq!(s.orientation.yaw, Radians(0.5));

    let got = collect(a.as_mut(), 1);
    let NetMessage::EntityState(s) = &got[0] else {
        panic!("a state, got {:?}", got[0])
    };
    assert_eq!(s.key, from_b);

    // Removal and run control cross as well, in order.
    a.publish(
        at(2),
        &[
            NetMessage::EntityRemoved { key: from_a },
            NetMessage::SimulationControl(ControlAction::Pause),
        ],
    )
    .expect("publish");
    let got = collect(b.as_mut(), 2);
    assert_eq!(got[0], NetMessage::EntityRemoved { key: from_a });
    assert_eq!(got[1], NetMessage::SimulationControl(ControlAction::Pause));

    // Nothing was dropped or malformed on either side (the host refreshes counters after a poll).
    assert_eq!(a.counters(), AdapterCounters::default());
    assert_eq!(b.counters(), AdapterCounters::default());
    a.stop();
    b.stop();
}

// ------------------------------------------------------------------- DIS next to loopback

#[test]
fn dis_and_the_loopback_run_side_by_side_without_crosstalk() {
    let env = load();
    let [pa, pb] = ports(2)[..] else {
        panic!("two ports")
    };
    let ca = env.build("dis_a", &params(1, pa, &[pb]));
    let cb = env.build("dis_b", &params(2, pb, &[pa]));

    // Two loopback endpoints of one hub, registered next to the plugin in the same registry.
    let hub = LoopbackHub::new(64);
    let lb_space = KeySpace {
        base: 50_000,
        count: 100,
    };
    let mut registry = Registry::new();
    registry
        .register_network_adapter(
            hub.factory(ComponentId::new("test.net.loopback").expect("id"), lb_space),
        )
        .expect("register the loopback");
    let lb_factory = registry
        .network_adapter(&ComponentId::new("test.net.loopback").expect("id"))
        .expect("registered");
    let lb_config = |name: &str| {
        lb_factory
            .build(&configure_input(name, &json!({}), []))
            .expect("loopback builds")
    };
    let (lb1, lb2) = (lb_config("lb1"), lb_config("lb2"));

    let mut dis_a = start(ca.as_ref());
    let mut dis_b = start(cb.as_ref());
    let mut lb_a = start(lb1.as_ref());
    let mut lb_b = start(lb2.as_ref());

    // A publishes one entity on the DIS side and another on the loopback side; B hears only DIS.
    let dis_key = NetKey(ca.key_space().base + 1);
    let lb_key = NetKey(lb_space.base + 1);
    dis_a.publish(at(1), &[tank(dis_key)]).expect("dis publish");
    lb_a.publish(at(1), &[tank(lb_key)])
        .expect("loopback publish");

    let over_dis = collect(dis_b.as_mut(), 1);
    assert!(matches!(&over_dis[0], NetMessage::EntityState(s) if s.key == dis_key));
    let over_loopback = collect(lb_b.as_mut(), 1);
    assert!(matches!(&over_loopback[0], NetMessage::EntityState(s) if s.key == lb_key));

    // Nothing crossed: the loopback peer never hears the DIS entity, DIS never hears the loopback one.
    let mut extra = Vec::new();
    lb_b.poll(at(2), 8, &mut extra).expect("poll");
    dis_b.poll(at(2), 8, &mut extra).expect("poll");
    dis_a.poll(at(2), 8, &mut extra).expect("poll");
    lb_a.poll(at(2), 8, &mut extra).expect("poll");
    assert!(extra.is_empty(), "no message may cross adapters: {extra:?}");

    // Counters are per adapter: the DIS side saw one datagram each way, the loopback its own.
    assert_eq!(dis_a.counters(), AdapterCounters::default());
    assert_eq!(dis_b.counters(), AdapterCounters::default());
    assert_eq!(lb_a.counters(), AdapterCounters::default());
    assert_eq!(lb_b.counters(), AdapterCounters::default());
    assert_eq!(hub.starts(), 2);
    for a in [&mut dis_a, &mut dis_b, &mut lb_a, &mut lb_b] {
        a.stop();
    }
}

// --------------------------------------------------------------------------- the contract

/// What [`feed`] needs, kept per test thread by [`contract_config`].
struct Feeder {
    /// The port of the adapter under test.
    port: u16,
    /// Receives a copy of everything the peer sends.
    observer: UdpSocket,
    /// A second adapter (application 2) that sends to the adapter under test and to the observer.
    peer: Box<dyn NetworkAdapter>,
    /// Keeps the peer's configuration alive.
    _peer_config: Arc<dyn NetworkAdapterConfig>,
}

thread_local! {
    /// Set by [`contract_config`] in the test thread, read by [`feed`].
    static FEEDER: RefCell<Option<Feeder>> = const { RefCell::new(None) };
}

/// A fresh configuration for one generated contract test: bound to a reserved port and sending to
/// nobody. A peer adapter is started next to it to feed it (an adapter drops its own run control
/// when it comes back, so it cannot feed itself with the contract's control messages).
fn contract_config() -> Arc<dyn NetworkAdapterConfig> {
    let env = load();
    let observer = UdpSocket::bind("127.0.0.1:0").expect("observer");
    observer
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let observer_port = observer.local_addr().expect("address").port();
    let [port, peer_port] = ports(2)[..] else {
        panic!("two ports")
    };
    let peer_config = env.build("peer", &params(2, peer_port, &[port, observer_port]));
    let peer = start(peer_config.as_ref());
    FEEDER.with(|f| {
        *f.borrow_mut() = Some(Feeder {
            port,
            observer,
            peer,
            _peer_config: peer_config,
        });
    });
    // The host is dropped here; the configurations keep the image alive.
    env.build("dis", &params(1, port, &[]))
}

/// Delivers `messages` to the adapter's `poll` over UDP and returns once they are queued:
/// 1. the peer publishes them (it sends each datagram to the adapter first, then to the observer);
/// 2. wait until the observer has all of them, so every send to the adapter's port is done;
/// 3. send one oversized datagram to the adapter from a raw socket and wait until its `rx_dropped`
///    shows it (polling zero messages refreshes the host's counters). The receive thread handles
///    datagrams in order, so the earlier ones are queued by then.
fn feed(adapter: &mut dyn NetworkAdapter, messages: &[NetMessage]) {
    let port = FEEDER.with(|f| {
        let mut slot = f.borrow_mut();
        let feeder = slot.as_mut().expect("contract_config ran in this thread");
        feeder.peer.publish(at(0), messages).expect("feed: publish");
        let mut buf = [0_u8; 512];
        for _ in messages {
            feeder
                .observer
                .recv_from(&mut buf)
                .expect("feed: the observer got the datagram");
        }
        feeder.port
    });
    let raw = UdpSocket::bind("127.0.0.1:0").expect("raw socket");
    raw.send_to(&vec![0_u8; 9000], ("127.0.0.1", port))
        .expect("sentinel");
    wait_until("the sentinel", || {
        adapter
            .poll(at(0), 0, &mut Vec::new())
            .expect("feed: poll 0");
        (adapter.counters().rx_dropped == 1).then_some(())
    });
}

mod contract {
    umbra_core::network_adapter_contract_tests!(super::contract_config(), feed = super::feed);
}

#[test]
fn start_errors_are_net_errors_naming_the_adapter() {
    let env = load();
    let [port] = ports(1)[..] else {
        panic!("one port")
    };
    let first = env.build("first", &params(1, port, &[]));
    let second = env.build("second", &params(2, port, &[]));
    let mut running = start(first.as_ref());
    let e = second
        .start(&AdapterStart { now: at(0) })
        .err()
        .expect("the port is taken");
    let NetError::Start(m) = &e else {
        panic!("a start error, got {e:?}")
    };
    assert!(m.contains(&format!("127.0.0.1:{port}")), "{m}");
    running.stop();
}
