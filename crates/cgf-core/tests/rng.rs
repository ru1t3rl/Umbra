//! AT-5: `SimRng`, derived streams and the pinned generators. Requirements: REQ-K-4, ADR 0006.
//!
//! Reference values come from running the public-domain reference C implementations
//! (`splitmix64.c` and `xoshiro256starstar.c`, Sebastiano Vigna and David Blackman,
//! prng.di.unimi.it), compiled with gcc on 2026-10-17, plus the ADR 0006 derivation written in the
//! same C program. The `SplitMix64` vector was additionally confirmed with Java's
//! `SplittableRandom(1234567L).nextLong()`, an independent implementation of the same algorithm.

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use core::num::NonZeroU64;

use cgf_core::{EntityId, RngStream, SimRng, SplitMix64, StreamKey};
use proptest::prelude::*;

fn entity(id: u64) -> EntityId {
    EntityId::new(id).expect("non-zero id")
}

/// The first `n` outputs of a stream.
fn take(s: &mut RngStream, n: usize) -> Vec<u64> {
    (0..n).map(|_| s.next_u64()).collect()
}

/// ADR 0006: `SplitMix64` from seed 1234567 reproduces the reference outputs.
///
/// Verified: reference C (`splitmix64.c`) and Java `SplittableRandom(1234567L)` print the same
/// five values, matching the plan.
#[test]
fn splitmix64_reference_vector() {
    let mut sm = SplitMix64::new(1_234_567);

    let got: Vec<u64> = (0..5).map(|_| sm.next_u64()).collect();

    assert_eq!(
        got,
        [
            6_457_827_717_110_365_317,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ]
    );
}

/// ADR 0006: Xoshiro256** from state `[1, 2, 3, 4]` reproduces the reference outputs.
///
/// Verified with the reference C (`xoshiro256starstar.c`, compiled with gcc) on state
/// `{1, 2, 3, 4}`: 11520, 0, 1509978240, 1215971899390074240. The first value also follows by
/// hand: `rotl(2 * 5, 7) * 9 = 1280 * 9 = 11520`. The plan's values were correct.
///
/// Needs `RngStream::from_state`, which the plan does not list (see the M1 running log).
#[test]
fn xoshiro256starstar_reference_vector() {
    let mut s = RngStream::from_state([1, 2, 3, 4]).expect("non-zero state");

    assert_eq!(
        take(&mut s, 4),
        [11_520, 0, 1_509_978_240, 1_215_971_899_390_074_240]
    );
}

/// ADR 0006: the all-zero state is a fixed point of Xoshiro256** and must be rejected.
#[test]
fn from_state_rejects_all_zero() {
    assert_eq!(RngStream::from_state([0; 4]), None);
}

/// ADR 0006: `state()` returns what `from_state` was given.
#[test]
fn from_state_then_state_round_trips() {
    let s = RngStream::from_state([9, 8, 7, 6]).expect("non-zero state");

    assert_eq!(s.state(), [9, 8, 7, 6]);
}

/// ADR 0006: the generator also matches the reference on a derived stream.
///
/// Stream (seed 42, ENTITY, id 1): reference C derived state
/// `[1474049808699972579, 13410151454407229751, 17430489139590219639, 4915803149847433915]`
/// and the next four outputs below. The `[1, 2, 3, 4]` vector (11520, 0, 1509978240,
/// 1215971899390074240) is covered by the same code path: only the starting state differs.
#[test]
fn xoshiro256starstar_reference_vector_from_derived_state() {
    let mut s = SimRng::new(42).stream(StreamKey::entity(entity(1)));
    assert_eq!(
        s.state(),
        [
            1_474_049_808_699_972_579,
            13_410_151_454_407_229_751,
            17_430_489_139_590_219_639,
            4_915_803_149_847_433_915,
        ]
    );

    assert_eq!(
        take(&mut s, 4),
        [
            5_954_940_763_750_750_297,
            13_249_745_710_940_346_206,
            3_232_118_136_454_135_603,
            12_688_203_589_355_131_329,
        ]
    );
}

