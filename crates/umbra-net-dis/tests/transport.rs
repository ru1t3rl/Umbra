//! The UDP transport and the adapter over it (M6 WI-15; REQ-N-1, REQ-N-6, ADR 0033, ADR 0034, NFR-2,
//! ASM-246, ASM-247, ASM-410, ASM-427..): bounded queues that drop and count on both sides, `stop`
//! joining both threads within a second, oversized datagrams, the error contract of `publish`, garbage
//! on the socket, unicast exchange between adapter instances in one process and several instances at
//! once. Ports are never fixed and never reserved-then-released (ASM-1200): an instance binds port 0
//! and its peers are given the address it reports; only where two instances need each other's port
//! before either is bound does [`with_free_ports`] retry a bounded number of times. Waits are bounded
//! wait-until loops, not sleeps.

#![forbid(unsafe_code)]
#![expect(
    clippy::disallowed_methods,
    reason = "bounded wait-until loops on real sockets need the wall clock and short sleeps (REQ-K-4 governs simulation logic)"
)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use umbra_net_dis::adapter::{DisAdapter, DisConfig};
use umbra_net_dis::codec::EntityId;
use umbra_net_dis::config::{Config, Mode};
use umbra_net_dis::translate::{Inbound, Stamp, pack_key};
use umbra_net_dis::transport::{Closed, MAX_DATAGRAM, Transport};
use umbra_sdk::serde_json::{Value, json};
use umbra_sdk::{
    ControlAction, DeadReckoning, EntityState, Euler, NetKey, NetMessage, NetworkAdapter as _,
};

const AT: Stamp = Stamp {
    tick: 0,
    rate_hz: 30,
};
const TIMEOUT: Duration = Duration::from_secs(10);
/// Datagrams of the send-queue burst.
const TOTAL: u64 = 50_000;

/// Polls `f` until it returns `Some`, at most [`TIMEOUT`].
fn wait_until<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(start.elapsed() < TIMEOUT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Attempts of [`with_free_ports`].
const PORT_ATTEMPTS: usize = 5;

/// Runs `attempt` with `n` distinct loopback UDP ports for instances that need each other's port
/// before either is bound (a cycle port 0 cannot break). The ports are released before `attempt`
/// binds them, so another process can take one in between (ASM-1200); only that failure, "Address
/// already in use", is retried, at most [`PORT_ATTEMPTS`] times with fresh ports.
fn with_free_ports<T>(n: usize, mut attempt: impl FnMut(&[u16]) -> Result<T, String>) -> T {
    for _ in 1..PORT_ATTEMPTS {
        match attempt(&ports(n)) {
            Ok(v) => return v,
            Err(e) if e.contains("Address already in use") => {}
            Err(e) => panic!("{e}"),
        }
    }
    attempt(&ports(n)).expect("the adapters start")
}

/// `n` distinct free loopback UDP ports, held until every one is chosen, then released.
fn ports(n: usize) -> Vec<u16> {
    let sockets: Vec<UdpSocket> = (0..n)
        .map(|_| UdpSocket::bind("127.0.0.1:0").expect("bind an ephemeral port"))
        .collect();
    sockets
        .iter()
        .map(|s| s.local_addr().expect("local address").port())
        .collect()
}

fn lo(port: u16) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)
}

fn config(extra: &Value) -> Config {
    let mut params = json!({"site": 1, "application": 1});
    if let (Some(base), Some(add)) = (params.as_object_mut(), extra.as_object()) {
        base.extend(add.clone());
    }
    Config::parse("dis", &params, &BTreeMap::new()).expect("valid params")
}

fn start(c: Config) -> DisAdapter {
    DisConfig::new(c)
        .expect("config")
        .start_adapter()
        .expect("the adapter starts")
}

/// The port the adapter's socket is bound to.
fn port_of(a: &DisAdapter) -> u16 {
    a.local_addr().port()
}

