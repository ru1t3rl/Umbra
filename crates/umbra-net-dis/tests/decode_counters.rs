//! `DecodeCounters` (M6 WI-14): unknown PDU types are skipped and counted, per instance (ASM-410).
#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod support;

use support::load;
use umbra_net_dis::codec::DecodeCounters;
use umbra_net_dis::version::DisVersion;

fn with_type(mut bytes: Vec<u8>, pdu_type: u8) -> Vec<u8> {
    bytes[2] = pdu_type;
    bytes
}

#[test]
fn unknown_types_are_skipped_and_counted_by_type() {
    let es = load(DisVersion::V7, "es_basic");
    let mut c = DecodeCounters::default();
    assert!(c.decode(&es).is_some());
    for t in [4u8, 4, 27, 200] {
        assert!(c.decode(&with_type(es.clone(), t)).is_none());
    }
    assert_eq!(c.decoded, 1);
    assert_eq!(c.skipped_unsupported_type(), 4);
    assert_eq!(c.skipped_by_type.get(&4), Some(&2));
    assert_eq!(c.skipped_by_type.get(&27), Some(&1));
    assert_eq!(c.skipped_by_type.get(&200), Some(&1));
    assert_eq!((c.unsupported_version, c.malformed), (0, 0));
}

#[test]
fn versions_and_malformed_datagrams_are_counted_apart() {
    let es = load(DisVersion::V6, "es_basic");
    let mut c = DecodeCounters::default();
    let mut v5 = es.clone();
    v5[0] = 5;
    assert!(c.decode(&v5).is_none());
    assert!(c.decode(&es[..50]).is_none());
    assert!(c.decode(&[]).is_none());
    let mut wrong_family = es.clone();
    wrong_family[3] = 2;
    assert!(c.decode(&wrong_family).is_none());
    assert_eq!(c.unsupported_version, 1);
    assert_eq!(c.malformed, 3);
    assert_eq!((c.decoded, c.skipped_unsupported_type()), (0, 0));
}

#[test]
fn every_supported_family_counts_as_decoded() {
    let mut c = DecodeCounters::default();
    for name in [
        "es_basic",
        "fire_basic",
        "det_basic",
        "start_resume_basic",
        "stop_freeze_basic",
    ] {
        for version in [DisVersion::V6, DisVersion::V7] {
            assert!(c.decode(&load(version, name)).is_some(), "{name}");
        }
    }
    assert_eq!(c.decoded, 10);
}

#[test]
fn instances_do_not_share_counters() {
    let unknown = with_type(load(DisVersion::V7, "es_basic"), 99);
    let (mut a, mut b) = (DecodeCounters::default(), DecodeCounters::default());
    a.decode(&unknown);
    a.decode(&unknown);
    b.decode(&unknown);
    assert_eq!(
        (a.skipped_unsupported_type(), b.skipped_unsupported_type()),
        (2, 1)
    );
    assert_eq!(a.skipped_by_type.get(&99), Some(&2));
    assert_eq!(b.skipped_by_type.get(&99), Some(&1));
}