/// ADR 0006: derivation `x = mix64(mix64(seed ^ mix64(domain)) ^ id)`, state = first four
/// `SplitMix64` outputs from `x`, then Xoshiro256** (golden for seed 42, entity 1).
///
/// Integer-only, so identical on every OS. Reference C program printed these values.
#[test]
fn golden_entity_stream_seed_42_id_1() {
    let mut s = SimRng::new(42).stream(StreamKey::entity(entity(1)));

    assert_eq!(
        take(&mut s, 4),
        [
            5_954_940_763_750_750_297,
            13_249_745_710_940_346_206,
            3_232_118_136_454_135_603,
            12_688_203_589_355_131_329,
        ]
    );
}

/// ADR 0006: goldens for neighbouring keys (entity 2, seed 43, scenario domain, seed 0).
#[test]
fn golden_streams_for_neighbouring_keys() {
    let mut e2 = SimRng::new(42).stream(StreamKey::entity(entity(2)));
    let mut seed43 = SimRng::new(43).stream(StreamKey::entity(entity(1)));
    let mut scenario1 = SimRng::new(42).stream(StreamKey::scenario(1));
    let mut scenario0 = SimRng::new(42).stream(StreamKey::scenario(0));
    let mut seed0 = SimRng::new(0).stream(StreamKey::entity(entity(1)));

    assert_eq!(
        take(&mut e2, 4),
        [
            6_119_543_312_789_633_137,
            1_247_433_782_006_424_410,
            4_937_970_546_071_242_301,
            9_142_628_240_739_485_298,
        ]
    );
    assert_eq!(
        take(&mut seed43, 4),
        [
            5_193_486_813_553_809_179,
            7_952_967_364_005_444_400,
            16_035_842_218_566_306_608,
            11_813_375_548_959_362_999,
        ]
    );
    assert_eq!(
        take(&mut scenario1, 4),
        [
            7_021_701_097_543_080_583,
            1_238_776_937_392_015_995,
            5_809_946_832_906_245_176,
            1_863_440_282_667_841_211,
        ]
    );
    assert_eq!(
        take(&mut scenario0, 4),
        [
            577_443_369_841_590_850,
            13_685_529_448_546_696_826,
            5_985_365_977_386_063_450,
            7_393_018_529_557_275_929,
        ]
    );
    assert_eq!(
        take(&mut seed0, 4),
        [
            8_799_085_128_811_779_821,
            1_414_500_788_609_713_358,
            17_149_476_524_947_920_709,
            5_410_049_552_738_598_609,
        ]
    );
}

/// ADR 0006: the domain constants are part of the derivation contract (never renumbered).
#[test]
fn stream_key_domains_and_constructors() {
    assert_eq!(StreamKey::ENTITY, 1);
    assert_eq!(StreamKey::SCENARIO, 2);
    assert_eq!(StreamKey::entity(entity(5)), StreamKey { domain: 1, id: 5 });
    assert_eq!(StreamKey::scenario(7), StreamKey { domain: 2, id: 7 });
}

/// ADR 0006: `SimRng` stores only the master seed.
#[test]
fn sim_rng_reports_its_seed() {
    assert_eq!(SimRng::new(42).seed(), 42);
}

/// ADR 0006: `next_f64` uses the top 53 bits: first draw of the golden stream is
/// `(5954940763750750297 >> 11) / 2^53 = 2907685919800171 / 2^53` (reference C printed the
/// integer `2907685919800171`).
#[test]
fn next_f64_uses_top_53_bits() {
    let mut s = SimRng::new(42).stream(StreamKey::entity(entity(1)));

    let got = s.next_f64();

    assert_eq!(got, 2_907_685_919_800_171.0 / 9_007_199_254_740_992.0);
}