/// An adapter on `bind_port` (0: the OS picks, read it with [`port_of`]) sending to `peers`, site 1,
/// application `app`.
fn adapter(app: u16, bind_port: u16, peers: &[u16], extra: &Value) -> DisAdapter {
    try_adapter(app, bind_port, peers, extra).expect("the adapter starts")
}

/// [`adapter`] reporting a start failure as its message.
fn try_adapter(
    app: u16,
    bind_port: u16,
    peers: &[u16],
    extra: &Value,
) -> Result<DisAdapter, String> {
    let mut e = json!({
        "application": app,
        "bind": format!("127.0.0.1:{bind_port}"),
        "destinations": peers.iter().map(|p| format!("127.0.0.1:{p}")).collect::<Vec<_>>(),
    });
    if let (Some(base), Some(add)) = (e.as_object_mut(), extra.as_object()) {
        base.extend(add.clone());
    }
    DisConfig::new(config(&e))
        .expect("config")
        .start_adapter()
        .map_err(|e| e.to_string())
}

fn key(app: u16, entity: u16) -> NetKey {
    pack_key(EntityId {
        site: 1,
        application: app,
        entity,
    })
}

fn state(key: NetKey, x: f64) -> EntityState<'static> {
    EntityState {
        key,
        entity_type: Some("1.1.225.1.1.0.0"),
        force: Some("friendly"),
        name: Some("unit"),
        position: [x, 2.0, 3.0],
        velocity: [1.0, 0.0, 0.0],
        acceleration: [0.0; 3],
        angular_velocity: [0.0; 3],
        orientation: Euler::default(),
        dead_reckoning: DeadReckoning::Velocity,
    }
}

/// Polls `a` until `n` messages arrived (or fails), returning them in arrival order.
fn collect(a: &mut DisAdapter, n: usize) -> Vec<Inbound> {
    let mut got = Vec::new();
    wait_until(&format!("{n} messages"), || {
        a.poll_into(n - got.len(), &mut got).expect("poll");
        (got.len() >= n).then_some(())
    });
    got
}

fn peer_socket() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").expect("peer socket");
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .expect("timeout");
    s
}

// ----------------------------------------------------------------------------- stop and joins

#[test]
fn stop_joins_both_threads_within_one_second_and_releases_the_socket() {
    let mut t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![],
        },
        8,
        8,
    )
    .expect("start");
    let port = t.local_addr().port();
    assert_ne!(port, 0, "the OS chose a port");
    assert_eq!(t.local_addr(), SocketAddr::V4(lo(port)));
    assert!(!t.is_stopped());
    // While running, the port is taken.
    assert!(
        UdpSocket::bind(lo(port)).is_err(),
        "the transport holds the port"
    );

    let begin = Instant::now();
    t.stop();
    let took = begin.elapsed();
    assert!(took < Duration::from_secs(1), "stop took {took:?}");
    assert!(t.is_stopped());
    // Both threads own a clone of the socket: only when both ended is the port free again.
    UdpSocket::bind(lo(port)).expect("both threads ended and released the socket");
    t.stop(); // idempotent
}

#[test]
fn after_stop_nothing_can_be_sent_or_received_and_the_counters_stay_readable() {
    let mut t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![],
        },
        8,
        8,
    )
    .expect("start");
    t.send(vec![1, 2, 3]).expect("running");
    t.stop();
    assert_eq!(t.send(vec![1]), Err(Closed));
    assert_eq!(t.receive(), Err(Closed));
    assert_eq!(t.counters().tx_dropped, 0);
}

#[test]
fn dropping_a_running_transport_stops_it() {
    let t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![],
        },
        8,
        8,
    )
    .expect("start");
    let port = t.local_addr().port();
    drop(t);
    UdpSocket::bind(lo(port)).expect("the drop joined the threads");
}

