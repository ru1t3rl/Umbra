//! The DIS adapter's params (M6 WI-15; REQ-N-6, ADR 0034, NFR-4, ASM-427..): defaults, every key,
//! mode-dependent rules, id grammars, the `[network.<name>]` tables of the templates (a bad
//! `entity_type` is a configure error naming the template) and the security warning.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddrV4};

use umbra_net_dis::config::{
    Config, DEFAULT_BIND, DEFAULT_QUEUE, DEFAULT_TTL, DEFAULT_WIDE_BIND, Mode, PARAM_SCHEMA, Queues,
};
use umbra_net_dis::translate::format_entity_type;
use umbra_net_dis::version::DisVersion;
use umbra_sdk::serde_json::{Value, json};

fn parse(params: &Value) -> Result<Config, umbra_sdk::Error> {
    Config::parse("dis", params, &BTreeMap::new())
}

fn base() -> Value {
    json!({"site": 3, "application": 4})
}

fn with(extra: &Value) -> Value {
    let mut v = base();
    if let (Some(b), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        b.extend(e.clone());
    }
    v
}

fn refused(params: &Value) -> String {
    parse(params)
        .expect_err("the params must be refused")
        .to_string()
}

fn addr(s: &str) -> SocketAddrV4 {
    s.parse().expect("socket address")
}

#[test]
fn defaults_are_unicast_on_loopback_version_7_exercise_1() {
    let c = parse(&base()).expect("site and application are enough");
    assert_eq!((c.site, c.application), (3, 4));
    assert_eq!(c.version, DisVersion::V7);
    assert_eq!(c.exercise, 1);
    assert_eq!(c.bind, DEFAULT_BIND);
    assert_eq!(c.bind, addr("127.0.0.1:3000"));
    assert_eq!(
        c.mode,
        Mode::Unicast {
            destinations: vec![]
        },
        "receive only"
    );
    assert_eq!(
        c.queues,
        Queues {
            send: DEFAULT_QUEUE,
            receive: DEFAULT_QUEUE
        }
    );
    assert_eq!(DEFAULT_QUEUE, 4096);
    assert_eq!(
        c.forces,
        BTreeMap::from([
            ("friendly".to_owned(), 1),
            ("neutral".to_owned(), 3),
            ("opposing".to_owned(), 2)
        ])
    );
    assert!(c.weapons.is_empty());
    assert_eq!(c.name, "dis");
}

#[test]
fn every_key_is_read() {
    let c = parse(&with(&json!({
        "version": 6,
        "exercise": 77,
        "mode": "unicast",
        "bind": "127.0.0.1:4100",
        "destinations": ["127.0.0.1:4200", "127.0.0.2:4300"],
        "forces": {"blue": 1, "red": 2},
        "weapons": {"acme.cannon_120": "2.1.225.1.1.0.0"},
        "queue": {"send": 8, "receive": 16},
    })))
    .expect("valid");
    assert_eq!(c.version, DisVersion::V6);
    assert_eq!(c.exercise, 77);
    assert_eq!(c.bind, addr("127.0.0.1:4100"));
    assert_eq!(
        c.mode,
        Mode::Unicast {
            destinations: vec![addr("127.0.0.1:4200"), addr("127.0.0.2:4300")]
        }
    );
    assert_eq!(
        c.forces,
        BTreeMap::from([("blue".to_owned(), 1), ("red".to_owned(), 2)])
    );
    assert_eq!(
        c.weapons
            .get("acme.cannon_120")
            .map(|t| format_entity_type(*t))
            .as_deref(),
        Some("2.1.225.1.1.0.0")
    );
    assert_eq!(
        c.queues,
        Queues {
            send: 8,
            receive: 16
        }
    );
}

