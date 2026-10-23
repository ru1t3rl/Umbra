//! AT-1, AT-2, AT-4, AT-6 and the several-adapters rows of M6 WI-16: engines exchanging entities over
//! real DIS UDP on `127.0.0.1`, through the real `dis` plugin and the scenarios of
//! `examples/scenarios/dis_pair`. Requirements: M6 acceptance 1 and 2, REQ-N-3..6, REQ-K-5,
//! ASM-246, ASM-247, ASM-410, ASM-438..444.
//!
//! Each engine runs in real time on its own thread (8 MiB stack) and reports a [`Report`] after every
//! tick; the test thread waits (bounded wait-until, never a sleep as synchronisation) for the state
//! it expects, stops the engines, and then judges the recorded histories: first-tick convergence,
//! ghost positions against the owner's true position, no ghost of anything but the peers' entities,
//! no entity count that changes after convergence. Ports come from binding `127.0.0.1:0` (ASM-246).
//!
//! Opt-in capture (ASM-441): with `UMBRA_DIS_CAPTURE_DIR` set, the three unicast tests (v7, v6, v6 to v7) add a
//! listening socket to every sender's destinations and write each datagram it receives as one
//! `.hex` file (hex bytes, no annotations), for an independent decode with Open-DIS.

#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_methods,
    reason = "bounded wait-until loops on real sockets and real-time engines need the wall clock and short sleeps (REQ-K-4 governs simulation logic)"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]
#![expect(
    clippy::print_stderr,
    reason = "the counters of a run are printed for the milestone log (run with --nocapture)"
)]

mod net_support;
mod plugin_support;
mod port_support;

use port_support::{ports, retry_on_port_in_use};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::net::UdpSocket;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::json;
use umbra_core::net::{
    AdapterStart, NetMessage, NetworkAdapter, NetworkAdapterConfig, configure_input,
};
use umbra_core::{ComponentId, Ecef, Registry, SimTime, TickRate};
use umbra_engine::builtins::builtin_registry;
use umbra_engine::scenario::RunOptions;
use umbra_engine::{
    AdapterStatus, Ownership, StdWallClock, instantiate_with_plugins, load_scenario_with_plugins,
};
use umbra_net::LoopbackHub;
use umbra_plugin_host::{HostOptions, PluginHost};

use net_support::{distance, registry_with};

const WAIT: Duration = Duration::from_secs(30);
const TICK_RATE: u64 = 30;
/// One tick in seconds.
const DT: f64 = 1.0 / 30.0;
/// The default `position_threshold_m` of the publish policy (ASM-228).
const POSITION_THRESHOLD_M: f64 = 1.0;

// ------------------------------------------------------------------------------ scenarios

fn scratch(name: &str) -> PathBuf {
    let root = plugin_support::scratch_copy(&plugin_support::dis_pair_example_dir(), name);
    // The templates also describe the extra adapter names of the several-adapters tests; a table of
    // an adapter a scenario does not have is ignored.
    for (file, id, base, token, lb) in [
        (
            "tank_mbt_generic.toml",
            "tank.mbt_generic",
            "ground_vehicle.base",
            "1.1.225.1.1.0.0",
            "tank",
        ),
        (
            "truck_cargo.toml",
            "truck.cargo",
            "ground_vehicle.base",
            "1.1.225.4.1.0.0",
            "truck",
        ),
    ] {
        let mut text = std::fs::read_to_string(root.join("templates").join(file)).expect("read");
        write!(
            text,
            "\n[network.dis2]\nentity_type = \"{token}\"\n\n[network.lb]\nentity_type = \"{lb}\"\n"
        )
        .expect("write to a string");
        assert!(text.contains(id) && text.contains(base), "{file}: {text}");
        std::fs::write(root.join("templates").join(file), text).expect("write template");
    }
    root
}

/// One adapter block, publish and ghost settings as in the example (heartbeat 1 s, timeout 3 s).
fn adapter_block(adapter: &str, name: &str, params: &str) -> String {
    format!(
        "[[network_adapters]]\nadapter = \"{adapter}\"\nname = \"{name}\"\nparams = {params}\n\
         [network_adapters.publish]\nheartbeat_s = 1.0\n[network_adapters.ghosts]\ntimeout_s = 3.0\n\n"
    )
}

fn dis_block(name: &str, params: &str) -> String {
    adapter_block("umbra.net.dis", name, params)
}