#[test]
fn stop_sends_what_is_queued_first() {
    let peer = UdpSocket::bind(lo(0)).expect("peer");
    let theirs = peer.local_addr().expect("address").port();
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![lo(theirs)],
        },
        64,
        8,
    )
    .expect("start");
    for i in 0..20_u8 {
        t.send(vec![i; 10]).expect("queued");
    }
    t.stop();
    let mut buf = [0_u8; 64];
    for i in 0..20_u8 {
        let (n, _) = peer
            .recv_from(&mut buf)
            .expect("queued datagrams are sent before the stop completes");
        assert_eq!(buf[..n], [i; 10]);
    }
}

#[test]
fn a_port_in_use_is_a_start_error_naming_the_address() {
    let first = adapter(1, 0, &[], &json!({}));
    let port = port_of(&first);
    let mut second =
        DisConfig::new(config(&json!({"bind": format!("127.0.0.1:{port}")}))).expect("config");
    let e = second.start_adapter().expect_err("the port is taken");
    let m = e.to_string();
    assert!(
        m.contains(&format!("127.0.0.1:{port}")) && m.contains("bind"),
        "{m}"
    );
    assert!(m.contains("`dis`"), "names the adapter: {m}");
}

#[test]
fn start_adapter_runs_once_per_configuration() {
    let mut c = DisConfig::new(config(&json!({"bind": "127.0.0.1:0"}))).expect("config");
    let first = c.start_adapter().expect("first start");
    let again = c.start_adapter().expect_err("second start");
    assert!(again.to_string().contains("already started"));
    drop(first);
}

// -------------------------------------------------------------------------------- queues

#[test]
fn a_full_receive_queue_drops_the_new_datagrams_and_counts_them() {
    let t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![],
        },
        8,
        2,
    )
    .expect("start");
    let port = t.local_addr().port();
    let raw = peer_socket();
    for i in 0..10_u8 {
        raw.send_to(&[i; 20], lo(port)).expect("send");
    }
    wait_until("all ten datagrams seen", || {
        let c = t.counters();
        (c.received + c.rx_dropped == 10).then_some(())
    });
    let c = t.counters();
    assert_eq!((c.received, c.rx_dropped), (2, 8));
    assert_eq!(c.oversized_rx, 0);
    // The queue keeps the first two, in order; nothing else is left.
    assert_eq!(t.receive(), Ok(Some(vec![0; 20])));
    assert_eq!(t.receive(), Ok(Some(vec![1; 20])));
    assert_eq!(t.receive(), Ok(None));
}

#[test]
fn a_full_send_queue_drops_the_new_datagrams_and_counts_them() {
    let peer = UdpSocket::bind(lo(0)).expect("peer");
    let theirs = peer.local_addr().expect("address").port();
    let t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![lo(theirs)],
        },
        1,
        8,
    )
    .expect("start");
    for _ in 0..TOTAL {
        t.send(vec![0; 16])
            .expect("an overflow is a drop, not an error");
    }
    // Every datagram was either sent (to the one target) or dropped at the queue.
    wait_until("the queue to drain", || {
        let c = t.counters();
        (c.sent + c.tx_dropped == TOTAL).then_some(())
    });
    let c = t.counters();
    assert!(
        c.tx_dropped > 0,
        "a one-slot queue cannot take a burst: {c:?}"
    );
    assert!(c.sent >= 1);
    assert_eq!(c.io_errors, 0);
    drop(peer);
}

