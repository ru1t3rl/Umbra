//! The only randomness source: `SimRng`, derived streams, `Xoshiro256**` and `SplitMix64`
//! (ADR 0006, REQ-K-4).
//!
//! Both generators are hand-written and pinned by reference vectors, so the output for a given
//! seed never changes with a dependency bump (recordings and golden hashes depend on it). They are
//! integer-only, hence bit-identical on every platform. Nothing here reads OS entropy or the clock.
//!
//! [`SimRng`] holds only the master seed. [`SimRng::stream`] derives an independent
//! [`RngStream`] per [`StreamKey`] (for example one per entity), so spawning another entity never
//! changes the draws of an existing one.

use core::num::NonZeroU64;

use crate::id::EntityId;

const SPLITMIX_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// The `SplitMix64` output finaliser, a bijection on `u64`.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The 128-bit product of two `u64` as `(high, low)` halves.
#[expect(
    clippy::cast_possible_truncation,
    reason = "splitting a 128-bit product into its two 64-bit halves"
)]
fn widening_mul(a: u64, b: u64) -> (u64, u64) {
    let product = u128::from(a) * u128::from(b);
    ((product >> 64) as u64, product as u64)
}

/// The master seed of a run. Cheap to copy; derive streams from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimRng {
    seed: u64,
}

impl SimRng {
    /// A master RNG for `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// The master seed.
    #[must_use]
    pub const fn seed(self) -> u64 {
        self.seed
    }

    /// The independent stream for `key`.
    ///
    /// Derivation (ADR 0006): `x = mix64(mix64(seed ^ mix64(domain)) ^ id)`; the four state words
    /// are the first four `SplitMix64` outputs seeded with `x`.
    #[must_use]
    pub fn stream(self, key: StreamKey) -> RngStream {
        let x = mix64(mix64(self.seed ^ mix64(key.domain)) ^ key.id);
        let mut sm = SplitMix64::new(x);
        let s = [sm.next_u64(), sm.next_u64(), sm.next_u64(), sm.next_u64()];
        // SplitMix64 is a bijection of a counter, so four consecutive outputs are not all zero;
        // the fallback only keeps the type total.
        RngStream {
            s: if s == [0; 4] { [1, 0, 0, 0] } else { s },
        }
    }
}

/// Names one derived stream: a domain (what kind of consumer) and an id within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StreamKey {
    /// Consumer kind; see [`StreamKey::ENTITY`] and [`StreamKey::SCENARIO`].
    pub domain: u64,
    /// Identifier within the domain.
    pub id: u64,
}

impl StreamKey {
    /// Domain of per-entity streams (id = [`EntityId::get`]). Never renumbered.
    pub const ENTITY: u64 = 1;
    /// Domain of scenario-level streams (id = placement index). Never renumbered.
    pub const SCENARIO: u64 = 2;

    /// The stream of entity `id`.
    #[must_use]
    pub fn entity(id: EntityId) -> Self {
        Self {
            domain: Self::ENTITY,
            id: id.get(),
        }
    }

    /// The scenario stream for placement `index`.
    #[must_use]
    pub fn scenario(index: u64) -> Self {
        Self {
            domain: Self::SCENARIO,
            id: index,
        }
    }
}

/// Xoshiro256** state. Not `Copy`: copying a stream would duplicate draws by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RngStream {
    s: [u64; 4],
}

impl RngStream {
    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// A uniform value in `[0, 1)` from the top 53 bits of the next draw.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a 53-bit integer converts to f64 exactly"
    )]
    pub fn next_f64(&mut self) -> f64 {
        // 53 bits convert to f64 exactly.
        (self.next_u64() >> 11) as f64 / 9_007_199_254_740_992.0
    }

    /// A uniform value in `[0, n)` (Lemire's multiply-shift method with rejection, unbiased).
    pub fn gen_range_u64(&mut self, n: NonZeroU64) -> u64 {
        let n = n.get();
        let (mut high, mut low) = widening_mul(self.next_u64(), n);
        if low < n {
            let threshold = n.wrapping_neg() % n;
            while low < threshold {
                (high, low) = widening_mul(self.next_u64(), n);
            }
        }
        high
    }

    /// A uniform value in `[lo, hi)`; `lo` if the interval is empty, reversed, or its width
    /// `hi - lo` overflows `f64`. A draw is always consumed.
    pub fn uniform_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let u = self.next_f64();
        if !(hi > lo && (hi - lo).is_finite()) {
            return lo;
        }
        // Rounding can land exactly on `hi`; keep the interval half-open.
        (lo + (hi - lo) * u).min(hi.next_down())
    }

    /// The raw state, for hashing and replay.
    #[must_use]
    pub fn state(&self) -> [u64; 4] {
        self.s
    }

    /// Rebuilds a stream from a raw state; `None` for the all-zero state (a fixed point).
    #[must_use]
    pub fn from_state(s: [u64; 4]) -> Option<Self> {
        (s != [0; 4]).then_some(Self { s })
    }
}

/// `SplitMix64`: used only to expand a 64-bit key into a stream state. Public so the reference
/// vector can be tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator starting from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX_GAMMA);
        mix64(self.state)
    }
}