/// ADR 0006: `gen_range_u64(n)` is Lemire's multiply-shift with rejection; first six draws below
/// 10 on the golden stream, computed by the reference C program with 128-bit arithmetic.
#[test]
fn gen_range_matches_lemire_reference() {
    let mut s = SimRng::new(42).stream(StreamKey::entity(entity(1)));
    let n = NonZeroU64::new(10).expect("non-zero");

    let got: Vec<u64> = (0..6).map(|_| s.gen_range_u64(n)).collect();

    assert_eq!(got, [3, 7, 1, 6, 9, 2]);
}

/// REQ-K-4: the same seed and key give the same sequence, every time.
#[test]
fn same_seed_and_key_give_same_sequence() {
    let key = StreamKey::entity(entity(3));
    let mut a = SimRng::new(7).stream(key);
    let mut b = SimRng::new(7).stream(key);

    assert_eq!(take(&mut a, 64), take(&mut b, 64));
}

/// ADR 0006: a different id, domain or seed gives a different sequence.
#[test]
fn different_ids_or_domains_or_seeds_give_different_sequences() {
    let base = take(
        &mut SimRng::new(7).stream(StreamKey { domain: 1, id: 1 }),
        8,
    );

    let other_id = take(
        &mut SimRng::new(7).stream(StreamKey { domain: 1, id: 2 }),
        8,
    );
    let other_domain = take(
        &mut SimRng::new(7).stream(StreamKey { domain: 2, id: 1 }),
        8,
    );
    let other_seed = take(
        &mut SimRng::new(8).stream(StreamKey { domain: 1, id: 1 }),
        8,
    );

    assert_ne!(base, other_id);
    assert_ne!(base, other_domain);
    assert_ne!(base, other_seed);
}

/// ADR 0006: drawing from one stream never changes another (spawn order independence).
#[test]
fn drawing_from_one_stream_does_not_affect_another() {
    let rng = SimRng::new(11);
    let mut first = rng.stream(StreamKey::entity(entity(1)));
    let mut second = rng.stream(StreamKey::entity(entity(2)));
    let untouched = rng.stream(StreamKey::entity(entity(2)));

    let _ = take(&mut first, 1000);

    assert_eq!(second.state(), untouched.state());
    assert_eq!(
        second.next_u64(),
        rng.stream(StreamKey::entity(entity(2))).next_u64()
    );
}

/// ADR 0006: the state of a derived stream is never all zero (Xoshiro256** needs that).
#[test]
fn derived_state_is_never_all_zero() {
    for seed in [0_u64, 1, u64::MAX] {
        for id in 1..=4 {
            let s = SimRng::new(seed).stream(StreamKey::entity(entity(id)));
            assert_ne!(s.state(), [0; 4], "seed {seed} id {id}");
        }
    }
}

proptest! {
    /// ADR 0006: `next_f64` is in `[0, 1)`.
    #[test]
    fn next_f64_in_unit_interval(seed in any::<u64>(), id in 1_u64..u64::MAX) {
        let mut s = SimRng::new(seed).stream(StreamKey { domain: 1, id });
        for _ in 0..64 {
            let x = s.next_f64();
            prop_assert!((0.0..1.0).contains(&x), "{x}");
        }
    }

    /// ADR 0006: `gen_range_u64(n)` is in `[0, n)`.
    #[test]
    fn gen_range_below_bound(seed in any::<u64>(), n in 1_u64..u64::MAX) {
        let mut s = SimRng::new(seed).stream(StreamKey { domain: 2, id: 0 });
        let bound = NonZeroU64::new(n).expect("non-zero");
        for _ in 0..64 {
            prop_assert!(s.gen_range_u64(bound) < n);
        }
    }

    /// ADR 0006: `uniform_f64(lo, hi)` stays within `[lo, hi)`.
    #[test]
    fn uniform_f64_within_bounds(seed in any::<u64>(), lo in -1.0e6_f64..1.0e6, width in 1.0e-3_f64..1.0e6) {
        let hi = lo + width;
        let mut s = SimRng::new(seed).stream(StreamKey { domain: 2, id: 1 });
        for _ in 0..64 {
            let x = s.uniform_f64(lo, hi);
            prop_assert!(x >= lo && x < hi, "{x} not in [{lo}, {hi})");
        }
    }
}