#[test]
fn oversized_datagrams_are_dropped_and_counted_on_both_sides() {
    let peer = UdpSocket::bind(lo(0)).expect("peer");
    let theirs = peer.local_addr().expect("address").port();
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let t = Transport::start(
        lo(0),
        &Mode::Unicast {
            destinations: vec![lo(theirs)],
        },
        8,
        8,
    )
    .expect("start");
    let mine = t.local_addr().port();

    // Inbound: one byte over the limit is dropped, exactly the limit is kept.
    let raw = peer_socket();
    raw.send_to(&vec![7; MAX_DATAGRAM + 1], lo(mine))
        .expect("send");
    raw.send_to(&vec![8; MAX_DATAGRAM], lo(mine)).expect("send");
    wait_until("both datagrams seen", || {
        let c = t.counters();
        (c.received + c.rx_dropped == 2).then_some(())
    });
    let c = t.counters();
    assert_eq!((c.oversized_rx, c.rx_dropped, c.received), (1, 1, 1));
    assert_eq!(
        t.receive().expect("open").map(|d| (d.len(), d[0])),
        Some((MAX_DATAGRAM, 8))
    );

    // Outbound: the same limit; the oversized one never reaches the peer.
    t.send(vec![1; MAX_DATAGRAM + 1])
        .expect("a drop is not an error");
    t.send(vec![2; MAX_DATAGRAM]).expect("fits");
    let mut buf = vec![0_u8; MAX_DATAGRAM + 16];
    let (n, _) = peer
        .recv_from(&mut buf)
        .expect("the fitting datagram arrives");
    assert_eq!(
        (n, buf[0]),
        (MAX_DATAGRAM, 2),
        "the first datagram to arrive is the fitting one"
    );
    let c = t.counters();
    assert_eq!((c.oversized_tx, c.tx_dropped), (1, 1));
}

// -------------------------------------------------------------------------- error contract

#[test]
fn a_failed_send_is_counted_and_publish_still_returns_ok() {
    let mut peer = adapter(2, 0, &[], &json!({}));
    let theirs = port_of(&peer);
    // Destination port 0 is refused by `sendto`; the config check forbids it, so the mode is set
    // directly. The good destination after it must keep working.
    let mut c = config(&json!({"bind": "127.0.0.1:0"}));
    c.mode = Mode::Unicast {
        destinations: vec![lo(0), lo(theirs)],
    };
    let mut a = start(c);
    let msgs = [
        NetMessage::EntityState(state(key(1, 1), 10.0)),
        NetMessage::EntityState(state(key(1, 2), 20.0)),
        NetMessage::SimulationControl(ControlAction::Start),
    ];
    assert!(
        a.publish_at(AT, &msgs).is_ok(),
        "a failed send is not an error"
    );
    let got = collect(&mut peer, 3);
    assert_eq!(
        got.len(),
        3,
        "the good destination still got every datagram"
    );
    wait_until("the failures to be counted", || {
        (a.stats().transport.io_errors == 3).then_some(())
    });
    let counters = a.abi_counters();
    assert_eq!((counters.io_errors, counters.tx_dropped), (3, 3));
    // The adapter stays enabled: more publishes work and are counted again.
    assert!(a.publish_at(AT, &msgs[..1]).is_ok());
    collect(&mut peer, 1);
    wait_until("the fourth failure", || {
        (a.abi_counters().io_errors == 4).then_some(())
    });
}

#[test]
fn publish_and_poll_fail_after_stop_and_a_message_that_cannot_be_sent_is_a_counted_drop() {
    let mut a = adapter(1, 0, &[], &json!({}));
    // A key outside the adapter's key space is not sent; it is counted, not an error.
    let foreign = NetMessage::EntityState(state(key(9, 1), 0.0));
    assert!(a.publish_at(AT, &[foreign]).is_ok());
    assert_eq!(a.abi_counters().tx_dropped, 1);
    assert_eq!(a.stats().rejected_out, 1);

    a.stop();
    a.stop();
    assert!(a.publish_at(AT, &[]).is_err(), "nothing more can be done");
    let mut into = Vec::new();
    assert!(a.poll_into(4, &mut into).is_err());
    assert_eq!(into.len(), 0, "{into:?}");
    assert_eq!(a.abi_counters().tx_dropped, 1, "counters stay readable");
}

// ------------------------------------------------------------------------- garbage and limits