/// `scenario.toml` for the node `dir` in `root`: the header and entities of the example side `side`
/// (`a` or `b`) with the entity names `<side>-` replaced by `<prefix>-`, and `adapters` as the
/// network section.
fn write_node(root: &Path, dir: &str, side: &str, prefix: &str, adapters: &str) -> PathBuf {
    let text = std::fs::read_to_string(root.join(side).join("scenario.toml")).expect("read");
    let (head, rest) = text
        .split_once("[[network_adapters]]")
        .expect("the example has a network section");
    let (_, entities) = rest
        .split_once("[[entities]]")
        .expect("the example has entities");
    let entities =
        format!("[[entities]]{entities}").replace(&format!("\"{side}-"), &format!("\"{prefix}-"));
    assert!(
        entities.contains(&format!("\"{prefix}-tank\"")),
        "{entities}"
    );
    let dir_path = root.join(dir);
    std::fs::create_dir_all(&dir_path).expect("node dir");
    let file = dir_path.join("scenario.toml");
    std::fs::write(&file, format!("{head}{adapters}{entities}")).expect("write scenario");
    file
}

fn unicast(version: u8, app: u16, bind: u16, peers: &[u16]) -> String {
    let destinations: Vec<String> = peers.iter().map(|p| format!("\"127.0.0.1:{p}\"")).collect();
    format!(
        "{{ site = 1, application = {app}, version = {version}, bind = \"127.0.0.1:{bind}\", \
         destinations = [{}] }}",
        destinations.join(", ")
    )
}

fn broadcast(app: u16, port: u16) -> String {
    format!(
        "{{ site = 1, application = {app}, mode = \"broadcast\", bind = \"0.0.0.0:{port}\", \
         broadcast_address = \"127.255.255.255:{port}\" }}"
    )
}

fn multicast(app: u16, group: &str, port: u16) -> String {
    format!(
        "{{ site = 1, application = {app}, mode = \"multicast\", group = \"{group}\", \
         interface = \"127.0.0.1\", bind = \"0.0.0.0:{port}\" }}"
    )
}

// ------------------------------------------------------------------------------ engine nodes

#[derive(Clone, Debug)]
struct Row {
    template: String,
    /// The adapter a ghost comes from.
    source: Option<String>,
    force: Option<String>,
    position: Ecef,
    speed: f64,
}

/// What one engine looked like after one tick.
#[derive(Clone, Debug)]
struct Report {
    tick: u64,
    owned: BTreeMap<String, Row>,
    ghosts: BTreeMap<String, Row>,
    /// Every entity of the kernel's snapshot; the maps above are keyed by name and would collapse
    /// two entities with one name, this count would not.
    entities: usize,
    status: Vec<AdapterStatus>,
}

impl Report {
    fn ghost_sources(&self) -> BTreeMap<String, String> {
        self.ghosts
            .iter()
            .map(|(n, r)| (n.clone(), r.source.clone().unwrap_or_default()))
            .collect()
    }

    fn adapter(&self, name: &str) -> &AdapterStatus {
        self.status
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no adapter {name}"))
    }
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<Report>>,
    error: Mutex<Option<String>>,
    done: AtomicBool,
}

/// Marks the node done however its thread ends.
struct DoneGuard(Arc<Shared>);

impl Drop for DoneGuard {
    fn drop(&mut self) {
        self.0.done.store(true, Ordering::SeqCst);
    }
}

/// Every engine of a test starts ticking only when all of them are instantiated (their sockets are
/// bound), so no first datagram goes to a closed port.
struct Gate {
    ready: AtomicUsize,
    nodes: usize,
}

struct Node {
    label: String,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Vec<Report>>>,
}

type MakeRegistry = Box<dyn FnOnce() -> Registry + Send>;

fn plain_registry() -> MakeRegistry {
    Box::new(|| builtin_registry().expect("the built-in registry builds"))
}

fn spawn_node(label: &str, scenario: PathBuf, registry: MakeRegistry, gate: &Arc<Gate>) -> Node {
    let shared = Arc::new(Shared::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (s, st, g) = (Arc::clone(&shared), Arc::clone(&stop), Arc::clone(gate));
    let join = std::thread::Builder::new()
        .name(label.to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let _done = DoneGuard(Arc::clone(&s));
            node_body(&scenario, &registry(), &s, &st, &g).unwrap_or_else(|e| {
                *s.error.lock().expect("lock") = Some(e);
                Vec::new()
            })
        })
        .expect("spawn the engine thread");
    Node {
        label: label.to_owned(),
        shared,
        stop,
        join: Some(join),
    }
}

