# ADR 0006: `SimRng` algorithm and stream derivation

- **Status**: Accepted (approved by the human project owner on 2026-10-17)
- **Requirement IDs**: REQ-K-4, A-2, REQ-K-7, REQ-P-5, ADR 0003
- **Affected**: `umbra-core` (`rng` module), `umbra-kernel` (owns the instance and per-entity streams), later the plugin host API (M4) and replay (M7)

## Context
REQ-K-4 requires one seeded RNG service, `SimRng`, as the only randomness source, with a per-entity
derived stream "so spawn order does not alter other entities' streams". Plugins get randomness only via
the host (REQ-P-5), and replay (REQ-K-7) must reproduce runs from the recorded seed. So the exact
output sequence for a given seed is a long-lived contract: if it changes, every recording and every
golden hash changes. The generator must use no OS entropy, be fast enough to draw per entity per tick,
and make streams cheap to create and store (one per entity, part of the hashed state).

## Options
1. **`rand` + `rand_chacha` (or `rand_xoshiro`).**
   - Pros: well-known, audited algorithms, many distributions.
   - Cons: the values a seed produces are only as stable as the crate's value-stability policy; `rand`
     has changed seeding and distribution outputs across major versions, so a dependency bump can
     silently change every run. Extra dependencies; `rand` also exposes `thread_rng`/OS entropy, which
     we would have to ban.
2. **Hand-rolled, pinned algorithms in `umbra-core` (about 80 lines).** Xoshiro256** for each stream,
   SplitMix64 to expand a 64-bit key into the 256-bit stream state (the method the algorithms' authors
   recommend), both checked against their published reference outputs.
   - Pros: output is fixed forever by our own code and tests; no dependency; integer-only, so
     bit-identical on every platform (ADR 0003 does not even apply); the state is four `u64`, cheap to
     hash and to hand across the C ABI later.
   - Cons: we own the code (small, with reference vectors); only the few distributions we write.
3. **Counter-based generator (e.g. Philox) keyed by (seed, entity, tick).**
   - Pros: no state at all, random access by tick.
   - Cons: more code and constants to get right; no current need for random access.

## Decision
Option 2 (KISS, determinism by construction).
- `SimRng::new(seed: u64)` holds only the master seed. `SimRng::stream(key)` returns an `RngStream`.
- `StreamKey { domain: u64, id: u64 }`. Domains are named constants: `ENTITY` (id = `EntityId::get()`)
  and `SCENARIO` (id = placement index). New domains are appended, never renumbered.
- Derivation: `x = mix64(mix64(seed ^ mix64(domain)) ^ id)`, where `mix64` is the SplitMix64 output
  finaliser (a bijection on `u64`). For a fixed seed and domain, distinct ids give distinct `x`.
  The four Xoshiro256** state words are the first four outputs of SplitMix64 seeded with `x` (never all
  zero).
- `RngStream` offers `next_u64`, `next_f64` (top 53 bits, in `[0, 1)`), `gen_range_u64(n)` (Lemire's
  method with rejection) and `uniform_f64(lo, hi)`. More distributions are added when a consumer needs them.
- Each entity gets its stream at spawn (kernel component, part of the state hash), so spawning another
  entity never changes an existing entity's draws.
- Tests pin the SplitMix64 and Xoshiro256** reference vectors and one golden derived stream.

## Consequences
- Easier: replay and golden hashes never shift on a dependency bump; the plugin host can pass a stream
  by value or handle without our types depending on `rand`.
- Harder: changing the algorithm later is a breaking change for recordings (needs a new ADR and a
  version tag in recordings).
- Clippy bans the usual `rand`/`getrandom`/`fastrand` entry points (ASM-042), so no second source appears.
- Revisit if statistical quality is questioned for some model (swap behind the same API with a new
  domain/version) or if random access by tick is needed (option 3).
