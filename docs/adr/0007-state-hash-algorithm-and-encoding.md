# ADR 0007: State hash algorithm and canonical encoding

- **Status**: Accepted (approved by the human project owner on 2026-10-17)
- **Requirement IDs**: REQ-K-4, A-2, section 15 (determinism tests), REQ-K-7 (M7 "replay reproduces state hashes"), ADR 0003
- **Affected**: `cgf-core` (`hash` module), `cgf-kernel` (what is hashed), `cgf-engine` determinism test, `cgf-cli` output

## Context
The determinism gate (section 15, gate 4) compares per-tick state hashes between runs and clock modes;
M7 replay must reproduce them. So the hash must be identical for identical state on the same OS, across
processes and builds, and stable across Rust and dependency versions. `std`'s `DefaultHasher` is
explicitly not stable across releases and `RandomState` is seeded per process (banned, ASM-013). Floats
need a canonical form: `-0.0 == 0.0` and NaN payloads may differ without any real divergence. The hash
detects accidental divergence; it is not a security boundary.

## Options
1. **Hand-rolled FNV-1a 64 over a canonical, versioned byte encoding.**
   - Pros: about 15 lines, published test vectors, no dependency, no `unsafe`, never changes; the
     encoding (not the hash) carries the meaning, so it is easy to review.
   - Cons: byte-at-a-time is slower than modern hashes (about 1.5 MB of state at 10k entities; hashing
     runs in tests and when asked, not every tick in production). 64 bits: accidental collision odds
     about 2^-64 per comparison, irrelevant here.
2. **xxh3 (`twox-hash`, MIT).** Fast, good quality. Cons: a dependency whose output stability we would
   rely on across versions; more than we need.
3. **BLAKE3 (`blake3`).** Cryptographic, fast with SIMD. Cons: C/assembly build, extra licences
   (CC0/Apache-2.0 with LLVM exception), far beyond the need.

## Decision
Option 1 (KISS; stability by construction).
- `cgf-core::hash::StateHasher` wraps FNV-1a 64 (offset basis `0xcbf29ce484222325`, prime
  `0x100000001b3`) and exposes only canonical writers: `write_u8/u32/u64`, `write_f64`, `write_bytes`
  (length-prefixed with a `u64`), `write_str`. Integers are little-endian, fixed width.
- `write_f64` canonicalises: `-0.0` becomes `+0.0`; every NaN becomes `0x7FF8_0000_0000_0000`; then
  `to_bits()` little-endian.
- The stream starts with `STATE_HASH_VERSION: u32 = 1`. Any change to what is hashed bumps it.
- Content (kernel, version 1): tick, tick rate (Hz), entity count; then for each entity in ascending
  `EntityId` order: id, ownership tag (and remote source string), motion model id string, kinematic
  state (position x, y, z; velocity x, y, z; orientation w, x, y, z), route state (waypoint count,
  next index; absent = tag 0), entity RNG state (four `u64`). Transient per-tick buffers (intents) and
  wall-clock data (timings) are never hashed.
- `StateHash(u64)` displays as 16 lowercase hex digits.

## Consequences
- Easier: hashes are comparable across processes and builds on one OS; a platform-independent golden
  test pins the encoding (integers and fixed float constants only).
- Harder: new components must be added to the encoding deliberately (with a version bump), or the gate
  will not see divergence in them. The code reviewer checks this for every new component.
- Canonicalising NaN and signed zero can hide a divergence that only flips a sign of zero; accepted,
  because the next arithmetic difference shows up in later ticks anyway.
- Revisit if hashing becomes a measured hot spot (switch to option 2 with a version bump) or if
  cross-OS hash comparison is required (ADR 0003 option 2).