fn node_body(
    scenario: &Path,
    registry: &Registry,
    shared: &Shared,
    stop: &AtomicBool,
    gate: &Gate,
) -> Result<Vec<Report>, String> {
    let plugins = plugin_support::dis();
    let loaded = load_scenario_with_plugins(registry, scenario, &[plugins])
        .map_err(|e| format!("loading {}: {e}", scenario.display()))?;
    let mut run = instantiate_with_plugins(
        &loaded,
        RunOptions {
            wall: Box::new(StdWallClock::new()),
            seed: None,
            clock: None, // the scenarios say realtime
        },
    )
    .map_err(|e| format!("instantiating {}: {e}", scenario.display()))?;
    let names: BTreeMap<_, _> = run
        .entities
        .iter()
        .map(|i| {
            (
                i.id,
                (i.name.clone().unwrap_or_default(), i.template.to_string()),
            )
        })
        .collect();

    gate.ready.fetch_add(1, Ordering::SeqCst);
    let begin = Instant::now();
    while gate.ready.load(Ordering::SeqCst) < gate.nodes {
        if begin.elapsed() > WAIT {
            return Err("the other engines did not come up".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let mut history = Vec::new();
    loop {
        run.engine
            .step(1, |_| ControlFlow::Continue(()))
            .map_err(|e| format!("tick: {e}"))?;
        let snapshot = run.engine.snapshot();
        let link = run.engine.network().ok_or("no network link")?;
        let infos: BTreeMap<_, _> = link.ghosts().map(|(id, g)| (id, g.clone())).collect();
        let mut report = Report {
            tick: run.engine.now().tick(),
            owned: BTreeMap::new(),
            ghosts: BTreeMap::new(),
            entities: snapshot.entities.len(),
            status: link.status(),
        };
        for e in &snapshot.entities {
            let row = |template: String, source: Option<String>| Row {
                template,
                source,
                force: e.force.as_ref().map(|f| f.as_str().to_owned()),
                position: e.state.position,
                speed: e.state.velocity.norm(),
            };
            match (&e.ownership, names.get(&e.id), infos.get(&e.id)) {
                (Ownership::Owned, Some((name, template)), _) => {
                    if report
                        .owned
                        .insert(name.clone(), row(template.clone(), None))
                        .is_some()
                    {
                        return Err(format!("two owned entities named {name}"));
                    }
                }
                (Ownership::Remote { source }, _, Some(info)) => {
                    let name = info.name.as_deref().unwrap_or("?").to_owned();
                    let ghost = row(info.template.to_string(), Some(source.as_str().to_owned()));
                    if report.ghosts.insert(name.clone(), ghost).is_some() {
                        return Err(format!("two ghosts named {name} (key churn?)"));
                    }
                }
                other => return Err(format!("an entity the test cannot place: {other:?}")),
            }
        }
        *shared.latest.lock().expect("lock") = Some(report.clone());
        history.push(report);
        if stop.load(Ordering::SeqCst) {
            return Ok(history);
        }
    }
}

impl Drop for Node {
    /// A test that panics must not leave its engines ticking.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Node {
    fn latest(&self) -> Option<Report> {
        self.shared.latest.lock().expect("lock").clone()
    }

    fn check(&self) {
        if self.shared.done.load(Ordering::SeqCst) {
            let error = self.shared.error.lock().expect("lock").clone();
            panic!("engine {} ended early: {error:?}", self.label);
        }
    }

    /// Stops the engine after the tick in progress and returns its history, one report per tick.
    fn finish(mut self) -> Vec<Report> {
        self.stop.store(true, Ordering::SeqCst);
        let history = self
            .join
            .take()
            .expect("joined once")
            .join()
            .unwrap_or_else(|_| panic!("engine {} panicked", self.label));
        let error = self.shared.error.lock().expect("lock").clone();
        assert!(error.is_none(), "engine {} failed: {error:?}", self.label);
        history
    }
}

/// Waits until `done` holds for the latest report of every node; a node that ended early or a
/// timeout fails the test.
fn wait_for(nodes: &[&Node], what: &str, mut done: impl FnMut(&[Report]) -> bool) {
    let begin = Instant::now();
    loop {
        for n in nodes {
            n.check();
        }
        let latest: Option<Vec<Report>> = nodes.iter().map(|n| n.latest()).collect();
        if let Some(reports) = latest
            && done(&reports)
        {
            return;
        }
        assert!(begin.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn has_ghosts(report: &Report, expected: &BTreeMap<String, String>) -> bool {
    &report.ghost_sources() == expected
}

fn sources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(n, s)| ((*n).to_owned(), (*s).to_owned()))
        .collect()
}

/// The first tick at which the ghost set is `expected`.
fn converged_at(history: &[Report], expected: &BTreeMap<String, String>, who: &str) -> u64 {
    history
        .iter()
        .find(|r| has_ghosts(r, expected))
        .unwrap_or_else(|| panic!("{who} never held the ghosts {expected:?}"))
        .tick
}

/// What every engine must satisfy at every tick, whoever its peers are:
/// - its owned entities are exactly `owned` (no remote entity ever appears among them);
/// - no ghost outside `allowed` ever exists (no crosstalk between adapters, no ghost of its own
///   entities);
/// - from convergence on, the ghosts are exactly `allowed` and the entity count never changes.
fn check_membership(
    who: &str,
    history: &[Report],
    owned: &[String],
    allowed: &BTreeMap<String, String>,
) -> u64 {
    let converged = converged_at(history, allowed, who);
    for r in history {
        assert_eq!(
            r.owned.keys().cloned().collect::<Vec<_>>(),
            owned,
            "{who} tick {}: owned set",
            r.tick
        );
        for (name, source) in r.ghost_sources() {
            assert_eq!(
                allowed.get(&name),
                Some(&source),
                "{who} tick {}: unexpected ghost {name} from {source}",
                r.tick
            );
        }
        // Counters are cumulative, so a per-tick check only pinpoints the first tick at which one
        // went wrong; it cannot tell a one-off from a stream.
        // A forwarded ghost would reach a peer as an entity without a type, which it counts apart.
        for status in &r.status {
            assert_eq!(
                (
                    status.unmapped,
                    status.world_errors,
                    status.counters.decode_errors
                ),
                (0, 0, 0),
                "{who} tick {}: {status:?}",
                r.tick
            );
        }
        if r.tick >= converged {
            assert_eq!(&r.ghost_sources(), allowed, "{who} tick {}", r.tick);
            assert_eq!(
                r.entities,
                owned.len() + allowed.len(),
                "{who} tick {}: entity count (a duplicated ghost?)",
                r.tick
            );
        }
    }
    converged
}

/// A ghost lies within `position_threshold + |v| * 3 ticks * dt` of its owner's true position at the
/// same tick, sampled every 10 ticks once converged (AT-1). Returns the number of samples.
fn check_tracking(
    who: &str,
    ghosts: &[Report],
    owner: &[Report],
    converged: u64,
    names: &[&str],
) -> usize {
    let by_tick: BTreeMap<u64, &Report> = owner.iter().map(|r| (r.tick, r)).collect();
    let mut samples = 0;
    for r in ghosts
        .iter()
        .filter(|r| r.tick >= converged && r.tick % 10 == 0)
    {
        let Some(truth) = by_tick.get(&r.tick) else {
            continue;
        };
        for name in names {
            let (ghost, own) = (&r.ghosts[*name], &truth.owned[*name]);
            let allowed = POSITION_THRESHOLD_M + own.speed * 3.0 * DT;
            let off = distance(ghost.position, own.position);
            assert!(
                off <= allowed,
                "{who} tick {}: ghost {name} is {off:.2} m from its owner (allowed {allowed:.2} m)",
                r.tick
            );
            samples += 1;
        }
    }
    samples
}

/// The counters every healthy exchange keeps at zero (ASM-444).
fn assert_clean(who: &str, s: &AdapterStatus) {
    assert_eq!(
        (
            s.unmapped,
            s.counters.tx_dropped,
            s.counters.rx_dropped,
            s.counters.decode_errors,
            s.invalid_state,
            s.world_errors
        ),
        (0, 0, 0, 0, 0, 0),
        "{who}: {s:?}"
    );
}

fn assert_all_clean(who: &str, report: &Report) {
    for s in &report.status {
        assert_clean(&format!("{who}/{}", s.name), s);
    }
}

fn print_counters(who: &str, report: &Report) {
    for s in &report.status {
        eprintln!(
            "counters {who}/{}: tick={} published={} received={} ghosts={} echo_dropped={} \
             unmapped={} tx_dropped={} rx_dropped={} decode_errors={} invalid_state={} world_errors={}",
            s.name,
            report.tick,
            s.published,
            s.received,
            s.ghosts,
            s.echo_dropped,
            s.unmapped,
            s.counters.tx_dropped,
            s.counters.rx_dropped,
            s.counters.decode_errors,
            s.invalid_state,
            s.world_errors,
        );
    }
}

// ------------------------------------------------------------------------------- capture

/// A listening socket that writes every datagram it receives as `<label>_<n>.hex` (ASM-441).
struct Capture {
    port: u16,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<usize>>,
}

impl Drop for Capture {
    /// A test that panics must not leave the capture thread running.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Capture {
    /// `None` unless `UMBRA_DIS_CAPTURE_DIR` is set.
    fn start(label: &str) -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("UMBRA_DIS_CAPTURE_DIR")?);
        std::fs::create_dir_all(&dir).expect("capture dir");
        let socket = UdpSocket::bind("127.0.0.1:0").expect("capture socket");
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout");
        let port = socket.local_addr().expect("address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, label) = (Arc::clone(&stop), label.to_owned());
        let join = std::thread::spawn(move || {
            let mut buf = [0_u8; 8192];
            let mut count = 0;
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((n, _)) => {
                        count += 1;
                        let mut hex = String::with_capacity(n * 2 + 1);
                        for byte in &buf[..n] {
                            write!(hex, "{byte:02x}").expect("write to a string");
                        }
                        hex.push('\n');
                        std::fs::write(dir.join(format!("{label}_{count:05}.hex")), hex)
                            .expect("write capture");
                    }
                    Err(_) if flag.load(Ordering::SeqCst) => return count,
                    Err(_) => {}
                }
            }
        });
        Some(Self {
            port,
            stop,
            join: Some(join),
        })
    }

    fn finish(mut self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.join
            .take()
            .expect("joined once")
            .join()
            .expect("capture thread")
    }
}

// ----------------------------------------------------------------------- AT-1, AT-2, AT-4

/// How many ticks past convergence a pair keeps running (3 s of steady state).
const STEADY_TICKS: u64 = 90;

/// A (site 1, app 1) and B (site 1, app 2) with the given adapter params; checks the whole of AT-1.
fn run_pair(label: &str, params_a: &str, params_b: &str, echoes: bool) {
    let root = scratch(label);
    let a_file = write_node(&root, "na", "a", "a", &dis_block("dis", params_a));
    let b_file = write_node(&root, "nb", "b", "b", &dis_block("dis", params_b));
    let gate = Arc::new(Gate {
        ready: AtomicUsize::new(0),
        nodes: 2,
    });
    let a = spawn_node("a", a_file, plain_registry(), &gate);
    let b = spawn_node("b", b_file, plain_registry(), &gate);

    let (a_ghosts, b_ghosts) = (
        sources(&[("b-tank", "dis"), ("b-truck", "dis")]),
        sources(&[("a-tank", "dis"), ("a-truck", "dis")]),
    );
    wait_for(
        &[&a, &b],
        "both engines to hold the other's entities",
        |r| has_ghosts(&r[0], &a_ghosts) && has_ghosts(&r[1], &b_ghosts),
    );
    let target = a.latest().map_or(0, |r| r.tick) + STEADY_TICKS;
    wait_for(&[&a, &b], "three seconds of steady state", |r| {
        r.iter().all(|r| r.tick >= target)
    });
    let (ha, hb) = (a.finish(), b.finish());

    let ca = check_membership("A", &ha, &names(&["a-tank", "a-truck"]), &a_ghosts);
    let cb = check_membership("B", &hb, &names(&["b-tank", "b-truck"]), &b_ghosts);
    // Within 3 s of simulated time each engine holds the other's entities.
    assert!(
        ca <= 3 * TICK_RATE && cb <= 3 * TICK_RATE,
        "converged at {ca}/{cb}"
    );
    assert!(check_tracking("A", &ha, &hb, ca, &["b-tank", "b-truck"]) > 0);
    assert!(check_tracking("B", &hb, &ha, cb, &["a-tank", "a-truck"]) > 0);

    // Mapped template, force and name (the key of the ghost map).
    for (who, history, peer) in [("A", &ha, "b"), ("B", &hb, "a")] {
        let last = history.last().expect("ticks ran");
        let force = if peer == "a" { "friendly" } else { "opposing" };
        for (kind, template) in [("tank", "tank.mbt_generic"), ("truck", "truck.cargo")] {
            let g = &last.ghosts[&format!("{peer}-{kind}")];
            assert_eq!(g.template, template, "{who}");
            assert_eq!(g.force.as_deref(), Some(force), "{who}");
        }
        let status = last.adapter("dis");
        assert!(
            status.received > 0 && status.published > 0,
            "{who}: {status:?}"
        );
        // Broadcast and multicast senders hear themselves; the manager drops that as an echo.
        assert_eq!(status.echo_dropped > 0, echoes, "{who}: {status:?}");
        assert_clean(who, status);
        print_counters(&format!("{label}/{who}"), last);
    }
}

/// `run_pair` with the opt-in capture destination added to both senders.
fn run_unicast_pair(label: &str, version_a: u8, version_b: u8) {
    retry_on_port_in_use(|| {
        let capture = Capture::start(label);
        let [pa, pb] = ports(2)[..] else {
            panic!("two ports")
        };
        let mut to_b = vec![pb];
        let mut to_a = vec![pa];
        if let Some(c) = &capture {
            to_b.push(c.port);
            to_a.push(c.port);
        }
        run_pair(
            label,
            &unicast(version_a, 1, pa, &to_b),
            &unicast(version_b, 2, pb, &to_a),
            false,
        );
        if let Some(c) = capture {
            let files = c.finish();
            assert!(files > 0, "the capture socket saw nothing");
            eprintln!("captured {files} datagrams for {label}");
        }
    });
}

#[test]
fn two_engines_exchange_entities_over_dis_v7_unicast() {
    run_unicast_pair("v7_unicast", 7, 7);
}

#[test]
fn two_engines_exchange_entities_over_dis_v6_unicast() {
    run_unicast_pair("v6_unicast", 6, 6);
}

#[test]
fn v6_sender_v7_receiver_interoperate() {
    run_unicast_pair("v6_to_v7", 6, 7);
}

#[test]
fn broadcast_on_loopback_exchanges_entities() {
    retry_on_port_in_use(|| {
        let [port] = ports(1)[..] else {
            panic!("one port")
        };
        run_pair("broadcast", &broadcast(1, port), &broadcast(2, port), true);
    });
}

#[test]
fn multicast_on_loopback_exchanges_entities() {
    retry_on_port_in_use(|| {
        let [port] = ports(1)[..] else {
            panic!("one port")
        };
        run_pair(
            "multicast",
            &multicast(1, "239.255.42.21", port),
            &multicast(2, "239.255.42.21", port),
            true,
        );
    });
}

// --------------------------------------------------------------------------------- AT-6

fn at(tick: u64) -> SimTime {
    SimTime::at_tick(TickRate::new(30).expect("30 Hz"), tick)
}

/// A third member of the multicast group, an adapter of the same plugin (site 9), that reports the
/// keys it hears, so the test sees exactly what the engine published.
struct Observer {
    _host: PluginHost,
    _config: Arc<dyn NetworkAdapterConfig>,
    adapter: Box<dyn NetworkAdapter>,
}

impl Observer {
    fn join(group: &str, port: u16) -> Self {
        let mut host = PluginHost::new(HostOptions::default());
        for package in PluginHost::discover(&[plugin_support::dis()]).expect("discover") {
            host.load(&package).expect("load the dis package");
        }
        let mut registry = Registry::new();
        host.register(&mut registry).expect("register");
        let factory = registry
            .network_adapter(&ComponentId::new("umbra.net.dis").expect("id"))
            .expect("the dis adapter");
        let params = json!({
            "site": 9, "application": 9, "mode": "multicast", "group": group,
            "interface": "127.0.0.1", "bind": format!("0.0.0.0:{port}"),
        });
        let config = {
            let _scope = host.build_scope();
            factory
                .build(&configure_input("observer", &params, []))
                .expect("the observer builds")
        };
        let adapter = config
            .start(&AdapterStart { now: at(0) })
            .expect("the observer starts");
        Self {
            _host: host,
            _config: config,
            adapter,
        }
    }

    /// The keys of the entity states heard since the last call.
    fn keys(&mut self) -> Vec<u64> {
        let mut got = Vec::new();
        self.adapter.poll(at(1), 256, &mut got).expect("poll");
        got.iter()
            .filter_map(|m| match m {
                NetMessage::EntityState(s) => Some(s.key.0),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.adapter.stop();
    }
}

#[test]
fn multicast_loop_back_is_dropped_as_echo_and_counts_stay_constant() {
    retry_on_port_in_use(|| {
        let [port] = ports(1)[..] else {
            panic!("one port")
        };
        let group = "239.255.42.22";
        let root = scratch("echo");
        let file = write_node(
            &root,
            "na",
            "a",
            "a",
            &dis_block("dis", &multicast(1, group, port)),
        );
        let mut observer = Observer::join(group, port);
        let gate = Arc::new(Gate {
            ready: AtomicUsize::new(0),
            nodes: 1,
        });
        let a = spawn_node("a", file, plain_registry(), &gate);

        // Ten seconds of simulated time: the engine's own packets come back every tick it publishes.
        let mut heard = Vec::new();
        wait_for(&[&a], "ten seconds of ticks", |r| {
            heard.extend(observer.keys());
            r[0].tick >= 10 * TICK_RATE
        });
        let history = a.finish();
        heard.extend(observer.keys());

        let base = 1_u64 << 32 | 1 << 16;
        assert!(!heard.is_empty(), "the observer heard no entity state");
        for key in &heard {
            assert!(
                *key > base && *key <= base + 65_534,
                "published key {key:#x} lies outside the own key space"
            );
        }
        let distinct: std::collections::BTreeSet<_> = heard.iter().collect();
        assert_eq!(distinct.len(), 2, "tank and truck only: {distinct:?}");

        for r in &history {
            assert_eq!(
                (r.owned.len(), r.ghosts.len(), r.entities),
                (2, 0, 2),
                "tick {}: an own entity came back as a ghost",
                r.tick
            );
        }
        let last = history.last().expect("ticks ran");
        let status = last.adapter("dis");
        assert!(status.echo_dropped > 0, "{status:?}");
        assert_eq!(status.ghosts, 0, "{status:?}");
        // Everything received was the engine's own traffic coming back.
        assert_eq!(status.received, status.echo_dropped, "{status:?}");
        assert_all_clean("echo/A", last);
        print_counters("echo/A", last);
        eprintln!("echo: observer heard {} entity states", heard.len());
    });
}

// ------------------------------------------------------------------- several adapters (ASM-410)

/// One engine runs DIS next to the loopback (ASM-410, a). `x` has `dis` (peer `y`) and `lb`
/// (peer `z`); `y` is a DIS-only peer, `z` a loopback-only peer. Everyone sees exactly what it should:
/// `x` both peers, `y` and `z` only `x`, and neither forwards the other's entities (no gateway).
#[test]
fn dis_next_to_the_loopback_in_one_scenario_has_no_crosstalk_and_no_gateway() {
    retry_on_port_in_use(|| {
        let [px, py] = ports(2)[..] else {
            panic!("two ports")
        };
        let root = scratch("dis_lb");
        let lb = adapter_block("test.net.loopback", "lb", "{}");
        let x_file = write_node(
            &root,
            "nx",
            "a",
            "x",
            &format!("{}{lb}", dis_block("dis", &unicast(7, 1, px, &[py]))),
        );
        let y_file = write_node(
            &root,
            "ny",
            "b",
            "y",
            &dis_block("dis", &unicast(7, 2, py, &[px])),
        );
        let z_file = write_node(&root, "nz", "b", "z", &lb);

        let hub = LoopbackHub::new(4096);
        let registry_of = |base: u64| -> MakeRegistry {
            let hub = Arc::clone(&hub);
            Box::new(move || registry_with(&[("test.net.loopback", &hub, base)]))
        };
        let gate = Arc::new(Gate {
            ready: AtomicUsize::new(0),
            nodes: 3,
        });
        let x = spawn_node("x", x_file, registry_of(1000), &gate);
        let y = spawn_node("y", y_file, plain_registry(), &gate);
        let z = spawn_node("z", z_file, registry_of(2000), &gate);

        let x_ghosts = sources(&[
            ("y-tank", "dis"),
            ("y-truck", "dis"),
            ("z-tank", "lb"),
            ("z-truck", "lb"),
        ]);
        let from_x = sources(&[("x-tank", "dis"), ("x-truck", "dis")]);
        let from_x_lb = sources(&[("x-tank", "lb"), ("x-truck", "lb")]);
        wait_for(&[&x, &y, &z], "every engine to hold its peers", |r| {
            has_ghosts(&r[0], &x_ghosts)
                && has_ghosts(&r[1], &from_x)
                && has_ghosts(&r[2], &from_x_lb)
        });
        let target = x.latest().map_or(0, |r| r.tick) + STEADY_TICKS;
        wait_for(&[&x, &y, &z], "three seconds of steady state", |r| {
            r.iter().all(|r| r.tick >= target)
        });
        let (hx, hy, hz) = (x.finish(), y.finish(), z.finish());

        check_membership("X", &hx, &names(&["x-tank", "x-truck"]), &x_ghosts);
        check_membership("Y", &hy, &names(&["y-tank", "y-truck"]), &from_x);
        check_membership("Z", &hz, &names(&["z-tank", "z-truck"]), &from_x_lb);

        // Counters are per adapter: Y heard only X over DIS, Z only X over the loopback, and X's
        // adapters each hold exactly their own peer's ghosts.
        let (lx, ly, lz) = (
            hx.last().expect("x"),
            hy.last().expect("y"),
            hz.last().expect("z"),
        );
        assert_eq!(lx.adapter("dis").ghosts, 2);
        assert_eq!(lx.adapter("lb").ghosts, 2);
        assert_eq!(ly.adapter("dis").ghosts, 2);
        assert_eq!(lz.adapter("lb").ghosts, 2);
        assert_eq!(ly.adapter("dis").counters.decode_errors, 0);
        for (who, l) in [("x", lx), ("y", ly), ("z", lz)] {
            assert_all_clean(who, l);
            print_counters(&format!("dis_lb/{who}"), l);
        }
    });
}

/// One engine runs two DIS adapters (ASM-410, b): `dis` (unicast, peer `y`) and `dis2` (multicast,
/// peer `w`). `y` and `w` each see only `x`; `x` forwards nothing between them.
#[test]
fn two_dis_adapters_in_one_scenario_see_only_their_own_peers() {
    retry_on_port_in_use(|| {
        let [px, py, pm] = ports(3)[..] else {
            panic!("three ports")
        };
        let group = "239.255.42.23";
        let root = scratch("two_dis");
        let x_file = write_node(
            &root,
            "nx",
            "a",
            "x",
            &format!(
                "{}{}",
                dis_block("dis", &unicast(7, 1, px, &[py])),
                dis_block("dis2", &multicast(3, group, pm))
            ),
        );
        let y_file = write_node(
            &root,
            "ny",
            "b",
            "y",
            &dis_block("dis", &unicast(7, 2, py, &[px])),
        );
        let w_file = write_node(
            &root,
            "nw",
            "b",
            "w",
            &dis_block("dis2", &multicast(4, group, pm)),
        );
        let gate = Arc::new(Gate {
            ready: AtomicUsize::new(0),
            nodes: 3,
        });
        let x = spawn_node("x", x_file, plain_registry(), &gate);
        let y = spawn_node("y", y_file, plain_registry(), &gate);
        let w = spawn_node("w", w_file, plain_registry(), &gate);

        let x_ghosts = sources(&[
            ("y-tank", "dis"),
            ("y-truck", "dis"),
            ("w-tank", "dis2"),
            ("w-truck", "dis2"),
        ]);
        let from_x = sources(&[("x-tank", "dis"), ("x-truck", "dis")]);
        let from_x_dis2 = sources(&[("x-tank", "dis2"), ("x-truck", "dis2")]);
        wait_for(&[&x, &y, &w], "every engine to hold its peers", |r| {
            has_ghosts(&r[0], &x_ghosts)
                && has_ghosts(&r[1], &from_x)
                && has_ghosts(&r[2], &from_x_dis2)
        });
        let target = x.latest().map_or(0, |r| r.tick) + STEADY_TICKS;
        wait_for(&[&x, &y, &w], "three seconds of steady state", |r| {
            r.iter().all(|r| r.tick >= target)
        });
        let (hx, hy, hw) = (x.finish(), y.finish(), w.finish());

        check_membership("X", &hx, &names(&["x-tank", "x-truck"]), &x_ghosts);
        check_membership("Y", &hy, &names(&["y-tank", "y-truck"]), &from_x);
        check_membership("W", &hw, &names(&["w-tank", "w-truck"]), &from_x_dis2);

        let (lx, ly, lw) = (
            hx.last().expect("x"),
            hy.last().expect("y"),
            hw.last().expect("w"),
        );
        assert_eq!(lx.adapter("dis").ghosts, 2);
        assert_eq!(lx.adapter("dis2").ghosts, 2);
        assert!(
            lx.adapter("dis2").echo_dropped > 0,
            "multicast comes back to X"
        );
        assert_eq!(ly.adapter("dis").ghosts, 2);
        assert_eq!(lw.adapter("dis2").ghosts, 2);
        for (who, l) in [("x", lx), ("y", ly), ("w", lw)] {
            assert_all_clean(who, l);
            print_counters(&format!("two_dis/{who}"), l);
        }
    });
}