#[test]
fn broadcast_and_multicast_have_their_own_keys_and_a_wide_default_bind() {
    let b = parse(&with(
        &json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:3000"}),
    ))
    .expect("broadcast");
    assert_eq!(
        b.mode,
        Mode::Broadcast {
            address: addr("127.255.255.255:3000")
        }
    );
    assert_eq!(b.bind, DEFAULT_WIDE_BIND);
    let m = parse(&with(&json!({"mode": "multicast", "group": "239.255.1.2"}))).expect("multicast");
    assert_eq!(
        m.mode,
        Mode::Multicast {
            group: Ipv4Addr::new(239, 255, 1, 2),
            interface: Ipv4Addr::UNSPECIFIED,
            ttl: DEFAULT_TTL
        }
    );
    assert_eq!(m.bind, DEFAULT_WIDE_BIND);
    let m = parse(&with(&json!({
        "mode": "multicast", "group": "239.255.1.2", "interface": "127.0.0.1", "ttl": 4, "bind": "0.0.0.0:3100"
    })))
    .expect("multicast");
    assert_eq!(
        m.mode,
        Mode::Multicast {
            group: Ipv4Addr::new(239, 255, 1, 2),
            interface: Ipv4Addr::LOCALHOST,
            ttl: 4
        }
    );
    assert_eq!(m.bind, addr("0.0.0.0:3100"), "an explicit bind wins");
}

#[test]
fn site_and_application_are_required_and_in_range() {
    assert!(refused(&json!({"application": 4})).contains("`site`"));
    assert!(refused(&json!({"site": 3})).contains("`application`"));
    for bad in [0, 65_535, 70_000] {
        assert!(
            refused(&json!({"site": bad, "application": 4})).contains("`site`"),
            "{bad}"
        );
        assert!(
            refused(&json!({"site": 3, "application": bad})).contains("`application`"),
            "{bad}"
        );
    }
    assert!(refused(&json!({"site": "3", "application": 4})).contains("`site`"));
    assert!(parse(&json!({"site": 1, "application": 65_534})).is_ok());
    assert!(refused(&json!([])).contains("params"));
}

#[test]
fn scalar_keys_are_checked() {
    assert!(refused(&with(&json!({"version": 5}))).contains("`version`"));
    assert!(refused(&with(&json!({"version": "7"}))).contains("`version`"));
    assert!(refused(&with(&json!({"exercise": 256}))).contains("`exercise`"));
    assert!(refused(&with(&json!({"exercise": -1}))).contains("`exercise`"));
    assert!(parse(&with(&json!({"exercise": 0}))).is_ok());
    assert!(refused(&with(&json!({"mode": "anycast"}))).contains("anycast"));
    assert!(refused(&with(&json!({"queue": {"send": 0}}))).contains("queue.send"));
    assert!(refused(&with(&json!({"queue": {"receive": 1_000_001}}))).contains("queue.receive"));
    assert!(refused(&with(&json!({"queue": {"depth": 4}}))).contains("queue.depth"));
    assert!(refused(&with(&json!({"queue": 4}))).contains("`queue`"));
    assert!(parse(&with(&json!({"queue": {"send": 1, "receive": 1}}))).is_ok());
}

#[test]
fn an_unknown_key_names_itself_and_the_known_ones() {
    let m = refused(&with(&json!({"bnid": "127.0.0.1:1"})));
    assert!(m.contains("`bnid`") && m.contains("bind"), "{m}");
}

#[test]
fn addresses_must_be_ipv4_with_a_port() {
    for bad in [
        "127.0.0.1",
        "localhost:3000",
        "[::1]:3000",
        "127.0.0.1:99999",
        "",
    ] {
        let m = refused(&with(&json!({"bind": bad})));
        assert!(m.contains("`bind`") && m.contains(bad), "{bad}: {m}");
    }
    assert!(refused(&with(&json!({"bind": "[::1]:3000"}))).contains("IPv4 only"));
    assert!(refused(&with(&json!({"bind": 3000}))).contains("`bind`"));
    let m = refused(&with(&json!({"destinations": ["127.0.0.1:4200", "nope"]})));
    assert!(m.contains("`destinations`") && m.contains("nope"), "{m}");
    assert!(refused(&with(&json!({"destinations": ["127.0.0.1:0"]}))).contains("port 0"));
    assert!(refused(&with(&json!({"destinations": "127.0.0.1:1"}))).contains("must be a list"));
    assert!(
        parse(&with(&json!({"bind": "127.0.0.1:0"}))).is_ok(),
        "port 0 binds an ephemeral port"
    );
}