#[test]
fn garbage_on_the_socket_is_counted_and_never_a_message() {
    let mut b = adapter(1, 0, &[], &json!({}));
    let pb = port_of(&b);
    let raw = peer_socket();
    let to_b = lo(pb);

    // Valid datagrams of an application 2 (exercise 1) and of an application 3 in exercise 9.
    let datagram = |extra: &Value, app: u16| -> Vec<u8> {
        let mut t = umbra_net_dis::translate::Translator::new(&config(extra));
        t.outbound(AT, &NetMessage::EntityState(state(key(app, 1), 5.0)))
            .expect("sendable")
    };
    let good = datagram(&json!({"application": 2}), 2);
    let wrong_exercise = datagram(&json!({"application": 3, "exercise": 9}), 3);
    let with = |index: usize, byte: u8| {
        let mut d = good.clone();
        d[index] = byte;
        d
    };
    let mut extended = good.clone();
    extended.push(0);

    // Nine datagrams that cannot be messages, then a valid one from the same socket, so everything
    // before it has been looked at when it arrives (one socket, one order).
    let junk = [
        vec![],                // malformed: empty
        vec![7],               // malformed: shorter than a header
        good[..60].to_vec(),   // malformed: truncated
        extended,              // malformed: longer than its header says
        with(0, 5),            // other protocol version
        vec![0; MAX_DATAGRAM], // other protocol version (0), the largest datagram
        with(2, 4),            // unknown PDU type 4
        with(2, 4),            // unknown PDU type 4
        with(2, 27),           // unknown PDU type 27
    ];
    for d in junk {
        raw.send_to(&d, to_b).expect("send junk");
    }
    raw.send_to(&wrong_exercise, to_b).expect("send");
    raw.send_to(&good, to_b).expect("send");

    let got = collect(&mut b, 1);
    let Inbound::State(s) = &got[0] else {
        panic!("a state, got {:?}", got[0])
    };
    assert_eq!(s.key, key(2, 1));
    let stats = b.stats();
    assert_eq!(
        stats.decode.decoded, 2,
        "the other exercise decodes, then is skipped"
    );
    assert_eq!(stats.decode.malformed, 4);
    assert_eq!(stats.decode.unsupported_version, 2);
    assert_eq!(
        stats.decode.skipped_by_type,
        BTreeMap::from([(4, 2), (27, 1)])
    );
    assert_eq!(stats.skips.exercise, 1);
    assert_eq!(stats.skips.total(), 1);
    assert_eq!(stats.delivered, 1);
    let counters = b.abi_counters();
    assert_eq!(counters.decode_errors, 4 + 2 + 3 + 1);
    assert_eq!(
        (counters.rx_dropped, counters.tx_dropped, counters.io_errors),
        (0, 0, 0)
    );
}

// ----------------------------------------------------------------------- unicast exchange

#[test]
fn two_instances_exchange_unicast_on_loopback() {
    // Each instance sends to the other's port: a cycle, so the ports are chosen first.
    let (mut a, mut b) = with_free_ports(2, |p| {
        let (pa, pb) = (p[0], p[1]);
        let a = try_adapter(1, pa, &[pb], &json!({"version": 7}))?;
        let b = try_adapter(2, pb, &[pa], &json!({"version": 6}))?;
        Ok((a, b))
    });

    a.publish_at(AT, &[NetMessage::EntityState(state(key(1, 1), 10.0))])
        .expect("a publishes");
    b.publish_at(AT, &[NetMessage::EntityState(state(key(2, 7), 20.0))])
        .expect("b publishes");

    let at_b = collect(&mut b, 1);
    let Inbound::State(from_a) = &at_b[0] else {
        panic!("a state")
    };
    assert_eq!(from_a.key, key(1, 1));
    assert_eq!(from_a.position, [10.0, 2.0, 3.0]);
    assert_eq!(from_a.entity_type.as_deref(), Some("1.1.225.1.1.0.0"));
    assert_eq!(from_a.force.as_deref(), Some("friendly"));
    assert_eq!(from_a.name.as_deref(), Some("unit"));
    assert_eq!(from_a.dead_reckoning, DeadReckoning::Velocity);

    let at_a = collect(&mut a, 1);
    let Inbound::State(from_b) = &at_a[0] else {
        panic!("a state")
    };
    assert_eq!(from_b.key, key(2, 7));
    assert_eq!(from_b.position, [20.0, 2.0, 3.0]);

    // Removal and events cross as well.
    a.publish_at(
        AT,
        &[
            NetMessage::EntityRemoved { key: key(1, 1) },
            NetMessage::SimulationControl(ControlAction::Pause),
        ],
    )
    .expect("publish");
    let more = collect(&mut b, 2);
    assert_eq!(more[0], Inbound::Removed(key(1, 1)));
    assert_eq!(more[1], Inbound::Control(ControlAction::Pause));
}