#[test]
fn keys_of_another_mode_are_refused_not_ignored() {
    let m = refused(&with(&json!({"group": "239.1.1.1"})));
    assert!(
        m.contains("`group`") && m.contains("multicast") && m.contains("unicast"),
        "{m}"
    );
    assert!(refused(&with(&json!({"ttl": 1}))).contains("`ttl`"));
    assert!(refused(&with(&json!({"interface": "127.0.0.1"}))).contains("`interface`"));
    assert!(
        refused(&with(&json!({"broadcast_address": "127.255.255.255:1"})))
            .contains("`broadcast_address`")
    );
    let m = refused(&with(
        &json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:1", "destinations": []}),
    ));
    assert!(m.contains("`destinations`"), "{m}");
    let m = refused(&with(
        &json!({"mode": "multicast", "group": "239.1.1.1", "destinations": []}),
    ));
    assert!(m.contains("`destinations`"), "{m}");
    let m = refused(&with(
        &json!({"mode": "multicast", "group": "239.1.1.1", "broadcast_address": "127.255.255.255:1"}),
    ));
    assert!(m.contains("`broadcast_address`"), "{m}");
    let m = refused(&with(
        &json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:1", "ttl": 3}),
    ));
    assert!(m.contains("`ttl`"), "{m}");
}

#[test]
fn broadcast_and_multicast_need_their_address() {
    assert!(refused(&with(&json!({"mode": "broadcast"}))).contains("`broadcast_address`"));
    assert!(refused(&with(&json!({"mode": "multicast"}))).contains("`group`"));
    assert!(
        refused(&with(
            &json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:0"})
        ))
        .contains("port 0")
    );
    let m = refused(&with(&json!({"mode": "multicast", "group": "192.168.1.1"})));
    assert!(
        m.contains("`group`") && m.contains("not a multicast"),
        "{m}"
    );
    assert!(refused(&with(&json!({"mode": "multicast", "group": "ff02::1"}))).contains("`group`"));
    assert!(
        refused(&with(
            &json!({"mode": "multicast", "group": "239.1.1.1", "interface": "eth0"})
        ))
        .contains("`interface`")
    );
    assert!(
        refused(&with(
            &json!({"mode": "multicast", "group": "239.1.1.1", "ttl": 256})
        ))
        .contains("`ttl`")
    );
    assert!(
        parse(&with(
            &json!({"mode": "multicast", "group": "239.1.1.1", "ttl": 0})
        ))
        .is_ok()
    );
}

#[test]
fn force_names_and_ids_are_checked() {
    assert!(refused(&with(&json!({"forces": {"Blue": 1}}))).contains("forces.Blue"));
    assert!(refused(&with(&json!({"forces": {"a.b": 1}}))).contains("forces.a.b"));
    assert!(refused(&with(&json!({"forces": {"blue": 0}}))).contains("forces.blue"));
    assert!(refused(&with(&json!({"forces": {"blue": 256}}))).contains("forces.blue"));
    let m = refused(&with(&json!({"forces": {"blue": 1, "navy": 1}})));
    assert!(m.contains("force ID 1 is already used"), "{m}");
    assert!(refused(&with(&json!({"forces": []}))).contains("`forces`"));
    assert!(parse(&with(&json!({"forces": {"blue": 1, "red_2": 255}}))).is_ok());
}

#[test]
fn weapon_ids_and_munition_tokens_are_checked() {
    assert!(
        refused(&with(&json!({"weapons": {"cannon": "2.1.225.1.1.0.0"}})))
            .contains("weapons.cannon")
    );
    assert!(
        refused(&with(
            &json!({"weapons": {"Acme.cannon": "2.1.225.1.1.0.0"}})
        ))
        .contains("weapons.Acme.cannon")
    );
    let m = refused(&with(&json!({"weapons": {"acme.cannon": "2.1.225"}})));
    assert!(
        m.contains("weapons.acme.cannon") && m.contains("seven"),
        "{m}"
    );
    assert!(
        refused(&with(&json!({"weapons": {"acme.cannon": 5}}))).contains("weapons.acme.cannon")
    );
    assert!(refused(&with(&json!({"weapons": ["x"]}))).contains("`weapons`"));
    assert!(parse(&with(&json!({"weapons": {"a.b.c": "1.2.3.4.5.6.7"}}))).is_ok());
}

fn templates(tables: &[(&str, Value)]) -> BTreeMap<String, Value> {
    tables
        .iter()
        .map(|(id, t)| ((*id).to_owned(), t.clone()))
        .collect()
}

#[test]
fn a_bad_entity_type_token_in_a_template_names_the_template() {
    let t = templates(&[
        ("blue.tank", json!({"entity_type": "1.1.225.1.1.0.0"})),
        ("red.truck", json!({"entity_type": "1.1.225"})),
    ]);
    let e = Config::parse("dis", &base(), &t).expect_err("the token is bad");
    assert_eq!(e.code(), umbra_sdk::abi::UMBRA_ERR_PARAMS);
    let m = e.to_string();
    assert!(
        m.contains("red.truck") && m.contains("[network.dis]") && m.contains("1.1.225"),
        "{m}"
    );
    assert!(
        !m.contains("blue.tank"),
        "the good template is not blamed: {m}"
    );
    // The adapter's own name selects the table name in the message.
    let e = Config::parse("dis2", &base(), &t).expect_err("still bad");
    assert!(
        e.to_string().contains("[network.dis2]"),
        "{}",
        e.to_string()
    );
}

#[test]
fn template_tables_hold_a_string_entity_type_and_nothing_else() {
    let ok = templates(&[
        ("a.b", json!({"entity_type": "01.1.225.1.1.0.0"})),
        ("c.d", json!({})),
    ]);
    assert!(
        Config::parse("dis", &base(), &ok).is_ok(),
        "a table without entity_type is fine"
    );
    for (table, needle) in [
        (json!({"entity_type": 5}), "must be a string"),
        (json!({"entity_type": "1.1.x.1.1.0.0"}), "not a decimal"),
        (
            json!({"entity_typo": "1.1.225.1.1.0.0"}),
            "unknown key `entity_typo`",
        ),
        (json!("1.1.225.1.1.0.0"), "must be a table"),
    ] {
        let t = templates(&[("a.b", table)]);
        let m = Config::parse("dis", &base(), &t)
            .expect_err(needle)
            .to_string();
        assert!(m.contains(needle) && m.contains("a.b"), "{needle}: {m}");
    }
}

// ------------------------------------------------------------------------------- warning

#[test]
fn the_security_warning_appears_exactly_for_a_non_loopback_bind_broadcast_or_multicast() {
    let broadcast = json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:3000"});
    let multicast = json!({"mode": "multicast", "group": "239.255.1.2"});
    let rows: [(&str, Value, bool); 8] = [
        ("default", json!({}), false),
        ("loopback", json!({"bind": "127.0.0.1:3000"}), false),
        (
            "another loopback address",
            json!({"bind": "127.1.2.3:3000"}),
            false,
        ),
        (
            "loopback with destinations",
            json!({"destinations": ["192.168.1.9:3000"]}),
            false,
        ),
        ("any address", json!({"bind": "0.0.0.0:3000"}), true),
        ("a LAN address", json!({"bind": "192.168.1.5:3000"}), true),
        ("broadcast", broadcast.clone(), true),
        ("multicast", multicast.clone(), true),
    ];
    for (what, extra, warns) in rows {
        let c = parse(&with(&extra)).expect(what);
        let warning = c.security_warning();
        assert_eq!(warning.is_some(), warns, "{what}: {warning:?}");
        if let Some(w) = warning {
            assert!(
                w.contains("accepts unauthenticated DIS traffic from the network"),
                "{w}"
            );
            assert!(w.contains("`dis`"), "names the adapter: {w}");
        }
    }
}