#[test]
fn several_instances_in_one_process_are_independent() {
    // A sends to B only; C listens and sends to nobody.
    let mut b = adapter(2, 0, &[], &json!({}));
    let mut a = adapter(1, 0, &[port_of(&b)], &json!({}));
    let mut c = adapter(3, 0, &[], &json!({}));
    assert_eq!(
        [a.local_addr(), b.local_addr(), c.local_addr()]
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3,
        "three sockets"
    );

    // Two firing instances send to their own capture socket: event ids and request ids are per
    // instance, so both count from 1 with their own site and application.
    let weapons = json!({"weapons": {"acme.rocket": "2.2.225.3.0.0.0"}});
    let (cap_a, cap_b) = (peer_socket(), peer_socket());
    let socket_port = |s: &UdpSocket| s.local_addr().expect("address").port();
    let mut fa = adapter(11, 0, &[socket_port(&cap_a)], &weapons);
    let mut fb = adapter(12, 0, &[socket_port(&cap_b)], &weapons);
    let fire = |app: u16| NetMessage::Fire {
        shooter: key(app, 1),
        target: None,
        weapon: "acme.rocket",
        position: [0.0; 3],
    };
    fa.publish_at(AT, &[fire(11), fire(11), fire(11)])
        .expect("publish");
    fb.publish_at(AT, &[fire(12)]).expect("publish");
    let events = |cap: &UdpSocket, n: usize| -> Vec<(u16, u16, u16)> {
        let mut buf = vec![0_u8; MAX_DATAGRAM];
        (0..n)
            .map(|_| {
                let (len, _) = cap.recv_from(&mut buf).expect("a Fire datagram");
                let (_, pdu) = umbra_net_dis::codec::decode(&buf[..len]).expect("decodes");
                let umbra_net_dis::codec::Body::Fire(f) = pdu.body else {
                    panic!("a Fire")
                };
                (f.event.site, f.event.application, f.event.event)
            })
            .collect()
    };
    assert_eq!(events(&cap_a, 3), [(1, 11, 1), (1, 11, 2), (1, 11, 3)]);
    assert_eq!(events(&cap_b, 1), [(1, 12, 1)]);

    for i in 0..3_u16 {
        a.publish_at(
            AT,
            &[NetMessage::EntityState(state(key(1, 1 + i), f64::from(i)))],
        )
        .expect("publish");
    }
    let got = collect(&mut b, 3);
    assert_eq!(
        got.iter()
            .map(|m| match m {
                Inbound::State(s) => s.key,
                other => panic!("a state, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        [key(1, 1), key(1, 2), key(1, 3)],
        "in publish order"
    );

    // C heard nothing and counted nothing; A received nothing; every counter is the instance's own.
    let mut nothing = Vec::new();
    c.poll_into(8, &mut nothing).expect("poll");
    a.poll_into(8, &mut nothing).expect("poll");
    assert_eq!(nothing.len(), 0, "{nothing:?}");
    assert_eq!(c.stats().transport.received, 0);
    assert_eq!(c.abi_counters(), umbra_sdk::Counters::default());
    assert_eq!(a.stats().delivered, 0);
    assert_eq!(b.stats().delivered, 3);
    assert_eq!(a.stats().transport.sent, 3);
    assert_eq!(b.stats().transport.sent, 0);
    assert_eq!(fa.stats().transport.sent, 3);
    assert_eq!(fb.stats().transport.sent, 1);
}

#[test]
fn poll_hands_over_at_most_max_and_keeps_the_rest_queued() {
    let mut b = adapter(2, 0, &[], &json!({}));
    let mut a = adapter(1, 0, &[port_of(&b)], &json!({}));
    for i in 0..5_u16 {
        a.publish_at(AT, &[NetMessage::EntityState(state(key(1, 1 + i), 0.0))])
            .expect("publish");
    }
    wait_until("five datagrams queued", || {
        (b.stats().transport.received == 5).then_some(())
    });
    let mut got = Vec::new();
    b.poll_into(0, &mut got).expect("poll");
    assert_eq!(got.len(), 0, "{got:?}");
    b.poll_into(2, &mut got).expect("poll");
    assert_eq!(got.len(), 2);
    b.poll_into(2, &mut got).expect("poll");
    assert_eq!(got.len(), 4);
    b.poll_into(10, &mut got).expect("poll");
    assert_eq!(got.len(), 5, "the fifth was still queued");
    let keys: Vec<NetKey> = got
        .iter()
        .filter_map(|m| {
            if let Inbound::State(s) = m {
                Some(s.key)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        keys,
        (1..=5).map(|e| key(1, e)).collect::<Vec<_>>(),
        "arrival order"
    );
}

// ---------------------------------------------------------------- broadcast and multicast

/// Sends one state from `sender` and expects `receiver` to get it.
fn expect_exchange(sender: &mut DisAdapter, receiver: &mut DisAdapter, from: NetKey) {
    sender
        .publish_at(AT, &[NetMessage::EntityState(state(from, 1.0))])
        .expect("publish");
    let got = collect(receiver, 1);
    let Inbound::State(s) = &got[0] else {
        panic!("a state")
    };
    assert_eq!(s.key, from);
}

#[test]
fn multicast_on_loopback_reaches_every_member_including_the_sender() {
    let params = |app: u16, port: u16| {
        json!({
            "application": app, "mode": "multicast", "group": "239.255.42.17",
            "interface": "127.0.0.1", "bind": format!("0.0.0.0:{port}"),
        })
    };
    // The first member takes a port; the second joins it (shared ports use SO_REUSEADDR).
    let mut a = start(config(&params(1, 0)));
    let mut b = start(config(&params(2, port_of(&a))));
    expect_exchange(&mut a, &mut b, key(1, 1));
    // The sender hears its own packet (multicast loop): the manager drops it as an echo (REQ-K-5).
    let own = collect(&mut a, 1);
    let Inbound::State(s) = &own[0] else {
        panic!("a state")
    };
    assert_eq!(s.key, key(1, 1));
    expect_exchange(&mut b, &mut a, key(2, 1));
}

#[test]
fn broadcast_on_loopback_reaches_the_other_instances() {
    // The broadcast address carries the shared port, which is only known once bound: a cycle.
    let (mut a, mut b) = with_free_ports(1, |p| {
        let port = p[0];
        let params = |app: u16| {
            json!({
                "application": app, "mode": "broadcast",
                "broadcast_address": format!("127.255.255.255:{port}"),
                "bind": format!("0.0.0.0:{port}"),
            })
        };
        let start_one = |app: u16| {
            DisConfig::new(config(&params(app)))
                .expect("config")
                .start_adapter()
                .map_err(|e| e.to_string())
        };
        Ok((start_one(1)?, start_one(2)?))
    });
    expect_exchange(&mut a, &mut b, key(1, 1));
    // A broadcast also comes back to its sender: the manager drops it as an echo (REQ-K-5).
    let own = collect(&mut a, 1);
    assert!(matches!(&own[0], Inbound::State(s) if s.key == key(1, 1)));
    expect_exchange(&mut b, &mut a, key(2, 1));
}

#[test]
fn two_groups_on_one_port_do_not_hear_each_other() {
    // Linux would deliver a group's packets to every socket on the port (IP_MULTICAST_ALL) unless
    // the transport restricts each socket to the groups it joined.
    let params = |app: u16, group: &str, port: u16| {
        json!({
            "application": app, "mode": "multicast", "group": group,
            "interface": "127.0.0.1", "bind": format!("0.0.0.0:{port}"),
        })
    };
    let mut a = start(config(&params(1, "239.255.42.30", 0)));
    let mut b = start(config(&params(2, "239.255.42.31", port_of(&a))));
    let first = |got: &[Inbound]| match &got[0] {
        Inbound::State(s) => s.key,
        other => panic!("a state, got {other:?}"),
    };

    // A speaks and hears itself; then B speaks and hears itself. If A's packet had leaked into B, it
    // would be queued in front of B's own (same socket, same order), and the other way round.
    a.publish_at(AT, &[NetMessage::EntityState(state(key(1, 1), 1.0))])
        .expect("publish");
    assert_eq!(first(&collect(&mut a, 1)), key(1, 1));
    b.publish_at(AT, &[NetMessage::EntityState(state(key(2, 1), 2.0))])
        .expect("publish");
    assert_eq!(
        first(&collect(&mut b, 1)),
        key(2, 1),
        "nothing of group .30 reached .31"
    );
    a.publish_at(AT, &[NetMessage::EntityState(state(key(1, 2), 3.0))])
        .expect("publish");
    assert_eq!(
        first(&collect(&mut a, 1)),
        key(1, 2),
        "nothing of group .31 reached .30"
    );
    assert_eq!(a.stats().transport.received, 2);
    assert_eq!(b.stats().transport.received, 1);
}

#[test]
fn the_adapter_works_through_the_sdk_traits_without_a_host() {
    // `SimTime::new` and `ConfigureInput::from_parts` are public, so `configure`, `start` and `publish`
    // run here exactly as the host calls them (`poll` needs a host sink).
    let now = umbra_sdk::SimTime::new(30, 30);
    // The adapter sends to its own port, which is only known once bound: a cycle.
    let mut adapter = with_free_ports(1, |p| {
        let port = p[0];
        let input = umbra_sdk::ConfigureInput::from_parts(
            "dis",
            json!({"site": 1, "application": 1, "bind": format!("127.0.0.1:{port}"), "destinations": [format!("127.0.0.1:{port}")]}),
            BTreeMap::new(),
        );
        let mut cfg =
            <umbra_net_dis::DisFactory as umbra_sdk::NetworkAdapterFactory>::configure(&input)
                .expect("configure");
        assert_eq!(
            umbra_sdk::NetworkAdapterConfig::key_space(&cfg).base(),
            1 << 32 | 1 << 16
        );
        umbra_sdk::NetworkAdapterConfig::start(&mut cfg, now).map_err(|e| e.to_string())
    });
    umbra_sdk::NetworkAdapter::publish(
        &mut adapter,
        now,
        &[NetMessage::EntityState(state(key(1, 1), 1.0))],
    )
    .expect("publish");
    wait_until("the datagram to loop back", || {
        (adapter.stats().transport.received == 1).then_some(())
    });
    assert_eq!(
        umbra_sdk::NetworkAdapter::counters(&adapter),
        umbra_sdk::Counters::default()
    );
    umbra_sdk::NetworkAdapter::stop(&mut adapter);
}