#[test]
fn broadcast_and_multicast_refuse_a_loopback_bind() {
    let broadcast = json!({"mode": "broadcast", "broadcast_address": "127.255.255.255:3000"});
    let multicast = json!({"mode": "multicast", "group": "239.255.1.2"});
    for (what, extra) in [("broadcast", broadcast), ("multicast", multicast)] {
        for bind in ["127.0.0.1:3000", "127.9.9.9:4000"] {
            let m = refused(&with(&with_bind(&extra, bind)));
            assert!(
                m.contains("`dis`")
                    && m.contains("`bind`")
                    && m.contains(bind)
                    && m.contains("loopback"),
                "{what} {bind}: {m}"
            );
        }
        let ok = with_bind(&extra, "0.0.0.0:3000");
        assert!(parse(&with(&ok)).is_ok(), "{what} on any address");
    }
    // Unicast on loopback is the default.
    assert!(parse(&with(&json!({"bind": "127.0.0.1:3000"}))).is_ok());
}

#[test]
fn a_configure_input_built_in_code_gives_the_same_config() {
    let params = with(&json!({"version": 6, "bind": "127.0.0.1:4100"}));
    let tables = templates(&[("a.b", json!({"entity_type": "1.1.225.1.1.0.0"}))]);
    let input = umbra_sdk::ConfigureInput::from_parts("dis", params.clone(), tables.clone());
    assert_eq!(input.name(), "dis");
    let from_input = Config::from_input(&input).expect("valid");
    assert_eq!(
        from_input,
        Config::parse("dis", &params, &tables).expect("valid")
    );
    assert_eq!(from_input.version, DisVersion::V6);
}

#[test]
fn the_template_keys_the_engine_reads_are_accepted() {
    for key in umbra_sdk::MANAGER_TEMPLATE_KEYS {
        let t = templates(&[("a.b", json!({ *key: "1.1.225.1.1.0.0" }))]);
        assert!(Config::parse("dis", &base(), &t).is_ok(), "{key}");
    }
}

fn with_bind(v: &Value, bind: &str) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.insert("bind".to_owned(), json!(bind));
    }
    v
}

#[test]
fn the_warning_names_the_reason() {
    let c = parse(&with(&json!({"bind": "0.0.0.0:3000"}))).expect("valid");
    assert!(
        c.security_warning()
            .expect("warns")
            .contains("not loopback")
    );
    let c = parse(&with(&json!({"mode": "multicast", "group": "239.1.1.1"}))).expect("valid");
    assert!(c.security_warning().expect("warns").contains("multicast"));
}

#[test]
fn the_param_schema_is_json_with_the_documented_keys() {
    let schema: Value = umbra_sdk::serde_json::from_str(PARAM_SCHEMA).expect("the schema is JSON");
    let props = schema["properties"].as_object().expect("properties");
    let mut keys: Vec<&str> = props.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "application",
            "bind",
            "broadcast_address",
            "destinations",
            "exercise",
            "forces",
            "group",
            "interface",
            "mode",
            "queue",
            "site",
            "ttl",
            "version",
            "weapons"
        ]
    );
    assert_eq!(schema["required"], json!(["site", "application"]));
    assert_eq!(schema["additionalProperties"], json!(false));
}
